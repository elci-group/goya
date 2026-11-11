//! Per-instance native UI-plugin content hosting and capability validation.
//!
//! This crate serves bounded static assets on a private loopback origin and
//! renders them through `goya-webview`'s public browser API. The capability
//! broker validates messages; it does not invoke Host or runtime services.

use goya_webview::{
    BrowserConfig, BrowserError, BrowserSession, KeyAction, PointerAction, RgbaFrame,
};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_REQUEST_LINE_BYTES: usize = 4096;
const MAX_ASSET_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PLUGIN_MESSAGE_BYTES: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
static INSTANCE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum PluginHostError {
    InvalidIdentity,
    InvalidRoot,
    InvalidPath,
    InvalidMessage(&'static str),
    CapabilityDenied,
    ScopeMismatch,
    StaleGeneration,
    Revoked,
    UnsupportedCursor,
    AssetTooLarge,
    Browser(BrowserError),
    Io(std::io::Error),
    Presentation(String),
}

impl std::fmt::Display for PluginHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentity => f.write_str("plugin or session identity is invalid"),
            Self::InvalidRoot => f.write_str("plugin root must be an existing directory"),
            Self::InvalidPath => f.write_str("plugin asset path is invalid or outside its root"),
            Self::InvalidMessage(reason) => write!(f, "plugin message rejected: {reason}"),
            Self::CapabilityDenied => f.write_str("plugin capability is not granted"),
            Self::ScopeMismatch => f.write_str("plugin message scope does not match its instance"),
            Self::StaleGeneration => f.write_str("plugin message belongs to a stale generation"),
            Self::Revoked => f.write_str("plugin instance has been revoked"),
            Self::UnsupportedCursor => {
                f.write_str("conversation cursor paging is not available in this projection")
            }
            Self::AssetTooLarge => f.write_str("plugin asset exceeds the configured limit"),
            Self::Browser(error) => write!(f, "plugin browser failed: {error}"),
            Self::Io(error) => write!(f, "plugin asset server failed: {error}"),
            Self::Presentation(error) => write!(f, "native surface presentation failed: {error}"),
        }
    }
}

impl std::error::Error for PluginHostError {}

impl From<std::io::Error> for PluginHostError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<BrowserError> for PluginHostError {
    fn from(value: BrowserError) -> Self {
        Self::Browser(value)
    }
}

/// Host-issued read-only authority. Plugin messages cannot create grants.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PluginCapability {
    SessionSummaryRead,
    ConversationRead,
}

/// Operations recognized by the initial bridge contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginOperation {
    ReadSessionSummary,
    ReadConversation,
}

impl PluginOperation {
    fn parse(value: &str) -> Result<Self, PluginHostError> {
        match value {
            "session.readSummary" => Ok(Self::ReadSessionSummary),
            "session.readConversation" => Ok(Self::ReadConversation),
            _ => Err(PluginHostError::InvalidMessage("unknown operation")),
        }
    }

    fn required_capability(self) -> PluginCapability {
        match self {
            Self::ReadSessionSummary => PluginCapability::SessionSummaryRead,
            Self::ReadConversation => PluginCapability::ConversationRead,
        }
    }
}

/// Host-side authorization result. It has no method that executes an RPC.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthorizedPluginRequest {
    pub request_id: String,
    pub instance_id: String,
    pub session_id: String,
    pub generation: u64,
    pub operation: PluginOperation,
    pub payload: Option<Value>,
}

