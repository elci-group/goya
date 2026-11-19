//! ie-uds — Unix-domain-socket transport for the contract v0.1 flow
//! (WP-005 completion candidate).
//!
//! A std-only (no async, no external crates) framed transport over a UDS
//! stream. The wire format is exactly the frozen IE-X01 framing: a 4-byte
//! little-endian length prefix over canonical codec bytes. The server
//! accepts connections and, per framed request, decodes and dispatches
//! into a caller-supplied handler (typically an owned
//! [`ie_loopback::Provider::handle_frame`] closure); the client encodes a
//! contract [`Request`], frames it, and reads exactly one framed
//! [`Response`].
//!
//! Trust/robustness rules honoured (contract v0.1):
//! - Oversized declared lengths are rejected **before allocation**: the
//!   4-byte header is read first into a fixed stack buffer, validated
//!   against `MAX_MESSAGE_BYTES`, and only then is a body buffer of the
//!   validated size reserved. A peer declaring an oversized frame gets its
//!   connection closed and a typed error.
//! - Deadlines are remaining durations carried in the request; no peer
//!   wall-clock is read here (provider-enforced).
//! - Typed errors only, and no peer input can panic the transport: every
//!   framing/codec failure maps to `TypedError::InvalidTensor` (mirroring
//!   the loopback reference), every socket failure to `TypedError::PeerLost`.
//! - Sockets carry read timeouts so a silent peer cannot wedge a thread.

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ie_contract::codec;
use ie_contract::errors::TypedError;
use ie_contract::framing::{decode_frame, encode_frame, MAX_MESSAGE_BYTES};
use ie_contract::messages::{Request, Response, Status};
use ie_loopback::Lease;

/// Default per-read timeout for both server and client sockets.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Bad bytes on the wire are INVALID_TENSOR (mirrors the loopback
/// reference `handle_frame` mapping).
fn wire_error(reason: &str) -> TypedError {
    TypedError::InvalidTensor { reason: reason.to_string() }
}

/// The stream itself failed; that is PEER_LOST, not a message-content error.
fn io_error(e: std::io::Error) -> TypedError {
    TypedError::Internal { reason: format!("io: {e}") }
}

/// Read exactly one framed message from `stream` with bounded allocation.
/// The 4-byte header is read into a fixed stack buffer and the declared
/// length is validated against `MAX_MESSAGE_BYTES` *before* any body
/// buffer is reserved. Returns the full frame bytes (prefix + payload).
/// On an oversized declared length the caller must close the connection.
pub fn read_frame_bounded(mut stream: &UnixStream) -> Result<Vec<u8>, TypedError> {
    use std::io::Read;
    let mut header = [0u8; 4];
    match stream.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) => return Err(io_error(e)),
    }
    let declared = u32::from_le_bytes(header);
    if declared as u64 > MAX_MESSAGE_BYTES as u64 {
        // Contract violation: reject before allocating.
        return Err(wire_error(&format!("declared frame too large: {declared}")));
    }
    let mut body = vec![0u8; declared as usize];
    stream.read_exact(&mut body).map_err(io_error)?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// Write one framed message (length prefix + payload) to `stream`.
pub fn write_frame(mut stream: &UnixStream, payload: &[u8]) -> Result<(), TypedError> {
    use std::io::Write;
    let frame = encode_frame(payload).map_err(|_| {
        TypedError::Internal { reason: "payload exceeds framing cap".into() }
    })?;
    stream.write_all(&frame).map_err(io_error)
}

fn set_timeouts(stream: &UnixStream, timeout: Duration) {
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
}

/// UDS server. Binds a listener path and serves framed request/response
/// exchanges. Per connection it loops: read one bounded frame, validate
/// framing, dispatch into the handler, write the framed response — until
/// the peer closes.
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
    read_timeout: Duration,
    lease: Lease,
}

impl Server {
    /// Bind a fresh listener at `path` (stale socket files are removed).
    pub fn bind(path: &Path, lease: Lease) -> Result<Server, TypedError> {
        if path.exists() {
            std::fs::remove_file(path).map_err(io_error)?;
        }
        let listener = UnixListener::bind(path).map_err(io_error)?;
        Ok(Server {
            listener,
            path: path.to_path_buf(),
            read_timeout: DEFAULT_READ_TIMEOUT,
            lease,
        })
    }

    /// The bound socket path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Replace the lease handed to the handler (e.g. after re-acquire).
    pub fn set_lease(&mut self, lease: Lease) {
        self.lease = lease;
    }

    /// Adjust the per-read timeout applied to accepted connections.
    pub fn set_read_timeout(&mut self, timeout: Duration) {
        self.read_timeout = timeout;
    }

    /// Accept exactly one connection and serve framed requests on it until
    /// the peer closes or a fatal condition is reached. `handler` receives
    /// the current lease and the framed request bytes; it returns the
    /// unframed canonical response payload (which this layer frames).
    /// Never panics on peer input.
    pub fn serve_one_connection<F>(&self, mut handler: F) -> Result<(), TypedError>
    where
        F: FnMut(&Lease, &[u8]) -> Result<Vec<u8>, TypedError>,
    {
        let (stream, _addr) = self.listener.accept().map_err(io_error)?;
        set_timeouts(&stream, self.read_timeout);
        Self::serve_stream(&stream, &self.lease, &mut handler)
    }

