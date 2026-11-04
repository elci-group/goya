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
peer: unit failed` and no AT-SPI desktop path.

### Observed Linux smoke (2026-11-04)

Environment: COSMIC Wayland session, GTK 3.24.41, AT-SPI 2.58.1, Orca 49.4,
session D-Bus and AT-SPI registry active. The `atspi_demo` process appeared
in the AT-SPI registry. Traversal exposed the window as `Goya AT-SPI demo` /
`frame` and controls as `Search` / `text`, `Ready` / `label`, and `Submit` /
`button`. Direct AT-SPI calls to `Component.GrabFocus` and
`EditableText.SetTextContents("query")` on Search returned true;
`Action.DoAction(0)` on Submit returned true. The host callback logged
`updated query: query` and `activated submit`.

This did **not** verify interactive keyboard traversal or Orca announcements:
Orca's debug log repeatedly reported `Locus of focus is None` during this
smoke. The bridge's direct focus request and action/value dispatch therefore
have evidence, while user-visible focus and screen-reader speech remain
unverified. This smoke establishes neither WCAG conformance nor successful
navigation by Orca.

### Release accessibility gates

Before making a WCAG conformance claim, select and document the applicable
WCAG version and conformance level, then retain evidence for every applicable
criterion on every shipped platform and input mode. At minimum, test each
declared OS/toolkit and its supported screen reader/browser combination; a
Linux AT-SPI pass cannot stand in for macOS VoiceOver, Windows Narrator or
NVDA/UI Automation, or mobile screen readers if those platforms ship. For each
combination, record OS/toolkit and assistive-technology versions, sample/app
revision, steps, expected and observed results, and failures.

The release run must cover:

- Keyboard-only traversal in both directions through all exposed controls;
  visible focus, initial and restored focus, no keyboard traps, and activation
  with the platform's expected keys.
- Screen-reader output for each control's name, role, enabled/disabled state,
  current value, value changes, and focus changes; verify labels remain
  available when placeholders disappear and dynamic state updates are
  announced usefully.
- Real actions and values in the host runtime, including button/menu
  activation, text editing, slider adjustment, and disabled controls that
  cannot be activated; check the resulting state is reflected back in the
  accessible tree without losing focus.
- Rendered visual checks on supported themes and display settings: text and
  non-text contrast, focus indicator visibility, text scaling/zoom, clipping
  and reflow at small windows, and any color/motion-dependent state cues.
- Touch or other non-keyboard input and platform-specific accessibility APIs
  wherever those are part of a shipped device experience.

Store a criterion-by-criterion report and representative screenshots or
recordings with the release evidence. Until those gates pass, describe the
GTK/AT-SPI bridge and the specific smoke evidence only; do not claim WCAG
conformance. Contrast helper unit tests do not replace checks of actual
rendered colors and all applicable WCAG criteria.
