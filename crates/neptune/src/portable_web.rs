//! Experimental Neptune lowering through Arachnid's shared browser assets.
//! The compiler is an explicitly supplied, trusted executable. Its output
//! manifest binds copied assets to the original Goya source revision.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    net::Ipv4Addr,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const REQUIRED: [&str; 5] = [
    "index.html",
    "app.css",
    "app.js",
    "arachnid-splash.png",
    "icon.png",
];
const LANLINK: &str = "lanlink.js";
const APP_ID: &str = "com.goya.neptune.portable";
const APP_VERSION: &str = "0.0.1";
const ASSET_LIMIT: u64 = 8 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 64 * 1024;
const DIAGNOSTIC_LIMIT: u64 = 16 * 1024;
const BUILD_TIMEOUT: Duration = Duration::from_secs(30);

/// Emit an unchanged Goya source through Arachnid's browser engine. The
/// parent directory must exist, and `output` must be a new directory. On
/// success it contains the verified browser assets and no webOS package.
pub fn compile(source: &Path, compiler: &Path, output: &Path) -> Result<Value, String> {
    let source_bytes = read_bounded(source, crate::MAX_SOURCE_BYTES as u64)
        .map_err(|error| format!("scene {}: {error}", source.display()))?;
    std::str::from_utf8(&source_bytes).map_err(|error| format!("scene is not UTF-8: {error}"))?;
    let compiler = compiler
        .canonicalize()
        .map_err(|error| format!("Arachnid compiler {}: {error}", compiler.display()))?;
    let compiler_hash =
        binary_hash(&compiler).map_err(|error| format!("Arachnid compiler: {error}"))?;

    fs::create_dir(output)
        .map_err(|error| format!("create Web output {}: {error}", output.display()))?;
    let result = compile_into(source, &source_bytes, &compiler, &compiler_hash, output);
    if result.is_err() {
        let _ = fs::remove_dir_all(output);
    }
    result
}

fn compile_into(
    source: &Path,
    source_bytes: &[u8],
    compiler: &Path,
    compiler_hash: &str,
    output: &Path,
) -> Result<Value, String> {
    let snapshot = output.join(".scene.gscene");
    let staging = output.join(".arachnid-build");
    let stderr = output.join(".arachnid-stderr");
    write_new(&snapshot, source_bytes).map_err(|error| format!("snapshot Goya source: {error}"))?;
    if let Err(error) = run_compiler(compiler, &snapshot, &staging, &stderr) {
        return Err(error.replace(
            &snapshot.to_string_lossy().into_owned(),
            &source.to_string_lossy(),
        ));
    }
    if binary_hash(compiler).map_err(|error| format!("Arachnid compiler: {error}"))?
        != compiler_hash
    {
        return Err("Arachnid compiler changed during the build".into());
    }
    let (metadata, assets) = checked_assets(&staging, &hash(source_bytes))
        .map_err(|error| format!("Arachnid output: {error}"))?;
    let lan_endpoint = assets
        .get(LANLINK)
        .map(|bytes| runtime_endpoint(bytes))
        .transpose()?;
    let html = std::str::from_utf8(&assets["index.html"])
        .map_err(|error| format!("Arachnid index.html is not UTF-8: {error}"))?;
    if html.contains("<script src=\"lanlink.js\"></script>") != lan_endpoint.is_some() {
        return Err("Arachnid index.html and lanlink.js disagree".into());
    }
    let mut asset_sha256 = BTreeMap::new();
    for (name, bytes) in assets {
        write_new(&output.join(name), &bytes)
            .map_err(|error| format!("write Web asset {name}: {error}"))?;
        asset_sha256.insert(name, hash(&bytes));
    }
    fs::remove_dir_all(staging).map_err(|error| format!("remove Arachnid staging: {error}"))?;
    fs::remove_file(snapshot).map_err(|error| format!("remove source snapshot: {error}"))?;
    fs::remove_file(stderr).map_err(|error| format!("remove compiler log: {error}"))?;

    Ok(json!({
        "schema_version": "1",
        "profile": "neptune.ios.portable-web/0.0.1",
        "source_sha256": metadata["source_sha256"],
        "backend": {
            "compiler": metadata["compiler"],
            "sha256": compiler_hash,
            "contract": metadata["contract"],
            "engine_baseline": "webos:22",
            "ontology_contract": metadata["ontology_contract"],
        },
        "canvas": [1920, 1080],
        "lan_endpoint": lan_endpoint,
        "asset_sha256": asset_sha256,
        "stages": {
            "web_lowering": "PASS",
            "apple_validation": "NOT_RUN",
            "device_install": "NOT_RUN"
        }
    }))
}

