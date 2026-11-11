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

## Current interface gate

The present native `ConversationProjection` validates rows/revision/control
but does not retain `queue` or `pendingInteractions`; `AgentCmd` currently
supports only text submission and session switching. Workflow controls must
not ship as visual mockups. They become enabled when the native projection and
public command bridge expose the canonical v4 data and supported owner
requests. Command/API ownership remains in the host attachment/runtime layer.
