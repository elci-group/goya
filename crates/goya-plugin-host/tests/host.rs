use goya_plugin_host::{
    BrowserPluginSession, PluginCapability, PluginHostError, PluginHostService, PluginInstance,
    PluginOperation, PluginRevocationHandle,
};
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct RevokingService(PluginRevocationHandle);
impl PluginHostService for RevokingService {
    fn read_session_summary<'a>(
        &'a self,
        _id: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<serde_json::Value, PluginHostError>>
                + Send
                + 'a,
        >,
    > {
        self.0.revoke();
        Box::pin(async { Ok(json!({"title":"must not escape"})) })
    }
    fn read_conversation<'a>(
        &'a self,
        _id: &'a str,
        _cursor: Option<&'a str>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<serde_json::Value, PluginHostError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async { Ok(json!([])) })
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    struct Noop;
    impl Wake for Noop {
        fn wake(self: std::sync::Arc<Self>) {}
    }
    let waker = Waker::from(std::sync::Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::yield_now();
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn temp_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("goya-plugin-test-{}-{stamp}", std::process::id()))
}

fn request(origin: &str, method: &str, path: &str) -> String {
    let address = origin.strip_prefix("http://").unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .write_all(
            format!("{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn message(instance: &PluginInstance, generation: u64, operation: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "requestId":"req-1",
        "instanceId":instance.instance_id(),
        "sessionId":instance.session_id(),
        "generation":generation,
        "operation":operation
    }))
    .unwrap()
}

#[test]
fn loopback_server_scopes_assets_and_rejects_unsafe_requests() {
    let root = temp_dir();
    fs::create_dir_all(&root).unwrap();
    fs::copy(fixture_root().join("index.html"), root.join("index.html")).unwrap();
    fs::copy(fixture_root().join("style.css"), root.join("style.css")).unwrap();
    fs::write(root.join("large.bin"), vec![0_u8; 4 * 1024 * 1024 + 1]).unwrap();
    let outside = root.with_extension("outside.txt");
    fs::write(&outside, "outside-root").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("escape.txt")).unwrap();

    let instance = PluginInstance::serve("fixture.plugin", &root, "session-7").unwrap();
    let first = request(instance.origin(), "GET", "/index.html");
    assert!(first.starts_with("HTTP/1.1 200 OK"));
    assert!(first.contains("Content-Type: text/html; charset=utf-8"));
    assert!(first.contains("Content-Security-Policy: default-src 'self'"));
    assert!(first.contains("connect-src 'none'"));
    assert!(first.contains("Plugin fixture"));
    assert!(request(instance.origin(), "POST", "/index.html").starts_with("HTTP/1.1 405"));
    assert!(request(instance.origin(), "GET", "/%2e%2e/outside.txt").starts_with("HTTP/1.1 400"));
    assert!(request(instance.origin(), "GET", "/%ZZ").starts_with("HTTP/1.1 400"));
    assert!(request(instance.origin(), "GET", "/large.bin").starts_with("HTTP/1.1 413"));
    #[cfg(unix)]
    assert!(request(instance.origin(), "GET", "/escape.txt").starts_with("HTTP/1.1 404"));

    drop(instance);
    fs::remove_dir_all(root).unwrap();
    fs::remove_file(outside).unwrap();
}

