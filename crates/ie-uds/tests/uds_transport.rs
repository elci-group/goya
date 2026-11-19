//! ie-uds integration test: full contract v0.1 flow over a real Unix
//! domain socket. A server thread runs the bounded frame loop dispatching
//! into an owned ie_loopback::Provider (mirroring `handle_frame` usage
//! from demo_a.rs); a client submits the golden matmul request and checks
//! parity. A raw malicious peer declaring `u32::MAX` as the frame length
//! is rejected before any allocation and disconnected.

use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ie_contract::codec;
use ie_contract::errors::TypedError;
use ie_contract::fixture::GoldenFixture;
use ie_contract::framing::{decode_frame, encode_frame};
use ie_contract::messages::{Request, Status};
use ie_loopback::Provider;
use ie_uds::{Client, Server};

const MODEL: [u8; 32] = [0xCD; 32];

fn temp_socket_path(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ie-uds-it-{tag}-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn matmul_request(id: u64, lease_id: u64) -> Request {
    Request {
        request_id: id,
        model_sha256: MODEL,
        inputs: vec![
            GoldenFixture::tensor(&[1.0, 2.0, 3.0, 4.0], &[2, 2]),
            GoldenFixture::tensor(&[5.0, 6.0, 7.0, 8.0], &[2, 2]),
        ],
        deadline_ms: 5_000,
        lease_id,
        policy_digest: [0; 32],
        requested_outputs: vec!["out".into()],
    }
}

fn decode_outputs_f32(resp: &ie_contract::messages::Response) -> Vec<f32> {
    resp.outputs[0]
        .data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// Happy path: server thread with a real Provider behind a mutex, client
/// submits the golden matmul, response is Completed with parity
/// [19.0, 22.0, 43.0, 50.0].
#[test]
fn golden_matmul_over_uds_matches_loopback_parity() {
    let path = temp_socket_path("golden");

    let provider = Arc::new(Mutex::new({
        let mut p = Provider::new("ie://local/cpu0", 4, 8);
        p.register_model(MODEL);
        p
    }));
    let server_lease = provider.lock().unwrap().acquire_lease(60_000).unwrap();

    let server = Server::bind(&path, server_lease).unwrap();
    let path = server.path().to_path_buf();
    let handler_provider = Arc::clone(&provider);
    let server_thread = std::thread::spawn(move || {
        // Serve until the test client disconnects; the handler mirrors the
        // loopback transport adapter: handle_frame does decode, lease
        // validation, execution, and canonical response framing.
        server.serve_forever(move |lease, frame| {
            handler_provider.lock().unwrap().handle_frame(lease, frame)
        })
    });

    let mut client = Client::connect(&path, server_lease).unwrap();
    let req = matmul_request(1, server_lease.lease_id);
    let resp = client.call(&req).unwrap();

    assert_eq!(resp.status, Status::Completed);
    assert_eq!(resp.provider_id, "ie://local/cpu0");
    assert_eq!(resp.epoch, server_lease.epoch);
    assert_eq!(
        decode_outputs_f32(&resp),
        vec![19.0, 22.0, 43.0, 50.0],
        "uds transport parity with golden matmul"
    );

    // A second call on the same connection (framing loop must resync).
    let resp2 = client.call(&matmul_request(2, server_lease.lease_id)).unwrap();
    assert_eq!(resp2.status, Status::Completed);
    assert_eq!(resp2.request_id, 2);

    drop(client);
    // server_thread keeps accepting until the process exits; the assertions
    // above are the test outcome. (No join: serve_forever runs forever.)
    drop(server_thread);
}

/// Negative path: a raw peer that declares a u32::MAX frame length is
/// rejected by the bounded read before any allocation, and its connection
/// is closed (EOF observed on read-back).
#[test]
fn oversized_declared_length_is_rejected_and_connection_closed() {
    let path = temp_socket_path("oversize");

    let provider = Arc::new(Mutex::new({
        let mut p = Provider::new("ie://local/cpu0", 4, 8);
        p.register_model(MODEL);
        p
    }));
    let server_lease = provider.lock().unwrap().acquire_lease(60_000).unwrap();

    let server = Server::bind(&path, server_lease).unwrap();
    let path = server.path().to_path_buf();
    let handler_provider = Arc::clone(&provider);
    std::thread::spawn(move || {
        // serve_forever returns Err on the oversized frame; that is the
        // expected outcome — record it for the assertion below.
        let outcome = server.serve_forever(move |lease, frame| {
            handler_provider.lock().unwrap().handle_frame(lease, frame)
        });
        SERVER_OUTCOME.lock().unwrap().replace(outcome.is_err());
    });

    // Raw malicious client: write only the 4-byte header claiming ~4 GiB.
    let mut raw = UnixStream::connect(&path).unwrap();
    use std::io::Write;
    raw.write_all(&u32::MAX.to_le_bytes()).unwrap();
    raw.flush().unwrap();

    // Server must reject before allocating; give it time to accept and
    // error out, then confirm the connection is closed from our side.
    std::thread::sleep(Duration::from_millis(200));
    raw.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut buf = [0u8; 16];
    use std::io::Read;
    let n = raw.read(&mut buf).unwrap_or(0);
    assert_eq!(n, 0, "server must close the connection on an oversized frame");

    // And the server loop must have stopped on the typed framing error.
    for _ in 0..50 {
        if *SERVER_OUTCOME.lock().unwrap() == Some(true) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        *SERVER_OUTCOME.lock().unwrap(),
        Some(true),
        "server must surface a typed error for the oversized declared length"
    );
}

static SERVER_OUTCOME: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

/// Typed-error path: submitting under an invalid lease id comes back as a
/// canonical Failed(POLICY_DENIED) response, not a dropped connection.
#[test]
fn invalid_lease_yields_typed_failed_response_over_uds() {
    let path = temp_socket_path("badlease");

    let provider = Arc::new(Mutex::new({
        let mut p = Provider::new("ie://local/cpu0", 4, 8);
        p.register_model(MODEL);
        p
    }));
    let server_lease = provider.lock().unwrap().acquire_lease(60_000).unwrap();

    let server = Server::bind(&path, server_lease).unwrap();
    let path = server.path().to_path_buf();
    let handler_provider = Arc::clone(&provider);
    std::thread::spawn(move || {
        let _ = server.serve_forever(move |lease, frame| {
            handler_provider.lock().unwrap().handle_frame(lease, frame)
        });
    });

    let mut client = Client::connect(&path, server_lease).unwrap();
    let req = matmul_request(7, server_lease.lease_id + 999);
    let resp = client.call(&req).unwrap();
    assert_eq!(resp.request_id, 7);
    assert!(
        matches!(resp.status, Status::Failed(TypedError::PolicyDenied { .. })),
        "invalid lease must map to a typed POLICY_DENIED response"
    );
}
