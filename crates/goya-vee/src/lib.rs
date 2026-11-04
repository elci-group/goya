//! Adapter to the ViCo Execution Environment (VEE).
//!
//! VEE serves a REST API on port 9987 (`POST /api/v1/tasks` for submission,
//! plus status/cancel endpoints). We cannot link VEE's crate, so this module
//! is a self-contained network adapter over [`std::net::TcpStream`], std-only.
//!
//! Contract surface:
//! - [`VeeClient`] connects to a VEE host/port and [`VeeClient::submit`]s a
//!   [`SubmitRequest`], returning a [`VeeResponse`] or [`VeeError`].
//! - [`SubmitRequest::to_json`] is a tiny deterministic JSON writer
//!   (string/number fields only, quotes/backslashes escaped; no serde).
//! - Response parsing is a hand-rolled scan for `"id":"..."` and
//!   `"status":"..."`. This is deliberately NOT a full JSON parser: keys
//!   nested in other objects with the same names, escaped quotes inside the
//!   values, or key order changes may mis-extract. `VeeResponse::raw` always
//!   carries the full body so callers can re-parse if needed.
//! - Offline degradation: a refused connection surfaces as
//!   [`VeeError::Connection`]; the adapter never panics. Use
//!   [`VeeClient::available`] to probe before submitting so the application
//!   can continue without VEE.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Default TCP port VEE serves its REST API on.
pub const VEE_DEFAULT_PORT: u16 = 9987;

/// HTTP read/connect timeout applied per I/O operation.
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound untrusted VEE responses so a peer cannot exhaust the caller's memory.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 32 * 1024;

/// Connection parameters for a VEE instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeeClient {
    pub host: String,
    pub port: u16,
}

impl VeeClient {
    /// Build a client for the VEE instance at `host:port`.
    pub fn connect(host: &str, port: u16) -> VeeClient {
        VeeClient {
            host: host.to_string(),
            port,
        }
    }

    /// Probe whether VEE is reachable. Never panics; returns `false` on any
    /// connection failure. Offline deployments use this to degrade gracefully.
    pub fn available(&mut self) -> bool {
        TcpStream::connect((self.host.as_str(), self.port)).is_ok()
    }

    /// Submit a task to VEE (`POST /api/v1/tasks`).
    ///
    /// Opens a fresh TCP connection, writes a minimal HTTP/1.1 request with a
    /// correct `Content-Length`, reads the response up to `Content-Length`
    /// bytes or connection close, and extracts `id`/`status` by scan.
    pub fn submit(&mut self, req: &SubmitRequest) -> Result<VeeResponse, VeeError> {
        let body = req.to_json();
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| VeeError::Connection(e.to_string()))?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|e| VeeError::Connection(e.to_string()))?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|e| VeeError::Connection(e.to_string()))?;

        let http = format!(
            "POST /api/v1/tasks HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            self.host,
            self.port,
            body.len(),
            body
        );
        stream
            .write_all(http.as_bytes())
            .map_err(|e| VeeError::Connection(e.to_string()))?;
        stream
            .flush()
            .map_err(|e| VeeError::Connection(e.to_string()))?;

        let raw = read_response(&mut stream)?;
        let status_code = parse_status_code(&raw).ok_or(VeeError::Protocol)?;
        if !(200..300).contains(&status_code) {
            return Err(VeeError::Protocol);
        }
        let body = extract_body(&raw).ok_or(VeeError::Protocol)?;

        Ok(VeeResponse {
            task_id: scan_string_field(&body, "id"),
            status: scan_string_field(&body, "status"),
            raw: body,
        })
    }
}

/// A task submission payload for VEE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitRequest {
    pub capability: String,
    pub language: String,
    pub source_code: String,
    pub budget_ms: u64,
}

impl SubmitRequest {
    /// Deterministic hand-rolled JSON serialisation (no serde). Field order
    /// is fixed: capability, language, source_code, budget_ms.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(64 + self.source_code.len());
        out.push('{');
        out.push_str("\"capability\":");
        push_json_string(&mut out, &self.capability);
        out.push_str(",\"language\":");
        push_json_string(&mut out, &self.language);
        out.push_str(",\"source_code\":");
        push_json_string(&mut out, &self.source_code);
        out.push_str(",\"budget_ms\":");
        out.push_str(&self.budget_ms.to_string());
        out.push('}');
        out
    }
}

/// Append a JSON string literal, escaping `"`, `\`, and control characters.
fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Result of a successful submit call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeeResponse {
    pub task_id: Option<String>,
    pub status: Option<String>,
    /// Full response body, kept so callers can re-parse if the naive scan
    /// extracted the wrong field (see module docs).
    pub raw: String,
}

