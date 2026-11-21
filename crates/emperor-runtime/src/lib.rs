//! Linux runtime for one admitted counter (`emperor.linux.interactive/0.0.1`).
//!
//! Preedit and commit text are ignored. That is a limitation, not input-method
//! support. The crate does not sandbox the process, does not reflow on resize,
//! and presents through the existing Wayland software host. It is not a Vulkan
//! renderer and it does not run any other profile.

#![forbid(unsafe_code)]

mod exercise;
#[cfg(feature = "kumimanu")]
mod glyph;
mod host;
#[cfg(feature = "kumimanu")]
mod host_kumimanu;

pub use exercise::{exercise_interaction, InteractionReport};

use goya_parity::keys::KeyController;
use sha2::{Digest, Sha256};

/// Native IR schema accepted by this runtime.
const SCHEMA: &str = "GY-EMP-NIR/1";
/// Profile of the single admitted Linux counter.
const PROFILE: &str = "emperor.linux.interactive/0.0.1";
/// Element count bound. Sash admits at most 32 source cells, so a native IR
/// with more elements cannot come from an admitted scene. The runtime
/// re-checks the bound because the JSON parsed here is a separate artifact
/// from the source Sash admitted; the compiler embed is not trusted to have
/// gone through admission unchanged.
const MAX_ELEMENTS: usize = 32;
/// Maximum serialized native IR accepted from a file or embedding. The
/// admitted source itself is capped at 1 MiB; this allowance covers JSON
/// escaping and repeated element metadata while bounding deserialization.
pub const MAX_NIR_JSON_BYTES: usize = 8 * 1024 * 1024;
/// Sash refuses scene source above this size before lowering it to NIR.
const MAX_SCENE_SOURCE_BYTES: usize = 1024 * 1024;
/// Geometry bound in DIP, mirroring `sash::admit::MAX_DIP`. Positions are
/// accepted in `0..=MAX_DIP` and extents in `1..=MAX_DIP`, exactly as Sash's
/// admission accepts them. Values inside these bounds cannot combine into an
/// `i32` overflow in downstream rectangle arithmetic.
const MAX_DIP: i32 = 4096;

pub(crate) const WM_KEYDOWN: u32 = 0x0100;
pub(crate) const VK_TAB: usize = 9;
pub(crate) const VK_RETURN: usize = 13;
pub(crate) const VK_SPACE: usize = 0x20;

/// One lowered control. Coordinates are device-independent pixels.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: String,
    pub role: String,
    pub text: String,
    pub x_dip: i32,
    pub y_dip: i32,
    pub width_dip: i32,
    pub height_dip: i32,
    pub span_start: usize,
    pub span_end: usize,
}

/// Capability flags carried beside the admitted scene.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub copy: bool,
    pub paste: bool,
    pub cut: String,
    pub filesystem: String,
    pub network: bool,
    pub subprocess: bool,
    pub web_content: bool,
}

/// Lowered interactive counter. Field order is the canonical JSON order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeIr {
    pub schema: String,
    pub profile: String,
    pub source_digest_sha256: String,
    pub scene: String,
    pub action_id: String,
    pub state_id: String,
    pub input_id: String,
    pub initial_state: i64,
    pub initial_input: i64,
    pub width_dip: i32,
    pub height_dip: i32,
    pub elements: Vec<Element>,
    pub capabilities: Capabilities,
}

/// Static control geometry embedded in a compiled scene.
#[derive(Debug, Clone, Copy)]
pub struct CompiledElement {
    pub id: &'static str,
    pub role: &'static str,
    pub text: &'static str,
    pub x_dip: i32,
    pub y_dip: i32,
    pub width_dip: i32,
    pub height_dip: i32,
    pub span_start: usize,
    pub span_end: usize,
}

/// Scene compiled into the process. `nir_json` is the native IR.
#[derive(Debug, Clone)]
pub struct CompiledScene {
    pub nir_json: &'static str,
    pub schema: &'static str,
    pub profile: &'static str,
    pub source_digest_sha256: &'static str,
    pub scene: &'static str,
    pub action_id: &'static str,
    pub state_id: &'static str,
    pub input_id: &'static str,
    pub initial_state: i64,
    pub initial_input: i64,
    pub width_dip: i32,
    pub height_dip: i32,
    pub copy: bool,
    pub paste: bool,
    pub cut: &'static str,
    pub filesystem: &'static str,
    pub network: bool,
    pub subprocess: bool,
    pub web_content: bool,
    pub elements: &'static [CompiledElement],
}

/// Headless or hosted result of one counter run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleReport {
    pub count: i64,
    pub revision: u64,
    pub copy_allowed: bool,
    pub paste_allowed: bool,
    pub presented: u32,
    pub wayland: &'static str,
    /// `wayland`, `kumimanu`, or `not-requested` when no window was opened.
    pub window: &'static str,
    pub edit: String,
}

/// How [`run`] drives the admitted counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// One add, no window.
    Oracle,
    /// Open the Wayland host and present one frame.
    Smoke,
    /// Run the scripted counter on the Wayland host and present that frame.
    Exercise,
    /// Pump until close, or until `max_frames` pumps when set.
    Live { max_frames: Option<u32> },
}

/// Admit `nir_json`, apply one add, and ignore paste when it is denied.
pub fn oracle(nir_json: &str) -> Result<OracleReport, String> {
    run(nir_json, RunMode::Oracle)
}

/// Admit `nir_json` and run it in `mode`. Windowed modes present through the
/// host picked by [`open_window`]; `Oracle` never opens a window.
pub fn run(nir_json: &str, mode: RunMode) -> Result<OracleReport, String> {
    let (ir, mut controller) = open_admitted(nir_json)?;
    match mode {
        RunMode::Oracle => {
            press_add(&mut controller);
            prove_paste_denied(&ir, &mut controller)?;
            Ok(report(&controller, 0, "not-requested"))
        }
        RunMode::Smoke => open_window(&ir, controller, host::WindowMode::Smoke),
        RunMode::Exercise => open_window(&ir, controller, host::WindowMode::Exercise),
        RunMode::Live { max_frames } => {
            open_window(&ir, controller, host::WindowMode::Live { max_frames })
        }
    }
}

/// Apply `times` successful add activations. Enter activates without moving focus.
pub fn activate_add(nir_json: &str, times: u32) -> Result<OracleReport, String> {
    let (_ir, mut controller) = open_admitted(nir_json)?;
    for _ in 0..times {
        press_add(&mut controller);
    }
    Ok(report(&controller, 0, "not-requested"))
}

/// Attempt a paste and report whether the edit stayed `"1"`.
///
/// Denied paste does not change the edit. Allowed paste replaces it, so the
/// result is false unless the replacement is still `"1"`.
pub fn paste_is_ignored(nir_json: &str) -> Result<bool, String> {
    let (ir, mut controller) = open_admitted(nir_json)?;
    attempt_paste(&ir, &mut controller);
    Ok(controller.session.edit == "1")
}

