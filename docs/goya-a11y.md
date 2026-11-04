# Goya GTK/AT-SPI bridge

`goya-a11y` can export its `A11yTree` through GTK3's ATK implementation and
the desktop AT-SPI bridge. Enable the optional `atspi-gtk` feature on a Linux
host with GTK 3.24, AT-SPI2, and a session D-Bus. The feature is optional so
headless and non-GTK consumers keep the platform-neutral tree without GTK
dependencies.

```toml
goya-a11y = { version = "0.1", features = ["atspi-gtk"] }
```

```rust,ignore
use goya_a11y::gtk_atspi::{AtAction, GtkAtspiBridge};

let bridge = GtkAtspiBridge::new(current_a11y_tree, "Calculator");
bridge.run(|node_id, action| match action {
    AtAction::Press => runtime.invoke_accessible(&node_id),
    AtAction::SetValue(value) => runtime.set_accessible_value(&node_id, value),
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

The bridge creates GTK labels, buttons, menu items, entries, and sliders from
the projected DIG. GTK exposes these through ATK/AT-SPI; keyboard focus follows
the projected tree's stable node order, disabled nodes are insensitive, and
activation/value callbacks carry the stable Goya node id back to the host.
Only nodes in the exported A11yTree are exposed, so hidden DIG nodes remain
hidden from assistive technology.

The bridge currently publishes a snapshot. After a state change, the host
must rebuild the tree and reopen/update the bridge to refresh enabled and
focused state. The semantic projection and contrast helpers do not by
themselves establish WCAG conformance. A release conformance run must also
exercise each supported screen reader, keyboard traversal, focus changes,
announced values, and contrast against actual rendered colors.

## Local verification

Run unit tests and compile the GTK adapter with:

```sh
cargo test -p goya-a11y
cargo check -p goya-a11y --features atspi-gtk
```

For an interactive Linux check, launch a host that constructs
`GtkAtspiBridge` inside a desktop session, then navigate its controls using
Orca. A D-Bus/AT-SPI registry and accessible GTK session are required; a
headless unit test does not substitute for that check.

The sample tree can be launched in a disposable X11 and D-Bus session with:

```sh
xvfb-run -a dbus-run-session -- cargo run -p goya-a11y --features atspi-gtk --example atspi_demo
```

Close the sample window to exit. The example logs button and text-entry
callbacks; it is a bridge smoke test, not a full Orca conformance report.
