//! `emperor build` for a general scene (`emperor.linux.scene/0.0.1`).
//!
//! A source that is not the one-counter subset is admitted as GSSH/4 (or
//! GSSH/4-live), checked against what the Linux scene runtime implements,
//! and built as `emperor-scene-app` with the scene, its compiled graphics
//! and the UI font embedded:
//!
//! * **Refused at their range**: host state, `@link`, `image-from` and D2
//!   interactions (no Linux platform services yet).
//! * **Graphics**: every `@graphic` is compiled and verified by
//!   `goya_scene_graphics` (GVI-D/A/O, kept or baked, reference decoder).
//! * **Durable state**: only `linux.xdg-state.v1`, the app's own record
//!   under `$XDG_STATE_HOME/goya` (Nebula decision 6 as amended on
//!   25 November 2026); without `@persistence` the default is used with a
//!   warning (D11). `linux.sqlite.v1` is refused until built.
//! * **Font**: DejaVu Sans, read once at build time and embedded; the app
//!   reads no font files. Its SHA-256 is in the report.
//! * **Accessibility**: `@iden` and D9 announcements go to AT-SPI through
//!   AccessKit at run time.
//!
//! With `--desktop cosmic` (Nebula) the COSMIC build-time verdict and theme
//! are recorded in the report; authored scene colours are kept.

use super::{finish, Invocation, Run};
use crate::stage::{Stages, Status};
use goya_lang::StateLifetime;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

const FONT: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
const LINUX_LOCAL_STORE: &str = "linux.xdg-state.v1";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn rust_string(s: &str) -> String {
    format!("{s:?}")
}

fn fail(invocation: &Invocation, stages: &mut Stages, code: u8, message: String) -> Run {
    stages.push(Status::Fail, message.clone());
    finish(invocation, code, std::mem::take(stages), format!("{message}\n"), json!({}))
}

