//! Typed and capability-gated boundaries to Goya's companion systems.
//!
//! The adapters avoid embedding sibling checkout paths or invoking a shell.
//! RUGID scene export is in-process and loss-aware; 3form and Theosis use
//! their documented process protocols; Mesut is exposed through a typed host
//! trait because its supported API is an in-process Rust runtime, not a
//! daemon protocol.

use goya_lang::{CellKind, Scene, SceneCell};
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::future::Future;
use std::io::Write;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

pub const ECOSYSTEM_CONTRACT: &str = "GY-ECOSYSTEM/1";

/// Authority required before a companion adapter performs work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Export a Goya scene into RUGID's RDF interchange format.
    ExportScene,
    /// Invoke the 3form agent's formatting API.
    FormatOutput,
    /// Submit work through an embedding application's Mesut runtime.
    ScheduleWork,
    /// Query an installed application's remote version via Theosis.
    InspectUpdate,
    /// Allow Theosis to build and install an application update.
    InstallUpdate,
}

/// Explicit set of authorities granted to one integration call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilityGrant(BTreeSet<Capability>);

impl CapabilityGrant {
    pub fn new(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self(capabilities.into_iter().collect())
    }

    pub fn allows(&self, capability: Capability) -> bool {
        self.0.contains(&capability)
    }

    fn require(&self, capability: Capability) -> Result<(), IntegrationError> {
        if self.allows(capability) {
            Ok(())
        } else {
            Err(IntegrationError::CapabilityDenied(capability))
        }
    }
}

#[derive(Debug)]
pub enum IntegrationError {
    CapabilityDenied(Capability),
    InvalidInput(&'static str),
    Unavailable(PathBuf),
    Io(std::io::Error),
    Launch(std::io::Error),
    Failed { status: Option<i32>, stderr: String },
    InvalidReply(&'static str),
    Mesut(String),
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityDenied(capability) => write!(f, "capability denied: {capability:?}"),
            Self::InvalidInput(reason) => write!(f, "invalid integration input: {reason}"),
            Self::Unavailable(path) => {
                write!(f, "companion executable unavailable: {}", path.display())
            }
            Self::Io(error) => write!(f, "integration filesystem error: {error}"),
            Self::Launch(error) => write!(f, "could not start companion executable: {error}"),
            Self::Failed { status, stderr } => {
                write!(f, "companion failed ({status:?}): {}", stderr.trim())
            }
            Self::InvalidReply(reason) => write!(f, "invalid companion reply: {reason}"),
            Self::Mesut(message) => write!(f, "Mesut adapter failed: {message}"),
        }
    }
}

impl std::error::Error for IntegrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::Launch(error) => Some(error),
            _ => None,
        }
    }
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempWorkspace(PathBuf);

impl TempWorkspace {
    fn create() -> Result<Self, IntegrationError> {
        for _ in 0..32 {
            let serial = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("goya-rugid-{}-{serial}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(IntegrationError::Io(error)),
            }
        }
        Err(IntegrationError::InvalidInput(
            "could not allocate a unique RUGID workspace",
        ))
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Real headless RUGID renderer adapter. It invokes RUGID's `rdf_render`
/// binary through Cargo in a unique temporary working directory, because the
/// current RUGID binary reads `demo.rdf` and writes `rdf_output.svg` there.
#[derive(Debug, Clone)]
pub struct RugidRenderer {
    manifest: PathBuf,
    cargo: PathBuf,
}

impl RugidRenderer {
    pub fn new(manifest: impl Into<PathBuf>) -> Self {
        Self {
            manifest: manifest.into(),
            cargo: PathBuf::from("cargo"),
        }
    }

    pub fn with_cargo(mut self, cargo: impl Into<PathBuf>) -> Self {
        self.cargo = cargo.into();
        self
    }

    pub fn render(
        &self,
        scene: &Scene,
        width: f64,
        height: f64,
        grant: &CapabilityGrant,
    ) -> Result<String, IntegrationError> {
        let export = export_rugid(scene, width, height, grant)?;
        let workspace = TempWorkspace::create()?;
        fs::write(workspace.0.join("demo.rdf"), export.rdf).map_err(IntegrationError::Io)?;
        let output = Command::new(&self.cargo)
            .args(["run", "--quiet", "--no-default-features", "--manifest-path"])
            .arg(&self.manifest)
            .args(["--bin", "rdf_render"])
            .current_dir(&workspace.0)
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    IntegrationError::Unavailable(self.cargo.clone())
                } else {
                    IntegrationError::Launch(error)
                }
            })?;
        if !output.status.success() {
            return Err(IntegrationError::Failed {
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        let svg_path = workspace.0.join("rdf_output.svg");
        let metadata = fs::metadata(&svg_path).map_err(|_| {
            IntegrationError::InvalidReply("RUGID succeeded without producing rdf_output.svg")
        })?;
        if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
            return Err(IntegrationError::InvalidReply(
                "RUGID SVG output is missing, not a file, or exceeds 8 MiB",
            ));
        }
        fs::read_to_string(svg_path).map_err(IntegrationError::Io)
    }
}

/// RUGID RDF export result. Button actions are reported as omitted because
/// RUGID's RDF visual tree does not encode Goya runtime action identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RugidExport {
    pub rdf: String,
    pub omitted_semantics: Vec<String>,
}