/// Parse `scene.nir_json` and require every compiled field to match it.
pub fn assert_scene_matches(scene: &CompiledScene) -> Result<(), String> {
    if scene.nir_json.len() > MAX_NIR_JSON_BYTES {
        return Err(format!(
            "native ir is {} bytes; the limit is {MAX_NIR_JSON_BYTES}",
            scene.nir_json.len()
        ));
    }
    let ir: NativeIr = serde_json::from_str(scene.nir_json).map_err(|err| err.to_string())?;
    same("schema", scene.schema, &ir.schema)?;
    same("profile", scene.profile, &ir.profile)?;
    same(
        "source_digest_sha256",
        scene.source_digest_sha256,
        &ir.source_digest_sha256,
    )?;
    same("scene", scene.scene, &ir.scene)?;
    same("action_id", scene.action_id, &ir.action_id)?;
    same("state_id", scene.state_id, &ir.state_id)?;
    same("input_id", scene.input_id, &ir.input_id)?;
    same_i64("initial_state", scene.initial_state, ir.initial_state)?;
    same_i64("initial_input", scene.initial_input, ir.initial_input)?;
    same_i32("width_dip", scene.width_dip, ir.width_dip)?;
    same_i32("height_dip", scene.height_dip, ir.height_dip)?;
    same_bool("copy", scene.copy, ir.capabilities.copy)?;
    same_bool("paste", scene.paste, ir.capabilities.paste)?;
    same("cut", scene.cut, &ir.capabilities.cut)?;
    same("filesystem", scene.filesystem, &ir.capabilities.filesystem)?;
    same_bool("network", scene.network, ir.capabilities.network)?;
    same_bool("subprocess", scene.subprocess, ir.capabilities.subprocess)?;
    same_bool(
        "web_content",
        scene.web_content,
        ir.capabilities.web_content,
    )?;
    if scene.elements.len() != ir.elements.len() {
        return Err(format!(
            "element count {} does not match {}",
            scene.elements.len(),
            ir.elements.len()
        ));
    }
    for (index, (compiled, element)) in scene.elements.iter().zip(&ir.elements).enumerate() {
        let label = |field: &str| format!("elements[{index}].{field}");
        same(&label("id"), compiled.id, &element.id)?;
        same(&label("role"), compiled.role, &element.role)?;
        same(&label("text"), compiled.text, &element.text)?;
        same_i32(&label("x_dip"), compiled.x_dip, element.x_dip)?;
        same_i32(&label("y_dip"), compiled.y_dip, element.y_dip)?;
        same_i32(&label("width_dip"), compiled.width_dip, element.width_dip)?;
        same_i32(
            &label("height_dip"),
            compiled.height_dip,
            element.height_dip,
        )?;
        same_usize(
            &label("span_start"),
            compiled.span_start,
            element.span_start,
        )?;
        same_usize(&label("span_end"), compiled.span_end, element.span_end)?;
    }
    Ok(())
}

pub(crate) fn press_add(controller: &mut KeyController) {
    controller.on_key(WM_KEYDOWN, VK_RETURN, false);
}

pub(crate) fn open_admitted(nir_json: &str) -> Result<(NativeIr, KeyController), String> {
    if nir_json.len() > MAX_NIR_JSON_BYTES {
        return Err(format!(
            "native ir is {} bytes; the limit is {MAX_NIR_JSON_BYTES}",
            nir_json.len()
        ));
    }
    let ir: NativeIr = serde_json::from_str(nir_json).map_err(|err| err.to_string())?;
    if ir.scene.len() > MAX_SCENE_SOURCE_BYTES {
        return Err(format!(
            "scene is {} bytes; the admission limit is {MAX_SCENE_SOURCE_BYTES}",
            ir.scene.len()
        ));
    }
    if ir.elements.len() > MAX_ELEMENTS {
        return Err(format!(
            "element count {} exceeds the {MAX_ELEMENTS} cells this profile admits",
            ir.elements.len()
        ));
    }
    if ir.schema != SCHEMA {
        return Err(format!("schema {} is not {SCHEMA}", ir.schema));
    }
    if ir.profile != PROFILE {
        return Err(format!("profile {} is not {PROFILE}", ir.profile));
    }
    if ir.source_digest_sha256.len() != 64
        || !ir
            .source_digest_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("source_digest_sha256 must be 64 lowercase hexadecimal characters".into());
    }
    let actual_digest = Sha256::digest(ir.scene.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if ir.source_digest_sha256 != actual_digest {
        return Err("source_digest_sha256 does not match scene bytes".into());
    }
    if ir.capabilities.filesystem != "none" {
        return Err(format!(
            "filesystem {} is not admitted for this profile",
            ir.capabilities.filesystem
        ));
    }
    if ir.capabilities.network || ir.capabilities.subprocess || ir.capabilities.web_content {
        return Err(
            "network, subprocess, and web_content are not admitted for this profile".into(),
        );
    }
    if !matches!(ir.capabilities.cut.as_str(), "allow" | "default" | "deny") {
        return Err(format!("cut {} is not admitted", ir.capabilities.cut));
    }
    let mut ids = std::collections::HashSet::with_capacity(ir.elements.len());
    let mut clients = 0;
    let mut edits = 0;
    let mut buttons = 0;
    let mut results = 0;
    let mut spans = Vec::with_capacity(ir.elements.len());
    for element in &ir.elements {
        if element.id.is_empty() || !ids.insert(element.id.as_str()) {
            return Err(format!(
                "element id {:?} is empty or duplicated",
                element.id
            ));
        }
        if element.span_start > element.span_end
            || element.span_end > ir.scene.len()
            || !ir.scene.is_char_boundary(element.span_start)
            || !ir.scene.is_char_boundary(element.span_end)
        {
            return Err(format!(
                "element {} has an invalid UTF-8 source span",
                element.id
            ));
        }
        if &ir.scene[element.span_start..element.span_end] != element.id {
            return Err(format!(
                "element {} source span does not match its semantic id",
                element.id
            ));
        }
        spans.push((element.span_start, element.span_end, element.id.as_str()));
        if !matches!(
            element.role.as_str(),
            "client" | "edit" | "button" | "result"
        ) {
            return Err(format!("role {} is not admitted", element.role));
        }
        // Sash admission bounds geometry before it can produce a scene, so
        // mirror those bounds here: a native IR outside them is not something
        // an admitted scene lowers to. Sash accepts positions in
        // `0..=MAX_DIP` and sizes in `1..=MAX_DIP`; anything else, including
        // negative extents or coordinates that would overflow `i32` when
        // combined, is rejected before any rectangle arithmetic sees it.
        if element.x_dip < 0
            || element.y_dip < 0
            || element.x_dip > MAX_DIP
            || element.y_dip > MAX_DIP
        {
            return Err(format!(
                "element {} is at {} by {} dip; this profile accepts positions in 0..={MAX_DIP}",
                element.id, element.x_dip, element.y_dip
            ));
        }
        if element.width_dip <= 0
            || element.height_dip <= 0
            || element.width_dip > MAX_DIP
            || element.height_dip > MAX_DIP
        {
            return Err(format!(
                "element {} is {} by {} dip; this profile accepts extents in 1..={MAX_DIP}",
                element.id, element.width_dip, element.height_dip
            ));
        }
        match element.role.as_str() {
            "client" => clients += 1,
            "edit" => edits += 1,
            "button" => buttons += 1,
            "result" => results += 1,
            _ => return Err(format!("role {} is not admitted", element.role)),
        }
    }
    spans.sort_unstable_by_key(|span| span.0);
    for pair in spans.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(format!(
                "element source spans overlap: {} and {}",
                pair[0].2, pair[1].2
            ));
        }
    }
    if clients != 1 || edits != 1 || buttons != 1 || results != 1 {
        return Err(format!(
            "native ir needs exactly one client, edit, button, and result; found {clients}, {edits}, {buttons}, {results}"
        ));
    }
    // The window rectangle gets the same bound as the elements. Sash admits
    // the client size in `1..=MAX_DIP` on each edge, and the host converts
    // this value directly into the frame size.
    if ir.width_dip <= 0 || ir.height_dip <= 0 || ir.width_dip > MAX_DIP || ir.height_dip > MAX_DIP
    {
        return Err(format!(
            "window size {} by {} dip is not admitted; this profile accepts extents in 1..={MAX_DIP}",
            ir.width_dip, ir.height_dip
        ));
    }
    let controller = KeyController::open(&ir.scene).map_err(|err| err.to_string())?;
    if controller.session.count != ir.initial_state {
        return Err(format!(
            "lowering bug: initial count {} does not match admitted {}",
            ir.initial_state, controller.session.count
        ));
    }
    let expected_edit = ir.initial_input.to_string();
    if controller.session.edit != expected_edit {
        return Err(format!(
            "lowering bug: initial edit {} does not match formatted input {expected_edit}",
            controller.session.edit
        ));
    }
    if controller.session.copy_allowed != ir.capabilities.copy {
        return Err(format!(
            "lowering bug: copy flag admitted {} does not match capabilities {}",
            controller.session.copy_allowed, ir.capabilities.copy
        ));
    }
    if controller.session.paste_allowed != ir.capabilities.paste {
        return Err(format!(
            "lowering bug: paste flag admitted {} does not match capabilities {}",
            controller.session.paste_allowed, ir.capabilities.paste
        ));
    }
    Ok((ir, controller))
}