/// Validates one instance's capability boundary.
pub struct CapabilityBroker {
    plugin_id: String,
    instance_id: String,
    session_id: String,
    origin: String,
    active_origin: Option<String>,
    generation: u64,
    grants: Vec<PluginCapability>,
    live_generation: Arc<AtomicU64>,
    live: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct PluginRevocationHandle {
    generation: Arc<AtomicU64>,
    live: Arc<AtomicBool>,
}
impl PluginRevocationHandle {
    pub fn revoke(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
    pub fn teardown(&self) {
        self.live.store(false, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl CapabilityBroker {
    fn new(plugin_id: String, instance_id: String, session_id: String, origin: String) -> Self {
        Self {
            plugin_id,
            instance_id,
            session_id,
            active_origin: Some(origin.clone()),
            origin,
            generation: 1,
            grants: Vec::new(),
            live_generation: Arc::new(AtomicU64::new(1)),
            live: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn revocation_handle(&self) -> PluginRevocationHandle {
        PluginRevocationHandle {
            generation: self.live_generation.clone(),
            live: self.live.clone(),
        }
    }

    pub fn grant(&mut self, capability: PluginCapability) -> Result<(), PluginHostError> {
        if self.active_origin.as_deref() != Some(self.origin.as_str()) {
            return Err(PluginHostError::Revoked);
        }
        if !self.grants.contains(&capability) {
            self.grants.push(capability);
        }
        Ok(())
    }

    /// Revoke every grant while keeping the current local document eligible
    /// for a later explicit host grant.
    pub fn revoke(&mut self) {
        self.advance_generation();
        self.grants.clear();
        self.restore_live();
    }

    /// Called by the trusted browser owner after a document navigation.
    /// Only the original instance origin can remain eligible for grants.
    pub fn document_committed(&mut self, observed_origin: &str) -> Result<(), PluginHostError> {
        self.advance_generation();
        self.grants.clear();
        if observed_origin == self.origin {
            self.active_origin = Some(self.origin.clone());
            self.restore_live();
            Ok(())
        } else {
            self.active_origin = None;
            Err(PluginHostError::ScopeMismatch)
        }
    }

    pub fn teardown(&mut self) {
        self.advance_generation();
        self.grants.clear();
        self.active_origin = None;
        self.revoke_live();
    }

    /// `source_instance_id` and `observed_origin` must come from the trusted
    /// browser event adapter; values inside JSON are never used for them.
    pub fn authorize_message(
        &self,
        source_instance_id: &str,
        observed_origin: &str,
        bytes: &[u8],
    ) -> Result<AuthorizedPluginRequest, PluginHostError> {
        if self.active_origin.is_none() {
            return Err(PluginHostError::Revoked);
        }
        if source_instance_id != self.instance_id || observed_origin != self.origin {
            return Err(PluginHostError::ScopeMismatch);
        }
        if bytes.is_empty() || bytes.len() > MAX_PLUGIN_MESSAGE_BYTES {
            return Err(PluginHostError::InvalidMessage("size limit"));
        }
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|_| PluginHostError::InvalidMessage("invalid JSON"))?;
        let object = value
            .as_object()
            .ok_or(PluginHostError::InvalidMessage("expected object"))?;
        let allowed = [
            "requestId",
            "instanceId",
            "sessionId",
            "generation",
            "operation",
            "payload",
        ];
        if object.keys().any(|key| !allowed.contains(&key.as_str()))
            || [
                "requestId",
                "instanceId",
                "sessionId",
                "generation",
                "operation",
            ]
            .iter()
            .any(|key| !object.contains_key(*key))
        {
            return Err(PluginHostError::InvalidMessage("unknown or missing field"));
        }
        let bounded_string = |key: &str, max: usize| -> Result<&str, PluginHostError> {
            object[key]
                .as_str()
                .filter(|value| {
                    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
                })
                .ok_or(PluginHostError::InvalidMessage("invalid string field"))
        };
        let request_id = bounded_string("requestId", 128)?.to_owned();
        if bounded_string("instanceId", 256)? != self.instance_id
            || bounded_string("sessionId", 256)? != self.session_id
        {
            return Err(PluginHostError::ScopeMismatch);
        }
        if object["generation"].as_u64() != Some(self.generation) {
            return Err(PluginHostError::StaleGeneration);
        }
        let operation = PluginOperation::parse(bounded_string("operation", 128)?)?;
        let capability = operation.required_capability();
        if !self.grants.contains(&capability) {
            return Err(PluginHostError::CapabilityDenied);
        }
        let payload = object.get("payload").cloned();
        if let Some(payload) = &payload {
            if !payload.is_object() {
                return Err(PluginHostError::InvalidMessage("payload must be an object"));
            }
        }
        if operation == PluginOperation::ReadSessionSummary && payload.is_some() {
            return Err(PluginHostError::InvalidMessage(
                "summary operation takes no payload",
            ));
        }
        Ok(AuthorizedPluginRequest {
            request_id,
            instance_id: self.instance_id.clone(),
            session_id: self.session_id.clone(),
            generation: self.generation,
            operation,
            payload,
        })
    }

    /// Execute a previously authorized read against the trusted Host service.
    /// A revocation or navigation racing the await suppresses the result.
    pub async fn execute_authorized(
        &self,
        request: &AuthorizedPluginRequest,
        service: &dyn PluginHostService,
    ) -> Result<Value, PluginHostError> {
        if request.instance_id != self.instance_id || request.session_id != self.session_id {
            return Err(PluginHostError::ScopeMismatch);
        }
        if !self.live.load(Ordering::Acquire) {
            return Err(PluginHostError::Revoked);
        }
        let generation = self.live_generation.load(Ordering::Acquire);
        if generation != request.generation {
            return Err(PluginHostError::StaleGeneration);
        }
        let result = match request.operation {
            PluginOperation::ReadSessionSummary => {
                service.read_session_summary(&request.session_id).await?
            }
            PluginOperation::ReadConversation => {
                let cursor = request
                    .payload
                    .as_ref()
                    .and_then(|value| value.get("cursor"))
                    .and_then(Value::as_str);
                service
                    .read_conversation(&request.session_id, cursor)
                    .await?
            }
        };
        if !self.live.load(Ordering::Acquire) {
            return Err(PluginHostError::Revoked);
        }
        if self.live_generation.load(Ordering::Acquire) != request.generation {
            return Err(PluginHostError::StaleGeneration);
        }
        let bytes = serde_json::to_vec(&result)
            .map_err(|_| PluginHostError::InvalidMessage("service returned invalid data"))?;
        if bytes.len() > MAX_PLUGIN_MESSAGE_BYTES {
            return Err(PluginHostError::AssetTooLarge);
        }
        Ok(result)
    }

    fn advance_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.live_generation
            .store(self.generation, Ordering::Release);
    }

    fn revoke_live(&self) {
        self.live.store(false, Ordering::Release);
    }

    fn restore_live(&self) {
        self.live.store(true, Ordering::Release);
    }
}

/// Injected trusted Host read surface. Implementations return projected,
/// bounded data and must not expose runtime or repository objects to plugins.
pub trait PluginHostService: Send + Sync {
    fn read_session_summary<'a>(
        &'a self,
        session_id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Value, PluginHostError>> + Send + 'a>,
    >;

    fn read_conversation<'a>(
        &'a self,
        session_id: &'a str,
        cursor: Option<&'a str>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Value, PluginHostError>> + Send + 'a>,
    >;
}

/// Immutable, caller-owned projection snapshot. The native attached-session
/// client refreshes this value; plugin calls never reach repositories/runtime.
#[derive(Clone, Default)]
pub struct ProjectionHostService {
    pub summaries: std::collections::HashMap<String, Value>,
    pub conversations: std::collections::HashMap<String, Value>,
}

impl PluginHostService for ProjectionHostService {
    fn read_session_summary<'a>(
        &'a self,
        session_id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Value, PluginHostError>> + Send + 'a>,
    > {
        Box::pin(async move { clone_bounded(self.summaries.get(session_id)) })
    }
    fn read_conversation<'a>(
        &'a self,
        session_id: &'a str,
        _cursor: Option<&'a str>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Value, PluginHostError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if _cursor.is_some() {
                return Err(PluginHostError::UnsupportedCursor);
            }
            clone_bounded(self.conversations.get(session_id))
        })
    }
}

fn clone_bounded(value: Option<&Value>) -> Result<Value, PluginHostError> {
    let value = value.ok_or(PluginHostError::CapabilityDenied)?;
    if serde_json::to_vec(value)
        .map_err(|_| PluginHostError::InvalidMessage("invalid projection"))?
        .len()
        > MAX_PLUGIN_MESSAGE_BYTES
    {
        return Err(PluginHostError::AssetTooLarge);
    }
    Ok(value.clone())
}

/// One static plugin instance and its private loopback origin.
pub struct PluginInstance {
    plugin_id: String,
    instance_id: String,
    session_id: String,
    root: PathBuf,
    origin: String,
    _server: AssetServer,
    broker: CapabilityBroker,
}

impl PluginInstance {
    pub fn serve(
        plugin_id: &str,
        root: impl AsRef<Path>,
        session_id: &str,
    ) -> Result<Self, PluginHostError> {
        validate_identity(plugin_id)?;
        validate_identity(session_id)?;
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| PluginHostError::InvalidRoot)?;
        if !root.is_dir() || !root.join("index.html").is_file() {
            return Err(PluginHostError::InvalidRoot);
        }
        let instance_id = format!(
            "goya-plugin-{}-{}",
            std::process::id(),
            INSTANCE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let server = AssetServer::start(root.clone())?;
        let origin = server.origin.clone();
        let broker = CapabilityBroker::new(
            plugin_id.to_owned(),
            instance_id.clone(),
            session_id.to_owned(),
            origin.clone(),
        );
        Ok(Self {
            plugin_id: plugin_id.to_owned(),
            instance_id,
            session_id: session_id.to_owned(),
            root,
            origin,
            _server: server,
            broker,
        })
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn entry_url(&self) -> String {
        format!("{}/index.html", self.origin)
    }

    pub fn broker(&self) -> &CapabilityBroker {
        &self.broker
    }

    pub fn broker_mut(&mut self) -> &mut CapabilityBroker {
        &mut self.broker
    }

    pub fn canonical_root(&self) -> &Path {
        &self.root
    }

    pub fn local_asset_url(&self, path: &str) -> Result<String, PluginHostError> {
        validate_relative_url_path(path)?;
        Ok(format!("{}{}", self.origin, path))
    }
}

/// Owns a plugin instance and isolated browser. It exposes no CDP/session
/// identifiers, JavaScript evaluation, or arbitrary navigation methods.
pub struct BrowserPluginSession {
    browser: BrowserSession,
    instance: PluginInstance,
}

impl BrowserPluginSession {
    pub fn launch(
        instance: PluginInstance,
        executable: impl Into<PathBuf>,
        width: u32,
        height: u32,
    ) -> Result<Self, PluginHostError> {
        let mut browser = BrowserSession::launch(BrowserConfig::new(executable, width, height))?;
        browser.install_binding("goyaHostRequest")?;
        browser.navigate(&instance.entry_url())?;
        browser.initialize_plugin_bridge(
            instance.instance_id(),
            instance.session_id(),
            instance.broker().generation(),
        )?;
        let mut session = Self { browser, instance };
        session.check_origin()?;
        Ok(session)
    }

