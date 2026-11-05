//! goya-scene — build a Goya GUI from a RUGID-style scene file.
//!
//! `cargo run --bin scene [--file app.gscene] [--live]`
//!
//! The scene language (GY-LANG/1) adopts RUGID's grammar: `@ontology`,
//! `@cell`, `@button`, `position`/`extent` (absolute points) and
//! `position_p`/`extent_p` (proportional). Buttons are interactive: clicks
//! print their name. Without `--live`, programmatic checks run headless.

use goya_lang::{hit_test, layout, Anchor, DrawNode};
use goya_runtime::Primitive;

const DEFAULT_SCENE: &str = "\
@ontology goya_workspace {
    @cell window {
        position: (0, 0, 0)
        extent: (400, 150, 0)
        fill: #14141c

        @cell title {
            position: (200, 24, 0)
            text: \"Goya Workspace\"
            text-size: 16
            fill: #eaeaef
            text-anchor: middle
        }

        // Three actions, RUGID style — absolute geometry per cell.
        @button open {
            position: (30, 70, 0)
            extent: (100, 40, 0)
            fill: #3267c8
            text: \"Open\"
        }
        @button save {
            position: (150, 70, 0)
            extent: (100, 40, 0)
            fill: #2a8a4a
            text: \"Save\"
        }
        @button quit {
            position: (270, 70, 0)
            extent: (100, 40, 0)
            fill: #8a2a2a
            text: \"Quit\"
        }

        @cell statusbar {
            position: (0, 128, 0)
            extent: (400, 22, 0)
            fill: #1e1e28
            @cell status {
                position_p: (0.5, 0.5, 0)
                text: \"ready\"
                text-size: 9
                fill: #9aa0b0
                text-anchor: middle
            }
        }
    }
}
";

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
    let max_frames: u32 = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1800);
    let sound = args.iter().any(|a| a == "--sound");
    let path = args
        .iter()
        .position(|a| a == "--file")
        .and_then(|i| args.get(i + 1));

    let source = match path {
        Some(p) => std::fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("goya-scene: cannot read {p}: {e}");
            std::process::exit(1);
        }),
        None => DEFAULT_SCENE.to_string(),
    };

    let mut store = match goya_lang::SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-scene: {e}");
            std::process::exit(1);
        }
    };
    let mut last_mtime = path
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| modified(&m));
    println!(
        "goya-scene: ontology '{}' parsed ({} nodes drawn at 400x150)",
        store.get().name,
        layout(store.get(), 400.0, 150.0).nodes.len()
    );

    if !live {
        // Headless verification: proportional child centred in statusbar.
        let frame = layout(store.get(), 400.0, 150.0);
        let (status, statusbar) = match (frame.find("status"), frame.find("statusbar")) {
            (Some(s), Some(b)) => (s, b),
            _ => {
                eprintln!("goya-scene: scene is missing status/statusbar cells");
                std::process::exit(1);
            }
        };
        // position_p 0.5 places the cell's ORIGIN at the parent midpoint;
        // with anchor=middle the text centres there. The origin check is
        // against parent.x + 0.5*parent.width.
        let expected_x = statusbar.rect.x + statusbar.rect.width / 2;
        let ok = (status.rect.x - expected_x).abs() < 5_000;
        println!(
            "proportional centring of status cell [{}]",
            if ok { "PASS" } else { "FAIL" }
        );
        let hit = hit_test(&frame, 80_000, 90_000);
        println!(
            "hit test on 'Open' button [{}]",
            if hit.as_deref() == Some("open") {
                "PASS"
            } else {
                "FAIL"
            }
        );
        if !(ok && hit.as_deref() == Some("open")) {
            std::process::exit(1);
        }
        return;
    }

    let mut win = match goya_wayland::GoyaWindow::create("goya-scene", 400, 150) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("goya-scene: live unavailable: {e}");
            return;
        }
    };
    let mut frames = 0u32;
    let mut clicked = std::collections::BTreeMap::new();
    // Interaction state (click counts, keyed by button NAME) survives
    // hot-swaps: the transactional store swaps only the scene, never the
    // application's state. A swap that fails to parse leaves the running
    // scene untouched (transactional hot-swap, GY-LANG/1).
    let mut audio = if sound {
        goya_audio_linux::PwCatOutput::spawn()
    } else {
        None
    };
    while frames < max_frames && !win.close_requested() {
        // Hot-swap: poll the scene file; transactional via SceneStore.
        if watch {
            if let Some(p) = path {
                if let Ok(m) = std::fs::metadata(p) {
                    let mtime = modified(&m);
                    if last_mtime != Some(mtime) {
                        last_mtime = Some(mtime);
                        match std::fs::read_to_string(p) {
                            Ok(src) => match store.swap(&src) {
                                Ok(gen) => {
                                    println!("goya-scene: hot-swapped to generation {gen}")
                                }
                                Err(e) => println!(
                                    "goya-scene: swap rejected ({e}) — running scene retained"
                                ),
                            },
                            Err(e) => println!("goya-scene: read failed ({e}) — retained"),
                        }
                    }
                }
            }
        }
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let frame = layout(store.get(), 400.0, 150.0);
                    if let Some(name) = hit_test(
                        &frame,
                        (x as i64) * goya_wayland::MP_PER_PX,
                        (y as i64) * goya_wayland::MP_PER_PX,
                    ) {
                        println!("goya-scene: button '{name}' activated");
                        *clicked.entry(name).or_insert(0u32) += 1;
                        if let Some(out) = audio.as_mut() {
                            let t = goya_audio_dsp::Tap::new(880.0, 48_000.0, 0.12);
                            let mut limiter = goya_audio_dsp::Limiter::new();
                            let buf: Vec<f32> = (0..5_760)
                                .map(|i| limiter.push(0.5 * t.sample(i as f32 / 48_000.0)))
                                .collect();
                            let _ = goya_audio_linux::AudioOutput::write(out, &buf);
                        }
                    }
                }
                goya_wayland::PlatformEvent::KeyChar(c) => {
                    println!("goya-scene: key '{c}'");
                }
                goya_wayland::PlatformEvent::Resized(w, h) => {
                    println!("goya-scene: resized to {w}x{h} — relayout on next frame");
                }
                _ => {}
            }
        }
        let frame = layout(store.get(), 400.0, 150.0);
        let prims: Vec<Primitive> = frame.nodes.iter().map(present_node).collect();
        let _ = win.present(&prims, &[], [235, 235, 240], [20, 20, 28], 3);
        win.pump(1);
        // Watch cadence: ~60 fps is pointless for file polling; keep the
        // frame budget long enough to catch edits.
        if watch {
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        frames += 1;
    }
    println!(
        "goya-scene live: {frames} frames, generation {}, clicks: {clicked:?}",
        store.generation()
    );
}

/// Convert a scene draw node into the renderer's primitive vocabulary.
fn present_node(n: &DrawNode) -> Primitive {
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
}

// Anchor is part of the language surface; keep the import honest until the
// renderer supports anchored text (Wave 4 typography).
#[allow(unused)]
fn _anchor_check(a: Anchor) -> bool {
    matches!(a, Anchor::Start | Anchor::Middle | Anchor::End)
}
