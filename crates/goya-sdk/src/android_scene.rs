//! Aragon scene compiler: a GSSH/3 scene to Kotlin for Android.
//!
//! Every construct GSSH/3 admits is either emitted or rejected with the
//! exact source range of that construct (see `docs/aragon/android-scene.md`).
//!
//! Output, all in one package:
//!
//! * `<Spec>.kt`: pure Kotlin with no Android imports. `resolve` mirrors
//!   `goya_lang::layout` in millipoints, `scrollLimits` mirrors
//!   `Frame::scroll_limits`, and `convertInt`/`textWithinLimit` mirror
//!   `handoff::convert_field`. It runs on a plain JVM so it can be
//!   differential-tested against the Rust reference.
//! * `GoyaSceneView.kt`: Android views. One cell is one `ViewGroup`; one
//!   point is one dp; pixel edges are rounded independently so adjacent
//!   cells never gap or overlap.
//! * `GoyaD1Runtime.kt` (only when the scene has interactions): the
//!   canonical D1 runtime emitter's output.

use crate::kotlin::{is_identifier, AndroidSourceMap, KotlinFile, KotlinWriter, SourceMapEntry};
use goya_lang::handoff::{admit_v3, Origin, SemanticHandoffV2};
use goya_lang::{
    CellKind, InteractionClass, InteractionType, InteractionValue, SceneCell, SourceRange,
    StateLifetime, StrictParseError,
};
use std::{collections::BTreeMap, fmt};

/// Largest scene the compiler emits.
pub const ANDROID_SCENE_MAX_CELLS: usize = 1024;
/// Largest UTF-8 text of one element (labels and cell text).
pub const ANDROID_SCENE_MAX_TEXT_BYTES: usize = 16 * 1024;
/// Default package of generated sources.
pub const ANDROID_SCENE_PACKAGE: &str = "dev.goya.preview";
/// Default name of the generated layout specification object.
pub const ANDROID_SCENE_SPEC: &str = "GoyaSceneSpec";

const CHUNK: usize = 32;
// D1 limits the generated runtime enforces at construction.
const D1_MAX_TEXT_BYTES: usize = 64 * 1024;
const D1_MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;

/// Names used in generated sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidSceneOptions {
    /// Dotted Kotlin package.
    pub package: String,
    /// Name of the pure-Kotlin layout specification object.
    pub spec_object: String,
}

impl Default for AndroidSceneOptions {
    fn default() -> Self {
        Self {
            package: ANDROID_SCENE_PACKAGE.to_owned(),
            spec_object: ANDROID_SCENE_SPEC.to_owned(),
        }
    }
}

/// Generated Kotlin and its source map for one admitted source revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidScene {
    pub package: String,
    /// The specification, the view file, and the D1 runtime when present.
    pub files: Vec<KotlinFile>,
    pub source_map: AndroidSourceMap,
    /// Authored element IDs in depth-first order, the index order of the
    /// specification's arrays.
    pub cell_ids: Vec<String>,
}

/// Why a construct was not emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AndroidSceneReason {
    /// GSSH/3 admission failed (absolute lengths are GY-E1009).
    Admission(Box<StrictParseError>),
    /// The construct is admitted by GSSH/3 but Aragon does not implement it.
    Unsupported(&'static str),
    /// The canonical D1 runtime emitter refused the scene.
    Runtime(String),
    /// The options name an invalid package or object.
    InvalidOptions,
}

/// A refusal tied to the exact source range of the refused construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidSceneError {
    pub reason: AndroidSceneReason,
    /// The construct by identity, when it has one.
    pub origin: Option<Origin>,
    /// Half-open UTF-8 byte range in the source.
    pub range: SourceRange,
}

impl fmt::Display for AndroidSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}: ", self.range.start, self.range.end)?;
        match &self.reason {
            AndroidSceneReason::Admission(error) => write!(f, "{error}"),
            AndroidSceneReason::Unsupported(what) => write!(f, "Aragon does not support {what}"),
            AndroidSceneReason::Runtime(error) => write!(f, "D1 runtime: {error}"),
            AndroidSceneReason::InvalidOptions => f.write_str("invalid package or object name"),
        }
    }
}

impl std::error::Error for AndroidSceneError {}

/// Compile with the default package and specification name.
pub fn compile_android_scene(source: &str) -> Result<AndroidScene, AndroidSceneError> {
    compile_android_scene_with(source, &AndroidSceneOptions::default())
}

struct Plan<'a> {
    cell: &'a SceneCell,
    id: &'a str,
    parent: Option<usize>,
    overflow: bool,
    /// Text after the reference wrap transform.
    text: Option<String>,
}

enum Dim {
    Prop(f64),
    Fill,
}

/// Compile a GSSH/3 source to Kotlin.
pub fn compile_android_scene_with(
    source: &str,
    options: &AndroidSceneOptions,
) -> Result<AndroidScene, AndroidSceneError> {
    compile_scene(source, options, false)
}

pub(crate) fn compile_tv_scene(source: &str) -> Result<AndroidScene, AndroidSceneError> {
    compile_scene(source, &AndroidSceneOptions::default(), true)
}

