//! zcode-monitor — GY-ZCODE/1 host binary.
//!
//! Read-only native Goya window mirroring a live ZCode agent session.
//! See docs/goya-zcode.md. Modes:
//!   default        headless scene acceptance (PASS/FAIL, non-zero on fail)
//!   --stdin-dump   live protocol run, projection printed to stdout
//!   --live         Wayland window
//!
//! Wave 3 (GY-ZCODE-003): the composer commits into a send queue drained
//! by the agent thread (`session/send` on the CLI stdin). Sending is
//! armed only by `--allow-send` (default off — no tokens are spent); the
//! transcript scrolls via PageUp/PageDown (and PointerScroll once
//! goya-wayland emits it) while the composer is unfocused.

use goya_lang::{hit_test, layout, DrawNode, SceneStore};
use goya_runtime::Primitive;
use goya_wayland::{GoyaWindow, KeySpecial};
use goya_zcode::agent::{self};
use goya_zcode::composer::Composer;
use goya_zcode::view::{LinkState, ProjEvent, SessionView, SCROLL_STEP, TAIL_LINES};
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
    frames: Option<u32>,
    /// GY-ZCODE-003: arming the wire send path. Default OFF — default
    /// acceptance and normal runs never spend tokens.
    allow_send: bool,
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
    let has_live = argv.iter().any(|a| a == "--live") || argv.iter().any(|a| a == "--frames");
    let has_dump = argv.iter().any(|a| a == "--stdin-dump");
    if has_live && has_dump {
        eprintln!("zcode-monitor: choose either --live/--frames or --stdin-dump");
        std::process::exit(2);
    }
    let parse_positive = |flag: &str| -> Option<u64> {
        if !argv.iter().any(|arg| arg == flag) {
            return None;
        }
        Some(match get(flag).and_then(|raw| raw.parse::<u64>().ok()) {
            Some(n) if n > 0 => n,
            _ => {
                eprintln!("zcode-monitor: {flag} requires a positive integer");
                std::process::exit(2);
            }
        })
    };
    let mode = if has_live {
        Mode::Live
    } else if has_dump {
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
        duration: parse_positive("--duration"),
        frames: parse_positive("--frames").map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
        allow_send: argv.iter().any(|a| a == "--allow-send"),
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

/// GY-ZCODE-002 gate integration point: map `goya_wayland::KeySpecial`
/// onto the composer's `&str` key names (see `composer` module docs).
/// Wave-3: J1 landed `KeySpecial::{PageUp, PageDown}`; they map to
/// "page_up"/"page_down" and `key_special_to_action` routes them to
/// transcript scrolling (never to the composer).
fn special_name(special: KeySpecial) -> &'static str {
    match special {
        KeySpecial::ArrowLeft => "arrow_left",
        KeySpecial::ArrowRight => "arrow_right",
        KeySpecial::ArrowUp => "arrow_up",
        KeySpecial::ArrowDown => "arrow_down",
        KeySpecial::Home => "home",
        KeySpecial::End => "end",
        KeySpecial::Delete => "delete",
        KeySpecial::Escape => "escape",
        KeySpecial::Tab => "tab",
        KeySpecial::PageUp => "page_up",
        KeySpecial::PageDown => "page_down",
    }
}

/// What a special-key name does in the monitor. Composer-scoped names are
/// forwarded to the composer only while it is focused; page keys scroll
/// the transcript history only while it is NOT (arrow keys stay
/// composer-only, per the frozen Wave 3 contract).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KeyAction {
    Composer(&'static str),
    PageUp,
    PageDown,
}

/// Map the `&str` names produced by [`special_name`] onto monitor actions.
/// "page_up"/"page_down" are accepted NOW so the mapping is ready when
/// goya-wayland emits them (J1, wave 3); until that lands the variants do
/// not exist upstream and these entries are a graceful no-op.
fn key_special_to_action(name: &'static str) -> KeyAction {
    match name {
        "page_up" => KeyAction::PageUp,
        "page_down" => KeyAction::PageDown,
        other => KeyAction::Composer(other),
    }
}

