//! Copies an optional embedded scene into `OUT_DIR/embed.rs` and an optional
//! baked presentation into `OUT_DIR/presentation.rs`.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=EMPEROR_EMBED");
    println!("cargo:rerun-if-env-changed=EMPEROR_PRESENTATION");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    expose_unversioned_xkbcommon(&out_dir);
    let dest = out_dir.join("embed.rs");
    match env::var("EMPEROR_EMBED") {
        Ok(path) if !path.is_empty() => {
            println!("cargo:rerun-if-changed={path}");
            fs::copy(&path, &dest).unwrap_or_else(|err| {
                panic!(
                    "failed to copy EMPEROR_EMBED {path} to {}: {err}",
                    dest.display()
                );
            });
        }
        _ => {
            fs::write(
                &dest,
                "pub fn embedded() -> Option<&'static emperor_runtime::CompiledScene> { None }\n",
            )
            .expect("write embed.rs");
        }
    }
    // The generic profile is the default. A desktop profile's colours are
    // resolved by the compiler and arrive here as generated Rust.
    let presentation = out_dir.join("presentation.rs");
    match env::var("EMPEROR_PRESENTATION") {
        Ok(path) if !path.is_empty() => {
            println!("cargo:rerun-if-changed={path}");
            fs::copy(&path, &presentation).unwrap_or_else(|err| {
                panic!(
                    "failed to copy EMPEROR_PRESENTATION {path} to {}: {err}",
                    presentation.display()
                );
            });
        }
        _ => {
            fs::write(
                &presentation,
                "pub fn presentation() -> &'static emperor_runtime::Presentation { &emperor_runtime::Presentation::GENERIC }\n",
            )
            .expect("write presentation.rs");
        }
    }
}

/// rustc links `xkbcommon` as `-lxkbcommon`, which needs the unversioned
/// development symlink. Some hosts install only `libxkbcommon.so.0`.
fn expose_unversioned_xkbcommon(out_dir: &std::path::Path) {
    let link_dir = out_dir.join("native-link");
    let dest = link_dir.join("libxkbcommon.so");
    let candidates = [
        "/usr/lib/x86_64-linux-gnu/libxkbcommon.so.0",
        "/lib/x86_64-linux-gnu/libxkbcommon.so.0",
        "/usr/lib/aarch64-linux-gnu/libxkbcommon.so.0",
        "/lib/aarch64-linux-gnu/libxkbcommon.so.0",
        "/usr/lib64/libxkbcommon.so.0",
        "/usr/lib/libxkbcommon.so.0",
    ];
    let Some(source) = candidates
        .iter()
        .find(|path| std::path::Path::new(path).is_file())
    else {
        return;
    };
    fs::create_dir_all(&link_dir).expect("create native-link");
    let _ = fs::remove_file(&dest);
    std::os::unix::fs::symlink(source, &dest).unwrap_or_else(|err| {
        panic!("failed to expose {source} as {}: {err}", dest.display());
    });
    println!("cargo:rustc-link-search=native={}", link_dir.display());
}