fn compile_scene(
    source: &str,
    options: &AndroidSceneOptions,
    tv: bool,
) -> Result<AndroidScene, AndroidSceneError> {
    let whole = SourceRange {
        start: 0,
        end: source.len(),
    };
    if !options.package.split('.').all(is_identifier)
        || !crate::is_kotlin_class_name(&options.spec_object)
        || is_reserved_class(&options.spec_object)
    {
        return Err(AndroidSceneError {
            reason: AndroidSceneReason::InvalidOptions,
            origin: None,
            range: whole,
        });
    }
    let handoff = admit_v3(source).map_err(|error| AndroidSceneError {
        range: error.range,
        reason: AndroidSceneReason::Admission(Box::new(error)),
        origin: None,
    })?;
    let refuse = |origin: Origin, what: &'static str| AndroidSceneError {
        range: handoff.origin(&origin).unwrap_or(whole),
        reason: AndroidSceneReason::Unsupported(what),
        origin: Some(origin),
    };
    let scene = &handoff.parsed().scene;

    let mut plans = Vec::new();
    collect(&scene.root, None, false, &mut plans);
    // The reference wrap transform is viewport-independent; take its text
    // from the reference frame so Android never re-implements wrapping.
    let frame = goya_lang::layout(scene, 1.0, 1.0);
    assert_eq!(frame.nodes.len(), plans.len());
    for (plan, node) in plans.iter_mut().zip(&frame.nodes) {
        plan.text = node.text.clone();
    }

    let mut name_counts = BTreeMap::<&str, usize>::new();
    for plan in &plans {
        *name_counts.entry(plan.cell.name.as_str()).or_default() += 1;
    }
    for (index, plan) in plans.iter().enumerate() {
        let cell = plan.cell;
        let id = plan.id.to_owned();
        let property = |name: &str| Origin::Property {
            element: id.clone(),
            property: name.to_owned(),
        };
        if index == ANDROID_SCENE_MAX_CELLS {
            return Err(refuse(
                Origin::Element(id),
                "more than 1024 elements in one scene",
            ));
        }
        if cell.anchor.is_some() {
            return Err(refuse(
                property("text_anchor"),
                "text-anchor (the reference renderer ignores it)",
            ));
        }
        if cell.stroke.is_some() != cell.stroke_width_p.is_some() {
            let (name, what) = if cell.stroke.is_some() {
                ("stroke", "a stroke colour without stroke-width-p")
            } else {
                ("stroke_width_p", "stroke-width-p without a stroke colour")
            };
            return Err(refuse(property(name), what));
        }
        if cell.gestures.is_some() {
            return Err(refuse(
                property("gestures"),
                "clipboard gesture policies on Android",
            ));
        }
        if cell.kind == CellKind::Input && cell.text.is_some() {
            return Err(refuse(
                property("text"),
                "`text:` on @input (initial content, decision D3) until Aragon implements it",
            ));
        }
        if matches!(cell.kind, CellKind::Button | CellKind::Input) {
            if let Some(child) = cell.children.first() {
                let child_id = child.authored_id.clone().unwrap_or_default();
                return Err(refuse(
                    Origin::Element(child_id),
                    "elements nested inside a button or input",
                ));
            }
        }
        if plan
            .text
            .as_ref()
            .is_some_and(|text| text.len() > ANDROID_SCENE_MAX_TEXT_BYTES)
        {
            return Err(refuse(
                property("text"),
                "element text above 16 KiB of UTF-8",
            ));
        }
        if cell.scroll && name_counts[cell.name.as_str()] > 1 {
            return Err(refuse(
                Origin::Element(id),
                "a scroll viewport whose structural name is not unique (the reference keys scroll offsets by name)",
            ));
        }
    }

    let mut snapshot_bytes = 8usize;
    for decl in handoff.state() {
        if decl.lifetime == StateLifetime::Durable {
            return Err(refuse(
                Origin::State(decl.key.clone()),
                "durable state before the Android storage adapter (gate ARG-G07)",
            ));
        }
        let value_bytes = match &decl.initial {
            InteractionValue::Int(_) => 8,
            InteractionValue::Bool(_) => 1,
            InteractionValue::Text(text) if text.len() > D1_MAX_TEXT_BYTES => {
                return Err(refuse(
                    Origin::State(decl.key.clone()),
                    "an initial text value above the 64 KiB D1 limit",
                ));
            }
            InteractionValue::Text(text) => text.len(),
        };
        snapshot_bytes += 8 + decl.key.len() + value_bytes;
    }
    if snapshot_bytes > D1_MAX_SNAPSHOT_BYTES {
        let range = handoff
            .parsed()
            .declarations
            .state
            .as_ref()
            .map_or(whole, |(_, range)| *range);
        return Err(AndroidSceneError {
            reason: AndroidSceneReason::Unsupported(
                "an initial state snapshot above the 1 MiB D1 limit",
            ),
            origin: None,
            range,
        });
    }
    for interaction in &scene.interactions {
        let id = interaction.authored_id.clone().unwrap_or_default();
        if interaction.class == InteractionClass::D2 {
            return Err(refuse(
                Origin::Interaction(id),
                "D2 interactions (no Android capability adapters)",
            ));
        }
    }

    let package = options.package.as_str();
    let spec = options.spec_object.as_str();
    let mut files = Vec::new();
    let mut entries = Vec::new();
    let (file, mapped) = emit_spec(&handoff, &plans, package, spec);
    files.push(file);
    entries.extend(mapped);
    let (file, mapped) = emit_view(&handoff, &plans, package, spec, tv);
    files.push(file);
    entries.extend(mapped);
    if !scene.interactions.is_empty() {
        let runtime = crate::android_d1_runtime::emit_for_parsed(handoff.parsed(), "GoyaD1Runtime")
            .map_err(|error| AndroidSceneError {
                reason: AndroidSceneReason::Runtime(error.to_string()),
                origin: scene.interactions[0]
                    .authored_id
                    .clone()
                    .map(Origin::Interaction),
                range: scene.interactions[0]
                    .authored_id
                    .clone()
                    .and_then(|id| handoff.origin(&Origin::Interaction(id)))
                    .unwrap_or(whole),
            })?;
        files.push(KotlinFile {
            name: "GoyaD1Runtime.kt".to_owned(),
            text: format!("package {package}\n\n{}", runtime.kotlin),
        });
    }
    entries.sort_by(|a, b| {
        (a.file.as_str(), a.generated.start, a.generated.end).cmp(&(
            b.file.as_str(),
            b.generated.start,
            b.generated.end,
        ))
    });
    Ok(AndroidScene {
        package: package.to_owned(),
        files,
        source_map: AndroidSourceMap {
            source_digest: handoff.parsed().source_digest,
            entries,
        },
        cell_ids: plans.iter().map(|plan| plan.id.to_owned()).collect(),
    })
}