/// Enter-commit path (GY-ZCODE-003): the committed line is enqueued for
/// the agent thread, which owns stdin and issues `session/send`. The
/// `[local]` echo is retired — the wire send is the record. Without
/// `--allow-send` the status hint explains the gate and the command is
/// queued with `wire == false` (admission identical, no wire request).
fn send_committed_line(handle: &agent::MonitorHandle, line: &str, allow_send: bool) {
    if !allow_send {
        if let Ok(mut view) = handle.view.lock() {
            view.status = "send disabled — pass --allow-send".to_string();
        }
    }
    handle.send_text(line.to_string(), allow_send);
}

/// Per-frame composer render override: live text with a `│` cursor marker
/// (visible window kept inside the node by `Composer::display`). Missing
/// composer node in the scene is tolerated (pre-Wave-2 parser).
fn apply_composer_to_frame(nodes: &mut [DrawNode], composer: &Composer) {
    if !composer.focused {
        return;
    }
    let text = composer.display();
    for node in nodes.iter_mut() {
        if node.name == "composer" {
            node.text = Some(text.clone());
            break;
        }
    }
}

fn sync_text_input(
    window: &mut goya_wayland::GoyaWindow,
    composer: &Composer,
    scene: &goya_lang::Scene,
) {
    let frame = layout(scene, 400.0, 320.0);
    let rect = frame.find("composer").map(|node| node.rect);
    let Some(rect) = rect.filter(|_| composer.focused) else {
        let _ = window.set_text_input_state(false, "", 0, 0, (0, 0, 0, 0));
        return;
    };
    let x = (rect.x / goya_wayland::MP_PER_PX) as i32;
    let y = (rect.y / goya_wayland::MP_PER_PX) as i32;
    let cursor_x = x.saturating_add((composer.cursor_grapheme() as i32).saturating_mul(6));
    let cursor_byte = composer.buffer.cursor_byte().min(u32::MAX as usize) as u32;
    let _ = window.set_text_input_state(
        true,
        composer.text(),
        cursor_byte,
        cursor_byte,
        (cursor_x, y, 1, 16),
    );
}

/// Feed one key event into the composer; returns the committed line, if
/// any. Page keys while the composer is unfocused scroll the transcript
/// history instead (handled by the caller — they are not composer keys).
fn feed_key(composer: &mut Composer, ev: &goya_wayland::PlatformEvent) -> Option<String> {
    match ev {
        goya_wayland::PlatformEvent::KeyChar(c) => composer.handle_char(*c),
        goya_wayland::PlatformEvent::KeySpecial(s) => {
            match key_special_to_action(special_name(*s)) {
                KeyAction::Composer(name) => composer.handle_special(name),
                // Composer focused: page keys are inert for the view.
                KeyAction::PageUp | KeyAction::PageDown => {}
            }
            None
        }
        goya_wayland::PlatformEvent::ImeBatch { preedit, commit, delete, .. } => {
            composer.handle_ime_batch(preedit.clone(), commit.as_deref(), *delete);
            None
        }
        _ => None,
    }
}

/// Apply an unfocused page-key action to the transcript scrollback.
fn scroll_history(handle: &agent::MonitorHandle, action: KeyAction) -> bool {
    let dir = match action {
        KeyAction::PageUp => -1i32,
        KeyAction::PageDown => 1,
        KeyAction::Composer(_) => return false,
    };
    if let Ok(mut view) = handle.view.lock() {
        if dir < 0 {
            view.scroll_up(SCROLL_STEP);
        } else {
            view.scroll_down(SCROLL_STEP);
        }
    }
    true
}