/// Denied paste leaves the edit alone. Allowed paste is not forced into that path.
fn attempt_paste(ir: &NativeIr, controller: &mut KeyController) {
    if ir.capabilities.paste {
        controller.session.set_edit("999");
    }
}

fn prove_paste_denied(ir: &NativeIr, controller: &mut KeyController) -> Result<(), String> {
    if ir.capabilities.paste {
        return Ok(());
    }
    let before = controller.session.edit.clone();
    attempt_paste(ir, controller);
    if controller.session.edit != before {
        return Err("paste changed the edit".into());
    }
    Ok(())
}

/// Window backend for [`open_window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Wayland,
    Kumimanu,
}

/// Route `EMPEROR_WINDOW_BACKEND` to a backend. An explicit value wins; the
/// `kumimanu` feature decides only when the variable is unset or names
/// something else. Each variant is compiled under its own `cfg`, so tests run
/// against whichever variant the feature set selects.
#[cfg(feature = "kumimanu")]
fn backend_for(env: Option<&str>) -> Backend {
    match env {
        Some("wayland") => Backend::Wayland,
        _ => Backend::Kumimanu,
    }
}

#[cfg(not(feature = "kumimanu"))]
fn backend_for(env: Option<&str>) -> Backend {
    match env {
        Some("kumimanu") => Backend::Kumimanu,
        _ => Backend::Wayland,
    }
}

/// Present through the kumimanu host when `EMPEROR_WINDOW_BACKEND=kumimanu` or
/// when the `kumimanu` feature is enabled without an override; otherwise use
/// the Wayland host. `--features kumimanu` alone is sufficient, matching the
/// env var the emperor build sets.
fn open_window(
    ir: &NativeIr,
    controller: KeyController,
    mode: host::WindowMode,
) -> Result<OracleReport, String> {
    match backend_for(option_env!("EMPEROR_WINDOW_BACKEND")) {
        Backend::Kumimanu => open_kumimanu(ir, controller, mode),
        Backend::Wayland => host::run_window(ir, controller, mode),
    }
}

#[cfg(feature = "kumimanu")]
fn open_kumimanu(
    ir: &NativeIr,
    controller: KeyController,
    mode: host::WindowMode,
) -> Result<OracleReport, String> {
    host_kumimanu::run_window(ir, controller, mode)
}

#[cfg(not(feature = "kumimanu"))]
fn open_kumimanu(
    _ir: &NativeIr,
    _controller: KeyController,
    _mode: host::WindowMode,
) -> Result<OracleReport, String> {
    Err("kumimanu backend was requested but this runtime was linked without it".into())
}

fn report(controller: &KeyController, presented: u32, wayland: &'static str) -> OracleReport {
    let window = if wayland == "not-requested" {
        "not-requested"
    } else {
        "wayland"
    };
    OracleReport {
        count: controller.session.count,
        revision: controller.session.revision,
        copy_allowed: controller.session.copy_allowed,
        paste_allowed: controller.session.paste_allowed,
        presented,
        wayland,
        window,
        edit: controller.session.edit.clone(),
    }
}

fn same(field: &str, compiled: &str, ir: &str) -> Result<(), String> {
    if compiled == ir {
        Ok(())
    } else {
        Err(format!("{field} does not match native ir"))
    }
}

fn same_i64(field: &str, compiled: i64, ir: i64) -> Result<(), String> {
    if compiled == ir {
        Ok(())
    } else {
        Err(format!("{field} {compiled} does not match {ir}"))
    }
}

fn same_i32(field: &str, compiled: i32, ir: i32) -> Result<(), String> {
    if compiled == ir {
        Ok(())
    } else {
        Err(format!("{field} {compiled} does not match {ir}"))
    }
}

fn same_usize(field: &str, compiled: usize, ir: usize) -> Result<(), String> {
    if compiled == ir {
        Ok(())
    } else {
        Err(format!("{field} {compiled} does not match {ir}"))
    }
}

