//! Deterministic output for the experimental interactive profile.
//!
//! Files, all served from one origin over HTTP(S) (module scripts do not run
//! from `file:`):
//!
//! * `index.html`, `app.css`: the document and its static styles.
//! * `scene.js`: generated data — elements, initial state, activations.
//! * `layout.js`: static mirror of `goya_lang::layout` (no scroll), of
//!   `handoff::convert_field`, and of `goya_lang::desktop::place`.
//! * `d1.js`: the canonical D1 runtime (only when the scene has interactions).
//! * `storage.js`: the durable state adapter (only when a state key is
//!   `durable`), over `localStorage` or, when the scene declares it,
//!   IndexedDB (decision D11): restore before the first frame, write before
//!   commit.
//! * `app.js`: static shell — builds the DOM, lays it out, and runs the
//!   GSSH/2 activation protocol.
//! * `manifest.json`: hashes and source origins.

use maja_runtime::{emit_d1_runtime, D1Runtime};
use maja_semantic::{
    Diagnostic, DurableStore, InteractiveWebIr, WebCell, WebCellKind, WebCondition,
};
use sha2::{Digest, Sha256};
use std::{borrow::Cow, fmt::Write as _, fs, io, io::Write, path::Path};

/// Schema of `manifest.json`.
pub const MANIFEST_SCHEMA: &str = "MAJA-INTERACTIVE/1-draft";

/// Generated files in write order.
pub struct InteractivePackage {
    pub files: Vec<(&'static str, String)>,
}

/// Generate every file in memory. Fails only if the D1 runtime emitter
/// refuses the scene, or if `chrome` names an unknown library or overrides
/// controls the scene already authors. The diagnostic carries a source range
/// when the scene has one.
pub fn generate(
    ir: &InteractiveWebIr,
    chrome: Option<&str>,
) -> Result<InteractivePackage, Diagnostic> {
    let parsed = ir.handoff().parsed();
    let runtime = if parsed.scene.interactions.is_empty() {
        None
    } else {
        Some(emit_d1_runtime(parsed).map_err(|error| {
            Diagnostic {
                code: "MAJA-E0201",
                message: error.to_string(),
                range: parsed.scene.interactions[0]
                    .authored_id
                    .as_ref()
                    .and_then(|id| {
                        ir.handoff()
                            .origin(&goya_lang::handoff::Origin::Interaction(id.clone()))
                    }),
            }
        })?)
    };
    let graphics = bake_graphics(parsed)?;
    let scene = scene_js(ir, runtime.as_ref(), chrome, &graphics)?;
    let durable = ir.handoff().durable_keys().next().is_some();
    let app = format!(
        "{}\n{}\n{APP_JS}",
        if runtime.is_some() {
            "import { createRuntime } from \"./d1.js\";"
        } else {
            "const createRuntime = null;"
        },
        if durable {
            "import { restore, persister } from \"./storage.js\";"
        } else {
            "const restore = null, persister = null;"
        },
    );
    let index = if graphics.is_empty() {
        INDEX_HTML.to_owned()
    } else {
        INDEX_HTML.replace(
            "<script type=\"module\" src=\"app.js\"></script>",
            "<script src=\"gvi.js\"></script><script type=\"module\" src=\"app.js\"></script>",
        )
    };
    let mut files = vec![
        ("index.html", index),
        ("app.css", APP_CSS.to_owned()),
        ("scene.js", scene),
        ("layout.js", LAYOUT_JS.to_owned()),
    ];
    if let Some(runtime) = &runtime {
        files.push(("d1.js", runtime.javascript.clone()));
    }
    if durable {
        files.push(("storage.js", storage_js(ir.durable_store(), ir.migration())));
    }
    if !graphics.is_empty() {
        files.push(("gvi.js", include_str!("gvi.js").to_owned()));
    }
    files.push(("app.js", app));
    let manifest = manifest(ir, runtime.as_ref(), &files);
    files.push(("manifest.json", manifest));
    Ok(InteractivePackage { files })
}

/// webOS names the remote's select key OK. Maja's page is a browser, where a
/// button is activated by clicking it, so instructions that name that key are
/// rewritten into the visible text and `@iden` hints. Arachnid still emits
/// the scene's words.
///
/// Layout has already wrapped the authored line, so a sentence can contain
/// newlines. Phrases are matched across those breaks. Other line breaks stay.
fn browser_copy(value: &str) -> Cow<'_, str> {
    // Longest first. A bare "Press OK" must not eat the start of "OKAY".
    const PHRASES: &[(&str, bool, bool)] = &[
        ("Arrow keys move between cards; OK turns one over.", false, false),
        ("Arrow keys move between cards", false, false),
        ("OK turns one over.", false, false),
        ("OK turns one over", false, false),
        ("Press OK to ", true, false),
        ("press OK to ", true, false),
        ("Press OK", false, true),
        ("press OK", false, true),
    ];
    if !mentions_remote(value) {
        return Cow::Borrowed(value);
    }
    let mut text = value.to_owned();
    let mut changed = false;
    loop {
        let hit = PHRASES.iter().find_map(|(phrase, capitalize, boundary)| {
            find_flex(&text, phrase, *boundary).map(|(start, end)| (start, end, *capitalize))
        });
        let Some((start, end, capitalize)) = hit else {
            break;
        };
        text.replace_range(start..end, "");
        if capitalize {
            capitalize_at(&mut text, start);
        }
        changed = true;
    }
    if !changed {
        return Cow::Borrowed(value);
    }
    let tidy = tidy_copy(&text);
    if tidy == value {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(tidy)
    }
}

fn mentions_remote(value: &str) -> bool {
    let folded = value.split_whitespace().collect::<Vec<_>>().join(" ");
    folded.contains("Press OK")
        || folded.contains("press OK")
        || folded.contains("Arrow keys move between cards")
        || folded.contains("OK turns one over")
}

/// Byte span of `phrase` in `haystack`. A space in the phrase matches any
/// whitespace run, including a wrap newline. When `boundary` is set, the
/// match must not be the prefix of a longer word.
fn find_flex(haystack: &str, phrase: &str, boundary: bool) -> Option<(usize, usize)> {
    let h: Vec<(usize, char)> = haystack.char_indices().collect();
    let p: Vec<char> = phrase.chars().collect();
    if p.is_empty() || h.is_empty() {
        return None;
    }
    for i in 0..h.len() {
        let Some(next) = match_at(&h, i, &p) else {
            continue;
        };
        if boundary && h.get(next).is_some_and(|(_, c)| c.is_alphanumeric()) {
            continue;
        }
        let start = h[i].0;
        let end = h.get(next).map(|(at, _)| *at).unwrap_or(haystack.len());
        return Some((start, end));
    }
    None
}

fn match_at(haystack: &[(usize, char)], start: usize, phrase: &[char]) -> Option<usize> {
    let mut hi = start;
    for &pc in phrase {
        if pc.is_whitespace() {
            if hi >= haystack.len() || !haystack[hi].1.is_whitespace() {
                return None;
            }
            while hi < haystack.len() && haystack[hi].1.is_whitespace() {
                hi += 1;
            }
            continue;
        }
        if hi >= haystack.len() || haystack[hi].1 != pc {
            return None;
        }
        hi += 1;
    }
    Some(hi)
}

fn capitalize_at(text: &mut String, at: usize) {
    let rest = &text[at..];
    let Some((rel, ch)) = rest.char_indices().find(|(_, c)| !c.is_whitespace()) else {
        return;
    };
    let upper: String = ch.to_uppercase().collect();
    if upper.chars().eq(std::iter::once(ch)) {
        return;
    }
    let abs = at + rel;
    text.replace_range(abs..abs + ch.len_utf8(), &upper);
}

fn tidy_copy(text: &str) -> String {
    let mut out = String::new();
    for line in text.split('\n') {
        let line = tidy_line(line);
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    let core = out.trim_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | ' '));
    if core.is_empty() {
        String::new()
    } else {
        out
    }
}

fn tidy_line(line: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for c in line.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if matches!(c, '.' | ',' | ';' | ':') {
            pending_space = false;
        } else if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(c);
    }
    out
}

/// Visible text and hints. An instruction that was only the remote key
/// becomes no text, so the page does not render an empty line or an empty
/// description.
fn js_browser_text(value: &str) -> String {
    let copy = browser_copy(value);
    if copy.is_empty() {
        "null".into()
    } else {
        js_string(&copy)
    }
}

fn js_string(value: &str) -> String {
    let mut out = String::from("\"");
    for unit in value.encode_utf16() {
        match unit {
            0x22 => out.push_str("\\\""),
            0x5c => out.push_str("\\\\"),
            0x20..=0x7e => out.push(unit as u8 as char),
            _ => {
                let _ = write!(out, "\\u{unit:04x}");
            }
        }
    }
    out.push('"');
    out
}

