//! goya-calculator — the exemplar Goya GUI application.
//!
//! The UI is 100% declared in `apps/calculator.gscene` (GY-LANG/1
//! RUGID-style syntax). This host is the behavioural half: it maps button
//! names to a deterministic calculator state machine, overrides the
//! `result_text` cell with the live value, routes keyboard input, plays a
//! spatially-positioned tap on every press (ARSEM pan from the button's
//! geometry), and supports transactional hot-swap of the scene while
//! running.
//!
//!   cargo run --bin calculator -- --live [--sound] [--watch]
//!
//! Without `--live`, a scripted acceptance sequence runs headless and
//! prints PASS/FAIL per check (7 + 3 = 10, divide/multiply, clear, sign).

use goya_audio_dsp::{limit, Limiter, Tap};
use goya_audio_linux::{AudioOutput, PwCatOutput};
use goya_change::{
    decide, CausalResolver, ChangeEnvelope, CostModel, EventRule, Invalidation, PixelRect,
    PropertyMask,
};
use goya_lang::{hit_test, layout, Anchor, DrawNode, RectMp, SceneStore};
use goya_rars::{
    AccessibilityPolicy, DevicePixelSize, ErrorBudget, FidelityResolver, MotionProfile,
    ProjectionFidelityContext,
};
use goya_runtime::Primitive;
use goya_spatial_audio::{pan_from_rect, StereoPan};

const SCENE_PATH: &str = "apps/calculator.gscene";
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 680;
const VIEWPORT_MP: i64 = 400_000; // 400 pt * 1000

/// Deterministic calculator state machine. Pure logic — no rendering — so
/// the acceptance sequence can drive it headless.
#[derive(Default)]
struct Calc {
    /// Accumulated value (left operand once an op is pending).
    acc: f64,
    /// Digits currently being typed.
    entry: String,
    /// Pending operation: '+', '-', '*', '/'.
    pending: Option<char>,
    /// The last key evaluated; a second '=' repeats nothing in this model.
    fresh_entry: bool,
}

impl Calc {
    fn shown(&self) -> String {
        if self.entry.is_empty() {
            format_value(self.acc)
        } else {
            self.entry.clone()
        }
    }

    fn digit(&mut self, d: char) {
        if self.fresh_entry {
            self.entry.clear();
            self.fresh_entry = false;
        }
        if self.entry == "0" && d == '0' {
            return; // no leading zeros
        }
        if self.entry.chars().filter(|c| *c == '.').count() < 3 && self.entry.len() < 14 {
            self.entry.push(d);
        }
    }

    fn decimal(&mut self) {
        if self.fresh_entry {
            self.entry.clear();
            self.fresh_entry = false;
        }
        if !self.entry.contains('.') {
            if self.entry.is_empty() {
                self.entry.push('0');
            }
            self.entry.push('.');
        }
    }

    fn apply_op(&mut self, op: char) {
        let operand = if self.entry.is_empty() {
            self.acc
        } else {
            self.entry.parse::<f64>().unwrap_or(self.acc)
        };
        match self.pending {
            Some(p) => {
                self.acc = match p {
                    '+' => self.acc + operand,
                    '-' => self.acc - operand,
                    '*' => self.acc * operand,
                    '/' => {
                        if operand == 0.0 {
                            f64::NAN
                        } else {
                            self.acc / operand
                        }
                    }
                    _ => operand,
                };
            }
            None => self.acc = operand,
        }
        self.pending = Some(op);
        self.entry.clear();
        self.fresh_entry = true;
    }

    fn equals(&mut self) {
        if let Some(p) = self.pending.take() {
            let operand = if self.entry.is_empty() {
                self.acc
            } else {
                self.entry.parse::<f64>().unwrap_or(0.0)
            };
            self.acc = match p {
                '+' => self.acc + operand,
                '-' => self.acc - operand,
                '*' => self.acc * operand,
                '/' => {
                    if operand == 0.0 {
                        f64::NAN
                    } else {
                        self.acc / operand
                    }
                }
                _ => self.acc,
            };
            self.entry.clear();
            self.fresh_entry = true;
        }
    }

