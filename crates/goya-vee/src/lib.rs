//! Adapter to the ViCo Execution Environment (VEE).
//!
//! VEE serves a REST API on port 9987 (`POST /vee/submit` for submission).
//! We cannot link VEE's crate, so this module
//! is a self-contained network adapter over [`std::net::TcpStream`], std-only.
//!
//! Contract surface:
//! - [`VeeClient`] connects to a VEE host/port and [`VeeClient::submit`]s a
//!   [`SubmitRequest`], returning a [`VeeResponse`] or [`VeeError`].
//! - [`SubmitRequest::to_json`] is a tiny deterministic JSON writer
//!   (string/number fields only, quotes/backslashes escaped; no serde).
//! - Response parsing is a hand-rolled scan for `success`, `execution_id`,
//!   `status`, and `error`. This is deliberately NOT a full JSON parser: keys
//!   nested in other objects with the same names, escaped quotes inside the
//!   values, or key order changes may mis-extract. `VeeResponse::raw` always
//!   carries the full body so callers can re-parse if needed.
//! - Offline degradation: a refused connection surfaces as
//!   [`VeeError::Connection`]; the adapter never panics. Use
//!   [`VeeClient::available`] to probe before submitting so the application
//!   can continue without VEE.

use goya_determinism::DeterminismClass;
use goya_executable::{execute, CompiledProgram, Instruction, VmError, BYTECODE_VERSION};
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
/// The EDC bridge accepts only small declarative programs. These bounds are
/// checked before producing a source string, so untrusted artifacts cannot
/// cause unbounded allocation in this adapter.
const MAX_EDC_INSTRUCTIONS: usize = 2_048;
const MAX_EDC_STRING_BYTES: usize = 32 * 1024;
const MAX_EDC_SOURCE_BYTES: usize = 256 * 1024;

/// Convert a verified D0 bytecode artifact to a VEE Python request.
///
/// The generated Python interprets the validated UI instruction tuple and
/// prints a JSON serialization of the resulting tree. It uses only the
/// standard-library `json` module and stdout; it does not invoke a shell,
/// access files/network, or run arbitrary Goya code.
pub fn request_from_compiled_program(
    program: &CompiledProgram,
    agent_id: &str,
) -> Result<SubmitRequest, VeeCompileError> {
    if program.artifact.proof.class != DeterminismClass::Pure {
        return Err(VeeCompileError::UnsupportedClass);
    }
    if agent_id.is_empty() || agent_id.len() > 128 || agent_id.chars().any(char::is_control) {
        return Err(VeeCompileError::InvalidAgentId);
    }
    if program.bytecode.version != BYTECODE_VERSION {
        return Err(VeeCompileError::UnsupportedBytecodeVersion(
            program.bytecode.version,
        ));
    }
    if program.bytecode.instructions.len() > MAX_EDC_INSTRUCTIONS {
        return Err(VeeCompileError::TooManyInstructions);
    }
    let mut string_bytes = 0usize;
    for instruction in &program.bytecode.instructions {
        let added = match instruction {
            Instruction::Text { content, .. } => content.len(),
            Instruction::Button { label, .. } => label.len(),
            _ => 0,
        };
        string_bytes = string_bytes
            .checked_add(added)
            .ok_or(VeeCompileError::StringDataTooLarge)?;
        if string_bytes > MAX_EDC_STRING_BYTES {
            return Err(VeeCompileError::StringDataTooLarge);
        }
    }
    execute(&program.bytecode).map_err(VeeCompileError::InvalidBytecode)?;

    let mut source = String::from(
        "# Goya EDC D0 interpreter; bounded UI tree output.\n\
         GOYA_EDC_BYTECODE_VERSION = 1\nGOYA_EDC_INSTRUCTIONS = (\n",
    );
    for instruction in &program.bytecode.instructions {
        source.push_str("    ");
        match instruction {
            Instruction::BeginColumn { node, child_count } => {
                source.push_str(&format!("('column', {}, {}),", node.0, child_count));
            }
            Instruction::BeginRow { node, child_count } => {
                source.push_str(&format!("('row', {}, {}),", node.0, child_count));
            }
            Instruction::EndContainer => source.push_str("('end',),"),
            Instruction::Text { node, content } => {
                source.push_str(&format!("('text', {}, ", node.0));
                push_python_string(&mut source, content);
                source.push_str("),");
            }
            Instruction::Button {
                node,
                action,
                label,
            } => {
                source.push_str(&format!("('button', {}, {}, ", node.0, action.0));
                push_python_string(&mut source, label);
                source.push_str("),");
            }
            Instruction::Spacer { node, size_bits } => {
                source.push_str(&format!("('spacer_bits', {}, {}),", node.0, size_bits));
            }
        }
        source.push('\n');
        if source.len() > MAX_EDC_SOURCE_BYTES {
            return Err(VeeCompileError::SourceTooLarge);
        }
    }
    source.push_str(")\n");
    source.push_str(
        r#"import json
_frames = []
_root = None
for _ins in GOYA_EDC_INSTRUCTIONS:
    _op = _ins[0]
    if _op in ('column', 'row'):
        _frames.append([_op, _ins[1], _ins[2], []])
        continue
    if _op == 'end':
        _kind, _node, _expected, _children = _frames.pop()
        if len(_children) != _expected:
            raise ValueError('invalid child count')
        _item = {'kind': _kind, 'node': _node, 'children': _children}
    elif _op == 'text':
        _item = {'kind': 'text', 'node': _ins[1], 'content': _ins[2]}
    elif _op == 'button':
        _item = {'kind': 'button', 'node': _ins[1], 'action': _ins[2], 'label': _ins[3]}
    elif _op == 'spacer_bits':
        _item = {'kind': 'spacer', 'node': _ins[1], 'size_bits': _ins[2]}
    else:
        raise ValueError('invalid instruction')
    if _frames:
        _frames[-1][3].append(_item)
    elif _root is None:
        _root = _item
    else:
        raise ValueError('multiple roots')
if _frames or _root is None:
    raise ValueError('incomplete instruction stream')
print(json.dumps(_root, ensure_ascii=False, separators=(',', ':')))
"#,
    );
    if source.len() > MAX_EDC_SOURCE_BYTES {
        return Err(VeeCompileError::SourceTooLarge);
    }
    Ok(SubmitRequest {
        agent_id: agent_id.to_string(),
        language: "python".to_string(),
        source_code: source,
        capabilities: Vec::new(),
        budget: Some(VeeBudgetRequest {
            cpu_seconds: Some(3),
            memory_mb: Some(128),
            disk_mb: Some(64),
            token_budget: Some(1_024),
            wall_clock_seconds: Some(5),
        }),
    })
}