fn js_number(value: f64, origin: &str) -> Result<String, Diagnostic> {
    if !value.is_finite() {
        return Err(Diagnostic {
            code: "MAJA-E0200",
            message: format!("non-finite number in {origin}"),
            range: None,
        });
    }
    // `{:?}` is the shortest representation that round-trips, which
    // JavaScript parses to the same double.
    Ok(format!("{value:?}"))
}

fn scene_js(
    ir: &InteractiveWebIr,
    runtime: Option<&D1Runtime>,
    chrome: Option<&str>,
    graphics: &[BakedGraphic],
) -> Result<String, Diagnostic> {
    let mut out = format!("// Generated by Maja ({}). Do not edit.\n", ir.profile());
    out.push_str("export const CELLS = [\n");
    for cell in ir.cells() {
        let pair = |v: Option<[f64; 2]>, what: &str| -> Result<String, Diagnostic> {
            Ok(match v {
                Some([x, y]) => format!("[{}, {}]", js_number(x, what)?, js_number(y, what)?),
                None => "null".into(),
            })
        };
        let present = cell.present.as_ref().map(condition_js).unwrap_or_else(|| "null".into());
        let appear = match cell.appear {
            Some((seconds, [x, y])) => format!(
                "{{ s: {}, from: [{}, {}] }}",
                js_number(seconds, &cell.id)?,
                js_number(x, &cell.id)?,
                js_number(y, &cell.id)?
            ),
            None => "null".into(),
        };
        let fill_to = match cell.fill_to {
            Some((colour, period)) => format!(
                "[{}, {}]",
                js_string(&format!("#{colour:06x}")),
                js_number(period, &cell.id)?
            ),
            None => "null".into(),
        };
        let _ = writeln!(
            out,
            "  {{ id: {}, kind: {}, parent: {}, position: {}, extent: {}, fill: {}, stroke: {}, textSize: {}, text: {}, scroll: {}, textFrom: {}, iden: {}, present: {present}, fillTo: {fill_to}, draw: {}, appear: {appear} }},",
            js_string(&cell.id),
            js_string(match cell.kind {
                WebCellKind::Cell => "cell",
                WebCellKind::Button => "button",
                WebCellKind::Input => "input",
            }),
            cell.parent.map_or("null".into(), |p| p.to_string()),
            pair(cell.position_p, &cell.id)?,
            pair(cell.extent_p, &cell.id)?,
            cell.fill.map_or("null".into(), |c| js_string(&format!("#{c:06x}"))),
            match cell.stroke {
                Some((colour, width)) => format!(
                    "[{}, {}]",
                    js_string(&format!("#{colour:06x}")),
                    js_number(width, &cell.id)?
                ),
                None => "null".into(),
            },
            cell.text_size_p
                .map(|v| js_number(v, &cell.id))
                .transpose()?
                .unwrap_or_else(|| "null".into()),
            cell.text.as_deref().map_or("null".into(), js_browser_text),
            cell.scroll,
            cell.text_from.as_deref().map_or("null".into(), js_string),
            cell.iden.as_ref().map_or("null".into(), |iden| format!(
                "[{}, {}]",
                js_browser_text(&iden.label),
                iden.hint.as_deref().map_or("null".into(), js_browser_text)
            )),
            draw_js(ir, cell)?,
        );
    }
    out.push_str("];\n");
    // Storage namespace (GSSH/2 rule 4): the ontology name.
    let _ = writeln!(
        out,
        "export const ONTOLOGY = {};",
        js_string(&ir.handoff().parsed().scene.name)
    );
    // Initial state: [key, type, value, lifetime] with ints as decimal strings.
    out.push_str("export const INITIAL_STATE = [\n");
    for decl in ir.handoff().state() {
        let (ty, value) = match &decl.initial {
            goya_lang::InteractionValue::Int(n) => ("int", js_string(&n.to_string())),
            goya_lang::InteractionValue::Text(s) => ("text", js_string(s)),
            goya_lang::InteractionValue::Bool(b) => ("bool", b.to_string()),
        };
        let lifetime = match decl.lifetime {
            goya_lang::StateLifetime::Session => "session",
            goya_lang::StateLifetime::Durable => "durable",
        };
        let _ = writeln!(
            out,
            "  [{}, \"{ty}\", {value}, \"{lifetime}\"],",
            js_string(&decl.key)
        );
    }
    out.push_str("];\n");
    // Activations: button ID -> action ID and bound fields in key order.
    out.push_str("export const ACTIVATIONS = [\n");
    if let Some(runtime) = runtime {
        let parsed = ir.handoff().parsed();
        for (interaction, action) in parsed.scene.interactions.iter().zip(&runtime.actions) {
            let button = ir.cell_by_name(&action.trigger_button).ok_or(Diagnostic {
                code: "MAJA-E0201",
                message: format!(
                    "trigger button {} does not name exactly one element",
                    action.trigger_button
                ),
                range: Some(action.range),
            })?;
            let mut fields = Vec::new();
            for (key, ty) in &action.inputs {
                let binding = ir
                    .handoff()
                    .bindings_for(&interaction.name)
                    .find(|b| &b.input == key)
                    .ok_or(Diagnostic {
                        code: "MAJA-E0201",
                        message: format!("input {key} has no binding"),
                        range: Some(action.range),
                    })?;
                let ty = match ty {
                    goya_lang::InteractionType::Int => "int",
                    goya_lang::InteractionType::Text => "text",
                    goya_lang::InteractionType::Bool => "bool",
                };
                fields.push(format!(
                    "[{}, \"{ty}\", {}]",
                    js_string(key),
                    js_string(&binding.field_id)
                ));
            }
            let _ = writeln!(
                out,
                "  [{}, {}, [{}]],",
                js_string(&button.id),
                js_string(&action.action_id),
                fields.join(", ")
            );
        }
    }
    out.push_str("];\n");
    out.push_str(&desktop_js(ir, chrome)?);
    out.push_str(&graphics_js(graphics));
    Ok(out)
}

struct BakedGraphic {
    name: String,
    data: String,
    sha: String,
    radices: Vec<usize>,
    steps: Vec<u32>,
}

fn bake_graphics(parsed: &goya_lang::ParsedScene) -> Result<Vec<BakedGraphic>, Diagnostic> {
    parsed
        .declarations
        .graphics
        .iter()
        .map(|graphic| {
            let compiled = goya_scene_graphics::compile(&parsed.scene.name, graphic).map_err(
                |message| Diagnostic {
                    code: "MAJA-E0200",
                    message: format!("graphic {}: {message}", graphic.name),
                    range: Some(graphic.range),
                },
            )?;
            Ok(BakedGraphic {
                name: graphic.name.clone(),
                data: base64(&compiled.bytes),
                sha: compiled.sha256,
                radices: compiled.dims.iter().map(|dim| dim.1.len()).collect(),
                steps: compiled.dims.iter().map(|dim| dim.2.unwrap_or(0)).collect(),
            })
        })
        .collect()
}

fn graphics_js(graphics: &[BakedGraphic]) -> String {
    let mut out = String::from("export const GRAPHICS = {\n");
    for graphic in graphics {
        let radices = graphic
            .radices
            .iter()
            .map(u32_or_usize_string)
            .collect::<Vec<_>>()
            .join(", ");
        let steps = graphic
            .steps
            .iter()
            .map(|step| step.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "  {}: {{ data: {}, sha: {}, radices: [{radices}], steps: [{steps}] }},",
            js_string(&graphic.name),
            js_string(&graphic.data),
            js_string(&graphic.sha)
        );
    }
    out.push_str("};\n");
    out
}

fn u32_or_usize_string(value: &usize) -> String {
    value.to_string()
}

fn condition_js(condition: &WebCondition) -> String {
    let op = if condition.at_least { "ge" } else { "is" };
    let value = match &condition.value {
        goya_lang::InteractionValue::Int(n) => n.to_string(),
        goya_lang::InteractionValue::Bool(b) => b.to_string(),
        goya_lang::InteractionValue::Text(text) => js_string(text),
    };
    format!("[{}, \"{op}\", {value}]", js_string(&condition.key))
}

