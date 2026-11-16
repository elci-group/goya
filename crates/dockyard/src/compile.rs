//! Link one admitted service and publish an OCI layout beside it.
//!
//! `build` does not start a container. `verify` runs the static executable
//! and still reports image start as blocked.

use crate::admit::{admit, Diagnostic, Service};
use crate::oci::{self, read_archive, sha256_hex, ImageFiles};
use crate::{CONTENT_TYPE, CREATED, IMAGE_USER, LINK_MODE, METHOD, PROFILE, STATUS, TARGET};
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum Failure {
    Admit(Diagnostic),
    Output { code: &'static str, message: String },
}

impl Failure {
    pub fn code(&self) -> &str {
        match self {
            Self::Admit(diagnostic) => diagnostic.code,
            Self::Output { code, .. } => code,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Admit(diagnostic) => diagnostic.message.clone(),
            Self::Output { message, .. } => message.clone(),
        }
    }

    fn output(code: &'static str, message: impl Into<String>) -> Self {
        Self::Output {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub executable_sha256: String,
    pub layer_sha256: String,
    pub config_sha256: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    pub layout_pass: bool,
    pub executable_pass: bool,
    pub detail: String,
}

impl VerifyReport {
    pub fn qualification_blocked(&self) -> bool {
        self.layout_pass && self.executable_pass
    }
}

pub fn build(source: &str, out_dir: &Path) -> Result<BuildReport, Failure> {
    let service = admit(source).map_err(Failure::Admit)?;
    validate_new_dir(out_dir)?;
    let parent = parent_dir(out_dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(
        ".{name}.dockyard-tmp-{pid}-{stamp}",
        name = out_dir
            .file_name()
            .and_then(|text| text.to_str())
            .unwrap_or("out"),
        pid = std::process::id(),
    );
    let tmp = parent.join(tmp_name);
    if tmp.exists() {
        return Err(Failure::output(
            "DK-E1002",
            "temporary output already exists",
        ));
    }
    fs::create_dir(&tmp).map_err(|error| {
        Failure::output(
            "DK-E3001",
            format!("cannot create temporary output: {error}"),
        )
    })?;
    let built = publish(&service, source, &tmp);
    if built.is_err() {
        let _ = fs::remove_dir_all(&tmp);
        return built;
    }
    let _ = fs::remove_dir_all(tmp.join("src"));
    if out_dir.exists() {
        let _ = fs::remove_dir_all(&tmp);
        return Err(Failure::output("DK-E1002", "output already exists"));
    }
    fs::rename(&tmp, out_dir).map_err(|error| {
        let _ = fs::remove_dir_all(&tmp);
        Failure::output("DK-E3001", format!("cannot publish output: {error}"))
    })?;
    built
}

pub fn verify(dir: &Path) -> Result<VerifyReport, Failure> {
    let checked = match check_layout(dir) {
        Ok(checked) => checked,
        Err(failure) => {
            return Ok(VerifyReport {
                layout_pass: false,
                executable_pass: false,
                detail: format!("{}: {}", failure.code(), failure.message()),
            });
        }
    };
    match exercise(&checked) {
        Ok(()) => Ok(VerifyReport {
            layout_pass: true,
            executable_pass: true,
            detail: String::new(),
        }),
        Err(detail) => Ok(VerifyReport {
            layout_pass: true,
            executable_pass: false,
            detail,
        }),
    }
}

pub(crate) struct Checked {
    pub(crate) binary: Vec<u8>,
    pub(crate) port: u16,
    pub(crate) path: String,
    pub(crate) body_sha256: String,
}

fn publish(service: &Service, source: &str, dir: &Path) -> Result<BuildReport, Failure> {
    let binary_path = dir.join("service");
    link(&emit_source(service), dir, &binary_path)?;
    let binary = fs::read(&binary_path)
        .map_err(|error| Failure::output("DK-E3001", format!("cannot read executable: {error}")))?;
    require_static_x86_64(&binary).map_err(|message| Failure::output("DK-E2003", message))?;
    let image =
        oci::pack(&binary, service).map_err(|message| Failure::output("DK-E3001", message))?;
    write_bytes(&dir.join("oci-layout"), &image.oci_layout, 0o644)?;
    write_bytes(&dir.join("index.json"), &image.index_json, 0o644)?;
    let blob_dir = dir.join("blobs").join("sha256");
    fs::create_dir_all(&blob_dir).map_err(|error| {
        Failure::output("DK-E3001", format!("cannot create blob directory: {error}"))
    })?;
    for (digest, bytes) in &image.blobs {
        write_bytes(&blob_dir.join(digest), bytes, 0o644)?;
    }
    let evidence = dir.join("evidence");
    fs::create_dir(&evidence).map_err(|error| {
        Failure::output(
            "DK-E3001",
            format!("cannot create evidence directory: {error}"),
        )
    })?;
    let source_sha = sha256_hex(source.as_bytes());
    write_bytes(
        &evidence.join("report.txt"),
        report(source, service, &image).as_bytes(),
        0o644,
    )?;
    write_bytes(
        &evidence.join("sbom.spdx.json"),
        crate::sbom::spdx_json(
            &service.name,
            &source_sha,
            &image.executable_sha256,
            &image.layer_sha256,
        )
        .as_bytes(),
        0o644,
    )?;
    write_bytes(
        &evidence.join("provenance.json"),
        crate::provenance::predicate(
            &source_sha,
            &image.executable_sha256,
            &image.layer_sha256,
            &image.config_sha256,
            &image.manifest_sha256,
        )
        .as_bytes(),
        0o644,
    )?;
    set_mode(&binary_path, 0o755)?;
    Ok(BuildReport {
        executable_sha256: image.executable_sha256,
        layer_sha256: image.layer_sha256,
        config_sha256: image.config_sha256,
        manifest_sha256: image.manifest_sha256,
    })
}

fn report(source: &str, service: &Service, image: &ImageFiles) -> String {
    format!(
        "\
profile: {PROFILE}
target: {TARGET}
link: {LINK_MODE}
service: {name}
listen: {port}
route: {METHOD} {path}
status: {STATUS}
content-type: {CONTENT_TYPE}
source-sha256: {source_sha}
body-sha256: {body}
executable-sha256: {executable}
layer-sha256: {layer}
config-sha256: {config}
manifest-sha256: {manifest}
image-user: {IMAGE_USER}
image-start: BLOCKED
runtime-qualification: BLOCKED
",
        name = service.name,
        port = service.port,
        path = service.path,
        source_sha = sha256_hex(source.as_bytes()),
        body = image.body_sha256,
        executable = image.executable_sha256,
        layer = image.layer_sha256,
        config = image.config_sha256,
        manifest = image.manifest_sha256,
    )
}

fn emit_source(service: &Service) -> String {
    let body = service
        .body
        .bytes()
        .map(|byte| format!("0x{byte:02x}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut source = String::new();
    source.push_str("// Generated by Dockyard. Profile ");
    source.push_str(PROFILE);
    source.push_str(".\n");
    source.push_str(EMIT_HEAD);
    source.push_str(&format!("const PORT: u16 = {};\n", service.port));
    source.push_str("const PATH: &[u8] = b\"");
    source.push_str(&service.path);
    source.push_str("\";\nconst BODY: &[u8] = &[");
    source.push_str(&body);
    source.push_str("];\n");
    source.push_str(EMIT_TAIL);
    source
}

const EMIT_HEAD: &str = "\
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

";

const EMIT_TAIL: &str = r#"
fn main() {
    let listener = match TcpListener::bind(format!("0.0.0.0:{PORT}")) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("dockyard-service: bind failed: {error}");
            std::process::exit(1);
        }
    };
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let _ = respond(&mut stream);
    }
}

fn respond(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut buffer = [0u8; 2048];
    let mut filled = 0usize;
    loop {
        if filled == buffer.len() {
            break;
        }
        match stream.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => {
                filled += count;
                if buffer[..filled].windows(2).any(|pair| pair == b"\r\n") {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Ok(()),
        }
    }
    let line_end = buffer[..filled]
        .windows(2)
        .position(|pair| pair == b"\r\n")
        .unwrap_or(filled);
    let line = &buffer[..line_end];
    if request_matches(line) {
        write_response(stream, "200 OK", BODY)
    } else {
        write_response(stream, "404 Not Found", b"not found")
    }
}

fn request_matches(line: &[u8]) -> bool {
    let Some(rest) = line.strip_prefix(b"GET ") else {
        return false;
    };
    let path_len = PATH.len();
    if rest.len() < path_len || &rest[..path_len] != PATH {
        return false;
    }
    matches!(&rest[path_len..], b" HTTP/1.1" | b" HTTP/1.0")
}

fn write_response(stream: &mut TcpStream, status: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/plain\r\ncontent-length: {length}\r\nconnection: close\r\n\r\n",
        length = body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}
"#;

fn link(generated: &str, work: &Path, output: &Path) -> Result<(), Failure> {
    let src_dir = work.join("src");
    fs::create_dir_all(&src_dir).map_err(|error| {
        Failure::output(
            "DK-E3001",
            format!("cannot create generated source: {error}"),
        )
    })?;
    let main_rs = src_dir.join("main.rs");
    fs::write(&main_rs, generated).map_err(|error| {
        Failure::output(
            "DK-E3001",
            format!("cannot write generated source: {error}"),
        )
    })?;
    let src_abs = fs::canonicalize(&src_dir).map_err(|error| {
        Failure::output(
            "DK-E3001",
            format!("cannot resolve generated source: {error}"),
        )
    })?;
    let mut command = Command::new("rustc");
    command
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env("SOURCE_DATE_EPOCH", "0")
        .arg("--edition")
        .arg("2021")
        .arg("--crate-name")
        .arg("service")
        .arg("--crate-type")
        .arg("bin")
        .arg("-C")
        .arg("target-feature=+crt-static")
        .arg("-C")
        .arg("opt-level=3")
        .arg("-C")
        .arg("debuginfo=0")
        .arg("-C")
        .arg("overflow-checks=on")
        .arg("-C")
        .arg("link-arg=-Wl,--build-id=none")
        .arg("--remap-path-prefix")
        .arg(format!("{}=/dockyard/service", src_abs.display()))
        .arg("-o")
        .arg(output)
        .arg(src_abs.join("main.rs"));
    let compiled = match command.output() {
        Ok(compiled) => compiled,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Failure::output(
                "DK-E2001",
                "rustc is unavailable. Dockyard does not download a compiler.",
            ));
        }
        Err(error) => {
            return Err(Failure::output(
                "DK-E2002",
                format!("rustc failed to start: {error}"),
            ));
        }
    };
    if !compiled.status.success() {
        let stderr = String::from_utf8_lossy(&compiled.stderr);
        let trimmed: String = stderr.chars().take(2000).collect();
        return Err(Failure::output(
            "DK-E2002",
            format!("rustc failed: {trimmed}"),
        ));
    }
    Ok(())
}

pub(crate) fn check_layout(dir: &Path) -> Result<Checked, Failure> {
    let layout = read_regular(&dir.join("oci-layout"))?;
    if layout != b"{\"imageLayoutVersion\":\"1.0.0\"}" {
        return Err(Failure::output(
            "DK-E4001",
            "oci-layout is not version 1.0.0",
        ));
    }
    let index = read_text(&dir.join("index.json"))?;
    let index_refs = digest_sizes(&index)
        .map_err(|_| Failure::output("DK-E4001", "image index does not name one manifest"))?;
    if index_refs.len() != 1 || !index.contains("\"architecture\":\"amd64\"") {
        return Err(Failure::output(
            "DK-E4001",
            "image index does not name one linux/amd64 manifest",
        ));
    }
    let manifest = read_blob(dir, &index_refs[0].0, index_refs[0].1)?;
    let manifest_text = std::str::from_utf8(&manifest)
        .map_err(|_| Failure::output("DK-E4001", "manifest is not utf-8"))?;
    let manifest_refs = digest_sizes(manifest_text)
        .map_err(|_| Failure::output("DK-E4001", "manifest descriptors are not intact"))?;
    if manifest_refs.len() != 2 {
        return Err(Failure::output(
            "DK-E4001",
            "manifest does not name one config and one layer",
        ));
    }
    require_closed_layout(
        dir,
        &[
            index_refs[0].0.as_str(),
            manifest_refs[0].0.as_str(),
            manifest_refs[1].0.as_str(),
        ],
    )?;
    let config = read_blob(dir, &manifest_refs[0].0, manifest_refs[0].1)?;
    let config_text = std::str::from_utf8(&config)
        .map_err(|_| Failure::output("DK-E4001", "image config is not utf-8"))?;
    if !config_text.contains(&format!("\"User\":\"{IMAGE_USER}\""))
        || !config_text.contains(CREATED)
    {
        return Err(Failure::output(
            "DK-E4001",
            "image config does not match the profile user and epoch",
        ));
    }
    let layer = read_blob(dir, &manifest_refs[1].0, manifest_refs[1].1)?;
    let entries = read_archive(&layer).map_err(|message| Failure::output("DK-E4001", message))?;
    if entries.len() != 3
        || entries[0].name != "srv/"
        || entries[1].name != "srv/dockyard/"
        || entries[2].name != "srv/dockyard/service"
        || entries
            .iter()
            .any(|entry| entry.uid != 65532 || entry.gid != 65532 || entry.mode != 0o555)
    {
        return Err(Failure::output(
            "DK-E4001",
            "layer is not the single non-root executable",
        ));
    }
    let published = read_regular(&dir.join("service"))?;
    if published != entries[2].data {
        return Err(Failure::output(
            "DK-E4001",
            "published executable differs from the image layer",
        ));
    }
    let evidence = read_text(&dir.join("evidence").join("report.txt"))?;
    if evidence.contains("/home/")
        || !evidence.contains("image-start: BLOCKED")
        || !evidence.contains("runtime-qualification: BLOCKED")
    {
        return Err(Failure::output(
            "DK-E4001",
            "build report does not match the layout",
        ));
    }
    let service_name = report_field(&evidence, "service")?;
    let source_sha = report_field(&evidence, "source-sha256")?;
    let executable_sha = report_field(&evidence, "executable-sha256")?;
    let layer_sha = report_field(&evidence, "layer-sha256")?;
    let config_sha = report_field(&evidence, "config-sha256")?;
    let manifest_sha = report_field(&evidence, "manifest-sha256")?;
    if label(config_text, "service").as_deref() != Some(service_name)
        || executable_sha != sha256_hex(&published)
        || layer_sha != sha256_hex(&layer)
        || config_sha != sha256_hex(&config)
        || manifest_sha != sha256_hex(&manifest)
    {
        return Err(Failure::output(
            "DK-E4001",
            "build report does not match the layout",
        ));
    }
    let sbom = read_text(&dir.join("evidence").join("sbom.spdx.json"))?;
    let expected_sbom = crate::sbom::spdx_json(service_name, source_sha, executable_sha, layer_sha);
    if sbom != expected_sbom || sbom.contains("/home/") {
        return Err(Failure::output(
            "DK-E4001",
            "sbom does not match the layout",
        ));
    }
    let provenance = read_text(&dir.join("evidence").join("provenance.json"))?;
    let expected_provenance = crate::provenance::predicate(
        source_sha,
        executable_sha,
        layer_sha,
        config_sha,
        manifest_sha,
    );
    if provenance != expected_provenance || provenance.contains("/home/") {
        return Err(Failure::output(
            "DK-E4001",
            "provenance does not match the layout",
        ));
    }
    let port = label(config_text, "port")
        .and_then(|text| text.parse::<u16>().ok())
        .filter(|port| (1024..=65535).contains(port))
        .ok_or_else(|| Failure::output("DK-E4001", "image config has no admitted port"))?;
    let path = label(config_text, "path")
        .filter(|path| path.starts_with('/'))
        .ok_or_else(|| Failure::output("DK-E4001", "image config has no admitted path"))?;
    let body_sha256 = label(config_text, "body-sha256")
        .filter(|digest| digest.len() == 64)
        .ok_or_else(|| Failure::output("DK-E4001", "image config has no body digest"))?;
    if label(config_text, "method").as_deref() != Some(METHOD)
        || label(config_text, "status").as_deref() != Some("200")
        || label(config_text, "content-type").as_deref() != Some(CONTENT_TYPE)
        || label(config_text, "profile").as_deref() != Some(PROFILE)
        || label(config_text, "qualification").as_deref() != Some("blocked")
    {
        return Err(Failure::output(
            "DK-E4001",
            "image labels do not match the admitted profile",
        ));
    }
    require_static_x86_64(&published).map_err(|message| Failure::output("DK-E4001", message))?;
    Ok(Checked {
        binary: published,
        port,
        path,
        body_sha256,
    })
}

fn exercise(checked: &Checked) -> Result<(), String> {
    let temp_name = format!(
        "dockyard-verify-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    );
    let temp_dir = std::env::temp_dir().join(temp_name);
    fs::create_dir(&temp_dir)
        .map_err(|error| format!("cannot create verify directory: {error}"))?;
    let cleanup = RemoveDir(temp_dir.clone());
    let binary = temp_dir.join("service");
    fs::write(&binary, &checked.binary)
        .map_err(|error| format!("cannot write verify executable: {error}"))?;
    set_mode(&binary, 0o755).map_err(|failure| failure.message())?;
    let child = Command::new(&binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start executable: {error}"))?;
    let reap = Reap(Some(child));
    let missing = exchange_when_up(checked.port, "/no-such")?;
    if missing.status != 404 || missing.content_type != CONTENT_TYPE || missing.body != b"not found"
    {
        return Err(format!(
            "unexpected response for an unknown path: {} {}",
            missing.status, missing.content_type
        ));
    }
    let found = exchange(checked.port, &checked.path)?;
    if found.status != 200 || found.content_type != CONTENT_TYPE {
        return Err(format!(
            "admitted route returned {} {}",
            found.status, found.content_type
        ));
    }
    if sha256_hex(&found.body) != checked.body_sha256 {
        return Err("response body does not match the image label".to_string());
    }
    let http10 = exchange_line(checked.port, &format!("GET {} HTTP/1.0", checked.path))?;
    if http10.status != 200
        || http10.content_type != CONTENT_TYPE
        || sha256_hex(&http10.body) != checked.body_sha256
    {
        return Err("HTTP/1.0 admitted route was not served".to_string());
    }
    for (label, line) in [
        ("POST", format!("POST {} HTTP/1.1", checked.path)),
        ("query", format!("GET {}?x HTTP/1.1", checked.path)),
        ("prefix", format!("GET {}x HTTP/1.1", checked.path)),
    ] {
        let rejected = exchange_line(checked.port, &line)?;
        if rejected.status != 404
            || rejected.content_type != CONTENT_TYPE
            || rejected.body != b"not found"
        {
            return Err(format!(
                "{label} request returned {} {}",
                rejected.status, rejected.content_type
            ));
        }
    }
    drop(reap);
    drop(cleanup);
    Ok(())
}

pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) content_type: String,
    pub(crate) body: Vec<u8>,
}

fn exchange(port: u16, path: &str) -> Result<Response, String> {
    exchange_line(port, &format!("GET {path} HTTP/1.1"))
}

fn exchange_line(port: u16, request_line: &str) -> Result<Response, String> {
    if request_line
        .bytes()
        .any(|byte| byte == b'\r' || byte == b'\n')
    {
        return Err("request line is not a single line".to_string());
    }
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|error| format!("cannot connect to 127.0.0.1:{port}: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("cannot set read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("cannot set write timeout: {error}"))?;
    let request = format!("{request_line}\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("cannot write request: {error}"))?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read response: {error}"))?;
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "response has no header terminator".to_string())?;
    let header = std::str::from_utf8(&bytes[..split])
        .map_err(|_| "response header is not utf-8".to_string())?;
    let body = bytes[split + 4..].to_vec();
    let status_line = header.lines().next().unwrap_or("");
    let mut status_parts = status_line.split(' ');
    if status_parts.next() != Some("HTTP/1.1") {
        return Err(format!(
            "response status line is not HTTP/1.1: {status_line}"
        ));
    }
    let status = status_parts
        .next()
        .unwrap_or("")
        .parse::<u16>()
        .map_err(|_| format!("response status line is not admitted: {status_line}"))?;
    if !header.lines().any(|line| line == "connection: close") {
        return Err("response did not close the connection".to_string());
    }
    let content_type = header
        .lines()
        .find_map(|line| line.strip_prefix("content-type: "))
        .unwrap_or("")
        .to_string();
    let length = header
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .unwrap_or("");
    if length != body.len().to_string() {
        return Err(format!(
            "content-length {length} does not match {} body bytes",
            body.len()
        ));
    }
    Ok(Response {
        status,
        content_type,
        body,
    })
}

