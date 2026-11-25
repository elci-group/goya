//! A selected target scopes platform exceptions. It does not open a surface.

use std::process::Command;

use goya_resolver::{Resolver, Target};

const WINDOWS_ONLY: &str = r#"@ontology app {
  @cell root { id: "root"; }
  @platform windows { "media": cede; }
}
"#;

#[test]
fn served_ids_round_trip_and_unserved_names_do_not() {
    let ids: Vec<_> = Target::ALL.iter().map(|target| target.id()).collect();
    assert_eq!(
        ids,
        [
            "android",
            "android_tv",
            "browser",
            "ios",
            "linux",
            "windows"
        ]
    );
    for target in Target::ALL {
        assert_eq!(Target::parse(target.id()), Some(target));
        assert!(!target.compiler().is_empty());
        assert!(!target.surface().is_empty());
    }
    assert!(Target::parse("macos").is_none());
    assert!(Target::parse("webos").is_none());
    assert!(Target::parse("tvos").is_none());
}

#[test]
fn linux_ignores_a_windows_exception_and_a_linux_exception_fails() {
    let linux = Resolver::builder()
        .target(Target::Linux)
        .build()
        .expect("linux");
    let application = linux.load(WINDOWS_ONLY).expect("foreign rule");
    assert_eq!(application.target(), Some(Target::Linux));
    let owned = WINDOWS_ONLY.replace("windows", "linux");
    match linux.load(&owned) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-PLATFORM");
            assert_eq!(
                error.message,
                "unresolved linux platform exception for 'media'"
            );
            assert!(error.range.is_some());
        }
        Ok(_) => panic!("linux executed its own exception"),
    }
}