fn is_reserved_class(name: &str) -> bool {
    matches!(
        name,
        "GoyaSceneView"
            | "GoyaCell"
            | "GoyaScrollLayer"
            | "GoyaSession"
            | "GoyaD1Runtime"
            | "MainActivity"
    )
}

fn collect<'a>(
    cell: &'a SceneCell,
    parent: Option<usize>,
    overflow: bool,
    out: &mut Vec<Plan<'a>>,
) {
    let index = out.len();
    out.push(Plan {
        cell,
        // Strict admission requires a non-empty authored ID on every element.
        id: cell.authored_id.as_deref().unwrap_or_default(),
        parent,
        overflow,
        text: None,
    });
    for child in &cell.children {
        collect(child, Some(index), cell.scroll, out);
    }
}

/// The reference fraction rule: clamped to [0, 1] of the parent, or only
/// to >= 0 inside a scroll viewport. Absolute lengths never reach here:
/// GSSH/3 admission rejects them (GY-E1009).
fn fraction(value: f64, overflow: bool) -> f64 {
    if overflow {
        value.max(0.0)
    } else {
        value.clamp(0.0, 1.0)
    }
}

fn extent(cell: &SceneCell, overflow: bool) -> (Dim, Dim, Option<&'static str>) {
    match cell.extent_p {
        Some(e) => (
            Dim::Prop(fraction(e.x, overflow)),
            Dim::Prop(fraction(e.y, overflow)),
            Some("extent_p"),
        ),
        None => (Dim::Fill, Dim::Fill, None),
    }
}

fn position(cell: &SceneCell, overflow: bool) -> (Option<Dim>, Option<Dim>, Option<&'static str>) {
    match cell.position_p {
        Some(p) => (
            Some(Dim::Prop(fraction(p.x, overflow))),
            Some(Dim::Prop(fraction(p.y, overflow))),
            Some("position_p"),
        ),
        None => (None, None, None),
    }
}

fn write_extent(w: &mut KotlinWriter, dim: &Dim, parent_size: &'static str) {
    match dim {
        Dim::Prop(fraction) => {
            w.code("prop(")
                .double(*fraction)
                .code(", ")
                .code(parent_size)
                .code(")");
        }
        Dim::Fill => {
            w.code(parent_size);
        }
    }
}

fn write_position(
    w: &mut KotlinWriter,
    dim: &Option<Dim>,
    parent_origin: &'static str,
    parent_size: &'static str,
) {
    match dim {
        Some(Dim::Prop(fraction)) => {
            w.code("satAdd(")
                .code(parent_origin)
                .code(", prop(")
                .double(*fraction)
                .code(", ")
                .code(parent_size)
                .code("))");
        }
        Some(Dim::Fill) | None => {
            w.code(parent_origin);
        }
    }
}