fn draw_js(ir: &InteractiveWebIr, cell: &WebCell) -> Result<String, Diagnostic> {
    let Some(draw) = &cell.draw else {
        return Ok("null".into());
    };
    let graphic = ir
        .handoff()
        .parsed()
        .declarations
        .graphics
        .iter()
        .find(|candidate| candidate.name == draw.graphic)
        .ok_or_else(|| Diagnostic {
            code: "MAJA-E0200",
            message: format!("@draw names unknown graphic {}", draw.graphic),
            range: Some(cell.range),
        })?;
    let mut dims = Vec::new();
    for dimension in &graphic.dimensions {
        let mut rules = Vec::new();
        for rule in draw.rules.iter().filter(|rule| rule.dimension == dimension.name) {
            let index = dimension
                .values
                .iter()
                .position(|value| value.name == rule.value)
                .ok_or_else(|| Diagnostic {
                    code: "MAJA-E0200",
                    message: format!(
                        "graphic {} dimension {} has no value {}",
                        graphic.name, dimension.name, rule.value
                    ),
                    range: Some(cell.range),
                })?;
            let when = rule
                .when
                .as_ref()
                .map(condition_js)
                .unwrap_or_else(|| "null".into());
            rules.push(format!("[{when}, {index}]"));
        }
        dims.push(format!("[{}]", rules.join(", ")));
    }
    Ok(format!(
        "{{ g: {}, dims: [{}] }}",
        js_string(&draw.graphic),
        dims.join(", ")
    ))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn desktop_js(ir: &InteractiveWebIr, chrome: Option<&str>) -> Result<String, Diagnostic> {
    use goya_lang::desktop::{self, DesktopPosition, DesktopState};
    let declarations = &ir.handoff().parsed().declarations;
    let range = declarations.desktop.as_ref().map(|desktop| desktop.range);
    let resolved = declarations
        .resolved_desktop()
        .map_err(|fault| Diagnostic {
            code: "MAJA-E0200",
            message: fault.message(),
            range,
        })?;
    if chrome.is_some() && resolved.controls_authored {
        return Err(Diagnostic {
            code: "MAJA-E0200",
            message: "scene authors window controls; --chrome cannot override them".into(),
            range,
        });
    }
    let (colours, library_id) = if let Some(name) = chrome {
        let style = desktop::library(name).ok_or_else(|| Diagnostic {
            code: "MAJA-E0200",
            message: format!("unknown window chrome library '{name}'"),
            range,
        })?;
        (style.colours, Some(style.id))
    } else {
        (resolved.colours, resolved.library_id)
    };
    let position = match resolved.position {
        DesktopPosition::Center => "\"center\"".to_owned(),
        DesktopPosition::Dip { x, y } => format!("[{x}, {y}]"),
    };
    let extent = match resolved.extent_dip {
        None => "null".to_owned(),
        Some((width, height)) => format!("[{width}, {height}]"),
    };
    let library = library_id.map(js_string).unwrap_or_else(|| "null".into());
    Ok(format!(
        "export const DESKTOP = {{ state: \"{}\", position: {position}, extent: {extent}, frame: {}, chromeName: {library}, chrome: {{ bar: \"#{:06x}\", glyph: \"#{:06x}\", close: \"#{:06x}\", closeGlyph: \"#{:06x}\", desk: \"#{:06x}\" }}, title: {}, titleBar: {}, chipWidth: {}, chipMargin: {}, fallbackExtent: [{}, {}], authored: {} }};\n",
        resolved.state.as_str(),
        resolved.state != DesktopState::Fullscreen,
        colours.bar,
        colours.glyph,
        colours.close,
        colours.close_glyph,
        colours.desk,
        js_string(&ir.handoff().parsed().scene.name),
        desktop::TITLE_BAR_DIP,
        desktop::CHIP_WIDTH_DIP,
        desktop::CHIP_MARGIN_DIP,
        desktop::DEFAULT_WINDOW_DIP.0,
        desktop::DEFAULT_WINDOW_DIP.1,
        resolved.authored,
    ))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn manifest(
    ir: &InteractiveWebIr,
    runtime: Option<&D1Runtime>,
    files: &[(&'static str, String)],
) -> String {
    let mut out = String::from("{\n");
    let _ = writeln!(out, "  \"schema\": \"{MANIFEST_SCHEMA}\",");
    let _ = writeln!(
        out,
        "  \"compiler\": \"maja {}\",",
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(out, "  \"profile\": \"{}\",", ir.profile());
    let _ = writeln!(out, "  \"qualification\": \"experimental\",");
    let _ = writeln!(out, "  \"handoff\": \"{}\",", ir.handoff().contract());
    let durable = ir.handoff().durable_keys().next().is_some();
    let _ = writeln!(
        out,
        "  \"durable_store\": {},",
        if durable {
            js_string(ir.durable_store().adapter())
        } else {
            "null".into()
        }
    );
    let _ = writeln!(
        out,
        "  \"migration\": {},",
        if durable {
            js_string(migration_name(ir.migration()))
        } else {
            "null".into()
        }
    );
    let _ = writeln!(
        out,
        "  \"d1_runtime\": {},",
        if runtime.is_some() {
            js_string(maja_runtime::D1_RUNTIME_CONTRACT)
        } else {
            "null".into()
        }
    );
    let _ = writeln!(
        out,
        "  \"source_sha256\": \"{}\",",
        hex(&ir.source_digest())
    );
    out.push_str("  \"elements\": {");
    for (n, cell) in ir.cells().iter().enumerate() {
        let _ = write!(
            out,
            "{}\n    {}: [{}, {}]",
            if n > 0 { "," } else { "" },
            js_string(&cell.id),
            cell.range.start,
            cell.range.end
        );
    }
    out.push_str("\n  },\n  \"actions\": {");
    if let Some(runtime) = runtime {
        for (n, action) in runtime.actions.iter().enumerate() {
            let _ = write!(
                out,
                "{}\n    {}: [{}, {}]",
                if n > 0 { "," } else { "" },
                js_string(&action.action_id),
                action.range.start,
                action.range.end
            );
        }
    }
    out.push_str("\n  },\n  \"artifacts_sha256\": {");
    for (n, (name, text)) in files.iter().enumerate() {
        let _ = write!(
            out,
            "{}\n    \"{name}\": \"{}\"",
            if n > 0 { "," } else { "" },
            hex(&Sha256::digest(text.as_bytes()))
        );
    }
    out.push_str("\n  }\n}\n");
    out
}

/// Write a new private output directory. An existing path is never
/// replaced; on failure only the files this call created are removed.
pub fn write(package: &InteractivePackage, output: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(output)?;
    let mut created = Vec::new();
    let result = (|| {
        for (name, text) in &package.files {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output.join(name))?;
            created.push(*name);
            file.write_all(text.as_bytes())?;
        }
        Ok(())
    })();
    if result.is_err() {
        for name in created {
            let _ = fs::remove_file(output.join(name));
        }
        let _ = fs::remove_dir(output);
    }
    result
}

const INDEX_HTML: &str = concat!(
    "<!doctype html>\n<html lang=\"und\"><head><meta charset=\"utf-8\">",
    "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
    "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'self'; style-src 'self'; img-src data:; base-uri 'none'; form-action 'none'\">",
    "<link rel=\"icon\" href=\"data:,\">",
    "<link rel=\"stylesheet\" href=\"app.css\"><title>Goya application</title>",
    "<script type=\"module\" src=\"app.js\"></script></head>",
    "<body><div id=\"goya-desktop\"><div id=\"goya-window\">",
    "<div id=\"goya-titlebar\"><span id=\"goya-title\"></span><div id=\"goya-window-controls\">",
    "<button type=\"button\" id=\"goya-min\" aria-label=\"Minimise\"><svg viewBox=\"0 0 12 12\" aria-hidden=\"true\"><path d=\"M2 6h8\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.2\"></path></svg></button>",
    "<button type=\"button\" id=\"goya-max\" aria-label=\"Maximise\"><svg class=\"goya-glyph-max\" viewBox=\"0 0 12 12\" aria-hidden=\"true\"><rect x=\"2.5\" y=\"2.5\" width=\"7\" height=\"7\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.2\"></rect></svg><svg class=\"goya-glyph-restore\" viewBox=\"0 0 12 12\" aria-hidden=\"true\" hidden><path d=\"M4 2.5h5.5V8M2.5 4.5h5.5V10\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.2\"></path></svg></button>",
    "<button type=\"button\" id=\"goya-close\" aria-label=\"Close\"><svg viewBox=\"0 0 12 12\" aria-hidden=\"true\"><path d=\"M3 3l6 6M9 3L3 9\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.2\"></path></svg></button>",
    "</div></div><main id=\"goya-scene\"></main></div></div>",
    "<p id=\"goya-status\" role=\"status\" aria-live=\"polite\"></p></body></html>\n"
);

const APP_CSS: &str = "\
html,body{margin:0;width:100%;height:100%;overflow:hidden}
#goya-desktop{position:absolute;inset:0;overflow:hidden}
#goya-window{position:absolute;display:flex;flex-direction:column;box-sizing:border-box;min-width:0;min-height:0}
#goya-titlebar{flex:0 0 32px;display:flex;align-items:center;height:32px;background:var(--goya-bar);color:var(--goya-glyph);user-select:none;touch-action:none}
#goya-title{flex:1 1 auto;padding:0 10px;font:13px/32px system-ui,sans-serif;overflow:hidden;white-space:nowrap;text-overflow:ellipsis}
#goya-window-controls{display:flex;flex:0 0 auto}
#goya-window-controls button{width:32px;height:32px;padding:0;border:0;background:transparent;color:inherit;cursor:pointer}
#goya-window-controls button:focus-visible{outline:2px solid currentColor;outline-offset:-2px}
#goya-window-controls svg{width:12px;height:12px;margin:10px auto}
#goya-window-controls svg[hidden]{display:none}
#goya-close{background:var(--goya-close)!important;color:var(--goya-close-glyph)!important}
#goya-scene{position:relative;flex:1 1 auto;min-height:0;overflow:hidden}
#goya-window[data-mode=fullscreen] #goya-titlebar{position:absolute;top:0;right:0;width:auto;background:transparent;z-index:2}
#goya-window[data-mode=fullscreen] #goya-title{display:none}
#goya-window[data-mode=fullscreen] #goya-scene{position:absolute;inset:0}
#goya-window[data-mode=minimized] #goya-scene{display:none}
#goya-window[data-mode=closed]{display:none}
.goya-el{position:absolute;box-sizing:border-box;overflow:hidden;margin:0}
.goya-el[hidden]{display:none}
.goya-text{display:block;white-space:pre;margin:0}
.goya-gv{position:absolute;inset:0;width:100%;height:100%;pointer-events:none}
.goya-breathe{position:absolute;inset:0;pointer-events:none;opacity:0;animation:goya-breathe 1s ease-in-out infinite alternate}
@keyframes goya-breathe{from{opacity:0}to{opacity:1}}
@media (prefers-reduced-motion:reduce){.goya-breathe{animation:none}}
.goya-appear{animation:goya-appear var(--goya-appear,0.4s) ease-out both}
@keyframes goya-appear{from{opacity:0;transform:translate(var(--goya-dx,0px),var(--goya-dy,0px))}to{opacity:1;transform:none}}
@media (prefers-reduced-motion:reduce){.goya-appear{animation:none}}
.goya-control{position:absolute;inset:0;width:100%;height:100%;box-sizing:border-box;margin:0;font:inherit;font-size:inherit}
button.goya-control{background:transparent;border:0;color:inherit;padding:0;appearance:none;cursor:pointer}
.goya-scroller{position:absolute;inset:0;overflow:auto;scrollbar-width:none}
.goya-stroke{position:absolute;inset:0;box-sizing:border-box;border-style:solid;border-width:var(--goya-stroke-width);border-color:var(--goya-stroke);pointer-events:none}
#goya-status{position:fixed;left:0;right:0;bottom:0;margin:0;padding:4px 8px;font:13px/1.4 system-ui,sans-serif;background:#222;color:#fff}
#goya-status:empty{display:none}
.goya-live{white-space:pre-wrap}
.goya-sr{position:absolute;width:1px;height:1px;margin:-1px;padding:0;border:0;overflow:hidden;clip-path:inset(50%);white-space:nowrap}
";

/// Mirror of `goya_lang::layout` for scenes without scroll viewports, and
/// of `handoff::convert_field`. Millipoints are integers held in doubles;
/// viewports are bounded so every value is an exact safe integer.
pub const LAYOUT_JS: &str = r#"// Maja layout and field conversion mirrors. Do not edit.
export const MAX_VIEWPORT_POINTS = 1000000;
const trunc = Math.trunc;
const clamp01 = (v) => Math.min(Math.max(v, 0), 1);
const nonNegative = (v) => Math.max(v, 0);
// Resolve every cell to a millipoint rect, as goya_lang::layout does for a
// viewport of widthPt x heightPt points.
export function resolve(cells, widthPt, heightPt) {
  const w = Math.min(Math.max(widthPt, 0), MAX_VIEWPORT_POINTS);
  const h = Math.min(Math.max(heightPt, 0), MAX_VIEWPORT_POINTS);
  const out = [];
  for (const cell of cells) {
    const parent = cell.parent === null
      ? { x: 0, y: 0, width: trunc(w * 1000), height: trunc(h * 1000) }
      : out[cell.parent].rect;
    // Direct children of a scroll viewport may extend past it (>= 0 only).
    const overflow = cell.parent !== null && cells[cell.parent].scroll;
    const fraction = overflow ? nonNegative : clamp01;
    let width, height;
    if (cell.extent !== null) {
      width = trunc(fraction(cell.extent[0]) * parent.width);
      height = trunc(fraction(cell.extent[1]) * parent.height);
    } else {
      width = parent.width;
      height = parent.height;
    }
    width = Math.max(width, 0);
    height = Math.max(height, 0);
    let x = parent.x, y = parent.y;
    if (cell.position !== null) {
      x = parent.x + trunc(fraction(cell.position[0]) * parent.width);
      y = parent.y + trunc(fraction(cell.position[1]) * parent.height);
    }
    const right = parent.x + parent.width;
    const bottom = parent.y + parent.height;
    x = overflow ? Math.max(x, parent.x) : Math.min(Math.max(x, parent.x), right);
    y = overflow ? Math.max(y, parent.y) : Math.min(Math.max(y, parent.y), bottom);
    const rect = overflow ? { x, y, width, height } : {
      x, y,
      width: Math.max(Math.min(width, right - x), 0),
      height: Math.max(Math.min(height, bottom - y), 0),
    };
    out.push({
      rect,
      textSize: cell.textSize === null ? null : trunc(Math.max(cell.textSize, 0) * rect.height),
      strokeWidth: cell.stroke === null ? null : trunc(Math.min(Math.max(cell.stroke[1], 0), 0.5) * Math.min(rect.width, rect.height)),
    });
  }
  return out;
}
// Mirror of Frame::scroll_limits: for each scroll viewport, how far its
// descendants extend past its right and bottom edges; null otherwise.
export function scrollLimits(cells, resolved) {
  return cells.map((cell, i) => {
    if (!cell.scroll) return null;
    const v = resolved[i].rect;
    let right = v.x + v.width, bottom = v.y + v.height;
    for (let j = 0; j < cells.length; j++) {
      let a = cells[j].parent;
      while (a !== null && a !== i) a = cells[a].parent;
      if (a !== i) continue;
      const r = resolved[j].rect;
      right = Math.max(right, r.x + r.width);
      bottom = Math.max(bottom, r.y + r.height);
    }
    return [right - (v.x + v.width), bottom - (v.y + v.height)];
  });
}
const MAX_FIELD_TEXT_BYTES = 65536;
const I64_MIN = -9223372036854775808n;
const I64_MAX = 9223372036854775807n;
export function utf8Length(s) {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c <= 0xdbff) {
      const d = s.charCodeAt(i + 1);
      if (!(d >= 0xdc00 && d <= 0xdfff)) return -1;
      n += 4; i++;
    } else if (c >= 0xdc00 && c <= 0xdfff) return -1;
    else n += 3;
  }
  return n;
}
// Mirrors of goya_lang::display_binding (decision D9).
export function shownText(value) {
  return value.type === "text" ? value.value : String(value.value);
}
const isBlank = (c) => c === " " || c === "\t" || c === "\n" || c === "\r";
function trimBlankEnd(s) {
  let end = s.length;
  while (end > 0 && isBlank(s[end - 1])) end--;
  return s.slice(0, end);
}
export function displayContext(authoredText, key) {
  let text = trimBlankEnd(authoredText ?? "");
  if (text.endsWith(":")) text = trimBlankEnd(text.slice(0, -1));
  return text === "" ? key : text;
}
export function describeChange(context, oldValue, newValue) {
  if (oldValue.type !== newValue.type || oldValue.value === newValue.value) return null;
  if (oldValue.type === "int") {
    const d = newValue.value - oldValue.value;
    const verb = d > 0n ? "increased" : "decreased";
    return `${context} ${verb} by ${d > 0n ? d : -d}, from ${oldValue.value} to ${newValue.value}`;
  }
  if (oldValue.type === "text") return `${context} changed from "${oldValue.value}" to "${newValue.value}"`;
  return `${context} changed from ${oldValue.value} to ${newValue.value}`;
}
export function announcement(changes) {
  const parts = [];
  for (const [context, oldValue, newValue] of changes) {
    const part = describeChange(context, oldValue, newValue);
    if (part !== null) parts.push(part);
  }
  return parts.length === 0 ? null : parts.join("; ");
}
// Mirror of handoff::convert_field. Returns {ok, value} or {ok: false, error}.
export function convertField(type, text) {
  if (typeof text !== "string") return { ok: false, error: "INVALID_TEXT" };
  if (type === "int") {
    if (!/^(?:0|-?[1-9][0-9]*)$/.test(text)) return { ok: false, error: "NOT_CANONICAL_INT" };
    const value = BigInt(text);
    if (value < I64_MIN || value > I64_MAX) return { ok: false, error: "NOT_CANONICAL_INT" };
    return { ok: true, value: Object.freeze({ type: "int", value }) };
  }
  if (type === "text") {
    const n = utf8Length(text);
    if (n < 0) return { ok: false, error: "INVALID_TEXT" };
    if (n > MAX_FIELD_TEXT_BYTES) return { ok: false, error: "TEXT_LIMIT" };
    return { ok: true, value: Object.freeze({ type: "text", value: text }) };
  }
  return { ok: false, error: "UNSUPPORTED" };
}
// Mirror of goya_lang::desktop::place and minimized_chip. A hidden document
// can report a 0 client size; the shell substitutes 1 before calling these.
// placeDesktop itself returns null when a work-area edge is below 1.
export const DESKTOP_LIMITS = Object.freeze({
  maxExtent: 8192,
  titleBar: 32,
  chipWidth: 240,
  chipMargin: 8,
  fallbackExtent: Object.freeze([960, 640]),
});
const workOk = (workW, workH) => Number.isInteger(workW) && Number.isInteger(workH) && workW >= 1 && workH >= 1;
export function placeDesktop(spec, workW, workH) {
  if (!workOk(workW, workH)) return null;
  if (spec.state === "fullscreen" || spec.state === "maximized") {
    return { state: spec.state, x: 0, y: 0, width: workW, height: workH, frame: spec.state !== "fullscreen" };
  }
  if (spec.state !== "windowed" || !Array.isArray(spec.extent)) return null;
  const extentW = spec.extent[0], extentH = spec.extent[1];
  if (!Number.isInteger(extentW) || !Number.isInteger(extentH) || extentW < 1 || extentH < 1 || extentW > DESKTOP_LIMITS.maxExtent || extentH > DESKTOP_LIMITS.maxExtent) return null;
  const width = Math.min(extentW, workW);
  const height = Math.min(extentH, workH);
  let x, y;
  if (spec.position === "center") {
    x = trunc((workW - width) / 2);
    y = trunc((workH - height) / 2);
  } else {
    x = Math.min(Math.max(spec.position[0], 0), workW - width);
    y = Math.min(Math.max(spec.position[1], 0), workH - height);
  }
  return { state: "windowed", x, y, width, height, frame: true };
}
export function minimizedChip(workW, workH) {
  if (!workOk(workW, workH)) return null;
  const width = Math.min(DESKTOP_LIMITS.chipWidth, workW);
  const height = Math.min(DESKTOP_LIMITS.titleBar, workH);
  return {
    x: Math.min(DESKTOP_LIMITS.chipMargin, workW - width),
    y: Math.max(workH - height - DESKTOP_LIMITS.chipMargin, 0),
    width,
    height,
  };
}
"#;

/// Shared part of `storage.js`: the D8 record (`goya.state.v1`), restore
/// rules (GSSH/2 rule 4) and record encoding. One record per ontology holds
/// every durable key, so a transition that writes several durable keys is
/// persisted by one store write. Both backends keep the same record text.
const STORAGE_COMMON_JS: &str = r#"import { convertField, utf8Length } from "./layout.js";
export const RECORD_FORMAT = "goya.state.v1";
const MAX_SNAPSHOT_BYTES = 1048576;
// Keep hostile or corrupted records from making JSON.parse allocate without
// bound. This allows generous JSON overhead over the canonical 1 MiB state
// snapshot.
const MAX_RECORD_CODE_UNITS = 8 * 1048576;
const TYPES = ["int", "text", "bool"];
// Storage namespace is (ontology name, state key): one record per ontology,
// one entry per durable key.
export function recordKey(ontology) { return `${RECORD_FORMAT}/${ontology}`; }
const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
const plain = (o) => o !== null && typeof o === "object" && !Array.isArray(o);
const durableKeysOf = (initialState) => initialState.filter((row) => row[3] === "durable").map((row) => row[0]);
function initialValue(type, value) {
  return Object.freeze({ type, value: type === "int" ? BigInt(value) : value });
}
function decode(type, raw) {
  if (type === "bool") return typeof raw === "boolean" ? Object.freeze({ type, value: raw }) : null;
  const converted = convertField(type, raw);
  return converted.ok ? converted.value : null;
}
function snapshotBytes(state) {
  let n = 8;
  for (const [k, v] of state) n += 8 + utf8Length(k) + (v.type === "int" ? 8 : v.type === "bool" ? 1 : utf8Length(v.value));
  return n;
}
// GSSH/2 rule 4. `text` is the record read (null when absent); `failure`
// is a code when it could not be read. Returns { ok, code, state, preserved }.
// On failure `state` is the authored initial state and the stored record is
// left untouched; the first committed durable write replaces it.
// `preserved` holds entries of a decodable record that this scene does not
// declare durable; they are written back unchanged.
function decodeRecord(text, initialState, failure) {
  const state = new Map(initialState.map(([k, t, v]) => [k, initialValue(t, v)]));
  const durable = initialState.filter((row) => row[3] === "durable");
  const preserved = Object.create(null);
  const fail = (code, kept) => ({ ok: false, code, state, preserved: kept ?? Object.create(null) });
  if (failure) return fail(failure);
  if (text === null) return { ok: true, code: null, state, preserved };
  if (typeof text !== "string") return fail("RECORD_UNDECODABLE");
  if (text.length > MAX_RECORD_CODE_UNITS) return fail("RECORD_LIMIT");
  let record;
  try { record = JSON.parse(text); } catch { return fail("RECORD_UNDECODABLE"); }
  if (!plain(record) || Object.keys(record).length !== 2 || record.format !== RECORD_FORMAT || !plain(record.values)) return fail("RECORD_UNDECODABLE");
  const durableKeys = new Set(durable.map((row) => row[0]));
  for (const k of Object.keys(record.values)) if (!durableKeys.has(k)) preserved[k] = record.values[k];
  const restored = new Map(state);
  for (const [key, type] of durable) {
    if (!own(record.values, key)) continue;
    const entry = record.values[key];
    if (!Array.isArray(entry) || entry.length !== 2 || !TYPES.includes(entry[0])) return fail("RECORD_UNDECODABLE", preserved);
    if (entry[0] !== type) return fail("RECORD_TYPE_MISMATCH", preserved);
    const value = decode(type, entry[1]);
    if (value === null) return fail("RECORD_UNDECODABLE", preserved);
    restored.set(key, value);
  }
  if (snapshotBytes(restored) > MAX_SNAPSHOT_BYTES) return fail("RECORD_LIMIT", preserved);
  return { ok: true, code: null, state: restored, preserved };
}
// The whole durable snapshot after a transition, as record text.
function encodeRecord(durableKeys, preserved, current, writes) {
  const values = Object.create(null);
  for (const k of Object.keys(preserved)) values[k] = preserved[k];
  for (const k of durableKeys) {
    const v = writes.has(k) ? writes.get(k) : current.get(k);
    values[k] = [v.type, v.type === "int" ? v.value.toString() : v.value];
  }
  return JSON.stringify({ format: RECORD_FORMAT, values });
}
"#;

/// `localStorage` as a store: synchronous compare-and-set on the record
/// text. Every method throws when storage is unavailable.
const LOCAL_STORE_JS: &str = r#"const localStore = {
  adapter: "browser.local-storage.v1",
  read(key) { return globalThis.localStorage.getItem(key); },
  // Write `text` only if the record is still `expected` (decision D8).
  write(key, expected, text) {
    if (globalThis.localStorage.getItem(key) !== expected) return { ok: false, conflict: true };
    globalThis.localStorage.setItem(key, text);
    return { ok: true };
  },
  remove(key) { globalThis.localStorage.removeItem(key); },
};
"#;

/// IndexedDB as a store: database `goya.state.v1`, object store `records`,
/// the record text as the value. Methods return promises; the comparison
/// and the write share one `readwrite` transaction, which IndexedDB
/// serialises across windows.
const IDB_STORE_JS: &str = r#"const DB_NAME = RECORD_FORMAT;
const STORE = "records";
// Stored values that are not text compare equal to each other, so the first
// durable commit after RECORD_UNDECODABLE can replace them.
const OPAQUE = Object.freeze({});
const normal = (value) => value === undefined ? null : typeof value === "string" ? value : OPAQUE;
let opening = null;
function openDb() {
  if (opening !== null) return opening;
  opening = new Promise((resolve, reject) => {
    const request = globalThis.indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE);
    };
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => { db.close(); opening = null; };
      db.onclose = () => { opening = null; };
      resolve(db);
    };
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error("blocked"));
  });
  opening.catch(() => { opening = null; });
  return opening;
}
// Run `step(store, setResult, tx)` in one transaction; resolves with the
// result once the transaction has completed, rejects if it aborts.
function transact(mode, step) {
  return openDb().then((db) => new Promise((resolve, reject) => {
    const tx = mode === "readwrite" ? db.transaction(STORE, mode, { durability: "strict" }) : db.transaction(STORE, mode);
    let result;
    tx.oncomplete = () => resolve(result);
    tx.onabort = () => reject(tx.error);
    step(tx.objectStore(STORE), (value) => { result = value; }, tx);
  }));
}
const idbStore = {
  adapter: "browser.indexeddb.v1",
  read(key) {
    return transact("readonly", (store, set) => {
      const request = store.get(key);
      request.onsuccess = () => set(normal(request.result));
    });
  },
  write(key, expected, text) {
    let conflict = false;
    return transact("readwrite", (store, set, tx) => {
      const request = store.get(key);
      request.onsuccess = () => {
        if (normal(request.result) !== expected) { conflict = true; tx.abort(); return; }
        store.put(text, key);
      };
    }).then(() => ({ ok: true }), (error) => { if (conflict) return { ok: false, conflict: true }; throw error; });
  },
  remove(key) { return transact("readwrite", (store) => { store.delete(key); }); },
};
"#;