    // 页面声明的 origin 不是权限依据；从浏览器进程读取已提交 frame 的真实 origin。
    fn check_origin(&mut self) -> Result<(), PluginHostError> {
        match self.browser.committed_origin() {
            Ok(origin) if origin == self.instance.origin => Ok(()),
            _ => {
                self.instance.broker.teardown();
                self.browser.close();
                Err(PluginHostError::ScopeMismatch)
            }
        }
    }

    pub fn instance(&self) -> &PluginInstance {
        &self.instance
    }

    pub fn grant(&mut self, capability: PluginCapability) -> Result<(), PluginHostError> {
        self.check_origin()?;
        self.instance.broker.grant(capability)?;
        self.browser.initialize_plugin_bridge(
            self.instance.instance_id(),
            self.instance.session_id(),
            self.instance.broker.generation(),
        )?;
        Ok(())
    }

    pub fn revoke(&mut self) {
        self.instance.broker.revoke();
    }

    pub fn request_host_summary(&mut self) -> Result<(), PluginHostError> {
        self.browser.request_host_summary()?;
        Ok(())
    }

    pub fn plugin_bridge_result(&mut self) -> Result<String, PluginHostError> {
        Ok(self.browser.plugin_bridge_result()?)
    }

    pub fn authorize_message(
        &mut self,
        source_instance_id: &str,
        observed_origin: &str,
        bytes: &[u8],
    ) -> Result<AuthorizedPluginRequest, PluginHostError> {
        self.check_origin()?;
        self.instance
            .broker
            .authorize_message(source_instance_id, observed_origin, bytes)
    }

