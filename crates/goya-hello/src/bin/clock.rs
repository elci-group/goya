//! goya-clock — CLOCK exemplar Goya application.
//!
//! The UI is 100% declared in `apps/clock.gscene` (GY-LANG/1 RUGID-style
//! syntax). This host is the behavioural half: it owns a deterministic
//! synthetic clock (seconds-since-midnight + day counter, no wall-clock
//! reads in headless mode), maps the h+/m+/s+/clear buttons onto it,
//! overrides the `time_text` and `day_text` cells with the live values,
//! and presents via `goya_wayland::GoyaWindow`.
//!
//!   cargo run --bin clock -- --live
//!
//! Without `--live`, a deterministic acceptance sequence runs headless:
//! render formatting, wrap-around arithmetic, 86400-step day sweep with
//! no drift, and reset. Prints PASS/FAIL; exit 0/1.

use goya_lang::{hit_test, layout, SceneStore};
use goya_runtime::Primitive;

const SCENE_PATH: &str = "apps/clock.gscene";
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 400;
const SCENE_W_PT: f64 = 100.0;
const SCENE_H_PT: f64 = 100.0;
const SECONDS_PER_DAY: u64 = 86_400;

/// Deterministic synthetic clock. Pure logic — no rendering, no
/// wall-clock — so the headless acceptance sequence is repeatable.
#[derive(Default)]
struct Clock {
    /// Seconds since midnight of the current day, 0..86399.
    seconds_since_midnight: u64,
    /// 1-based day counter; starts at 1.
    day: u64,
}

impl Clock {
    fn new() -> Self {
        Clock {
            seconds_since_midnight: 0,
            day: 1,
        }
    }

    fn add_sec(&mut self) {
        self.bump(1);
    }

    fn add_min(&mut self) {
        self.bump(60);
    }

    fn add_hour(&mut self) {
        self.bump(3600);
    }

    /// Advance by `n` seconds, rolling the day counter on midnight
    /// crossings (23.59.59 + 1s becomes 00.00.00 on day+1).
    fn bump(&mut self, n: u64) {
        let total = self.seconds_since_midnight + n;
        self.seconds_since_midnight = total % SECONDS_PER_DAY;
        self.day += total / SECONDS_PER_DAY;
    }

    /// Return to the start state: 00.00.00 day 1.
    fn reset(&mut self) {
        *self = Clock::new();
    }

    fn hour(&self) -> u64 {
        self.seconds_since_midnight / 3600
    }

    fn minute(&self) -> u64 {
        (self.seconds_since_midnight % 3600) / 60
    }

    fn second(&self) -> u64 {
        self.seconds_since_midnight % 60
    }

    fn day_text(&self) -> String {
        format!("day {}", self.day)
    }
}

/// Zero-padded 2-digit fields joined with '.' (':' has no glyph in the
/// limited renderer set), e.g. render(23, 59, 59) == "23.59.59".
fn render(h: u64, m: u64, s: u64) -> String {
    format!("{:02}.{:02}.{:02}", h, m, s)
}

impl Clock {
    fn time_text(&self) -> String {
        render(self.hour(), self.minute(), self.second())
    }
}

/// Host-side data binding: the scene declares structure, the host
/// supplies state. Override the time and day cell texts every frame.
fn with_display(
    mut nodes: Vec<goya_lang::DrawNode>,
    time: &str,
    day: &str,
) -> Vec<goya_lang::DrawNode> {
    for n in &mut nodes {
        if n.name == "time_text" {
            n.text = Some(time.to_string());
        } else if n.name == "day_text" {
            n.text = Some(day.to_string());
        }
    }
    nodes
}

/// Map the resolved draw list onto runtime primitives (panels, buttons,
/// text) with the per-node fill overrides required by GoyaWindow::present.
fn present_nodes(nodes: &[goya_lang::DrawNode]) -> (Vec<Primitive>, Vec<Option<u32>>) {
    let mut prims = Vec::with_capacity(nodes.len());
    let mut fills = Vec::with_capacity(nodes.len());
    for n in nodes {
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
            prims.push(Primitive::Button(rect, label));
        } else if n.text.is_some() {
            prims.push(Primitive::Text(rect, label));
        } else {
            prims.push(Primitive::Panel(rect));
        }
        fills.push(n.fill);
    }
    (prims, fills)
}