/// Restore and write over the scene's store (`PRIMARY`), with migration
/// from the other browser store (`SECONDARY`, decision D13).
const STORAGE_FLOW_JS: &str = r#"export const ADAPTER = PRIMARY.adapter;
export const MIGRATION_RULE = MIGRATION;
async function readPrimary(key, initialState) {
  let text;
  try { text = await PRIMARY.read(key); } catch {
    return { ...decodeRecord(null, initialState, "STORAGE_UNAVAILABLE"), seen: undefined, migrated: null };
  }
  return { ...decodeRecord(text, initialState), seen: text, migrated: null };
}
// Resolves to decodeRecord's result plus `seen`, the record text read from
// the scene's store (null when absent, undefined when unreadable), which
// the persister uses to detect writes by other windows, and `migrated`,
// the adapter a record was adopted from (or null).
//
// Decision D13: when the scene's store has no record and the other browser
// store has one for the same ontology, that record is decoded with the
// same rules, copied into the scene's store (only if it is still empty)
// and, under `move`, removed from the old store. An old record that fails
// to decode is reported and left where it is.
export async function restore(ontology, initialState) {
  const key = recordKey(ontology);
  const first = await readPrimary(key, initialState);
  if (first.seen !== null || SECONDARY === null || MIGRATION === "none") return first;
  let old;
  try { old = await SECONDARY.read(key); } catch { return first; }
  if (old === null) return first;
  const decoded = decodeRecord(old, initialState);
  if (!decoded.ok) return { ...decoded, seen: null, migrated: null };
  let written;
  // Not copied yet: the values are used and the next durable commit stores them.
  try { written = await PRIMARY.write(key, null, old); } catch { return { ...decoded, seen: null, migrated: null }; }
  // Another window filled the store first: restore what it wrote.
  if (!written.ok) return readPrimary(key, initialState);
  if (MIGRATION === "move") { try { await SECONDARY.remove(key); } catch { /* the copy stands */ } }
  return { ...decoded, seen: old, migrated: SECONDARY.adapter };
}
// A writer for one prepared transition. Transitions that write no durable
// key never touch storage. Otherwise the whole durable snapshot is written
// as one record, only if the store still holds exactly the text this window
// last read or wrote (decision D8); a changed record is a conflict and a
// failed write refuses the commit. The answer is immediate for
// localStorage and a promise for IndexedDB.
export function persister(ontology, initialState, preserved, seen) {
  const durableKeys = durableKeysOf(initialState);
  const key = recordKey(ontology);
  let expected = seen;
  return (current, writes) => {
    if (!durableKeys.some((k) => writes.has(k))) return { ok: true, wrote: false };
    const text = encodeRecord(durableKeys, preserved, current, writes);
    const failed = () => ({ ok: false, conflict: false, wrote: false });
    const settle = (written) => {
      if (!written.ok) return { ok: false, conflict: true, wrote: false };
      expected = text;
      return { ok: true, conflict: false, wrote: true };
    };
    let result;
    try { result = PRIMARY.write(key, expected, text); } catch { return failed(); }
    return result instanceof Promise ? result.then(settle, failed) : settle(result);
  };
}
"#;

