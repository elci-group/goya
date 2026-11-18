# Emperor threat model

Programme GY-EMP-001, revision 1.0.0. Date 16 November 2026. Scope is milestone EMP-M1, compiler version 0.0.1, profile `emperor.linux.interactive/0.0.1`. This is not a completed security review and not a sandbox.

Describing capabilities is not a sandbox. A manifest that says `filesystem` is `none`, `network` is false, `subprocess` is false, and `web_content` is false does not confine a process. EMP-M1 fails closed on a required sandbox by not claiming one. No profile in this cycle both requires enforcement and then runs without it. Optional degradation is not offered, because no enforcement path exists to degrade.

The linked ELF is a normal desktop process for whoever later runs it. Rust memory safety in the compiler is not a Linux sandbox.

## Assets

The authored `.gscene`, the canonical NIR JSON, the generated Rust embed, the linked `App`, `build-report.json`, the direct-edge SBOM, and `provenance.json`. Signing keys are out of scope because this cycle does not sign.

## Actors

### Untrusted scene

M1 control that is specified: admission goes through `sash::admit::open` and `goya_lang::handoff::admit`. The only buildable scene is one Sash admits as the interactive counter. Sash bounds the source at 1 MiB. If the two frontends disagree on success or failure, admission fails. Scene text is escaped into Rust string literals. It is not interpolated into a shell. Output paths, evidence paths, and `--nir` paths that contain a `..` component are rejected.

Control that does not exist: a fuzzer corpus, a separate native-IR verifier beyond `deny_unknown_fields` and the profile checks, and a review that the generated window cannot be aimed at a different scene by editing `embed.rs` after the report is written.

### Third-party dependencies

M1 control that is specified: `cargo` is invoked as `Command::new("cargo")` with `--offline`. The SBOM names the direct edges emperor-runtime, goya-parity, goya-wayland, sash, and goya-lang, and says it is not transitive.

Control that does not exist: a vulnerability review, a licence review, a check that the lockfile matches a recorded digest, and verification of crate or toolchain signatures. Cargo's own lock is not a review.

### Build scripts and procedural macros

M1 control that is specified: the runtime build script may only copy `EMPEROR_EMBED` or write an embed that returns `None`. The compiler does not pass scene text through a shell. Cargo's stderr tail, truncated to 4 KiB, may be stored in a stage detail.

Control that does not exist: a sandbox around build scripts, an allowlist of proc macros, or a reproducible build directory that strips local paths. A hostile `build.rs` in a dependency is out of reach of this cycle.

### Malformed assets and fonts

M1 control that is specified: the counter profile has no image assets and no authored font file. Paint uses the host's software path and a fixed `glyph_px` of 8. Strings come from the admitted scene or from the session's decimal formatting and edit buffer.

Control that does not exist: a font parser limit, a rejection policy for malformed font files, or tests with hostile images. The host's text shaper is not re-audited here.

### Network-delivered content

M1 control that is specified: `web_content` and `network` are false. The profile does not fetch a document and does not link a browser engine. The counter scene does not declare a web-content capability.

Control that does not exist: a network namespace, a seccomp filter that denies connect, or a content process. Absence of a feature is not a tested network boundary.

### Untrusted documents

M1 control that is specified: the only document is the `.gscene` under admission. There is no open or save path. Filesystem capability for this profile is `none`.

Control that does not exist: a document worker, portal-mediated file choice, or a check that a hostile file dropped on the window is ignored. Dropping a file is not a specified event.

### Privileged Linux APIs

M1 control that is specified: the compiler does not request a sandbox and then continue. It does not call Landlock, seccomp, or a capability-bounding set. It must not report those as enabled.

Control that does not exist: any confinement of the running `App`. The process can do what the user who launched it can do. Failure to establish a sandbox cannot be observed because no sandbox is attempted. That is an honest gap, not a pass.

### Plugins

M1 control that is specified: the profile has no plugin host and no native-plugin load. Nothing in the NIR represents a plugin.

Control that does not exist: a plugin process, a seccomp profile for one, or a signature check on a plugin binary. `goya-plugin-host` is not part of this compiler.

### Interprocess communication

M1 control that is specified: the Wayland socket is the display connection used by `goya-wayland`. Clipboard writes happen only when copy is allowed, and they use the host's in-memory `text/plain` offer. Paste is ignored when the capability is false, including a later `ClipboardText` event.

Under `--window kumimanu` the X socket is the display connection. Copy and cut set this window as `CLIPBOARD` owner. The text stays in process memory and is written only into the property a requestor names in a `SelectionRequest`. A request for another selection, a request stamped before ownership, or an unknown target gets property `None`. A payload over 256 KiB, the same bound GTK 3 uses, is served by INCR even when BIG-REQUESTS would allow one request: Kumimanu selects `PropertyChange` on the requestor's window for the transfer, writes 64 KiB chunks only when the requestor deletes the property, and releases that selection when the transfer ends or idles for five seconds. Four transfers at once is the bound; a fifth is refused. Paste converts to `UTF8_STRING`, then `STRING`, and refuses partial, empty, invalid, or over-1-MiB replies. An INCR reply that passes 1 MiB, changes type, or arrives partial is drained and discarded, not buffered, and ends as no text. An unanswered conversion, or an INCR peer silent between chunks, resolves as no text after two seconds. X11 lets any client on the same server read the selection or own it. Kumimanu does not claim isolation from those clients.

Control that does not exist: an allowlist of Wayland protocols, an IPC broker, or authentication of the peer. The compositor is trusted for window policy. Emperor does not add a second check.

### Package installation

M1 control that is specified: `package --format linux-archive` may create a tar archive next to the output directory and must reject `..`. It does not install, does not register a desktop file, and does not run a post-install script. AppImage, Flatpak, deb, and rpm are refused.

Control that does not exist: a signed archive, a clean-machine install test, an uninstall record, or a check that the archive has no unexpected members beyond what `tar` was asked to pack. The desktop file's `Exec=App` is a relative name, not an installed command.

### Compiler artifact substitution

M1 control that is specified: `verify` recomputes the SHA-256 of `App` and compares it with `build-report.json`. A mismatch exits 4. ELF magic, class, endianness, machine, and a non-executable `GNU_STACK` are checked again. A release build fails closed when `GNU_RELRO` is absent.

Control that does not exist: a signature over the pair of binary and report. Replacing both files together is not detected. There is no transparency log and no separately trusted public key. Reproducible bytes and authenticity are different claims. This cycle has neither a multi-machine reproduction nor a signature.

## Out of scope until a later profile admits them

Notifications, startup registration, portals, drag-and-drop payloads, update feeds, and any sandbox that is advertised as enforced. Those need fixtures and a fail-closed implementation before a PASS.