#[test]
fn capability_broker_checks_source_origin_session_instance_generation_and_grant() {
    let instance = PluginInstance::serve("fixture.plugin", fixture_root(), "session-7").unwrap();
    let body = message(
        &instance,
        instance.broker().generation(),
        "session.readSummary",
    );
    assert!(matches!(
        instance
            .broker()
            .authorize_message(instance.instance_id(), instance.origin(), &body)
            .unwrap_err(),
        PluginHostError::CapabilityDenied
    ));
    // Host grants explicitly; the plugin message cannot do so itself.
    let mut instance = instance;
    instance
        .broker_mut()
        .grant(PluginCapability::SessionSummaryRead)
        .unwrap();
    let body = message(
        &instance,
        instance.broker().generation(),
        "session.readSummary",
    );
    let authorized = instance
        .broker()
        .authorize_message(instance.instance_id(), instance.origin(), &body)
        .unwrap();
    assert_eq!(authorized.operation, PluginOperation::ReadSessionSummary);
    assert_eq!(authorized.session_id, "session-7");

    assert!(matches!(
        instance
            .broker()
            .authorize_message("another-instance", instance.origin(), &body)
            .unwrap_err(),
        PluginHostError::ScopeMismatch
    ));
    assert!(matches!(
        instance
            .broker()
            .authorize_message(instance.instance_id(), "http://127.0.0.1:1", &body)
            .unwrap_err(),
        PluginHostError::ScopeMismatch
    ));
    let wrong_session = serde_json::to_vec(&json!({
        "requestId":"req-2","instanceId":instance.instance_id(),"sessionId":"other",
        "generation":instance.broker().generation(),"operation":"session.readSummary"
    }))
    .unwrap();
    assert!(matches!(
        instance
            .broker()
            .authorize_message(instance.instance_id(), instance.origin(), &wrong_session)
            .unwrap_err(),
        PluginHostError::ScopeMismatch
    ));
    let unknown = message(&instance, instance.broker().generation(), "host.rawRpc");
    assert!(matches!(
        instance
            .broker()
            .authorize_message(instance.instance_id(), instance.origin(), &unknown),
        Err(PluginHostError::InvalidMessage("unknown operation"))
    ));
    let mut extra: serde_json::Value = serde_json::from_slice(&body).unwrap();
    extra["rawCdp"] = json!(true);
    assert!(matches!(
        instance.broker().authorize_message(
            instance.instance_id(),
            instance.origin(),
            &serde_json::to_vec(&extra).unwrap()
        ),
        Err(PluginHostError::InvalidMessage("unknown or missing field"))
    ));
    assert!(matches!(
        instance.broker().authorize_message(
            instance.instance_id(),
            instance.origin(),
            &vec![b'x'; 64 * 1024 + 1]
        ),
        Err(PluginHostError::InvalidMessage("size limit"))
    ));
}

#[test]
fn revoke_during_async_host_read_suppresses_result() {
    let mut instance =
        PluginInstance::serve("fixture.plugin", fixture_root(), "session-7").unwrap();
    instance
        .broker_mut()
        .grant(PluginCapability::SessionSummaryRead)
        .unwrap();
    let bytes = message(
        &instance,
        instance.broker().generation(),
        "session.readSummary",
    );
    let request = instance
        .broker()
        .authorize_message(instance.instance_id(), instance.origin(), &bytes)
        .unwrap();
    let service = RevokingService(instance.broker().revocation_handle());
    let result = block_on(instance.broker().execute_authorized(&request, &service));
    assert!(matches!(result, Err(PluginHostError::StaleGeneration)));
}

#[test]
fn navigation_and_revocation_invalidate_old_generation_and_clear_grants() {
    let mut instance =
        PluginInstance::serve("fixture.plugin", fixture_root(), "session-7").unwrap();
    instance
        .broker_mut()
        .grant(PluginCapability::SessionSummaryRead)
        .unwrap();
    let old_generation = instance.broker().generation();
    let old_message = message(&instance, old_generation, "session.readSummary");
    let origin = instance.origin().to_owned();
    instance.broker_mut().document_committed(&origin).unwrap();
    assert!(matches!(
        instance
            .broker()
            .authorize_message(instance.instance_id(), &origin, &old_message)
            .unwrap_err(),
        PluginHostError::StaleGeneration
    ));
    let current = message(
        &instance,
        instance.broker().generation(),
        "session.readSummary",
    );
    assert!(matches!(
        instance
            .broker()
            .authorize_message(instance.instance_id(), &origin, &current)
            .unwrap_err(),
        PluginHostError::CapabilityDenied
    ));
    instance
        .broker_mut()
        .grant(PluginCapability::SessionSummaryRead)
        .unwrap();
    instance.broker_mut().revoke();
    let after_revoke = message(
        &instance,
        instance.broker().generation(),
        "session.readSummary",
    );
    assert!(matches!(
        instance
            .broker()
            .authorize_message(instance.instance_id(), &origin, &after_revoke)
            .unwrap_err(),
        PluginHostError::CapabilityDenied
    ));
    instance.broker_mut().teardown();
    assert!(matches!(
        instance
            .broker()
            .authorize_message(instance.instance_id(), &origin, &after_revoke)
            .unwrap_err(),
        PluginHostError::Revoked
    ));
}

