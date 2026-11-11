# ZCode v4 protocol parity

## Product rule

The Goya monitor reads the same durable conversation projection and submits
commands through the CLI's v4 command inbox. It does not derive conversation
history from runtime events or keep a second accepted-input queue. On reopen or
reconnect it restores the persisted snapshot, then applies only frames owned by
the current subscription and contiguous with the confirmed projection cursor.

Goya currently attaches directly to a local CLI `app-server`, so it declares
the trusted `desktop-continuous` profile. This slice does not claim remote Host
attachment support. Desktop continuous events are read directly; if the CLI
reports an epoch/cursor gap, the monitor requests same-subscription recovery.
It must not substitute the mobile `web-remote-replayable` snapshot/gap policy.

## State ownership and identity

- The CLI owns persisted rows, session summaries, revisions, event sequence,
  command admission, idempotency, and command outcomes.
- The agent thread owns one NDJSON connection, request IDs, one active
  sessions-index subscription, one active conversation subscription, their
  cursors, bounded fragment assembly, and one in-flight command request.
- `SessionView` owns only render-ready text lines and visible session metadata.
  It is a bounded derived projection, never an alternate source of conversation
  truth or accepted inputs.
- The workspace topic key is
  `workspaceIdentity.trim()` when present, otherwise the workspace path. The
  workspace reference always keeps the real path for file operations and sends
  the normalized identity key as `workspaceKey`.
- A `connectionId` is generated once per spawned app-server connection and is
  reused for all of its subscriptions, commands, resyncs, and unsubscriptions.

## Wire contract

1. Subscribe to `sessions-index/<workspaceKey>` through
   `v4/conversation/subscribe`; the v4 response is an ACK and initial physical
   topic frames arrive afterward as `v4/conversation/frame` notifications.
2. Choose or retain the requested session from the sessions-index snapshot.
   Subscribe to `conversation/<sessionId>` with no base unless the client
   actually retains a matching snapshot, log epoch, and sequence.
3. Reassemble v4 physical `complete`/`fragment` frames within the protocol's
   byte, fragment, and concurrent-assembly bounds. Validate the topic,
   subscription ID, frame kind, row/delta shape, and sequence range before
   changing `SessionView`. A malformed frame for an owned subscription clears
   its partial assemblies and requests same-subscription recovery; stale or
   unowned frames fail closed. Queue overflow reconnects so both topic cursors
   are recovered from the CLI. Legacy events are never interpreted.
4. A conversation snapshot replaces the current derived transcript and records
   its `logEpoch`, `seq`, and `revision`. A delta frame applies only when its
   topic/subscription match and `fromSeq` equals the current cursor; `toSeq`
   advances the cursor after the whole frame is applied. A stale subscription,
   epoch, overlap, or gap cannot mutate the visible transcript. Request
   `v4/conversation/resync` for the same topic/connection/subscription using
   the confirmed base (or `null` to force a snapshot).
5. Older visible rows are loaded through the bounded `rowsRange` query and
   merged by stable `rowId`; a result from another `logEpoch` is discarded.
   `row.upserted`, `row.delta`, and `row.removed` update the derived visible
   lines; they are not blindly appended as new lines. The current projection
   retains at most 2,000 rows; when an older range fills that cap, newest rows
   can leave the bounded window and later upserts for those unloaded IDs are
   ignored until a snapshot/resync. A larger virtualized residency model is a
   separate follow-up.
6. Every send gets a stable `commandId` and `clientId`. Send uses
   `v4/command` with `type: "sendText"`, the latest known `baseRevision`, and
   the original command ID on any retry. ACK outcomes are preserved as
   `accepted`, `rejected`, `stale`, `duplicate`, `noop`, or `failed`. On an
   ambiguous response, query the same key with `v4/commands/query`; never
   automatically replay an unknown command. A rejected/stale/failed command
   does not create a local user row.

The event order is:

```text
agent thread → sessions-index subscribe → ACK → owned index frame
             → conversation subscribe → ACK → snapshot frame
             → contiguous deltas → SessionView (derived rows)
composer → one SendText commandId → v4/command → CLI CommandInbox
                                  ← ACK or v4/commands/query outcome
```

## Acceptance

- Deterministic protocol reducer tests cover strict decoding, snapshot
  hydration, atomic delta and rowsRange application, row upsert/delta/removal,
  command ID stability and outcomes, and stale subscription/epoch/sequence
  rejection.
- A fake `app-server` CLI fixture exercises both v4 subscriptions, post-ACK
  frame notification order, persisted history hydration, one command and its
  query outcome without a model/provider call or real user data.
- `SessionView::rows_for_window(n)` returns exactly `n` display rows with
  leading blank padding and viewport-aware scroll clamping. Existing `rows()`
  callers retain their 14-row behavior.

The native window owner calls `MonitorHandle::shutdown` before exit. This cancels
ACK/frame waits, joins the connection worker and lets its process guard reap the
app-server. Regression evidence includes shutdown during an unresponsive startup
request; UI teardown does not rely on an arbitrary post-stop sleep.