pub(crate) fn exchange_when_up(port: u16, path: &str) -> Result<Response, String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut last = String::from("executable did not accept connections");
    while Instant::now() < deadline {
        match exchange(port, path) {
            Ok(response) => return Ok(response),
            Err(error) => {
                last = error;
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    Err(last)
}

struct Reap(Option<Child>);

impl Drop for Reap {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct RemoveDir(PathBuf);

impl Drop for RemoveDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn require_closed_layout(dir: &Path, blobs: &[&str]) -> Result<(), Failure> {
    require_real_dir(dir)?;
    require_names(
        dir,
        &["blobs", "evidence", "index.json", "oci-layout", "service"],
    )?;
    require_real_dir(&dir.join("blobs"))?;
    require_names(&dir.join("blobs"), &["sha256"])?;
    let blob_dir = dir.join("blobs").join("sha256");
    require_real_dir(&blob_dir)?;
    let mut expected_blobs = blobs.to_vec();
    expected_blobs.sort_unstable();
    expected_blobs.dedup();
    if expected_blobs.len() != 3 {
        return Err(Failure::output(
            "DK-E4001",
            "layout is not the closed profile set",
        ));
    }
    require_names(&blob_dir, &expected_blobs)?;
    require_real_dir(&dir.join("evidence"))?;
    require_names(
        &dir.join("evidence"),
        &["provenance.json", "report.txt", "sbom.spdx.json"],
    )
}

fn require_names(dir: &Path, expected: &[&str]) -> Result<(), Failure> {
    let found = entry_names(dir)?;
    if found.len() != expected.len() || found.iter().zip(expected).any(|(got, want)| got != want) {
        return Err(Failure::output(
            "DK-E4001",
            "layout is not the closed profile set",
        ));
    }
    Ok(())
}

fn require_real_dir(path: &Path) -> Result<(), Failure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| Failure::output("DK-E4001", "layout is not the closed profile set"))?;
    let kind = metadata.file_type();
    if kind.is_symlink() {
        return Err(Failure::output("DK-E4001", "layout contains a symlink"));
    }
    if !kind.is_dir() {
        return Err(Failure::output(
            "DK-E4001",
            "layout is not the closed profile set",
        ));
    }
    Ok(())
}

fn entry_names(dir: &Path) -> Result<Vec<String>, Failure> {
    let mut names = Vec::new();
    let entries = fs::read_dir(dir)
        .map_err(|_| Failure::output("DK-E4001", "layout is not the closed profile set"))?;
    for entry in entries {
        let entry = entry
            .map_err(|_| Failure::output("DK-E4001", "layout is not the closed profile set"))?;
        let file_type = entry
            .file_type()
            .map_err(|_| Failure::output("DK-E4001", "layout is not the closed profile set"))?;
        if file_type.is_symlink() {
            return Err(Failure::output("DK-E4001", "layout contains a symlink"));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Failure::output("DK-E4001", "layout file name is not utf-8"))?;
        names.push(name);
    }
    names.sort();
    Ok(names)
}

fn report_field<'a>(report: &'a str, key: &str) -> Result<&'a str, Failure> {
    let prefix = format!("{key}: ");
    report
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Failure::output("DK-E4001", format!("build report has no {key}")))
}