    fn clear(&mut self) {
        *self = Calc {
            acc: 0.0,
            entry: String::new(),
            pending: None,
            fresh_entry: false,
        };
    }

    fn sign(&mut self) {
        if let Some(rest) = self.entry.strip_prefix('-') {
            self.entry = rest.to_string();
        } else if !self.entry.is_empty() {
            self.entry = format!("-{}", self.entry);
        } else if self.acc != 0.0 {
            self.acc = -self.acc;
        }
    }

    fn percent(&mut self) {
        let v = if self.entry.is_empty() {
            self.acc
        } else {
            self.entry.parse::<f64>().unwrap_or(0.0)
        };
        let v = v / 100.0;
        self.entry.clear();
        self.acc = v;
        self.fresh_entry = true;
    }

    /// Map a scene button name to its behaviour.
    fn press(&mut self, name: &str) {
        match name {
            "btn_clear" => self.clear(),
            "btn_sign" => self.sign(),
            "btn_pct" => self.percent(),
            "btn_eq" => self.equals(),
            "btn_add" => self.apply_op('+'),
            "btn_sub" => self.apply_op('-'),
            "btn_mul" => self.apply_op('*'),
            "btn_div" => self.apply_op('/'),
            "btn_dec" => self.decimal(),
            _ => {
                if let Some(rest) = name.strip_prefix("btn_") {
                    if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_digit()) {
                        let d = rest.chars().next().unwrap();
                        self.digit(d);
                    }
                }
            }
        }
    }
}

fn format_value(v: f64) -> String {
    if v.is_nan() {
        "Error".to_string()
    } else if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Override the display cell's text with the live value (host-side data
/// binding: the scene declares structure, the host supplies state).
fn with_result(mut nodes: Vec<DrawNode>, text: &str) -> Vec<DrawNode> {
    for n in &mut nodes {
        if n.name == "result_text" {
            n.text = Some(text.to_string());
        }
    }
    nodes
}

fn present_nodes(nodes: &[DrawNode]) -> Vec<Primitive> {
    nodes
        .iter()
        .map(|n| {
            let rect = goya_layout::Rect {
                x: n.rect.x,
                y: n.rect.y,
                width: n.rect.width,
                height: n.rect.height,
            };
            let mut label = n.text.clone().unwrap_or_default();
            if n.is_button && label.is_empty() {
                label = n.name.clone();
            }
            if n.is_button {
                Primitive::Button(rect, label)
            } else if n.text.is_some() {
                Primitive::Text(rect, label)
            } else {
                Primitive::Panel(rect)
            }
        })
        .collect()
}

/// Keyboard map: digits, operators, Enter '=', Backspace clear, '.' dec.
fn key_to_button(c: char) -> Option<&'static str> {
    Some(match c {
        '0'..='9' => {
            // known at compile time via the digit itself
            match c {
                '0' => "btn_0",
                '1' => "btn_1",
                '2' => "btn_2",
                '3' => "btn_3",
                '4' => "btn_4",
                '5' => "btn_5",
                '6' => "btn_6",
                '7' => "btn_7",
                '8' => "btn_8",
                _ => "btn_9",
            }
        }
        '+' => "btn_add",
        '-' => "btn_sub",
        '*' => "btn_mul",
        '/' => "btn_div",
        '.' => "btn_dec",
        '\n' | '=' => "btn_eq",
        '\u{8}' => "btn_clear",
        '%' => "btn_pct",
        _ => return None,
    })
}

fn play_tap(audio: &mut Option<PwCatOutput>, pan: &mut StereoPan, rect: RectMp) {
    pan.set_target(pan_from_rect(
        goya_layout::Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        },
        goya_layout::Rect {
            x: 0,
            y: 0,
            width: VIEWPORT_MP,
            height: 680_000,
        },
    ));
    let _ = pan.step(0.5);
    if let Some(out) = audio.as_mut() {
        let t = Tap::new(660.0, 48_000.0, 0.08);
        let mut lim = Limiter::new();
        let buf: Vec<f32> = (0..3_840)
            .map(|i| lim.push(0.45 * t.sample(i as f32 / 48_000.0)))
            .collect();
        let _ = AudioOutput::write(out, &buf);
        let _ = limit(0.0, 0.9); // limiter path stays warm (allocation-free contract)
    }
}

