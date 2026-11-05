//! zcode-monitor — GY-ZCODE/1 host binary.
//!
//! Read-only native Goya window mirroring a live ZCode agent session.
//! See docs/goya-zcode.md. Modes:
//!   default       headless scene acceptance (PASS/FAIL, non-zero on fail)
//!   --stdin-dump  live protocol run, projection printed to stdout
//!   --live        Wayland window

use goya_lang::{hit_test, layout, DrawNode, SceneStore};
use goya_runtime::Primitive;
use goya_wayland::GoyaWindow;
use goya_zcode::agent::{self};
use goya_zcode::view::{LinkState, ProjEvent, SessionView, TAIL_LINES};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};

const SCENE_PATH: &str = "apps/zcode-monitor.gscene";
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 320;

struct Args {
    cli: String,
    workspace: PathBuf,
    session: Option<String>,
    mode: Mode,
    watch: bool,
    duration: Option<u64>,
}

#[derive(PartialEq)]
enum Mode {
    Headless,
    Dump,
    Live,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().collect();
    let get = |name: &str| {
        argv.iter()
            .position(|a| a == name)
            .and_then(|i| argv.get(i + 1))
            .cloned()
    };
    let mode = if argv.iter().any(|a| a == "--live") {
        Mode::Live
    } else if argv.iter().any(|a| a == "--stdin-dump") {
        Mode::Dump
    } else {
        Mode::Headless
    };
    Args {
        cli: get("--cli").unwrap_or_else(|| {
            std::env::var("ZCODE_CLI").unwrap_or_else(|_| String::from("zcode"))
        }),
        workspace: get("--workspace")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        session: get("--session"),
        mode,
        watch: argv.iter().any(|a| a == "--watch"),
        duration: get("--duration").and_then(|v| v.parse().ok()),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('~');
    out
}

/// Override scene node texts from the view before primitive conversion
/// (calculator `with_result` pattern; no per-frame scene rebuild).
fn apply_view_to_frame(nodes: &mut [DrawNode], view: &SessionView) {
    let rows = view.rows();
    let status = match &view.link {
        LinkState::Connecting => String::from("linking"),
        LinkState::Up => view.status.clone(),
        LinkState::Down(reason) => format!("down: {reason}"),
    };
    let status = truncate(&status, 14);
    let title = truncate(&view.title, 34);
    for node in nodes.iter_mut() {
        match node.name.as_str() {
            "title" => node.text = Some(title.clone()),
            "status" => node.text = Some(status.clone()),
            n if n.starts_with("row_") => {
                if let Ok(i) = n[4..].parse::<usize>() {
                    node.text = Some(rows.get(i).cloned().unwrap_or_default());
                }
            }
            _ => {}
        }
    }
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

fn main() {
    let args = parse_args();
    let source = std::fs::read_to_string(SCENE_PATH).unwrap_or_else(|e| {
        eprintln!(
            "zcode-monitor: cannot read {SCENE_PATH}: {e} (run from the goya workspace root)"
        );
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("zcode-monitor: {e}");
            std::process::exit(1);
        }
    };
    let mut last_mtime = std::fs::metadata(SCENE_PATH)
        .ok()
        .and_then(|m| m.modified().ok());

    match args.mode {
        Mode::Headless => headless_acceptance(&store),
        Mode::Dump => run_dump(&args, &mut store, &mut last_mtime),
        Mode::Live => run_live(&args, &mut store, &mut last_mtime),
    }
}

/// Headless acceptance (same shape as goya-scene): scene structure and a
/// projection round-trip, PASS/FAIL per check.
fn headless_acceptance(store: &SceneStore) {
    let mut ok = true;

    // Scene contract: header cells + all TAIL_LINES rows + quit button.
    let frame = layout(store.get(), 400.0, 320.0);
    for name in ["title", "status", "quit"] {
        let found = frame.find(name).is_some();
        println!("scene has '{name}' [{pass}]", pass = pass_str(found));
        ok &= found;
    }
    let rows_found = frame
        .nodes
        .iter()
        .filter(|n| n.name.starts_with("row_"))
        .count();
    let rows_ok = rows_found == TAIL_LINES;
    println!(
        "scene has {rows_found}/{TAIL_LINES} transcript rows [{}]",
        pass_str(rows_ok)
    );
    ok &= rows_ok;

    // Projection contract: feed a synthetic turn through the view.
    let mut view = SessionView::default();
    view.apply(&ProjEvent::TurnStarted {
        input: "acceptance input".into(),
    });
    view.apply(&ProjEvent::PartDelta {
        delta: "streamed response".into(),
    });
    view.apply(&ProjEvent::TurnCompleted {
        response: "final answer".into(),
        result: "success".into(),
    });
    let rows = view.rows();
    let proj_ok = view.status == "idle"
        && rows.last().map(String::as_str) == Some("final answer")
        && rows.contains(&"> acceptance input".to_string());
    println!("projection turn lifecycle [{}]", pass_str(proj_ok));
    ok &= proj_ok;

    // Hit test on the quit button (bottom-right).
    let frame = layout(store.get(), 400.0, 320.0);
    let hit_ok = hit_test(&frame, 340_000, 304_000).as_deref() == Some("quit");
    println!("hit test on 'quit' button [{}]", pass_str(hit_ok));
    ok &= hit_ok;

    if !ok {
        std::process::exit(1);
    }
    println!("zcode-monitor: headless acceptance PASS");
}

fn pass_str(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

fn watch_swap(store: &mut SceneStore, last_mtime: &mut Option<SystemTime>) {
    if let Ok(m) = std::fs::metadata(SCENE_PATH) {
        if let Ok(mtime) = m.modified() {
            if last_mtime.as_ref() != Some(&mtime) {
                *last_mtime = Some(mtime);
                match std::fs::read_to_string(SCENE_PATH).map(|src| store.swap(&src)) {
                    Ok(Ok(gen)) => println!("zcode-monitor: hot-swapped to generation {gen}"),
                    Ok(Err(e)) => println!("zcode-monitor: swap rejected ({e}) — scene retained"),
                    Err(e) => println!("zcode-monitor: read failed ({e}) — retained"),
                }
            }
        }
    }
}

fn run_dump(args: &Args, store: &mut SceneStore, last_mtime: &mut Option<SystemTime>) {
    println!(
        "zcode-monitor: dumping projection (cli={}, workspace={})",
        args.cli,
        args.workspace.display()
    );
    let handle = agent::spawn_agent(&args.cli, &args.workspace, args.session.clone());
    let mut last_rows: Vec<String> = Vec::new();
    let started = Instant::now();
    while !handle.stop.load(Ordering::Relaxed) {
        if args.watch {
            watch_swap(store, last_mtime);
        }
        if handle.take_dirty() {
            if let Ok(view) = handle.view.lock() {
                let view = view.clone();
                let rows = view.rows();
                if rows != last_rows {
                    last_rows = rows.clone();
                    println!(
                        "--- link={:?} status={} title={}",
                        view.link, view.status, view.title
                    );
                    for row in &rows {
                        println!("| {row}");
                    }
                }
            }
        }
        if let Some(secs) = args.duration {
            if started.elapsed() >= Duration::from_secs(secs) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    handle.stop.store(true, Ordering::Relaxed);
    println!("zcode-monitor: dump done");
}

fn run_live(args: &Args, store: &mut SceneStore, last_mtime: &mut Option<SystemTime>) {
    let mut win = match GoyaWindow::create("zcode-monitor", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("zcode-monitor: live unavailable: {e}");
            return;
        }
    };
    let handle = agent::spawn_agent(&args.cli, &args.workspace, args.session.clone());
    let started = Instant::now();
    let mut frames = 0u32;
    while !handle.stop.load(Ordering::Relaxed) && !win.close_requested() {
        if args.watch {
            watch_swap(store, last_mtime);
        }
        let mut dirty = handle.take_dirty();
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let frame = layout(store.get(), 400.0, 320.0);
                    if let Some(name) = hit_test(
                        &frame,
                        (x as i64) * goya_wayland::MP_PER_PX,
                        (y as i64) * goya_wayland::MP_PER_PX,
                    ) {
                        if name == "quit" {
                            handle.stop.store(true, Ordering::Relaxed);
                            dirty = true;
                        }
                    }
                }
                goya_wayland::PlatformEvent::Resized(_, _) => dirty = true,
                _ => {}
            }
        }
        if dirty {
            let frame = layout(store.get(), 400.0, 320.0);
            let mut nodes = frame.nodes.clone();
            if let Ok(view) = handle.view.lock() {
                apply_view_to_frame(&mut nodes, &view);
            }
            let prims: Vec<Primitive> = nodes.iter().map(present_node).collect();
            let _ = win.present(&prims, [20, 20, 28], [225, 228, 236], 3);
        }
        if let Some(secs) = args.duration {
            if started.elapsed() >= Duration::from_secs(secs) {
                break;
            }
        }
        win.pump(1);
        std::thread::sleep(Duration::from_millis(16));
        frames += 1;
    }
    handle.stop.store(true, Ordering::Relaxed);
    println!("zcode-monitor: live ended after {frames} frames");
}