/// Serialize the visual part of a Goya scene into RUGID's RDF grammar.
/// Geometry is normalized to each parent's extent. The operation performs no
/// file writes and refuses names/text that could escape RDF attribute syntax.
pub fn export_rugid(
    scene: &Scene,
    width: f64,
    height: f64,
    grant: &CapabilityGrant,
) -> Result<RugidExport, IntegrationError> {
    grant.require(Capability::ExportScene)?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(IntegrationError::InvalidInput(
            "viewport dimensions must be positive and finite",
        ));
    }
    let title = rdf_value(&scene.name)?;
    let mut rdf = format!(
        "window:::title[{title}]::sdims[{},{}]\n",
        number(width),
        number(height)
    );
    let mut omitted_semantics = Vec::new();
    export_cell(
        &scene.root,
        "window",
        width,
        height,
        1,
        &mut rdf,
        &mut omitted_semantics,
    )?;
    Ok(RugidExport {
        rdf,
        omitted_semantics,
    })
}

fn export_cell(
    cell: &SceneCell,
    parent_id: &str,
    parent_w: f64,
    parent_h: f64,
    depth: usize,
    out: &mut String,
    omitted: &mut Vec<String>,
) -> Result<(), IntegrationError> {
    let id = rdf_identifier(&cell.name)?;
    let (x, y) = cell
        .position_p
        .map(|p| (p.x, p.y))
        .or_else(|| cell.position.map(|p| (p.x / parent_w, p.y / parent_h)))
        .unwrap_or((0.0, 0.0));
    let (w, h) = cell
        .extent_p
        .map(|e| (e.x, e.y))
        .or_else(|| cell.extent.map(|e| (e.x / parent_w, e.y / parent_h)))
        .unwrap_or((1.0, 1.0));
    if [x, y, w, h]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0)
    {
        return Err(IntegrationError::InvalidInput(
            "RUGID normalized geometry must be finite and within 0..=1",
        ));
    }
    let indent = "    ".repeat(depth);
    let element = if cell.kind == CellKind::Button {
        "widget"
    } else {
        "pane"
    };
    out.push_str(&format!(
        "{indent}{element}:::id[{id}]::parent[{parent_id}]::origin[{},{}]::extent[{},{}]",
        number(x),
        number(y),
        number(w),
        number(h)
    ));
    if let Some(fill) = cell.fill {
        out.push_str(&format!("::fill[#{fill:06x}]"));
    }
    out.push('\n');
    if let Some(text) = &cell.text {
        let text = rdf_value(text)?;
        out.push_str(&format!("{indent}    text:::parent[{id}]::value[{text}]"));
        if let Some(size) = cell.text_size {
            let cell_height = parent_h * h;
            let ratio = size / cell_height;
            if !size.is_finite() || cell_height <= 0.0 || !ratio.is_finite() {
                return Err(IntegrationError::InvalidInput(
                    "text size must be finite and its cell height positive",
                ));
            }
            out.push_str(&format!("::size[{}]", number(ratio)));
        }
        if let Some(color) = cell.fill {
            out.push_str(&format!("::color[#{color:06x}]"));
        }
        if let Some(anchor) = cell.anchor {
            let align = match anchor {
                goya_lang::Anchor::Start => "left",
                goya_lang::Anchor::Middle => "center",
                goya_lang::Anchor::End => "right",
            };
            out.push_str(&format!("::align[{align}]"));
        }
        out.push('\n');
    }
    if cell.kind == CellKind::Button {
        omitted.push(format!("button action on {}", cell.name));
    }
    if cell.stroke.is_some() || cell.stroke_width.is_some() {
        omitted.push(format!("stroke styling on {}", cell.name));
    }
    let child_parent_w = parent_w * w;
    let child_parent_h = parent_h * h;
    for child in &cell.children {
        export_cell(
            child,
            &cell.name,
            child_parent_w,
            child_parent_h,
            depth + 1,
            out,
            omitted,
        )?;
    }
    Ok(())
}