fn same_bool(field: &str, compiled: bool, ir: bool) -> Result<(), String> {
    if compiled == ir {
        Ok(())
    } else {
        Err(format!("{field} {compiled} does not match {ir}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = include_str!("../../sash/fixtures/interactive-counter.gscene");

    fn counter_ir() -> NativeIr {
        NativeIr {
            schema: SCHEMA.to_owned(),
            profile: PROFILE.to_owned(),
            source_digest_sha256: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_owned(),
            scene: SCENE.to_owned(),
            action_id: "action.add".to_owned(),
            state_id: "count".to_owned(),
            input_id: "amount".to_owned(),
            initial_state: 0,
            initial_input: 1,
            width_dip: 320,
            height_dip: 160,
            elements: vec![
                Element {
                    id: "window.root".to_owned(),
                    role: "client".to_owned(),
                    text: "Count".to_owned(),
                    x_dip: 0,
                    y_dip: 0,
                    width_dip: 320,
                    height_dip: 160,
                    span_start: 0,
                    span_end: 0,
                },
                Element {
                    id: "window.amount".to_owned(),
                    role: "edit".to_owned(),
                    text: "1".to_owned(),
                    x_dip: 8,
                    y_dip: 64,
                    width_dip: 120,
                    height_dip: 32,
                    span_start: 0,
                    span_end: 0,
                },
                Element {
                    id: "window.add".to_owned(),
                    role: "button".to_owned(),
                    text: "Add".to_owned(),
                    x_dip: 136,
                    y_dip: 64,
                    width_dip: 80,
                    height_dip: 32,
                    span_start: 0,
                    span_end: 0,
                },
                Element {
                    id: "state.count".to_owned(),
                    role: "result".to_owned(),
                    text: "0".to_owned(),
                    x_dip: 8,
                    y_dip: 112,
                    width_dip: 160,
                    height_dip: 32,
                    span_start: 0,
                    span_end: 0,
                },
            ],
            capabilities: Capabilities {
                copy: true,
                paste: false,
                cut: "default".to_owned(),
                filesystem: "none".to_owned(),
                network: false,
                subprocess: false,
                web_content: false,
            },
        }
    }

    fn counter_json() -> String {
        serde_json::to_string(&counter_ir()).expect("counter serializes")
    }

    #[test]
    fn backend_routing_prefers_an_explicit_env_var() {
        assert_eq!(backend_for(Some("wayland")), Backend::Wayland);
        assert_eq!(backend_for(Some("kumimanu")), Backend::Kumimanu);
    }

    // The feature decides only when `EMPEROR_WINDOW_BACKEND` does not name a
    // known backend. Each `cfg` variant of `backend_for` compiles with one
    // default; running the suite with and without `--features kumimanu` covers
    // both.
    #[test]
    #[cfg(not(feature = "kumimanu"))]
    fn backend_default_is_wayland_without_the_feature() {
        assert_eq!(backend_for(None), Backend::Wayland);
        assert_eq!(backend_for(Some("other")), Backend::Wayland);
    }

    #[test]
    #[cfg(feature = "kumimanu")]
    fn backend_default_is_kumimanu_with_the_feature() {
        assert_eq!(backend_for(None), Backend::Kumimanu);
        assert_eq!(backend_for(Some("other")), Backend::Kumimanu);
    }

    #[test]
    fn oracle_stores_zero_plus_one() {
        let report = oracle(&counter_json()).expect("oracle");
        assert_eq!(report.count, 1);
        assert_eq!(report.revision, 1);
        assert!(report.copy_allowed);
        assert!(!report.paste_allowed);
        assert_eq!(report.presented, 0);
        assert_eq!(report.wayland, "not-requested");
    }

    #[test]
    fn two_activations_store_count_two() {
        let report = activate_add(&counter_json(), 2).expect("two adds");
        assert_eq!(report.count, 2);
        assert_eq!(report.revision, 2);
        assert!(!report.paste_allowed);
        assert_eq!(report.presented, 0);
        assert_eq!(report.wayland, "not-requested");
    }

    #[test]
    fn unknown_field_is_rejected() {
        let mut value = serde_json::to_value(counter_ir()).expect("value");
        value
            .as_object_mut()
            .expect("object")
            .insert("extra".to_owned(), serde_json::json!(true));
        let json = serde_json::to_string(&value).expect("json");
        assert!(oracle(&json).is_err());
    }

    #[test]
    fn wrong_schema_is_rejected() {
        let mut ir = counter_ir();
        ir.schema = "nope".to_owned();
        let json = serde_json::to_string(&ir).expect("json");
        let err = oracle(&json).expect_err("schema");
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn denied_paste_leaves_the_edit_at_one() {
        assert!(paste_is_ignored(&counter_json()).expect("paste"));
    }

    #[test]
    fn privileged_filesystem_is_rejected() {
        let mut ir = counter_ir();
        ir.capabilities.filesystem = "deny".to_owned();
        let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("filesystem");
        assert!(err.contains("filesystem"), "{err}");
    }

    #[test]
    fn negative_extent_is_rejected() {
        let mut ir = counter_ir();
        ir.elements[1].width_dip = -120;
        let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("width");
        assert!(err.contains("window.amount"), "{err}");
    }

    #[test]
    fn oversized_geometry_is_rejected() {
        let mut ir = counter_ir();
        ir.elements[2].x_dip = MAX_DIP + 1;
        let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("position");
        assert!(err.contains("window.add"), "{err}");
        let mut ir = counter_ir();
        ir.elements[2].height_dip = MAX_DIP + 1;
        let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("extent");
        assert!(err.contains("window.add"), "{err}");
    }

    #[test]
    fn oversized_window_is_rejected() {
        let mut ir = counter_ir();
        ir.width_dip = MAX_DIP + 1;
        let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("window");
        assert!(err.contains("window size"), "{err}");
    }

    #[test]
    fn more_elements_than_sash_cells_is_rejected() {
        let mut ir = counter_ir();
        ir.elements.resize(MAX_ELEMENTS + 1, ir.elements[0].clone());
        let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("count");
        assert!(err.contains("33"), "{err}");
    }

    #[test]
    fn canonical_json_keeps_field_order() {
        let json = counter_json();
        assert!(json.starts_with(
            "{\"schema\":\"GY-EMP-NIR/1\",\"profile\":\"emperor.linux.interactive/0.0.1\","
        ));
    }

    const ELEMENTS: &[CompiledElement] = &[
        CompiledElement {
            id: "window.root",
            role: "client",
            text: "Count",
            x_dip: 0,
            y_dip: 0,
            width_dip: 320,
            height_dip: 160,
            span_start: 0,
            span_end: 0,
        },
        CompiledElement {
            id: "window.amount",
            role: "edit",
            text: "1",
            x_dip: 8,
            y_dip: 64,
            width_dip: 120,
            height_dip: 32,
            span_start: 0,
            span_end: 0,
        },
        CompiledElement {
            id: "window.add",
            role: "button",
            text: "Add",
            x_dip: 136,
            y_dip: 64,
            width_dip: 80,
            height_dip: 32,
            span_start: 0,
            span_end: 0,
        },
        CompiledElement {
            id: "state.count",
            role: "result",
            text: "0",
            x_dip: 8,
            y_dip: 112,
            width_dip: 160,
            height_dip: 32,
            span_start: 0,
            span_end: 0,
        },
    ];

    #[test]
    fn compiled_scene_matches_its_json() {
        let json = Box::leak(counter_json().into_boxed_str());
        let scene = CompiledScene {
            nir_json: json,
            schema: SCHEMA,
            profile: PROFILE,
            source_digest_sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            scene: SCENE,
            action_id: "action.add",
            state_id: "count",
            input_id: "amount",
            initial_state: 0,
            initial_input: 1,
            width_dip: 320,
            height_dip: 160,
            copy: true,
            paste: false,
            cut: "default",
            filesystem: "none",
            network: false,
            subprocess: false,
            web_content: false,
            elements: ELEMENTS,
        };
        assert_scene_matches(&scene).expect("match");
        let mut mismatched = scene.clone();
        mismatched.copy = false;
        assert!(assert_scene_matches(&mismatched).is_err());
    }

    #[test]
    fn interaction_script_matches_the_admitted_counter() {
        let ir = counter_ir();
        assert_eq!(ir.schema, SCHEMA);
        assert_eq!(ir.capabilities.filesystem, "none");
        assert!(ir.capabilities.copy);
        assert!(!ir.capabilities.paste);
        assert_eq!(ir.state_id, "count");
        assert_eq!(ir.input_id, "amount");
        let add = ir
            .elements
            .iter()
            .find(|element| element.id == "window.add")
            .expect("add");
        assert_eq!(
            (add.x_dip, add.y_dip, add.width_dip, add.height_dip),
            (136, 64, 80, 32)
        );
        let report = exercise_interaction(&counter_json()).expect("interaction");
        assert_eq!(
            report,
            InteractionReport {
                count: 3,
                revision: 2,
                edit: "2".to_owned(),
                focus: "button".to_owned(),
                paste_ignored: true,
                copy_allowed: true,
                paste_allowed: false,
                add_hit: true,
                rects_stable: true,
                space_activates: true,
            }
        );
    }

    #[test]
    fn denied_paste_ignores_shortcut_empty_and_payload() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert!(!host::paste_shortcut_requests_clipboard(&ir, &controller));
        assert_eq!(controller.session.edit, "1");
        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("9")),
            host::Step::Unchanged
        );
        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("")),
            host::Step::Unchanged
        );
        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, None),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, "1");

        let (mut allowed, mut controller) = open_admitted(&counter_json()).expect("admit");
        allowed.capabilities.paste = true;
        assert!(host::paste_shortcut_requests_clipboard(
            &allowed,
            &controller
        ));
        assert_eq!(
            host::on_clipboard_text(&allowed, &mut controller, None),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, "1");
        assert_eq!(
            host::on_clipboard_text(&allowed, &mut controller, Some("")),
            host::Step::Changed
        );
        assert_eq!(controller.session.edit, "");
        assert_eq!(
            host::on_clipboard_text(&allowed, &mut controller, Some("9")),
            host::Step::Changed
        );
        assert_eq!(controller.session.edit, "9");

        let (mut button_focus, mut controller) = open_admitted(&counter_json()).expect("admit");
        button_focus.capabilities.paste = true;
        controller.focus = goya_parity::keys::Focus::Button;
        assert!(!host::paste_shortcut_requests_clipboard(
            &button_focus,
            &controller
        ));
        assert_eq!(controller.session.edit, "1");
    }

    #[test]
    fn paste_payload_applies_after_focus_moves() {
        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        ir.capabilities.paste = true;
        assert!(host::paste_shortcut_requests_clipboard(&ir, &controller));

        controller.focus = goya_parity::keys::Focus::Button;
        assert!(!host::paste_shortcut_requests_clipboard(&ir, &controller));
        assert_eq!(controller.session.edit, "1");

        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("9")),
            host::Step::Changed
        );
        assert_eq!(controller.session.edit, "9");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("")),
            host::Step::Changed
        );
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);

        controller.session.set_edit("1");
        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, None),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert!(!ir.capabilities.paste);
        controller.focus = goya_parity::keys::Focus::Button;
        assert!(!host::paste_shortcut_requests_clipboard(&ir, &controller));
        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("9")),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
    }

    #[test]
    fn paste_payload_is_not_rewritten() {
        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        ir.capabilities.paste = true;
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("00")),
            host::Step::Changed
        );
        assert_eq!(controller.session.edit, "00");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_ne!(controller.session.edit, controller.session.result);

        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("\n")),
            host::Step::Changed
        );
        assert_eq!(controller.session.edit, "\n");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("1\u{200D}")),
            host::Step::Changed
        );
        assert_eq!(controller.session.edit, "1\u{200D}");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert!(!ir.capabilities.paste);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("00")),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, "1");
        assert_eq!(
            host::on_clipboard_text(&ir, &mut controller, Some("\n")),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
    }

    #[test]
    fn shift_delete_deny_does_not_change_the_edit() {
        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        ir.capabilities.cut = "deny".to_owned();
        controller.session.set_edit("12");
        let step = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Delete,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(step.step, host::Step::Unchanged);
        assert!(step.clipboard.is_none());
        assert_eq!(controller.session.edit, "12");
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn button_release_inside_activates_and_outside_does_not() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        let outside = host::on_pointer_release(&mut controller, &hits, &mut arm, 8.0, 8.0);
        assert_eq!(outside.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.step, host::Step::Unchanged);
        let inside = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(inside.hit.as_deref(), Some("window.add"));
        assert_eq!(inside.step, host::Step::Changed);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.edit, "1");
    }

    #[test]
    fn release_without_a_position_does_not_activate() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        let outside = host::on_pointer_release(&mut controller, &hits, &mut arm, -1.0, -1.0);
        assert_eq!(outside.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn edit_press_focuses_without_activating() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 68.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.amount"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 68.0, 80.0);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn result_press_does_not_move_focus() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 88.0, 128.0);
        assert_eq!(press.hit.as_deref(), Some("state.count"));
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 88.0, 128.0);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 88.0, 128.0);
        assert_eq!(press.hit.as_deref(), Some("state.count"));
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 8.0, 8.0);
        assert_eq!(press.hit.as_deref(), Some("window.root"));
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn outside_client_press_does_not_move_focus() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, -1.0, 0.0);
        assert_eq!(press.hit.as_deref(), None);
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 320.0, 80.0);
        assert_eq!(press.hit.as_deref(), None);
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 8.0, 160.0);
        assert_eq!(press.hit.as_deref(), None);
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn non_finite_press_stays_inside_the_client() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let press =
            host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, f64::NAN, f64::NAN);
        assert_eq!(press.hit.as_deref(), Some("window.root"));
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");
        assert!(press.hit.is_some());

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let press =
            host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, f64::INFINITY, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.root"));
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);

        let press = host::on_pointer_press(
            &ir,
            &mut controller,
            &hits,
            &mut arm,
            80.0,
            f64::NEG_INFINITY,
        );
        assert_eq!(press.hit.as_deref(), Some("window.root"));
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn finite_press_truncates_toward_zero() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 215.9, 95.9);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(press.step, host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, -0.9, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.root"));
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(
            &ir,
            &mut controller,
            &hits,
            &mut arm,
            f64::from(i32::MAX) + 1.0,
            80.0,
        );
        assert_eq!(press.hit.as_deref(), None);
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 215.9, 95.9);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);

        let press = host::on_pointer_press(
            &ir,
            &mut controller,
            &hits,
            &mut arm,
            f64::from(i32::MIN) - 1.0,
            80.0,
        );
        assert_eq!(press.hit.as_deref(), None);
        assert_eq!(press.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn release_truncates_toward_zero() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 215.9, 95.9);
        assert_eq!(release.hit.as_deref(), Some("window.add"));
        assert_eq!(release.step, host::Step::Changed);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 135.9, 80.0);
        assert_eq!(release.hit.as_deref(), Some("window.root"));
        assert_ne!(release.hit.as_deref(), None);
        assert_ne!(release.hit.as_deref(), Some("window.add"));
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 63.9);
        assert_eq!(release.hit.as_deref(), Some("window.root"));
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    }

    #[test]
    fn non_finite_release_stays_inside_the_client() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let release =
            host::on_pointer_release(&mut controller, &hits, &mut arm, f64::NAN, f64::NAN);
        assert_eq!(release.hit.as_deref(), Some("window.root"));
        assert_ne!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let release =
            host::on_pointer_release(&mut controller, &hits, &mut arm, f64::NAN, f64::NAN);
        assert_eq!(release.hit.as_deref(), Some("window.root"));
        assert_ne!(release.hit.as_deref(), None);
        assert_ne!(release.hit.as_deref(), Some("window.add"));
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release =
            host::on_pointer_release(&mut controller, &hits, &mut arm, f64::INFINITY, 80.0);
        assert_eq!(release.hit.as_deref(), Some("window.root"));
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release =
            host::on_pointer_release(&mut controller, &hits, &mut arm, 80.0, f64::NEG_INFINITY);
        assert_eq!(release.hit.as_deref(), Some("window.root"));
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn release_truncates_to_a_miss() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, -1.9, 80.0);
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 320.9, 80.0);
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 80.0, 160.9);
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, -1.9, 80.0);
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 319.9, 159.9);
        assert_eq!(release.hit.as_deref(), Some("window.root"));
        assert_ne!(release.hit.as_deref(), None);
        assert_ne!(release.hit.as_deref(), Some("window.add"));
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn saturated_release_is_a_miss() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let release = host::on_pointer_release(
            &mut controller,
            &hits,
            &mut arm,
            f64::from(i32::MAX) + 1.0,
            80.0,
        );
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(
            &mut controller,
            &hits,
            &mut arm,
            f64::from(i32::MAX) + 1.0,
            80.0,
        );
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(
            &mut controller,
            &hits,
            &mut arm,
            f64::from(i32::MIN) - 1.0,
            80.0,
        );
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(
            &mut controller,
            &hits,
            &mut arm,
            80.0,
            f64::from(i32::MAX) + 1.0,
        );
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(
            &mut controller,
            &hits,
            &mut arm,
            80.0,
            f64::from(i32::MIN) - 1.0,
        );
        assert_eq!(release.hit.as_deref(), None);
        assert_eq!(release.step, host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn second_truncated_release_does_not_store() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
        let mut arm = host::PointerArm::default();
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(press.hit.as_deref(), Some("window.add"));
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, 0);

        let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 215.9, 95.9);
        assert_eq!(release.hit.as_deref(), Some("window.add"));
        assert_eq!(release.step, host::Step::Changed);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);

        let second = host::on_pointer_release(&mut controller, &hits, &mut arm, 215.9, 95.9);
        assert_eq!(second.hit.as_deref(), Some("window.add"));
        assert_ne!(second.hit.as_deref(), None);
        assert_eq!(second.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);

        let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
        assert_eq!(late.hit.as_deref(), Some("window.add"));
        assert_eq!(late.step, host::Step::Unchanged);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    }

    #[test]
    fn space_on_the_edit_does_not_insert() {
        let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(host::on_char(&mut controller, ' '), host::Step::Unchanged);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    }

    #[test]
    fn space_on_the_button_activates() {
        let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(host::on_char(&mut controller, ' '), host::Step::Changed);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    }

    #[test]
    fn admitted_cut_publishes_the_edit_and_clears_it() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(ir.capabilities.cut, "default");
        let effect = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Delete,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(effect.step, host::Step::Changed);
        assert_eq!(effect.clipboard.as_deref(), Some("1"));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.focus = goya_parity::keys::Focus::Button;
        let effect = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Delete,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "1");
    }

    #[test]
    fn delete_chords_that_are_not_cuts() {
        // Cut is Shift+Delete without ctrl. `allow` matches `default`.
        // Plain Delete, Ctrl+Delete, and Ctrl+Shift+Delete copy nothing.
        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        ir.capabilities.cut = "allow".to_owned();
        controller.session.set_edit("12");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        let count = controller.session.count;
        let revision = controller.session.revision;

        let shift_delete = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Delete,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(shift_delete.step, host::Step::Changed);
        assert_eq!(shift_delete.clipboard.as_deref(), Some("12"));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, count);
        assert_eq!(controller.session.revision, revision);

        controller.session.set_edit("12");
        let ctrl_shift_delete = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Delete,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: true,
            },
        );
        assert_eq!(ctrl_shift_delete.step, host::Step::Unchanged);
        assert!(ctrl_shift_delete.clipboard.is_none());
        assert_eq!(controller.session.edit, "12");
        assert_eq!(controller.session.count, count);
        assert_eq!(controller.session.revision, revision);

        let ctrl_delete = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Delete,
            goya_wayland::KeyModifiers {
                shift: false,
                ctrl: true,
            },
        );
        assert_eq!(ctrl_delete.step, host::Step::Unchanged);
        assert!(ctrl_delete.clipboard.is_none());
        assert_eq!(controller.session.edit, "12");
        assert_eq!(controller.session.count, count);
        assert_eq!(controller.session.revision, revision);

        let plain_delete = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Delete,
            goya_wayland::KeyModifiers {
                shift: false,
                ctrl: false,
            },
        );
        assert_eq!(plain_delete.step, host::Step::Unchanged);
        assert!(plain_delete.clipboard.is_none());
        assert_eq!(controller.session.edit, "12");
        assert_eq!(controller.session.count, count);
        assert_eq!(controller.session.revision, revision);

        controller.focus = goya_parity::keys::Focus::Button;
        let button_shift_delete = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Delete,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(button_shift_delete.step, host::Step::Unchanged);
        assert!(button_shift_delete.clipboard.is_none());
        assert_eq!(controller.session.edit, "12");
        assert_eq!(controller.session.count, count);
        assert_eq!(controller.session.revision, revision);
    }

    #[test]
    fn resize_dirties_without_moving_dip_rects() {
        let (ir, _controller) = open_admitted(&counter_json()).expect("admit");
        let before = ir.elements.clone();
        assert_eq!(host::on_resize(&ir), host::Step::Changed);
        assert_eq!(ir.elements, before);
    }

    #[test]
    fn copy_text_follows_the_capability() {
        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(
            host::copy_text(&ir, &controller),
            Some(controller.session.edit.as_str())
        );
        ir.capabilities.copy = false;
        assert_eq!(host::copy_text(&ir, &controller), None);
        assert_eq!(controller.session.edit, "1");
        ir.capabilities.copy = true;
        controller.focus = goya_parity::keys::Focus::Button;
        assert_eq!(host::copy_text(&ir, &controller), None);
        assert_eq!(controller.session.edit, "1");
    }

    #[test]
    fn empty_edit_copy_offers_an_empty_string() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert!(ir.capabilities.copy);
        assert_eq!(ir.capabilities.cut, "default");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        controller.session.set_edit("");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "");

        assert_eq!(host::copy_text(&ir, &controller), Some(""));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.session.set_edit("");
        controller.focus = goya_parity::keys::Focus::Button;
        assert_eq!(host::copy_text(&ir, &controller), None);
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);

        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.session.set_edit("");
        ir.capabilities.copy = false;
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(host::copy_text(&ir, &controller), None);
        assert_eq!(controller.session.edit, "");
    }

    #[test]
    fn noncanonical_edit_is_the_clipboard_text() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert!(ir.capabilities.copy);
        assert_eq!(ir.capabilities.cut, "default");

        controller.session.set_edit("00");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "00");

        assert_eq!(host::copy_text(&ir, &controller), Some("00"));
        assert_eq!(controller.session.edit, "00");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Changed);
        assert_eq!(effect.clipboard.as_deref(), Some("00"));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.session.set_edit("-0");
        assert_eq!(host::copy_text(&ir, &controller), Some("-0"));
        assert_eq!(controller.session.result, "0");
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Changed);
        assert_eq!(effect.clipboard.as_deref(), Some("-0"));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.session.set_edit("00");
        controller.focus = goya_parity::keys::Focus::Button;
        assert_eq!(host::copy_text(&ir, &controller), None);
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "00");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.session.set_edit("00");
        ir.capabilities.copy = false;
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(host::copy_text(&ir, &controller), None);
        assert_eq!(controller.session.edit, "00");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
    }

    #[test]
    fn on_cut_clears_a_focused_edit_unless_blocked() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(ir.capabilities.cut, "default");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        let count = controller.session.count;
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Changed);
        assert_eq!(effect.clipboard.as_deref(), Some("1"));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, count);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.focus = goya_parity::keys::Focus::Button;
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);

        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        ir.capabilities.cut = "deny".to_owned();
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);

        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.session.set_edit("");
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn cut_ignores_the_copy_flag() {
        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert!(ir.capabilities.copy);
        assert_eq!(ir.capabilities.cut, "default");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        ir.capabilities.copy = false;
        assert_eq!(host::copy_text(&ir, &controller), None);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Changed);
        assert_eq!(effect.clipboard.as_deref(), Some("1"));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        controller.session.set_edit("00");
        ir.capabilities.copy = false;
        assert_eq!(ir.capabilities.cut, "default");
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Changed);
        assert_eq!(effect.clipboard.as_deref(), Some("00"));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        ir.capabilities.copy = false;
        ir.capabilities.cut = "allow".to_owned();
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Changed);
        assert_eq!(effect.clipboard.as_deref(), Some("1"));
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        ir.capabilities.copy = false;
        ir.capabilities.cut = "deny".to_owned();
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        ir.capabilities.copy = false;
        assert_eq!(ir.capabilities.cut, "default");
        controller.focus = goya_parity::keys::Focus::Button;
        let effect = host::on_cut(&ir, &mut controller);
        assert_eq!(effect.step, host::Step::Unchanged);
        assert!(effect.clipboard.is_none());
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn ime_events_do_not_change_the_edit() {
        let (_ir, controller) = open_admitted(&counter_json()).expect("admit");
        let edit = controller.session.edit.clone();
        let count = controller.session.count;
        assert_eq!(host::ignore_ime(&controller), host::Step::Unchanged);
        assert_eq!(controller.session.edit, edit);
        assert_eq!(controller.session.count, count);
    }

    #[test]
    fn focused_frame_uses_the_live_session() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(goya_wayland::MP_PER_PX, 250);
        let scale = goya_wayland::MP_PER_PX;
        let rect = |x: i32, y: i32, width: i32, height: i32| goya_layout::Rect {
            x: i64::from(x) * scale,
            y: i64::from(y) * scale,
            width: i64::from(width) * scale,
            height: i64::from(height) * scale,
        };
        let client = rect(0, 0, 320, 160);
        let edit = rect(8, 64, 120, 32);
        let button = rect(136, 64, 80, 32);
        let result = rect(8, 112, 160, 32);
        let focus_fill = Some(0xFFD6E4F5);

        let expect_frame = |controller: &goya_parity::keys::KeyController,
                            fills: [Option<u32>; 5]| {
            let frame = host::scene_frame(&ir, controller).expect("frame");
            assert_eq!(
                frame.primitives,
                vec![
                    goya_runtime::Primitive::Panel(client),
                    goya_runtime::Primitive::Text(client, "Count".to_owned()),
                    goya_runtime::Primitive::Text(edit, controller.session.edit.clone()),
                    goya_runtime::Primitive::Button(button, "Add".to_owned()),
                    goya_runtime::Primitive::Text(result, controller.session.result.clone()),
                ]
            );
            assert_eq!(frame.fills.as_slice(), fills);
            assert_eq!(frame.foregrounds, vec![None; 5]);
            assert_eq!(frame.fills.iter().filter(|fill| fill.is_some()).count(), 1);
            assert!(frame.fills[0].is_none());
            assert!(frame.fills[1].is_none());
            assert!(frame.fills[4].is_none());
            frame
        };

        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        expect_frame(&controller, [None, None, focus_fill, None, None]);

        assert_eq!(host::on_char(&mut controller, ' '), host::Step::Unchanged);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        expect_frame(&controller, [None, None, focus_fill, None, None]);

        assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        expect_frame(&controller, [None, None, None, focus_fill, None]);

        assert_eq!(host::on_tab(&mut controller, true), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.result, "1");
        let frame = expect_frame(&controller, [None, None, focus_fill, None, None]);
        match &frame.primitives[2] {
            goya_runtime::Primitive::Text(rect, text) => {
                assert_eq!(rect, &edit);
                assert_eq!(text, "1");
            }
            other => panic!("edit primitive was {other:?}"),
        }
        match &frame.primitives[4] {
            goya_runtime::Primitive::Text(rect, text) => {
                assert_eq!(rect, &result);
                assert_eq!(text, "1");
            }
            other => panic!("result primitive was {other:?}"),
        }

        controller.session.set_edit("e\u{0301}x");
        let count = controller.session.count;
        let revision = controller.session.revision;
        let result_text = controller.session.result.clone();
        assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
        assert_eq!(controller.session.edit, "e\u{0301}");
        assert_eq!(controller.session.count, count);
        assert_eq!(controller.session.revision, revision);
        assert_eq!(controller.session.result, result_text);
        let frame = expect_frame(&controller, [None, None, focus_fill, None, None]);
        match &frame.primitives[2] {
            goya_runtime::Primitive::Text(_, text) => assert_eq!(text, &controller.session.edit),
            other => panic!("edit primitive was {other:?}"),
        }

        assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        let edit_text = controller.session.edit.clone();
        assert_eq!(
            host::on_char(&mut controller, '\u{8}'),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, edit_text);
        assert_eq!(controller.session.count, count);
        assert_eq!(controller.session.revision, revision);
        assert_eq!(controller.session.result, result_text);
        let frame = expect_frame(&controller, [None, None, None, focus_fill, None]);
        match &frame.primitives[2] {
            goya_runtime::Primitive::Text(_, text) => assert_eq!(text, &controller.session.edit),
            other => panic!("edit primitive was {other:?}"),
        }
    }

    #[test]
    fn digits_append_only_on_the_edit() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");

        assert_eq!(host::on_char(&mut controller, '2'), host::Step::Changed);
        assert_eq!(controller.session.edit, "12");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(host::on_char(&mut controller, 'é'), host::Step::Changed);
        assert!(controller.session.edit.ends_with('é'));
        assert_eq!(controller.session.edit, "12é");
        assert_eq!(controller.session.count, 0);

        let edit = controller.session.edit.clone();
        assert_eq!(host::on_char(&mut controller, '\t'), host::Step::Unchanged);
        assert_eq!(host::on_char(&mut controller, '\r'), host::Step::Unchanged);
        assert_eq!(controller.session.edit, edit);

        let to_button = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Tab,
            goya_wayland::KeyModifiers {
                shift: false,
                ctrl: true,
            },
        );
        assert_eq!(to_button.step, host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.edit, edit);
        assert_eq!(controller.session.count, 0);

        assert_eq!(host::on_char(&mut controller, '3'), host::Step::Unchanged);
        assert_eq!(controller.session.edit, edit);

        let to_edit = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Tab,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: true,
            },
        );
        assert_eq!(to_edit.step, host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, edit);
        assert_eq!(controller.session.count, 0);

        controller.session.set_edit("1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(controller.session.edit, "1");
    }

    #[test]
    fn delete_does_not_append_and_backspace_drops_one_cluster() {
        let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(
            host::on_char(&mut controller, '\u{7f}'),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(
            host::on_char(&mut controller, '\u{200D}'),
            host::Step::Changed
        );
        assert_eq!(controller.session.edit, "1\u{200D}");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        controller.session.set_edit("e\u{0301}");
        assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

        controller.session.set_edit("1");
        assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(
            host::on_char(&mut controller, '\u{7f}'),
            host::Step::Unchanged
        );
        assert_eq!(
            host::on_char(&mut controller, '\u{200D}'),
            host::Step::Unchanged
        );
        assert_eq!(
            host::on_char(&mut controller, '\u{8}'),
            host::Step::Unchanged
        );
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    }

    #[test]
    fn shift_tab_on_the_edit_wraps_to_the_button() {
        let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let to_button = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Tab,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(to_button.step, host::Step::Changed);
        assert!(to_button.clipboard.is_none());
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        let back_to_edit = host::on_modified_special(
            &ir,
            &mut controller,
            goya_wayland::KeySpecial::Tab,
            goya_wayland::KeyModifiers {
                shift: true,
                ctrl: false,
            },
        );
        assert_eq!(back_to_edit.step, host::Step::Changed);
        assert!(back_to_edit.clipboard.is_none());
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);

        assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
    }

    #[test]
    fn rejected_edit_does_not_store() {
        let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

        assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
        assert_eq!(controller.session.edit, "");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.edit, "");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(host::on_char(&mut controller, '0'), host::Step::Changed);
        assert_eq!(host::on_char(&mut controller, '0'), host::Step::Changed);
        assert_eq!(controller.session.edit, "00");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.edit, "00");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        controller.session.set_edit("1");
        assert_eq!(host::on_char(&mut controller, ','), host::Step::Changed);
        assert_eq!(controller.session.edit, "1,");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.edit, "1,");
        assert_eq!(controller.session.count, 0);

        controller.session.set_edit("");
        assert_eq!(host::on_char(&mut controller, '-'), host::Step::Changed);
        assert_eq!(host::on_char(&mut controller, '0'), host::Step::Changed);
        assert_eq!(controller.session.edit, "-0");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.edit, "-0");
        assert_eq!(controller.session.count, 0);

        let overflow = "9223372036854775808";
        controller.session.set_edit(overflow);
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.edit, overflow);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        controller.session.set_edit("1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(controller.session.edit, "1");

        assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(host::on_char(&mut controller, '0'), host::Step::Unchanged);
        assert_eq!(controller.session.edit, "1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
        assert_eq!(controller.session.count, 2);
        assert_eq!(controller.session.revision, 2);
        assert_eq!(controller.session.result, "2");
        assert_eq!(controller.session.edit, "1");
    }

    #[test]
    fn checked_add_overflow_does_not_store() {
        let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

        controller.session.count = i64::MAX;
        assert_eq!(controller.session.edit, "1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.count, i64::MAX);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "1");

        // Enter is not focus-gated. This failure is the addition.
        assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
        assert_eq!(controller.session.count, i64::MAX);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "1");
        assert_eq!(host::on_tab(&mut controller, true), host::Step::Changed);
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, i64::MAX);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "1");

        controller.session.count = i64::MIN;
        controller.session.set_edit("-1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.count, i64::MIN);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "-1");

        controller.session.count = 0;
        controller.session.revision = u64::MAX;
        controller.session.set_edit("1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, u64::MAX);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "1");

        // i64::MAX plus 0 stores. The count is not rejected by itself.
        controller.session.count = i64::MAX;
        controller.session.revision = 0;
        controller.session.set_edit("0");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
        assert_eq!(controller.session.count, i64::MAX);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "9223372036854775807");
        assert_eq!(controller.session.edit, "0");

        controller.session.count = 0;
        controller.session.revision = 0;
        controller.session.set_edit("1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(controller.session.edit, "1");

        assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
        assert_eq!(controller.session.edit, "");
        assert_eq!(host::on_char(&mut controller, '-'), host::Step::Changed);
        assert_eq!(host::on_char(&mut controller, '1'), host::Step::Changed);
        assert_eq!(controller.session.edit, "-1");
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, "1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 2);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "-1");
    }

    #[test]
    fn canonical_i64_min_stores() {
        let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");
        assert_eq!(controller.session.edit, "1");

        let min_text = i64::MIN.to_string();
        assert_eq!(min_text, "-9223372036854775808");
        assert_eq!(min_text.len(), 20);

        controller.session.set_edit(&min_text);
        assert_eq!(controller.session.count, 0);
        assert_eq!(controller.session.revision, 0);
        assert_eq!(controller.session.result, "0");

        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
        assert_eq!(controller.session.count, i64::MIN);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, min_text);
        assert_eq!(controller.session.edit, min_text);

        controller.session.set_edit("-1");
        assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
        assert_eq!(controller.session.count, i64::MIN);
        assert_eq!(controller.session.revision, 1);
        assert_eq!(controller.session.result, min_text);
        assert_eq!(controller.session.edit, "-1");
        assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    }

    #[test]
    #[ignore = "opens one Wayland window and presents the scripted counter"]
    fn exercise_presents_the_scripted_counter() {
        let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
        assert!(!display.is_empty(), "WAYLAND_DISPLAY is unset");
        let report = run(&counter_json(), RunMode::Exercise).expect("window");
        assert_eq!(report.wayland, "opened");
        assert!(report.presented >= 1, "{report:?}");
        assert_eq!(report.count, 3);
        assert_eq!(report.revision, 2);
        assert!(report.copy_allowed);
        assert!(!report.paste_allowed);
    }

    #[cfg(feature = "kumimanu")]
    #[test]
    #[ignore = "opens one X11 window; run with --ignored when DISPLAY is set"]
    fn kumimanu_counter_stores_twelve_from_sent_input() {
        let display = std::env::var("DISPLAY").unwrap_or_default();
        assert!(!display.is_empty(), "DISPLAY is unset");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(super::inject_count_window());
        });
        if let Ok(elf) = std::env::var("KUMIMANU_ELF") {
            let output = std::process::Command::new(&elf)
                .args(["--live", "--frames", "700"])
                .output()
                .unwrap_or_else(|err| panic!("could not start {elf}: {err}"));
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success(),
                "elf exited {:?}\n{stdout}\n{stderr}",
                output.status.code()
            );
            assert!(stdout.contains("window=kumimanu"), "{stdout}");
            assert!(stdout.contains("count=12 "), "{stdout}");
            assert!(stdout.contains("revision=1 "), "{stdout}");
            assert!(stdout.contains("edit=12"), "{stdout}");
            assert!(!stdout.contains("presented=0 "), "{stdout}");
        } else {
            let (ir, controller) = open_admitted(&counter_json()).expect("admit");
            let report = host_kumimanu::run_window(
                &ir,
                controller,
                host::WindowMode::Live {
                    max_frames: Some(500),
                },
            )
            .expect("window");
            assert_eq!(report.window, "kumimanu");
            assert_eq!(report.wayland, "not-used");
            assert_eq!(report.edit, "12");
            assert_eq!(report.count, 12);
            assert_eq!(report.revision, 1);
            assert!(report.presented >= 1, "{report:?}");
        }
        let injected = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("injector finished");
        injected.expect("sent key and button");
    }
}