#[test]
#[ignore = "requires real sandboxed Chromium; execute explicitly"]
fn synthetic_plugin_runs_in_private_browser_and_returns_bounded_rgba() {
    let executable = std::env::var_os("GOYA_CHROMIUM")
        .map(PathBuf::from)
        .or_else(|| {
            [
                "/usr/bin/google-chrome",
                "/usr/bin/chromium",
                "/usr/bin/chromium-browser",
            ]
            .iter()
            .map(Path::new)
            .find(|path| path.is_file())
            .map(Path::to_path_buf)
        });
    let Some(executable) = executable else {
        if std::env::var_os("GOYA_REQUIRE_BROWSER_SMOKE").is_some() {
            panic!("GOYA_REQUIRE_BROWSER_SMOKE is set but no Chromium executable was found");
        }
        panic!("native plugin acceptance requires a Chromium executable");
    };
    let instance =
        PluginInstance::serve("fixture.plugin", fixture_root(), "synthetic-session").unwrap();
    let mut browser = BrowserPluginSession::launch(instance, executable, 320, 180).unwrap();
    let frame = browser.capture_frame().unwrap();
    assert_eq!(
        (frame.width, frame.height, frame.stride),
        (320, 180, 320 * 4)
    );
    assert_eq!(frame.rgba.len(), 320 * 180 * 4);
    assert_eq!(&frame.rgba[..3], &[0x12, 0x34, 0x56]);
    browser.close();
}

#[test]
#[ignore = "requires real sandboxed Chromium; execute explicitly"]
fn real_chrome_plugin_binding_returns_projection_reply() {
    let executable = std::env::var_os("GOYA_CHROMIUM")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/bin/google-chrome"));
    assert!(
        executable.is_file(),
        "Chrome executable missing: {}",
        executable.display()
    );
    let instance =
        PluginInstance::serve("fixture.plugin", fixture_root(), "chrome-session").unwrap();
    let mut browser = BrowserPluginSession::launch(instance, executable, 320, 180).unwrap();
    browser.grant(PluginCapability::SessionSummaryRead).unwrap();
    browser.request_host_summary().unwrap();
    let service = goya_plugin_host::ProjectionHostService {
        summaries: [(
            "chrome-session".to_owned(),
            json!({"title":"fixture summary"}),
        )]
        .into(),
        ..Default::default()
    };
    let mut handled = 0;
    for _ in 0..50 {
        handled += block_on(browser.pump_bridge(&service)).unwrap();
        if handled > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(handled, 1, "Chrome binding event was not dispatched");
    let reply = browser.plugin_bridge_result().unwrap();
    assert!(
        reply.contains("fixture summary"),
        "Chrome page did not receive Host reply: {reply}"
    );
    browser.close();
}

#[test]
#[cfg(target_os = "linux")]
fn root_directory_replacement_cannot_redirect_the_asset_capability() {
    let root = temp_dir();
    let moved = root.with_extension("held");
    let outside = root.with_extension("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("index.html"), "original plugin").unwrap();
    fs::write(outside.join("index.html"), "outside secret").unwrap();
    let instance = PluginInstance::serve("fixture.plugin", &root, "session-7").unwrap();
    fs::rename(&root, &moved).unwrap();
    std::os::unix::fs::symlink(&outside, &root).unwrap();
    let response = request(instance.origin(), "GET", "/index.html");
    assert!(response.contains("original plugin"));
    assert!(!response.contains("outside secret"));
    let address = instance.origin().strip_prefix("http://").unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .write_all(b"GET /index.html HTTP/1.1\r\nHost: attacker.test\r\n\r\n")
        .unwrap();
    let mut denied = String::new();
    stream.read_to_string(&mut denied).unwrap();
    assert!(denied.starts_with("HTTP/1.1 403"));
    drop(instance);
    fs::remove_file(root).unwrap();
    fs::remove_dir_all(moved).unwrap();
    fs::remove_dir_all(outside).unwrap();
}