fn label(config: &str, key: &str) -> Option<String> {
    let pattern = format!("\"org.elci.goya.dockyard.{key}\":\"");
    let start = config.find(&pattern)? + pattern.len();
    let rest = config.get(start..)?;
    let end = rest.find('"')?;
    let value = &rest[..end];
    if value.bytes().any(|byte| byte == b'\\' || byte == b'"') {
        return None;
    }
    Some(value.to_string())
}

fn digest_sizes(json: &str) -> Result<Vec<(String, usize)>, ()> {
    let needle = "\"digest\":\"sha256:";
    let mut rest = json;
    let mut found = Vec::new();
    while let Some(offset) = rest.find(needle) {
        let hex_at = offset + needle.len();
        let hex = rest.get(hex_at..hex_at + 64).ok_or(())?;
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(());
        }
        let after = rest.get(hex_at + 64..).ok_or(())?;
        let size_needle = "\",\"size\":";
        if !after.starts_with(size_needle) {
            return Err(());
        }
        let digits = after[size_needle.len()..]
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            return Err(());
        }
        let size = digits.parse::<usize>().map_err(|_| ())?;
        found.push((hex.to_string(), size));
        rest = after.get(size_needle.len() + digits.len()..).ok_or(())?;
    }
    if found.is_empty() {
        return Err(());
    }
    Ok(found)
}

