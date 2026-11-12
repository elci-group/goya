//! zcode-native — GY-ZCODE/1 host binary.
//!
//! Read-only native Goya window mirroring a live ZCode agent session.
//! See docs/goya-zcode-native.md. Modes:
//!   default        headless scene acceptance (PASS/FAIL, non-zero on fail)
//!   --stdin-dump   live protocol run, projection printed to stdout
//!   --live         Wayland window
//!
//! Wave 3 (GY-ZCODE-003): the composer commits into a send queue drained
//! by the agent thread (`session/send` on the CLI stdin). Sending is
//! armed only by `--allow-send` (default off — no tokens are spent); the
//! transcript scrolls via PageUp/PageDown (and PointerScroll once
//! goya-wayland emits it) while the composer is unfocused.
//!
//! Wave 4 (GY-ZCODE-004): ctrl+c/ctrl+v/ctrl+q arrive as
//! `PlatformEvent::Shortcut` — Copy puts the last non-empty transcript row
//! on the clipboard, Paste inserts at the composer cursor, Quit exits. The
//! chroma-derived palette (goya-chroma, AA-enforced) overrides scene fills
//! per frame; header cells use the documented header fill and the status
//! cell maps ok/err/accent by status text.
//!
//! Wave 9 (GY-ZCODE-009): the clipboard is now system-backed — Copy
//! records on the ring AND sets the compositor selection
//! (`set_clipboard_text`); Paste requests the compositor selection
//! (`request_clipboard_text`) and the `ClipboardText` event inserts at the
//! composer cursor, falling back to the ring on `None` (see the
//! None-ambiguity note in the clipboard module docs). Per-node text
//! colours ride the `fgs` slice (status ok/err/accent, tool ✓/✗, markdown
//! headings accent).

use goya_clipboard::PrincipalId;
use goya_lang::{hit_test, DrawNode, Frame, SceneStore};
use goya_runtime::Primitive;
use goya_wayland::{GoyaWindow, KeySpecial, PlatformEvent, Shortcut};
use goya_zcode_native::agent::{self};
use goya_zcode_native::clipboard::{
    plan_copy, resolve_paste, AllowAllPolicy, CopyTarget, MonitorClipboard,
};
use goya_zcode_native::composer::{Composer, InputDelivery};
use goya_zcode_native::layout::{layout_monitor, layout_monitor_with_controls};
use goya_zcode_native::theme::{
    monitor_palette, node_fill_override, override_slices, status_fill_override,
};
use goya_zcode_native::view::{
    LinkState, ProjEvent, SessionEntry, SessionView, ToolState, WorkflowInteraction,
    WorkflowInteractionOption, WorkflowQueueItem, WorkflowState, SCROLL_STEP, TAIL_LINES,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};

const SCENE_PATH: &str = "apps/zcode-native.gscene";
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 320;

struct Args {
    cli: String,
    workspace: PathBuf,
    session: Option<String>,
    host_attachment: Option<PathBuf>,
    a11y_helper: Option<PathBuf>,
    mode: Mode,
    watch: bool,
    duration: Option<u64>,
    frames: Option<u32>,
    /// GY-ZCODE-003: arming the wire send path. Default OFF — default
    /// acceptance and normal runs never spend tokens.
    allow_send: bool,
    /// GY-ZCODE-008: in dump mode, print a one-line-per-node A11yTree
    /// dump on each a11y republish (i.e. per dirty flag).
    a11y_dump: bool,
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
        eprintln!("zcode-native: choose either --live/--frames or --stdin-dump");
        std::process::exit(2);
    }
    let parse_positive = |flag: &str| -> Option<u64> {
        if !argv.iter().any(|arg| arg == flag) {
            return None;
        }
        Some(match get(flag).and_then(|raw| raw.parse::<u64>().ok()) {
            Some(n) if n > 0 => n,
            _ => {
                eprintln!("zcode-native: {flag} requires a positive integer");
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
        host_attachment: get("--host-attachment").map(PathBuf::from),
        a11y_helper: get("--a11y-helper").map(PathBuf::from),
        mode,
        watch: argv.iter().any(|a| a == "--watch"),
        duration: parse_positive("--duration"),
        frames: parse_positive("--frames").map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
        allow_send: argv.iter().any(|a| a == "--allow-send"),
        a11y_dump: argv.iter().any(|a| a == "--a11y-dump"),
    }
}

fn spawn_monitor(args: &Args) -> agent::MonitorHandle {
    if let Some(path) = &args.host_attachment {
        agent::spawn_host_attached_agent(path, args.session.clone())
    } else {
        agent::spawn_agent(&args.cli, &args.workspace, args.session.clone())
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
fn apply_view_to_frame(nodes: &mut [DrawNode], view: &SessionView, visible_rows: usize) {
    let rows = view.rows_for_window(visible_rows);
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
    /// Wave 6 (GY-ZCODE-006): session switching. No `KeySpecial` source
    /// exists yet — ctrl+Left/Right arrive (once J1 lands them) as
    /// `Shortcut::{SessionPrevious, SessionNext}` through the shortcut
    /// path (see `shortcut_switch_delta`). The entries are wired here
    /// either way; until then they are a graceful no-op.
    SessionNext,
    SessionPrevious,
}

/// Map the `&str` names produced by [`special_name`] onto monitor actions.
/// "page_up"/"page_down" are accepted NOW so the mapping is ready when
/// goya-wayland emits them (J1, wave 3); until that lands the variants do
/// not exist upstream and these entries are a graceful no-op.
fn key_special_to_action(name: &'static str) -> KeyAction {
    match name {
        "page_up" => KeyAction::PageUp,
        "page_down" => KeyAction::PageDown,
        // GATE (GY-ZCODE-006): J1 lands Shortcut::{SessionNext,
        // SessionPrevious} on ctrl+Right/Left; these entries stay inert
        // until a key source produces the names.
        "session_next" => KeyAction::SessionNext,
        "session_previous" => KeyAction::SessionPrevious,
        other => KeyAction::Composer(other),
    }
}

/// Wave 6 (GY-ZCODE-006): cycle direction of a session-switch shortcut.
/// J1's `Shortcut::{SessionNext, SessionPrevious}` (ctrl+Right / ctrl+Left)
/// landed mid-wave; the GATE arms are wired.
fn shortcut_switch_delta(shortcut: Shortcut) -> Option<isize> {
    match shortcut {
        Shortcut::SessionNext => Some(1),
        Shortcut::SessionPrevious => Some(-1),
        Shortcut::Copy | Shortcut::Paste | Shortcut::Quit => None,
    }
}

/// Wave 6: session-switch action shared by the (gated) Shortcut path and
/// the key_special mapping. Unfocuses the composer, cycles the sessions
/// selection by `delta`, snapshots the composer (Wave 7 retention — the
/// composer instance lives here in the bin, so the snapshot travels into
/// `MonitorHandle::switch_session`, the single retention owner), and
/// delegates to `MonitorHandle::switch_session`, which retains the
/// outgoing transcript+composer in the LRU, enqueues
/// `AgentCmd::SwitchSession`, resets the transcript and sets the
/// `"<n>/<m> switching session…"` status — all synchronously, so the bin
/// gets instant visual feedback. Returns true when a switch was enqueued.
fn handle_session_switch(
    handle: &agent::MonitorHandle,
    composer: &mut Composer,
    delta: isize,
) -> bool {
    composer.focused = false;
    let target = handle.view.lock().ok().and_then(|mut view| {
        view.cycle_selection(delta)
            .map(|entry| entry.session_id.clone())
    });
    match target {
        Some(session_id) => {
            handle.switch_session(session_id, composer.snapshot());
            true
        }
        None => false,
    }
}

/// Enter-commit path (GY-ZCODE-003): true means the single agent command
/// channel synchronously accepted the line. CLI acknowledgement happens
/// later and remains the agent owner's responsibility.
fn send_committed_line(
    handle: &agent::MonitorHandle,
    line: &str,
    allow_send: bool,
    delivery: InputDelivery,
) -> bool {
    let send_pending = handle.view.lock().ok().is_some_and(|view| {
        view.owner_action_pending()
            && view
                .workflow_controls()
                .iter()
                .any(|control| control.id == "send" && !control.enabled)
    });
    if send_pending {
        return false;
    }
    if !allow_send {
        // Keep the legacy owner path as the single source of the disabled-send
        // status while guaranteeing it cannot write a wire command.
        return handle.send_text(line.to_string(), false);
    }
    let delivery = match delivery {
        InputDelivery::Auto => agent::SendDelivery::Auto,
        InputDelivery::Queue => agent::SendDelivery::Queue,
        InputDelivery::Guide => agent::SendDelivery::Guide,
        InputDelivery::StartNow => agent::SendDelivery::StartNow,
    };
    handle.send_text_with_delivery(line.to_string(), delivery)
}

/// Commit the current composer line only after synchronous enqueue succeeds.
/// 中文说明：旧路径先清空 Composer 再入队，门禁、无会话或队列拒绝时会丢草稿；
/// 失败时恢复同一份完整快照，保留光标、选区及历史草稿状态。
fn submit_composer_line(
    handle: &agent::MonitorHandle,
    composer: &mut Composer,
    allow_send: bool,
) -> bool {
    let before = composer.snapshot();
    let Some(line) = composer.handle_char('\n') else {
        return false;
    };
    let interaction = handle.view.lock().ok().and_then(|view| {
        view.workflow
            .interactions
            .first()
            .filter(|item| {
                item.kind == "userInput" && item.free_text && item.unsupported_reason.is_none()
            })
            .cloned()
    });
    if let Some(interaction) = interaction {
        let answer_pending = handle.view.lock().ok().is_some_and(|view| {
            view.owner_action_pending()
                && view.workflow_controls().iter().any(|control| {
                    matches!(control.id.as_str(), "send" | "question_submit") && !control.enabled
                })
        });
        if answer_pending {
            composer.restore(before);
            return false;
        }
        let accepted = handle.resolve_interaction(
            interaction.interaction_id,
            agent::InteractionAnswer {
                free_text: Some(line),
                ..Default::default()
            },
        );
        if !accepted {
            composer.restore(before);
        }
        return accepted;
    }
    if send_committed_line(handle, &line, allow_send, composer.delivery) {
        true
    } else {
        composer.restore(before);
        false
    }
}

/// All visible controls and keyboard shortcuts converge here. Actions resolve
/// against the current SessionView projection at activation time, so a stale
/// painted item cannot target a different queue entry or interaction.
fn workflow_control_enabled(handle: &agent::MonitorHandle, id: &str) -> bool {
    handle.view.lock().ok().is_some_and(|view| {
        view.workflow_controls()
            .iter()
            .any(|control| control.id == id && control.enabled)
    })
}

fn activate_workflow_control(
    id: &str,
    handle: &agent::MonitorHandle,
    composer: &mut Composer,
    allow_send: bool,
) -> bool {
    if !workflow_control_enabled(handle, id) {
        return false;
    }
    match id {
        "queue_mode" => {
            composer.delivery = if composer.delivery == InputDelivery::Queue {
                InputDelivery::Auto
            } else {
                InputDelivery::Queue
            };
            true
        }
        "guide_mode" => {
            composer.delivery = if composer.delivery == InputDelivery::Guide {
                InputDelivery::Auto
            } else {
                InputDelivery::Guide
            };
            true
        }
        "send" => submit_composer_line(handle, composer, allow_send),
        "queue_auto_drain" => {
            let auto_drain = handle.view.lock().ok().map(|view| view.workflow.auto_drain);
            let Some(auto_drain) = auto_drain else {
                return false;
            };
            handle.set_auto_drain(!auto_drain)
        }
        "permission_allow" | "permission_deny" => {
            let interaction = handle
                .view
                .lock()
                .ok()
                .and_then(|view| view.workflow.interactions.first().cloned())
                .filter(|item| item.unsupported_reason.is_none());
            let Some(interaction) = interaction else {
                return false;
            };
            let option = interaction.options.iter().find(|option| {
                if id == "permission_allow" {
                    matches!(option.kind.as_str(), "allowOnce" | "allowAlways")
                } else {
                    option.kind == "deny"
                }
            });
            let Some(option) = option else { return false };
            let answer_text = if interaction.free_text {
                let text = composer.text().trim();
                (!text.is_empty()).then(|| text.to_owned())
            } else {
                None
            };
            let accepted = handle.resolve_interaction(
                interaction.interaction_id,
                agent::InteractionAnswer {
                    option_id: Some(option.option_id.clone()),
                    free_text: answer_text,
                    ..Default::default()
                },
            );
            if accepted && interaction.free_text {
                composer.buffer.set_text("");
            }
            accepted
        }
        "question_submit" => {
            let was_focused = composer.focused;
            composer.focused = true;
            let accepted = submit_composer_line(handle, composer, allow_send);
            composer.focused = was_focused;
            accepted
        }
        name if name.starts_with("queue_remove_") => {
            let Some(index) = name
                .rsplit('_')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return false;
            };
            let item = handle
                .view
                .lock()
                .ok()
                .and_then(|view| view.workflow.queue_items.get(index).cloned());
            item.map(|item| handle.delete_queue_item(item.queue_item_id))
                .unwrap_or(false)
        }
        name if name.starts_with("permission_option_") => {
            let Some(index) = name
                .rsplit('_')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return false;
            };
            let interaction = handle
                .view
                .lock()
                .ok()
                .and_then(|view| view.workflow.interactions.first().cloned())
                .filter(|item| item.unsupported_reason.is_none());
            let Some(interaction) = interaction.filter(|item| item.kind == "permission") else {
                return false;
            };
            let Some(option) = interaction.options.get(index) else {
                return false;
            };
            handle.resolve_interaction(
                interaction.interaction_id,
                agent::InteractionAnswer {
                    option_id: Some(option.option_id.clone()),
                    free_text: if interaction.free_text && !composer.text().trim().is_empty() {
                        Some(composer.text().trim().to_owned())
                    } else {
                        None
                    },
                    ..Default::default()
                },
            )
        }
        "stop" => handle.stop_session(),
        "goal_pause" => handle.pause_goal(),
        "goal_resume" => handle.resume_goal(),
        "compact_session" => handle.compact_session(),
        name if name.starts_with("queue_now_") => {
            let Some(index) = name
                .rsplit('_')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return false;
            };
            let item = handle
                .view
                .lock()
                .ok()
                .and_then(|view| view.workflow.queue_items.get(index).cloned());
            item.filter(|item| !item.queue_item_id.is_empty())
                .is_some_and(|item| handle.send_queued_now(item.queue_item_id))
        }
        name if name.starts_with("queue_earlier_") => {
            let Some(index) = name
                .rsplit('_')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return false;
            };
            let ids = handle.view.lock().ok().and_then(|view| {
                let item = view.workflow.queue_items.get(index)?;
                let previous = index
                    .checked_sub(1)
                    .and_then(|previous| view.workflow.queue_items.get(previous))?;
                if item.queue_item_id.is_empty() || previous.queue_item_id.is_empty() {
                    return None;
                }
                Some((item.queue_item_id.clone(), previous.queue_item_id.clone()))
            });
            ids.is_some_and(|(queue_item_id, before_queue_item_id)| {
                handle.reorder_queue_item(queue_item_id, before_queue_item_id)
            })
        }
        name if name.starts_with("work_cancel_") => {
            let Some(index) = name
                .rsplit('_')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return false;
            };
            let work = handle
                .view
                .lock()
                .ok()
                .and_then(|view| view.workflow.background_works.get(index).cloned());
            work.filter(|work| !work.work_id.is_empty())
                .is_some_and(|work| handle.cancel_background_work(work.work_id))
        }
        name if name.starts_with("question_option_") => {
            let Some(index) = name
                .rsplit('_')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return false;
            };
            let interaction = handle
                .view
                .lock()
                .ok()
                .and_then(|view| view.workflow.interactions.first().cloned())
                .filter(|item| item.unsupported_reason.is_none());
            let Some(interaction) = interaction.filter(|item| item.kind == "userInput") else {
                return false;
            };
            let Some(option) = interaction.options.get(index) else {
                return false;
            };
            handle.resolve_interaction(
                interaction.interaction_id,
                agent::InteractionAnswer {
                    option_id: Some(option.option_id.clone()),
                    free_text: if interaction.free_text && !composer.text().trim().is_empty() {
                        Some(composer.text().trim().to_owned())
                    } else {
                        None
                    },
                    ..Default::default()
                },
            )
        }
        _ => false,
    }
}

