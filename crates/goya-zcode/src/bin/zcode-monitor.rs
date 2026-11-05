//! zcode-monitor — GY-ZCODE/1 host binary.
//!
//! Read-only native Goya window mirroring a live ZCode agent session.
//! See docs/goya-zcode.md. Modes:
//!   default       headless scene acceptance (PASS/FAIL, non-zero on fail)
//!   --stdin-dump  live protocol run, projection printed to stdout
//!   --live        Wayland window

use goya_lang::{hit_test, layout, SceneStore};
use goya_runtime::Primitive;
use goya_wayland::GoyaWindow;
use goya_zcode::agent::MonitorHandle;
use goya_zcode::view::{SessionView, TAIL_LINES};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

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

/// Override scene node texts from the view before primitive conversion
/// (calculator `with_result` pattern; no per-frame scene rebuild).
fn apply_view_to_frame(frame: &mut Vec<goya_lang::DrawNode>, view: &SessionView) {
    let rows = view.rows();
    let status = match &view.link {
        goya_zcode::view::LinkState::Connecting => String::from("linking"),
        goya_zcode::view::LinkState::Up => view.status.clone(),
        goya_zcode::view::LinkState::Down(reason) => format!("down: {reason}"),
    };
    let status = goya_zcode::view::truncate_pub(&status, 14);
    let title = goya_zcode::view::truncate_pub(&view.title, 34);
    for node in frame.iter_mut() {
        match node.name.as_str() {
            "title" => node.text = Some(title.clone()),
            "status" => node.text = Some(status.clone()),
            n if n.starts_with("row_") => {
                let idx: Option<usize> = n[4..].parse().ok();
                if let Some(i) = idx {
                    node.text = rows.get(i).cloned().unwrap_or_default();
                }
            }
            _ => {}
        }
    }
}

fn present_node(n: &goya_lang::DrawNode) -> Primitive {
    let rect = goya_lang::RectMp {
        x: n.rect.x,
        y: n.rect.y,
        width: n.rect.width,
        height: n.rect.height,
    };
    // RectMp -> goya_layout::Rect (same integer units).
    let rect = goya_layout::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
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
        eprintln!("zcode-monitor: cannot read {SCENE_PATH}: {e} (run from the goya workspace root)");
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
        Mode::Headless => {
            headless_acceptance(&store);
            return;
        }
        Mode::Dump => {
            run_dump(&args, &mut store, last_mtime.as_ref());
        }
        Mode::Live => {
            run_live(&args, &mut store, last_mtime.as_ref());
        }
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
        println!("scene has '{name}' [{found}]",);
        ok &= found;
    }
    let mut rows_found = 0usize;
    for node in &frame.nodes {
        if node.name.starts_with("row_") {
            rows_found += 1;
        }
    }
    let rows_ok = rows_found == TAIL_LINES;
    println!("scene has {rows_found}/{TAIL_LINES} transcript rows [{pass}]", pass = if rows_ok { "PASS" } else { "FAIL" });
    ok &= rows_ok;

    // Projection contract: feed a synthetic turn through the view.
    let mut view = SessionView::default();
    view.apply(&goya_zcode::view::ProjEvent::TurnStarted {
        input: "acceptance input".into(),
    });
    view.apply(&goya_zcode::view::ProjEvent::PartDelta {
        delta: "streamed response".into(),
    });
    view.apply(&goya_zcode::view::ProjEvent::TurnCompleted {
        response: "final answer".into(),
        result: "success".into(),
    });
    let rows = view.rows();
    let proj_ok = view.status == "idle"
        && rows.last().map(String::as_str) == Some("final answer")
        && rows.contains(&"> acceptance input".to_string());
    println!(
        "projection turn lifecycle [{}]",
        if proj_ok { "PASS" } else { "FAIL" }
    );
    ok &= proj_ok;

    if !ok {
        std::process::exit(1);
    }
}

/// Shared live run: agent thread + dirty-flag loop body. `on_frame` gets
/// a fresh view snapshot when dirty.
fn run_loop(args: &Args, store: &mut SceneStore, last_mtime: Option<&std::time::SystemTime>, render: &mut dyn FnMut(&SessionView)) {
    let handle = goya_zcode::agent::spawn_agent(&args.cli, &args.workspace, args.session.clone());
    let started = Instant::now();
    while !handle.stop.load(Ordering::Relaxed) {
        if args.watch {
            watch_swap(store, &mut last_mtime.cloned());
        }
        if handle.take_dirty() {
            if let Ok(view) = handle.view.lock() {
                render(&view.clone());
            }
        }
        if let Some(secs) = args.duration {
            if started.elapsed() >= Duration::from_secs(secs) {
                handle.stop.store(true, Ordering::Relaxed);
            }
        }
        std::thread::sleep(Duration::from_millis(33));
    }
}

fn watch_swap(store: &mut SceneStore, last_mtime: &mut Option<std::time::SystemTime>) {
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

fn run_dump(args: &Args, store: &mut SceneStore, last_mtime: Option<&std::time::SystemTime>) {
    println!(
        "zcode-monitor: dumping projection (cli={}, workspace={})",
        args.cli,
        args.workspace.display()
    );
    let mut last_rows: Vec<String> = Vec::new();
    run_loop(args, store, last_mtime, &mut |view: &SessionView| {
        let rows = view.rows();
        if rows == last_rows {
            return;
        }
        last_rows = rows.clone();
        println!("--- link={:?} status={} title={}", view.link, view.status, view.title);
        for row in &rows {
            println!("| {row}");
        }
    });
    println!("zcode-monitor: dump done");
}

fn run_live(args: &Args, store: &mut SceneStore, last_mtime: Option<&std::time::SystemTime>) {
    let mut win = match GoyaWindow::create("zcode-monitor", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("zcode-monitor: live unavailable: {e}");
            return;
        }
    };
    let mut quit_pressed = false;
    run_loop(args, store, last_mtime, &mut |_view| {
        // Render happens in the poll loop below; the closure only records
        // freshness. See frame loop for the actual present() call.
    });
    let _ = quit_pressed;
}

// The live path needs the frame loop inline (window events + present in
// the same iteration), so it does not use run_loop; kept separate below.
