# Goya plugin host boundary

## Product rule

Each plugin instance gets a separate loopback HTTP origin, canonical asset
root, browser profile, session association and capability generation. The host
serves static plugin content and renders it through the existing
`goya-webview::BrowserSession`. The plugin host does not create sessions,
write task storage, execute commands, expose DevTools, or dispatch arbitrary
Host RPC. Connecting approved operations to public Host services is a later
integration boundary and must preserve the existing session/runtime owner.

## Ownership and lifecycle

- `PluginInstance` owns immutable plugin ID, instance ID, session ID, canonical
  root and the loopback asset server.
- `BrowserPluginSession` owns the isolated browser profile and one
  `BrowserSession`. Its methods expose navigation only to that instance's
  origin, bounded viewport/input, pixels and teardown; raw CDP and JavaScript
  evaluation are not part of this API.
- `CapabilityBroker` owns the current capability grant set and generation for
  one instance/session/origin tuple. Host code grants capabilities; plugin
  messages cannot grant themselves access.
- Every committed document navigation revokes the old generation. Revocation
  increments the generation and clears grants, so delayed messages cannot
  reuse prior authority.

```mermaid
sequenceDiagram
  participant Host
  participant Instance as PluginInstance
  participant Assets as Loopback asset server
  participant Browser as BrowserSession
  participant Broker as CapabilityBroker
  Host->>Instance: canonical root + session + grants
  Instance->>Assets: bind 127.0.0.1 ephemeral port
  Host->>Browser: launch isolated profile
  Browser->>Assets: GET /index.html
  Assets-->>Browser: bounded bytes + restrictive CSP
  Browser-->>Host: bounded RGBA capture
  Host->>Broker: host-observed source, origin, strict envelope
  Broker-->>Host: typed authorized operation or rejection
  Note over Broker,Host: This slice authorizes; it does not execute Host RPC
  Host->>Broker: navigation commit or revoke
  Broker->>Broker: increment generation and clear grants
```

## Asset server

- Bind only IPv4 loopback on an ephemeral port, one listener per plugin
  instance. Never listen on wildcard, LAN, or public interfaces.
- Serve only GET/HEAD requests with bounded request lines/headers and bounded
  response files. Permit regular files under the canonical root only. Decode
  URL path encoding strictly, reject traversal, absolute paths, backslashes,
  malformed escapes and symlinks whose canonical targets leave the root.
- Set MIME from a fixed extension table and send a restrictive Content
  Security Policy: scripts/styles/images/fonts may load from the same origin;
  connections, objects, forms and framing are denied. Do not add permissive
  CORS headers or network proxies.
- Unknown methods/paths, malformed requests, oversized files and unsafe paths
  fail with explicit HTTP errors. Never return a success placeholder.

## Capability message boundary

### Native bridge execution contract

The host remains the sole owner of session data and capability grants. A plugin
page may submit only the two declared read operations; the browser adapter
supplies the committed frame origin and instance identity, never fields from
plugin JSON. The host validates scope, grant and generation before dispatch,
performs a bounded asynchronous read through the injected Host service, then
validates the same generation and revocation state again before returning a
reply to the browser. Navigation, grant revocation and teardown advance the
generation and invalidate in-flight results. Replies are size bounded and
contain either projected data or a generic failure code.

The browser bridge uses a bounded Runtime binding event queue. It accepts
messages only from the owned main frame at the plugin instance origin and never
exposes arbitrary JavaScript evaluation. Acceptance: deterministic service
tests revoke during an awaited read and prove no reply is emitted; real Chrome
test sends a plugin request and observes the correlated response, rejects a
child-frame/foreign-origin message, and proves navigation/revocation invalidates
queued and in-flight work.

`BrowserPluginSession::pump_bridge` is the bridge owner: it drains trusted
Chromium binding events, validates the event context's committed origin,
authorizes the strict message, awaits the injected read service, checks the
generation again, and replies into that same still-live execution context.
`ProjectionHostService` is an immutable snapshot adapter for attached native
session projections; refresh ownership remains with the native client. It does
not call Drowzy repositories or create a second runtime path.

The broker accepts a strict JSON envelope no larger than 64 KiB, with exactly
`requestId`, `instanceId`, `sessionId`, `generation`, `operation`, and optional
`payload`. The browser adapter supplies the source instance and observed origin
as trusted metadata; fields in plugin JSON cannot substitute for either.
Request IDs and all identity fields are bounded nonempty strings, and unknown
fields or operations are rejected.

Initial operations are read-only `session.readSummary` and
`session.readConversation`. Each requires its corresponding host-issued grant.
No message performs a write, send, file access, arbitrary URL fetch, raw RPC,
CDP or script evaluation. Authorization returns a typed operation plus the
validated session/instance/generation scope; an existing public Host adapter
must separately own execution. Unknown operations fail closed.

All four scope values must match the current broker state: observed source
instance, observed origin, session ID, instance ID and generation. A stale
generation or origin mismatch is rejected. A navigation commit, explicit
revocation or instance teardown increments generation and clears all grants.

## Acceptance

- Broker tests cover each scope mismatch, unknown/extra fields, oversized
  messages, missing grants, navigation-generation invalidation and teardown.
- Asset tests cover same-root MIME/CSP responses, method denial, encoded
  traversal, malformed encoding, missing files, size bounds and symlink escape.
- A deterministic local HTML fixture loads through a per-instance loopback
  origin in an isolated `BrowserSession`; captured dimensions/bytes are bounded
  and the session tears down its child/profile. No external network or user
  plugin data is used.
- This establishes the isolated hosting foundation only. Service bridge
  execution, installation/configuration lifecycle parity, storage semantics,
  permissions, downloads, popups, accessibility and cross-platform support
  remain unverified until connected to their authoritative owners and tested.

## Integration hardening

Linux assets are opened relative to an already-open directory using
`openat2` with BENEATH, NO_SYMLINKS and NO_MAGICLINKS. Root-directory
replacement cannot redirect reads; unsupported kernels/platforms fail closed.
The supplied root must be a dedicated public asset bundle, never a settings,
credential or account-data directory. Requests must carry the exact per-instance
Host authority; nested frames/workers and network connections are denied by CSP.
Header reads and writes have wall deadlines. The browser owner checks committed
origin through Page.getFrameTree before grants, authorization and pixel delivery;
page-supplied JSON never establishes that origin. The service bridge remains
unimplemented: authorization records do not invoke runtime RPCs.
