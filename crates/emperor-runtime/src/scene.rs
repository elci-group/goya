//! General GSSH/4-live scene runtime (`emperor.linux.scene/0.0.1`), used by
//! Emperor scene apps and the Nebula COSMIC profile.
//!
//! The scene is admitted again at start from its embedded source (Emperor
//! admitted it at build time), laid out with `goya_lang::layout` at the
//! window size (one point is one surface pixel), and rendered on the CPU with
//! tiny-skia into one RGBA frame for `goya-wayland`.
//!
//! * **Interactions** run on the canonical D1 executor
//!   (`goya_lang::CompiledInteraction::execute`): reads come from the
//!   pre-transition snapshot, addition is checked, writes commit together.
//! * **Live constructs**: `text-from` (authored text then value, wrapped at
//!   the element's edge), `present-when` (`is`, `at-least`, conjoined with
//!   ancestors), `remote-digit` (digit keys), and the bounded motion of
//!   `fill-to`, `drift-p` and `appear-s`/`appear-from-p`.
//! * **Graphics**: each `@graphic` was compiled to GY-GVI/0 and verified at
//!   build time; the runtime opens it with the reference decoder
//!   (`goya_gvi::Inventory::open`, binding checked) and paints the chosen
//!   projection's items. A dimension with `step-s` steps through its
//!   intermediate values.
//! * **Durable state** (D8, D11, D12; Nebula decision 6 as amended on
//!   25 November 2026): the app's own record `goya.state.v1/<ontology>` at
//!   `$XDG_STATE_HOME/goya/<ontology>.json` (`linux.xdg-state.v1`), and
//!   nothing else on the file system. Restore runs before the first frame,
//!   all-or-nothing; a record that fails to restore is reported and never
//!   overwritten (D4). A transition writing durable state is saved (atomic
//!   rename) before it is shown; another writer's change fails it (D8).
//! * **Accessibility**: an AccessKit tree published over AT-SPI. Buttons
//!   carry their `@iden` label as name and hint as description, live text is
//!   exposed as label values, and each committed transition's D9
//!   announcement updates a polite live region.
//!
//! Host state, `@link`, `image-from` and D2 interactions need platform
//! services the Linux runtime does not have yet; Emperor refuses them at
//! build time.

// Nodes are held as parallel arrays (cells, frame nodes, presence, appear
// times) indexed by preorder position, so index loops are the clear form.
#![allow(clippy::needless_range_loop)]

use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use goya_gvi::{GviLimits, Inventory, ParentBinding};
use goya_lang::{
    CompiledInteraction, Condition, ConditionOp, InteractionType, InteractionValue, ParsedScene,
    SceneCell, StateLifetime,
};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const CONTRACT: &str = "emperor.linux.scene/0.0.1";

/// One compiled `@graphic`, embedded by Emperor.
pub struct GraphicSpec {
    pub name: &'static str,
    pub bytes: &'static [u8],
    pub binding: [u8; 32],
    pub radices: &'static [usize],
    pub steps_ms: &'static [u32],
}

/// What Emperor embeds in a scene app.
pub struct SceneSpec {
    pub source: &'static str,
    pub title: &'static str,
    pub graphics: &'static [GraphicSpec],
    /// Whether the scene has durable state (stored as `linux.xdg-state.v1`).
    pub durable: bool,
    /// The UI font (embedded at build time; the app reads no font files).
    pub font: &'static [u8],
}

struct Gfx {
    inv: Inventory<'static>,
    paths: Vec<tiny_skia::Path>,
    steps: Vec<u32>,
}

struct DrawState {
    graphic: usize,
    shown: Vec<usize>,
    target: Vec<usize>,
    next_step: Option<Instant>,
}

/// A running scene: state, layout and presentation.
pub struct Scene {
    parsed: ParsedScene,
    cells: Vec<SceneCell>,
    parents: Vec<Option<usize>>,
    state: BTreeMap<String, D1Value>,
    types: BTreeMap<String, InteractionType>,
    durable: Vec<String>,
    store: Option<Store>,
    actions: HashMap<String, CompiledInteraction>,
    displays: Vec<(String, String, InteractionType)>,
    graphics: Vec<Gfx>,
    draws: Vec<Option<DrawState>>,
    was_present: Vec<bool>,
    appear_from: Vec<Option<Instant>>,
    focus: Option<usize>,
    pub last_announcement: String,
    pub errors: Vec<String>,
    frame: goya_lang::Frame,
    width: u32,
    height: u32,
    start: Instant,
    font: ab_glyph::FontRef<'static>,
    reduce_motion: bool,
}

fn value_of(v: &InteractionValue) -> D1Value {
    match v {
        InteractionValue::Int(i) => D1Value::Int(*i),
        InteractionValue::Text(t) => D1Value::Text(t.clone()),
        InteractionValue::Bool(b) => D1Value::Bool(*b),
    }
}