fn run_compiler(
    compiler: &Path,
    source: &Path,
    staging: &Path,
    stderr: &Path,
) -> Result<(), String> {
    if source.to_str().is_none() || staging.to_str().is_none() {
        return Err("Arachnid requires UTF-8 staging paths".into());
    }
    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stderr)
        .map_err(|error| format!("create Arachnid log: {error}"))?;
    let mut child = Command::new(compiler)
        .arg("build")
        .arg(source)
        .args([
            "--id",
            APP_ID,
            "--version",
            APP_VERSION,
            "--title",
            "Neptune",
            "--target",
            "webos:22",
            "--out",
        ])
        .arg(staging)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .map_err(|error| format!("start Arachnid: {error}"))?;
    let deadline = Instant::now() + BUILD_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => break Err("Arachnid exceeded the 30 second build deadline".into()),
            Err(error) => break Err(format!("wait for Arachnid: {error}")),
        }
    };
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => {
            let mut bytes = Vec::new();
            File::open(stderr)
                .and_then(|file| file.take(DIAGNOSTIC_LIMIT + 1).read_to_end(&mut bytes))
                .map_err(|error| format!("Arachnid failed ({status}); read log: {error}"))?;
            let truncated = bytes.len() as u64 > DIAGNOSTIC_LIMIT;
            bytes.truncate(DIAGNOSTIC_LIMIT as usize);
            let diagnostics = String::from_utf8_lossy(&bytes);
            let suffix = if truncated { " [truncated]" } else { "" };
            Err(format!(
                "Arachnid failed ({status}): {}{suffix}",
                diagnostics.trim_end()
            ))
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

type Assets = BTreeMap<&'static str, Vec<u8>>;

fn checked_assets(staging: &Path, source_hash: &str) -> io::Result<(Value, Assets)> {
    let app = staging.join("app");
    let reports = staging.join("reports");
    for dir in [staging, &app, &reports] {
        if !fs::symlink_metadata(dir)?.file_type().is_dir() {
            return Err(io::Error::other(
                "expected real Arachnid output directories",
            ));
        }
    }
    let manifest: Value = serde_json::from_slice(&read_bounded(
        &reports.join("compilation-manifest.json"),
        MANIFEST_LIMIT,
    )?)?;
    if manifest["contract"] != "arachnid.webos/0.0.1"
        || manifest["ontology_contract"] != "arachnid.ontology/0.0.1"
        || manifest["ontology_limit_pt"] != json!([1920, 1080])
        || manifest["source_sha256"] != source_hash
        || manifest["target_platform"] != "22"
        || manifest["app_id"] != APP_ID
        || manifest["app_version"] != APP_VERSION
        || manifest["compiler"].as_str().is_none()
    {
        return Err(io::Error::other(
            "compiler manifest does not match the requested build",
        ));
    }
    let hashes = manifest["files"]
        .as_object()
        .ok_or_else(|| io::Error::other("missing Arachnid asset digest map"))?;
    if hashes
        .keys()
        .any(|name| !REQUIRED.contains(&name.as_str()) && name != LANLINK && name != "appinfo.json")
    {
        return Err(io::Error::other("unrecognized asset in Arachnid output"));
    }
    let mut assets = BTreeMap::new();
    for name in REQUIRED
        .into_iter()
        .chain(hashes.contains_key(LANLINK).then_some(LANLINK))
    {
        let bytes = read_bounded(&app.join(name), ASSET_LIMIT)?;
        let digest = hash(&bytes);
        if bytes.is_empty() || hashes.get(name).and_then(Value::as_str) != Some(digest.as_str()) {
            return Err(io::Error::other(format!(
                "Arachnid asset digest mismatch: {name}"
            )));
        }
        assets.insert(name, bytes);
    }
    Ok((manifest, assets))
}

fn runtime_endpoint(bytes: &[u8]) -> Result<String, String> {
    let script =
        std::str::from_utf8(bytes).map_err(|error| format!("lanlink.js is not UTF-8: {error}"))?;
    let mut configs = script
        .lines()
        .filter_map(|line| line.trim().strip_prefix("var CONFIG = {name:"));
    let config = configs
        .next()
        .ok_or("lanlink.js has no recognized compiler configuration")?;
    if configs.next().is_some() {
        return Err("lanlink.js contains multiple compiler configurations".into());
    }
    let mut name = serde_json::Deserializer::from_str(config).into_iter::<String>();
    name.next()
        .and_then(Result::ok)
        .ok_or("invalid lanlink name")?;
    let config = config[name.byte_offset()..]
        .strip_prefix(",endpoint:")
        .ok_or("lanlink.js has no configured endpoint")?;
    let mut endpoint = serde_json::Deserializer::from_str(config).into_iter::<String>();
    let origin = endpoint
        .next()
        .and_then(Result::ok)
        .ok_or("invalid lanlink endpoint string")?;
    if !config[endpoint.byte_offset()..].starts_with(",roles:{") {
        return Err("unrecognized lanlink endpoint syntax".into());
    }
    let authority = origin
        .strip_prefix("http://")
        .ok_or("lanlink endpoint must use http://")?;
    let (host, port) = authority
        .split_once(':')
        .ok_or("lanlink endpoint must use an explicit port")?;
    let ip: Ipv4Addr = host
        .parse()
        .map_err(|_| "lanlink endpoint must use literal IPv4")?;
    let port: u16 = port.parse().map_err(|_| "invalid lanlink port")?;
    if port == 0
        || origin != format!("http://{ip}:{port}")
        || !(ip.is_private() || ip.is_link_local() || ip.is_loopback())
    {
        return Err("lanlink endpoint must be a canonical http://IPv4:port local origin".into());
    }
    Ok(origin)
}

fn read_bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(io::Error::other(format!(
            "{} must be a regular file no larger than {limit} bytes",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other(format!(
            "{} exceeds {limit} bytes",
            path.display()
        )));
    }
    Ok(bytes)
}

fn binary_hash(path: &Path) -> io::Result<String> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::other("expected a regular executable file"));
    }
    let mut digest = Sha256::new();
    io::copy(&mut File::open(path)?, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
