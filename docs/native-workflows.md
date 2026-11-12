# Native ZCode workflows

Status: implementation spec for the native session shell. This closes the
read-only-monitor gap for workflows that can be backed by the existing v4
conversation snapshot and CLI command inbox. Full project/task management,
files, Git, PTY, settings, accounts, remote Host attachment, and phone recovery
remain separate parity work.

## Product rules

- The CLI/runtime is the sole owner of session rows, queue items, interaction
  requests, queue/guide behavior, command admission, and command outcomes.
- The native UI renders the active v4 projection and sends typed requests to
  the existing owner. It never predicts accepted queue state or removes an
  item optimistically. A control remains pending until the next projection or
  command outcome confirms the change.
- A permission or user-input card is derived from the current snapshot's
  `pendingInteractions`. It is actionable only while its interaction ID is
  present in the latest projection. Resolve requests include that ID and the
  active session ID; stale requests fail closed and refresh from projection.
- The compact native resolver supports permission requests with at most four
  advertised options, and simple user-input prompts with at most four choices
  or one free-text answer. Multi-question, multi-select, workspace-hook review,
  and larger option sets remain visible with an explicit explanation to answer
  in the full ZCode client. They expose no resolution controls and cannot be
  submitted as a partial or guessed answer.
- Queue and guide delivery are explicit user choices on composer submission.
  The CLI inbox decides admission. The native client does not maintain an
  accepted-input queue, and does not label a draft as queued/guided until the
  owner reports that outcome.
- Session navigation uses the existing session index and `SwitchSession`
  path. Switching clears focus; view and composer retention stay with the
  existing `MonitorHandle` owner.
- Keyboard and pointer activation invoke the same action handler. Every
  actionable node has a stable ID and an accessible name/state.

## State ownership and event order

```text
CLI v4 snapshot / contiguous deltas
       ├── rows ───────────────> transcript renderer
       ├── queue + delivery ───> queue controls/list renderer
       └── pendingInteractions > permission/question renderer

composer + selected delivery ── typed command ──> agent connection owner
       <── ACK/outcome or refreshed v4 projection ──────────────── CLI
```

The adapter may keep presentation-only selection, focus, and unsubmitted draft
state. All session facts and accepted workflow state come from the projection.
Stale interaction IDs are rejected by the command owner and never clear the
card locally.

## Acceptance scenarios

1. A deterministic synthetic snapshot displays rows, queue length/delivery,
   and each pending permission or question from its canonical fields. Applying
   a contiguous update replaces or removes these cards from the projection;
   stale or gapped frames change none of them.
   Unsupported interaction shapes remain visible and have no enabled answer
   action; a multi-question snapshot never submits only its first question.
2. Pointer and keyboard activation of queue/guide mode produce the same typed
   send intent. The rendered mode follows the user's current composer choice;
   accepted queue state changes only after owner confirmation.
3. Resolving a visible permission/question sends its current interaction ID
   and active session ID. A stale ID or rejected request leaves the current
   projection visible and reports the owner outcome; it does not fabricate a
   resolution.
4. Queue controls submit only supported public owner commands. The UI does not
   splice queue items or change auto-drain state in its local model.
5. Session previous/next controls and their keyboard bindings invoke the
   existing switch path and retain the current draft/transcript through its
   established owner.
6. Headless acceptance exercises rendered control IDs, pointer hit targets,
   focused keyboard actions, accessible names, disabled/pending states, and
   small-window layout without a live model/provider.

## Current interface

`ConversationProjection` retains canonical `queue` and `pendingInteractions`
and replaces `SessionView.workflow` only from an accepted snapshot or
contiguous `state.updated` patch. `deleteQueueItem`, `setAutoDrain`, and
`resolveInteraction` are public session commands. They carry the active
session id, stable command id, and current projection revision. The UI does
not splice queue items, clear cards, or change auto-drain locally.

A second permission choice keeps its own control when the first choice
already uses Allow. Unsupported shapes stay visible and expose no answer
control. Pending is presentation-only and keyed by command id. Mark the
control before enqueue. The outcome, a session mismatch, a stale epoch, or
an enqueue failure clears that command id only. A newer projection replaces
workflow facts and clears pending. One command must not disable an unrelated
owner control. Answer resolution disables the answer family and send, not
queue removal.

Headless `zcode-native` acceptance covers rendered control ids, pointer hit
targets, keyboard activation through the same handler, accessible names,
disabled and pending states, and the small-window layout. Project and task
management, files, Git, PTY, settings, accounts, and phone recovery remain
separate parity work.

## Session operations (DROWZY-GUI-PARITY-004 S1)

The same projection also retains optional `control` (`canStop`, `stopState`,
`activeWorks`), `availability` (`compact`, `sendQueuedNow`, `pauseGoal`,
`resumeGoal`), nullable `goal` (`status`, `objective`), nullable `plan`
(up to eight items), and `backgroundWorks` (up to eight items). A snapshot
that omits a section clears it. A `state.updated` patch replaces a section
only when that key is present, including JSON null for `goal` and `plan`.
A present but malformed section fails the frame and leaves the previous
projection unchanged. `workflowRuns` is not parsed in this slice.

The native client submits `stop`, `pauseGoal`, `resumeGoal`, `compact`,
`sendQueuedNow`, `reorderQueueItem`, and `cancelBackgroundWork` through the
existing session-command path. Reorder and send-now do not edit the local
queue list. `editQueueItem`, session create/rename/delete, row retry/fork/
rewind, and saved-workflow commands are not part of this slice. Control ids,
enable rules, payloads, and keyboard keys are specified in
`drowzy: docs/specs/goya-parity/completion-directive.md` §4. New controls must
not take geometry away from `queue_remove_00` or `permission_option_01`.
