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

`run` publishes the initial snapshot. For live state propagation, create a
channel and call `run_with_updates`; send a newly projected tree after a DIG
change. The GTK main loop coalesces queued snapshots and applies the latest
one to the existing window, including changed names, enabled state, tree
structure, and focus. Focus is restored to the node marked focused in each
snapshot. Dropping the sender stops the refresh source cleanly. Use a bounded
channel if updates can arrive faster than GTK can render them.

Action callbacks continue to carry stable node ids. Buttons and menu items
emit `Press`; entries and sliders emit `SetValue` as their values change. The
host remains responsible for applying these actions to its runtime and
publishing a new snapshot when resulting state changes. The semantic
projection and contrast helpers do not by themselves establish WCAG
conformance. A release conformance run must also exercise each supported
screen reader, keyboard traversal, focus changes, announced values, and
contrast against actual rendered colors.

## Local verification

Run unit tests and compile the GTK adapter with:

```sh
cargo test -p goya-a11y
cargo check -p goya-a11y --features atspi-gtk
```

For an interactive Linux check, launch a host that constructs
`GtkAtspiBridge` inside a desktop session, then navigate its controls using
Orca. A D-Bus/AT-SPI registry and accessible GTK session are required; a
headless unit test does not substitute for that check. Local availability of
Orca and successful smoke navigation still do not establish WCAG conformance
or replace release testing on each supported desktop and screen reader.

For a manual Orca check, use an active graphical user session with its
systemd user manager and AT-SPI bus, then start the sample in Xvfb:

```sh
xvfb-run -a sh -c '
  cargo run -p goya-a11y --features atspi-gtk --example atspi_demo &
  app=$!
  orca --replace &
  screen_reader=$!
  wait "$app"
  kill "$screen_reader" 2>/dev/null || true
'
```

The example logs button and text-entry callbacks; this is a bridge smoke test,
not a full Orca conformance report.

On a systemd-integrated Linux desktop, `dbus-run-session` alone may not be
sufficient: `/usr/share/dbus-1/services/org.a11y.Bus.service` delegates to
the `at-spi-dbus-bus.service` user unit. In an isolated session without the
user systemd manager, activation can fail with `Could not activate remote
peer: unit failed` and no AT-SPI desktop path. In the current Xvfb smoke, the
existing desktop bus listed Orca and the sample application. A PID-matched,
recursive AT-SPI traversal reported the application root as `atspi_demo` /
`application`, the window as `Goya AT-SPI demo` / `frame`, and semantic
children including `Search` / `text`, `Ready` / `label`, and `Submit` /
`button`. An earlier query reported only `Close` because it queried a numeric
AT-SPI object path directly; those paths are transient, and the traversal
parser also missed later children because `gdbus` omits the repeated
`objectpath` annotation in tuple output. The corrected query confirmed the
semantic controls are present. This verifies registry exposure only; Orca
navigation, focus behavior, action dispatch, and value announcements remain
external gates, along with rendered contrast and evaluation on each supported
screen reader. No WCAG conformance claim follows from this smoke.