fn enabled_workflow_control(view: &SessionView, id: &'static str) -> Option<&'static str> {
    view.workflow_controls()
        .iter()
        .any(|control| control.id == id && control.enabled)
        .then_some(id)
}

fn keyboard_workflow_control(key: char, view: &SessionView) -> Option<&'static str> {
    match key.to_ascii_lowercase() {
        'q' => enabled_workflow_control(view, "queue_mode"),
        'g' => enabled_workflow_control(view, "guide_mode"),
        'd' => enabled_workflow_control(view, "queue_auto_drain"),
        'y' => enabled_workflow_control(view, "permission_allow"),
        'n' => enabled_workflow_control(view, "permission_deny"),
        's' => enabled_workflow_control(view, "stop"),
        'p' => enabled_workflow_control(view, "goal_pause"),
        'r' => enabled_workflow_control(view, "goal_resume"),
        'c' => enabled_workflow_control(view, "compact_session"),
        'u' => enabled_workflow_control(view, "queue_now_00"),
        '[' => enabled_workflow_control(view, "queue_earlier_00"),
        'x' => enabled_workflow_control(view, "work_cancel_00"),
        '1'..='4' => {
            let index = (key as u8 - b'1') as usize;
            let interaction = view.workflow.interactions.first()?;
            let id = goya_zcode_native::view::answer_control_id(interaction, index)?;
            enabled_workflow_control(view, id)
        }
        _ => None,
    }
}

/// Per-frame composer render override: live text with a `│` cursor marker
/// (visible window kept inside the node by `Composer::display`). Missing
/// composer node in the scene is tolerated (pre-Wave-2 parser).
fn apply_composer_to_frame(nodes: &mut [DrawNode], composer: &Composer) {
    if !composer.focused {
        return;
    }
    for node in nodes.iter_mut() {
        if node.name == "composer" {
            // 中文说明：旧实现固定按 30 个字素截取；输入框随窗口缩放后会多留空白或裁掉光标。
            // 以当前场景框宽度估算可见字素数，并继续由 renderer 按节点矩形裁剪。
            let view_width = (node.rect.width / (6 * goya_wayland::MP_PER_PX)).max(1) as usize;
            let text = composer.display_with_width(view_width);
            node.text = Some(text.clone());
            break;
        }
    }
}