/// Why a compiled program could not be represented as a bounded VEE request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VeeCompileError {
    UnsupportedClass,
    UnsupportedBytecodeVersion(u32),
    InvalidAgentId,
    TooManyInstructions,
    StringDataTooLarge,
    SourceTooLarge,
    InvalidBytecode(VmError),
}

impl std::fmt::Display for VeeCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedClass => f.write_str("VEE bridge accepts D0 programs only"),
            Self::UnsupportedBytecodeVersion(version) => {
                write!(f, "unsupported bytecode version {version}")
            }
            Self::InvalidAgentId => {
                f.write_str("agent id must be 1-128 bytes without control characters")
            }
            Self::TooManyInstructions => {
                f.write_str("program exceeds the VEE bridge instruction limit")
            }
            Self::StringDataTooLarge => {
                f.write_str("program exceeds the VEE bridge string-data limit")
            }
            Self::SourceTooLarge => {
                f.write_str("generated Python exceeds the VEE bridge source limit")
            }
            Self::InvalidBytecode(error) => write!(f, "invalid bytecode: {error}"),
        }
    }
}

impl std::error::Error for VeeCompileError {}

/// Append a Python single-quoted string literal without relying on `repr` or
/// external serializers. Non-ASCII characters are preserved as UTF-8.
fn push_python_string(out: &mut String, value: &str) {
    out.push('\'');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 || (0x7f..=0x9f).contains(&(ch as u32)) => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('\'');
}

/// Connection parameters for a VEE instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeeClient {
    pub host: String,
    pub port: u16,
    auth_token: Option<String>,
}

impl VeeClient {
    /// Build a client for the VEE instance at `host:port`.
    pub fn connect(host: &str, port: u16) -> VeeClient {
        VeeClient {
            host: host.to_string(),
            port,
            auth_token: None,
        }
    }

    /// Attach an API token for VEE deployments with authentication enabled.
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Probe whether VEE is reachable. Never panics; returns `false` on any
    /// connection failure. Offline deployments use this to degrade gracefully.
    pub fn available(&mut self) -> bool {
        TcpStream::connect((self.host.as_str(), self.port)).is_ok()
    }