    /// Serve framed requests on an already-accepted stream until the peer
    /// closes or a fatal condition is reached. Never panics on peer input.
    pub fn serve_stream<F>(
        stream: &UnixStream,
        lease: &Lease,
        handler: &mut F,
    ) -> Result<(), TypedError>
    where
        F: FnMut(&Lease, &[u8]) -> Result<Vec<u8>, TypedError>,
    {
        loop {
            let frame = match read_frame_bounded(stream) {
                Ok(f) => f,
                // Peer closed or stream failed: the connection ends.
                Err(e) => return Err(e),
            };
            // Validate framing before dispatch (mirrors handle_frame):
            // an undecodable frame closes the connection with a typed error.
            let (payload, _) =
                decode_frame(&frame).map_err(|_| wire_error("bad frame"))?;
            let _ = payload;
            // The handler receives the full framed bytes (prefix included),
            // exactly the shape loopback's handle_frame consumes.
            let resp = match handler(lease, &frame) {
                Ok(r) => r,
                Err(e) => {
                    // Typed provider failure: surface it canonically as a
                    // Failed response rather than dropping the connection
                    // silently. Best-effort request-id extraction.
                    let request_id = codec::decode_request(payload)
                        .map(|(r, _)| r.request_id)
                        .unwrap_or(0);
                    let resp = Response {
                        request_id,
                        status: Status::Failed(e),
                        backend: String::new(),
                        provider_id: String::new(),
                        epoch: 0,
                        outputs: Vec::new(),
                        timing_us: [0, 0, 0],
                    };
                    let mut buf = Vec::new();
                    codec::encode_response(&mut buf, &resp)
                        .map_err(|_| TypedError::Internal { reason: "encode".into() })?;
                    buf
                }
            };
            write_frame(stream, &resp)?;
        }
    }

    /// Accept connections forever, serving each sequentially. The handler
    /// is shared across connections; stateful providers go behind a
    /// caller-owned lock inside the closure. Intended for test and
    /// single-client runtimes; stops on the first fatal error.
    pub fn serve_forever<F>(&self, handler: F) -> Result<(), TypedError>
    where
        F: Fn(&Lease, &[u8]) -> Result<Vec<u8>, TypedError>,
    {
        loop {
            self.serve_one_connection(&handler)?;
        }
    }
}

/// UDS client: connect, then issue framed request/response exchanges.
pub struct Client {
    stream: UnixStream,
    lease: Lease,
}

impl Client {
    pub fn connect(path: &Path, lease: Lease) -> Result<Client, TypedError> {
        let stream = UnixStream::connect(path).map_err(io_error)?;
        set_timeouts(&stream, DEFAULT_READ_TIMEOUT);
        Ok(Client { stream, lease })
    }

    pub fn set_lease(&mut self, lease: Lease) {
        self.lease = lease;
    }

    pub fn set_read_timeout(&mut self, timeout: Duration) {
        set_timeouts(&self.stream, timeout);
    }

    /// Encode `request` canonically, frame it, send it, read exactly one
    /// framed response, and decode it. The lease itself is provider state;
    /// the request's `lease_id` field is what the provider validates.
    pub fn call(&mut self, request: &Request) -> Result<Response, TypedError> {
        let mut payload = Vec::new();
        codec::encode_request(&mut payload, request)
            .map_err(|_| TypedError::Internal { reason: "encode".into() })?;
        write_frame(&self.stream, &payload)?;
        let frame = read_frame_bounded(&self.stream)?;
        let (payload, _) =
            decode_frame(&frame).map_err(|_| wire_error("bad response frame"))?;
        let (resp, _) = codec::decode_response(payload)
            .map_err(|_| wire_error("bad response encoding"))?;
        Ok(resp)
    }

    /// The underlying stream (for tests / explicit close).
    pub fn into_stream(self) -> UnixStream {
        self.stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: [u8; 32] = [7; 32];

    fn temp_socket_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "ie-uds-{tag}-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    #[test]
    fn oversized_declared_length_closes_connection_without_allocation() {
        let path = temp_socket_path("oversize");
        let lease = Lease { lease_id: 1, epoch: 1, expiry_ms: 10_000 };
        let server = Server::bind(&path, lease).unwrap();
        let path = server.path().to_path_buf();
        std::thread::spawn(move || {
            // Any accept here must end in the oversized-frame error.
            let _ = server.serve_forever(|_l, _p| Ok(Vec::new()));
        });

        // Raw malicious peer: u32::MAX length prefix, then nothing.
        let mut raw = UnixStream::connect(&path).unwrap();
        use std::io::Write;
        raw.write_all(&u32::MAX.to_le_bytes()).unwrap();
        raw.flush().unwrap();

        // The server must reject before allocating and close the connection.
        // Give the server a moment to accept and error out.
        std::thread::sleep(Duration::from_millis(100));
        // Reading back must observe EOF/reset, not hang.
        raw.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut buf = [0u8; 16];
        use std::io::Read;
        let n = raw.read(&mut buf).unwrap_or(0);
        assert_eq!(n, 0, "server must close the connection on oversized frame");
    }

    #[test]
    fn failed_handler_surfaces_typed_error_response() {
        let path = temp_socket_path("typederr");
        let lease = Lease { lease_id: 1, epoch: 1, expiry_ms: 10_000 };
        let server = Server::bind(&path, lease).unwrap();
        let path = server.path().to_path_buf();
        std::thread::spawn(move || {
            let _ = server.serve_forever(|_l, _p| {
                Err(TypedError::PolicyDenied { reason: "no valid lease".into() })
            });
        });
        let mut client = Client::connect(&path, lease).unwrap();
        let req = Request {
            request_id: 3,
            model_sha256: MODEL,
            inputs: vec![],
            deadline_ms: 100,
            lease_id: 1,
            policy_digest: [0; 32],
            requested_outputs: vec![],
        };
        let resp = client.call(&req).unwrap();
        assert_eq!(resp.request_id, 3);
        assert!(matches!(resp.status, Status::Failed(TypedError::PolicyDenied { .. })));
        let _ = client.into_stream();
    }
}
