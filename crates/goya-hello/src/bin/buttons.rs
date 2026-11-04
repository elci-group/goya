//! goya-buttons — GY-SON-001 §7 first ARSEM deliverable.
//!
//! A window with three identical buttons (left, centre, right) producing the
//! same tap sound, spatialised by resolved geometry — plus a drag button
//! whose acoustic position follows the object. Verifies the directive's
//! acceptance table programmatically: pan sign per button position, mono
//! fold-down keeps information audible, missing audio device keeps the app
//! operational (NullOutput sink), high-frequency events rate-limited.

use goya_audio_dsp::{limit, Limiter, Mixer, Tap, Voice};
use goya_audio_linux::{AudioOutput, NullOutput};
use goya_ir::parse;
use goya_layout::Constraints;
use goya_runtime::{Event, Runtime};
use goya_sonic::{ObjectId, PolicyEngine, SonicEvent, SonicEventKind, SonicPriority, SoundId};
use goya_spatial_audio::{distance_gain, fold_mono, pan_from_rect, stereo_gains, StereoPan};
use goya_state::Store;

const WIN_W: i64 = 800_000; // millipoints
const WIN_H: i64 = 300_000;

// Row layout math (GY-LAYOUT/1): glyph width 600 mp, container padding
// 8000 mp, child spacing 4000 mp. Spacers 7/8/9 position the three buttons
// at 25% / 50% / 75% of the 800 000 mp viewport.
const MODULE_SRC: &str = "\
# goya-buttons: ARSEM three-button spatial proof
column 1 2 3
row 3 7 4 8 5 9 6
button 4 1 Left
button 5 2 Centre
button 6 3 Right
spacer 7 178.0
spacer 8 193.0
spacer 9 192.7
button 2 4 Drag me
root 1
";