/// `storage.js` for the scene's store and migration rule.
fn storage_js(store: DurableStore, migration: goya_lang::Migration) -> String {
    let rule = migration_name(migration);
    let (primary, secondary) = match store {
        DurableStore::LocalStorage => ("localStore", "idbStore"),
        DurableStore::IndexedDb => ("idbStore", "localStore"),
    };
    let needs_secondary = migration != goya_lang::Migration::None;
    let uses = |name: &str| primary == name || (needs_secondary && secondary == name);
    let mut out = format!(
        "// Maja durable state adapter (maja.web.local.v2/v3, {}, migration {rule}). Do not edit.\n{STORAGE_COMMON_JS}",
        store.adapter()
    );
    if uses("localStore") {
        out.push_str(LOCAL_STORE_JS);
    }
    if uses("idbStore") {
        out.push_str(IDB_STORE_JS);
    }
    let _ = writeln!(
        out,
        "const PRIMARY = {primary};\nconst SECONDARY = {};\nconst MIGRATION = \"{rule}\";",
        if needs_secondary { secondary } else { "null" }
    );
    out.push_str(STORAGE_FLOW_JS);
    out
}

fn migration_name(migration: goya_lang::Migration) -> &'static str {
    match migration {
        goya_lang::Migration::Copy => "copy",
        goya_lang::Migration::Move => "move",
        goya_lang::Migration::None => "none",
    }
}