    /// Process currently queued bridge calls one at a time. Event source origin
    /// and context are provided by Chromium metadata, not plugin JSON.
    pub async fn pump_bridge(
        &mut self,
        service: &dyn PluginHostService,
    ) -> Result<usize, PluginHostError> {
        self.check_origin()?;
        let events = self.browser.poll_binding_events("goyaHostRequest");
        let mut handled = 0;
        for event in events {
            let Some(context_id) = event["executionContextId"].as_i64() else {
                continue;
            };
            let origin = self
                .browser
                .context_origin(context_id)
                .unwrap_or_default()
                .to_owned();
            if origin != self.instance.origin {
                continue;
            }
            let Some(payload) = event["payload"].as_str() else {
                continue;
            };
            let request = match self.instance.broker.authorize_message(
                self.instance.instance_id(),
                &origin,
                payload.as_bytes(),
            ) {
                Ok(request) => request,
                Err(_) => {
                    if let Ok(value) = serde_json::from_str::<Value>(payload) {
                        if let Some(request_id) =
                            value.get("requestId").and_then(Value::as_str).filter(|s| {
                                !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
                            })
                        {
                            let denied = serde_json::json!({"requestId":request_id,"ok":false,"error":"request denied"});
                            self.browser.reply_binding(
                                context_id,
                                "goyaHostRequest",
                                &origin,
                                &denied,
                            )?;
                        }
                    }
                    handled += 1;
                    continue;
                }
            };
            let result = self
                .instance
                .broker
                .execute_authorized(&request, service)
                .await;
            self.check_origin()?;
            let body = match result {
                Ok(data) => {
                    serde_json::json!({"requestId":request.request_id,"ok":true,"data":data})
                }
                Err(error) => {
                    serde_json::json!({"requestId":request.request_id,"ok":false,"error":error.to_string()})
                }
            };
            self.browser
                .reply_binding(context_id, "goyaHostRequest", &origin, &body)?;
            handled += 1;
        }
        Ok(handled)
    }