fn main() {
    let live = std::env::args().any(|a| a == "--live");
    match run(live) {
        Ok(()) => {}
        Err(e) => {
            // Headless Wayland is a graceful no-op per GY-WAYLAND/1; any
            // other failure is a real defect.
            if e.contains("Wayland") || e.contains("display") {
                eprintln!("goya-buttons: {e}");
            } else {
                eprintln!("goya-buttons: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn run(live: bool) -> Result<(), String> {
    let module =
        parse(MODULE_SRC).map_err(|e| format!("module parse failed at line {}", e.line))?;
    let mut rt = Runtime::new(
        module,
        Store::new(),
        Constraints {
            max_width: WIN_W,
            max_height: WIN_H,
        },
    );

    // ARSEM: geometry-derived pan per button, verified against the table.
    let viewport = goya_layout::Rect {
        x: 0,
        y: 0,
        width: WIN_W,
        height: WIN_H,
    };
    let mut failures = Vec::new();
    let mut pans = [0.0f32; 3];
    for (i, (node, name, expect)) in [
        (NodeId(4), "Left", -0.5),
        (NodeId(5), "Centre", 0.0),
        (NodeId(6), "Right", 0.5),
    ]
    .iter()
    .enumerate()
    {
        let rect = rt.frame().rect(*node).ok_or("layout missing button rect")?;
        let p = pan_from_rect(rect, viewport);
        pans[i] = p;
        let ok = match *expect {
            -0.5 => p < -0.5,
            0.0 => p.abs() <= 0.15,
            _ => p > 0.5,
        };
        println!(
            "{name}: pan {p:+.3} gains {:?} [{}]",
            stereo_gains(p),
            if ok { "PASS" } else { "FAIL" }
        );
        if !ok {
            failures.push(format!("{name} pan {p} outside expectation"));
        }
    }

    // Distance: a panel two layers down is quieter than the foreground.
    let fg = distance_gain(0, 2);
    let bg = distance_gain(5, 2);
    println!(
        "depth: foreground {fg:.2} > background {bg:.2} [{}]",
        if fg > bg { "PASS" } else { "FAIL" }
    );
    if fg <= bg {
        failures.push("distance ordering inverted".into());
    }

    // Mono fold-down: information remains audible (midpoint of the pair).
    let mono = fold_mono(stereo_gains(pans[0]));
    println!(
        "mono fold-down of left button: {mono:.3} (audible midpoint) [{}]",
        if mono > 0.3 { "PASS" } else { "FAIL" }
    );
    if mono <= 0.3 {
        failures.push("mono fold-down too quiet".into());
    }

    // Rate limiting: 100 rapid events on one source, limit 30/s.
    let mut engine = PolicyEngine::default();
    let mut admitted = 0;
    for i in 0..100u64 {
        let ev = SonicEvent {
            source: ObjectId("btn-left".into()),
            kind: SonicEventKind::Press,
            geometry_revision: 1,
            timestamp_ns: i, // same instant
            sound: SoundId("tap".into()),
            priority: SonicPriority::Normal,
        };
        if engine.admit(&ev) {
            admitted += 1;
        }
    }
    println!(
        "rate limit: {admitted}/100 admitted [{}]",
        if admitted == 30 { "PASS" } else { "FAIL" }
    );
    if admitted != 30 {
        failures.push(format!("rate limiter admitted {admitted}"));
    }

    // Audio sink: NullOutput proves the missing-device path stays operational.
    let mut out = NullOutput::default();
    let mut mixer = Mixer::new(4);
    let mut voices = vec![Voice {
        active: true,
        gain: 0.7,
        priority: 1,
        phase: 0.0,
        tap: Tap::new(880.0, 48_000.0, 0.03),
    }];
    let mut limiter = Limiter::new();
    let mut wrote = 0usize;
    for _ in 0..1440 {
        voices[0].phase += 1.0 / 48_000.0;
        let s = limit(mixer.mix(&mut voices), 0.9);
        let _ = limiter.push(s);
        wrote += out.write(&[limiter.push(s)]).unwrap_or(0);
    }
    println!(
        "audio sink: {wrote} samples through NullOutput [{}]",
        if wrote == 1440 { "PASS" } else { "FAIL" }
    );
    if wrote != 1440 {
        failures.push("audio sink dropped samples".into());
    }

    // Drag trajectory: pan follows the object smoothly as its rect moves.
    let drag_rect = rt
        .frame()
        .rect(NodeId(2))
        .ok_or("layout missing drag rect")?;
    let mut pan = StereoPan::new();
    let mut prev = pan.step(0.0); // seed with StereoPan's initial gains
    let mut max_jump = 0.0f32;
    for step in 0..20 {
        let moved = goya_layout::Rect {
            x: drag_rect.x + i64::from(step) * (WIN_W / 20),
            y: drag_rect.y,
            width: drag_rect.width,
            height: drag_rect.height,
        };
        pan.set_target(pan_from_rect(moved, viewport));
        let (gl, gr) = pan.step(0.3);
        max_jump = max_jump.max((gl - prev.0).abs());
        prev = (gl, gr);
    }
    println!(
        "drag trajectory: smooth (max step delta {max_jump:.3}) [{}]",
        if max_jump < 0.25 { "PASS" } else { "FAIL" }
    );
    if max_jump >= 0.25 {
        failures.push("trajectory jumped (click risk)".into());
    }

    if failures.is_empty() {
        println!("goya-buttons: ALL ACCEPTANCE CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("ACCEPTANCE FAILURE: {f}");
        }
        return Err("acceptance failures".into());
    }

    // Optional live window: same runtime, real pointer events into hit_test.
    if live {
        live_loop(&mut rt)?;
    }
    Ok(())
}

fn live_loop(rt: &mut Runtime) -> Result<(), String> {
    let mut win = goya_wayland::GoyaWindow::create("goya-buttons", 400, 300)
        .map_err(|e| format!("live window unavailable: {e} (Wayland host layer)"))?;
    let mut frames = 0u32;
    while frames < 900 && !win.close_requested() {
        for ev in win.poll() {
            if let goya_wayland::PlatformEvent::PointerPress(x, y) = ev {
                let mx = (x as i64) * goya_wayland::MP_PER_PX;
                let my = (y as i64) * goya_wayland::MP_PER_PX;
                if let Some(action) = rt.hit_test(mx, my) {
                    let _ = rt.handle(Event::Activate(action));
                }
            }
        }
        let _ = win.present(&rt.render_list(), [235, 235, 240], [24, 24, 28], 2);
        win.pump(1);
        frames += 1;
    }
    println!("goya-buttons live: {frames} frames");
    Ok(())
}

use goya_ir::NodeId;