fn read_blob(dir: &Path, digest: &str, size: usize) -> Result<Vec<u8>, Failure> {
    if digest.len() != 64 || digest.bytes().any(|byte| !byte.is_ascii_hexdigit()) {
        return Err(Failure::output("DK-E4001", "blob digest is not sha256"));
    }
    let bytes = read_regular(&dir.join("blobs").join("sha256").join(digest))?;
    if bytes.len() != size || sha256_hex(&bytes) != digest {
        return Err(Failure::output(
            "DK-E4001",
            "blob bytes do not match the descriptor",
        ));
    }
    Ok(bytes)
}

fn read_regular(path: &Path) -> Result<Vec<u8>, Failure> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        Failure::output(
            "DK-E4001",
            format!("missing regular file {}", path.display()),
        )
    })?;
    if !metadata.file_type().is_file() {
        return Err(Failure::output(
            "DK-E4001",
            format!("{} is not a regular file", path.display()),
        ));
    }
    fs::read(path).map_err(|error| {
        Failure::output(
            "DK-E4001",
            format!("cannot read {}: {error}", path.display()),
        )
    })
}

fn read_text(path: &Path) -> Result<String, Failure> {
    let bytes = read_regular(path)?;
    String::from_utf8(bytes)
        .map_err(|_| Failure::output("DK-E4001", format!("{} is not utf-8", path.display())))
}