/// Errors from the adapter. The adapter never panics; all failures map here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VeeError {
    /// TCP connect/write failed (e.g. VEE not running / connection refused).
    Connection(String),
    /// Read timed out.
    Timeout,
    /// Non-2xx status, unparseable HTTP framing, or malformed body.
    Protocol,
}

/// Read the HTTP response until `Content-Length` bytes of body are received
/// or the connection closes.
fn read_response(stream: &mut TcpStream) -> Result<String, VeeError> {
    let mut raw: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    // Header terminator within the accumulated bytes.
    let mut header_end: Option<usize> = None;
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                return match e.kind() {
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                        Err(VeeError::Timeout)
                    }
                    _ => Err(VeeError::Connection(e.to_string())),
                };
            }
        };
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > MAX_RESPONSE_BYTES {
            return Err(VeeError::Protocol);
        }
        if header_end.is_none() {
            if let Some(pos) = find_subslice(&raw, b"\r\n\r\n") {
                header_end = Some(pos + 4);
                if pos + 4 > MAX_HEADER_BYTES {
                    return Err(VeeError::Protocol);
                }
            } else if raw.len() > MAX_HEADER_BYTES {
                return Err(VeeError::Protocol);
            }
        }
        if let Some(start) = header_end {
            if let Some(len) = parse_content_length(&raw[..start]) {
                if raw.len() - start >= len {
                    break;
                }
            }
        }
    }
    String::from_utf8(raw).map_err(|_| VeeError::Protocol)
}

/// Byte-oriented substring search.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Extract the `Content-Length` value from header bytes (case-insensitive).
fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(headers).ok()?;
    for line in text.split("\r\n") {
        if let Some(rest) = strip_prefix_ignore_case(line, "content-length:") {
            return rest.trim().parse::<usize>().ok();
        }
    }
    None
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Parse `HTTP/1.1 NNN ...` from the raw response.
fn parse_status_code(raw: &str) -> Option<u16> {
    let line = raw.split("\r\n").next()?;
    let mut parts = line.split_whitespace();
    let version = parts.next()?;
    if !version.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse::<u16>().ok()
}

/// Split off the body after the `\r\n\r\n` header terminator.
fn extract_body(raw: &str) -> Option<String> {
    let idx = raw.find("\r\n\r\n")?;
    Some(raw[idx + 4..].to_string())
}

