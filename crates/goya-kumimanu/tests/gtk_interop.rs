//! Kumimanu against GTK 3's selection code, on a private selection name.
//!
//! These tests need `DISPLAY`, `python3`, and PyGObject with GTK 3. They
//! fail, not skip, when run without them: an absent peer is not a pass.
//! The selection is `GOYA_KUMIMANU_TEST_*`, so the desktop clipboard is not
//! read or replaced. The PRIMARY tests are the exception: PRIMARY is always
//! the real atom, so they briefly borrow the desktop primary selection.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use goya_kumimanu::{Event, Session, WindowConfig, WindowId, MAX_CLIPBOARD_BYTES};

/// Same text as `gtk_peer.py`: nine bytes per line, one of them a scalar
/// that UTF-8 encodes in two bytes.
fn text_for(lines: usize) -> String {
    (0..lines).map(|i| format!("{i:06}é\n")).collect()
}

fn selection(tag: &str) -> String {
    format!("GOYA_KUMIMANU_TEST_GTK_{tag}_{}", std::process::id())
}

fn peer() -> Command {
    assert!(
        std::env::var("DISPLAY").is_ok_and(|value| !value.is_empty()),
        "DISPLAY is unset"
    );
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/gtk_peer.py");
    let mut command = Command::new("python3");
    command
        .arg("-I")
        .arg(script)
        .env("GDK_BACKEND", "x11")
        .env_remove("WAYLAND_DISPLAY");
    command
}

fn window(name: &str) -> (Session, WindowId) {
    let mut session = Session::connect_with_selection(name.as_bytes()).expect("connect");
    let mut config = WindowConfig::new("kumimanu-gtk", 8, 8);
    config.override_redirect = true;
    config.x = -80;
    config.y = -80;
    let id = session.create_window(config).expect("create");
    session.show_window(id).expect("map");
    (session, id)
}

/// Like [`window`], but on the default connection. The PRIMARY tests have no
/// private name to swap in: PRIMARY is always the real atom.
fn default_window() -> (Session, WindowId) {
    let mut session = Session::connect().expect("connect");
    let mut config = WindowConfig::new("kumimanu-gtk", 8, 8);
    config.override_redirect = true;
    config.x = -80;
    config.y = -80;
    let id = session.create_window(config).expect("create");
    session.show_window(id).expect("map");
    (session, id)
}

/// Start GTK as owner of a selection (`own` or `own-primary` plus its
/// arguments) and wait until it says so.
fn gtk_owner(args: &[&str]) -> Child {
    let mut child = peer()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("python3 with GTK 3");
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .expect("peer stdout");
    assert_eq!(line.trim(), "ready", "GTK peer did not take the selection");
    child
}

/// The text, and whether it arrived by INCR.
fn kumimanu_reads(name: &str) -> (Option<String>, bool) {
    let (mut session, id) = window(name);
    session.request_clipboard_text(id).expect("request");
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        for event in session
            .poll_events(Duration::from_millis(20))
            .expect("poll")
        {
            if let Event::ClipboardText(text) = event {
                let incr = session.diagnostics().incr_reads > 0;
                session.close_window(id).unwrap();
                return (text, incr);
            }
        }
    }
    panic!("no clipboard answer from GTK");
}

/// The PRIMARY text Kumimanu sees from whoever currently owns the atom.
fn kumimanu_reads_primary() -> Option<String> {
    let (mut session, id) = default_window();
    session.request_primary_text(id).expect("request");
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        for event in session
            .poll_events(Duration::from_millis(20))
            .expect("poll")
        {
            if let Event::PrimaryText(text) = event {
                session.close_window(id).unwrap();
                return text;
            }
        }
    }
    panic!("no primary answer from GTK");
}

fn finish(mut child: Child) {
    drop(child.stdin.take());
    let status = child.wait().expect("peer exit");
    assert!(status.success(), "GTK peer exited with {status}");
}

#[test]
#[ignore = "needs DISPLAY and PyGObject GTK 3; run with --ignored"]
fn kumimanu_reads_small_and_incr_text_from_gtk() {
    // 90 bytes in one property, then 900000 bytes that GTK sends by INCR
    // because it is over GTK's 262144-byte single-property bound.
    for (tag, lines) in [("small", 10usize), ("incr", 100_000)] {
        let name = selection(tag);
        let child = gtk_owner(&["own", &name, &lines.to_string()]);
        let (got, incr) = kumimanu_reads(&name);
        finish(child);
        assert_eq!(incr, tag == "incr", "{tag}: INCR use");
        let want = text_for(lines);
        assert_eq!(got.as_ref().map(String::len), Some(want.len()), "{tag}");
        assert!(got.as_deref() == Some(want.as_str()), "{tag} text differs");
    }
}