/// Build `source` as a scene app into `out`.
pub(super) fn build_scene(invocation: &Invocation, source: &str, out: &Path, desktop: Option<Value>) -> Run {
    let mut stages = Stages::new();
    let handoff = match goya_lang::handoff::admit_v4(source) {
        Err(e) if e.code == "GY-E1012" => goya_lang::handoff::admit_v4_live(source),
        other => other,
    };
    let handoff = match handoff {
        Ok(h) => h,
        Err(e) => return fail(invocation, &mut stages, 1, format!("EMP-SCENE {}: {e} (bytes {}..{})", e.code, e.range.start, e.range.end)),
    };
    let parsed = handoff.parsed();
    stages.push(Status::Pass, format!("admitted as {}", handoff.contract()));
    // What the Linux scene runtime does not implement is refused, not dropped.
    let refusal = parsed
        .declarations
        .state
        .iter()
        .flat_map(|(d, _)| d)
        .find(|d| d.lifetime == StateLifetime::Host)
        .map(|d| (format!("host state '{}'", d.key), d.range))
        .or_else(|| parsed.entries.iter().find(|e| e.kind == "link").map(|e| ("@link".to_owned(), e.range)))
        .or_else(|| parsed.entries.iter().find(|e| e.kind == "apps").map(|e| ("@apps".to_owned(), e.range)))
        .or_else(|| parsed.entries.iter().find(|e| e.kind == "property" && e.name.replace('-', "_") == "remote_colour").map(|e| ("remote-colour".to_owned(), e.range)))
        .or_else(|| parsed.entries.iter().find(|e| e.kind == "property" && e.name.replace('-', "_") == "colour_group").map(|e| ("colour-group".to_owned(), e.range)))
        .or_else(|| parsed.entries.iter().find(|e| e.kind == "property" && e.name == "sky").map(|e| ("sky".to_owned(), e.range)))
        .or_else(|| parsed.entries.iter().find(|e| e.kind == "property" && e.name.replace('-', "_") == "image_from").map(|e| ("image-from".to_owned(), e.range)))
        .or_else(|| {
            parsed.scene.interactions.iter().find(|i| i.class == goya_lang::InteractionClass::D2).map(|i| {
                let range = parsed.entries.iter().find(|e| e.kind == "interaction" && e.name == i.name).map_or(goya_lang::SourceRange { start: 0, end: 0 }, |e| e.range);
                (format!("D2 interaction {}", i.name), range)
            })
        });
    if let Some((what, range)) = refusal {
        return fail(invocation, &mut stages, 1, format!("EMP-SCENE: {what} needs Linux platform services Emperor does not have yet (bytes {}..{})", range.start, range.end));
    }
    if let Err(e) = goya_lang::compile_d1_interactions(&parsed.scene) {
        return fail(invocation, &mut stages, 1, format!("EMP-SCENE: interactions: {e}"));
    }
    stages.push(Status::Pass, format!("{} D1 interactions compiled by the canonical compiler", parsed.scene.interactions.len()));

    // Durable store (D11).
    let durable = parsed.declarations.state.iter().flat_map(|(d, _)| d).any(|d| d.lifetime == StateLifetime::Durable);
    let mut warnings = Vec::new();
    if durable {
        use goya_caprichios::state_persistence::{resolve, Declaration, Platform};
        let declaration = parsed.declarations.persistence_cache_targets.as_ref().map(|t| Declaration { cache_targets: t.clone() });
        match resolve(declaration.as_ref(), Platform::Linux) {
            Ok(r) if r.adapter == LINUX_LOCAL_STORE => {
                if r.defaulted {
                    warnings.push(format!("warning: durable state uses the default Linux store {LINUX_LOCAL_STORE}; name it in @persistence to make the choice explicit (decision D11)"));
                }
            }
            Ok(r) => return fail(invocation, &mut stages, 1, format!("EMP-SCENE: durable store {} is not implemented; use {LINUX_LOCAL_STORE}", r.adapter)),
            Err(e) => return fail(invocation, &mut stages, 1, format!("EMP-SCENE: no Linux durable store: {e:?}")),
        }
        stages.push(Status::Pass, format!("durable state in {LINUX_LOCAL_STORE} ($XDG_STATE_HOME/goya/{}.json, the only file the App writes)", parsed.scene.name));
    }

    // Graphics.
    if let Err(e) = fs::create_dir_all(out.join("graphics")) {
        return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: {e}"));
    }
    let mut graphic_specs = Vec::new();
    let mut graphic_reports = Vec::new();
    for g in &parsed.declarations.graphics {
        let compiled = match goya_scene_graphics::compile(&parsed.scene.name, g) {
            Ok(c) => c,
            Err(why) => return fail(invocation, &mut stages, 1, format!("EMP-SCENE: @graphic {}: {why}", g.name)),
        };
        let file = out.join("graphics").join(format!("{}.gvi", compiled.name));
        if let Err(e) = fs::write(&file, &compiled.bytes) {
            return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: {e}"));
        }
        let binding: Vec<String> = (0..32).map(|k| format!("0x{}", &compiled.binding[2 * k..2 * k + 2])).collect();
        let radices: Vec<String> = compiled.dims.iter().map(|d| d.1.len().to_string()).collect();
        let steps: Vec<String> = compiled.dims.iter().map(|d| d.2.unwrap_or(0).to_string()).collect();
        let absolute = fs::canonicalize(&file).unwrap_or(file.clone());
        graphic_specs.push(format!(
            "emperor_runtime::scene::GraphicSpec {{ name: {}, bytes: include_bytes!({}), binding: [{}], radices: &[{}], steps_ms: &[{}] }}",
            rust_string(&compiled.name),
            rust_string(&absolute.display().to_string()),
            binding.join(", "),
            radices.join(", "),
            steps.join(", ")
        ));
        graphic_reports.push(json!({
            "name": compiled.name, "id": compiled.authored_id, "bytes": compiled.bytes.len(), "sha256": compiled.sha256,
            "binding": compiled.binding, "states": compiled.states, "projections": compiled.projections,
            "baked_transforms": compiled.baked, "other_encoding_bytes": compiled.other_size,
        }));
    }
    if !graphic_specs.is_empty() {
        stages.push(Status::Pass, format!("{} @graphic compiled to GY-GVI/0 and verified by the reference decoder", graphic_specs.len()));
    }

    // Font, embedded.
    let font = match fs::read(FONT) {
        Ok(f) => f,
        Err(e) => return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: font {FONT}: {e}")),
    };
    let font_path = out.join("font.ttf");
    if let Err(e) = fs::write(&font_path, &font) {
        return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: {e}"));
    }
    let font_abs = fs::canonicalize(&font_path).unwrap_or(font_path);
    let source_abs = out.join("scene.gscene");
    if let Err(e) = fs::write(&source_abs, source) {
        return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: {e}"));
    }
    let embed = format!(
        "// Generated by Emperor {}. Do not edit.\npub static SCENE_SPEC: emperor_runtime::scene::SceneSpec = emperor_runtime::scene::SceneSpec {{\n    source: include_str!({}),\n    title: {},\n    graphics: &[{}],\n    durable: {durable},\n    font: include_bytes!({}),\n}};\n",
        crate::COMPILER_VERSION,
        rust_string(&fs::canonicalize(&source_abs).unwrap_or(source_abs.clone()).display().to_string()),
        rust_string(&parsed.scene.name),
        graphic_specs.join(",\n        "),
        rust_string(&font_abs.display().to_string()),
    );
    let embed_path = out.join("embed.rs");
    if let Err(e) = fs::write(&embed_path, &embed) {
        return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: {e}"));
    }

    // Link.
    let target_dir = out.join("target");
    let mut command = Command::new("cargo");
    command
        .args(["build", "-p", "emperor-runtime", "--bin", "emperor-scene-app", "--features", "scene", "--offline", "--target-dir"])
        .arg(&target_dir)
        .current_dir(super::workspace_root())
        .env("EMPEROR_EMBED", &embed_path)
        .env("EMPEROR_WINDOW_BACKEND", "wayland")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if invocation.release {
        command.arg("--release");
    } else {
        command.env("CARGO_PROFILE_DEV_DEBUG", "0");
    }
    match command.output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            let tail: String = err.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: cargo build failed: {tail}"));
        }
        Err(e) => return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: could not start cargo: {e}")),
    }
    let produced = target_dir.join(if invocation.release { "release" } else { "debug" }).join("emperor-scene-app");
    let app = out.join("App");
    if let Err(e) = fs::copy(&produced, &app) {
        return fail(invocation, &mut stages, 2, format!("EMP-TOOLCHAIN: {e}"));
    }
    let app_bytes = fs::read(&app).unwrap_or_default();
    stages.push(Status::Pass, "rustc produced the scene App ELF".to_owned());
    let text = format!(
        "{}built {} ({}), scene runtime {}\n",
        warnings.iter().map(|w| format!("{w}\n")).collect::<String>(),
        app.display(),
        hex(&Sha256::digest(&app_bytes)),
        emperor_runtime_contract()
    );
    finish(
        invocation,
        0,
        stages,
        text,
        json!({
            "out": out.display().to_string(),
            "app": app.display().to_string(),
            "app_sha256": hex(&Sha256::digest(&app_bytes)),
            "runtime_contract": emperor_runtime_contract(),
            "scene": parsed.scene.name,
            "source_sha256": hex(&parsed.source_digest),
            "graphics": graphic_reports,
            "durable_store": if durable { Value::from(LINUX_LOCAL_STORE) } else { Value::Null },
            "capabilities": { "filesystem": if durable { "own state record only" } else { "none" }, "accessibility": "AT-SPI via AccessKit" },
            "font": { "file": FONT, "sha256": hex(&Sha256::digest(&font)) },
            "desktop": desktop.unwrap_or(Value::Null),
            "warnings": warnings,
        }),
    )
}

fn emperor_runtime_contract() -> &'static str {
    "emperor.linux.scene/0.0.1"
}