fn workflow_caption(view: &SessionView) -> (String, String) {
    let plan = view.workflow.plan.as_ref().map(|items| {
        items
            .iter()
            .map(|item| item.content.as_str())
            .filter(|content| !content.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    });
    let with_plan = |detail: String| match plan.as_deref() {
        Some(plan) if !plan.is_empty() && !detail.is_empty() => format!("{detail} • {plan}"),
        Some(plan) if !plan.is_empty() => plan.to_owned(),
        _ => detail,
    };
    if let Some(interaction) = view.workflow.interactions.first() {
        let title = interaction.title.clone();
        let detail = if let Some(reason) = &interaction.unsupported_reason {
            format!("Answer in the full ZCode client: {reason}")
        } else if interaction.summary.is_empty() {
            format!(
                "{} • {} pending",
                interaction.kind,
                view.workflow.interactions.len()
            )
        } else {
            format!(
                "{} • {} pending",
                interaction.summary,
                view.workflow.interactions.len()
            )
        };
        return (title, detail);
    }
    if let Some(goal) = view
        .workflow
        .goal
        .as_ref()
        .filter(|goal| !goal.objective.trim().is_empty())
    {
        return (goal.objective.clone(), with_plan(goal.status.clone()));
    }
    if let Some(item) = view.workflow.queue_items.first() {
        return (
            item.text.clone(),
            with_plan(format!(
                "{} queued • auto drain {}",
                view.workflow.queue_items.len(),
                if view.workflow.auto_drain {
                    "on"
                } else {
                    "paused"
                }
            )),
        );
    }
    if let Some(work) = view.workflow.background_works.first() {
        return (work.title.clone(), with_plan(work.status.clone()));
    }
    (String::new(), with_plan(String::new()))
}

fn apply_workflow_to_frame(nodes: &mut [DrawNode], view: &SessionView, composer: &Composer) {
    let interaction = view.workflow.interactions.first();
    let (title, detail) = workflow_caption(view);
    let controls = view.workflow_controls();
    for node in nodes.iter_mut() {
        match node.name.as_str() {
            "workflow_title" => node.text = Some(truncate(&title, 62)),
            "workflow_detail" => node.text = Some(truncate(&detail, 72)),
            "queue_mode" => {
                node.text = Some(
                    if composer.delivery == InputDelivery::Queue {
                        "✓ Queue"
                    } else {
                        "Queue"
                    }
                    .into(),
                )
            }
            "guide_mode" => {
                node.text = Some(
                    if composer.delivery == InputDelivery::Guide {
                        "✓ Guide"
                    } else {
                        "Guide"
                    }
                    .into(),
                )
            }
            "send" => {
                node.text = Some(
                    if interaction.is_some_and(|item| item.kind == "userInput" && item.free_text) {
                        "Answer"
                    } else {
                        "Send"
                    }
                    .into(),
                )
            }
            "queue_auto_drain" => {
                node.text = Some(
                    if view.workflow.auto_drain {
                        "Pause queue"
                    } else {
                        "Resume queue"
                    }
                    .into(),
                )
            }
            name if name.starts_with("queue_remove_") => {
                if let Some(index) = name
                    .rsplit('_')
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                {
                    node.text = view
                        .workflow
                        .queue_items
                        .get(index)
                        .map(|item| truncate(&format!("× {}", item.text), 15));
                }
            }
            "permission_allow" => {
                node.text = interaction.and_then(|item| {
                    item.options
                        .iter()
                        .find(|option| matches!(option.kind.as_str(), "allowOnce" | "allowAlways"))
                        .map(|option| truncate(&option.label, 14))
                })
            }
            "permission_deny" => {
                node.text = interaction.and_then(|item| {
                    item.options
                        .iter()
                        .find(|option| option.kind == "deny")
                        .map(|option| truncate(&option.label, 14))
                })
            }
            name if name.starts_with("permission_option_")
                || name.starts_with("question_option_") =>
            {
                let question = name.starts_with("question_");
                let index = name
                    .rsplit('_')
                    .next()
                    .and_then(|n| n.parse::<usize>().ok());
                node.text = interaction.and_then(|item| {
                    let is_expected = if question {
                        item.kind == "userInput"
                    } else {
                        item.kind == "permission"
                    };
                    if !is_expected {
                        return None;
                    }
                    index
                        .and_then(|i| item.options.get(i))
                        .map(|option| truncate(&option.label, 14))
                });
            }
            "question_submit" => node.text = Some("Submit".into()),
            _ => {
                if let Some(control) = controls.iter().find(|control| control.id == node.name) {
                    node.text = Some(truncate(&control.label, 20));
                }
            }
        }
    }
}

fn workflow_layout(
    store: &SceneStore,
    handle: &agent::MonitorHandle,
    width: i32,
    height: i32,
) -> goya_zcode_native::layout::MonitorLayout {
    let controls = handle
        .view
        .lock()
        .map(|view| view.workflow_controls())
        .unwrap_or_default();
    layout_monitor_with_controls(store.get(), width, height, &controls)
}

fn apply_accessible_action(
    action: goya_zcode_native::accessibility_bridge::AccessibleAction,
    handle: &agent::MonitorHandle,
    composer: &mut Composer,
    allow_send: bool,
) -> bool {
    use goya_zcode_native::accessibility_bridge::AccessibleAction;
    match action {
        AccessibleAction::SetValue { id, value } if id == "composer" => {
            composer.buffer.set_text(&value);
            composer.preedit = None;
            composer.focused = true;
            true
        }
        AccessibleAction::Press { id } if id == "quit" => {
            handle.stop.store(true, Ordering::Relaxed);
            true
        }
        AccessibleAction::Press { id } if id == "session_prev" || id == "session_next" => handle
            .view
            .lock()
            .ok()
            .and_then(|mut view| {
                view.cycle_selection(if id == "session_prev" { -1 } else { 1 })
                    .map(|entry| entry.session_id.clone())
            })
            .map(|session_id| {
                handle.switch_session(session_id, composer.snapshot());
                composer.focused = false;
            })
            .is_some(),
        AccessibleAction::Press { id } if id == "composer" => {
            composer.focused = true;
            true
        }
        AccessibleAction::Press { id } => {
            activate_workflow_control(&id, handle, composer, allow_send)
        }
        AccessibleAction::SetValue { .. } => false,
    }
}

fn sync_text_input(window: &mut goya_wayland::GoyaWindow, composer: &Composer, frame: &Frame) {
    let Some((cursor_byte, anchor_byte, cursor_rect)) = composer
        .focused
        .then(|| text_input_geometry(frame, composer))
        .flatten()
    else {
        let _ = window.set_text_input_state(false, "", 0, 0, (0, 0, 0, 0));
        return;
    };
    let _ =
        window.set_text_input_state(true, composer.text(), cursor_byte, anchor_byte, cursor_rect);
}

/// Calculate the text-input-v3 surrounding offsets and candidate rectangle
/// from the current frame. The decorated/windowed monitor composer has not
/// adopted Goya shaped caret geometry yet; its estimate is clamped to the node.
type TextInputGeometry = (u32, u32, (i32, i32, i32, i32));
fn text_input_geometry(frame: &Frame, composer: &Composer) -> Option<TextInputGeometry> {
    let rect = frame.find("composer")?.rect;
    let x = (rect.x / goya_wayland::MP_PER_PX) as i32;
    let y = (rect.y / goya_wayland::MP_PER_PX) as i32;
    let width = (rect.width / goya_wayland::MP_PER_PX).max(0) as i32;
    let cursor_x = x
        .saturating_add((composer.cursor_grapheme() as i32).saturating_mul(6))
        .clamp(x, x.saturating_add(width.saturating_sub(1).max(0)));
    Some((
        composer.buffer.cursor_byte().min(u32::MAX as usize) as u32,
        composer.buffer.anchor_byte().min(u32::MAX as usize) as u32,
        (
            cursor_x,
            y,
            if width > 0 { 1 } else { 0 },
            (rect.height / goya_wayland::MP_PER_PX).clamp(0, 16) as i32,
        ),
    ))
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
                // Composer focused: page keys and session switches are
                // inert for the view (switching unfocuses first; there is
                // no key source for session names yet — GATE, Wave 6).
                KeyAction::PageUp | KeyAction::PageDown => {}
                KeyAction::SessionNext | KeyAction::SessionPrevious => {}
            }
            None
        }
        goya_wayland::PlatformEvent::KeySpecialWithModifiers { key, modifiers } => {
            match key_special_to_action(special_name(*key)) {
                KeyAction::Composer(name) => {
                    composer.handle_special_with_shift(name, modifiers.shift)
                }
                KeyAction::PageUp | KeyAction::PageDown => {}
                KeyAction::SessionNext | KeyAction::SessionPrevious => {}
            }
            None
        }
        // Native batches are already ordered by `done` in TextInputState:
        // clear preedit, delete surrounding bytes, commit, then new preedit.
        goya_wayland::PlatformEvent::Preedit(text) => {
            composer.set_preedit(text.as_deref());
            None
        }
        goya_wayland::PlatformEvent::PreeditWithCursor {
            text,
            cursor_begin,
            cursor_end,
        } => {
            composer.set_preedit_with_cursor(Some(text.as_str()), *cursor_begin, *cursor_end);
            None
        }
        goya_wayland::PlatformEvent::CommitText(text) => {
            composer.commit_text(text);
            None
        }
        goya_wayland::PlatformEvent::DeleteSurroundingText {
            before_bytes,
            after_bytes,
        } => {
            composer.delete_surrounding(*before_bytes, *after_bytes);
            None
        }
        _ => None,
    }
}

/// Apply an unfocused page-key action to the transcript scrollback.
fn scroll_history(handle: &agent::MonitorHandle, action: KeyAction, visible_rows: usize) -> bool {
    let dir = match action {
        KeyAction::PageUp => -1i32,
        KeyAction::PageDown => 1,
        // GATE (Wave 6): no key source yet; not a scroll action either.
        KeyAction::SessionNext | KeyAction::SessionPrevious => return false,
        KeyAction::Composer(_) => return false,
    };
    if let Ok(mut view) = handle.view.lock() {
        if dir < 0 {
            view.scroll_up(visible_rows.max(1));
        } else {
            view.scroll_down(visible_rows.max(1));
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
    visible_rows: usize,
) -> bool {
    if composer_focused || lines == 0 {
        return false;
    }
    let count = lines
        .unsigned_abs()
        .min(visible_rows.max(1).min(u32::MAX as usize) as u32) as usize;
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
        eprintln!("zcode-native: cannot read {SCENE_PATH}: {e} (run from the goya workspace root)");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("zcode-native: {e}");
            std::process::exit(1);
        }
    };
    let mut last_mtime = std::fs::metadata(SCENE_PATH)
        .ok()
        .and_then(|m| m.modified().ok());
    // Geometry starts at the window's configured size and is refreshed from
    // the same resize-aware frame used by rendering and pointer hit testing.
    let initial_layout = layout_monitor(store.get(), WIN_W_PX, WIN_H_PX);
    let mut a11y = goya_zcode_native::a11y::MonitorA11y::new(&initial_layout.frame);

    match args.mode {
        Mode::Headless => headless_acceptance(&store),
        Mode::Dump => run_dump(&args, &mut store, &mut last_mtime, &mut a11y),
        Mode::Live => {
            if let Err(error) = run_live(&args, &mut store, &mut last_mtime, &mut a11y) {
                eprintln!("zcode-native: {error}");
                std::process::exit(1);
            }
        }
    }
}