fn rdf_identifier(value: &str) -> Result<&str, IntegrationError> {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(value)
    } else {
        Err(IntegrationError::InvalidInput(
            "RUGID ids must contain only ASCII letters, digits, '_' or '-'",
        ))
    }
}

fn rdf_value(value: &str) -> Result<&str, IntegrationError> {
    if value.contains(['[', ']', '\n', '\r']) {
        Err(IntegrationError::InvalidInput(
            "RDF values cannot contain brackets or line breaks",
        ))
    } else {
        Ok(value)
    }
}

fn number(value: f64) -> String {
    format!("{value:.6}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

/// One bounded 3form-agent JSON-RPC request. The adapter starts a configured
/// executable directly and never interprets it through a shell.
#[derive(Debug, Clone)]
pub struct Form3Agent {
    executable: PathBuf,
    max_reply_bytes: usize,
}

impl Form3Agent {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            max_reply_bytes: 1024 * 1024,
        }
    }

    pub fn request(
        &self,
        json_line: &str,
        grant: &CapabilityGrant,
    ) -> Result<String, IntegrationError> {
        grant.require(Capability::FormatOutput)?;
        if json_line.trim().is_empty() || json_line.contains(['\n', '\r']) {
            return Err(IntegrationError::InvalidInput(
                "form3 request must be one non-empty JSON line",
            ));
        }
        if json_line.len() > 64 * 1024
            || !json_line.trim_start().starts_with('{')
            || !json_line.trim_end().ends_with('}')
        {
            return Err(IntegrationError::InvalidInput(
                "form3 request must be a JSON object no larger than 64 KiB",
            ));
        }
        let mut child = Command::new(&self.executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    IntegrationError::Unavailable(self.executable.clone())
                } else {
                    IntegrationError::Launch(e)
                }
            })?;
        let write_error = child
            .stdin
            .take()
            .ok_or(IntegrationError::InvalidReply("stdin unavailable"))
            .and_then(|mut stdin| {
                stdin
                    .write_all(format!("{json_line}\n").as_bytes())
                    .map_err(IntegrationError::Launch)
            })
            .err();
        let output = child.wait_with_output().map_err(IntegrationError::Launch)?;
        // A companion that rejects the request must surface its exit status
        // even when the write also failed, usually with EPIPE.
        if !output.status.success() {
            return Err(IntegrationError::Failed {
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        if let Some(error) = write_error {
            return Err(error);
        }
        if output.stdout.len() > self.max_reply_bytes {
            return Err(IntegrationError::InvalidReply("reply exceeds 1 MiB"));
        }
        let reply = String::from_utf8(output.stdout)
            .map_err(|_| IntegrationError::InvalidReply("reply is not UTF-8"))?;
        let reply = reply.trim_end_matches(['\n', '\r']);
        if reply.is_empty() || reply.contains(['\n', '\r']) {
            return Err(IntegrationError::InvalidReply(
                "expected exactly one non-empty JSON line",
            ));
        }
        Ok(reply.to_owned())
    }
}

/// Typed description mapped by an embedding to Mesut's in-process `Work`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MesutTask {
    pub id: String,
    pub label: String,
    pub kind: WorkKind,
    pub budget_ms: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    Io,
    Compute,
    Blocking,
}

/// Host bridge implemented using `MesuT`/`Work` in the application that owns
/// the Mesut runtime. It intentionally carries explicit budget parameters
/// rather than accepting arbitrary closures from Goya.
pub trait MesutBridge {
    type Handle;
    fn submit<'a>(
        &'a self,
        task: MesutTask,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Handle, IntegrationError>> + Send + 'a>>;
}

pub async fn submit_mesut<B: MesutBridge>(
    bridge: &B,
    task: MesutTask,
    grant: &CapabilityGrant,
) -> Result<B::Handle, IntegrationError> {
    grant.require(Capability::ScheduleWork)?;
    validate_mesut_task(&task)?;
    bridge.submit(task).await
}

fn validate_mesut_task(task: &MesutTask) -> Result<(), IntegrationError> {
    if task.id.is_empty()
        || task.id.len() > 128
        || task.label.len() > 256
        || task.id.contains(['\0', '\n', '\r'])
        || task.label.contains(['\0', '\n', '\r'])
        || task.payload.len() > 16 * 1024 * 1024
        || task.budget_ms == 0
        || task.budget_ms > 24 * 60 * 60 * 1000
    {
        return Err(IntegrationError::InvalidInput(
            "Mesut task identity, label, or budget is outside bounds",
        ));
    }
    Ok(())
}