/// Map a scene button name to its behaviour.
fn press(clock: &mut Clock, name: &str) {
    match name {
        "btn_h_add" => clock.add_hour(),
        "btn_m_add" => clock.add_min(),
        "btn_s_add" => clock.add_sec(),
        "btn_reset" => clock.reset(),
        _ => {}
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "--live");
    let scene_path = args
        .iter()
        .position(|a| a == "--file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or(SCENE_PATH);

    let source = std::fs::read_to_string(scene_path).unwrap_or_else(|e| {
        eprintln!("goya-clock: cannot read {scene_path}: {e}");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-clock: {e}");
            std::process::exit(1);
        }
    };

    if !live {
        self_test(&mut store);
        return;
    }

    live_loop(&mut store);
}

/// Headless acceptance: deterministic clock arithmetic and scene
/// structure checks. No wall-clock reads anywhere in here.
fn self_test(store: &mut SceneStore) {
    let mut failures = Vec::new();
    let mut clock = Clock::new();
    let frame = layout(store.get(), SCENE_W_PT, SCENE_H_PT);

    // Structural checks: all buttons and display cells present,
    // buttons hit-testable at their centre.
    for n in [
        "btn_h_add",
        "btn_m_add",
        "btn_s_add",
        "btn_reset",
        "time_text",
        "day_text",
    ] {
        if frame.find(n).is_none() {
            failures.push(format!("missing scene node {n}"));
        }
    }
    for n in ["btn_h_add", "btn_m_add", "btn_s_add", "btn_reset"] {
        if let Some(b) = frame.find(n) {
            let hit = hit_test(
                &frame,
                b.rect.x + b.rect.width / 2,
                b.rect.y + b.rect.height / 2,
            );
            if hit.as_deref() != Some(n) {
                failures.push(format!("{n} not hit-testable"));
            }
        }
    }

    // render() formatting.
    if render(0, 0, 0) != "00.00.00" {
        failures.push(format!("render(0,0,0): got {}", render(0, 0, 0)));
    }
    if render(23, 59, 59) != "23.59.59" {
        failures.push(format!("render(23,59,59): got {}", render(23, 59, 59)));
    }

    // Start state: 00.00.00 day 1.
    if clock.time_text() != "00.00.00" || clock.day_text() != "day 1" {
        failures.push(format!(
            "start state: got {} {}",
            clock.time_text(),
            clock.day_text()
        ));
    }

    // add_sec at 23.59.59 rolls to day 2 00.00.00.
    clock.bump(23 * 3600 + 59 * 60 + 59);
    if clock.time_text() != "23.59.59" || clock.day_text() != "day 1" {
        failures.push(format!(
            "seek to 23.59.59: got {} {}",
            clock.time_text(),
            clock.day_text()
        ));
    }
    clock.add_sec();
    if clock.time_text() != "00.00.00" || clock.day_text() != "day 2" {
        failures.push(format!(
            "midnight rollover: got {} {}",
            clock.time_text(),
            clock.day_text()
        ));
    }

    // add_min at minute 59 wraps the hour without touching the day.
    clock.reset();
    clock.bump(1 * 3600 + 59 * 60 + 30); // 01.59.30, day 1
    clock.add_min(); // 02.00.30
    if clock.time_text() != "02.00.30" || clock.day_text() != "day 1" {
        failures.push(format!(
            "minute wrap: got {} {}",
            clock.time_text(),
            clock.day_text()
        ));
    }

    // add_hour at hour 23 wraps the day.
    clock.reset();
    clock.bump(23 * 3600 + 15 * 60); // 23.15.00, day 1
    clock.add_hour(); // 00.15.00, day 2
    if clock.time_text() != "00.15.00" || clock.day_text() != "day 2" {
        failures.push(format!(
            "hour wrap: got {} {}",
            clock.time_text(),
            clock.day_text()
        ));
    }

    // 86400 consecutive add_sec = exactly one day, no drift.
    clock.reset();
    for _ in 0..SECONDS_PER_DAY {
        clock.add_sec();
    }
    if clock.seconds_since_midnight != 0 || clock.day != 2 {
        failures.push(format!(
            "day sweep drift: seconds={}, day={}",
            clock.seconds_since_midnight, clock.day
        ));
    }
    if clock.time_text() != "00.00.00" || clock.day_text() != "day 2" {
        failures.push(format!(
            "day sweep display: got {} {}",
            clock.time_text(),
            clock.day_text()
        ));
    }

    // reset returns to start.
    clock.reset();
    if clock.time_text() != "00.00.00" || clock.day_text() != "day 1" {
        failures.push(format!(
            "reset: got {} {}",
            clock.time_text(),
            clock.day_text()
        ));
    }

    // Button wiring drives the same state machine.
    press(&mut clock, "btn_s_add");
    if clock.time_text() != "00.00.01" {
        failures.push(format!("btn_s_add: got {}", clock.time_text()));
    }
    press(&mut clock, "btn_m_add");
    if clock.time_text() != "00.01.01" {
        failures.push(format!("btn_m_add: got {}", clock.time_text()));
    }
    press(&mut clock, "btn_h_add");
    if clock.time_text() != "01.01.01" {
        failures.push(format!("btn_h_add: got {}", clock.time_text()));
    }
    press(&mut clock, "btn_reset");
    if clock.time_text() != "00.00.00" || clock.day_text() != "day 1" {
        failures.push("btn_reset did not reset".to_string());
    }

    // Scene geometry sanity: extent (100,100) points laid out at 4 px/pt.
    if let Some(win) = frame.find("time_panel") {
        if win.rect.width != 90_000 {
            failures.push(format!(
                "time_panel extent: expected 90000, got {}",
                win.rect.width
            ));
        }
    }

    if failures.is_empty() {
        println!("clock: ALL CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("CLOCK FAILURE: {f}");
        }
        std::process::exit(1);
    }
}

/// Live mode: window, button hit-testing, display override, and an
/// optional one-second tick driven by `Instant` elapsed time (headless
/// mode above never touches this path, so it stays deterministic).
fn live_loop(store: &mut SceneStore) {
    let mut win = match goya_wayland::GoyaWindow::create("goya-clock", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("goya-clock: live unavailable: {e}");
            return;
        }
    };

    let mut clock = Clock::new();
    let mut dirty = true;
    let mut tick_instant = std::time::Instant::now();
    let mut frames = 0u32;
    let mut presents = 0u32;

    while !win.close_requested() {
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let frame = layout(store.get(), SCENE_W_PT, SCENE_H_PT);
                    if let Some(name) = hit_test(
                        &frame,
                        (x as i64) * goya_wayland::MP_PER_PX,
                        (y as i64) * goya_wayland::MP_PER_PX,
                    ) {
                        if name.starts_with("btn_") {
                            press(&mut clock, &name);
                            dirty = true;
                        }
                    }
                }
                goya_wayland::PlatformEvent::Resized(w, h) => {
                    println!("goya-clock: resized {w}x{h}");
                    dirty = true;
                }
                _ => {}
            }
        }

        // Tick once per second from elapsed wall time (live only).
        if tick_instant.elapsed() >= std::time::Duration::from_secs(1) {
            tick_instant -= std::time::Duration::from_secs(1);
            clock.add_sec();
            dirty = true;
        }

        if dirty {
            let nodes = layout(store.get(), SCENE_W_PT, SCENE_H_PT).nodes;
            let nodes = with_display(nodes, &clock.time_text(), &clock.day_text());
            let (prims, fills) = present_nodes(&nodes);
            let _ = win.present(&prims, &fills, [235, 235, 240], [16, 16, 24], 4);
            presents += 1;
            dirty = false;
        }
        win.pump(1);
        frames += 1;
    }

    println!(
        "goya-clock live: {frames} frames, {presents} presents, final display '{} {}', day {}",
        clock.time_text(),
        clock.day_text(),
        clock.day
    );
}