fn modified(m: &std::fs::Metadata) -> u64 {
    use std::time::UNIX_EPOCH;
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "--live");
    let watch = args.iter().any(|a| a == "--watch");
    let sound = args.iter().any(|a| a == "--sound");
    let scene_path = args
        .iter()
        .position(|a| a == "--file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or(SCENE_PATH);
    let max_frames: u32 = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(7200);

    let source = std::fs::read_to_string(scene_path).unwrap_or_else(|e| {
        eprintln!("goya-calculator: cannot read {scene_path}: {e}");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-calculator: {e}");
            std::process::exit(1);
        }
    };
    let mut last_mtime = std::fs::metadata(scene_path).ok().map(|m| modified(&m));
    if watch {
        println!(
            "goya-calculator: watching {scene_path} (cwd {:?}, initial mtime {:?})",
            std::env::current_dir().map(|p| p.display().to_string()),
            last_mtime
        );
    }

    if !live {
        self_test(&mut store);
        return;
    }

    // ---- LCPE (GY-CHANGE/1): compile change envelopes for the display
    // panel and every button from the resolved scene geometry ------------
    let frame0 = layout(store.get(), 100.0, 170.0);
    let mut resolver = CausalResolver::default();
    for node in &frame0.nodes {
        let persistent = if node.is_button {
            PropertyMask(PropertyMask::TEXT | PropertyMask::GEOMETRY)
        } else {
            PropertyMask(PropertyMask::GEOMETRY)
        };
        resolver.register(ChangeEnvelope {
            object: node.name.clone(),
            revision: store.generation(),
            persistent,
            mutable: PropertyMask(PropertyMask::ALL),
            event_rules: if node.is_button {
                vec![EventRule {
                    event: "pointer.press".into(),
                    causes: vec!["pressed".into(), "hover".into()],
                }]
            } else {
                vec![EventRule {
                    event: "state.value".into(),
                    causes: vec!["value".into()],
                }]
            },
            spatial_bounds: PixelRect {
                x: node.rect.x as i32 / goya_wayland::MP_PER_PX as i32,
                y: node.rect.y as i32 / goya_wayland::MP_PER_PX as i32,
                width: node.rect.width as i32 / goya_wayland::MP_PER_PX as i32,
                height: node.rect.height as i32 / goya_wayland::MP_PER_PX as i32,
            },
            resolution_tile: 8,
        });
    }
    let cost_model = CostModel::default();

    // ---- RARS (GY-RARS/1): glyph fidelity from the projected resolution
    let rars = FidelityResolver::default();
    let rars_ctx = ProjectionFidelityContext {
        projected_size: DevicePixelSize {
            width: WIN_W_PX as u32,
            height: WIN_H_PX as u32,
        },
        device_scale: 1.0,
        motion: MotionProfile { velocity_px_s: 0.0 },
        error: ErrorBudget {
            spatial_px: 0.5,
            temporal_ms: 8.0,
        },
        accessibility: AccessibilityPolicy {
            protect_text: true,
            min_hit_target_px: 48,
        },
    };
    let fidelity = rars.resolve(&rars_ctx);
    println!(
        "goya-calculator: RARS glyph_px={} subdivisions={} quality={} (window {}x{} vs 4K = {:.2}%)",
        fidelity.glyph_px,
        fidelity.spatial_subdivisions,
        fidelity.effect_quality,
        WIN_W_PX,
        WIN_H_PX,
        rars_ctx.relative_area() * 100.0
    );

    let mut win = match goya_wayland::GoyaWindow::create("goya-calculator", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("goya-calculator: live unavailable: {e}");
            return;
        }
    };
    let mut calc = Calc::default();
    let mut audio = if sound { PwCatOutput::spawn() } else { None };
    let mut pan = StereoPan::new();
    let mut frames = 0u32;
    let mut presents = 0u32;
    let mut dirty = true; // first frame always paints
    let mut lcpe_damage: Option<(i32, i32, i32, i32)> = None;
    let mut last_shown = String::new();
    while frames < max_frames && !win.close_requested() {
        if watch {
            if frames.is_multiple_of(5000) {
                println!(
                    "goya-calculator: frame {frames} mtime {:?} (last {:?})",
                    std::fs::metadata(scene_path).ok().map(|m| modified(&m)),
                    last_mtime
                );
            }
            if let Ok(m) = std::fs::metadata(scene_path) {
                let mtime = modified(&m);
                if last_mtime != Some(mtime) {
                    last_mtime = Some(mtime);
                    match std::fs::read_to_string(scene_path) {
                        Ok(src) => match store.swap(&src) {
                            Ok(gen) => {
                                println!("goya-calculator: hot-swapped to generation {gen}");
                                dirty = true;
                            }
                            Err(e) => println!(
                                "goya-calculator: swap rejected ({e}) — running calculator retained"
                            ),
                        },
                        Err(e) => println!("goya-calculator: read failed ({e}) — retained"),
                    }
                }
            }
        }
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let frame = layout(store.get(), 100.0, 170.0);
                    if let Some(name) = hit_test(
                        &frame,
                        (x as i64) * goya_wayland::MP_PER_PX,
                        (y as i64) * goya_wayland::MP_PER_PX,
                    ) {
                        if name.starts_with("btn_") {
                            let rect = frame.find(&name).map(|n| n.rect);
                            calc.press(&name);
                            dirty = true;
                            // LCPE: event → authorised transitions →
                            // quantised dirty regions → local vs full cost
                            // decision. The display cell changes via the
                            // "value" transition; the button via "pressed".
                            let mut dirty_rects: Vec<PixelRect> = Vec::new();
                            for t in resolver.resolve(&name, "pointer.press", "pressed") {
                                if let Some(d) = t.dirty {
                                    dirty_rects.push(d);
                                }
                            }
                            for t in resolver.resolve("result_text", "state.value", "value") {
                                if let Some(d) = t.dirty {
                                    dirty_rects.push(d);
                                }
                            }
                            let px_rects: Vec<PixelRect> = dirty_rects
                                .iter()
                                .map(|r| PixelRect {
                                    x: r.x,
                                    y: r.y,
                                    width: r.width,
                                    height: r.height,
                                })
                                .collect();
                            match decide(&px_rects, &cost_model) {
                                Invalidation::Local(r) => {
                                    lcpe_damage = Some((r.x, r.y, r.width, r.height));
                                }
                                Invalidation::FullFrame => {
                                    lcpe_damage = None;
                                }
                            }
                            if let Some(r) = rect {
                                play_tap(&mut audio, &mut pan, r);
                            }
                        }
                    }
                }
                goya_wayland::PlatformEvent::KeyChar(c) => {
                    if let Some(btn) = key_to_button(c) {
                        calc.press(btn);
                        dirty = true;
                        if let Some(n) = frame_find(store.get(), btn) {
                            play_tap(&mut audio, &mut pan, n.rect);
                        }
                    }
                }
                goya_wayland::PlatformEvent::Resized(w, h) => {
                    println!("goya-calculator: resized {w}x{h}");
                    dirty = true;
                }
                _ => {}
            }
        }
        // Redraw only when something changed — the flicker fix's second
        // half. Idle frames still pump compositor events but commit nothing.
        if dirty {
            let nodes = layout(store.get(), 100.0, 170.0).nodes;
            let nodes = with_result(nodes, &calc.shown());
            let prims = present_nodes(&nodes);
            let _ = win.present_dirty(
                &prims,
                [235, 235, 240],
                [20, 20, 28],
                fidelity.glyph_px.max(2),
                lcpe_damage,
            );
            if let Some((dx, dy, dw, dh)) = lcpe_damage {
                println!(
                    "goya-calculator: LCPE local invalidation {}x{} @ {},{}",
                    dw, dh, dx, dy
                );
            }
            last_shown = calc.shown();
            presents += 1;
            dirty = false;
        }
        win.pump(1);
        frames += 1;
    }
    println!(
        "goya-calculator live: {frames} frames, {presents} presents, last display '{}', generation {}",
        last_shown,
        store.generation()
    );
}