/// Headless acceptance (same shape as goya-scene): scene structure and a
/// projection round-trip, PASS/FAIL per check.
fn headless_acceptance(store: &SceneStore) {
    let mut ok = true;

    // Scene contract: header, viewport-sized rows, and native shell controls.
    let monitor_layout = layout_monitor(store.get(), WIN_W_PX, WIN_H_PX);
    let frame = monitor_layout.frame;
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
    let rows_ok = rows_found == monitor_layout.visible_rows;
    println!(
        "scene has {rows_found} viewport transcript rows [{}]",
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
    let frame = layout_monitor(store.get(), WIN_W_PX, WIN_H_PX).frame;
    let hit_ok = hit_test(
        &frame,
        354 * goya_wayland::MP_PER_PX,
        242 * goya_wayland::MP_PER_PX,
    )
    .as_deref()
        == Some("quit");
    println!("hit test on 'quit' button [{}]", pass_str(hit_ok));
    ok &= hit_ok;

    // Resize acceptance: the frame is the one geometry source for generated
    // transcript rows, hit testing, accessibility regions, and IME placement.
    let expanded = layout_monitor(store.get(), 720, 600);
    let compact = layout_monitor(store.get(), 80, 48);
    let zero = layout_monitor(store.get(), 0, 0);
    let extreme = layout_monitor(store.get(), i32::MAX, i32::MAX);
    let resized_quit = expanded.frame.find("quit").unwrap().rect;
    let mut resized_a11y = goya_zcode_native::a11y::MonitorA11y::new(&frame);
    resized_a11y.set_frame(&expanded.frame);
    resized_a11y.republish(&SessionView::default(), &Composer::new());
    let a11y_quit = resized_a11y
        .nodes()
        .iter()
        .find(|node| node.id == "quit")
        .map(|node| node.hit_region);
    let mut ime_composer = Composer::new();
    ime_composer.focused = true;
    ime_composer.commit_text("resize");
    let ime_geometry = text_input_geometry(&expanded.frame, &ime_composer);
    let resize_ok = expanded.visible_rows > TAIL_LINES
        && expanded
            .frame
            .find(&format!("row_{:02}", expanded.visible_rows - 1))
            .is_some()
        && expanded.frame.find("row_00").unwrap().rect.width == 708 * goya_wayland::MP_PER_PX
        && resized_a11y.rects().rows.len() == expanded.visible_rows
        && a11y_quit.is_some_and(|region| {
            region.x == resized_quit.x as i32
                && region.y == resized_quit.y as i32
                && region.width == resized_quit.width as i32
                && region.height == resized_quit.height as i32
        })
        && ime_geometry.is_some_and(|(_, _, (x, y, _, _))| {
            x >= (expanded.frame.find("composer").unwrap().rect.x / goya_wayland::MP_PER_PX) as i32
                && y == (expanded.frame.find("composer").unwrap().rect.y / goya_wayland::MP_PER_PX)
                    as i32
        })
        && compact.visible_rows == 0
        && compact.frame.find("row_00").is_none()
        && zero.visible_rows == 0
        && zero
            .frame
            .nodes
            .iter()
            .all(|node| node.rect.width == 0 && node.rect.height == 0)
        && extreme.visible_rows == 256
        && extreme.frame.find("row_255").is_some()
        && extreme.frame.find("row_256").is_none();
    println!(
        "resize layout, a11y, and IME geometry share current frame [{}]",
        pass_str(resize_ok)
    );
    ok &= resize_ok;

    // Modified-arrow interaction acceptance: selection is kept by the same
    // grapheme-safe TextBuffer offsets that the IME surrounding-text API uses.
    let mut selected = Composer::new();
    selected.focused = true;
    for ch in "abc".chars() {
        selected.handle_char(ch);
    }
    selected.handle_special("home");
    feed_key(
        &mut selected,
        &PlatformEvent::KeySpecialWithModifiers {
            key: KeySpecial::ArrowRight,
            modifiers: goya_wayland::KeyModifiers {
                shift: true,
                ctrl: false,
            },
        },
    );
    let selection_geometry_ok = selected.buffer.selection_bytes() == Some(0..1)
        && text_input_geometry(&expanded.frame, &selected)
            .is_some_and(|(cursor, anchor, _)| cursor != anchor);
    println!(
        "modified-arrow selection reaches IME surrounding state [{}]",
        pass_str(selection_geometry_ok)
    );
    ok &= selection_geometry_ok;

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
            && c.text().is_empty()
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
    // 1) Gate off rejects before enqueue; no accepted command is duplicated
    //    into a second UI queue, and the composer submit helper restores text.
    let gate_rejected =
        !send_committed_line(&gate_handle, "blocked line", false, InputDelivery::Auto);
    let gate_ok = gate_rejected
        && gate_handle.try_take_cmd().is_none()
        && gate_handle
            .view
            .lock()
            .map(|v| v.status.contains("send disabled"))
            .unwrap_or(false);
    // 2) An admitted wire send carries a ticket…
    let admit_ok = gate_handle.send_text("line one".to_string(), true)
        && matches!(
            gate_handle.try_take_cmd(),
            Some(agent::AgentCmd::SendText {
                ticket: Some(_),
                ..
            })
        );
    // 3) …and a second send while one is outstanding is dropped as busy.
    let busy_ok = !gate_handle.send_text("line two".to_string(), true)
        && gate_handle.try_take_cmd().is_none()
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

    // A failed synchronous admission leaves the exact editing snapshot in
    // place for every rejection class (wire gate, no session, pending send).
    let mut rejected_composer = Composer::new();
    rejected_composer.focused = true;
    for ch in "draft".chars() {
        rejected_composer.handle_char(ch);
    }
    rejected_composer.handle_special("arrow_left");
    let before_reject = rejected_composer.snapshot();
    let disabled_rejected = !submit_composer_line(&gate_handle, &mut rejected_composer, false)
        && rejected_composer.snapshot() == before_reject;

    let no_session = agent::MonitorHandle::new();
    let no_session_rejected = !submit_composer_line(&no_session, &mut rejected_composer, true)
        && rejected_composer.snapshot() == before_reject;

    let pending = agent::MonitorHandle::new();
    pending.set_active_session(Some("sess_acceptance".to_string()));
    let pending_accepted = pending.send_text("already pending".to_string(), true);
    let pending_rejected = pending_accepted
        && !submit_composer_line(&pending, &mut rejected_composer, true)
        && rejected_composer.snapshot() == before_reject;
    let draft_ok = disabled_rejected && no_session_rejected && pending_rejected;
    println!(
        "disabled/no-session/pending admission preserves composer draft [{}]",
        pass_str(draft_ok)
    );
    ok &= draft_ok;

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
    let wheel_up = scroll_pointer_history(&wheel_handle, i32::MAX, false, TAIL_LINES);
    let wheel_up_ok = wheel_up
        && wheel_handle
            .view
            .lock()
            .map(|view| {
                view.scroll_from_bottom() == SCROLL_STEP && view.rows()[TAIL_LINES - 1] == "line 25"
            })
            .unwrap_or(false);
    let focused_wheel = scroll_pointer_history(&wheel_handle, -5, true, TAIL_LINES);
    let focused_ok = !focused_wheel
        && wheel_handle
            .view
            .lock()
            .map(|view| view.scroll_from_bottom() == SCROLL_STEP)
            .unwrap_or(false);
    let wheel_down = scroll_pointer_history(&wheel_handle, -5, false, TAIL_LINES);
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

    // Markdown block contract (GY-ZCODE-004): heading + code + bullet
    // sample routes through the view's markdown mode.
    let mut md_view = SessionView::default();
    md_view.push_markdown("# report\n\n```rust\nlet a = 1;\n```\n\n- bullet one");
    let md_rows: Vec<String> = md_view.history.iter().take(6).cloned().collect();
    let md_ok = md_rows.contains(&"■ REPORT".to_string())
        && md_rows.contains(&"| lang: rust".to_string())
        && md_rows.contains(&"|   let a = 1;".to_string())
        && md_rows.contains(&"• bullet one".to_string());
    println!("markdown heading/code/bullet blocks [{}]", pass_str(md_ok));
    ok &= md_ok;

    // Clipboard ring contract (GY-ZCODE-004): wire-free round-trip.
    let clip_policy = AllowAllPolicy;
    let clip = MonitorClipboard::new(&clip_policy);
    let user = PrincipalId(1);
    let clip_ok = clip.copy_line(user, "first line")
        && clip.copy_line(user, "second line")
        && clip.paste_text().as_deref() == Some("second line");
    println!("clipboard copy/paste round-trip [{}]", pass_str(clip_ok));
    ok &= clip_ok;

    // System-clipboard adapter contract (GY-ZCODE-009), compositor-free:
    // the copy plan emits both the ring record and a set-clipboard marker
    // (RingAndSystem) for the last non-empty row; the paste decision
    // prefers the system payload and falls back to the ring on None
    // (the documented ambiguity: absent data-device vs empty selection).
    let sys_rows = vec![String::new(), "row nine".to_string()];
    let copy_plan_ok = plan_copy(&sys_rows, true)
        == Some(CopyTarget::RingAndSystem("row nine".to_string()))
        && plan_copy(&sys_rows, false) == Some(CopyTarget::Ring("row nine".to_string()))
        && plan_copy(&[String::new()], true).is_none();
    let paste_plan_ok = resolve_paste(Some("sys"), Some("ring")) == Some("sys".to_string())
        && resolve_paste(None, Some("ring")) == Some("ring".to_string())
        && resolve_paste(None, None).is_none();
    // Applying the plan through the real ring keeps the fallback live.
    let plan_applied = match plan_copy(&sys_rows, true) {
        Some(CopyTarget::RingAndSystem(line)) => clip.copy_line(user, &line),
        _ => false,
    } && clip.paste_text().as_deref() == Some("row nine");
    let sys_clip_ok = copy_plan_ok && paste_plan_ok && plan_applied;
    println!(
        "system clipboard copy/paste adapter plan [{}]",
        pass_str(sys_clip_ok)
    );
    ok &= sys_clip_ok;

    // Palette mapping contract (GY-ZCODE-004): pure, no display needed.
    let palette = monitor_palette();
    let pal_ok = node_fill_override("title", &palette) == Some(0x1e1e28)
        && node_fill_override("row_00", &palette).is_none()
        && status_fill_override("subscribed", &palette) == Some(rgb_u32_bin(palette.ok))
        && status_fill_override("down: x", &palette) == Some(rgb_u32_bin(palette.err))
        && status_fill_override("running", &palette) == Some(rgb_u32_bin(palette.accent));
    println!("palette node/status fill mapping [{}]", pass_str(pal_ok));
    ok &= pal_ok;

    // Text-colour mapping contract (GY-ZCODE-009): status text follows its
    // fill mapping, tool ✓/✗ rows take ok/err, markdown headings accent,
    // everything else stays flat. Checked through the same override_slices
    // helper the live render path uses; status is per-frame, so each
    // status state gets its own pass.
    let status_of = |status: &str, name: &str| {
        override_slices(vec![(name, Some("status"), None)], status, &palette).1[0]
    };
    let rows: Vec<(&str, Option<&str>, Option<u32>)> = vec![
        ("row_00", Some("⚙ read_file ✓"), None),
        ("row_01", Some("⚙ write_file ✗"), None),
        ("row_02", Some("■ REPORT"), None),
        ("row_03", Some("plain response"), None),
    ];
    let (_, row_fgs) = override_slices(rows, "running", &palette);
    let fg_ok = status_of("idle", "status") == Some(rgb_u32_bin(palette.ok))
        && status_of("error", "status") == Some(rgb_u32_bin(palette.err))
        && status_of("running", "status") == Some(rgb_u32_bin(palette.accent))
        && status_of("running", "title").is_none()
        && row_fgs
            == vec![
                Some(rgb_u32_bin(palette.ok)),
                Some(rgb_u32_bin(palette.err)),
                Some(rgb_u32_bin(palette.accent)),
                None,
            ];
    println!(
        "text-colour status/tool/heading mapping [{}]",
        pass_str(fg_ok)
    );
    ok &= fg_ok;

    // Tool-row lifecycle contract (GY-ZCODE-005): started → progress
    // (folded) → completed emits exactly two lines; failed emits ✗.
    let mut tv = SessionView::default();
    tv.apply(&ProjEvent::ToolUpdate {
        id: "call_1".into(),
        tool: "read_file".into(),
        state: ToolState::Started,
    });
    // progress arrives as a repeated Started (agent.rs folds the kind).
    tv.apply(&ProjEvent::ToolUpdate {
        id: "call_1".into(),
        tool: "read_file".into(),
        state: ToolState::Started,
    });
    tv.apply(&ProjEvent::ToolUpdate {
        id: "call_1".into(),
        tool: "read_file".into(),
        state: ToolState::Completed,
    });
    let gear: Vec<&String> = tv.history.iter().filter(|l| l.contains('⚙')).collect();
    let tool_ok = gear == vec!["⚙ read_file …", "⚙ read_file ✓"];
    // Failed case.
    tv.apply(&ProjEvent::ToolUpdate {
        id: "call_2".into(),
        tool: "write_file".into(),
        state: ToolState::Started,
    });
    tv.apply(&ProjEvent::ToolUpdate {
        id: "call_2".into(),
        tool: "write_file".into(),
        state: ToolState::Failed,
    });
    // After a terminal line, anything further is inert.
    let before = tv.history.clone();
    tv.apply(&ProjEvent::ToolUpdate {
        id: "call_1".into(),
        tool: "read_file".into(),
        state: ToolState::Started,
    });
    let tool_ok =
        tool_ok && tv.history.contains(&"⚙ write_file ✗".to_string()) && tv.history == before;
    println!(
        "tool-row started/progress/completed/failed [{}]",
        pass_str(tool_ok)
    );
    ok &= tool_ok;

    // Preedit display contract (GY-ZCODE-005): inline bracketed overlay,
    // cleared on Preedit(None), never committed into the buffer text.
    let mut pc = Composer::new();
    pc.focused = true;
    pc.set_preedit(Some("预"));
    let preedit_ok = pc.text().is_empty() && pc.display() == "«│预»";
    pc.set_preedit(None);
    let preedit_ok = preedit_ok && pc.display() == "│";
    println!("composer preedit inline overlay [{}]", pass_str(preedit_ok));
    ok &= preedit_ok;

    // Session-switch contract (GY-ZCODE-006): cycling over a synthetic
    // session list wraps both directions; the switch action unfocuses the
    // composer, enqueues SwitchSession for the selected id, resets the
    // transcript and shows the <n>/<m> index in the status line.
    let mut sv = SessionView::default();
    sv.sessions = (0..3)
        .map(|i| goya_zcode_native::view::SessionEntry {
            session_id: format!("sess_{i}"),
            title: format!("session {i}"),
            status: "idle".into(),
        })
        .collect();
    let fwd_ok = sv
        .cycle_selection(1)
        .map(|e| e.session_id.clone())
        .as_deref()
        == Some("sess_1")
        && sv
            .cycle_selection(1)
            .map(|e| e.session_id.clone())
            .as_deref()
            == Some("sess_2")
        && sv
            .cycle_selection(1)
            .map(|e| e.session_id.clone())
            .as_deref()
            == Some("sess_0")
        && sv
            .cycle_selection(-1)
            .map(|e| e.session_id.clone())
            .as_deref()
            == Some("sess_2")
        && sv
            .cycle_selection(-1)
            .map(|e| e.session_id.clone())
            .as_deref()
            == Some("sess_1");
    sv.push_line("old transcript");
    sv.apply(&ProjEvent::PartDelta {
        delta: "stream".into(),
    });
    sv.reset_transcript();
    let reset_ok = sv.tail.is_empty()
        && sv.history.is_empty()
        && sv.streaming.is_empty()
        && sv.switching_status() == "2/3 switching session…";
    let mut sw_composer = Composer::new();
    sw_composer.focused = true;
    let sw_handle = agent::MonitorHandle::new();
    if let Ok(mut view) = sw_handle.view.lock() {
        view.sessions = sv.sessions.clone();
        view.selected = 0;
        view.push_line("stale transcript");
    }
    let switched = handle_session_switch(&sw_handle, &mut sw_composer, -1);
    let switch_ok = switched
        && !sw_composer.focused
        && matches!(
            sw_handle.try_take_cmd(),
            Some(agent::AgentCmd::SwitchSession { ref session_id, .. }) if session_id == "sess_2"
        )
        && sw_handle
            .view
            .lock()
            .map(|v| v.tail.is_empty() && v.status == "3/3 switching session…")
            .unwrap_or(false);
    let sess_ok = fwd_ok && reset_ok && switch_ok;
    println!(
        "session switch cycle/reset/index status [{}]",
        pass_str(sess_ok)
    );
    ok &= sess_ok;

    // Per-session retention contract (GY-ZCODE-007): save/switch/restore
    // cycle over synthetic state — switching away retains the outgoing
    // transcript+composer in the handle LRU, acquiring an unknown session
    // leaves the default empty transcript, and switching back restores
    // both (the composer half via take_composer_restore).
    let ret_handle = agent::MonitorHandle::new();
    ret_handle.set_active_session(Some("sess_ret_a".to_string()));
    if let Ok(mut view) = ret_handle.view.lock() {
        view.push_line("retained line one");
        view.push_line("retained line two");
    }
    let mut ret_composer = Composer::new();
    ret_composer.focused = true;
    for ch in "draft for a".chars() {
        ret_composer.handle_char(ch);
    }
    ret_handle.switch_session("sess_ret_b".to_string(), ret_composer.snapshot());
    let saved_ok = ret_handle
        .view
        .lock()
        .map(|v| v.history.is_empty() && v.tail.is_empty())
        .unwrap_or(false);
    // Acquiring sess_ret_b (unknown): default empty, nothing staged.
    ret_handle.on_session_acquired("sess_ret_b");
    let unknown_ok = ret_handle
        .view
        .lock()
        .map(|v| v.history.is_empty())
        .unwrap_or(false)
        && ret_handle.take_composer_restore().is_none();
    // Switch back to sess_ret_a, re-acquire: transcript + composer return.
    ret_handle.set_active_session(Some("sess_ret_b".to_string()));
    ret_handle.switch_session("sess_ret_a".to_string(), Composer::new().snapshot());
    ret_handle.on_session_acquired("sess_ret_a");
    let restored_view_ok = ret_handle
        .view
        .lock()
        .map(|v| {
            v.history
                == vec![
                    "retained line one".to_string(),
                    "retained line two".to_string(),
                ]
        })
        .unwrap_or(false);
    let restored_composer_ok = ret_handle
        .take_composer_restore()
        .map(|snap| {
            let mut c = Composer::new();
            c.restore(snap);
            c.text() == "draft for a" && c.cursor_grapheme() == "draft for a".chars().count()
        })
        .unwrap_or(false);
    let ret_ok = saved_ok && unknown_ok && restored_view_ok && restored_composer_ok;
    println!(
        "per-session transcript/composer save/restore cycle [{}]",
        pass_str(ret_ok)
    );
    ok &= ret_ok;

    // Live a11y snapshot contract (GY-ZCODE-008): DIG from synthetic
    // state over the real startup geometry — composer as Textbox with
    // live text + focused, rows with their text, quit pressable, and
    // exactly one republish per dirty flag.
    let mut a11y = goya_zcode_native::a11y::MonitorA11y::new(&frame);
    let mut av = SessionView::default();
    av.push_line("> acceptance input");
    av.push_line("final answer");
    let mut ac = Composer::new();
    ac.focused = true;
    for ch in "typed".chars() {
        ac.handle_char(ch);
    }
    a11y.republish(&av, &ac);
    let node = |id: &str| a11y.nodes().iter().find(|n| n.id == id).cloned();
    let comp_ok = node("composer").map(|n| {
        goya_a11y::A11yRole::TextInput == n.role
            && n.resolved_name() == "typed"
            && n.focused
            && n.editable
            && n.focusable
    }) == Some(true);
    let last_visible_row = frame
        .nodes
        .iter()
        .filter(|frame_node| frame_node.name.starts_with("row_"))
        .count();
    let input_row = format!("row_{:02}", last_visible_row.saturating_sub(2));
    let answer_row = format!("row_{:02}", last_visible_row.saturating_sub(1));
    let rows_ok = node(&input_row).map(|n| n.resolved_name() == "> acceptance input") == Some(true)
        && node(&answer_row).map(|n| n.resolved_name() == "final answer") == Some(true);
    let quit_ok =
        node("quit").map(|n| n.role == goya_a11y::A11yRole::Button && n.focusable) == Some(true);
    a11y.republish(&av, &ac);
    let counter_ok = a11y.publishes() == 2;
    let a11y_ok = comp_ok && rows_ok && quit_ok && counter_ok;
    println!(
        "a11y snapshot composer/rows/quit + republish-on-change [{a11y_ok}]",
        a11y_ok = pass_str(a11y_ok)
    );
    ok &= a11y_ok;

    // Session-shell contract: queue and permission controls share one action
    // path, keep a distinct hit target for the second allow choice, and stay
    // disabled until the owner projection settles the in-flight command.
    let workflow_handle = agent::MonitorHandle::new();
    workflow_handle.set_active_session(Some("sess_workflow".into()));
    {
        let mut view = workflow_handle.view.lock().expect("workflow view");
        view.sessions = vec![
            SessionEntry {
                session_id: "sess_workflow".into(),
                title: "Workflow".into(),
                status: "idle".into(),
            },
            SessionEntry {
                session_id: "sess_other".into(),
                title: "Other".into(),
                status: "idle".into(),
            },
        ];
        view.replace_workflow(WorkflowState {
            queue_items: vec![WorkflowQueueItem {
                queue_item_id: "queue-1".into(),
                text: "next step".into(),
                delivery: "queue".into(),
            }],
            auto_drain: true,
            interactions: vec![WorkflowInteraction {
                interaction_id: "permission-1".into(),
                kind: "permission".into(),
                title: "Allow read?".into(),
                summary: "Read src/lib.rs".into(),
                options: vec![
                    WorkflowInteractionOption {
                        option_id: "once".into(),
                        label: "Allow once".into(),
                        kind: "allowOnce".into(),
                    },
                    WorkflowInteractionOption {
                        option_id: "always".into(),
                        label: "Allow always".into(),
                        kind: "allowAlways".into(),
                    },
                    WorkflowInteractionOption {
                        option_id: "deny".into(),
                        label: "Deny".into(),
                        kind: "deny".into(),
                    },
                ],
                free_text: false,
                unsupported_reason: None,
            }],
            ..Default::default()
        });
    }
    let controls = workflow_handle
        .view
        .lock()
        .expect("workflow controls")
        .workflow_controls();
    let shell = layout_monitor_with_controls(store.get(), WIN_W_PX, WIN_H_PX, &controls);
    let legacy_controls: Vec<_> = controls
        .iter()
        .filter(|control| {
            !matches!(
                control.id.as_str(),
                "stop" | "goal_pause" | "goal_resume" | "compact_session"
            ) && !control.id.starts_with("queue_now_")
                && !control.id.starts_with("queue_earlier_")
                && !control.id.starts_with("work_cancel_")
        })
        .cloned()
        .collect();
    let legacy = layout_monitor_with_controls(store.get(), WIN_W_PX, WIN_H_PX, &legacy_controls);
    let same_rect = |name: &str| {
        let current = shell.frame.find(name).unwrap().rect;
        let previous = legacy.frame.find(name).unwrap().rect;
        current.x == previous.x
            && current.y == previous.y
            && current.width == previous.width
            && current.height == previous.height
    };
    let remove = shell.frame.find("queue_remove_00").unwrap().rect;
    let allow = shell.frame.find("permission_allow").unwrap().rect;
    let always = shell.frame.find("permission_option_01").unwrap().rect;
    let targets_ok = remove.width > 0
        && allow.width > 0
        && always.width > 0
        && same_rect("queue_remove_00")
        && same_rect("permission_option_01")
        && hit_test(
            &shell.frame,
            remove.x + remove.width / 2,
            remove.y + remove.height / 2,
        )
        .as_deref()
            == Some("queue_remove_00")
        && hit_test(
            &shell.frame,
            always.x + always.width / 2,
            always.y + always.height / 2,
        )
        .as_deref()
            == Some("permission_option_01");
    let small = layout_monitor_with_controls(store.get(), 80, 48, &controls);
    let small_ok = small.frame.find("queue_remove_00").unwrap().rect.width == 0
        && small.frame.find("permission_allow").unwrap().rect.width == 0
        && small.frame.find("permission_option_01").unwrap().rect.width == 0
        && small.frame.find("stop").unwrap().rect.width == 0
        && small.frame.find("queue_now_00").unwrap().rect.width == 0
        && small.frame.find("quit").unwrap().rect.width > 0;
    let mut shell_a11y = goya_zcode_native::a11y::MonitorA11y::new(&shell.frame);
    {
        let view = workflow_handle.view.lock().expect("workflow a11y");
        shell_a11y.republish(&view, &Composer::new());
    }
    let names_ok = shell_a11y.nodes().iter().any(|node| {
        node.id == "permission_allow" && node.resolved_name() == "Allow once" && node.enabled
    }) && shell_a11y.nodes().iter().any(|node| {
        node.id == "permission_option_01" && node.resolved_name() == "Allow always" && node.enabled
    });
    let keyboard_choice = workflow_handle
        .view
        .lock()
        .ok()
        .and_then(|view| keyboard_workflow_control('2', &view));
    let mut shell_composer = Composer::new();
    let activated = keyboard_choice.is_some_and(|id| {
        activate_workflow_control(id, &workflow_handle, &mut shell_composer, true)
    });
    let command_ok = matches!(
        workflow_handle.try_take_cmd(),
        Some(agent::AgentCmd::SessionCommand { command_type, payload, .. })
            if command_type == "resolveInteraction" && payload["answer"]["optionId"] == "always"
    );
    let repeat_blocked = !activate_workflow_control(
        "permission_option_01",
        &workflow_handle,
        &mut shell_composer,
        true,
    ) && workflow_handle.try_take_cmd().is_none()
        && workflow_control_enabled(&workflow_handle, "queue_remove_00");
    {
        let mut view = workflow_handle.view.lock().expect("settle workflow");
        let workflow = view.workflow.clone();
        view.replace_workflow(workflow);
    }
    let settled = workflow_control_enabled(&workflow_handle, "permission_option_01");
    let shell_ok = targets_ok
        && small_ok
        && names_ok
        && keyboard_choice == Some("permission_option_01")
        && activated
        && command_ok
        && repeat_blocked
        && settled;
    println!(
        "workflow controls hit/keyboard/a11y/pending/small-window [{}]",
        pass_str(shell_ok)
    );
    ok &= shell_ok;

    let queue_before = workflow_handle
        .view
        .lock()
        .map(|view| view.workflow.queue_items.clone())
        .unwrap_or_default();
    {
        let mut view = workflow_handle.view.lock().expect("operation view");
        view.workflow.availability.send_queued_now = true;
        view.workflow.control.can_stop = true;
        view.workflow.control.active_works.clear();
    }
    let operation_key = workflow_handle
        .view
        .lock()
        .ok()
        .and_then(|view| keyboard_workflow_control('u', &view));
    let mut operation_composer = Composer::new();
    let operation_activated = operation_key.is_some_and(|id| {
        activate_workflow_control(id, &workflow_handle, &mut operation_composer, true)
    });
    let operation_command = matches!(
        workflow_handle.try_take_cmd(),
        Some(agent::AgentCmd::SessionCommand { command_type, payload, .. })
            if command_type == "sendQueuedNow"
                && payload["queueItemId"] == "queue-1"
                && payload.get("text").is_none()
    );
    let operation_queue = workflow_handle
        .view
        .lock()
        .map(|view| view.workflow.queue_items == queue_before)
        .unwrap_or(false);
    let operation_ok = operation_key == Some("queue_now_00")
        && operation_activated
        && operation_command
        && operation_queue;
    println!(
        "session operation sendQueuedNow keeps the owner queue [{}]",
        pass_str(operation_ok)
    );
    ok &= operation_ok;

    if !ok {
        std::process::exit(1);
    }
    println!("zcode-native: headless acceptance PASS");
}

/// `0xRRGGBB` from an sRGB triple (acceptance helper).
fn rgb_u32_bin(c: [u8; 3]) -> u32 {
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
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
                    Ok(Ok(gen)) => println!("zcode-native: hot-swapped to generation {gen}"),
                    Ok(Err(e)) => println!("zcode-native: swap rejected ({e}) — scene retained"),
                    Err(e) => println!("zcode-native: read failed ({e}) — retained"),
                }
            }
        }
    }
}