#[test]
fn cli_target_names_a_closed_surface() {
    let bin = env!("CARGO_BIN_EXE_goya-resolve");
    let scene = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sash/fixtures/interactive-live.gscene"
    );
    let ok = Command::new(bin)
        .args(["--target", "linux", "--dispatch", "action.add", scene])
        .output()
        .expect("run");
    let stdout = String::from_utf8(ok.stdout).unwrap();
    assert!(
        ok.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(stdout.contains("platform: linux\n"), "{stdout}");
    assert!(stdout.contains("compiler: emperor\n"), "{stdout}");
    assert!(stdout.contains("surface: wayland closed\n"), "{stdout}");
    assert!(stdout.contains("state count int 1 session"), "{stdout}");

    let unknown = Command::new(bin)
        .args(["--target", "macos", scene])
        .output()
        .expect("macos");
    assert_eq!(unknown.status.code(), Some(2));
    let stderr = String::from_utf8(unknown.stderr).unwrap();
    assert!(stderr.contains("unknown platform macos"), "{stderr}");

    let presented = Command::new(bin)
        .args([
            "--target",
            "linux",
            "--viewport",
            "320x160",
            "--dispatch",
            "action.add",
            "--present",
            scene,
        ])
        .output()
        .expect("present");
    let stdout = String::from_utf8(presented.stdout).unwrap();
    assert!(
        presented.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&presented.stderr)
    );
    assert!(
        stdout.contains("present linux wayland closed revision 1\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("node window.amount input amount 20000,60000,120000,30000 \"1\"\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("node window.total cell total 20000,110000,160000,30000 \"1\"\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("opened"), "{stdout}");

    let pointed = Command::new(bin)
        .args([
            "--target",
            "linux",
            "--viewport",
            "320x160",
            "--pointer",
            "100000,125000",
            "--pointer",
            "80000,75000",
            "--pointer",
            "200000,75000",
            scene,
        ])
        .output()
        .expect("pointer");
    let stdout = String::from_utf8(pointed.stdout).unwrap();
    assert!(
        pointed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&pointed.stderr)
    );
    assert!(stdout.contains("point miss\n"), "{stdout}");
    assert!(stdout.contains("point focus window.amount\n"), "{stdout}");
    assert!(
        stdout.contains("point activate add revision 1 affected window.total\n"),
        "{stdout}"
    );
    assert!(stdout.contains("state count int 1 session"), "{stdout}");

    let bare = Command::new(bin)
        .args(["--pointer", "0,0", scene])
        .output()
        .expect("bare pointer");
    assert_eq!(bare.status.code(), Some(2));
    assert!(String::from_utf8(bare.stderr)
        .unwrap()
        .contains("--pointer needs --target"));

    let entered = Command::new(bin)
        .args([
            "--target",
            "linux",
            "--viewport",
            "320x160",
            "--pointer",
            "80000,75000",
            "--enter",
            "4",
            "--pointer",
            "200000,75000",
            scene,
        ])
        .output()
        .expect("enter");
    let stdout = String::from_utf8(entered.stdout).unwrap();
    assert!(
        entered.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&entered.stderr)
    );
    assert!(stdout.contains("point focus window.amount\n"), "{stdout}");
    assert!(stdout.contains("enter window.amount\n"), "{stdout}");
    assert!(
        stdout.contains("point activate add revision 1 affected window.total\n"),
        "{stdout}"
    );
    assert!(stdout.contains("state count int 4 session"), "{stdout}");
    assert!(stdout.contains("field window.amount 4"), "{stdout}");

    let bare_enter = Command::new(bin)
        .args(["--enter", "4", scene])
        .output()
        .expect("bare enter");
    assert_eq!(bare_enter.status.code(), Some(2));
    assert!(String::from_utf8(bare_enter.stderr)
        .unwrap()
        .contains("--enter needs --target"));

    let unfocused = Command::new(bin)
        .args(["--target", "linux", "--enter", "4", scene])
        .output()
        .expect("unfocused enter");
    assert_eq!(unfocused.status.code(), Some(1));
    assert!(String::from_utf8(unfocused.stderr)
        .unwrap()
        .contains("no input is focused"));

    let scrolled = format!(
        "{}/tests/fixtures/scrolled-button.gscene",
        env!("CARGO_MANIFEST_DIR")
    );
    let wheeled = Command::new(bin)
        .args([
            "--target",
            "linux",
            "--scroll",
            "150000,150000,0,200000",
            "--pointer",
            "150000,175000",
            "--present",
            &scrolled,
        ])
        .output()
        .expect("scroll");
    let stdout = String::from_utf8(wheeled.stdout).unwrap();
    assert!(
        wheeled.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&wheeled.stderr)
    );
    assert!(stdout.contains("scroll viewport 0,130000\n"), "{stdout}");
    assert!(
        stdout.contains("point activate action revision 1 affected -\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("node action.button button action 120000,170000,180000,50000 \"go\""),
        "{stdout}"
    );
    assert!(stdout.contains("state count int 1 session"), "{stdout}");
    assert!(!stdout.contains("opened"), "{stdout}");

    let bare_scroll = Command::new(bin)
        .args(["--scroll", "0,0,0,1", &scrolled])
        .output()
        .expect("bare scroll");
    assert_eq!(bare_scroll.status.code(), Some(2));
    assert!(String::from_utf8(bare_scroll.stderr)
        .unwrap()
        .contains("--scroll needs --target"));

    let typed = Command::new(bin)
        .args([
            "--target",
            "linux",
            "--viewport",
            "320x160",
            "--pointer",
            "80000,75000",
            "--caret",
            "home",
            "--insert",
            "9",
            "--pointer",
            "200000,75000",
            scene,
        ])
        .output()
        .expect("insert");
    let stdout = String::from_utf8(typed.stdout).unwrap();
    assert!(
        typed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&typed.stderr)
    );
    assert!(stdout.contains("caret window.amount 0\n"), "{stdout}");
    assert!(stdout.contains("insert window.amount 1\n"), "{stdout}");
    assert!(
        stdout.contains("point activate add revision 1 affected window.total\n"),
        "{stdout}"
    );
    assert!(stdout.contains("state count int 91 session"), "{stdout}");
    assert!(stdout.contains("field window.amount 91"), "{stdout}");

    let deleted = Command::new(bin)
        .args([
            "--target",
            "linux",
            "--viewport",
            "320x160",
            "--pointer",
            "80000,75000",
            "--caret",
            "home",
            "--delete",
            scene,
        ])
        .output()
        .expect("delete");
    let stdout = String::from_utf8(deleted.stdout).unwrap();
    assert!(
        deleted.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&deleted.stderr)
    );
    assert!(stdout.contains("delete window.amount 0\n"), "{stdout}");
    assert!(stdout.contains("revision: 0\n"), "{stdout}");

    let bare_caret = Command::new(bin)
        .args(["--caret", "left", scene])
        .output()
        .expect("bare caret");
    assert_eq!(bare_caret.status.code(), Some(2));
    assert!(String::from_utf8(bare_caret.stderr)
        .unwrap()
        .contains("--caret needs --target"));
}