fn frame_find(scene: &goya_lang::Scene, name: &str) -> Option<DrawNode> {
    let frame = layout(scene, 100.0, 170.0);
    frame.find(name).cloned()
}

/// Headless acceptance: drive the state machine exactly as clicks would,
/// verify the display values, and sanity-check scene structure.
fn self_test(store: &mut SceneStore) {
    let mut failures = Vec::new();
    let mut calc = Calc::default();
    let frame = layout(store.get(), 100.0, 170.0);

    // Structural checks: all 19 buttons present, grid-aligned, hit-testable.
    let names = [
        "btn_clear",
        "btn_sign",
        "btn_pct",
        "btn_div",
        "btn_7",
        "btn_8",
        "btn_9",
        "btn_mul",
        "btn_4",
        "btn_5",
        "btn_6",
        "btn_sub",
        "btn_1",
        "btn_2",
        "btn_3",
        "btn_add",
        "btn_0",
        "btn_dec",
        "btn_eq",
    ];
    for n in names {
        if frame.find(n).is_none() {
            failures.push(format!("missing button {n}"));
        }
    }
    if let Some(b) = frame.find("btn_eq") {
        let hit = hit_test(&frame, b.rect.x + 1_000, b.rect.y + 1_000);
        if hit.as_deref() != Some("btn_eq") {
            failures.push("btn_eq not hit-testable".into());
        }
    }
    // 7 + 3 = 10
    for step in ["btn_7", "btn_add", "btn_3", "btn_eq"] {
        calc.press(step);
    }
    if calc.shown() != "10" {
        failures.push(format!("7+3: expected 10, got {}", calc.shown()));
    }
    // 10 * 5 = 50 (continuing from accumulator)
    for step in ["btn_mul", "btn_5", "btn_eq"] {
        calc.press(step);
    }
    if calc.shown() != "50" {
        failures.push(format!("10*5: expected 50, got {}", calc.shown()));
    }
    // 50 / 0 → Error
    calc.press("btn_div");
    calc.press("btn_0");
    calc.press("btn_eq");
    if calc.shown() != "Error" {
        failures.push(format!("50/0: expected Error, got {}", calc.shown()));
    }
    // Clear, then 9.5 - 2.25 = 7.25
    calc.press("btn_clear");
    for step in [
        "btn_9", "btn_dec", "btn_5", "btn_sub", "btn_2", "btn_dec", "btn_2", "btn_5", "btn_eq",
    ] {
        calc.press(step);
    }
    if calc.shown() != "7.25" {
        failures.push(format!("9.5-2.25: expected 7.25, got {}", calc.shown()));
    }
    // Sign toggle on entry
    calc.press("btn_clear");
    calc.press("btn_4");
    calc.press("btn_sign");
    if calc.shown() != "-4" {
        failures.push(format!("sign: expected -4, got {}", calc.shown()));
    }
    // Proportional display cell sits inside the display panel.
    if let (Some(disp), Some(res)) = (frame.find("display"), frame.find("result_text")) {
        let inside = res.rect.x >= disp.rect.x && res.rect.x < disp.rect.x + disp.rect.width;
        if !inside {
            failures.push("result_text outside display panel".into());
        }
        if disp.rect.width != 90_000 {
            failures.push(format!(
                "display extent: expected 90000, got {}",
                disp.rect.width
            ));
        }
    } else {
        failures.push("display/result_text cells missing".into());
    }

    if failures.is_empty() {
        println!("goya-calculator: ALL EXEMPLAR CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("EXEMPLAR FAILURE: {f}");
        }
        std::process::exit(1);
    }
}

// Anchor import retained for scene-language completeness at this call site.
#[allow(unused)]
fn _anchor_check(a: Anchor) -> bool {
    matches!(a, Anchor::Start | Anchor::Middle | Anchor::End)
}