    pub fn navigate_local(&mut self, path: &str) -> Result<u64, PluginHostError> {
        let url = self.instance.local_asset_url(path)?;
        self.instance.broker.revoke();
        self.browser.navigate(&url)?;
        self.check_origin()?;
        self.instance
            .broker
            .document_committed(&self.instance.origin)?;
        self.browser.initialize_plugin_bridge(
            self.instance.instance_id(),
            self.instance.session_id(),
            self.instance.broker.generation(),
        )?;
        Ok(self.instance.broker.generation())
    }

    pub fn set_viewport(&mut self, width: u32, height: u32) -> Result<(), PluginHostError> {
        self.browser.set_viewport(width, height)?;
        Ok(())
    }

    pub fn dispatch_pointer(&mut self, action: PointerAction) -> Result<(), PluginHostError> {
        self.browser.dispatch_pointer(action)?;
        Ok(())
    }

    pub fn dispatch_key(&mut self, action: KeyAction) -> Result<(), PluginHostError> {
        self.browser.dispatch_key(action)?;
        Ok(())
    }

    pub fn capture_frame(&mut self) -> Result<RgbaFrame, PluginHostError> {
        self.check_origin()?;
        let frame = self.browser.capture_frame()?;
        self.check_origin()?;
        Ok(frame)
    }

