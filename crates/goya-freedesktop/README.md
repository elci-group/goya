# goya-freedesktop

Desktop-neutral freedesktop.org integration for Goya on Linux. Std only, no
dependencies. COSMIC-specific behaviour belongs in `nebula`, not here.

## Implemented

- `dbus`: blocking session-bus client.
  - Address from `DBUS_SESSION_BUS_ADDRESS`; `unix:path=` and (Linux)
    `unix:abstract=` entries, `%XX` unescaping, first usable entry of a
    `;`-separated list. Anything else is `Error::UnsupportedAddress`.
  - SASL `AUTH EXTERNAL <hex of decimal euid>`, `BEGIN`, then `Hello`.
  - Marshalling of `y b n q i u x t d s o g v`, arrays, dicts and structs.
    Writes little-endian; reads either endianness.
  - `Connection::call` waits for the `METHOD_RETURN`/`ERROR` whose
    reply serial matches and discards other messages. Reply timeout is
    configurable (default 2 s) and covers the whole call.
  - Parsing is bounds-checked: messages over 1 MiB are refused before
    allocation, nesting is capped at 64, decoded values per message are
    capped, padding must be zero, strings must be UTF-8 and NUL-terminated.
    Malformed input returns `Error::Malformed`/`InvalidSignature`/
    `MessageTooLarge`.
  - A timeout before any reply byte leaves the connection usable; a
    partial read or write marks it broken (`Error::Disconnected`).
- `notifications`: `server_information`, `capabilities`, `notify`
  (actions, `urgency` and optional `category` hints), `close`.
  `Card` + `card_to_notification` / `degrade_card`: actions are dropped
  when `actions` is not advertised, the body is dropped when `body` is not
  advertised, and `& < >` are escaped when `body-markup` is advertised
  (the card body is plain text). `degrade_card` reports what changed.
- `portal`: `read_appearance` reads `color-scheme`, `accent-color` and
  `contrast` from `org.freedesktop.appearance`, each key separately. It
  uses `ReadOne` and falls back to the deprecated `Read` (extra variant
  layer) after an `UnknownMethod` error. A D-Bus error or wrong type for
  one key marks only that key `KeySource::Unavailable`. Accent colours
  with any component outside [0, 1] (or NaN) are `None`.

## Not implemented

- Unix fd passing (`h`), signal subscription (`AddMatch`), so no
  `ActionInvoked`, `NotificationClosed` or `SettingChanged` handling.
- Serving objects, TCP / `autolaunch:` / `launchd:` transports, other SASL
  mechanisms, the system bus, `$XDG_RUNTIME_DIR/bus` fallback when the
  environment variable is unset.
- Notification image, sound and other hints; portal `ReadAll`; other
  portal namespaces.
- Async or concurrent use: one `Connection` handles one call at a time.

## Evidence

`cargo test -p goya-freedesktop --offline` runs unit tests: marshalling
round-trips, exact byte layouts for alignment and array padding, malformed
input (every truncation, every single-bit flip, 20 000 random mutations of a
valid reply), a fake bus over a socketpair (SASL, unrelated-message
skipping, error replies, timeouts, oversized frames), Card degradation and
the portal fallback paths.

Live tests are `#[ignore]`d:

```
cargo test -p goya-freedesktop --offline -- --ignored --nocapture
```

On 2026-11-19, in a COSMIC session on Pop!_OS 24.04, these printed
`ServerInfo { name: "cosmic-notifications", vendor: "System76",
version: "0.1.0", spec_version: "1.2" }`, capabilities `["body",
"icon-static", "persistence", "actions", "action-icons", "body-markup",
"body-hyperlinks", "sound"]`, and `color_scheme: PreferDark`,
`accent: Some([0.388…, 0.815…, 0.874…])`, `contrast: Normal`, all via
`ReadOne`; these match `gdbus` on the same host. The `Read` fallback has
been exercised only against the fake portal, not a real old portal.

`live_notify_and_close` posts one visible notification ("Goya freedesktop
live test") and closes it within one second; it does nothing unless
`GOYA_FREEDESKTOP_POST_NOTIFICATION=1` is also set. Run once on the same
host: `Notify` returned id 3, the body was sent escaped
(`&lt;tag&gt; &amp; text`, degradation `BodyEscaped`), and
`CloseNotification` returned 758 ms after `Notify` was sent (including a
200 ms pause).