#[cfg(all(test, feature = "kumimanu"))]
fn inject_count_window() -> Result<(), String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt as _;
    use x11rb::rust_connection::RustConnection;

    let (connection, screen) = RustConnection::connect(None).map_err(|err| err.to_string())?;
    let (root, min, max) = {
        let setup = connection.setup();
        let root = setup
            .roots
            .get(screen)
            .ok_or("screen index is outside the setup")?
            .root;
        (root, setup.min_keycode, setup.max_keycode)
    };
    let count = max.saturating_sub(min).saturating_add(1);
    let mapping = connection
        .get_keyboard_mapping(min, count)
        .map_err(|err| err.to_string())?
        .reply()
        .map_err(|err| err.to_string())?;
    let per = usize::from(mapping.keysyms_per_keycode.max(1));
    let index = mapping
        .keysyms
        .chunks(per)
        .position(|syms| syms.contains(&0x32))
        .ok_or("no keycode for keysym 0x32")?;
    let keycode =
        min.saturating_add(u8::try_from(index).map_err(|_| "keycode index does not fit")?);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    let window = loop {
        if let Some(found) = find_count_window(&connection, root, 0)? {
            break found;
        }
        if std::time::Instant::now() >= deadline {
            return Err("timed out waiting for a Count window with class Kumimanu".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    };
    send_key(&connection, root, window, keycode, true)?;
    send_key(&connection, root, window, keycode, false)?;
    send_button(&connection, root, window, 176, 80, true)?;
    send_button(&connection, root, window, 176, 80, false)?;
    configure_smaller(&connection, window)?;
    connection.flush().map_err(|err| err.to_string())
}

#[cfg(all(test, feature = "kumimanu"))]
fn configure_smaller(
    connection: &x11rb::rust_connection::RustConnection,
    window: u32,
) -> Result<(), String> {
    use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _};

    connection
        .configure_window(window, &ConfigureWindowAux::new().width(300).height(140))
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(all(test, feature = "kumimanu"))]
fn find_count_window(
    connection: &x11rb::rust_connection::RustConnection,
    window: u32,
    depth: u32,
) -> Result<Option<u32>, String> {
    use x11rb::protocol::xproto::ConnectionExt as _;

    if depth > 8 {
        return Ok(None);
    }
    if depth > 0 && is_count_window(connection, window)? {
        return Ok(Some(window));
    }
    let reply = match connection.query_tree(window) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => reply,
            Err(_) => return Ok(None),
        },
        Err(_) => return Ok(None),
    };
    for child in reply.children {
        if let Some(found) = find_count_window(connection, child, depth + 1)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

#[cfg(all(test, feature = "kumimanu"))]
fn is_count_window(
    connection: &x11rb::rust_connection::RustConnection,
    window: u32,
) -> Result<bool, String> {
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

    let class =
        match connection.get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64) {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply.value,
                Err(_) => return Ok(false),
            },
            Err(err) => return Err(err.to_string()),
        };
    let name =
        match connection.get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 64) {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply.value,
                Err(_) => return Ok(false),
            },
            Err(err) => return Err(err.to_string()),
        };
    let kumimanu = class
        .split(|byte| *byte == 0)
        .any(|part| part == b"Kumimanu");
    Ok(kumimanu && name == b"Count")
}

