use goya_window_management::{linux::{LinuxWindowManager, LinuxWindowManagerKind}, WindowAction, WindowManager};
fn main() {
    let action = match std::env::args().nth(1).as_deref() { Some("close") => WindowAction::Close, Some("maximize") => WindowAction::Maximize, Some("restore") => WindowAction::Restore, _ => { eprintln!("usage: goya-window-floating <close|maximize|restore>"); std::process::exit(2); } };
    let Some(mut manager) = LinuxWindowManager::detect() else { eprintln!("no supported Linux window manager detected"); std::process::exit(1); };
    if manager.kind() != LinuxWindowManagerKind::Floating { eprintln!("detected desktop is not a floating/X11 manager"); std::process::exit(1); }
    if let Err(error) = manager.request(action) { eprintln!("{error:?}"); std::process::exit(1); }
}