/// Safe CLI adapter for Theosis. Read-only version inspection and mutating
/// update/install are separately capability-gated.
#[derive(Debug, Clone)]
pub struct TheosisClient {
    executable: PathBuf,
}

impl TheosisClient {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    pub fn check(
        &self,
        remote: impl AsRef<OsStr>,
        binary: &Path,
        grant: &CapabilityGrant,
    ) -> Result<String, IntegrationError> {
        grant.require(Capability::InspectUpdate)?;
        self.invoke([
            OsStr::new("check"),
            OsStr::new("--remote"),
            remote.as_ref(),
            OsStr::new("--binary"),
            binary.as_os_str(),
            OsStr::new("--json"),
        ])
    }

    pub fn update(
        &self,
        remote: impl AsRef<OsStr>,
        reference: &str,
        binary: &Path,
        install_dir: &Path,
        grant: &CapabilityGrant,
    ) -> Result<String, IntegrationError> {
        grant.require(Capability::InstallUpdate)?;
        if reference.is_empty() {
            return Err(IntegrationError::InvalidInput("Theosis ref is empty"));
        }
        self.invoke([
            OsStr::new("update"),
            OsStr::new("--remote"),
            remote.as_ref(),
            OsStr::new("--ref"),
            OsStr::new(reference),
            OsStr::new("--binary"),
            binary.as_os_str(),
            OsStr::new("--install-dir"),
            install_dir.as_os_str(),
            OsStr::new("--json"),
        ])
    }

