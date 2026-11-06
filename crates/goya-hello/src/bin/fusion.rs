//! goya-fusion — the NSB §10 fused milestone.
//!
//! One object that is simultaneously: movable (drag or click retargets its
//! NSB bridge), recolourable (chromatic polarity flip with brand-locked
//! accent), and sonically spatialised (ARSEM pan follows the *presented*
//! position, not the target). RPI prepares the likely next target and the
//! speculation cache commits only the authorised action.
//!
//! `cargo run --bin fusion` runs programmatic acceptance checks.
//! `--live` adds the interactive window: click the object to send it to the
//! other side (mid-flight clicks retarget without snapping), click the theme
//! chip to flip polarity, drag the object to move it continuously.

use goya_bridge::Bridge;
use goya_bridge_physics::{BridgePolicy, Mode};
use goya_chroma::{
    resolve, srgb_to_oklch, Harmony, Polarity, PropertyRelation, Rgb8, Theme, ThemeRole, WcagLevel,
};
use goya_layout::Rect;
use goya_rpi::{ActionId, ContextTag, ProbabilityField};
use goya_spatial_audio::{pan_from_rect, StereoPan};
use goya_speculation::{commit, Cache, CommitDecision};

const WIN_W: i64 = 800_000;
const WIN_H: i64 = 300_000;
const OBJ_W: i64 = 60_000;
const OBJ_H: i64 = 40_000;
const LEFT_X: i64 = 40_000;
const RIGHT_X: i64 = WIN_W - OBJ_W - 40_000;
const THEME_X: i64 = WIN_W / 2 - 45_000;
const THEME_W: i64 = 90_000;
const THEME_H: i64 = 30_000;
const OBJ_Y: i64 = 100_000;
const THEME_Y: i64 = 200_000;

const VIEWPORT: Rect = Rect {
    x: 0,
    y: 0,
    width: WIN_W,
    height: WIN_H,
};

/// Polarity-neutral declaration: the resolver applies the polarity.
fn theme() -> Theme {
    let anchor = srgb_to_oklch(Rgb8::new(0x32, 0x67, 0xC8));
    Theme {
        anchor,
        harmony: Harmony::NeutralDominant,
        surface: ThemeRole {
            lightness: PropertyRelation::Polar(0.06),
            chroma: PropertyRelation::Scale(0.85),
            ..ThemeRole::default()
        },
        foreground: ThemeRole {
            lightness: PropertyRelation::Polar(-0.55),
            contrast: Some(WcagLevel::AA),
            ..ThemeRole::default()
        },
        accent: ThemeRole {
            lightness: PropertyRelation::Absolute,
            brand_lock: true,
            ..ThemeRole::default()
        },
        border: ThemeRole {
            lightness: PropertyRelation::Polar(0.10),
            opacity: 0.65,
            ..ThemeRole::default()
        },
    }
}

struct FusionState {
    bridge: Bridge<f64>,
    policy: BridgePolicy,
    polarity: Polarity,
    pan: StereoPan,
    field: ProbabilityField,
    cache: Cache,
    geom_rev: u64,
}

impl FusionState {
    fn new() -> Self {
        let mut field = ProbabilityField::default();
        // Level-1 priors from declared structure: the object mostly alternates
        // sides (action 1) and occasionally the theme flips (action 2).
        let ctx = ContextTag("object".into());
        for _ in 0..7 {
            field.observe(&ctx, ActionId(1));
        }
        for _ in 0..3 {
            field.observe(&ctx, ActionId(2));
        }
        FusionState {
            bridge: Bridge::new(LEFT_X as f64),
            policy: BridgePolicy::new(Mode::Spring, 0.05),
            polarity: Polarity::Dark,
            pan: StereoPan::new(),
            field,
            cache: Cache::default(),
            geom_rev: 1,
        }
    }

    fn presented_rect(&self) -> Rect {
        Rect {
            x: self.bridge.current() as i64,
            y: OBJ_Y,
            width: OBJ_W,
            height: OBJ_H,
        }
    }
}

fn check(failures: &mut Vec<String>, ok: bool, label: &str) {
    println!("{label} [{}]", if ok { "PASS" } else { "FAIL" });
    if !ok {
        failures.push(label.to_string());
    }
}