fn emit_spec(
    handoff: &SemanticHandoffV2,
    plans: &[Plan<'_>],
    package: &str,
    spec: &str,
) -> (KotlinFile, Vec<SourceMapEntry>) {
    let mut w = KotlinWriter::new(format!("{spec}.kt"));
    write_package(&mut w, package);
    w.code("/** Generated by Aragon from a GSSH/3 scene; do not edit. Mirrors goya_lang::layout\n")
        .code(" * (millipoints), Frame::scroll_limits and handoff::convert_field. No Android imports. */\n")
        .code("object ")
        .ident(spec)
        .code(" {\n    const val CELL_COUNT: Int = ")
        .int(plans.len())
        .code("\n    const val MAX_FIELD_TEXT_BYTES: Int = 65536\n    val IDS: Array<String> = arrayOf(");
    for (index, plan) in plans.iter().enumerate() {
        if index > 0 {
            w.code(", ");
        }
        w.mapped(Origin::Element(plan.id.to_owned()), |w| {
            w.string(plan.id);
        });
    }
    w.code(")\n    val PARENTS: IntArray = intArrayOf(");
    for (index, plan) in plans.iter().enumerate() {
        if index > 0 {
            w.code(", ");
        }
        match plan.parent {
            Some(parent) => w.int(parent),
            None => w.code("-1"),
        };
    }
    w.code(")\n    val SCROLL: BooleanArray = booleanArrayOf(");
    for (index, plan) in plans.iter().enumerate() {
        if index > 0 {
            w.code(", ");
        }
        w.boolean(plan.cell.scroll);
    }
    // Text size: fraction of the element's height, >= 0. Stroke width:
    // fraction of its shorter side, in [0, 0.5]. -1 means not authored.
    for (array, property, clamp) in [
        ("TEXT_SIZE_P", "text_size_p", f64::INFINITY),
        ("STROKE_WIDTH_P", "stroke_width_p", 0.5),
    ] {
        w.code(")\n    val ")
            .ident(array)
            .code(": DoubleArray = doubleArrayOf(");
        for (index, plan) in plans.iter().enumerate() {
            if index > 0 {
                w.code(", ");
            }
            let value = if property == "text_size_p" {
                plan.cell.text_size_p
            } else {
                plan.cell.stroke_width_p
            };
            match value {
                Some(f) => {
                    w.mapped(
                        Origin::Property {
                            element: plan.id.to_owned(),
                            property: property.to_owned(),
                        },
                        |w| {
                            w.double(f.clamp(0.0, clamp));
                        },
                    );
                }
                None => {
                    w.code("-1.0");
                }
            }
        }
    }
    w.code(")\n\n    /** Millipoint rects [x, y, width, height] per cell for a viewport in points. */\n")
        .code("    fun resolve(widthPt: Double, heightPt: Double): LongArray {\n")
        .code("        val r = LongArray(4 * CELL_COUNT)\n")
        .code("        val vw = (widthPt * 1000.0).toLong()\n")
        .code("        val vh = (heightPt * 1000.0).toLong()\n");
    for chunk in 0..plans.len().div_ceil(CHUNK) {
        w.code("        resolve").int(chunk).code("(r, vw, vh)\n");
    }
    w.code("        return r\n    }\n");
    for (chunk, group) in plans.chunks(CHUNK).enumerate() {
        w.code("\n    private fun resolve")
            .int(chunk)
            .code("(r: LongArray, vw: Long, vh: Long) {\n")
            .code("        var px: Long\n        var py: Long\n        var pw: Long\n        var ph: Long\n");
        for (offset, plan) in group.iter().enumerate() {
            let index = chunk * CHUNK + offset;
            match plan.parent {
                None => {
                    w.code("        px = 0L; py = 0L; pw = vw; ph = vh\n");
                }
                Some(parent) => {
                    w.code("        px = r[")
                        .int(4 * parent)
                        .code("]; py = r[")
                        .int(4 * parent + 1)
                        .code("]; pw = r[")
                        .int(4 * parent + 2)
                        .code("]; ph = r[")
                        .int(4 * parent + 3)
                        .code("]\n");
                }
            }
            let id = plan.id.to_owned();
            let (ew, eh, extent_property) = extent(plan.cell, plan.overflow);
            let (x, y, position_property) = position(plan.cell, plan.overflow);
            w.code("        place(r, ")
                .int(index)
                .code(", px, py, pw, ph, ");
            let write_extents = |w: &mut KotlinWriter| {
                write_extent(w, &ew, "pw");
                w.code(", ");
                write_extent(w, &eh, "ph");
            };
            match extent_property {
                Some(property) => {
                    w.mapped(
                        Origin::Property {
                            element: id.clone(),
                            property: property.to_owned(),
                        },
                        write_extents,
                    );
                }
                None => {
                    write_extents(&mut w);
                }
            }
            w.code(", ");
            let write_positions = |w: &mut KotlinWriter| {
                write_position(w, &x, "px", "pw");
                w.code(", ");
                write_position(w, &y, "py", "ph");
            };
            match position_property {
                Some(property) => {
                    w.mapped(
                        Origin::Property {
                            element: id.clone(),
                            property: property.to_owned(),
                        },
                        write_positions,
                    );
                }
                None => {
                    write_positions(&mut w);
                }
            }
            w.code(", ").boolean(plan.overflow).code(")\n");
        }
        w.code("    }\n");
    }
    w.code(SPEC_HELPERS);
    w.code("}\n");
    w.finish(handoff)
}

const SPEC_HELPERS: &str = r#"
    private fun place(r: LongArray, i: Int, px: Long, py: Long, pw: Long, ph: Long, w0: Long, h0: Long, x0: Long, y0: Long, overflow: Boolean) {
        val w = maxOf(w0, 0L)
        val h = maxOf(h0, 0L)
        val right = satAdd(px, pw)
        val bottom = satAdd(py, ph)
        val x = if (overflow) maxOf(x0, px) else minOf(maxOf(x0, px), right)
        val y = if (overflow) maxOf(y0, py) else minOf(maxOf(y0, py), bottom)
        r[4 * i] = x
        r[4 * i + 1] = y
        r[4 * i + 2] = if (overflow) w else maxOf(minOf(w, satSub(right, x)), 0L)
        r[4 * i + 3] = if (overflow) h else maxOf(minOf(h, satSub(bottom, y)), 0L)
    }

    private fun prop(fraction: Double, parent: Long): Long = (fraction * parent.toDouble()).toLong()

    fun satAdd(a: Long, b: Long): Long {
        val s = a + b
        return if (((a xor s) and (b xor s)) < 0L) (if (a < 0L) Long.MIN_VALUE else Long.MAX_VALUE) else s
    }

    fun satSub(a: Long, b: Long): Long {
        val d = a - b
        return if (((a xor b) and (a xor d)) < 0L) (if (a < 0L) Long.MIN_VALUE else Long.MAX_VALUE) else d
    }

    fun isAncestor(ancestor: Int, cell: Int): Boolean {
        var p = PARENTS[cell]
        while (p >= 0) {
            if (p == ancestor) return true
            p = PARENTS[p]
        }
        return false
    }

    /** Maximum [x, y] scroll offsets in millipoints of scroll cell [i]. */
    fun scrollLimits(r: LongArray, i: Int): LongArray {
        if (!SCROLL[i]) return longArrayOf(0L, 0L)
        val viewRight = satAdd(r[4 * i], r[4 * i + 2])
        val viewBottom = satAdd(r[4 * i + 1], r[4 * i + 3])
        var right = viewRight
        var bottom = viewBottom
        for (j in 0 until CELL_COUNT) {
            if (isAncestor(i, j)) {
                right = maxOf(right, satAdd(r[4 * j], r[4 * j + 2]))
                bottom = maxOf(bottom, satAdd(r[4 * j + 1], r[4 * j + 3]))
            }
        }
        return longArrayOf(satSub(right, viewRight), satSub(bottom, viewBottom))
    }

    /** Resolved text size of cell [i] in millipoints, or -1. */
    fun textSizeMp(r: LongArray, i: Int): Long =
        if (TEXT_SIZE_P[i] < 0.0) -1L else (TEXT_SIZE_P[i] * r[4 * i + 3].toDouble()).toLong()

    /** Resolved stroke width of cell [i] in millipoints, or -1. */
    fun strokeWidthMp(r: LongArray, i: Int): Long =
        if (STROKE_WIDTH_P[i] < 0.0) -1L else (STROKE_WIDTH_P[i] * minOf(r[4 * i + 2], r[4 * i + 3]).toDouble()).toLong()

    /** handoff::convert_field for `int`; null is NotCanonicalInt. */
    fun convertInt(text: String): Long? {
        val digits = if (text.startsWith("-")) text.substring(1) else text
        if (digits.isEmpty() || text == "-0") return null
        for (c in digits) if (c < '0' || c > '9') return null
        if (digits.length > 1 && digits[0] == '0') return null
        return text.toLongOrNull()
    }

    /** handoff::convert_field for `text`; false is TextLimit. */
    fun textWithinLimit(text: String): Boolean = utf8Length(text) <= MAX_FIELD_TEXT_BYTES

    fun utf8Length(text: String): Int {
        var bytes = 0
        var i = 0
        while (i < text.length) {
            val c = text[i]
            if (c.isHighSurrogate() && i + 1 < text.length && text[i + 1].isLowSurrogate()) {
                bytes += 4
                i += 2
                continue
            }
            bytes += when {
                c.code < 0x80 -> 1
                c.code < 0x800 -> 2
                else -> 3
            }
            i += 1
        }
        return bytes
    }
"#;

fn write_package(w: &mut KotlinWriter, package: &str) {
    w.code("package ");
    for (index, segment) in package.split('.').enumerate() {
        if index > 0 {
            w.code(".");
        }
        w.ident(segment);
    }
    w.code("\n\n");
}

fn emit_view(
    handoff: &SemanticHandoffV2,
    plans: &[Plan<'_>],
    package: &str,
    spec: &str,
    tv: bool,
) -> (KotlinFile, Vec<SourceMapEntry>) {
    let scene = &handoff.parsed().scene;
    let has_actions = !scene.interactions.is_empty();
    let mut w = KotlinWriter::new("GoyaSceneView.kt");
    write_package(&mut w, package);
    w.code(VIEW_IMPORTS)
        .code("/** Generated by Aragon from a GSSH/3 scene; do not edit. One point is one dp. */\n")
        .code("class GoyaSceneView(context: Context, private val report: (String) -> Unit) : ViewGroup(context) {\n")
        .code("    private val density: Double = resources.displayMetrics.density.toDouble()\n")
        .code("    private val cells = arrayOfNulls<GoyaCell>(")
        .ident(spec)
        .code(".CELL_COUNT)\n    private val fields = HashMap<String, EditText>()\n    private val edges = IntArray(4 * ")
        .ident(spec)
        .code(".CELL_COUNT)\n    private val limits = IntArray(2 * ")
        .ident(spec)
        .code(".CELL_COUNT)\n");
    if tv {
        w.code(
            "    private val remote = NevadaRemote(context, report) { cells[it]?.focusTarget() }\n",
        );
    }
    w.code("\n    init {\n");
    for index in 0..plans.len() {
        w.code("        cell").int(index).code("()\n");
    }
    if tv {
        w.code("        remote.attach()\n");
    }
    w.code("    }\n");

    for (index, plan) in plans.iter().enumerate() {
        let cell = plan.cell;
        let id = plan.id.to_owned();
        w.code("\n    private fun cell")
            .int(index)
            .code("() {\n        val cell = ");
        w.mapped(Origin::Element(id.clone()), |w| {
            w.code("GoyaCell(context, this, ").int(index).code(")");
        });
        w.code("\n");
        if cell.scroll {
            w.code("        cell.enableScroll()\n");
        }
        let text = plan.text.as_deref().unwrap_or("");
        let text_origin = Origin::Property {
            element: id.clone(),
            property: "text".to_owned(),
        };
        let write_text = |w: &mut KotlinWriter| {
            if cell.text.is_some() {
                w.mapped(text_origin.clone(), |w| {
                    w.string(text);
                });
            } else {
                w.string(text);
            }
        };
        match cell.kind {
            CellKind::Cell => {
                if cell.text.is_some() {
                    w.code("        cell.setContent(label(");
                    write_text(&mut w);
                    w.code("))\n");
                }
            }
            CellKind::Button => {
                w.code("        cell.setContent(button(");
                write_text(&mut w);
                let action = scene
                    .interactions
                    .iter()
                    .find(|interaction| interaction.trigger_button == cell.name)
                    .and_then(|interaction| interaction.authored_id.clone());
                match action {
                    Some(action) => {
                        w.code(") { activate(");
                        w.mapped(Origin::Interaction(action.clone()), |w| {
                            w.string(&action);
                        });
                        w.code(") })\n");
                    }
                    None => {
                        w.code(", null))\n");
                    }
                }
            }
            CellKind::Input => {
                w.code("        cell.setContent(field(")
                    .string(plan.id)
                    .code(", ")
                    .int(index + 1)
                    .code("))\n");
            }
        }
        if let Some(stroke) = cell.stroke {
            w.code("        cell.stroke(");
            w.mapped(
                Origin::Property {
                    element: id.clone(),
                    property: "stroke".to_owned(),
                },
                |w| {
                    w.color(stroke);
                },
            );
            w.code(")\n");
        }
        if let Some(fill) = cell.fill {
            w.code("        cell.fill(");
            w.mapped(
                Origin::Property {
                    element: id.clone(),
                    property: "fill".to_owned(),
                },
                |w| {
                    w.color(fill);
                },
            );
            w.code(")\n");
        }
        w.code("        attach(cell)\n    }\n");
    }

    w.code("\n    private fun attach(cell: GoyaCell) {\n        cells[cell.index] = cell\n        val parent = ")
        .ident(spec)
        .code(".PARENTS[cell.index]\n        if (parent < 0) addView(cell) else cells[parent]!!.addCell(cell)\n    }\n");
    w.code("\n    override fun onMeasure(widthSpec: Int, heightSpec: Int) {\n")
        .code("        setMeasuredDimension(MeasureSpec.getSize(widthSpec), MeasureSpec.getSize(heightSpec))\n")
        .code("        // The GSSH viewport is the area inside the system bars and cutouts.\n")
        .code("        val width = maxOf(measuredWidth - paddingLeft - paddingRight, 0)\n")
        .code("        val height = maxOf(measuredHeight - paddingTop - paddingBottom, 0)\n");
    if tv {
        w.code("        remote.resize(width, height)\n");
    }
    w
        .code("        val rects = ")
        .ident(spec)
        .code(if tv {
            ".resolve(NevadaRemote.WIDTH_DP, NevadaRemote.HEIGHT_DP)"
        } else {
            ".resolve(width / density, height / density)"
        })
        .code("\n        for (i in 0 until ")
        .ident(spec)
        .code(".CELL_COUNT) {\n")
        .code("            val right = ")
        .ident(spec)
        .code(".satAdd(rects[4 * i], rects[4 * i + 2])\n            val bottom = ")
        .ident(spec)
        .code(".satAdd(rects[4 * i + 1], rects[4 * i + 3])\n")
        .code("            edges[4 * i] = px(rects[4 * i])\n")
        .code("            edges[4 * i + 1] = px(rects[4 * i + 1])\n")
        .code("            edges[4 * i + 2] = px(right)\n")
        .code("            edges[4 * i + 3] = px(bottom)\n")
        .code("            val limit = ")
        .ident(spec)
        .code(".scrollLimits(rects, i)\n            limits[2 * i] = px(")
        .ident(spec)
        .code(".satAdd(right, limit[0])) - edges[4 * i + 2]\n            limits[2 * i + 1] = px(")
        .ident(spec)
        .code(".satAdd(bottom, limit[1])) - edges[4 * i + 3]\n            val textMp = ")
        .ident(spec)
        .code(".textSizeMp(rects, i)\n            val strokeMp = ")
        .ident(spec)
        .code(".strokeWidthMp(rects, i)\n")
        .code(if tv {
            "            cells[i]!!.applyLengths(if (textMp < 0) -1f else remote.scale(textMp).toFloat(), if (strokeMp < 0) -1 else px(strokeMp))\n"
        } else {
            "            cells[i]!!.applyLengths(if (textMp < 0) -1f else (textMp.toDouble() * density / 1000.0).toFloat(), if (strokeMp < 0) -1 else px(strokeMp))\n"
        })
        .code("        }\n        measureCell(cells[0]!!)\n    }\n");
    w.code("\n    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {\n");
    if tv {
        w.code("        layoutCell(cells[0]!!, -paddingLeft - remote.offsetX, -paddingTop - remote.offsetY)\n")
            .code("        remote.restoreFocus(findFocus())\n");
    } else {
        w.code("        layoutCell(cells[0]!!, -paddingLeft, -paddingTop)\n");
    }
    w.code("    }\n");
    if tv {
        w.code(
            "\n    override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean =\n        remote.dispatch(event, findFocus()) || super.dispatchKeyEvent(event)\n",
        );
    }
    w.code(VIEW_LAYOUT);
    w.code(if tv {
        "        Math.round(remote.scale(mp)).coerceIn(Int.MIN_VALUE.toLong(), Int.MAX_VALUE.toLong()).toInt()\n"
    } else {
        "        Math.round(mp.toDouble() * density / 1000.0).coerceIn(Int.MIN_VALUE.toLong(), Int.MAX_VALUE.toLong()).toInt()\n"
    });
    w.code(VIEW_CONTENT);

    if has_actions {
        w.code("\n    private fun activate(actionId: String) {\n")
            .code("        val inputs = HashMap<String, GoyaD1Runtime.Value>()\n")
            .code("        val bound = ArrayList<EditText>()\n")
            .code("        when (actionId) {\n");
        for interaction in &scene.interactions {
            let action = interaction.authored_id.clone().unwrap_or_default();
            w.code("            ").string(&action).code(" -> {\n");
            let bindings: Vec<_> = handoff.bindings_for(&interaction.name).collect();
            // One snapshot of every bound field before any conversion.
            for (k, binding) in bindings.iter().enumerate() {
                w.code("                val f")
                    .int(k)
                    .code(" = fields.getValue(")
                    .string(&binding.field_id)
                    .code(")\n                bound.add(f")
                    .int(k)
                    .code(")\n                val t")
                    .int(k)
                    .code(" = f")
                    .int(k)
                    .code(".text.toString()\n");
            }
            for (k, binding) in bindings.iter().enumerate() {
                w.code("                ");
                w.mapped(
                    Origin::Binding {
                        interaction: action.clone(),
                        input: binding.input.clone(),
                    },
                    |w| match binding.value_type {
                        InteractionType::Int => {
                            w.code("inputs[")
                                .string(&binding.input)
                                .code("] = GoyaD1Runtime.Value.IntValue(")
                                .ident(spec)
                                .code(".convertInt(t")
                                .int(k)
                                .code(") ?: return rejectField(f")
                                .int(k)
                                .code(", \"Enter a whole number: digits with an optional leading minus, no spaces or leading zeros.\"))");
                        }
                        InteractionType::Text => {
                            w.code("if (!")
                                .ident(spec)
                                .code(".textWithinLimit(t")
                                .int(k)
                                .code(")) return rejectField(f")
                                .int(k)
                                .code(", \"Text is longer than 64 KiB.\")\n                inputs[")
                                .string(&binding.input)
                                .code("] = GoyaD1Runtime.Value.TextValue(t")
                                .int(k)
                                .code(")");
                        }
                        // Admission never binds bool inputs.
                        InteractionType::Bool => unreachable!("bool binding admitted"),
                    },
                );
                w.code("\n");
            }
            w.code("            }\n");
        }
        w.code("        }\n")
            .code("        when (val result = GoyaSession.runtime.dispatch(actionId, inputs)) {\n")
            .code("            is GoyaD1Runtime.DispatchResult.Success -> for (field in bound) field.error = null\n")
            .code("            is GoyaD1Runtime.DispatchResult.Failure -> report(\"Action failed: \" + result.code.name)\n")
            .code("        }\n    }\n\n")
            .code("    private fun rejectField(field: EditText, message: String) {\n        field.error = message\n    }\n");
    }
    w.code("}\n");
    w.code(VIEW_CELL);
    if tv {
        w.code("\n    internal fun focusTarget(): View? = content\n");
    }
    w.code(VIEW_CLASSES);

    if has_actions {
        w.code("\n/** Session state: lives as long as the process (GSSH `session`). */\nobject GoyaSession {\n")
            .code("    val runtime: GoyaD1Runtime by lazy {\n        GoyaD1Runtime(mapOf<String, GoyaD1Runtime.Value>(");
        for (index, decl) in handoff.state().iter().enumerate() {
            if index > 0 {
                w.code(",");
            }
            w.code("\n            ");
            w.mapped(Origin::State(decl.key.clone()), |w| {
                w.string(&decl.key).code(" to ");
                match &decl.initial {
                    InteractionValue::Int(value) => {
                        w.code("GoyaD1Runtime.Value.IntValue(")
                            .long(*value)
                            .code(")");
                    }
                    InteractionValue::Text(value) => {
                        w.code("GoyaD1Runtime.Value.TextValue(")
                            .string(value)
                            .code(")");
                    }
                    InteractionValue::Bool(value) => {
                        w.code("GoyaD1Runtime.Value.BoolValue(")
                            .boolean(*value)
                            .code(")");
                    }
                }
            });
        }
        w.code("\n        ))\n    }\n}\n");
    }
    w.finish(handoff)
}

const VIEW_IMPORTS: &str = "import android.content.Context
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Button
import android.widget.EditText
import android.widget.TextView

";

const VIEW_LAYOUT: &str = r#"
    override fun onApplyWindowInsets(insets: WindowInsets): WindowInsets {
        if (Build.VERSION.SDK_INT >= 30) {
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            setPadding(bars.left, bars.top, bars.right, bars.bottom)
        } else {
            @Suppress("DEPRECATION")
            setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
        }
        return insets
    }

    internal fun measureCell(cell: GoyaCell) {
        val i = cell.index
        cell.measure(exactly(edges[4 * i + 2] - edges[4 * i]), exactly(edges[4 * i + 3] - edges[4 * i + 1]))
    }

    internal fun layoutCell(cell: GoyaCell, originX: Int, originY: Int) {
        val i = cell.index
        cell.layout(edges[4 * i] - originX, edges[4 * i + 1] - originY, edges[4 * i + 2] - originX, edges[4 * i + 3] - originY)
    }

    internal fun left(i: Int): Int = edges[4 * i]
    internal fun top(i: Int): Int = edges[4 * i + 1]
    internal fun maxScrollX(i: Int): Int = maxOf(limits[2 * i], 0)
    internal fun maxScrollY(i: Int): Int = maxOf(limits[2 * i + 1], 0)

    private fun exactly(size: Int): Int = MeasureSpec.makeMeasureSpec(maxOf(size, 0), MeasureSpec.EXACTLY)

    /** Millipoints to pixels: each edge rounds half up on its own. */
    private fun px(mp: Long): Int =
"#;

const VIEW_CONTENT: &str = r#"
    private fun label(text: String): TextView = TextView(context).apply {
        this.text = text
        setHorizontallyScrolling(true)
        includeFontPadding = false
        setPadding(0, 0, 0, 0)
        gravity = Gravity.TOP or Gravity.START
    }

    private fun button(text: String, onClick: (() -> Unit)?): Button = Button(context).apply {
        this.text = text
        isAllCaps = false
        minWidth = 0
        minHeight = 0
        minimumWidth = 0
        minimumHeight = 0
        setPadding(0, 0, 0, 0)
        if (onClick != null) setOnClickListener { onClick() }
    }

    private fun field(fieldId: String, viewId: Int): EditText = EditText(context).apply {
        id = viewId
        minWidth = 0
        minHeight = 0
        minimumWidth = 0
        minimumHeight = 0
        setPadding(0, 0, 0, 0)
        inputType = InputType.TYPE_CLASS_TEXT
        fields[fieldId] = this
    }
"#;

const VIEW_CELL: &str = r#"
/** One scene element. Its own content fills it; child cells use scene edges. */
class GoyaCell(context: Context, private val scene: GoyaSceneView, val index: Int) : ViewGroup(context) {
    private var content: View? = null
    private var layer: GoyaScrollLayer? = null
"#;

const VIEW_CLASSES: &str = r#"
    fun setContent(view: View) {
        content = view
        addView(view, 0)
    }

    fun enableScroll() {
        val scrollLayer = GoyaScrollLayer(context, scene, index)
        layer = scrollLayer
        addView(scrollLayer)
    }

    fun addCell(cell: GoyaCell) {
        val scrollLayer = layer
        if (scrollLayer != null) scrollLayer.addView(cell) else addView(cell)
    }

    fun fill(color: Int) {
        setBackgroundColor(color)
        val view = content
        if (view is Button || view is EditText) view.setBackgroundColor(color)
    }

    private var strokeColor = 0
    private var strokeWidth = -1
    private var border: GradientDrawable? = null

    /** A border drawn inside the cell, above its content. */
    fun stroke(color: Int) {
        strokeColor = color
        val drawable = GradientDrawable()
        drawable.setColor(0)
        border = drawable
        foreground = drawable
    }

    /** Text size in pixels (em size) and stroke width in pixels; -1 means none. */
    fun applyLengths(textPx: Float, strokePx: Int) {
        val view = content
        if (textPx >= 0f && view is TextView && view.textSize != textPx) {
            view.setTextSize(TypedValue.COMPLEX_UNIT_PX, textPx)
        }
        val drawable = border
        if (drawable != null && strokePx >= 0 && strokePx != strokeWidth) {
            strokeWidth = strokePx
            drawable.setStroke(strokePx, strokeColor)
        }
    }

    override fun onMeasure(widthSpec: Int, heightSpec: Int) {
        val width = MeasureSpec.getSize(widthSpec)
        val height = MeasureSpec.getSize(heightSpec)
        setMeasuredDimension(width, height)
        val exactWidth = MeasureSpec.makeMeasureSpec(width, MeasureSpec.EXACTLY)
        val exactHeight = MeasureSpec.makeMeasureSpec(height, MeasureSpec.EXACTLY)
        content?.measure(exactWidth, exactHeight)
        layer?.measure(exactWidth, exactHeight)
        for (k in 0 until childCount) {
            val child = getChildAt(k)
            if (child is GoyaCell) scene.measureCell(child)
        }
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        content?.layout(0, 0, r - l, b - t)
        layer?.layout(0, 0, r - l, b - t)
        for (k in 0 until childCount) {
            val child = getChildAt(k)
            if (child is GoyaCell) scene.layoutCell(child, scene.left(index), scene.top(index))
        }
    }
}

/** Children of a scroll cell. Offsets are clamped to Frame::scroll_limits. */
class GoyaScrollLayer(context: Context, private val scene: GoyaSceneView, private val index: Int) : ViewGroup(context) {
    private val slop = ViewConfiguration.get(context).scaledTouchSlop
    private var lastX = 0f
    private var lastY = 0f
    private var dragging = false

    override fun onMeasure(widthSpec: Int, heightSpec: Int) {
        setMeasuredDimension(MeasureSpec.getSize(widthSpec), MeasureSpec.getSize(heightSpec))
        for (k in 0 until childCount) {
            val child = getChildAt(k)
            if (child is GoyaCell) scene.measureCell(child)
        }
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        for (k in 0 until childCount) {
            val child = getChildAt(k)
            if (child is GoyaCell) scene.layoutCell(child, scene.left(index), scene.top(index))
        }
        scrollClamped(scrollX, scrollY)
    }

    fun scrollClamped(x: Int, y: Int) {
        scrollTo(x.coerceIn(0, scene.maxScrollX(index)), y.coerceIn(0, scene.maxScrollY(index)))
    }

    override fun canScrollVertically(direction: Int): Boolean =
        if (direction < 0) scrollY > 0 else scrollY < scene.maxScrollY(index)

    override fun canScrollHorizontally(direction: Int): Boolean =
        if (direction < 0) scrollX > 0 else scrollX < scene.maxScrollX(index)

    override fun getAccessibilityClassName(): CharSequence = "android.widget.ScrollView"

    override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        val forward = scrollY < scene.maxScrollY(index) || scrollX < scene.maxScrollX(index)
        val backward = scrollY > 0 || scrollX > 0
        info.isScrollable = forward || backward
        if (forward) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD)
        if (backward) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD)
    }

    /** One page per action: vertical first, then horizontal. */
    override fun performAccessibilityAction(action: Int, arguments: Bundle?): Boolean {
        val pageX = maxOf(width, 1)
        val pageY = maxOf(height, 1)
        when (action) {
            AccessibilityNodeInfo.ACTION_SCROLL_FORWARD -> {
                if (scrollY < scene.maxScrollY(index)) scrollClamped(scrollX, scrollY + pageY)
                else if (scrollX < scene.maxScrollX(index)) scrollClamped(scrollX + pageX, scrollY)
                else return false
                return true
            }
            AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD -> {
                if (scrollY > 0) scrollClamped(scrollX, scrollY - pageY)
                else if (scrollX > 0) scrollClamped(scrollX - pageX, scrollY)
                else return false
                return true
            }
        }
        return super.performAccessibilityAction(action, arguments)
    }

    override fun onInterceptTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                lastX = event.x
                lastY = event.y
                dragging = false
            }
            MotionEvent.ACTION_MOVE -> {
                if (!dragging && (Math.abs(event.x - lastX) > slop || Math.abs(event.y - lastY) > slop)) {
                    dragging = true
                    lastX = event.x
                    lastY = event.y
                }
            }
        }
        return dragging
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                lastX = event.x
                lastY = event.y
            }
            MotionEvent.ACTION_MOVE -> {
                scrollClamped(scrollX + (lastX - event.x).toInt(), scrollY + (lastY - event.y).toInt())
                lastX = event.x
                lastY = event.y
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> dragging = false
        }
        return true
    }
}
"#;