fn shown(v: Option<&D1Value>) -> String {
    match v {
        Some(D1Value::Int(i)) => i.to_string(),
        Some(D1Value::Text(t)) => t.clone(),
        Some(D1Value::Bool(b)) => b.to_string(),
        None => String::new(),
    }
}

fn holds(c: &Condition, v: Option<&D1Value>) -> bool {
    match (c.op, v, &c.value) {
        (ConditionOp::Is, Some(D1Value::Int(a)), InteractionValue::Int(b)) => a == b,
        (ConditionOp::Is, Some(D1Value::Text(a)), InteractionValue::Text(b)) => a == b,
        (ConditionOp::Is, Some(D1Value::Bool(a)), InteractionValue::Bool(b)) => a == b,
        (ConditionOp::AtLeast, Some(D1Value::Int(a)), InteractionValue::Int(b)) => a >= b,
        _ => false,
    }
}

/// D9 wording, exactly as `goya_lang::display_binding` defines it.
fn change(ctx: &str, a: Option<&D1Value>, b: Option<&D1Value>) -> Option<String> {
    match (a, b) {
        (Some(D1Value::Int(x)), Some(D1Value::Int(y))) if x != y => Some(if y > x {
            format!("{ctx} increased by {}, from {x} to {y}", i128::from(*y) - i128::from(*x))
        } else {
            format!("{ctx} decreased by {}, from {x} to {y}", i128::from(*x) - i128::from(*y))
        }),
        (Some(D1Value::Text(x)), Some(D1Value::Text(y))) if x != y => Some(format!("{ctx} changed from \"{x}\" to \"{y}\"")),
        (Some(D1Value::Bool(x)), Some(D1Value::Bool(y))) if x != y => Some(format!("{ctx} changed from {x} to {y}")),
        _ => None,
    }
}

fn preorder(cell: &SceneCell, parent: Option<usize>, cells: &mut Vec<SceneCell>, parents: &mut Vec<Option<usize>>) {
    let me = cells.len();
    let mut flat = cell.clone();
    flat.children = Vec::new();
    cells.push(flat);
    parents.push(parent);
    for child in &cell.children {
        preorder(child, Some(me), cells, parents);
    }
}

fn path_of(p: &goya_graphics_ir::Path) -> Option<tiny_skia::Path> {
    use goya_graphics_ir::Segment;
    let mut b = tiny_skia::PathBuilder::new();
    for s in p.segments() {
        match *s {
            Segment::MoveTo(q) => b.move_to(q.x as f32, q.y as f32),
            Segment::LineTo(q) => b.line_to(q.x as f32, q.y as f32),
            Segment::QuadTo(c, q) => b.quad_to(c.x as f32, c.y as f32, q.x as f32, q.y as f32),
            Segment::CubicTo(c1, c2, q) => b.cubic_to(c1.x as f32, c1.y as f32, c2.x as f32, c2.y as f32, q.x as f32, q.y as f32),
            Segment::Close => b.close(),
        }
    }
    b.finish()
}