    /// Present browser pixels through Goya's compositor-owned surface API.
    pub fn capture_and_present(
        &mut self,
        window: &mut goya_wayland::GoyaWindow,
    ) -> Result<RgbaFrame, PluginHostError> {
        let frame = self.capture_frame()?;
        window
            .present_rgba(&frame.rgba, frame.width, frame.height)
            .map_err(|error| PluginHostError::Presentation(error.to_string()))?;
        Ok(frame)
    }

    pub fn close(&mut self) {
        self.instance.broker.teardown();
        self.browser.close();
    }
}

impl Drop for BrowserPluginSession {
    fn drop(&mut self) {
        self.close();
    }
}

struct AssetServer {
    origin: String,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl AssetServer {
    fn start(root: PathBuf) -> Result<Self, PluginHostError> {
        let root = fs::File::open(root)?;
        let _ =
            open_asset(&root, Path::new("index.html")).map_err(|_| PluginHostError::InvalidRoot)?;
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let origin = format!("http://127.0.0.1:{}", address.port());
        let authority = address.to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("goya-plugin-assets".into())
            .spawn(move || serve_assets(listener, root, authority, worker_stop))?;
        Ok(Self {
            origin,
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for AssetServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_assets(listener: TcpListener, root: fs::File, authority: String, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                let _ = handle_asset_request(&mut stream, &root, &authority);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn handle_asset_request(
    stream: &mut TcpStream,
    root: &fs::File,
    authority: &str,
) -> std::io::Result<()> {
    let request = read_http_head(stream)?;
    let mut lines = request.split("\r\n");
    let Some(first) = lines.next() else {
        return write_response(stream, 400, "text/plain; charset=utf-8", b"bad request");
    };
    let mut parts = first.split_ascii_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if parts.next().is_some() || !(version == "HTTP/1.1" || version == "HTTP/1.0") {
        return write_response(stream, 400, "text/plain; charset=utf-8", b"bad request");
    }
    let hosts = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .collect::<Vec<_>>();
    if hosts != [authority] {
        return write_response(stream, 403, "text/plain; charset=utf-8", b"wrong host");
    }
    if method != "GET" && method != "HEAD" {
        return write_response(
            stream,
            405,
            "text/plain; charset=utf-8",
            b"method not allowed",
        );
    }
    let path = match decode_asset_path(target) {
        Ok(path) => path,
        Err(_) => return write_response(stream, 400, "text/plain; charset=utf-8", b"bad path"),
    };
    let file = match open_asset(root, &path) {
        Ok(file) => file,
        Err(AssetPathError::TooLarge) => {
            return write_response(stream, 413, "text/plain; charset=utf-8", b"asset too large")
        }
        Err(_) => return write_response(stream, 404, "text/plain; charset=utf-8", b"not found"),
    };
    let metadata = file.metadata()?;
    if metadata.len() > MAX_ASSET_BYTES {
        return write_response(stream, 413, "text/plain; charset=utf-8", b"asset too large");
    }
    let mut body = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_ASSET_BYTES + 1).read_to_end(&mut body)?;
    if body.len() as u64 > MAX_ASSET_BYTES {
        return write_response(stream, 413, "text/plain; charset=utf-8", b"asset too large");
    }
    let mime = mime_for_path(&path);
    if method == "HEAD" {
        write_response_head(stream, 200, mime, body.len())
    } else {
        write_response(stream, 200, mime, &body)
    }
}

fn read_http_head(stream: &mut TcpStream) -> std::io::Result<String> {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        if Instant::now() >= deadline {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_HTTP_HEADER_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP headers too large",
            ));
        }
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            bytes.truncate(end + 4);
            break;
        }
        if bytes.len() > MAX_REQUEST_LINE_BYTES
            && bytes.iter().position(|byte| *byte == b'\n').is_none()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "request line too large",
            ));
        }
    }
    String::from_utf8(bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "HTTP is not UTF-8"))
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    mime: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write_response_head(stream, status, mime, body.len())?;
    write_bounded(stream, body)?;
    stream.flush()
}

