//! Linked-artifact reproduction gate.
//!
//! The six win10-floor SHA-256 pairs in `docs/sash/programme/sash-ent-002.json`
//! (the `*_x64_repro_pair` and `*_architecture_stamp` gates) were reproduced
//! byte-for-byte by the IR-refactored compiler on 2026-11-18. The four
//! win8-floor pairs were first linked on 2026-11-18 at this working-tree
//! state and pinned the same way; record them in the next programme record.
//!
//! The pins cover the whole pipeline — admission, IR lowering, code
//! generation, the Windows cross toolchain, and the link — so a deliberate
//! toolchain bump or an emitter change will fail here by design. When that
//! happens on purpose, produce a new programme record and update the table
//! below; do not loosen the assertion.
//!
//! Run explicitly because each case performs a full cross-link:
//!
//! ```text
//! cargo test -p sash --offline --test link_reproduction -- --ignored --nocapture
//! ```
//!
//! The test skips itself (pass) when this host lacks the Windows cross
//! toolchain; skipping is reported in the output so a silent skip on a host
//! that should link is visible.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use sash::cli;

/// One recorded link: fixture, triple, API floor, and the SHA-256 pair
/// (exe, pdb).
struct RecordedLink {
    key: &'static str,
    scene_path: &'static str,
    triple: &'static str,
    floor: &'static str,
    exe_sha256: &'static str,
    pdb_sha256: &'static str,
}

const RECORDED_LINKS: &[RecordedLink] = &[
    // win10 floor: recorded in sash-ent-002.json, reproduced 2026-11-18.
    RecordedLink {
        key: "static-x64",
        scene_path: "fixtures/static-window.gscene",
        triple: "x86_64-pc-windows-msvc",
        floor: "win10",
        exe_sha256: "b60cd84ab0daf87681902eccef8d55b76acfbec619804d03786e3646eb16f558",
        pdb_sha256: "db437db47a3f77606d8cc5020cf123e748ed46203936c33de5ed8383bad84d51",
    },
    RecordedLink {
        key: "static-i686",
        scene_path: "fixtures/static-window.gscene",
        triple: "i686-pc-windows-msvc",
        floor: "win10",
        exe_sha256: "ab0078ca6ef225e0eb18aaca5024979937476ea2ceab8adec836f21685ab27b3",
        pdb_sha256: "fb95594850f7044df2ba2813940003e9dd06c6416cc6b817c9295641f6fe8f83",
    },
    RecordedLink {
        key: "static-arm64",
        scene_path: "fixtures/static-window.gscene",
        triple: "aarch64-pc-windows-msvc",
        floor: "win10",
        exe_sha256: "743d2443e5d28d79c9ad5246cf96de1173f9fa3d4e48fa0e53a65af7c9717067",
        pdb_sha256: "13dbee124823f973a8199cbc0a8e06d5ee39b88bec5bb9498ed9875107a16990",
    },
    RecordedLink {
        key: "interactive-x64",
        scene_path: "fixtures/interactive-counter.gscene",
        triple: "x86_64-pc-windows-msvc",
        floor: "win10",
        exe_sha256: "97dc90d8bab50709ddcd3ec7a4468ca596f348c5a52df8cc53b9091957512ba4",
        pdb_sha256: "efc82903213d7fc8376551a3e8a5731bbd88e30fcd330cef83abb92c68291071",
    },
    RecordedLink {
        key: "interactive-i686",
        scene_path: "fixtures/interactive-counter.gscene",
        triple: "i686-pc-windows-msvc",
        floor: "win10",
        exe_sha256: "68e927a104e0bb0994eb6101d68a1aace6f567e49459f3a244af91c02490db00",
        pdb_sha256: "29d76e37c5dd1dcf9e5c5b6a02a789eff1722e5afe24b8dc4d3883d62d25a5d1",
    },
    RecordedLink {
        key: "interactive-arm64",
        scene_path: "fixtures/interactive-counter.gscene",
        triple: "aarch64-pc-windows-msvc",
        floor: "win10",
        exe_sha256: "9296068568e9f0ba3ff1d84a69a451ae3645dbc53e8a0a3025975876c89fddf1",
        pdb_sha256: "74bc935830d1e97f62881f53d1a49f5880644e94a03126834ee0711848479b12",
    },
    // win8 floor: first linked 2026-11-18. ARM64 is refused on this floor
    // because ARM64 Windows did not exist before Windows 10.
    RecordedLink {
        key: "legacy-static-x64",
        scene_path: "fixtures/static-window.gscene",
        triple: "x86_64-pc-windows-msvc",
        floor: "win8",
        exe_sha256: "dae65289a837d9417bf90198dc1265a2fd68d92c1ee6c01255d7507a5ffa5d81",
        pdb_sha256: "8662c3e3c7eceb6aa2580239394eab6bc9f49a776501a5cbf0c94db414e43ab4",
    },
    RecordedLink {
        key: "legacy-static-i686",
        scene_path: "fixtures/static-window.gscene",
        triple: "i686-pc-windows-msvc",
        floor: "win8",
        exe_sha256: "409f815c232ecb0d33a6cc36bf87db38033d844067d4e768e59e4471353d4960",
        pdb_sha256: "e297010bf5733eb18ba1f869383b0fe63e858d94f726a20dc6e9c82a463cf18d",
    },
    RecordedLink {
        key: "legacy-interactive-x64",
        scene_path: "fixtures/interactive-counter.gscene",
        triple: "x86_64-pc-windows-msvc",
        floor: "win8",
        exe_sha256: "0773897d556c1e5625a44874f51aad18e30cb25ecdd166ba7c0e93e970011740",
        pdb_sha256: "cbda448db561f6acd1a6f37b8a50b355df371b06ee46cae4af08f85025b13eed",
    },
    RecordedLink {
        key: "legacy-interactive-i686",
        scene_path: "fixtures/interactive-counter.gscene",
        triple: "i686-pc-windows-msvc",
        floor: "win8",
        exe_sha256: "79fa5384c2f8afa21f2de4e5e6072f943655df785e91ee84775ee54d0b1f3aeb",
        pdb_sha256: "194b8637365c96c63e4384ae5c247c07e6ff82bbec5ad69543768eef54771f1c",
    },
];