const APP_JS: &str = r#"// Maja application shell (maja.web.local.v2/v3). Do not edit.
import { CELLS, ONTOLOGY, INITIAL_STATE, ACTIVATIONS, DESKTOP, GRAPHICS } from "./scene.js";
import { resolve, convertField, shownText, displayContext, announcement, placeDesktop, minimizedChip } from "./layout.js";

const root = document.getElementById("goya-scene");
const status = document.getElementById("goya-status");
const desktop = document.getElementById("goya-desktop");
const win = document.getElementById("goya-window");
const titlebar = document.getElementById("goya-titlebar");
const title = document.getElementById("goya-title");
const minBtn = document.getElementById("goya-min");
const maxBtn = document.getElementById("goya-max");
const closeBtn = document.getElementById("goya-close");
const maxGlyph = maxBtn.querySelector(".goya-glyph-max");
const restoreGlyph = maxBtn.querySelector(".goya-glyph-restore");
document.title = DESKTOP.title;
title.textContent = DESKTOP.title;
desktop.style.setProperty("--goya-bar", DESKTOP.chrome.bar);
desktop.style.setProperty("--goya-glyph", DESKTOP.chrome.glyph);
desktop.style.setProperty("--goya-close", DESKTOP.chrome.close);
desktop.style.setProperty("--goya-close-glyph", DESKTOP.chrome.closeGlyph);
desktop.style.background = DESKTOP.chrome.desk;
// A hidden document can report 0. The contract rejects that work area, so
// the shell measures at least 1 DIP. placeDesktop stays a pure mirror.
function workSize() {
  return [Math.max(document.documentElement.clientWidth, 1), Math.max(document.documentElement.clientHeight, 1)];
}
let mode = DESKTOP.state;
let beforeMinimize = mode;
let windowed = null;
if (DESKTOP.state === "windowed" && DESKTOP.extent !== null) {
  const [workW, workH] = workSize();
  const placed = placeDesktop({ state: "windowed", position: DESKTOP.position, extent: DESKTOP.extent }, workW, workH);
  if (placed !== null) windowed = { x: placed.x, y: placed.y, width: placed.width, height: placed.height };
}
function remember(placed) {
  windowed = { x: placed.x, y: placed.y, width: placed.width, height: placed.height };
}
function applyFrame() {
  const [workW, workH] = workSize();
  win.dataset.mode = mode;
  if (mode === "closed") return;
  let rect;
  if (mode === "minimized") {
    rect = minimizedChip(workW, workH);
  } else if (mode === "windowed") {
    // Display is clamped. The remembered extent stays, so a larger work area
    // restores it. Drag writes the intended origin; this call reclamps it.
    rect = placeDesktop({ state: "windowed", position: [windowed.x, windowed.y], extent: [windowed.width, windowed.height] }, workW, workH);
  } else {
    rect = placeDesktop({ state: mode, position: "center", extent: null }, workW, workH);
  }
  win.style.left = `${rect.x}px`;
  win.style.top = `${rect.y}px`;
  win.style.width = `${rect.width}px`;
  win.style.height = `${rect.height}px`;
  const restoring = mode === "fullscreen" || mode === "maximized";
  maxGlyph.hidden = restoring;
  restoreGlyph.hidden = !restoring;
  maxBtn.setAttribute("aria-label", mode === "fullscreen" ? "Exit fullscreen" : mode === "windowed" ? "Maximise" : "Restore");
  if (mode === "minimized") minBtn.setAttribute("aria-disabled", "true");
  else minBtn.removeAttribute("aria-disabled");
}
function show() {
  applyFrame();
  layout();
}
minBtn.addEventListener("click", () => {
  if (mode === "minimized" || mode === "closed") return;
  beforeMinimize = mode;
  mode = "minimized";
  show();
});
maxBtn.addEventListener("click", () => {
  if (mode === "closed") return;
  if (mode === "minimized") {
    mode = beforeMinimize;
  } else if (mode === "windowed") {
    mode = "maximized";
  } else {
    if (windowed === null) {
      const [workW, workH] = workSize();
      remember(placeDesktop({ state: "windowed", position: "center", extent: DESKTOP.fallbackExtent }, workW, workH));
    }
    mode = "windowed";
  }
  show();
});
closeBtn.addEventListener("click", () => {
  mode = "closed";
  applyFrame();
  status.textContent = "The window is closed.";
});
titlebar.addEventListener("pointerdown", (event) => {
  if (mode !== "windowed" || event.target.closest("button")) return;
  const origin = { ...windowed };
  const startX = event.clientX;
  const startY = event.clientY;
  titlebar.setPointerCapture(event.pointerId);
  const move = (ev) => {
    windowed = { ...origin, x: origin.x + Math.trunc(ev.clientX - startX), y: origin.y + Math.trunc(ev.clientY - startY) };
    show();
  };
  const stop = () => {
    titlebar.removeEventListener("pointermove", move);
    titlebar.removeEventListener("pointerup", stop);
    titlebar.removeEventListener("pointercancel", stop);
  };
  titlebar.addEventListener("pointermove", move);
  titlebar.addEventListener("pointerup", stop);
  titlebar.addEventListener("pointercancel", stop);
});
addEventListener("keydown", (event) => {
  if (event.key !== "Escape" || mode !== "fullscreen") return;
  if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
  event.preventDefault();
  if (windowed === null) {
    const [workW, workH] = workSize();
    remember(placeDesktop({ state: "windowed", position: "center", extent: DESKTOP.fallbackExtent }, workW, workH));
  }
  mode = "windowed";
  show();
});
applyFrame();
// Durable keys are restored before the scene is built, so its first frame
// already shows them. An IndexedDB restore is asynchronous and awaited here.
const restored = restore === null ? null : await restore(ONTOLOGY, INITIAL_STATE);
const elements = [];
const controls = new Map();
// `text-from` targets: [span, state key, context]. The span follows any
// authored text; the whole text wraps at the element's edge (D9, as
// Arachnid renders it).
const displays = [];
function displaySpan(parent, cell) {
  if (cell.textFrom === null) return;
  parent.classList.add("goya-live");
  const value = document.createElement("span");
  value.dataset.goyaTextFrom = cell.textFrom;
  parent.append(value);
  displays.push([value, cell.textFrom, displayContext(cell.text, cell.textFrom)]);
}
// `@iden` (decision D14) through the browser's native accessibility: the
// label is the accessible name; the hint is the accessible description,
// referenced from a visually hidden element.
function identify(target, cell, index) {
  if (cell.iden === null) return;
  const [label, hint] = cell.iden;
  // A plain container gets a name only with a role that takes one.
  if (cell.kind === "cell") target.setAttribute("role", "group");
  // A remote-only instruction is omitted rather than named as "Press OK".
  if (label !== null && label !== "") target.setAttribute("aria-label", label);
  if (hint !== null) {
    const described = document.createElement("span");
    described.id = `goya-hint-${index}`;
    described.className = "goya-sr";
    described.textContent = hint;
    document.body.append(described);
    target.setAttribute("aria-describedby", described.id);
  }
}
for (const [index, cell] of CELLS.entries()) {
  const el = document.createElement("div");
  el.className = "goya-el";
  el.dataset.goyaId = cell.id;
  if (cell.fill !== null) el.style.backgroundColor = cell.fill;
  if (cell.kind === "button") {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "goya-control";
    if (cell.text !== null) button.textContent = cell.text;
    displaySpan(button, cell);
    el.append(button);
    identify(button, cell, index);
    controls.set(cell.id, button);
  } else if (cell.kind === "input") {
    const input = document.createElement("input");
    input.type = "text";
    input.className = "goya-control";
    input.autocomplete = "off";
    input.spellcheck = false;
    // `text:` is the field's initial content (GSSH decision D3).
    if (cell.text !== null) input.value = cell.text;
    input.addEventListener("input", () => { input.setCustomValidity(""); input.removeAttribute("aria-invalid"); });
    el.append(input);
    identify(input, cell, index);
    controls.set(cell.id, input);
  } else if (cell.text !== null || cell.textFrom !== null) {
    const text = document.createElement("span");
    text.className = "goya-text";
    if (cell.text !== null) text.textContent = cell.text;
    displaySpan(text, cell);
    el.append(text);
  }
  if (cell.kind === "cell") identify(el, cell, index);
  let canvas = null;
  if (cell.draw !== null) {
    canvas = document.createElement("canvas");
    canvas.className = "goya-gv";
    el.append(canvas);
  }
  if (cell.fillTo !== null && cell.fill !== null) {
    const veil = document.createElement("div");
    veil.className = "goya-breathe";
    veil.style.background = `linear-gradient(115deg, ${cell.fillTo[0]}, ${cell.fill})`;
    veil.style.animationDuration = `${cell.fillTo[1]}s`;
    el.append(veil);
  }
  let stroke = null;
  if (cell.stroke !== null) {
    stroke = document.createElement("div");
    stroke.className = "goya-stroke";
    stroke.style.setProperty("--goya-stroke", cell.stroke[0]);
  }
  // A scroll viewport's children live in an inner scroller, so the
  // viewport's own fill, text and stroke stay put while its content moves.
  let container = el;
  if (cell.scroll) {
    container = document.createElement("div");
    container.className = "goya-scroller";
    el.append(container);
  }
  elements.push({ el, stroke, container, canvas });
  (cell.parent === null ? root : elements[cell.parent].container).append(el);
}
// Stroke overlays are appended after children so they draw above content.
for (const { el, stroke } of elements) if (stroke !== null) el.append(stroke);