fn write_bytes(path: &Path, bytes: &[u8], mode: u32) -> Result<(), Failure> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(Failure::output(
                "DK-E3001",
                format!("parent is missing for {}", path.display()),
            ));
        }
    }
    fs::write(path, bytes).map_err(|error| {
        Failure::output(
            "DK-E3001",
            format!("cannot write {}: {error}", path.display()),
        )
    })?;
    set_mode(path, mode)
}

fn set_mode(path: &Path, mode: u32) -> Result<(), Failure> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
            Failure::output(
                "DK-E3001",
                format!("cannot set mode on {}: {error}", path.display()),
            )
        })?;
    }
    Ok(())
}

fn validate_new_dir(out_dir: &Path) -> Result<(), Failure> {
    let name = out_dir
        .file_name()
        .and_then(|text| text.to_str())
        .unwrap_or("");
    if name.is_empty() || name == "." || name == ".." || name.starts_with('.') {
        return Err(Failure::output(
            "DK-E1003",
            "output directory name is not admitted",
        ));
    }
    if out_dir.exists() {
        return Err(Failure::output("DK-E1002", "output already exists"));
    }
    let parent = parent_dir(out_dir)?;
    if !parent.is_dir() {
        return Err(Failure::output("DK-E1001", "output parent is missing"));
    }
    Ok(())
}

