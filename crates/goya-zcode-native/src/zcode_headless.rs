//! Headless scene and workflow acceptance checks.
use super::*;

/// Headless acceptance (same shape as goya-scene): scene structure and a
/// projection round-trip, PASS/FAIL per check.
pub(super) fn headless_acceptance(store: &SceneStore) {
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