    /// Submit a task to VEE (`POST /vee/submit`).
    ///
    /// Opens a fresh TCP connection, writes a minimal HTTP/1.1 request with a
    /// correct `Content-Length`, reads the response up to `Content-Length`
    /// bytes or connection close, and extracts the VEE response fields by scan.
    pub fn submit(&mut self, req: &SubmitRequest) -> Result<VeeResponse, VeeError> {
        if self.host.chars().any(char::is_control)
            || self
                .auth_token
                .as_deref()
                .is_some_and(|token| token.is_empty() || token.chars().any(char::is_control))
        {
            return Err(VeeError::InvalidRequest);
        }
        let body = req.to_json();
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| VeeError::Connection(e.to_string()))?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|e| VeeError::Connection(e.to_string()))?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|e| VeeError::Connection(e.to_string()))?;

        let auth = self
            .auth_token
            .as_ref()
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        let http = format!(
            "POST /vee/submit HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             {auth}\
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

        let success = scan_bool_field(&body, "success");
        let error = scan_string_field(&body, "error");
        if success == Some(false) {
            return Err(VeeError::Rejected(
                error.unwrap_or_else(|| "VEE rejected the task".to_string()),
            ));
        }
        let task_id =
            scan_string_field(&body, "execution_id").or_else(|| scan_string_field(&body, "id"));
        Ok(VeeResponse {
            success,
            task_id,
            status: scan_string_field(&body, "status"),
            error,
            raw: body,
        })
    }
}

/// Budget fields supported by VEE's submit endpoint. Missing fields use VEE defaults.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VeeBudgetRequest {
    pub cpu_seconds: Option<u64>,
    pub memory_mb: Option<u64>,
    pub disk_mb: Option<u64>,
    pub token_budget: Option<u64>,
    pub wall_clock_seconds: Option<u64>,
}

/// A task submission payload matching VEE's `VeeSubmitRequest` contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitRequest {
    pub agent_id: String,
    pub language: String,
    pub source_code: String,
    pub capabilities: Vec<String>,
    pub budget: Option<VeeBudgetRequest>,
}

impl SubmitRequest {
    /// Deterministic JSON for VEE's `/vee/submit` request schema.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(64 + self.source_code.len());
        out.push('{');
        out.push_str("\"agent_id\":");
        push_json_string(&mut out, &self.agent_id);
        out.push_str(",\"language\":");
        push_json_string(&mut out, &self.language);
        out.push_str(",\"source_code\":");
        push_json_string(&mut out, &self.source_code);
        out.push_str(",\"capabilities\":[");
        for (index, capability) in self.capabilities.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            push_json_string(&mut out, capability);
        }
        out.push(']');
        if let Some(budget) = &self.budget {
            out.push_str(",\"budget\":{");
            let fields = [
                ("cpu_seconds", budget.cpu_seconds),
                ("memory_mb", budget.memory_mb),
                ("disk_mb", budget.disk_mb),
                ("token_budget", budget.token_budget),
                ("wall_clock_seconds", budget.wall_clock_seconds),
            ];
            let mut first = true;
            for (name, value) in fields {
                if let Some(value) = value {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    push_json_string(&mut out, name);
                    out.push(':');
                    out.push_str(&value.to_string());
                }
            }
            out.push('}');
        }
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
    /// The application's success field, when returned by VEE.
    pub success: Option<bool>,
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub error: Option<String>,
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
    /// Request contains a value unsafe for an HTTP header.
    InvalidRequest,
    /// VEE returned a valid application-level task rejection.
    Rejected(String),
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

