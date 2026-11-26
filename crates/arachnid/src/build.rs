//! The Arachnid pipeline: admission → caprichio gate → emission →
//! packaging → reports, and writing the result to a new directory.

use crate::appinfo::{self, json_string, AppInfo};
use crate::diagnostics::{Code, Diagnostic};
use crate::emit;
use crate::ipk::{self, Ipk};
use crate::ontology::{self, Ontology, ONTOLOGY_HEIGHT_PT, ONTOLOGY_WIDTH_PT};
use crate::target::Target;
use goya_caprichios::webos_engine_caprichio::{self as caps, WebFeature};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::Path;

pub struct BuildOptions {
    pub app: AppInfo,
    pub target: Target,
    /// Optional 80×80 PNG; a default icon is generated otherwise.
    pub icon: Option<Vec<u8>>,
}

pub struct BuildOutput {
    /// App directory files, sorted by path, `appinfo.json` included.
    pub app_files: Vec<(String, Vec<u8>)>,
    pub ipk: Ipk,
    /// `reports/<name>` files, sorted by name.
    pub reports: Vec<(String, Vec<u8>)>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Refuse a target whose engine lacks a feature the emitter depends on.
pub fn feature_gate(target: &Target, required: &[WebFeature]) -> Result<(), Vec<Diagnostic>> {
    let missing: Vec<Diagnostic> = required
        .iter()
        .filter(|f| !target.platform.supports(**f))
        .map(|f| {
            Diagnostic::new(
                Code::FeatureUnsupported,
                format!(
                    "{} needs Chromium {}; {} ships Chromium {}",
                    f.name(),
                    f.min_chromium(),
                    target.platform,
                    target.platform.engine().chromium_major
                ),
            )
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

/// Minimum Chromium the scene's platform services need (0 when none).
fn min_chromium(o: &Ontology) -> u16 {
    if o.parsed.declarations.links.is_empty() {
        0
    } else {
        crate::lanlink::MIN_CHROMIUM
    }
}

/// Admit, check platform services and gate, without emitting anything.
pub fn check(source: &str, target: &Target) -> Result<Ontology, Vec<Diagnostic>> {
    let o = ontology::admit(source)?;
    let services = crate::lanlink::check(&o.parsed);
    if !services.is_empty() {
        return Err(services);
    }
    feature_gate(target, emit::REQUIRED_FEATURES)?;
    let have = target.platform.engine().chromium_major;
    let need = min_chromium(&o);
    if have < need {
        return Err(vec![Diagnostic::new(
            Code::FeatureUnsupported,
            format!(
                "{} needs Chromium {need}; {} ships Chromium {have}",
                crate::lanlink::PROTOCOL,
                target.platform
            ),
        )]);
    }
    Ok(o)
}

fn compatibility_report(target: &Target, min: u16) -> String {
    let p = target.platform;
    let supported: Vec<String> = caps::WebosPlatform::ALL
        .iter()
        .filter(|q| emit::REQUIRED_FEATURES.iter().all(|f| q.supports(*f)))
        .filter(|q| q.engine().chromium_major >= min)
        .map(|q| json_string(q.id()))
        .collect();
    let features: Vec<String> = emit::REQUIRED_FEATURES
        .iter()
        .map(|f| json_string(f.name()))
        .collect();
    let codes = |h: crate::remote::Handling| {
        crate::remote::KEYS
            .iter()
            .filter(|k| k.handling == h)
            .map(|k| k.code.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let input = format!(
        "{{\"remote_contract\": \"{}\", \"five_way\": \"required\", \"pointer\": \"optional\", \"navigate_keys\": [{}], \"activate_keys\": [{}], \"platform_keys\": [{}]}}",
        crate::remote::CONTRACT,
        codes(crate::remote::Handling::Navigate),
        codes(crate::remote::Handling::Activate),
        codes(crate::remote::Handling::Platform)
    );
    format!(
        "{{\n  \"engine_caprichio\": \"{}\",\n  \"series_caprichio\": {},\n  \"requested\": {},\n  \"resolved\": {},\n  \"platform\": {},\n  \"chromium\": {},\n  \"required_features\": [{}],\n  \"runs_on_platforms\": [{}],\n  \"input\": {}\n}}\n",
        caps::CONTRACT,
        target.series_contract.map_or("null".into(), json_string),
        json_string(&target.requested),
        json_string(&target.description),
        json_string(p.id()),
        p.engine().chromium_major,
        features.join(", "),
        supported.join(", "),
        input
    )
}

pub fn build(source: &str, opts: &BuildOptions) -> Result<BuildOutput, Vec<Diagnostic>> {
    opts.app.validate()?;
    let icon = match &opts.icon {
        Some(bytes) => {
            if crate::archive::png_dimensions(bytes) != Some((80, 80)) {
                return Err(vec![Diagnostic::new(
                    Code::InvalidIcon,
                    "icon must be an 80×80 PNG",
                )]);
            }
            bytes.clone()
        }
        None => emit::default_icon(),
    };
    let o = check(source, &opts.target)?;
    let link_runtime = crate::lanlink::runtime_js(&o.parsed);
    let service_script = link_runtime.as_ref().map(|_| crate::lanlink::FILE);

    let mut app_files = vec![
        ("app.css".to_string(), emit::app_css().into_bytes()),
        ("app.js".to_string(), emit::app_js(&o).into_bytes()),
        ("appinfo.json".to_string(), opts.app.to_json().into_bytes()),
        (appinfo::ICON.to_string(), icon),
        (
            appinfo::MAIN.to_string(),
            emit::index_html(&o, &opts.app, service_script).into_bytes(),
        ),
    ];
    // Enforced: every build carries the Arachnid splash (splash.rs).
    app_files.push((crate::splash::FILE.to_string(), crate::splash::LOGO.to_vec()));
    if let Some(js) = link_runtime {
        app_files.push((crate::lanlink::FILE.to_string(), js.into_bytes()));
    }
    app_files.sort_by(|a, b| a.0.cmp(&b.0));
    let ipk = ipk::package(&opts.app, &app_files)
        .map_err(|e| vec![Diagnostic::new(Code::OutputFailed, e.to_string())])?;

    let file_hashes: Vec<String> = app_files
        .iter()
        .map(|(p, b)| format!("    {}: \"{}\"", json_string(p), sha256(b)))
        .collect();
    let manifest = format!(
        "{{\n  \"contract\": \"{}\",\n  \"compiler\": \"goya-arachnid {}\",\n  \"ontology_contract\": \"{}\",\n  \"ontology_limit_pt\": [{}, {}],\n  \"source_sha256\": \"{}\",\n  \"caprichio_ir_sha256\": \"{}\",\n  \"target_platform\": {},\n  \"app_id\": {},\n  \"app_version\": {},\n  \"ipk\": {},\n  \"ipk_sha256\": \"{}\",\n  \"splash\": {{\"contract\": \"{}\", \"file\": \"{}\", \"sha256\": \"{}\", \"hold_ms\": {}, \"fade_ms\": {}}},\n  \"files\": {{\n{}\n  }}\n}}\n",
        crate::CONTRACT,
        env!("CARGO_PKG_VERSION"),
        ontology::CONTRACT,
        ONTOLOGY_WIDTH_PT,
        ONTOLOGY_HEIGHT_PT,
        hex(&o.parsed.source_digest),
        sha256(caps::canonical_ir().as_bytes()),
        json_string(opts.target.platform.id()),
        json_string(&opts.app.id),
        json_string(&opts.app.version),
        json_string(&ipk.file_name),
        sha256(&ipk.bytes),
        crate::splash::CONTRACT,
        crate::splash::FILE,
        sha256(crate::splash::LOGO),
        crate::splash::HOLD_MS,
        crate::splash::FADE_MS,
        file_hashes.join(",\n")
    );
    let reports = vec![
        (
            "compatibility.json".to_string(),
            compatibility_report(&opts.target, min_chromium(&o)).into_bytes(),
        ),
        (
            "compilation-manifest.json".to_string(),
            manifest.into_bytes(),
        ),
        (
            "navigation-graph.json".to_string(),
            emit::navigation_report(&o).into_bytes(),
        ),
    ];
    Ok(BuildOutput {
        app_files,
        ipk,
        reports,
    })
}

/// Write `output` under `out`, which must not exist yet. Its parent must
/// exist. Nothing existing is overwritten. On failure the partial
/// directory is removed.
pub fn write(output: &BuildOutput, out: &Path) -> io::Result<()> {
    fs::create_dir(out)?;
    let result = (|| {
        let write_new = |path: &Path, bytes: &[u8]| -> io::Result<()> {
            use io::Write;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?
                .write_all(bytes)
        };
        write_new(&out.join(&output.ipk.file_name), &output.ipk.bytes)?;
        for (dir, files) in [("app", &output.app_files), ("reports", &output.reports)] {
            let base = out.join(dir);
            fs::create_dir(&base)?;
            for (rel, bytes) in files {
                let path = base.join(rel);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                write_new(&path, bytes)?;
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(out);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target;

    const SCENE: &str = r#"@ontology media {
  @cell root { id: "root"; fill: #f4f1ea;
    @cell title { id: "title"; position_p: (0.05, 0.05, 0); extent_p: (0.9, 0.1, 0); text: "Goya Media Centre"; text-size-p: 0.6; }
    @button films { id: "films"; position_p: (0.05, 0.3, 0); extent_p: (0.25, 0.25, 0); text: "Films"; text-size-p: 0.2; fill: #d9e2ec; }
    @button music { id: "music"; position_p: (0.375, 0.3, 0); extent_p: (0.25, 0.25, 0); text: "Music"; text-size-p: 0.2; fill: #d9e2ec; }
    @button settings { id: "settings"; position_p: (0.7, 0.3, 0); extent_p: (0.25, 0.25, 0); text: "Settings"; text-size-p: 0.2; fill: #d9e2ec; }
  }
}"#;

    fn opts(target: Option<&str>) -> BuildOptions {
        BuildOptions {
            app: AppInfo {
                id: "com.goya.arachnid.media".into(),
                title: "Goya Media".into(),
                version: "0.1.0".into(),
                vendor: Some("Goya".into()),
            },
            target: target::resolve(target).unwrap(),
            icon: None,
        }
    }

    #[test]
    fn builds_are_byte_identical() {
        let a = build(SCENE, &opts(Some("model:OLED65C3PUA"))).unwrap();
        let b = build(SCENE, &opts(Some("model:OLED65C3PUA"))).unwrap();
        assert_eq!(a.ipk.bytes, b.ipk.bytes);
        assert_eq!(a.reports, b.reports);
        assert_eq!(a.app_files, b.app_files);
    }

    #[test]
    fn output_contains_graph_and_layout() {
        let out = build(SCENE, &opts(None)).unwrap();
        let file = |name: &str| {
            String::from_utf8(
                out.app_files
                    .iter()
                    .find(|(p, _)| p == name)
                    .unwrap()
                    .1
                    .clone(),
            )
            .unwrap()
        };
        let html = file("index.html");
        assert!(html.contains(r#"<div id="n2" class="n b" role="button" data-focus="0" data-goya-id="films" style="left:96px;top:324px;width:480px;height:270px;background:#d9e2ec;font-size:54px">Films</div>"#), "{html}");
        let js = file("app.js");
        assert!(js.contains(r#"var CONFIGS = {"0,1,2":{graph:{"0":[null,null,null,1],"1":[null,null,0,2],"2":[null,null,1,null]},initial:0}};"#), "{js}");
        let nav = String::from_utf8(out.reports[2].1.clone()).unwrap();
        assert!(nav.contains(r#""initial": "films""#), "{nav}");
        assert!(nav.contains(r#""max_presses": 2"#), "{nav}");
        assert!(nav.contains(r#""max_presses_home": 2"#), "{nav}");
        assert!(
            nav.contains(r#""id": "films", "presses": 0, "presses_home": 0"#),
            "{nav}"
        );
        // Stock remote: only the platform's cursor report enters pointer mode.
        assert!(!js.contains("mousemove"), "{js}");
        assert!(js.contains("pointerMode = !!(e.detail && e.detail.visibility);"));
        let compat = String::from_utf8(out.reports[0].1.clone()).unwrap();
        assert!(
            compat.contains(r#""platform": "4.0""#) && compat.contains(r#""chromium": 53"#),
            "{compat}"
        );
        assert!(
            compat.contains(r#""navigate_keys": [37, 38, 39, 40], "activate_keys": [13], "platform_keys": [461, 403, 404, 405, 406]"#),
            "{compat}"
        );
    }

    #[test]
    fn feature_gate_refuses_old_engines() {
        let t = target::resolve(Some("webos:6")).unwrap();
        let err = feature_gate(&t, &[WebFeature::OptionalChaining]).unwrap_err();
        assert_eq!(err[0].code, Code::FeatureUnsupported);
        assert!(feature_gate(
            &target::resolve(Some("webos:22")).unwrap(),
            &[WebFeature::OptionalChaining]
        )
        .is_ok());
    }

    #[test]
    fn invalid_metadata_and_icon_are_refused() {
        let mut o = opts(None);
        o.app.id = "com.lge.bad".into();
        assert_eq!(
            build(SCENE, &o).err().unwrap()[0].code,
            Code::InvalidAppInfo
        );
        let mut o = opts(None);
        o.icon = Some(crate::archive::png_rgb(1, 1, &[0, 0, 0]));
        assert_eq!(build(SCENE, &o).err().unwrap()[0].code, Code::InvalidIcon);
    }

    #[test]
    fn write_refuses_existing_directory() {
        let out = build(SCENE, &opts(None)).unwrap();
        let base = std::env::temp_dir().join(format!("arachnid-write-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        write(&out, &base).unwrap();
        assert!(base.join("com.goya.arachnid.media_0.1.0_all.ipk").is_file());
        assert!(base.join("app/index.html").is_file());
        assert!(base.join("reports/navigation-graph.json").is_file());
        assert!(write(&out, &base).is_err());
        fs::remove_dir_all(&base).unwrap();
    }
}
