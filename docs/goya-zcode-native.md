# Native ZCode client successor

Status: development prototype. This entry implements the protocol/GUI foundation of DROWZY-GUI-PARITY-001, not full frontend parity.

The original `goya-zcode`/`zcode-monitor` pilot has independent local Wave 10 work. It remains intact. `goya-zcode-native`/`zcode-native` is the explicit successor entry for the v4 adapter, durable history projection, command-outcome query, resize-aware GUI and improved IME. Only the selected entry runs; this migration does not introduce another accepted-input owner or attach a second runtime to an existing task. Existing-Host attachment remains unimplemented.

Contracts: [protocol and recovery](protocol-parity.md), [native layout and input](gui-parity.md), [native workflows](native-workflows.md), [host attachment](host-attachment.md), [integration gates](integration-gates.md). Authoritative protocol wire version is 3, distinct from projection version 1. The CLI/runtime remains the sole authoritative queue/admission owner. The native client stores only display projections, drafts and short-lived in-flight submission metadata.

Run from the workspace root:

```sh
cargo run -p goya-zcode-native --bin zcode-native
cargo run -p goya-zcode-native --bin zcode-native -- --live --cli /path/to/zcode --workspace /path/to/workspace
cargo run -p goya-webview --bin goya-browser -- https://example.test
```

The browser package is `goya-webview`: use `cargo run -p goya-webview --bin goya-browser -- <http(s)-url>`. Model sends remain disabled unless `--allow-send` is supplied. Automated acceptance uses synthetic fixtures and does not spend model tokens.

## Qualification limits

The native client is the session shell, not the full ZCode application shell. Queue, guide, permission, and question controls submit canonical owner commands and settle from the projection or command outcome. Session previous/next uses the existing switch path. `--host-attachment` reuses an existing Host descriptor and does not spawn a second app-server after a failed handshake. Project and task management, full rich messages, file/Git/PTY panels, settings, accounts, phone recovery, real screen-reader/IME qualification, and Windows/macOS qualification remain open. The browser, PDF, and plugin crates prove bounded native content foundations, not these missing application workflows. Plugin Host RPC execution is deliberately absent until the scoped bridge is integrated with the existing public Host services.
