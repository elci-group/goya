# Existing Host attachment for the native frontend

## Product rule

The native frontend can attach to the window-scoped Host that already owns a
workspace and session. An attached frontend reuses that Host's scoped
`IZCodeAgentService`, routing, command admission, persisted projection, and
runtime. It must not start a second `app-server`, CLI Agent, or session runtime
for the attached session.

Standalone use remains supported through the existing local `app-server
--stdio` connection. Standalone and Host-attached are explicit connection
modes; a failed attachment must fail closed instead of silently spawning a
second runtime.

## State ownership

- The existing Window Host owns attachment authorization, `workspaceIdentity`,
  remote-session scope, owner/lease routing, and the authoritative service
  connection generation.
- The CLI/runtime continues to own sessions, durable rows, revisions, event
  sequence, `CommandInbox` admission, command idempotency, and command results.
- The native agent owns only its transport, request IDs, subscriptions,
  reducers, and pending UI intents. `SessionView` remains a derived projection.
- `workspacePath` is the filesystem/cwd value. The identity key is
  `workspaceIdentity.trim()` when nonempty, otherwise `workspacePath`.
  Remote attachments carry `remoteSessionId`; path-only matching is invalid.

## Attachment contract

The native client receives a descriptor through `--host-attachment FILE`.
Version 1 has this shape:

```json
{
  "protocolVersion": 1,
  "socketPath": "/private/window-host/native.sock",
  "attachmentId": "opaque-id",
  "generation": 7,
  "capability": "short-lived-opaque-capability",
  "workspacePath": "/work/project",
  "workspaceIdentity": "optional-canonical-identity",
  "remoteSessionId": "optional-remote-session"
}
```

The descriptor is private to the native process. The socket is a transport
into the existing Host, not a new runtime. The Host consumes the capability
once and validates the current attachment generation and workspace identity.
For remote workspace descriptors, the Host also requires the exact
`remoteSessionId`; matching by path is invalid. A remote socket may be created
only for an already-online logical session after the Host attachment registry
resolves its scoped services. The Host must not create or reconnect a remote
session for Goya.

The socket carries newline-delimited JSON-RPC with the existing v4 method
names. The initial `native/attach` request carries the descriptor fields except
`socketPath`, plus the native connection's stable `clientId` (1–128 bytes).
The Host binds that ID for its trusted terminal-scope handshake and the client
uses the same ID in command envelopes. The ACK contains `protocolVersion`,
`attachmentId`, `generation`, `clientId`,
`workspacePath`, normalized `workspaceKey`, optional `workspaceIdentity` and
`remoteSessionId`, and `clientMode: "desktop-continuous"`. The client must not
provide a trusted `connectionId`, owner lease, or client mode; the Host derives
these values. The Host performs the terminal-scope hello/initialize handshake
before exposing the service stream. After the ACK, the native client uses the
existing conversation methods and `v4/conversation/frame` notifications,
including bounded resync and rows/range reads. Commands are forwarded through
the Host's scoped agent service so `CommandInbox` remains the sole accepted
input queue. Socket close releases only this attachment.

The Host owns generation and lease validation; the native client retains the
acknowledged generation as a stale-result guard. This document does not claim
the cross-repository bridge is available until that Host implementation and an
end-to-end fixture are present.

## Event order and stale-result rules

```text
trusted launcher → native client: socket path + one-time capability
native client → existing Window Host: native/attach(workspace, identity, remoteSessionId)
Window Host → native client: authoritative attachment ACK(generation, identity, desktop-continuous)
native client → Host scoped agent: sessions-index subscribe → ACK → owned frame
native client → Host scoped agent: conversation subscribe → ACK → snapshot
Host scoped agent → native client: same-subscription contiguous frames
native client → Host scoped agent → CLI CommandInbox: command → ACK/query result
socket close → Host: detach this attachment; runtime remains owned by Host
```

- Reject the handshake if identity, remote-session scope, capability, or Host
  generation does not match the Host's current attachment state.
- Frames are accepted only on the acknowledged socket for its Host attachment
  generation, identity, remote-session scope, and current subscription. Every
  local transport reconnect gets a new epoch; queued UI intents captured under
  an older epoch are dropped even if the same session is selected again.
- An ambiguous command response is resolved by querying the same stable
  `commandId`; the client never resends an unknown outcome.
- Desktop delivery remains `desktop-continuous`; it does not use the mobile
  `web-remote-replayable` path.

## Acceptance scenarios

- Attaching to a local workspace reuses the already running Host and does not
  spawn a second app-server or Agent runtime.
- A remote descriptor carries canonical identity and `remoteSessionId`; it is
  invalidated when its registry generation is replaced or the logical session
  is disposed.
- Invalid or expired capability, stale Host generation, and mismatched owner
  scope fail closed before subscriptions or commands are exposed.
- An attached client receives a durable session snapshot, then applies only
  frames for its acknowledged generation and owned subscription.
- Send, queue, guide, follow-up, and interaction commands reach the existing
  Host's scoped agent service; stale UI intents are rejected before admission.
- Detaching the native UI releases only its scoped transport and leaves the
  Host-owned session/runtime available to other attachments.
- Standalone mode continues to use `app-server --stdio`; attachment mode does
  not fall back to standalone mode after a handshake failure.