    fn invoke(
        &self,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Result<String, IntegrationError> {
        let output = Command::new(&self.executable)
            .args(args)
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    IntegrationError::Unavailable(self.executable.clone())
                } else {
                    IntegrationError::Launch(e)
                }
            })?;
        if !output.status.success() {
            return Err(IntegrationError::Failed {
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        if output.stdout.len() > THEOSIS_MAX_REPLY_BYTES {
            return Err(IntegrationError::InvalidReply("reply exceeds 1 MiB"));
        }
        String::from_utf8(output.stdout)
            .map_err(|_| IntegrationError::InvalidReply("reply is not UTF-8"))
    }
}

const THEOSIS_MAX_REPLY_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use goya_lang::parse;

    fn grant(capability: Capability) -> CapabilityGrant {
        CapabilityGrant::new([capability])
    }

    #[test]
    fn scene_export_preserves_tree_geometry_and_reports_action_loss() {
        let scene = parse("@ontology demo { @cell root { extent: (100, 100, 0); @button go { position: (10, 20, 0); extent: (30, 40, 0); text: \"Go\" } } }").unwrap();
        let export = export_rugid(&scene, 100.0, 100.0, &grant(Capability::ExportScene)).unwrap();
        assert!(export.rdf.contains("window:::title[demo]"));
        assert!(export
            .rdf
            .contains("widget:::id[go]::parent[root]::origin[0.1,0.2]::extent[0.3,0.4]"));
        assert_eq!(export.omitted_semantics, vec!["button action on go"]);
    }

    #[test]
    fn companion_calls_fail_closed_without_capability() {
        let scene = parse("@ontology demo { @cell root {} }").unwrap();
        assert!(matches!(
            export_rugid(&scene, 10.0, 10.0, &CapabilityGrant::default()),
            Err(IntegrationError::CapabilityDenied(Capability::ExportScene))
        ));
    }

    #[test]
    fn rdf_injection_characters_are_rejected() {
        let scene =
            parse("@ontology demo { @cell root { text: \"bad]\\nwindow:::title[escape]\" } }")
                .unwrap();
        assert!(matches!(
            export_rugid(&scene, 10.0, 10.0, &grant(Capability::ExportScene)),
            Err(IntegrationError::InvalidInput(_))
        ));
    }

    #[test]
    fn mesut_task_budget_is_bounded_before_dispatch() {
        let task = MesutTask {
            id: "x".into(),
            label: "work".into(),
            kind: WorkKind::Compute,
            budget_ms: 0,
            payload: vec![],
        };
        assert!(matches!(
            validate_mesut_task(&task),
            Err(IntegrationError::InvalidInput(_))
        ));
    }

    #[test]
    fn process_adapters_require_distinct_authorities() {
        let denied = CapabilityGrant::default();
        assert!(matches!(
            Form3Agent::new("missing-3form-agent").request("{}", &denied),
            Err(IntegrationError::CapabilityDenied(Capability::FormatOutput))
        ));
        assert!(matches!(
            TheosisClient::new("missing-theosis").check("remote", Path::new("app"), &denied),
            Err(IntegrationError::CapabilityDenied(
                Capability::InspectUpdate
            ))
        ));
        assert!(matches!(
            TheosisClient::new("missing-theosis").update(
                "remote",
                "main",
                Path::new("app"),
                Path::new("bin"),
                &grant(Capability::InspectUpdate)
            ),
            Err(IntegrationError::CapabilityDenied(
                Capability::InstallUpdate
            ))
        ));
    }

    #[test]
    fn configured_but_unavailable_form3_tool_is_reported() {
        let error = Form3Agent::new("missing-3form-agent")
            .request("{}", &grant(Capability::FormatOutput))
            .unwrap_err();
        assert!(matches!(error, IntegrationError::Unavailable(_)));
    }

    #[test]
    fn rugid_text_size_ratio_must_stay_finite() {
        let cell = SceneCell {
            name: "label".into(),
            kind: CellKind::Cell,
            is_input: false,
            position: None,
            extent: Some(goya_lang::Vec3 {
                x: 10.0,
                y: 1e-320,
                z: 0.0,
            }),
            position_p: None,
            extent_p: None,
            fill: None,
            stroke: None,
            stroke_width: None,
            text: Some("Hi".into()),
            text_size: Some(12.0),
            anchor: None,
            wrap: None,
            scroll: false,
            children: Vec::new(),
        };
        let mut rdf = String::new();
        let error = export_cell(&cell, "window", 100.0, 100.0, 1, &mut rdf, &mut Vec::new());
        assert!(matches!(error, Err(IntegrationError::InvalidInput(_))));
        assert!(!rdf.contains("inf"));
    }

    #[cfg(unix)]
    #[test]
    fn form3_nonzero_exit_wins_over_a_closed_stdin() {
        let dir = std::env::temp_dir().join(format!(
            "goya-form3-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let ready = dir.join("ready");
        let release = dir.join("release");
        unix_mkfifo(&ready);
        unix_mkfifo(&release);
        let tool = dir.join("form3");
        fs::write(
            &tool,
            format!(
                "#!/bin/sh\nexec 0<&-\nprintf x > '{}'\nread _ < '{}'\nexit 3\n",
                ready.display(),
                release.display()
            ),
        )
        .unwrap();
        unix_chmod(&tool);
        let agent = Form3Agent::new(&tool);
        let grant = grant(Capability::FormatOutput);
        let worker = std::thread::spawn(move || agent.request("{}", &grant));
        std::io::Read::read_exact(&mut std::fs::File::open(&ready).unwrap(), &mut [0]).unwrap();
        std::fs::write(&release, b"go\n").unwrap();
        let error = worker.join().unwrap().unwrap_err();
        assert!(matches!(
            error,
            IntegrationError::Failed {
                status: Some(3),
                ..
            }
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn theosis_preserves_non_utf8_paths_and_rejects_invalid_replies() {
        use std::os::unix::ffi::OsStringExt;
        let dir = std::env::temp_dir().join(format!(
            "goya-theosis-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let argfile = dir.join("args");
        let tool = dir.join("theosis");
        fs::write(
            &tool,
            format!(
                "#!/usr/bin/env python3\nimport os, sys\nopen({:?}, 'wb').write(os.fsencode(sys.argv[5]))\nsys.stdout.write('{{}}\\n')\n",
                argfile
            ),
        )
        .unwrap();
        unix_chmod(&tool);
        let binary = dir.join(std::ffi::OsString::from_vec(b"app-\xff".to_vec()));
        fs::write(&binary, b"bin").unwrap();
        let reply = TheosisClient::new(&tool)
            .check("origin", &binary, &grant(Capability::InspectUpdate))
            .unwrap();
        assert_eq!(reply, "{}");
        assert_eq!(fs::read(&argfile).unwrap(), b"app-\xff");

        fs::write(&tool, "#!/bin/sh\nprintf '\\xff'\n").unwrap();
        unix_chmod(&tool);
        let error = TheosisClient::new(&tool)
            .check("origin", Path::new("app"), &grant(Capability::InspectUpdate))
            .unwrap_err();
        assert!(matches!(error, IntegrationError::InvalidReply(_)));

        fs::write(
            &tool,
            "#!/bin/sh\nhead -c 1048577 /dev/zero | tr '\\0' a\n",
        )
        .unwrap();
        unix_chmod(&tool);
        let error = TheosisClient::new(&tool)
            .check("origin", Path::new("app"), &grant(Capability::InspectUpdate))
            .unwrap_err();
        assert!(matches!(error, IntegrationError::InvalidReply(_)));
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    fn unix_mkfifo(path: &Path) {
        let _ = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .unwrap();
    }

    #[cfg(unix)]
    fn unix_chmod(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions).unwrap();
    }
}