/// Scan a top-level boolean field from the small flat VEE response object.
fn scan_bool_field(body: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{}\":", key);
    let start = body.find(&needle)? + needle.len();
    let rest = body[start..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_determinism::proofs::ExecutionProof;
    use goya_determinism::ProcessDecl;
    use goya_ir::{ActionId, Module, Node, NodeId};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn test_request(source: &str) -> SubmitRequest {
        SubmitRequest {
            agent_id: "goya-test".to_string(),
            capabilities: vec!["process_spawn".to_string(), "network_read".to_string()],
            language: "python".to_string(),
            source_code: source.to_string(),
            budget: Some(VeeBudgetRequest {
                cpu_seconds: Some(3),
                memory_mb: Some(128),
                disk_mb: Some(64),
                token_budget: Some(800),
                wall_clock_seconds: Some(5),
            }),
        }
    }

    fn compiled_module(module: &Module) -> CompiledProgram {
        goya_executable::compile_verified(
            module,
            &ProcessDecl::new("vee-export", DeterminismClass::Pure, vec![]),
            &ExecutionProof {
                process: "vee-export".into(),
                class: DeterminismClass::Pure,
                compiler_version: "test".into(),
                schema_version: 1,
                policy_version: 1,
                target: "test".into(),
            },
        )
        .expect("compile test program")
    }

    fn compiled_text(content: &str) -> CompiledProgram {
        compiled_module(&Module {
            nodes: vec![(
                NodeId(1),
                Node::Text {
                    content: content.into(),
                },
            )],
            root: Some(NodeId(1)),
        })
    }

    fn expected_execution_json(node: &goya_executable::ExecutionNode, out: &mut String) {
        use goya_executable::ExecutionNode;
        match node {
            ExecutionNode::Column { node, children } => {
                expected_container_json("column", node.0, children, out);
            }
            ExecutionNode::Row { node, children } => {
                expected_container_json("row", node.0, children, out);
            }
            ExecutionNode::Text { node, content } => {
                out.push_str("{\"kind\":\"text\",\"node\":");
                out.push_str(&node.0.to_string());
                out.push_str(",\"content\":");
                push_json_string(out, content);
                out.push('}');
            }
            ExecutionNode::Button {
                node,
                action,
                label,
            } => {
                out.push_str("{\"kind\":\"button\",\"node\":");
                out.push_str(&node.0.to_string());
                out.push_str(",\"action\":");
                out.push_str(&action.0.to_string());
                out.push_str(",\"label\":");
                push_json_string(out, label);
                out.push('}');
            }
            ExecutionNode::Spacer { node, size } => {
                out.push_str("{\"kind\":\"spacer\",\"node\":");
                out.push_str(&node.0.to_string());
                out.push_str(",\"size_bits\":");
                out.push_str(&size.to_bits().to_string());
                out.push('}');
            }
        }
    }

    fn expected_container_json(
        kind: &str,
        node: u64,
        children: &[goya_executable::ExecutionNode],
        out: &mut String,
    ) {
        out.push_str("{\"kind\":\"");
        out.push_str(kind);
        out.push_str("\",\"node\":");
        out.push_str(&node.to_string());
        out.push_str(",\"children\":[");
        for (index, child) in children.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            expected_execution_json(child, out);
        }
        out.push_str("]}");
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

    const CANNED_OK: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"success\":true,\"execution_id\":\"t-1\",\"status\":\"pending\",\"message\":\"ok\"}";

    #[test]
    fn json_writer_basic_and_field_order() {
        let json = test_request("HALT").to_json();
        assert_eq!(
            json,
            "{\"agent_id\":\"goya-test\",\"language\":\"python\",\"source_code\":\"HALT\",\"capabilities\":[\"process_spawn\",\"network_read\"],\"budget\":{\"cpu_seconds\":3,\"memory_mb\":128,\"disk_mb\":64,\"token_budget\":800,\"wall_clock_seconds\":5}}"
        );
    }

    #[test]
    fn d0_program_becomes_deterministic_inert_python_request() {
        let program = compiled_text("hi 'there'\\\nworld");
        let first = request_from_compiled_program(&program, "goya-test").unwrap();
        let second = request_from_compiled_program(&program, "goya-test").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.language, "python");
        assert!(first.capabilities.is_empty());
        assert_eq!(first.budget.as_ref().unwrap().cpu_seconds, Some(3));
        assert!(first
            .source_code
            .starts_with("# Goya EDC D0 interpreter; bounded UI tree output."));
        assert!(first
            .source_code
            .contains("('text', 1, 'hi \\'there\\'\\\\\\nworld'),"));
        assert!(first.source_code.contains("GOYA_EDC_BYTECODE_VERSION = 1"));
        assert!(first.source_code.contains("print(json.dumps(_root"));
        assert!(!first.source_code.contains("exec("));
        assert!(!first.source_code.contains("subprocess"));
    }

    #[test]
    fn generated_python_interpreter_matches_reference_vm_json() {
        use std::io::Write as _;
        use std::process::{Command, Stdio};

        let program = compiled_module(&Module {
            nodes: vec![
                (
                    NodeId(1),
                    Node::Column {
                        children: vec![NodeId(2), NodeId(5)],
                    },
                ),
                (
                    NodeId(2),
                    Node::Row {
                        children: vec![NodeId(3), NodeId(4)],
                    },
                ),
                (
                    NodeId(3),
                    Node::Text {
                        content: "hello 'VEE'\nλ".into(),
                    },
                ),
                (
                    NodeId(4),
                    Node::Button {
                        label: "activate\\now".into(),
                        on_activate: ActionId(9),
                    },
                ),
                (NodeId(5), Node::Spacer { size: 2.5 }),
            ],
            root: Some(NodeId(1)),
        });
        let reference = execute(&program.bytecode).expect("reference VM");
        let mut expected = String::new();
        expected_execution_json(&reference, &mut expected);

        let probe = Command::new("python3").arg("--version").output();
        match probe {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("failed to probe python3: {error}"),
            Ok(output) => assert!(output.status.success(), "python3 --version failed"),
        }

        let request = request_from_compiled_program(&program, "goya-test").unwrap();
        let mut child = Command::new("python3")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn python3");
        child
            .stdin
            .take()
            .expect("python stdin")
            .write_all(request.source_code.as_bytes())
            .expect("write generated source");
        let output = child.wait_with_output().expect("wait for python3");
        assert!(
            output.status.success(),
            "python interpreter failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = String::from_utf8(output.stdout).expect("python output is UTF-8");
        assert_eq!(actual.strip_suffix('\n').unwrap_or(&actual), expected);
    }

    #[test]
    fn d0_bridge_rejects_non_d0_bad_bytecode_and_oversized_strings() {
        let mut non_d0 = compiled_text("small");
        non_d0.artifact.proof.class = DeterminismClass::StateDeterministic;
        assert_eq!(
            request_from_compiled_program(&non_d0, "agent"),
            Err(VeeCompileError::UnsupportedClass)
        );

        let mut tampered = compiled_text("small");
        tampered.bytecode.content_hash ^= 1;
        assert_eq!(
            request_from_compiled_program(&tampered, "agent"),
            Err(VeeCompileError::InvalidBytecode(VmError::HashMismatch))
        );
        assert_eq!(
            request_from_compiled_program(
                &compiled_text(&"x".repeat(MAX_EDC_STRING_BYTES + 1)),
                "agent"
            ),
            Err(VeeCompileError::StringDataTooLarge)
        );
    }

    #[test]
    fn d0_bridge_validates_agent_id() {
        let program = compiled_text("small");
        assert_eq!(
            request_from_compiled_program(&program, "bad\nid"),
            Err(VeeCompileError::InvalidAgentId)
        );
        assert_eq!(
            request_from_compiled_program(&program, ""),
            Err(VeeCompileError::InvalidAgentId)
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
        assert_eq!(resp.success, Some(true));
        assert_eq!(resp.status.as_deref(), Some("pending"));
        assert_eq!(
            resp.raw,
            "{\"success\":true,\"execution_id\":\"t-1\",\"status\":\"pending\",\"message\":\"ok\"}"
        );
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
            req.starts_with("POST /vee/submit HTTP/1.1\r\n"),
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
    fn request_uses_current_vee_fields_and_optional_bearer_auth() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = captured.clone();
        let port = spawn_mock_with(CANNED_OK, move |req| {
            *sink.lock().expect("lock") = req;
        });
        let mut client = VeeClient::connect("127.0.0.1", port).with_bearer_token("submit-token");
        client.submit(&test_request("AQID")).expect("submit ok");
        let req = captured.lock().expect("lock").clone();
        assert!(req.contains("Authorization: Bearer submit-token\r\n"));
        assert!(req.contains("\"agent_id\":\"goya-test\""));
        assert!(req.contains("\"capabilities\":[\"process_spawn\",\"network_read\"]"));
        assert!(req.contains("\"wall_clock_seconds\":5"));
        assert!(req.contains("\"cpu_seconds\":3"));
    }

    #[test]
    fn application_rejection_is_reported_and_header_injection_refused() {
        let port = spawn_mock("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"success\":false,\"error\":\"unsupported language\"}");
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert_eq!(
            client.submit(&test_request("HALT")),
            Err(VeeError::Rejected("unsupported language".into()))
        );
        let mut bad =
            VeeClient::connect("127.0.0.1", 9987).with_bearer_token("ok\r\nX-Injected: yes");
        assert_eq!(
            bad.submit(&test_request("HALT")),
            Err(VeeError::InvalidRequest)
        );
    }

    #[test]
    fn absent_budget_is_omitted_and_response_execution_id_is_used() {
        let mut req = test_request("print('hello')");
        req.budget = None;
        assert_eq!(
            req.to_json(),
            "{\"agent_id\":\"goya-test\",\"language\":\"python\",\"source_code\":\"print('hello')\",\"capabilities\":[\"process_spawn\",\"network_read\"]}"
        );
        let port = spawn_mock(CANNED_OK);
        let mut client = VeeClient::connect("127.0.0.1", port);
        let resp = client.submit(&req).expect("submit ok");
        assert_eq!(resp.task_id.as_deref(), Some("t-1"));
        assert_eq!(resp.success, Some(true));
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
