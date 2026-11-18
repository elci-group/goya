//! Makes `-lxkbcommon` resolvable for this crate and every crate that links it.
//!
//! The `xkbcommon` crate passes `-lxkbcommon` to the linker. That name needs
//! the unversioned development symlink. Debian and Ubuntu can install only
//! `libxkbcommon.so.0` until `libxkbcommon-dev` is present. Cargo propagates
//! `rustc-link-search` from this package to dependents such as `goya-document`
//! and `zcode-monitor`.

fn main() {
    #[cfg(unix)]
    expose_unversioned_xkbcommon();
}

#[cfg(unix)]
fn expose_unversioned_xkbcommon() {
    use std::fs;
    use std::path::{Path, PathBuf};

    let unversioned = [
        "/usr/lib/x86_64-linux-gnu/libxkbcommon.so",
        "/lib/x86_64-linux-gnu/libxkbcommon.so",
        "/usr/lib/aarch64-linux-gnu/libxkbcommon.so",
        "/lib/aarch64-linux-gnu/libxkbcommon.so",
        "/usr/lib64/libxkbcommon.so",
        "/usr/lib/libxkbcommon.so",
    ];
    if unversioned.iter().any(|path| Path::new(path).is_file()) {
        return;
    }

    let sonames = [
        "/usr/lib/x86_64-linux-gnu/libxkbcommon.so.0",
        "/lib/x86_64-linux-gnu/libxkbcommon.so.0",
        "/usr/lib/aarch64-linux-gnu/libxkbcommon.so.0",
        "/lib/aarch64-linux-gnu/libxkbcommon.so.0",
        "/usr/lib64/libxkbcommon.so.0",
        "/usr/lib/libxkbcommon.so.0",
    ];
    let Some(source) = sonames.iter().find(|path| Path::new(path).is_file()) else {
        return;
    };

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let link_dir = out_dir.join("native-link");
    fs::create_dir_all(&link_dir).expect("create native-link");
    let dest = link_dir.join("libxkbcommon.so");
    let _ = fs::remove_file(&dest);
    std::os::unix::fs::symlink(source, &dest).unwrap_or_else(|err| {
        panic!("failed to expose {source} as {}: {err}", dest.display());
    });
    println!("cargo:rustc-link-search=native={}", link_dir.display());
}