// Declared before the first layout. The painter is attached later, after
// durable state is restored, so this first pass only sizes the canvases.
let graphicsReady = false;
function layout() {
  const resolved = resolve(CELLS, root.clientWidth, root.clientHeight);
  resolved.forEach(({ rect, textSize, strokeWidth }, i) => {
    const parent = CELLS[i].parent === null ? { x: 0, y: 0 } : resolved[CELLS[i].parent].rect;
    const style = elements[i].el.style;
    style.left = `${(rect.x - parent.x) / 1000}px`;
    style.top = `${(rect.y - parent.y) / 1000}px`;
    style.width = `${rect.width / 1000}px`;
    style.height = `${rect.height / 1000}px`;
    if (textSize !== null) style.fontSize = `${textSize / 1000}px`;
    if (strokeWidth !== null) elements[i].stroke.style.setProperty("--goya-stroke-width", `${strokeWidth / 1000}px`);
    const appear = CELLS[i].appear;
    if (appear !== null) {
      const parentRect = CELLS[i].parent === null
        ? { width: root.clientWidth * 1000, height: root.clientHeight * 1000 }
        : resolved[CELLS[i].parent].rect;
      style.setProperty("--goya-dx", `${appear.from[0] * parentRect.width / 1000}px`);
      style.setProperty("--goya-dy", `${appear.from[1] * parentRect.height / 1000}px`);
      style.setProperty("--goya-appear", `${appear.s}s`);
    }
    const canvas = elements[i].canvas;
    if (canvas !== null) {
      const width = Math.max(1, Math.round(rect.width / 1000));
      const height = Math.max(1, Math.round(rect.height / 1000));
      if (canvas.width !== width || canvas.height !== height) {
        canvas.width = width;
        canvas.height = height;
      }
    }
  });
  if (graphicsReady) renderDraws(state, true);
}
layout();
addEventListener("resize", () => { applyFrame(); layout(); });

