//! Explicit synthetic plugin presentation proof, without a Host service bridge.
use goya_plugin_host::{BrowserPluginSession, PluginInstance};
use goya_wayland::{GoyaWindow, PlatformEvent};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let executable = std::env::var_os("GOYA_CHROMIUM")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/bin/google-chrome"));
    // Initialize and capture before a native event loop exists; production
    // pane integration must run this blocking controller on its worker.
    let instance = PluginInstance::serve("synthetic.plugin", root, "synthetic-session")?;
    let mut browser = BrowserPluginSession::launch(instance, executable, 400, 240)?;
    let frame = browser.capture_frame()?;
    let mut window = GoyaWindow::create("Goya plugin fixture", 400, 240)?;
    let started = Instant::now();
    let mut presented = 0;
    while !window.close_requested() && started.elapsed() < Duration::from_secs(3) {
        if window
            .poll()
            .iter()
            .any(|event| matches!(event, PlatformEvent::CloseRequested))
        {
            break;
        }
        if window.size() == (400, 240) {
            window.present_rgba(&frame.rgba, frame.width, frame.height)?;
            presented += 1;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    browser.close();
    if presented == 0 {
        return Err("no plugin frame presented".into());
    }
    println!("native plugin presentation PASS: {presented} frames; Host RPC bridge remains unimplemented");
    Ok(())
}
