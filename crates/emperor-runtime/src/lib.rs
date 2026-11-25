//! Linux runtime for one admitted counter (`emperor.linux.interactive/0.0.1`).
//!
//! Preedit and commit text are ignored. That is a limitation, not input-method
//! support. The default profile does not reflow on resize; an optional baked
//! desktop style can use proportional placement. The crate does not sandbox
//! the process and presents through the existing Wayland software host. It is not a Vulkan
//! renderer and it does not run any other profile.

#![forbid(unsafe_code)]

#[cfg(feature = "scene")]
pub mod scene;
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

/// A generated window chrome action emitted by the compiler metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowControl {
    Minimize,
    Maximize,
    Restore,
    Close,
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
    /// Window actions for which generated chrome should be shown.
    #[serde(default)]
    pub window_controls: Vec<WindowControl>,
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
    pub window_controls: &'static [WindowControl],
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
    /// Bound xdg-shell version for a presented Wayland window.
    pub xdg_wm_base_version: Option<u32>,
    /// Last compositor-committed toplevel state for that window.
    pub toplevel_state: Option<goya_wayland::ToplevelState>,
}

/// Desktop colours and resize behaviour compiled into an application.
/// The default Emperor path passes `None` and keeps its original presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopStyle {
    pub bg: [u8; 3],
    pub fg: [u8; 3],
    pub accent: [u8; 3],
    pub proportional: bool,
}

/// How [`run`] drives the admitted counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// One add, no window.
    Oracle,
    /// Run the scripted counter without opening a window.
    ExerciseOracle,
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
    run_with_style(nir_json, mode, None)
}

/// Run an admitted scene with an optional build-time desktop style.
pub fn run_with_style(
    nir_json: &str,
    mode: RunMode,
    style: Option<DesktopStyle>,
) -> Result<OracleReport, String> {
    let (ir, mut controller) = open_admitted(nir_json)?;
    match mode {
        RunMode::Oracle => {
            press_add(&mut controller);
            prove_paste_denied(&ir, &mut controller)?;
            Ok(report(&controller, 0, "not-requested"))
        }
        RunMode::ExerciseOracle => {
            let hits = goya_parity::pointer::hit_map(&ir.scene).map_err(|err| err.to_string())?;
            let mut arm = host::PointerArm::default();
            exercise::apply_visible_script(&ir, &mut controller, &hits, &mut arm)?;
            Ok(report(&controller, 0, "not-requested"))
        }
        RunMode::Smoke => open_window(&ir, controller, host::WindowMode::Smoke, style),
        RunMode::Exercise => open_window(&ir, controller, host::WindowMode::Exercise, style),
        RunMode::Live { max_frames } => open_window(
            &ir,
            controller,
            host::WindowMode::Live { max_frames },
            style,
        ),
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
    if scene.window_controls != ir.window_controls.as_slice() {
        return Err("window_controls does not match native ir".into());
    }
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
    let actual_digest = format!("{:x}", Sha256::digest(ir.scene.as_bytes()));
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
    let origins = goya_lang::parse_with_source_map(&ir.scene).map_err(|err| err.to_string())?;
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
        if !origins.entries.iter().any(|entry| {
            entry.kind == "property"
                && entry.name == "id"
                && entry.semantic_id.as_deref() == Some(element.id.as_str())
                && entry.range.start == element.span_start
                && entry.range.end == element.span_end
        }) {
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
    style: Option<DesktopStyle>,
) -> Result<OracleReport, String> {
    match backend_for(option_env!("EMPEROR_WINDOW_BACKEND")) {
        Backend::Kumimanu => open_kumimanu(ir, controller, mode),
        Backend::Wayland => host::run_window_with_style(ir, controller, mode, style),
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
        xdg_wm_base_version: None,
        toplevel_state: None,
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
#[path = "runtime_tests.rs"]
mod tests;

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