function publish(detail) {
  document.dispatchEvent(new CustomEvent("goya:transition", { detail: Object.freeze(detail) }));
}
function render(value) {
  return value.type === "int" ? String(value.value) : value.value;
}
function holds(cond, value) {
  if (cond === null) return true;
  const cur = value.get(cond[0]);
  if (cur === undefined) return false;
  if (cond[1] === "ge") return cur.type === "int" && cur.value >= BigInt(cond[2]);
  if (cur.type === "int") return cur.value === BigInt(cond[2]);
  return cur.value === cond[2];
}
function presentAt(index, value) {
  let cursor = index;
  while (cursor !== null) {
    if (!holds(CELLS[cursor].present, value)) return false;
    cursor = CELLS[cursor].parent;
  }
  return true;
}
const wasPresent = elements.map(() => false);
function replayAppear(index) {
  const spec = CELLS[index].appear;
  if (spec === null) return;
  const el = elements[index].el;
  if (reduceMotion) {
    el.style.opacity = "";
    el.classList.remove("goya-appear");
    return;
  }
  el.classList.remove("goya-appear");
  el.style.opacity = "0";
  void el.offsetWidth;
  el.style.opacity = "";
  el.classList.add("goya-appear");
}
function applyPresence(value) {
  for (let i = 0; i < elements.length; i++) {
    const on = presentAt(i, value);
    elements[i].el.hidden = !on;
    if (on && !wasPresent[i]) replayAppear(i);
    wasPresent[i] = on;
  }
}
const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
const inventories = {};
const painters = [];
function setupGraphics() {
  if (typeof GoyaGvi === "undefined") return;
  for (const name of Object.keys(GRAPHICS)) {
    const graphic = GRAPHICS[name];
    inventories[name] = GoyaGvi.open(graphic.data, graphic.sha, graphic.radices);
  }
  for (let i = 0; i < elements.length; i++) {
    const canvas = elements[i].canvas;
    const draw = CELLS[i].draw;
    if (canvas === null || draw === null) continue;
    painters.push({
      canvas,
      ctx: canvas.getContext("2d"),
      draw,
      inv: inventories[draw.g] || null,
      shown: null,
      target: null,
      timer: null,
    });
  }
  graphicsReady = painters.length === 0 || painters.every((painter) => painter.inv !== null);
}
function sameState(left, right) {
  if (left === null || right === null || left.length !== right.length) return false;
  for (let i = 0; i < left.length; i++) if (left[i] !== right[i]) return false;
  return true;
}
function chooseDraw(draw, value) {
  const chosen = [];
  for (const rules of draw.dims) {
    let picked = 0;
    for (const rule of rules) {
      if (holds(rule[0], value)) { picked = rule[1]; break; }
    }
    chosen.push(picked);
  }
  return chosen;
}
function paintDraw(painter) {
  if (painter.inv === null || painter.shown === null || painter.canvas.width < 1) return;
  painter.inv.paint(
    painter.ctx,
    painter.shown,
    painter.canvas.width / painter.inv.width,
    painter.canvas.height / painter.inv.height,
  );
}
function stepDraw(painter) {
  painter.timer = null;
  const steps = GRAPHICS[painter.draw.g].steps;
  let wait = 0;
  for (let k = 0; k < painter.shown.length; k++) {
    if (painter.shown[k] === painter.target[k]) continue;
    if (steps[k] > 0 && !reduceMotion) {
      painter.shown[k] += painter.target[k] > painter.shown[k] ? 1 : -1;
      wait = wait ? Math.min(wait, steps[k]) : steps[k];
    } else painter.shown[k] = painter.target[k];
  }
  paintDraw(painter);
  if (!sameState(painter.shown, painter.target)) painter.timer = setTimeout(() => stepDraw(painter), wait);
}
function renderDraws(value, snap) {
  for (const painter of painters) {
    const next = chooseDraw(painter.draw, value);
    if (snap || painter.shown === null) {
      if (painter.timer !== null) { clearTimeout(painter.timer); painter.timer = null; }
      painter.shown = next.slice();
      painter.target = next;
      paintDraw(painter);
      continue;
    }
    painter.target = next;
    if (painter.timer === null && !sameState(painter.shown, next)) stepDraw(painter);
  }
}
function showState(state) {
  for (const [span, key] of displays) {
    const text = shownText(state.get(key));
    if (span.textContent !== text) span.textContent = text;
  }
  applyPresence(state);
  if (graphicsReady) renderDraws(state, false);
}
// Changes of shown values are announced to assistive technology (D9): what
// changed and how, in a visually hidden polite live region. Nothing is
// announced at load or for a refused transition.
let announcer = null;
let announcementFrame = null;
if (displays.length > 0) {
  announcer = document.createElement("p");
  announcer.id = "goya-announce";
  announcer.className = "goya-sr";
  announcer.setAttribute("aria-live", "polite");
  announcer.setAttribute("aria-atomic", "true");
  document.body.append(announcer);
}
function announce(before, after) {
  const message = announcement(displays.map(([, key, context]) => [context, before.get(key), after.get(key)]));
  if (message === null) return;
  // A queued repeat must not overwrite a newer transition's announcement.
  if (announcementFrame !== null) {
    cancelAnimationFrame(announcementFrame);
    announcementFrame = null;
  }
  // Repeating the same text is cleared first so it is announced again.
  if (announcer.textContent === message) {
    announcer.textContent = "";
    announcementFrame = requestAnimationFrame(() => {
      announcementFrame = null;
      announcer.textContent = message;
    });
  } else {
    announcer.textContent = message;
  }
}

let state = new Map(INITIAL_STATE.map(([key, type, value]) => [
  key,
  Object.freeze({ type, value: type === "int" ? BigInt(value) : value }),
]));
// Every display binding shows its value before the scene is first painted:
// the scene's elements are created by this module, in the same task as the
// end of the restore. A failed restore is reported and the app starts from
// the authored values.
let runtime = null;
let persist = null;
let notice = "";
if (restored !== null) {
  state = restored.state;
  document.documentElement.dataset.goyaRestore = restored.ok ? "ok" : restored.code;
  if (restored.migrated !== null) document.documentElement.dataset.goyaMigrated = restored.migrated;
  if (!restored.ok) notice = `Saved state could not be restored (${restored.code}); starting from initial values.`;
  persist = persister(ONTOLOGY, INITIAL_STATE, restored.preserved, restored.seen);
}
setupGraphics();
showState(state);
status.textContent = notice;
// Run one activation. With durable state the transition is prepared, its
// record written, and only then committed; an IndexedDB write completes
// later, and until then further activations fail with REENTRANT_DISPATCH.
function activate(actionId, inputs, done) {
  if (persist === null) return done(runtime.dispatch(actionId, inputs));
  const prepared = runtime.prepare(actionId, inputs);
  if (!prepared.ok) return done(prepared);
  const settle = (written) => {
    if (written.wrote) notice = "";
    done(written.ok ? prepared.commit() : prepared.abandon(written.conflict ? "STORAGE_CONFLICT" : "STORAGE_FAILED"));
  };
  let written;
  try { written = persist(runtime.snapshot().state, prepared.writes); } catch { written = { ok: false, conflict: false, wrote: false }; }
  if (!(written instanceof Promise)) return settle(written);
  root.setAttribute("aria-busy", "true");
  written.then(settle, () => settle({ ok: false, conflict: false, wrote: false }))
    .finally(() => root.removeAttribute("aria-busy"));
}
const FAILURE_HINTS = {
  STORAGE_CONFLICT: " (saved state was changed in another window; reload to continue)",
  REENTRANT_DISPATCH: " (the previous action is still being saved)",
};

if (createRuntime !== null) {
  runtime = createRuntime(state);
  for (const [buttonId, actionId, fields] of ACTIVATIONS) {
    controls.get(buttonId).addEventListener("click", () => {
      // One snapshot of every bound field before converting any of them.
      const texts = fields.map(([key, type, fieldId]) => [key, type, fieldId, controls.get(fieldId).value]);
      const inputs = new Map();
      for (const [key, type, fieldId, text] of texts) {
        const converted = convertField(type, text);
        if (!converted.ok) {
          const field = controls.get(fieldId);
          field.setAttribute("aria-invalid", "true");
          field.setCustomValidity(converted.error === "NOT_CANONICAL_INT" ? "Enter a whole number." : "This text cannot be used.");
          field.reportValidity();
          publish({ action: actionId, ok: false, field: fieldId, code: converted.error, revision: String(runtime.revision) });
          return;
        }
        inputs.set(key, converted.value);
      }
      const before = runtime.snapshot().state;
      activate(actionId, inputs, (result) => {
        const snapshot = runtime.snapshot();
        // A committed transition is shown before the next frame and
        // announced; a refused one changes nothing on screen.
        if (result.ok) {
          showState(snapshot.state);
          if (announcer !== null) announce(before, snapshot.state);
        }
        status.textContent = result.ok ? notice : `Action failed: ${result.code}${FAILURE_HINTS[result.code] ?? ""}`;
        publish({
          action: actionId,
          ok: result.ok,
          code: result.ok ? null : result.code,
          revision: String(snapshot.revision),
          state: Object.fromEntries([...snapshot.state].map(([k, v]) => [k, [v.type, render(v)]])),
        });
      });
    });
  }
}
"#;

#[cfg(test)]
mod remote_copy_tests {
    use super::browser_copy;

    #[test]
    fn tv_remote_instructions_are_not_browser_copy() {
        let cases = [
            (
                "Find the eight pairs. Arrow keys move between cards; OK turns one over.",
                "Find the eight pairs.",
            ),
            (
                "Not a match. Press OK to turn them back.",
                "Not a match. Turn them back.",
            ),
            ("Press OK to turn it over", "Turn it over"),
            (
                "Press OK to turn it over and compare",
                "Turn it over and compare",
            ),
            (
                "Press OK to turn the two cards back",
                "Turn the two cards back",
            ),
            ("press OK to flip the card", "Flip the card"),
            (
                "Find the eight pairs. Arrow keys move between\ncards; OK turns one over.",
                "Find the eight pairs.",
            ),
            (
                "Not a match. Press\nOK to turn them back.",
                "Not a match. Turn them back.",
            ),
            (
                "Line one stays.\nPress OK to go on.",
                "Line one stays.\nGo on.",
            ),
            ("Keep\nthis wrap.", "Keep\nthis wrap."),
            ("Arrow keys move between cards; OK turns one over.", ""),
            ("Press OK", ""),
            ("Already matched", "Already matched"),
            ("OK", "OK"),
            ("Press OKAY to save", "Press OKAY to save"),
            ("New game", "New game"),
            ("Pick a card.", "Pick a card."),
        ];
        for (from, to) in cases {
            assert_eq!(browser_copy(from).as_ref(), to, "{from}");
        }
    }
}
