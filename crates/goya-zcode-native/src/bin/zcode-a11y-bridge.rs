//! Native GTK main-thread helper; stdin snapshots, stdout typed actions.
use goya_a11y::gtk_atspi::{AtAction, GtkAtspiBridge};
use goya_zcode_native::accessibility_bridge::{
    decode_snapshot, AccessibleAction, MAX_SNAPSHOT_BYTES,
};
use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (updates_tx, updates_rx) = mpsc::sync_channel(2);
    let reader = std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        loop {
            let mut bytes = Vec::new();
            let result = std::io::Read::take(&mut input, MAX_SNAPSHOT_BYTES as u64 + 1)
                .read_until(b'\n', &mut bytes);
            if !matches!(result, Ok(size) if size > 0)
                || bytes.len() > MAX_SNAPSHOT_BYTES
                || !bytes.ends_with(b"\n")
            {
                return;
            }
            let Ok(tree) = decode_snapshot(&bytes) else {
                return;
            };
            if updates_tx.send(tree).is_err() {
                return;
            }
        }
    });
    let first = updates_rx.recv_timeout(Duration::from_secs(5))?;
    let (actions_tx, actions_rx) = mpsc::sync_channel(64);
    let writer = std::thread::spawn(move || {
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        while let Ok(action) = actions_rx.recv() {
            if serde_json::to_writer(&mut output, &action).is_err()
                || output
                    .write_all(b"\n")
                    .and_then(|_| output.flush())
                    .is_err()
            {
                return;
            }
        }
    });
    GtkAtspiBridge::new(first, "Goya native accessibility").run_with_updates(
        updates_rx,
        move |id, action| {
            let action = match action {
                AtAction::Press => AccessibleAction::Press { id },
                AtAction::SetValue(value) => AccessibleAction::SetValue { id, value },
            };
            if actions_tx.try_send(action).is_err() {
                eprintln!("native accessibility action transport overloaded or closed");
                std::process::exit(1);
            }
        },
    )?;
    // stdin can still be open when the user closes the mirror. The parent
    // owns process shutdown; do not wait on its blocking stdin reader here.
    drop(reader);
    drop(writer);
    Ok(())
}