/// Hand-rolled scan for `"key":"value"` anywhere in the JSON body.
///
/// Limitation (documented by design): this is not a JSON parser. It matches
/// the first occurrence of the key, does not understand nesting, and does not
/// unescape the value. VEE's submit response is flat enough for this; the
/// full body is available in `VeeResponse::raw` otherwise.
fn scan_string_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let start = body.find(&needle)? + needle.len();
    let rest = body[start..].trim_start();
    let mut chars = rest.chars();
    if chars.next()? != '"' {
        return None;
    }
    let mut value = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(value),
            '\\' => {
                // Minimal unescape of the common pairs so values round-trip.
                match chars.next()? {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    other => {
                        value.push('\\');
                        value.push(other);
                    }
                }
            }
            c => value.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn test_request(source: &str) -> SubmitRequest {
        SubmitRequest {
            capability: "goya.unit.exec".to_string(),
            language: "gasm".to_string(),
            source_code: source.to_string(),
            budget_ms: 5000,
        }
    }

    /// Spawn a one-shot mock server: accepts one connection, optionally
    /// checks the request via `inspect`, replies with `response`, and drains
    /// remaining request bytes first.
    fn spawn_mock(response: &'static str) -> u16 {
        spawn_mock_with(response, |_| {})
    }

    fn spawn_mock_with<F: FnOnce(String) + Send + 'static>(
        response: &'static str,
        inspect: F,
    ) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local addr").port();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            // Read until end of POST body using Content-Length.
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let header_end = loop {
                let n = stream.read(&mut buf).expect("read request");
                assert!(n > 0, "client closed before sending headers");
                raw.extend_from_slice(&buf[..n]);
                if let Some(pos) = find_subslice(&raw, b"\r\n\r\n") {
                    break pos + 4;
                }
            };
            let headers = String::from_utf8_lossy(&raw[..header_end]).to_string();
            let len: usize = headers
                .lines()
                .find_map(|l| {
                    let lower = l.to_ascii_lowercase();
                    lower
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().expect("content-length"))
                })
                .expect("content-length header");
            while raw.len() - header_end < len {
                let n = stream.read(&mut buf).expect("read body");
                assert!(n > 0, "client closed before sending body");
                raw.extend_from_slice(&buf[..n]);
            }
            inspect(String::from_utf8_lossy(&raw).to_string());
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        port
    }

    const CANNED_OK: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 27\r\n\r\n{\"id\":\"t-1\",\"status\":\"queued\"}";

    #[test]
    fn json_writer_basic_and_field_order() {
        let json = test_request("HALT").to_json();
        assert_eq!(
            json,
            "{\"capability\":\"goya.unit.exec\",\"language\":\"gasm\",\"source_code\":\"HALT\",\"budget_ms\":5000}"
        );
    }

    #[test]
    fn json_writer_escapes_quotes_and_backslashes() {
        let req = test_request("say \"hi\"\\end\nline");
        let json = req.to_json();
        assert!(json.contains("\"source_code\":\"say \\\"hi\\\"\\\\end\\nline\""));
        // Must still contain the literal characters once unescaped by the scan.
        assert_eq!(
            scan_string_field(&json, "source_code"),
            Some("say \"hi\"\\end\nline".to_string())
        );
    }

    #[test]
    fn json_writer_control_chars_are_escaped() {
        let req = test_request("\t\r");
        let json = req.to_json();
        assert!(json.contains("\\t\\r"));
        assert!(!json.contains('\t'));
    }

    #[test]
    fn submit_roundtrip_via_mock() {
        let port = spawn_mock(CANNED_OK);
        let mut client = VeeClient::connect("127.0.0.1", port);
        let resp = client.submit(&test_request("HALT")).expect("submit ok");
        assert_eq!(resp.task_id.as_deref(), Some("t-1"));
        assert_eq!(resp.status.as_deref(), Some("queued"));
        assert_eq!(resp.raw, "{\"id\":\"t-1\",\"status\":\"queued\"}");
    }

    #[test]
    fn request_bytes_have_path_and_content_length() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = captured.clone();
        let port = spawn_mock_with(CANNED_OK, move |req| {
            *sink.lock().expect("lock") = req;
        });
        let mut client = VeeClient::connect("127.0.0.1", port);
        let body = test_request("HALT").to_json();
        client.submit(&test_request("HALT")).expect("submit ok");

        let req = captured.lock().expect("lock").clone();
        assert!(
            req.starts_with("POST /api/v1/tasks HTTP/1.1\r\n"),
            "wrong request line: {req}"
        );
        let expected_cl = format!("Content-Length: {}\r\n", body.len());
        assert!(
            req.contains(&expected_cl),
            "missing/incorrect Content-Length in: {req}"
        );
        assert!(req.contains("Content-Type: application/json"));
        assert!(req.ends_with(&body));
    }

    #[test]
    fn connection_refused_is_error_not_panic() {
        // Bind then immediately drop a listener to get a free, unused port.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        let mut client = VeeClient::connect("127.0.0.1", port);
        match client.submit(&test_request("HALT")) {
            Err(VeeError::Connection(_)) => {}
            other => panic!("expected Connection error, got {other:?}"),
        }
        // available() must report false without panicking.
        assert!(!client.available());
    }

    #[test]
    fn malformed_http_is_protocol_error() {
        let port = spawn_mock("not http at all\r\n\r\n");
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert_eq!(
            client.submit(&test_request("HALT")),
            Err(VeeError::Protocol)
        );
    }

    #[test]
    fn http_error_status_is_protocol_error() {
        let port = spawn_mock("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\n\r\n{}");
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert_eq!(
            client.submit(&test_request("HALT")),
            Err(VeeError::Protocol)
        );
    }

    #[test]
    fn missing_fields_scan_returns_none() {
        let port = spawn_mock("HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"other\":1}");
        let mut client = VeeClient::connect("127.0.0.1", port);
        let resp = client.submit(&test_request("HALT")).expect("submit ok");
        assert_eq!(resp.task_id, None);
        assert_eq!(resp.status, None);
    }

    #[test]
    fn available_true_against_mock() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        // Listener accepted the probe then closed is fine; probe just needs
        // the TCP handshake, so spawn a tiny accept loop.
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                drop(stream);
            }
        });
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert!(client.available());
    }

    #[test]
    fn scan_string_field_handles_escapes_and_nesting_limitation() {
        // Documents the known limitation: first key occurrence wins.
        let body = "{\"id\":\"a\\\"b\",\"nested\":{\"id\":\"c\"},\"status\":\"ok\"}";
        assert_eq!(scan_string_field(body, "id"), Some("a\"b".to_string()));
        assert_eq!(scan_string_field(body, "status"), Some("ok".to_string()));
        assert_eq!(scan_string_field(body, "missing"), None);
    }

    #[test]
    fn response_read_respects_content_length_or_close() {
        // Response without Content-Length: adapter reads to connection close.
        let port = spawn_mock("HTTP/1.1 200 OK\r\n\r\n{\"id\":\"t-2\",\"status\":\"queued\"}");
        let mut client = VeeClient::connect("127.0.0.1", port);
        let resp = client.submit(&test_request("HALT")).expect("submit ok");
        assert_eq!(resp.task_id.as_deref(), Some("t-2"));
    }
}