fn run_dump(
    args: &Args,
    store: &mut SceneStore,
    last_mtime: &mut Option<SystemTime>,
    a11y: &mut goya_zcode_native::a11y::MonitorA11y,
) {
    println!(
        "zcode-native: dumping projection (cli={}, workspace={})",
        args.cli,
        args.workspace.display()
    );
    let handle = spawn_monitor(args);
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
                    // GY-ZCODE-008: one a11y republish per change; in
                    // --a11y-dump mode each republish prints the tree.
                    // Dump mode has no composer instance (it lives in the
                    // live loop), so the composer projects unfocused with
                    // the placeholder label.
                    a11y.republish(&view, &Composer::new());
                    if args.a11y_dump {
                        println!("--- a11y");
                        print!("{}", a11y.dump());
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
    handle.shutdown();
    println!("zcode-native: dump done");
}

fn run_live(
    args: &Args,
    store: &mut SceneStore,
    last_mtime: &mut Option<SystemTime>,
    a11y: &mut goya_zcode_native::a11y::MonitorA11y,
) -> Result<(), String> {
    let mut win = match GoyaWindow::create("zcode-native", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            return Err(format!("live unavailable: {e}"));
        }
    };
    win.set_extended_text_input_events(true);
    let a11y_bridge = args
        .a11y_helper
        .as_ref()
        .map(|path| goya_zcode_native::accessibility_bridge::AccessibilityBridge::spawn(path))
        .transpose()?;
    let handle = spawn_monitor(args);
    // GY-ZCODE-002: composer focus is host-side state (parse/layout stay
    // deterministic per the frozen Wave 2 contract).
    let mut composer = Composer::new();
    // GY-ZCODE-004: in-process clipboard ring (Wayland data-device is out,
    // blocked upstream) and the chroma-derived monitor palette, computed
    // once — the palette is a pure function of the fixed dark theme.
    let clip_policy = AllowAllPolicy;
    let clipboard = MonitorClipboard::new(&clip_policy);
    let palette = monitor_palette();
    let started = Instant::now();
    let mut frames = 0u32;
    let mut render_error = None;
    let mut viewport_width = WIN_W_PX;
    let mut viewport_height = WIN_H_PX;
    while !handle.stop.load(Ordering::Relaxed)
        && !win.close_requested()
        && args.frames.is_none_or(|limit| frames < limit)
    {
        if args.watch {
            watch_swap(store, last_mtime);
        }
        let mut dirty = handle.take_dirty();
        // Wave 7 (GY-ZCODE-007): the run loop staged a retained composer
        // snapshot for the newly acquired session — apply it here (the
        // composer instance lives in the bin; focus stays wherever the
        // switch left it, i.e. unfocused).
        if let Some(snap) = handle.take_composer_restore() {
            composer.restore(snap);
            dirty = true;
        }
        let mut current_layout = workflow_layout(store, &handle, viewport_width, viewport_height);
        if let Some(bridge) = &a11y_bridge {
            if let Some(action) = bridge.try_action(a11y.tree()) {
                let changed =
                    apply_accessible_action(action, &handle, &mut composer, args.allow_send);
                if changed {
                    win.set_text_input_active(composer.focused);
                    sync_text_input(&mut win, &composer, &current_layout.frame);
                    dirty = true;
                }
            }
        }
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    if let Some(name) = hit_test(
                        &current_layout.frame,
                        (x as i64) * goya_wayland::MP_PER_PX,
                        (y as i64) * goya_wayland::MP_PER_PX,
                    ) {
                        if name == "quit" {
                            handle.stop.store(true, Ordering::Relaxed);
                            dirty = true;
                        } else if name == "session_prev" || name == "session_next" {
                            let delta = if name == "session_prev" { -1 } else { 1 };
                            dirty |= handle_session_switch(&handle, &mut composer, delta);
                        } else {
                            let now_focused = if name == "composer" {
                                true
                            } else if matches!(name.as_str(), "queue_mode" | "guide_mode") {
                                composer.focused
                            } else {
                                false
                            };
                            dirty |= composer.focused != now_focused;
                            composer.focused = now_focused;
                            if name != "composer" {
                                dirty |= activate_workflow_control(
                                    &name,
                                    &handle,
                                    &mut composer,
                                    args.allow_send,
                                );
                            }
                            // Wave 5 (GY-ZCODE-005): the window text input
                            // is activated iff the composer is focused
                            // (disabled by default in goya-wayland; the
                            // composer starts unfocused at startup, so no
                            // startup activation is needed).
                            win.set_text_input_active(now_focused);
                            sync_text_input(&mut win, &composer, &current_layout.frame);
                        }
                    }
                }
                goya_wayland::PlatformEvent::KeyChar(_)
                | goya_wayland::PlatformEvent::KeySpecial(_)
                | goya_wayland::PlatformEvent::KeySpecialWithModifiers { .. } => {
                    // Escape can unfocus the composer (composer-side); keep
                    // the window text input activation in sync with focus.
                    let was_focused = composer.focused;
                    if composer.focused {
                        // Composer owns the keys. Submit Enter transactionally:
                        // rejected admission restores the pre-submit draft.
                        if matches!(&ev, goya_wayland::PlatformEvent::KeyChar('\n' | '\r')) {
                            submit_composer_line(&handle, &mut composer, args.allow_send);
                        } else {
                            feed_key(&mut composer, &ev);
                        }
                    } else if let goya_wayland::PlatformEvent::KeyChar(key) = &ev {
                        let control = handle
                            .view
                            .lock()
                            .ok()
                            .and_then(|view| keyboard_workflow_control(*key, &view));
                        if let Some(control) = control {
                            let _ = activate_workflow_control(
                                control,
                                &handle,
                                &mut composer,
                                args.allow_send,
                            );
                        }
                    } else if let goya_wayland::PlatformEvent::KeySpecial(s) = &ev {
                        // Unfocused: page keys scroll the transcript,
                        // arrow keys fall through to the composer arm
                        // (inert while unfocused).
                        scroll_history(
                            &handle,
                            key_special_to_action(special_name(*s)),
                            current_layout.visible_rows,
                        );
                    } else if let goya_wayland::PlatformEvent::KeySpecialWithModifiers {
                        key, ..
                    } = &ev
                    {
                        scroll_history(
                            &handle,
                            key_special_to_action(special_name(*key)),
                            current_layout.visible_rows,
                        );
                    }
                    if composer.focused != was_focused {
                        win.set_text_input_active(composer.focused);
                    }
                    sync_text_input(&mut win, &composer, &current_layout.frame);
                    dirty = true;
                }
                goya_wayland::PlatformEvent::PointerScroll(lines) => {
                    dirty |= scroll_pointer_history(
                        &handle,
                        lines,
                        composer.focused,
                        current_layout.visible_rows,
                    );
                }
                goya_wayland::PlatformEvent::Preedit(_)
                | goya_wayland::PlatformEvent::PreeditWithCursor { .. }
                | goya_wayland::PlatformEvent::DeleteSurroundingText { .. }
                | goya_wayland::PlatformEvent::CommitText(_) => {
                    if composer.focused {
                        feed_key(&mut composer, &ev);
                    }
                    sync_text_input(&mut win, &composer, &current_layout.frame);
                    dirty = true;
                }
                // Wave 9: the compositor's answer to request_clipboard_text
                // (Ctrl+V). Some(text) inserts at the composer cursor;
                // None (no selection / no text mime / empty payload —
                // ambiguous upstream, see clipboard module docs) falls back
                // to the ring.
                goya_wayland::PlatformEvent::ClipboardText(text) => {
                    if composer.focused {
                        if let Some(insert) =
                            resolve_paste(text.as_deref(), clipboard.paste_text().as_deref())
                        {
                            for ch in insert.chars() {
                                if composer.handle_char(ch).is_some() {
                                    break;
                                }
                            }
                        }
                    }
                    sync_text_input(&mut win, &composer, &current_layout.frame);
                    dirty = true;
                }
                goya_wayland::PlatformEvent::Resized(width, height) => {
                    // 中文说明：旧处理只置 dirty，没有用 compositor 的新尺寸重算 frame，
                    // 导致绘制、命中测试、a11y 与 IME 候选框仍引用 400×320 几何。
                    viewport_width = width.max(0);
                    viewport_height = height.max(0);
                    current_layout =
                        workflow_layout(store, &handle, viewport_width, viewport_height);
                    if let Ok(mut view) = handle.view.lock() {
                        view.set_viewport_rows(current_layout.visible_rows);
                    }
                    a11y.set_frame(&current_layout.frame);
                    sync_text_input(&mut win, &composer, &current_layout.frame);
                    dirty = true;
                }
                // GY-ZCODE-004: ctrl+c/v/q shortcuts from goya-wayland.
                PlatformEvent::Shortcut(shortcut) => {
                    // Wave 6 (GY-ZCODE-006): ctrl+Left/Right session
                    // switching (Shortcut::SessionNext/SessionPrevious,
                    // J1's goya-wayland variants).
                    if let Some(delta) = shortcut_switch_delta(shortcut) {
                        dirty |= handle_session_switch(&handle, &mut composer, delta);
                        sync_text_input(&mut win, &composer, &current_layout.frame);
                    }
                    match shortcut {
                        Shortcut::Copy => {
                            // Wave 9: plan the copy purely (last non-empty
                            // row + targets), then apply: ring record via
                            // copy_line AND compositor selection via
                            // set_clipboard_text (a safe no-op when the
                            // compositor has no data device — the ring
                            // remains the offline fallback).
                            let rows = handle
                                .view
                                .lock()
                                .map(|view| view.rows())
                                .unwrap_or_default();
                            let copied = match plan_copy(&rows, true) {
                                Some(CopyTarget::RingAndSystem(line)) => {
                                    win.set_clipboard_text(&line);
                                    clipboard.copy_line(PrincipalId(1), &line)
                                }
                                Some(CopyTarget::Ring(line)) => {
                                    clipboard.copy_line(PrincipalId(1), &line)
                                }
                                None => false,
                            };
                            if let Ok(mut view) = handle.view.lock() {
                                view.status = if copied {
                                    "copied".to_string()
                                } else {
                                    "copy failed".to_string()
                                };
                            }
                            dirty = true;
                        }
                        Shortcut::Paste => {
                            // Wave 9: ask the compositor; the insertion
                            // happens when the ClipboardText event arrives
                            // (see its arm below). Nothing is inserted here
                            // — the ring fallback lives in resolve_paste.
                            win.request_clipboard_text();
                            sync_text_input(&mut win, &composer, &current_layout.frame);
                            dirty = true;
                        }
                        Shortcut::Quit => {
                            handle.stop.store(true, Ordering::Relaxed);
                            dirty = true;
                        }
                        // Handled above via shortcut_switch_delta.
                        Shortcut::SessionNext | Shortcut::SessionPrevious => {}
                    }
                }
                _ => {}
            }
        }
        if dirty {
            let mut nodes = current_layout.frame.nodes.clone();
            let mut status_text = String::new();
            if let Ok(view) = handle.view.lock() {
                status_text = match &view.link {
                    LinkState::Connecting => String::from("linking"),
                    LinkState::Up => view.status.clone(),
                    LinkState::Down(reason) => format!("down: {reason}"),
                };
                // GY-ZCODE-008: a11y republish rides the same dirty flag
                // as the render (view + composer change), silently in
                // live mode — AT-SPI bridging is upstream goya-a11y.
                a11y.set_frame(&current_layout.frame);
                a11y.republish(&view, &composer);
                apply_view_to_frame(&mut nodes, &view, current_layout.visible_rows);
                apply_workflow_to_frame(&mut nodes, &view, &composer);
            }
            if let Some(bridge) = &a11y_bridge {
                bridge.publish(a11y.tree())?;
            }
            apply_composer_to_frame(&mut nodes, &composer);
            let prims: Vec<Primitive> = nodes.iter().map(present_node).collect();
            // GY-ZCODE-004: chroma-derived per-frame fill overrides take
            // precedence; scene `fill:` values remain the defaults. Wave 9:
            // the fg overrides are built alongside, in the same primitive
            // order (status ok/err/accent, tool ✓/✗, headings accent).
            let (fills, fgs) = override_slices(
                nodes
                    .iter()
                    .map(|node| (node.name.as_str(), node.text.as_deref(), node.fill)),
                &status_text,
                &palette,
            );
            // GATE (GY-ZCODE-009) DONE: J1 landed the `fgs` parameter on
            // present — the per-node text colours are live.
            let text_sizes: Vec<_> = nodes
                .iter()
                .map(|node| node.text_size.map(|size| size as f32))
                .collect();
            if let Err(error) =
                win.present_styled(&prims, &fills, &fgs, &text_sizes, palette.fg, palette.bg, 1)
            {
                render_error = Some(error.to_string());
                break;
            }
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
    handle.shutdown();
    println!("zcode-native: live ended after {frames} frames");
    render_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod input_event_tests {
    use super::{
        activate_workflow_control, feed_key, keyboard_workflow_control, Composer, InputDelivery,
    };
    use goya_wayland::{KeySpecial, PlatformEvent};
    use goya_zcode_native::agent::{AgentCmd, MonitorHandle};
    use goya_zcode_native::view::{
        SessionEntry, WorkflowInteraction, WorkflowInteractionOption, WorkflowQueueItem,
        WorkflowState,
    };

    #[test]
    fn unicode_xkb_chars_and_ime_commits_share_grapheme_safe_cursor() {
        let mut composer = Composer::new();
        composer.focused = true;
        feed_key(&mut composer, &PlatformEvent::KeyChar('e'));
        feed_key(&mut composer, &PlatformEvent::KeyChar('\u{301}'));
        assert_eq!(composer.text(), "e\u{301}");
        assert_eq!(composer.cursor_grapheme(), 1);

        feed_key(
            &mut composer,
            &PlatformEvent::KeySpecial(KeySpecial::ArrowLeft),
        );
        feed_key(&mut composer, &PlatformEvent::CommitText("漢字".into()));
        assert_eq!(composer.text(), "漢字e\u{301}");
        assert_eq!(composer.cursor_grapheme(), 2);
        assert_eq!(composer.buffer.cursor_byte(), "漢字".len());
    }

    #[test]
    fn preedit_overlay_clears_without_committing_and_commit_replaces_it() {
        let mut composer = Composer::new();
        composer.focused = true;
        feed_key(&mut composer, &PlatformEvent::Preedit(Some("かな".into())));
        assert_eq!(composer.text(), "");
        assert_eq!(composer.display(), "«│かな»");
        feed_key(&mut composer, &PlatformEvent::Preedit(None));
        assert_eq!(composer.display(), "│");

        feed_key(&mut composer, &PlatformEvent::Preedit(Some("かな".into())));
        feed_key(&mut composer, &PlatformEvent::CommitText("仮名".into()));
        assert_eq!(composer.text(), "仮名");
        assert_eq!(composer.display(), "仮名│");
    }

    #[test]
    fn ime_byte_delete_and_preedit_cursor_follow_composer_graphemes() {
        // The upstream `PreeditWithCursor`/`DeleteSurroundingText`
        // PlatformEvent variants were removed in the GY-ZCODE-006
        // goya-wayland rework; the composer-side methods remain and are
        // exercised directly here (previously these went through
        // `feed_key`).
        let mut composer = Composer::new();
        composer.focused = true;
        composer.buffer.set_text("日本語");
        composer.buffer.move_end(false);
        composer.delete_surrounding(3, 0);
        assert_eq!(composer.text(), "日本");
        composer.set_preedit_with_cursor(Some("かな"), 3, 6);
        assert_eq!(composer.display(), "日本«か│な»");
    }

    #[test]
    fn done_batched_native_ime_events_apply_delete_commit_then_new_preedit() {
        let mut composer = Composer::new();
        composer.focused = true;
        composer.buffer.set_text("abcdef");
        composer.handle_special("home");
        for _ in 0..3 {
            composer.handle_special("arrow_right");
        }

        let mut batch = goya_wayland::TextInputState::new();
        batch.feed_preedit_with_cursor(Some("old".into()), 1, 2);
        batch.feed_done();
        for event in batch.drain() {
            feed_key(&mut composer, &event);
        }
        assert_eq!(composer.preedit, Some(("old".into(), 1, 2)));

        batch.feed_delete_surrounding(1, 1);
        batch.feed_commit(Some("X".into()));
        batch.feed_preedit_with_cursor(Some("かな".into()), 3, 6);
        assert!(batch.drain().is_empty());
        batch.feed_done();
        for event in batch.drain() {
            feed_key(&mut composer, &event);
        }
        assert_eq!(composer.text(), "abXef");
        assert_eq!(composer.preedit, Some(("かな".into(), 3, 6)));
    }

    // --- Wave 6 (GY-ZCODE-006): session switching ---

    #[test]
    fn shortcuts_map_to_switch_directions() {
        use super::shortcut_switch_delta;
        use goya_wayland::Shortcut;
        assert_eq!(shortcut_switch_delta(Shortcut::SessionNext), Some(1));
        assert_eq!(shortcut_switch_delta(Shortcut::SessionPrevious), Some(-1));
        assert_eq!(shortcut_switch_delta(Shortcut::Copy), None);
        assert_eq!(shortcut_switch_delta(Shortcut::Paste), None);
        assert_eq!(shortcut_switch_delta(Shortcut::Quit), None);
    }

    #[test]
    fn session_switch_unfocuses_cycles_and_enqueues() {
        use super::handle_session_switch;
        use goya_zcode_native::agent::{AgentCmd, MonitorHandle};
        use goya_zcode_native::view::SessionEntry;

        let mut composer = Composer::new();
        composer.focused = true;
        let handle = MonitorHandle::new();
        handle.view.lock().unwrap().sessions = (0..3)
            .map(|i| SessionEntry {
                session_id: format!("s{i}"),
                title: String::new(),
                status: String::new(),
            })
            .collect();
        assert!(handle_session_switch(&handle, &mut composer, 1));
        assert!(!composer.focused, "switch unfocuses the composer");
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SwitchSession { ref session_id, .. }) if session_id == "s1"
        ));
        // Wrap backward from the first entry.
        assert!(handle_session_switch(&handle, &mut composer, -1));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SwitchSession { ref session_id, .. }) if session_id == "s0"
        ));
        // Empty list: no switch is enqueued, composer stays unfocused.
        handle.view.lock().unwrap().sessions.clear();
        assert!(!handle_session_switch(&handle, &mut composer, 1));
        assert!(handle.try_take_cmd().is_none());
    }

    #[test]
    fn unfocused_composer_ignores_unicode_and_ime_batches() {
        let mut composer = Composer::new();
        feed_key(&mut composer, &PlatformEvent::KeyChar('ø'));
        feed_key(&mut composer, &PlatformEvent::Preedit(Some("ø".into())));
        feed_key(&mut composer, &PlatformEvent::CommitText("ø".into()));
        assert_eq!(composer.text(), "");
        assert_eq!(composer.preedit, None);
    }

    #[test]
    fn keyboard_and_pointer_workflow_choices_share_one_activation_path() {
        let mut view = goya_zcode_native::view::SessionView::default();
        view.sessions.push(SessionEntry {
            session_id: "session-1".into(),
            title: String::new(),
            status: String::new(),
        });
        let handle = MonitorHandle::new();
        handle.view.lock().unwrap().sessions = view.sessions.clone();
        let mut composer = Composer::new();
        let keyboard_id = keyboard_workflow_control('q', &view).unwrap();
        assert!(activate_workflow_control(
            keyboard_id,
            &handle,
            &mut composer,
            true
        ));
        assert_eq!(composer.delivery, InputDelivery::Queue);
        // Pointer activation calls the same typed action handler.
        assert!(activate_workflow_control(
            "guide_mode",
            &handle,
            &mut composer,
            true
        ));
        assert_eq!(composer.delivery, InputDelivery::Guide);
        assert!(
            handle.try_take_cmd().is_none(),
            "delivery choice is a draft preference"
        );
    }

    #[test]
    fn workflow_actions_target_current_owner_ids_without_optimistic_mutation() {
        let handle = MonitorHandle::new();
        handle.set_active_session(Some("session-1".into()));
        {
            let mut view = handle.view.lock().unwrap();
            view.sessions.push(SessionEntry {
                session_id: "session-1".into(),
                title: String::new(),
                status: String::new(),
            });
            view.projection_revision = 19;
            view.replace_workflow(WorkflowState {
                queue_items: vec![WorkflowQueueItem {
                    queue_item_id: "queue-1".into(),
                    text: "follow up".into(),
                    delivery: "queue".into(),
                }],
                auto_drain: true,
                interactions: vec![WorkflowInteraction {
                    interaction_id: "interaction-1".into(),
                    kind: "permission".into(),
                    title: "Allow read?".into(),
                    summary: "Read".into(),
                    options: vec![WorkflowInteractionOption {
                        option_id: "allow-once".into(),
                        label: "Allow once".into(),
                        kind: "allowOnce".into(),
                    }],
                    free_text: false,
                    unsupported_reason: None,
                }],
                ..Default::default()
            });
        }
        let mut composer = Composer::new();
        assert!(activate_workflow_control(
            "queue_remove_00",
            &handle,
            &mut composer,
            true
        ));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, base_revision: 19, .. })
                if command_type == "deleteQueueItem" && payload["queueItemId"] == "queue-1"
        ));
        assert_eq!(handle.view.lock().unwrap().workflow.queue_items.len(), 1);

        assert!(activate_workflow_control(
            "permission_allow",
            &handle,
            &mut composer,
            true
        ));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, base_revision: 19, .. })
                if command_type == "resolveInteraction"
                    && payload["interactionId"] == "interaction-1"
                    && payload["answer"]["optionId"] == "allow-once"
        ));
        assert_eq!(
            handle.view.lock().unwrap().workflow.interactions[0].interaction_id,
            "interaction-1"
        );
        assert!(
            !activate_workflow_control("permission_allow", &handle, &mut composer, true),
            "pending answer controls cannot enqueue a second resolution"
        );
        assert!(handle.try_take_cmd().is_none());
        {
            let mut view = handle.view.lock().unwrap();
            let workflow = view.workflow.clone();
            view.replace_workflow(workflow);
        }
        assert!(activate_workflow_control(
            "permission_allow",
            &handle,
            &mut composer,
            true
        ));
    }
}