fn write_response_head(
    stream: &mut TcpStream,
    status: u16,
    mime: &str,
    content_length: usize,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {content_length}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'none'; frame-src 'none'; worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n\r\n"
    );
    write_bounded(stream, header.as_bytes())
}

fn write_bounded(stream: &mut TcpStream, mut bytes: &[u8]) -> std::io::Result<()> {
    stream.set_nonblocking(true)?;
    let deadline = Instant::now() + IO_TIMEOUT;
    while !bytes.is_empty() {
        if Instant::now() >= deadline {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        match stream.write(bytes) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AssetPathError {
    Invalid,
    Missing,
    TooLarge,
}

fn decode_asset_path(target: &str) -> Result<PathBuf, AssetPathError> {
    if !target.starts_with('/') || target.starts_with("//") {
        return Err(AssetPathError::Invalid);
    }
    let path = target.split(['?', '#']).next().unwrap_or_default();
    let decoded = percent_decode(path.as_bytes()).ok_or(AssetPathError::Invalid)?;
    let decoded = std::str::from_utf8(&decoded).map_err(|_| AssetPathError::Invalid)?;
    if decoded
        .chars()
        .any(|c| c.is_control() || c == '\\' || c == '\0')
    {
        return Err(AssetPathError::Invalid);
    }
    let relative = decoded.trim_start_matches('/');
    if relative.is_empty() {
        return Ok(PathBuf::from("index.html"));
    }
    let path = Path::new(relative);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AssetPathError::Invalid);
    }
    Ok(path.to_path_buf())
}

// 先 canonicalize 再按路径打开存在目录替换竞态；以已打开根目录为能力边界，
// 由内核原子拒绝符号链接、magic link 和越界路径，绝不降级到不安全路径打开。
#[cfg(target_os = "linux")]
fn open_asset(root: &fs::File, relative: &Path) -> Result<fs::File, AssetPathError> {
    use rustix::fs::{openat2, Mode, OFlags, ResolveFlags};
    let fd = openat2(
        root,
        relative,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
    )
    .map_err(|_| AssetPathError::Missing)?;
    let file = fs::File::from(fd);
    let metadata = file.metadata().map_err(|_| AssetPathError::Missing)?;
    if !metadata.is_file() {
        return Err(AssetPathError::Invalid);
    }
    if metadata.len() > MAX_ASSET_BYTES {
        return Err(AssetPathError::TooLarge);
    }
    Ok(file)
}

#[cfg(not(target_os = "linux"))]
fn open_asset(_root: &fs::File, _relative: &Path) -> Result<fs::File, AssetPathError> {
    Err(AssetPathError::Invalid)
}

fn percent_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' {
            let high = hex(input.get(index + 1).copied()?)?;
            let low = hex(input.get(index + 2).copied()?)?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(input[index]);
            index += 1;
        }
    }
    Some(output)
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn validate_relative_url_path(path: &str) -> Result<(), PluginHostError> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['?', '#', '\\'])
        || decode_asset_path(path).is_err()
    {
        return Err(PluginHostError::InvalidPath);
    }
    Ok(())
}

fn validate_identity(value: &str) -> Result<(), PluginHostError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(PluginHostError::InvalidIdentity);
    }
    Ok(())
}