fn parent_dir(out_dir: &Path) -> Result<&Path, Failure> {
    match out_dir.parent() {
        Some(path) if path.as_os_str().is_empty() => Ok(Path::new(".")),
        Some(path) => Ok(path),
        None => Err(Failure::output(
            "DK-E1003",
            "output directory name is not admitted",
        )),
    }
}

pub(crate) fn require_static_x86_64(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.len() < 64 || &bytes[0..4] != b"\x7fELF" {
        return Err("executable is not an ELF file");
    }
    if bytes[4] != 2 || bytes[5] != 1 {
        return Err("executable is not little-endian ELF64");
    }
    let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
    if machine != 62 {
        return Err("executable is not x86-64");
    }
    let phoff = usize::try_from(u64::from_le_bytes(
        bytes[32..40]
            .try_into()
            .map_err(|_| "program header offset is truncated")?,
    ))
    .map_err(|_| "program header offset does not fit")?;
    let phentsize = usize::from(u16::from_le_bytes([bytes[54], bytes[55]]));
    let phnum = usize::from(u16::from_le_bytes([bytes[56], bytes[57]]));
    if phentsize < 56 || phnum == 0 {
        return Err("program headers are missing");
    }
    let table = phentsize
        .checked_mul(phnum)
        .and_then(|bytes| phoff.checked_add(bytes))
        .ok_or("program headers overflow")?;
    if table > bytes.len() {
        return Err("program headers overrun the file");
    }
    for index in 0..phnum {
        let start = phoff + index * phentsize;
        let kind = u32::from_le_bytes(
            bytes[start..start + 4]
                .try_into()
                .map_err(|_| "program header type is truncated")?,
        );
        if kind == 3 {
            return Err("executable has a dynamic interpreter");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elf_with(kind: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 64 + 56];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[18] = 62;
        bytes[32] = 64;
        bytes[54] = 56;
        bytes[56] = 1;
        bytes[64..68].copy_from_slice(&kind.to_le_bytes());
        bytes
    }

    #[test]
    fn dynamic_interpreter_is_rejected() {
        assert!(require_static_x86_64(&elf_with(3)).is_err());
        assert!(require_static_x86_64(&elf_with(1)).is_ok());
        assert!(require_static_x86_64(b"not-elf").is_err());
    }

    #[test]
    fn emitted_source_embeds_only_the_admitted_literals() {
        let service = crate::admit(include_str!("../fixtures/ping.gservice")).expect("fixture");
        let source = emit_source(&service);
        assert!(source.contains("const PORT: u16 = 39171;"));
        assert!(source.contains("const PATH: &[u8] = b\"/health\";"));
        assert!(source.contains("0x6f, 0x6b"));
        assert!(!source.contains("secret"));
        assert!(!source.contains("fn listen"));
    }
}