impl Scene {
    pub fn new(spec: &SceneSpec, width: u32, height: u32) -> Result<Scene, String> {
        let handoff = match goya_lang::handoff::admit_v4(spec.source) {
            Err(e) if e.code == "GY-E1012" => goya_lang::handoff::admit_v4_live(spec.source),
            other => other,
        }
        .map_err(|e| format!("{}: {}", e.code, e))?;
        let parsed = handoff.parsed().clone();
        let mut cells = Vec::new();
        let mut parents = Vec::new();
        preorder(&parsed.scene.root, None, &mut cells, &mut parents);
        let mut state = BTreeMap::new();
        let mut types = BTreeMap::new();
        let mut durable = Vec::new();
        for d in parsed.declarations.state.iter().flat_map(|(d, _)| d) {
            state.insert(d.key.clone(), value_of(&d.initial));
            types.insert(d.key.clone(), d.value_type);
            if d.lifetime == StateLifetime::Durable {
                durable.push(d.key.clone());
            }
        }
        let compiled = goya_lang::compile_d1_interactions(&parsed.scene).map_err(|e| e.to_string())?;
        let mut actions = HashMap::new();
        for c in compiled {
            if let Some(decl) = parsed.scene.interactions.iter().find(|i| i.name == c.name()) {
                actions.insert(decl.trigger_button.clone(), c);
            }
        }
        let displays = cells
            .iter()
            .filter_map(|c| {
                let key = c.text_from.as_ref()?;
                let ty = *types.get(key)?;
                Some((key.clone(), goya_lang::display_binding::context(c.text.as_deref(), key), ty))
            })
            .collect();
        let mut graphics = Vec::new();
        for g in spec.graphics {
            let inv = Inventory::open(g.bytes, &ParentBinding(g.binding), &GviLimits::default()).map_err(|e| format!("graphic {}: {e}", g.name))?;
            let paths = inv
                .decode_geometry()
                .map_err(|e| format!("graphic {}: {e}", g.name))?
                .iter()
                .map(|p| path_of(p).unwrap_or_else(|| tiny_skia::PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).expect("unit rect"))))
                .collect();
            graphics.push(Gfx { inv, paths, steps: g.steps_ms.to_vec() });
        }
        let draws = cells
            .iter()
            .map(|c| {
                let d = c.draw.as_ref()?;
                let gi = spec.graphics.iter().position(|g| g.name == d.graphic)?;
                let n = spec.graphics[gi].radices.len();
                Some(DrawState { graphic: gi, shown: vec![0; n], target: vec![0; n], next_step: None })
            })
            .collect();
        let font = ab_glyph::FontRef::try_from_slice(spec.font).map_err(|e| format!("font: {e}"))?;
        let mut scene = Scene {
            frame: goya_lang::layout(&parsed.scene, f64::from(width), f64::from(height)),
            was_present: vec![false; cells.len()],
            appear_from: vec![None; cells.len()],
            parsed,
            cells,
            parents,
            state,
            types,
            durable,
            store: None,
            actions,
            displays,
            graphics,
            draws,
            focus: None,
            last_announcement: String::new(),
            errors: Vec::new(),
            width,
            height,
            start: Instant::now(),
            font,
            reduce_motion: std::env::var_os("GOYA_REDUCE_MOTION").is_some(),
        };
        if spec.durable {
            let mut store = Store::open(&scene.parsed.scene.name);
            match store.restore(&scene.types, &scene.durable) {
                Ok(values) => scene.state.extend(values),
                Err(e) => scene.errors.push(format!("saved state was not restored: {e}")),
            }
            scene.store = Some(store);
        }
        let present = scene.presence();
        let now = Instant::now();
        for i in 0..scene.cells.len() {
            if present[i] && scene.cells[i].appear_s.is_some() {
                scene.appear_from[i] = Some(now);
            }
        }
        scene.was_present = present;
        scene.retarget(true);
        scene.focus = scene.buttons().first().copied();
        Ok(scene)
    }

    pub fn state(&self) -> &BTreeMap<String, D1Value> {
        &self.state
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if (width, height) != (self.width, self.height) && width > 0 && height > 0 {
            self.width = width;
            self.height = height;
            self.frame = goya_lang::layout(&self.parsed.scene, f64::from(width), f64::from(height));
        }
    }

    /// Presence of every node: its own condition and its ancestors'.
    fn presence(&self) -> Vec<bool> {
        let mut out = vec![true; self.cells.len()];
        for i in 0..self.cells.len() {
            let own = self.cells[i].present_when.as_ref().is_none_or(|c| holds(c, self.state.get(&c.key)));
            out[i] = own && self.parents[i].is_none_or(|p| out[p]);
        }
        out
    }

    /// Present buttons in source order.
    fn buttons(&self) -> Vec<usize> {
        let present = self.presence();
        (0..self.cells.len()).filter(|&i| present[i] && self.frame.nodes[i].is_button).collect()
    }

    fn rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let r = self.frame.nodes[i].rect;
        (r.x as f32 / 1000.0, r.y as f32 / 1000.0, r.width as f32 / 1000.0, r.height as f32 / 1000.0)
    }

    /// Choose each @draw's target state from its rules.
    fn retarget(&mut self, immediately: bool) {
        for i in 0..self.cells.len() {
            let Some(draw) = self.cells[i].draw.clone() else { continue };
            let Some(ds) = self.draws[i].as_mut() else { continue };
            let g = &self.graphics[ds.graphic];
            let names: Vec<(String, Vec<String>)> = g
                .inv
                .dimensions()
                .iter()
                .map(|d| {
                    let labels = match &d.domain {
                        goya_gvi::Domain::Discrete(v) => v.iter().map(|v| v.label.clone()).collect(),
                        _ => Vec::new(),
                    };
                    (d.name.clone(), labels)
                })
                .collect();
            for (k, (dim, labels)) in names.iter().enumerate() {
                let chosen = draw
                    .rules
                    .iter()
                    .filter(|r| &r.dimension == dim)
                    .find(|r| r.when.as_ref().is_none_or(|c| holds(c, self.state.get(&c.key))))
                    .and_then(|r| labels.iter().position(|l| *l == r.value))
                    .unwrap_or(0);
                ds.target[k] = chosen;
            }
            if immediately || self.reduce_motion {
                ds.shown = ds.target.clone();
                ds.next_step = None;
            } else if ds.shown != ds.target && ds.next_step.is_none() {
                ds.next_step = Some(Instant::now());
            }
        }
    }

    /// Advance stepping graphics; true while any animation is in progress.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let mut busy = false;
        for ds in self.draws.iter_mut().flatten() {
            let Some(due) = ds.next_step else { continue };
            busy = true;
            if now < due {
                continue;
            }
            let steps = &self.graphics[ds.graphic].steps;
            let mut wait = 0u32;
            for k in 0..ds.shown.len() {
                if ds.shown[k] == ds.target[k] {
                    continue;
                }
                if steps[k] > 0 {
                    if ds.target[k] > ds.shown[k] { ds.shown[k] += 1 } else { ds.shown[k] -= 1 }
                    wait = if wait == 0 { steps[k] } else { wait.min(steps[k]) };
                } else {
                    ds.shown[k] = ds.target[k];
                }
            }
            ds.next_step = (ds.shown != ds.target).then(|| now + std::time::Duration::from_millis(u64::from(wait.max(1))));
        }
        for (i, a) in self.appear_from.iter().enumerate() {
            if let (Some(t), Some(s)) = (a, self.cells[i].appear_s) {
                if now.duration_since(*t).as_secs_f64() < s {
                    busy = true;
                }
            }
        }
        busy
    }

    /// Whether the scene has ambient motion (breathing or drift).
    pub fn ambient(&self) -> bool {
        !self.reduce_motion && self.cells.iter().any(|c| c.fill_to.is_some() || c.drift_p.is_some())
    }

    /// Activate the present button `i`: stage, save durable state, commit,
    /// announce.
    pub fn activate(&mut self, i: usize) {
        let name = self.cells[i].name.clone();
        let Some(action) = self.actions.get(&name) else { return };
        let touched: Vec<String> = action
            .state_reads()
            .iter()
            .chain(action.state_writes())
            .map(|k| k.name.clone())
            .collect();
        let mut snapshot = BTreeMap::new();
        for k in &touched {
            if let Some(v) = self.state.get(k) {
                snapshot.insert(k.clone(), v.clone());
            }
        }
        let result = D1Snapshot::new(0, snapshot)
            .map_err(|e| format!("{e:?}"))
            .and_then(|s| {
                let inputs = InputSnapshot::new(BTreeMap::new()).map_err(|e| format!("{e:?}"))?;
                action.execute(&s, &inputs).map_err(|e| e.to_string())
            });
        let outcome = match result {
            Ok(o) => o,
            Err(e) => {
                self.errors.push(format!("{name}: {e}"));
                return;
            }
        };
        let mut staged = self.state.clone();
        staged.extend(outcome.next_state);
        // D12: save, then show.
        if action.state_writes().iter().any(|k| self.durable.contains(&k.name)) {
            if let Some(store) = self.store.as_mut() {
                if let Err(e) = store.save(&staged, &self.types, &self.durable) {
                    self.errors.push(format!("{name}: {e}"));
                    return;
                }
            }
        }
        let before = std::mem::replace(&mut self.state, staged);
        let present = self.presence();
        let now = Instant::now();
        for j in 0..self.cells.len() {
            if present[j] && !self.was_present[j] && self.cells[j].appear_s.is_some() && !self.reduce_motion {
                self.appear_from[j] = Some(now);
            }
        }
        self.was_present = present;
        self.retarget(false);
        self.keep_focus();
        let parts: Vec<String> = self
            .displays
            .iter()
            .filter_map(|(k, ctx, _)| change(ctx, before.get(k), self.state.get(k)))
            .collect();
        if !parts.is_empty() {
            self.last_announcement = parts.join("; ");
        }
    }

    /// Keep focus on the same place when its button is swapped for another
    /// variant; otherwise move to the nearest present button.
    fn keep_focus(&mut self) {
        let buttons = self.buttons();
        let Some(f) = self.focus else {
            self.focus = buttons.first().copied();
            return;
        };
        if buttons.contains(&f) {
            return;
        }
        let r = self.frame.nodes[f].rect;
        let (cx, cy) = (r.x + r.width / 2, r.y + r.height / 2);
        self.focus = buttons
            .iter()
            .copied()
            .min_by_key(|&b| {
                let s = self.frame.nodes[b].rect;
                if s == r { return 0 }
                (s.x + s.width / 2 - cx).abs() + (s.y + s.height / 2 - cy).abs() + 1
            });
    }

    /// Arrow-key navigation between present buttons.
    pub fn move_focus(&mut self, dx: i32, dy: i32) {
        let buttons = self.buttons();
        let Some(f) = self.focus.filter(|f| buttons.contains(f)) else {
            self.focus = buttons.first().copied();
            return;
        };
        let r = self.frame.nodes[f].rect;
        let (cx, cy) = (r.x + r.width / 2, r.y + r.height / 2);
        let best = buttons
            .iter()
            .copied()
            .filter(|&b| self.frame.nodes[b].rect != r)
            .filter_map(|b| {
                let s = self.frame.nodes[b].rect;
                let (ox, oy) = (s.x + s.width / 2 - cx, s.y + s.height / 2 - cy);
                let (along, across) = if dx != 0 { (ox * i64::from(dx), oy.abs()) } else { (oy * i64::from(dy), ox.abs()) };
                (along > 0).then_some((along + 2 * across, b))
            })
            .min();
        if let Some((_, b)) = best {
            self.focus = Some(b);
        }
    }

    pub fn activate_focused(&mut self) {
        if let Some(f) = self.focus {
            self.activate(f);
        }
    }

    pub fn press_digit(&mut self, d: u8) {
        if let Some(b) = self.buttons().into_iter().find(|&b| self.cells[b].remote_digit == Some(d)) {
            self.focus = Some(b);
            self.activate(b);
        }
    }

    /// Topmost present button under a point.
    pub fn hit(&self, x: f64, y: f64) -> Option<usize> {
        self.buttons().into_iter().rev().find(|&b| {
            let r = self.frame.nodes[b].rect;
            let (px, py) = ((x * 1000.0) as i64, (y * 1000.0) as i64);
            px >= r.x && py >= r.y && px < r.x + r.width && py < r.y + r.height
        })
    }

    pub fn pointer(&mut self, x: f64, y: f64) {
        if let Some(b) = self.hit(x, y) {
            self.focus = Some(b);
            self.activate(b);
        }
    }

    /// Offset and opacity a node inherits from drift and appear motion.
    fn motion(&self, i: usize, now: Instant) -> (f32, f32, f32) {
        let mut dx = 0f32;
        let mut dy = 0f32;
        let mut alpha = 1f32;
        let mut at = Some(i);
        while let Some(n) = at {
            let parent = self.parents[n];
            let (pw, ph) = parent.map_or((self.width as f32, self.height as f32), |p| (self.rect(p).2, self.rect(p).3));
            let c = &self.cells[n];
            if !self.reduce_motion {
                if let (Some(d), Some(period)) = (c.drift_p, c.drift_period_s) {
                    let a = 2.0 * std::f64::consts::PI * (now.duration_since(self.start).as_secs_f64() / period).fract();
                    dx += (d.x * f64::from(pw) * a.cos()) as f32;
                    dy += (d.y * f64::from(ph) * a.sin()) as f32;
                }
                if let (Some(t), Some(s)) = (self.appear_from[n], c.appear_s) {
                    let p = (now.duration_since(t).as_secs_f64() / s).clamp(0.0, 1.0);
                    let ease = 1.0 - (1.0 - p).powi(3);
                    alpha *= ease as f32;
                    if let Some(from) = c.appear_from_p {
                        dx += (from.x * f64::from(pw) * (1.0 - ease)) as f32;
                        dy += (from.y * f64::from(ph) * (1.0 - ease)) as f32;
                    }
                }
            }
            at = parent;
        }
        (dx, dy, alpha)
    }

    fn fill_at(&self, i: usize, now: Instant) -> Option<u32> {
        let c = &self.cells[i];
        let from = c.fill?;
        match (c.fill_to, c.fill_period_s) {
            (Some(to), Some(period)) if !self.reduce_motion => {
                let t = now.duration_since(self.start).as_secs_f64() / period;
                let k = 0.5 - 0.5 * (std::f64::consts::PI * t).cos();
                let mix = |s: u32| {
                    let a = f64::from((from >> s) & 0xff);
                    let b = f64::from((to >> s) & 0xff);
                    ((a + (b - a) * k).round() as u32) << s
                };
                Some(mix(16) | mix(8) | mix(0))
            }
            _ => Some(from),
        }
    }

    fn paint(rgb: u32, alpha: f32) -> tiny_skia::Paint<'static> {
        let mut p = tiny_skia::Paint::default();
        p.set_color_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, (alpha * 255.0).round().clamp(0.0, 255.0) as u8);
        p.anti_alias = true;
        p
    }

    /// Render the current state into an RGBA frame.
    pub fn render(&self) -> tiny_skia::Pixmap {
        let now = Instant::now();
        let mut pm = tiny_skia::Pixmap::new(self.width.max(1), self.height.max(1)).expect("non-empty frame");
        pm.fill(tiny_skia::Color::WHITE);
        let present = self.presence();
        let focused = self.focus.filter(|f| present[*f]);
        for i in 0..self.cells.len() {
            if !present[i] {
                continue;
            }
            let (dx, dy, alpha) = self.motion(i, now);
            if alpha <= 0.0 {
                continue;
            }
            let (x, y, w, h) = self.rect(i);
            let (x, y) = (x + dx, y + dy);
            let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w.max(0.01), h.max(0.01)) else { continue };
            let node = &self.frame.nodes[i];
            if let Some(fill) = self.fill_at(i, now) {
                pm.fill_rect(rect, &Self::paint(fill, alpha), tiny_skia::Transform::identity(), None);
            }
            if let Some(ds) = &self.draws[i] {
                self.paint_graphic(&mut pm, ds, x, y, w, h, alpha);
            }
            if let (Some(stroke), Some(sw)) = (node.stroke, node.stroke_width_mp) {
                let sw = sw as f32 / 1000.0;
                if let Some(inner) = tiny_skia::Rect::from_xywh(x + sw / 2.0, y + sw / 2.0, (w - sw).max(0.01), (h - sw).max(0.01)) {
                    let path = tiny_skia::PathBuilder::from_rect(inner);
                    let st = tiny_skia::Stroke { width: sw, ..Default::default() };
                    pm.stroke_path(&path, &Self::paint(stroke, alpha), &st, tiny_skia::Transform::identity(), None);
                }
            }
            let mut text = node.text.clone().unwrap_or_default();
            if let Some(key) = &self.cells[i].text_from {
                text.push_str(&shown(self.state.get(key)));
            }
            if !text.is_empty() {
                let size = node.text_size_mp.map_or(16.0, |s| s as f32 / 1000.0);
                self.draw_text(&mut pm, &text, x, y, w, h, size, node.is_button, node.anchor, self.cells[i].text_from.is_some(), alpha);
            }
            if Some(i) == focused {
                for (grow, colour) in [(6.0f32, 0xffffffu32), (2.0, 0x000000)] {
                    if let Some(ring) = tiny_skia::Rect::from_xywh(x - grow, y - grow, w + 2.0 * grow, h + 2.0 * grow) {
                        let st = tiny_skia::Stroke { width: 4.0, ..Default::default() };
                        pm.stroke_path(&tiny_skia::PathBuilder::from_rect(ring), &Self::paint(colour, 1.0), &st, tiny_skia::Transform::identity(), None);
                    }
                }
            }
        }
        pm
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_graphic(&self, pm: &mut tiny_skia::Pixmap, ds: &DrawState, x: f32, y: f32, w: f32, h: f32, alpha: f32) {
        let g = &self.graphics[ds.graphic];
        let state: Vec<u16> = ds.shown.iter().map(|&v| v as u16).collect();
        let Ok(p) = g.inv.resolve(&state) else { return };
        let (gw, gh) = g.inv_size();
        let (sx, sy) = (w / gw, h / gh);
        for k in 0..p.len() {
            let t = p.transform(k);
            let tr = tiny_skia::Transform::from_row(
                sx * t.a as f32 / 65536.0,
                sy * t.b as f32 / 65536.0,
                sx * t.c as f32 / 65536.0,
                sy * t.d as f32 / 65536.0,
                x + sx * t.e as f32,
                y + sy * t.f as f32,
            );
            let path = &g.paths[p.geometry(k) as usize];
            let style = p.style(k);
            let o = f32::from(p.opacity(k)) / 255.0 * alpha;
            if let Some(f) = style.fill {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(f.colour.r, f.colour.g, f.colour.b, (f32::from(f.colour.a) * o).round() as u8);
                paint.anti_alias = true;
                let rule = match f.rule {
                    goya_graphics_ir::FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
                    goya_graphics_ir::FillRule::NonZero => tiny_skia::FillRule::Winding,
                };
                pm.fill_path(path, &paint, rule, tr, None);
            }
            if let Some(s) = style.stroke {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(s.colour.r, s.colour.g, s.colour.b, (f32::from(s.colour.a) * o).round() as u8);
                paint.anti_alias = true;
                let st = tiny_skia::Stroke {
                    width: s.width as f32,
                    line_cap: match s.cap {
                        goya_graphics_ir::LineCap::Butt => tiny_skia::LineCap::Butt,
                        goya_graphics_ir::LineCap::Round => tiny_skia::LineCap::Round,
                        goya_graphics_ir::LineCap::Square => tiny_skia::LineCap::Square,
                    },
                    line_join: tiny_skia::LineJoin::Round,
                    ..Default::default()
                };
                pm.stroke_path(path, &paint, &st, tr, None);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(&self, pm: &mut tiny_skia::Pixmap, text: &str, x: f32, y: f32, w: f32, h: f32, size: f32, centered: bool, anchor: goya_lang::Anchor, wrap: bool, alpha: f32) {
        use ab_glyph::{Font, ScaleFont};
        let font = self.font.as_scaled(ab_glyph::PxScale::from(size));
        let width_of = |s: &str| -> f32 { s.chars().map(|c| font.h_advance(font.glyph_id(c))).sum() };
        // Lines: authored breaks, then (for live text) words wrapped at the edge.
        let mut lines: Vec<String> = Vec::new();
        for authored in text.split('\n') {
            if !wrap || width_of(authored) <= w {
                lines.push(authored.to_owned());
                continue;
            }
            let mut line = String::new();
            for word in authored.split(' ') {
                let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
                if !line.is_empty() && width_of(&candidate) > w {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_owned();
                } else {
                    line = candidate;
                }
            }
            lines.push(line);
        }
        let line_h = font.height() + font.line_gap();
        let block_h = line_h * lines.len() as f32;
        let mut top = if centered { y + (h - block_h) / 2.0 } else { y };
        let (cw, ch) = (pm.width() as i32, pm.height() as i32);
        let (clip_x0, clip_y0, clip_x1, clip_y1) = (x.max(0.0) as i32, y.max(0.0) as i32, ((x + w) as i32).min(cw), ((y + h) as i32).min(ch));
        let a = (alpha * 255.0) as u32;
        for line in &lines {
            let lw = width_of(line);
            let mut pen = if centered || anchor == goya_lang::Anchor::Middle {
                x + (w - lw) / 2.0
            } else if anchor == goya_lang::Anchor::End {
                x + w - lw
            } else {
                x
            };
            let baseline = top + font.ascent();
            for c in line.chars() {
                let id = font.glyph_id(c);
                let glyph = id.with_scale_and_position(ab_glyph::PxScale::from(size), ab_glyph::point(pen, baseline));
                pen += font.h_advance(id);
                let Some(outline) = self.font.outline_glyph(glyph) else { continue };
                let b = outline.px_bounds();
                let pixels = pm.pixels_mut();
                outline.draw(|gx, gy, cov| {
                    let px = b.min.x as i32 + gx as i32;
                    let py = b.min.y as i32 + gy as i32;
                    if px < clip_x0 || py < clip_y0 || px >= clip_x1 || py >= clip_y1 {
                        return;
                    }
                    let k = (cov.clamp(0.0, 1.0) * a as f32) as u32;
                    let idx = (py * cw + px) as usize;
                    let d = pixels[idx];
                    let blend = |dst: u8| ((u32::from(dst) * (255 - k)) / 255) as u8;
                    pixels[idx] = tiny_skia::PremultipliedColorU8::from_rgba(blend(d.red()), blend(d.green()), blend(d.blue()), 255).unwrap_or(d);
                });
            }
            top += line_h;
        }
    }

    /// Accessibility tree for AccessKit: window, present buttons and texts,
    /// and the polite live region carrying the last D9 announcement.
    pub fn accessibility(&self) -> accesskit::TreeUpdate {
        use accesskit::{Action, Live, NodeBuilder, NodeId, Rect, Role, Tree, TreeUpdate};
        let present = self.presence();
        let root = NodeId(u64::MAX);
        let live = NodeId(u64::MAX - 1);
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for i in 0..self.cells.len() {
            if !present[i] {
                continue;
            }
            let c = &self.cells[i];
            let node = &self.frame.nodes[i];
            let mut text = node.text.clone().unwrap_or_default();
            if let Some(key) = &c.text_from {
                text.push_str(&shown(self.state.get(key)));
            }
            let (x, y, w, h) = self.rect(i);
            let bounds = Rect { x0: f64::from(x), y0: f64::from(y), x1: f64::from(x + w), y1: f64::from(y + h) };
            if node.is_button {
                let mut b = NodeBuilder::new(Role::Button);
                b.set_name(c.iden.as_ref().map_or(text.clone(), |d| d.label.clone()));
                if let Some(hint) = c.iden.as_ref().and_then(|d| d.hint.clone()) {
                    b.set_description(hint);
                }
                b.add_action(Action::Default);
                b.add_action(Action::Focus);
                b.set_bounds(bounds);
                nodes.push((NodeId(i as u64), b.build()));
                children.push(NodeId(i as u64));
            } else if !text.trim().is_empty() || c.iden.is_some() {
                let mut l = NodeBuilder::new(Role::Label);
                l.set_name(c.iden.as_ref().map_or(text.clone(), |d| d.label.clone()));
                l.set_value(text);
                l.set_bounds(bounds);
                nodes.push((NodeId(i as u64), l.build()));
                children.push(NodeId(i as u64));
            }
        }
        let mut status = NodeBuilder::new(Role::Status);
        status.set_live(Live::Polite);
        status.set_name(self.last_announcement.clone());
        nodes.push((live, status.build()));
        children.push(live);
        let mut window = NodeBuilder::new(Role::Window);
        window.set_name(self.parsed.scene.name.clone());
        window.set_children(children);
        nodes.push((root, window.build()));
        let focus = self.focus.filter(|f| present[*f]).map_or(root, |f| NodeId(f as u64));
        TreeUpdate { nodes, tree: Some(Tree::new(root)), focus }
    }

    /// Handle an AccessKit action on node `id`.
    pub fn accessibility_action(&mut self, id: u64, action: accesskit::Action) {
        let i = id as usize;
        if i >= self.cells.len() || !self.buttons().contains(&i) {
            return;
        }
        self.focus = Some(i);
        if action == accesskit::Action::Default {
            self.activate(i);
        }
    }
}

impl Gfx {
    fn inv_size(&self) -> (f32, f32) {
        let (w, h) = self.inv.canvas();
        (w as f32, h as f32)
    }
}

/// `linux.xdg-state.v1`: the app's own D8 record.
struct Store {
    path: Option<PathBuf>,
    stored: Option<String>,
    failed: bool,
    extra: BTreeMap<String, String>,
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl Store {
    fn open(ontology: &str) -> Store {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")));
        Store { path: base.map(|b| b.join("goya").join(format!("{ontology}.json"))), stored: None, failed: false, extra: BTreeMap::new() }
    }

    fn read(&self) -> Result<Option<String>, String> {
        let Some(path) = &self.path else { return Err("no state directory".into()) };
        match std::fs::read_to_string(path) {
            Ok(t) => Ok(Some(t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn restore(&mut self, types: &BTreeMap<String, InteractionType>, durable: &[String]) -> Result<BTreeMap<String, D1Value>, String> {
        let result = (|| {
            let text = self.read()?;
            self.stored = text.clone();
            let Some(text) = text else { return Ok(BTreeMap::new()) };
            if text.len() > 1 << 20 {
                return Err("record above 1 MiB".to_owned());
            }
            let rec: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let obj = rec.as_object().ok_or("record is not an object")?;
            if obj.len() != 2 || obj.get("format").and_then(|f| f.as_str()) != Some("goya.state.v1") {
                return Err("unknown record envelope".into());
            }
            let values = obj.get("values").and_then(|v| v.as_object()).ok_or("record has no values")?;
            let mut out = BTreeMap::new();
            for (k, e) in values {
                let e = e.as_array().filter(|a| a.len() == 2).ok_or(format!("entry {k} is not a pair"))?;
                let tag = e[0].as_str().unwrap_or("");
                let v = match tag {
                    "int" => {
                        let s = e[1].as_str().ok_or(format!("int {k} is not a string"))?;
                        let digits = s.strip_prefix('-').unwrap_or(s);
                        let canonical = !digits.is_empty()
                            && digits.chars().all(|c| c.is_ascii_digit())
                            && (digits == "0" && s == "0" || !digits.starts_with('0'));
                        if !canonical {
                            return Err(format!("non-canonical int for {k}"));
                        }
                        D1Value::Int(s.parse().map_err(|_| format!("int out of range for {k}"))?)
                    }
                    "text" => D1Value::Text(e[1].as_str().ok_or(format!("invalid text for {k}"))?.to_owned()),
                    "bool" => D1Value::Bool(e[1].as_bool().ok_or(format!("invalid bool for {k}"))?),
                    _ => return Err(format!("unknown type tag for {k}")),
                };
                if durable.contains(k) {
                    let want = match types.get(k) {
                        Some(InteractionType::Int) => "int",
                        Some(InteractionType::Text) => "text",
                        _ => "bool",
                    };
                    if want != tag {
                        return Err(format!("durable key {k} was retyped"));
                    }
                    out.insert(k.clone(), v);
                } else {
                    self.extra.insert(k.clone(), serde_json::to_string(e).unwrap_or_default());
                }
            }
            Ok(out)
        })();
        if result.is_err() {
            self.failed = true;
            self.extra.clear();
        }
        result
    }

    fn save(&mut self, state: &BTreeMap<String, D1Value>, types: &BTreeMap<String, InteractionType>, durable: &[String]) -> Result<(), String> {
        if self.failed {
            return Err("durable state was not restored, so it is not overwritten (D4)".into());
        }
        if self.read()? != self.stored {
            return Err("durable state changed in another window; restart to continue (D8)".into());
        }
        let mut entries: Vec<String> = self.extra.iter().map(|(k, raw)| format!("{}:{raw}", quote(k))).collect();
        for k in durable {
            let entry = match (state.get(k), types.get(k)) {
                (Some(D1Value::Int(i)), _) => format!("[\"int\",{}]", quote(&i.to_string())),
                (Some(D1Value::Text(t)), _) => format!("[\"text\",{}]", quote(t)),
                (Some(D1Value::Bool(b)), _) => format!("[\"bool\",{b}]"),
                (None, _) => return Err(format!("durable key {k} has no value")),
            };
            entries.push(format!("{}:{entry}", quote(k)));
        }
        let text = format!("{{\"format\":\"goya.state.v1\",\"values\":{{{}}}}}", entries.join(","));
        let path = self.path.clone().ok_or("no state directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        self.stored = Some(text);
        Ok(())
    }
}

/// AccessKit bridge: actions arrive on the adapter's thread and are queued
/// for the event loop.
pub struct A11y {
    adapter: accesskit_unix::Adapter,
    actions: mpsc::Receiver<accesskit::ActionRequest>,
}

struct Activation(Arc<Mutex<Option<accesskit::TreeUpdate>>>);
impl accesskit::ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
        self.0.lock().ok().and_then(|t| t.clone())
    }
}
struct Actions(Mutex<mpsc::Sender<accesskit::ActionRequest>>);
impl accesskit::ActionHandler for Actions {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        if let Ok(tx) = self.0.lock() {
            let _ = tx.send(request);
        }
    }
}
struct Deactivation;
impl accesskit::DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}

impl A11y {
    pub fn new(initial: accesskit::TreeUpdate) -> A11y {
        let (tx, rx) = mpsc::channel();
        let adapter = accesskit_unix::Adapter::new(Activation(Arc::new(Mutex::new(Some(initial)))), Actions(Mutex::new(tx)), Deactivation);
        A11y { adapter, actions: rx }
    }

    pub fn update(&mut self, scene: &Scene) {
        self.adapter.update_if_active(|| scene.accessibility());
    }

    pub fn focus(&mut self, focused: bool) {
        self.adapter.update_window_focus_state(focused);
    }

    pub fn drain(&mut self) -> Vec<accesskit::ActionRequest> {
        self.actions.try_iter().collect()
    }
}