fn main() {
    let live = std::env::args().any(|a| a == "--live");
    let mut failures = Vec::new();
    let mut st = FusionState::new();

    // 1. RPI: relativistic conditional field ranks the side-flip as likely.
    let ctx = ContextTag("object".into());
    let p_flip = st.field.probability(&ctx, ActionId(1));
    check(
        &mut failures,
        (p_flip - 0.7).abs() < 1e-9,
        &format!("RPI P(flip|object) = {p_flip:.2} (expect 0.70)"),
    );

    // 2. NSB: retarget mid-flight keeps continuity (no presentation snap).
    st.bridge.set_target(RIGHT_X as f64);
    for _ in 0..24 {
        st.bridge.advance_dt(1.0 / 60.0, &st.policy);
    }
    let mid_x = st.bridge.current();
    let moved = (mid_x - LEFT_X as f64).abs() > (RIGHT_X - LEFT_X) as f64 * 0.1;
    check(
        &mut failures,
        moved,
        "object genuinely in flight before retarget",
    );
    st.bridge.set_target(LEFT_X as f64);
    st.bridge.advance_dt(1.0 / 240.0, &st.policy);
    let after = st.bridge.current();
    check(
        &mut failures,
        (after - mid_x).abs() < (RIGHT_X - LEFT_X) as f64 * 0.1,
        "retarget preserves presentation position (no snap)",
    );

    // 3. ARSEM: pan follows the PRESENTED position while in flight.
    let pan_now = pan_from_rect(st.presented_rect(), VIEWPORT);
    let pan_target = pan_from_rect(
        Rect {
            x: LEFT_X,
            y: OBJ_Y,
            width: OBJ_W,
            height: OBJ_H,
        },
        VIEWPORT,
    );
    check(
        &mut failures,
        (pan_now - pan_target).abs() > 0.05,
        &format!(
            "pan tracks presentation, not target (Δ {:.3})",
            (pan_now - pan_target).abs()
        ),
    );
    st.pan.set_target(pan_now);
    let (gl, _gr) = st.pan.step(0.3);
    check(
        &mut failures,
        gl.is_finite() && gl.abs() <= 1.0,
        "smoothed gains in range",
    );

    // 4. Chromatic polarity flip inverts surface, preserves brand accent hue.
    match (
        resolve(&theme(), Polarity::Dark).ok(),
        resolve(&theme(), Polarity::Light).ok(),
    ) {
        (Some(dark), Some(light)) => {
            check(
                &mut failures,
                (dark.accent.h - light.accent.h).abs() <= 1.0,
                "brand-locked accent hue preserved across polarity",
            );
            check(
                &mut failures,
                dark.surface_rgb != light.surface_rgb,
                "surface actually changes across polarity",
            );
        }
        _ => failures.push("theme resolution failed".to_string()),
    }

    // 5. Speculation: only the authorised action commits; stale rejected.
    st.cache.insert(goya_rpi::Preparation {
        action: ActionId(1),
        state_revision: 1,
        geometry_revision: st.geom_rev,
        payload_hash: 42,
    });
    let fresh = commit(&st.cache, ActionId(1), 1, st.geom_rev);
    check(
        &mut failures,
        matches!(fresh, CommitDecision::Commit(_)),
        "valid prepared outcome committed",
    );
    let stale = commit(&st.cache, ActionId(1), 1, st.geom_rev + 1);
    check(
        &mut failures,
        matches!(stale, CommitDecision::Reject),
        "stale preparation rejected",
    );

    if failures.is_empty() {
        println!("goya-fusion: ALL FUSED-MILESTONE CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("FUSED FAILURE: {f}");
        }
        std::process::exit(1);
    }

    if live {
        if let Err(e) = live_loop() {
            eprintln!("goya-fusion live: {e}");
        }
    }
}

fn live_loop() -> Result<(), String> {
    let mut win = goya_wayland::GoyaWindow::create("goya-fusion", 400, 150)
        .map_err(|e| format!("live window unavailable: {e}"))?;
    let mut st = FusionState::new();
    let mut last = std::time::Instant::now();
    let mut frames = 0u32;
    let mut dragging = false;
    while frames < 1800 && !win.close_requested() {
        let now = std::time::Instant::now();
        let dt = now.duration_since(last).as_secs_f64().min(0.1);
        last = now;
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let mx = (x as i64) * goya_wayland::MP_PER_PX;
                    let my = (y as i64) * goya_wayland::MP_PER_PX;
                    let obj = st.presented_rect();
                    if hit(mx, my, obj) {
                        dragging = true;
                        // Click sends the object to the other side; mid-flight
                        // clicks retarget without snapping (interruption).
                        let going_right = st.bridge.current() < (WIN_W / 2) as f64;
                        st.bridge.set_target(if going_right {
                            RIGHT_X as f64
                        } else {
                            LEFT_X as f64
                        });
                        st.geom_rev += 1;
                    } else if hit(
                        mx,
                        my,
                        Rect {
                            x: THEME_X,
                            y: THEME_Y,
                            width: THEME_W,
                            height: THEME_H,
                        },
                    ) {
                        st.polarity = match st.polarity {
                            Polarity::Dark => Polarity::Light,
                            Polarity::Light => Polarity::Dark,
                        };
                    }
                }
                goya_wayland::PlatformEvent::PointerRelease(..) => dragging = false,
                goya_wayland::PlatformEvent::PointerMove(x, _) if dragging => {
                    let mx = (x as i64) * goya_wayland::MP_PER_PX;
                    st.bridge.set_target(
                        (mx as f64 - (OBJ_W / 2) as f64).clamp(LEFT_X as f64, RIGHT_X as f64),
                    );
                }
                _ => {}
            }
        }
        st.bridge.advance_dt(dt, &st.policy);
        let presented = st.presented_rect();
        st.pan.set_target(pan_from_rect(presented, VIEWPORT));
        let _ = st.pan.step(0.2);
        let prims = vec![
            goya_runtime::Primitive::Panel(Rect {
                x: 0,
                y: 0,
                width: WIN_W,
                height: WIN_H,
            }),
            goya_runtime::Primitive::Button(presented, "Goya".to_string()),
            goya_runtime::Primitive::Button(
                Rect {
                    x: THEME_X,
                    y: THEME_Y,
                    width: THEME_W,
                    height: THEME_H,
                },
                match st.polarity {
                    Polarity::Dark => "Light mode".to_string(),
                    Polarity::Light => "Dark mode".to_string(),
                },
            ),
        ];
        let bg = match st.polarity {
            Polarity::Dark => [24, 24, 28],
            Polarity::Light => [232, 232, 236],
        };
        let fg = match st.polarity {
            Polarity::Dark => [235, 235, 240],
            Polarity::Light => [30, 30, 36],
        };
        let _ = win.present(&prims, &[], &[], fg, bg, 2);
        win.pump(1);
        frames += 1;
    }
    println!("goya-fusion live: {frames} frames");
    Ok(())
}

fn hit(mx: i64, my: i64, r: Rect) -> bool {
    mx >= r.x && mx < r.x + r.width && my >= r.y && my < r.y + r.height
}