#[cfg(all(test, feature = "kumimanu"))]
fn send_key(
    connection: &x11rb::rust_connection::RustConnection,
    root: u32,
    window: u32,
    keycode: u8,
    pressed: bool,
) -> Result<(), String> {
    use x11rb::protocol::xproto::{
        ConnectionExt as _, EventMask, KeyButMask, KeyPressEvent, KEY_PRESS_EVENT,
        KEY_RELEASE_EVENT,
    };

    let event = KeyPressEvent {
        response_type: if pressed {
            KEY_PRESS_EVENT
        } else {
            KEY_RELEASE_EVENT
        },
        detail: keycode,
        sequence: 0,
        time: 0,
        root,
        event: window,
        child: 0,
        root_x: 0,
        root_y: 0,
        event_x: 1,
        event_y: 1,
        state: KeyButMask::from(0u16),
        same_screen: true,
    };
    connection
        .send_event(false, window, EventMask::NO_EVENT, event)
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(all(test, feature = "kumimanu"))]
fn send_button(
    connection: &x11rb::rust_connection::RustConnection,
    root: u32,
    window: u32,
    x: i16,
    y: i16,
    pressed: bool,
) -> Result<(), String> {
    use x11rb::protocol::xproto::{
        ButtonPressEvent, ConnectionExt as _, EventMask, KeyButMask, BUTTON_PRESS_EVENT,
        BUTTON_RELEASE_EVENT,
    };

    let event = ButtonPressEvent {
        response_type: if pressed {
            BUTTON_PRESS_EVENT
        } else {
            BUTTON_RELEASE_EVENT
        },
        detail: 1,
        sequence: 0,
        time: 0,
        root,
        event: window,
        child: 0,
        root_x: x,
        root_y: y,
        event_x: x,
        event_y: y,
        state: KeyButMask::from(0u16),
        same_screen: true,
    };
    connection
        .send_event(false, window, EventMask::NO_EVENT, event)
        .map_err(|err| err.to_string())?;
    Ok(())
}
