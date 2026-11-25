//! Process entry for one Emperor scene app (`emperor.linux.scene/0.0.1`).
//!
//! * no arguments, or `--live`: open the Wayland window (with AccessKit);
//! * `--render PATH [--keys KEYS] [--size WxH]`: render one frame to a PNG
//!   without a compositor, after an optional key script;
//! * `--exercise KEYS [--size WxH]`: run the key script headless and print
//!   the state, the last D9 announcement and any errors as JSON;
//! * `--tree`: print the accessibility tree's buttons and labels.
//!
//! Keys: `L R U D` arrows, `O` activate (Enter), `0`–`9` digits.
//! Exit 0 on success, 1 on usage, 5 on runtime failure.

#![forbid(unsafe_code)]

use emperor_runtime::scene::{Scene, SceneSpec, A11y};
use std::process::ExitCode;

include!(concat!(env!("OUT_DIR"), "/embed.rs"));

fn size(args: &[String]) -> (u32, u32) {
    args.iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((1280, 720))
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn script(scene: &mut Scene, keys: &str) {
    for k in keys.chars() {
        match k {
            'L' => scene.move_focus(-1, 0),
            'R' => scene.move_focus(1, 0),
            'U' => scene.move_focus(0, -1),
            'D' => scene.move_focus(0, 1),
            'O' => scene.activate_focused(),
            d @ '0'..='9' => scene.press_digit(d as u8 - b'0'),
            _ => {}
        }
        while scene.tick() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let spec: &SceneSpec = &SCENE_SPEC;
    let (w, h) = size(&args);
    let mut scene = match Scene::new(spec, w, h) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("emperor scene: {e}");
            return ExitCode::from(5);
        }
    };
    if let Some(keys) = arg(&args, "--exercise") {
        script(&mut scene, keys);
        let state: Vec<String> = scene
            .state()
            .iter()
            .map(|(k, v)| format!("{}: {}", json_string(k), match v {
                goya_executable::D1Value::Int(i) => i.to_string(),
                goya_executable::D1Value::Text(t) => json_string(t),
                goya_executable::D1Value::Bool(b) => b.to_string(),
            }))
            .collect();
        println!(
            "{{\"contract\": \"{}\", \"state\": {{{}}}, \"announcement\": {}, \"errors\": [{}]}}",
            emperor_runtime::scene::CONTRACT,
            state.join(", "),
            json_string(&scene.last_announcement),
            scene.errors.iter().map(|e| json_string(e)).collect::<Vec<_>>().join(", ")
        );
        return ExitCode::SUCCESS;
    }
    if let Some(path) = arg(&args, "--render") {
        if let Some(keys) = arg(&args, "--keys") {
            script(&mut scene, keys);
        }
        // Let launch animations (appear, graphic steps) finish first.
        while scene.tick() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        return match scene.render().save_png(path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("emperor scene: {e}");
                ExitCode::from(5)
            }
        };
    }
    if args.iter().any(|a| a == "--tree") {
        if let Some(keys) = arg(&args, "--keys") {
            script(&mut scene, keys);
        }
        let tree = scene.accessibility();
        for (id, node) in &tree.nodes {
            println!("{:>20} {:?} name={:?} description={:?} value={:?} live={:?}", id.0, node.role(), node.name(), node.description(), node.value(), node.live());
        }
        return ExitCode::SUCCESS;
    }
    live(scene, spec.title)
}

fn live(mut scene: Scene, title: &str) -> ExitCode {
    let (w, h) = size(&std::env::args().collect::<Vec<_>>());
    let mut window = match goya_wayland::GoyaWindow::create(title, w as i32, h as i32) {
        Ok(win) => win,
        Err(e) => {
            eprintln!("emperor scene: window: {e}");
            return ExitCode::from(5);
        }
    };
    let mut a11y = A11y::new(scene.accessibility());
    a11y.focus(true);
    let mut dirty = true;
    let mut last_ambient = std::time::Instant::now();
    loop {
        if window.close_requested() {
            break;
        }
        for event in window.poll() {
            use goya_wayland::{KeySpecial, PlatformEvent};
            let before = scene.last_announcement.clone();
            match event {
                PlatformEvent::Resized(nw, nh) => scene.resize(nw.max(1) as u32, nh.max(1) as u32),
                PlatformEvent::PointerPress(x, y) => scene.pointer(x, y),
                PlatformEvent::KeySpecial(KeySpecial::ArrowLeft) => scene.move_focus(-1, 0),
                PlatformEvent::KeySpecial(KeySpecial::ArrowRight) => scene.move_focus(1, 0),
                PlatformEvent::KeySpecial(KeySpecial::ArrowUp) => scene.move_focus(0, -1),
                PlatformEvent::KeySpecial(KeySpecial::ArrowDown) => scene.move_focus(0, 1),
                PlatformEvent::KeyChar('\n') | PlatformEvent::KeyChar(' ') => scene.activate_focused(),
                PlatformEvent::KeyChar(c @ '0'..='9') => scene.press_digit(c as u8 - b'0'),
                PlatformEvent::CloseRequested => return ExitCode::SUCCESS,
                _ => {}
            }
            dirty = true;
            if scene.last_announcement != before {
                a11y.update(&scene);
            }
        }
        for request in a11y.drain() {
            scene.accessibility_action(request.target.0, request.action);
            dirty = true;
        }
        if scene.tick() {
            dirty = true;
        }
        if scene.ambient() && last_ambient.elapsed().as_millis() >= 250 {
            last_ambient = std::time::Instant::now();
            dirty = true;
        }
        if dirty {
            let (ww, wh) = window.size();
            scene.resize(ww.max(1) as u32, wh.max(1) as u32);
            let frame = scene.render();
            if window.present_rgba(frame.data(), frame.width(), frame.height()).is_ok() {
                dirty = false;
                a11y.update(&scene);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
        window.pump(1);
    }
    ExitCode::SUCCESS
}