/// The host can link only when every piece of the pinned cross toolchain is
/// present. Missing pieces skip the test rather than fail it, because the
/// compiler contract is that Sash never downloads a target or an SDK.
fn host_can_link() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some("this is not a Linux cross-link host".to_string());
    }
    let sysroot = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !sysroot.status.success() {
        return None;
    }
    let sysroot = String::from_utf8_lossy(&sysroot.stdout).trim().to_string();
    for triple in [
        "x86_64-pc-windows-msvc",
        "i686-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
    ] {
        if !std::path::Path::new(&sysroot)
            .join("lib/rustlib")
            .join(triple)
            .is_dir()
        {
            return Some(format!("rustc standard library for {triple} is not installed"));
        }
    }
    let xwin = Command::new("cargo").args(["xwin", "--version"]).output().ok()?;
    if !xwin.status.success() {
        return Some("cargo-xwin is not installed".to_string());
    }
    let rc = Command::new("llvm-rc").arg("/?").output().ok()?;
    if !rc.status.success() {
        return Some("llvm-rc is not installed".to_string());
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let done = std::path::Path::new(&home)
            .join(".cache/xwin/windows-msvc-sysroot/DONE");
        if !done.is_file() {
            return Some("the pinned windows-msvc-sysroot is not marked DONE".to_string());
        }
    } else {
        return Some("HOME is not set".to_string());
    }
    None
}