#[test]
#[ignore = "needs DISPLAY and PyGObject GTK 3; run with --ignored"]
fn kumimanu_refuses_gtk_text_over_the_cap() {
    let lines = MAX_CLIPBOARD_BYTES / 9 + 100;
    let name = selection("over");
    let child = gtk_owner(&["own", &name, &lines.to_string()]);
    let (got, incr) = kumimanu_reads(&name);
    assert_eq!(got, None);
    assert!(
        incr,
        "the over-cap text should arrive by INCR and be drained"
    );
    finish(child);
}

#[test]
#[ignore = "needs DISPLAY and PyGObject GTK 3; run with --ignored"]
fn gtk_reads_small_and_incr_text_from_kumimanu() {
    // 900000 bytes is over Kumimanu's 256 KiB threshold, so it goes by INCR.
    for (tag, lines) in [("own-small", 10usize), ("own-incr", 100_000)] {
        let name = selection(tag);
        let want = text_for(lines);
        let (mut session, id) = window(&name);
        assert!(session.set_clipboard_text(id, &want).expect("own"));
        let mut child = peer()
            .args(["read", &name])
            .stdout(Stdio::piped())
            .spawn()
            .expect("python3 with GTK 3");
        let mut stdout = child.stdout.take().unwrap();
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).map(|_| bytes)
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            // Kumimanu serves from its own event stream, so keep pumping.
            let _ = session
                .poll_events(Duration::from_millis(10))
                .expect("poll");
            if let Some(status) = child.try_wait().expect("peer") {
                break status;
            }
            assert!(Instant::now() < deadline, "{tag}: GTK did not finish");
        };
        assert!(status.success(), "{tag}: GTK read no text ({status})");
        let got = reader.join().unwrap().expect("peer stdout");
        assert_eq!(got.len(), want.len(), "{tag}");
        assert!(got == want.as_bytes(), "{tag} text differs");
        let incr = session.diagnostics().incr_writes > 0;
        assert_eq!(incr, tag == "own-incr", "{tag}: INCR use");
        session.close_window(id).unwrap();
    }
}

#[test]
#[ignore = "needs DISPLAY and PyGObject GTK 3; run with --ignored"]
fn kumimanu_reads_primary_text_from_gtk() {
    // PRIMARY is always the real atom, so this briefly replaces whatever
    // the desktop currently has selected; the peer releases it when it
    // exits. 1000 lines is about 9 KB, comfortably inside GTK's
    // single-property bound, so no INCR is involved.
    let lines = 1000;
    let child = gtk_owner(&["own-primary", &lines.to_string()]);
    let got = kumimanu_reads_primary();
    finish(child);
    let want = text_for(lines);
    assert_eq!(got.as_deref(), Some(want.as_str()), "primary text differs");
}

#[test]
#[ignore = "needs DISPLAY and PyGObject GTK 3; run with --ignored"]
fn gtk_reads_primary_text_from_kumimanu() {
    // This borrows the desktop PRIMARY for the test's duration: Kumimanu
    // owns the real atom until close_window releases it, and CLIPBOARD is
    // never touched. That is acceptable for a local display test.
    let lines = 1000;
    let want = text_for(lines);
    let (mut session, id) = default_window();
    assert!(session.set_primary_text(id, &want).expect("own"));
    let mut child = peer()
        .args(["read-primary"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("python3 with GTK 3");
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        // Kumimanu serves from its own event stream, so keep pumping.
        let _ = session
            .poll_events(Duration::from_millis(10))
            .expect("poll");
        if let Some(status) = child.try_wait().expect("peer") {
            break status;
        }
        assert!(Instant::now() < deadline, "GTK did not finish");
    };
    assert!(status.success(), "GTK read no primary text ({status})");
    let got = reader.join().unwrap().expect("peer stdout");
    assert_eq!(got.len(), want.len());
    assert!(got == want.as_bytes(), "primary text differs");
    session.close_window(id).unwrap();
}
