//! `viuda-pack` — package a `.gscene` with the generic Viuda runtime as a
//! native webOS app, and optionally install it on a Developer Mode TV.
//!
//! ```text
//! viuda-pack --scene app.gscene --runtime target/…/viuda --id com.x.y \
//!            --title Title [--version 0.1.0] [--out DIR]
//!            [--deploy HOST [--key ~/.ssh/webos_tv_open] [--launch]]
//! ```
//!
//! The package holds four files, and only `scene.gscene` and
//! `appinfo.json` differ between apps:
//! * `viuda`: the runtime, byte for byte as given (mode 0755);
//! * `scene.gscene`: the source, unchanged;
//! * `appinfo.json`: `"type": "native"`, `"main": "viuda"`;
//! * `icon.png`.
//!
//! The scene is loaded once here with the same `Viuda::load` the TV runs,
//! so a source the runtime would refuse is refused before packaging.
//! Nothing produced by that load is packaged (VIU-G02).

use std::path::PathBuf;
use std::process::ExitCode;

use arachnid::appinfo::{json_string, AppInfo};
use arachnid::archive::png_rgb;
use arachnid::deploy::{deploy, Device};
use arachnid::ipk::package_with_executables;
use goya_viuda::{system_font, Viuda};

/// The native-app `appinfo.json`, with a fixed key order.
fn native_appinfo(app: &AppInfo) -> String {
    let mut fields = vec![
        ("id", json_string(&app.id)),
        ("title", json_string(&app.title)),
        ("type", json_string("native")),
        ("main", json_string("viuda")),
        ("icon", json_string("icon.png")),
        ("version", json_string(&app.version)),
    ];
    if let Some(vendor) = &app.vendor {
        fields.push(("vendor", json_string(vendor)));
    }
    let body: Vec<String> = fields.iter().map(|(k, v)| format!("  \"{k}\": {v}")).collect();
    format!("{{\n{}\n}}\n", body.join(",\n"))
}

/// An 80×80 icon: a dark tile with a light inset square.
fn icon() -> Vec<u8> {
    let mut rgb = Vec::with_capacity(80 * 80 * 3);
    for y in 0..80u32 {
        for x in 0..80u32 {
            let inside = (20..60).contains(&x) && (20..60).contains(&y);
            rgb.extend_from_slice(if inside { &[0xf4, 0xf1, 0xea] } else { &[0x2b, 0x2d, 0x42] });
        }
    }
    png_rgb(80, 80, &rgb)
}

fn run() -> Result<(), String> {
    let mut scene = None;
    let mut runtime = None;
    let mut id = None;
    let mut title = None;
    let mut version = "0.1.0".to_owned();
    let mut out = PathBuf::from(".");
    let mut host = None;
    let mut key = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".ssh/webos_tv_open");
    let mut launch = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--scene" => scene = Some(PathBuf::from(value()?)),
            "--runtime" => runtime = Some(PathBuf::from(value()?)),
            "--id" => id = Some(value()?),
            "--title" => title = Some(value()?),
            "--version" => version = value()?,
            "--out" => out = PathBuf::from(value()?),
            "--deploy" => host = Some(value()?),
            "--key" => key = PathBuf::from(value()?),
            "--launch" => launch = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let scene = scene.ok_or("--scene is required")?;
    let runtime = runtime.ok_or("--runtime is required")?;
    let app = AppInfo {
        id: id.ok_or("--id is required")?,
        title: title.ok_or("--title is required")?,
        version,
        vendor: Some("Goya".into()),
    };
    app.validate().map_err(|d| {
        d.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("; ")
    })?;

    let source = std::fs::read_to_string(&scene).map_err(|e| format!("{}: {e}", scene.display()))?;
    let (font, _) = system_font()?;
    let checked = Viuda::load(&source, font, 1920, 1080)?;
    let runtime_bytes = std::fs::read(&runtime).map_err(|e| format!("{}: {e}", runtime.display()))?;
    if runtime_bytes.get(..4) != Some(b"\x7fELF") {
        return Err(format!("{} is not an ELF executable", runtime.display()));
    }

    let files = vec![
        ("appinfo.json".to_owned(), native_appinfo(&app).into_bytes()),
        ("icon.png".to_owned(), icon()),
        ("scene.gscene".to_owned(), source.into_bytes()),
        ("viuda".to_owned(), runtime_bytes),
    ];
    let ipk = package_with_executables(&app, &files, &["viuda"]).map_err(|e| format!("{e:?}"))?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(&ipk.file_name);
    std::fs::write(&path, &ipk.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "{} bytes={} contract={} actions={}",
        path.display(),
        ipk.bytes.len(),
        checked.report().contract,
        checked.report().actions
    );

    if let Some(host) = host {
        let control = std::env::temp_dir().join(format!("viuda-ssh-{}", std::process::id()));
        let device = Device { host, key, control };
        deploy(&device, &path, &app.id, false)?;
        println!("installed {}", app.id);
        if launch {
            println!("launch {}", launch_native(&device, &app.id)?);
        }
    }
    Ok(())
}

/// Launch a native app and return SAM's reply.
///
/// Arachnid's launch step sends `luna-send-pub -n 1` in the foreground of
/// the SSH session, straight after install. On 25 November 2026 that
/// returned and started nothing for a native app, three times. Sent from a
/// background job that is given time to receive its reply, the same call
/// started the app each time.
fn launch_native(device: &Device, id: &str) -> Result<String, String> {
    let payload = format!("{{\"id\":{}}}", json_string(id));
    let reply = format!("/tmp/viuda-launch-{}.txt", std::process::id());
    let script = format!(
        "sleep 2; (timeout 15 luna-send-pub -n 1 luna://com.webos.applicationManager/launch {} > {reply} 2>&1 &); \
         for i in 1 2 3 4 5 6 7 8 9 10; do sleep 1; test -s {reply} && break; done; cat {reply}; rm -f {reply}",
        arachnid::deploy::sh_quote(&payload)
    );
    let out = std::process::Command::new("ssh")
        .args(device.ssh_args())
        .arg("--")
        .arg(script)
        .output()
        .map_err(|e| format!("ssh: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if text.contains("\"returnValue\":true") {
        Ok(text)
    } else {
        Err(format!("launch refused or unanswered: {text}"))
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("viuda-pack: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appinfo_is_native_with_the_runtime_as_main() {
        let app = AppInfo {
            id: "com.goya.viuda.pairs".into(),
            title: "Goya Pairs".into(),
            version: "0.1.0".into(),
            vendor: None,
        };
        let json = native_appinfo(&app);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        assert_eq!(v["type"], "native");
        assert_eq!(v["main"], "viuda");
        assert_eq!(v["id"], "com.goya.viuda.pairs");
    }
}