fn link_and_check(link: &RecordedLink) -> Result<(), String> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let scene = std::path::Path::new(manifest).join(link.scene_path);
    let out = temp_out(&format!("sash-link-repro-{}", link.key));
    let _ = fs::remove_dir_all(&out);

    let args = vec![
        "sash".into(),
        "build".into(),
        scene.into_os_string(),
        "--profile".into(),
        "portable-native".into(),
        "--release".into(),
        "--api-floor".into(),
        link.floor.into(),
        "--out".into(),
        out.clone().into_os_string(),
        "--target".into(),
        link.triple.into(),
    ];
    let run = cli::execute(&args);
    if run.code != std::process::ExitCode::SUCCESS {
        return Err(format!("link failed for {}\n{}", link.key, run.text));
    }

    let exe = fs::read(out.join("App.exe"))
        .map_err(|error| format!("{}: linked App.exe was not promoted: {error}", link.key))?;
    let pdb = fs::read(out.join("App.pdb"))
        .map_err(|error| format!("{}: linked App.pdb was not promoted: {error}", link.key))?;
    let exe_hash = sash::sha256_hex(&exe);
    let pdb_hash = sash::sha256_hex(&pdb);
    if exe_hash != link.exe_sha256 {
        return Err(format!(
            "{}: App.exe {} does not match the recorded {}",
            link.key, exe_hash, link.exe_sha256
        ));
    }
    if pdb_hash != link.pdb_sha256 {
        return Err(format!(
            "{}: App.pdb {} does not match the recorded {}",
            link.key, pdb_hash, link.pdb_sha256
        ));
    }

    // The fresh image must also verify: PE PASS, runtime and release BLOCKED.
    let args = vec![
        "sash".into(),
        "verify".into(),
        out.join("App.exe").into_os_string(),
        "--evidence".into(),
        out.join("evidence").into_os_string(),
    ];
    let verified = cli::execute(&args);
    if verified.code != std::process::ExitCode::from(3) {
        return Err(format!(
            "{}: verify exited {:?}, expected 3 (PE PASS, qualification BLOCKED)\n{}",
            link.key, verified.code, verified.text
        ));
    }
    if verified.json["pe_structure"] != "PASS" {
        return Err(format!("{}: verify did not report pe_structure PASS", link.key));
    }
    if verified.json["target"] != link.triple {
        return Err(format!(
            "{}: verify reported target {}, expected {}",
            link.key, verified.json["target"], link.triple
        ));
    }
    let expected_profile = if link.floor == "win8" {
        if link.scene_path.contains("interactive") {
            "sash.win32.interactive-legacy/0.0.2"
        } else {
            "sash.win32.static-window-legacy/0.0.1"
        }
    } else if link.scene_path.contains("interactive") {
        "sash.win32.interactive/0.0.2"
    } else {
        "sash.win32.static-window/0.0.3"
    };
    if verified.json["presentation_profile"] != expected_profile {
        return Err(format!(
            "{}: verify reported presentation profile {}, expected {}",
            link.key, verified.json["presentation_profile"], expected_profile
        ));
    }
    let expected_floor = if link.floor == "win8" { "win8" } else { "win10" };
    if verified.json["api_floor"] != expected_floor {
        return Err(format!(
            "{}: verify reported api floor {}, expected {}",
            link.key, verified.json["api_floor"], expected_floor
        ));
    }
    println!(
        "{}: reproduced {} ({}, {})",
        link.key,
        &link.exe_sha256[..16],
        link.triple,
        link.floor
    );
    let _ = fs::remove_dir_all(&out);
    Ok(())
}

fn temp_out(key: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{key}-{}", std::process::id()))
}

#[test]
#[ignore = "performs ten full Windows cross-links; run with --ignored"]
fn linked_artifacts_reproduce_the_recorded_hashes() {
    let Some(reason) = host_can_link() else {
        println!("toolchain present; linking all ten recorded artifacts");
        for link in RECORDED_LINKS {
            if let Err(error) = link_and_check(link) {
                panic!("{error}");
            }
        }
        println!("all ten recorded links reproduced byte-for-byte");
        return;
    };
    println!(
        "SKIPPED: this host cannot link ({reason}). Sash never downloads a target or an SDK; \
         restore the pinned cross toolchain to run the reproduction gate."
    );
}