/// Apply a wheel scroll only while the composer is unfocused. Clamp one event
/// to a page so large or malformed line counts cannot jump arbitrary history.
fn scroll_pointer_history(
    handle: &agent::MonitorHandle,
    lines: i32,
    composer_focused: bool,
) -> bool {
    if composer_focused || lines == 0 {
        return false;
    }
    let count = lines.unsigned_abs().min(SCROLL_STEP as u32) as usize;
    if let Ok(mut view) = handle.view.lock() {
        if lines > 0 {
            view.scroll_up(count);
        } else {
            view.scroll_down(count);
        }
        true
    } else {
        false
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

    // Composer (GY-ZCODE-002): scripted type → backspace → commit run.
    // SKIP when the scene has no composer node (e.g. pre-Wave-2 parser);
    // that is a skip, not a failure, until J1's @input support lands.
    let has_composer = frame.find("composer").is_some();
    if has_composer {
        let mut c = Composer::new();
        c.focused = true;
        for ch in "abx".chars() {
            c.handle_char(ch);
        }
        c.handle_char('\u{8}');
        c.handle_char('c');
        let committed = c.handle_char('\n');
        let comp_ok = committed.as_deref() == Some("abc")
            && c.text.is_empty()
            && c.history.last().map(String::as_str) == Some("abc");
        println!("composer type/backspace/commit [{}]", pass_str(comp_ok));
        ok &= comp_ok;
    } else {
        println!("composer type/backspace/commit [SKIP] (no 'composer' node in scene)");
    }

    // Send-queue contract (GY-ZCODE-003), exercised without touching the
    // wire: admission, busy drop, and the --allow-send gate — all against
    // the public handle API (admission itself is unit-tested in agent.rs).
    let gate_handle = agent::MonitorHandle::new();
    gate_handle.set_active_session(Some("sess_acceptance".to_string()));
    // 1) Gate off: the command still flows but carries wire=false and no
    //    ticket — no session/send is ever issued by the run loop.
    send_committed_line(&gate_handle, "blocked line", false);
    let gate_ok = matches!(
        gate_handle.try_take_cmd(),
        Some(agent::AgentCmd::SendText {
            wire: false,
            ticket: None,
            ..
        })
    ) && gate_handle
        .view
        .lock()
        .map(|v| v.status.contains("--allow-send"))
        .unwrap_or(false);
    // 2) An admitted wire send carries a ticket…
    gate_handle.send_text("line one".to_string(), true);
    let admit_ok = matches!(
        gate_handle.try_take_cmd(),
        Some(agent::AgentCmd::SendText {
            ticket: Some(_),
            ..
        })
    );
    // 3) …and a second send while one is outstanding is dropped as busy.
    gate_handle.send_text("line two".to_string(), true);
    let busy_ok = gate_handle.try_take_cmd().is_none()
        && gate_handle
            .view
            .lock()
            .map(|v| v.status.contains("busy"))
            .unwrap_or(false);
    let send_ok = gate_ok && admit_ok && busy_ok;
    println!(
        "send queue admission/busy-drop/gate [{}]",
        pass_str(send_ok)
    );
    ok &= send_ok;

    // Scrollback contract (GY-ZCODE-003): history window + snap rules.
    let mut view = SessionView::default();
    for i in 0..40 {
        view.push_line(&format!("line {i:02}"));
    }
    view.scroll_up(SCROLL_STEP);
    let scrolled = view.rows();
    let scroll_ok = view.scroll_from_bottom() == SCROLL_STEP
        && scrolled[TAIL_LINES - 1] == "line 25"
        && view.rows().len() == TAIL_LINES;
    view.scroll_down(SCROLL_STEP);
    let live_ok = view.scroll_from_bottom() == 0 && view.rows()[TAIL_LINES - 1] == "line 39";
    println!(
        "scrollback page-up/page-down window [{}]",
        pass_str(scroll_ok && live_ok)
    );
    ok &= scroll_ok && live_ok;

    // Wheel history contract: positive scrolls up, negative scrolls down,
    // the event magnitude is bounded, and a focused composer keeps the wheel.
    let wheel_handle = agent::MonitorHandle::new();
    if let Ok(mut view) = wheel_handle.view.lock() {
        for i in 0..40 {
            view.push_line(&format!("line {i:02}"));
        }
    }
    let wheel_up = scroll_pointer_history(&wheel_handle, i32::MAX, false);
    let wheel_up_ok = wheel_up
        && wheel_handle
            .view
            .lock()
            .map(|view| {
                view.scroll_from_bottom() == SCROLL_STEP && view.rows()[TAIL_LINES - 1] == "line 25"
            })
            .unwrap_or(false);
    let focused_wheel = scroll_pointer_history(&wheel_handle, -5, true);
    let focused_ok = !focused_wheel
        && wheel_handle
            .view
            .lock()
            .map(|view| view.scroll_from_bottom() == SCROLL_STEP)
            .unwrap_or(false);
    let wheel_down = scroll_pointer_history(&wheel_handle, -5, false);
    let wheel_down_ok = wheel_down
        && wheel_handle
            .view
            .lock()
            .map(|view| view.scroll_from_bottom() == SCROLL_STEP - 5)
            .unwrap_or(false);
    let wheel_ok = wheel_up_ok && focused_ok && wheel_down_ok;
    println!(
        "pointer scroll bounded/focus-aware history [{}]",
        pass_str(wheel_ok)
    );
    ok &= wheel_ok;

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
    let mut last_snapshot: Option<(String, Vec<String>)> = None;
    let started = Instant::now();
    while !handle.stop.load(Ordering::Relaxed) {
        if args.watch {
            watch_swap(store, last_mtime);
        }
        if handle.take_dirty() {
            if let Ok(view) = handle.view.lock() {
                let view = view.clone();
                let header = format!(
                    "link={:?} status={} title={}",
                    view.link, view.status, view.title
                );
                let rows = view.rows();
                if last_snapshot.as_ref() != Some(&(header.clone(), rows.clone())) {
                    println!("--- {header}");
                    for row in &rows {
                        println!("| {row}");
                    }
                    last_snapshot = Some((header, rows));
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
    // GY-ZCODE-002: composer focus is host-side state (parse/layout stay
    // deterministic per the frozen Wave 2 contract).
    let mut composer = Composer::new();
    let started = Instant::now();
    let mut frames = 0u32;
    while !handle.stop.load(Ordering::Relaxed)
        && !win.close_requested()
        && args.frames.is_none_or(|limit| frames < limit)
    {
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
                        } else {
                            // Click on the composer focuses it; any other
                            // click unfocuses it.
                            let now_focused = name == "composer";
                            dirty |= composer.focused != now_focused;
                            composer.focused = now_focused;
                            sync_text_input(&mut win, &composer, store.get());
                        }
                    }
                }
                goya_wayland::PlatformEvent::KeyChar(_)
                | goya_wayland::PlatformEvent::KeySpecial(_) => {
                    if composer.focused {
                        // Composer owns the keys: type, and Enter commits.
                        if let Some(line) = feed_key(&mut composer, &ev) {
                            // GY-ZCODE-003: enqueue for the agent thread;
                            // no [local] echo — the wire send is the record.
                            send_committed_line(&handle, &line, args.allow_send);
                        }
                    } else if let goya_wayland::PlatformEvent::KeySpecial(s) = &ev {
                        // Unfocused: page keys scroll the transcript,
                        // arrow keys fall through to the composer arm
                        // (inert while unfocused).
                        scroll_history(&handle, key_special_to_action(special_name(*s)));
                    }
                    sync_text_input(&mut win, &composer, store.get());
                    dirty = true;
                }
                goya_wayland::PlatformEvent::PointerScroll(lines) => {
                    dirty |= scroll_pointer_history(&handle, lines, composer.focused);
                }
                goya_wayland::PlatformEvent::ImeBatch { .. } => {
                    if let goya_wayland::PlatformEvent::ImeBatch { preedit, commit, delete, .. } = ev {
                        composer.handle_ime_batch(preedit, commit.as_deref(), delete);
                    }
                    sync_text_input(&mut win, &composer, store.get());
                    dirty = true;
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
            apply_composer_to_frame(&mut nodes, &composer);
            let prims: Vec<Primitive> = nodes.iter().map(present_node).collect();
            let fills: Vec<Option<u32>> = nodes.iter().map(|node| node.fill).collect();
            let _ = win.present(&prims, &fills, [225, 228, 236], [20, 20, 28], 3);
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
