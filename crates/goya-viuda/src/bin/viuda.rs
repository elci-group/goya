//! `viuda` — the generic Viuda runtime. The same binary runs every app.
//!
//! Packaged (`viuda-pack`), it sits beside `scene.gscene` and
//! `appinfo.json` in the app directory, and SAM starts it with no scene
//! argument. On the TV it opens a webOS surface. With `--headless` it
//! applies a key script without a compositor and prints timings, for
//! measurement over SSH and in CI.
//!
//! ```text
//! viuda [scene.gscene] [--size WxH] [--log FILE]
//! viuda [scene.gscene] --headless [--keys ok,right,…] [--size WxH]
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use goya_viuda::{system_font, Key, LoadReport, Viuda, CONTRACT};

struct Args {
    scene: Option<PathBuf>,
    headless: bool,
    keys: Vec<Key>,
    size: (u32, u32),
    log: Option<PathBuf>,
}

fn parse_key(s: &str) -> Result<Key, String> {
    Ok(match s {
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "ok" => Key::Ok,
        d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => Key::Digit(d.as_bytes()[0] - b'0'),
        other => return Err(format!("unknown key {other:?}")),
    })
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        scene: None,
        headless: false,
        keys: Vec::new(),
        size: (1920, 1080),
        log: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--headless" => args.headless = true,
            "--keys" => {
                let list = it.next().ok_or("--keys needs a list")?;
                args.keys = list.split(',').filter(|s| !s.is_empty()).map(parse_key).collect::<Result<_, _>>()?;
            }
            "--size" => {
                let v = it.next().ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size needs WxH")?;
                args.size = (
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                );
                if args.size.0 == 0 || args.size.1 == 0 || args.size.0 > 3840 || args.size.1 > 2160 {
                    return Err("--size must be within 1x1 and 3840x2160".into());
                }
            }
            "--log" => args.log = Some(it.next().ok_or("--log needs a file")?.into()),
            // SAM passes launch parameters as one JSON argument.
            p if p.starts_with('{') => {}
            p if p.starts_with("--") => return Err(format!("unknown option {p}")),
            p => args.scene = Some(p.into()),
        }
    }
    Ok(args)
}

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `id` and `title` from the packaged `appinfo.json`, if there is one.
fn appinfo(dir: &Path) -> (String, String) {
    let parsed = std::fs::read_to_string(dir.join("appinfo.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let field = |k: &str| {
        parsed
            .as_ref()
            .and_then(|v| v.get(k))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    };
    let id = std::env::var("APP_ID")
        .ok()
        .or_else(|| field("id"))
        .unwrap_or_else(|| "com.goya.viuda".into());
    let title = field("title").unwrap_or_else(|| "Viuda".into());
    (id, title)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn write_report(log: &mut dyn Write, font: &str, r: &LoadReport, total: Duration) {
    let _ = writeln!(log, "{CONTRACT} contract={} digest={}", r.contract, hex(&r.digest));
    let _ = writeln!(
        log,
        "load source_bytes={} actions={} resolve_ms={} presentation_ms={} graphics_ms={} total_ms={} font={font}",
        r.source_bytes,
        r.actions,
        r.resolve.as_millis(),
        r.presentation.as_millis(),
        r.graphics_compile.as_millis(),
        total.as_millis()
    );
    for g in &r.graphics {
        let _ = writeln!(
            log,
            "graphic {} bytes={} states={} projections={} sha256={}",
            g.name, g.bytes, g.states, g.projections, g.sha256
        );
    }
}

fn run() -> Result<(), String> {
    let started = Instant::now();
    let args = parse_args()?;
    let dir = app_dir();
    let (app_id, title) = appinfo(&dir);
    let scene = args.scene.clone().unwrap_or_else(|| dir.join("scene.gscene"));
    let mut log: Box<dyn Write> = match (&args.log, args.headless) {
        (Some(p), _) => Box::new(std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?),
        (None, true) => Box::new(std::io::stdout()),
        (None, false) => {
            // The native jail mounts the TV's /tmp read-write, and the
            // Developer Mode SSH user can read it. The jail user cannot
            // create files in /media/developer/temp (seen 25 November 2026).
            ["/tmp", "/media/developer/temp"]
                .iter()
                .find_map(|d| {
                    // One file per process: a reinstalled app may run as
                    // another jail user and cannot truncate the old log.
                    std::fs::File::create(format!("{d}/viuda-{app_id}-{}.log", std::process::id())).ok()
                })
                .map_or_else(|| Box::new(std::io::stderr()) as Box<dyn Write>, |f| Box::new(f))
        }
    };
    let _ = writeln!(
        log,
        "start pid={} exe_dir={} args={:?} wayland={:?} xdg={:?} home={:?}",
        std::process::id(),
        dir.display(),
        std::env::args().skip(1).collect::<Vec<_>>(),
        std::env::var("WAYLAND_DISPLAY").ok(),
        std::env::var("XDG_RUNTIME_DIR").ok(),
        std::env::var("HOME").ok()
    );
    let _ = log.flush();
    let source = std::fs::read_to_string(&scene).map_err(|e| format!("GY-VIUDA-IO: {}: {e}", scene.display()))?;
    let (font, font_path) = system_font()?;
    let (w, h) = args.size;
    let viuda = match Viuda::load(&source, font, w, h) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(log, "load-failed {e}");
            return Err(e);
        }
    };
    write_report(&mut *log, &font_path, viuda.report(), started.elapsed());

    if args.headless {
        return headless(viuda, &args.keys, &mut *log);
    }
    #[cfg(target_os = "linux")]
    {
        let opts = goya_viuda::webos::Options {
            app_id,
            title,
            width: w,
            height: h,
            ambient_interval: Duration::from_millis(100),
            started,
        };
        goya_viuda::webos::run(viuda, opts, &mut *log)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_id, title);
        Err("GY-VIUDA-HOST: only the webOS (Linux) host exists; use --headless".into())
    }
}

fn headless(mut viuda: Viuda, keys: &[Key], log: &mut dyn Write) -> Result<(), String> {
    let mut renders = Vec::new();
    let mut dispatches = Vec::new();
    let t = Instant::now();
    let _ = viuda.render();
    renders.push(t.elapsed().as_micros());
    for &key in keys {
        let out = viuda.key(key);
        if out.action.is_some() {
            dispatches.push(out.dispatch.as_micros());
        }
        let t = Instant::now();
        let _ = viuda.render();
        renders.push(t.elapsed().as_micros());
        let _ = writeln!(
            log,
            "key {key:?} action={} revision={}",
            out.action.as_deref().unwrap_or("-"),
            out.revision
        );
    }
    for e in &viuda.errors {
        let _ = writeln!(log, "error {e}");
    }
    let _ = writeln!(log, "revision {} inert {}", viuda.revision(), viuda.inert_activations);
    #[cfg(target_os = "linux")]
    {
        let _ = writeln!(log, "{}", goya_viuda::webos::summary("render", &renders));
        let _ = writeln!(log, "{}", goya_viuda::webos::summary("dispatch", &dispatches));
    }
    if viuda.errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} resolver errors", viuda.errors.len()))
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
