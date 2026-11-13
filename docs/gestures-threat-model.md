# Gestures text profile: preliminary threat model

Status: **draft, not reviewed or approved**. Scope is `gestures.text/1` on
qualified native controls, not general data-loss prevention. Source, clipboard
data, native callbacks and target capabilities cross trust boundaries.
Goya-controlled routes and the editor owner are inside the enforcement
boundary; the OS, external apps, screenshots and cameras are outside it.

| Threat | Required control | Evidence needed |
| --- | --- | --- |
| Copy denial bypass through native routes | Inventory menus, shortcuts, touch, assistive, direct and applicable framework export routes; enforce at execution or reject the profile. Denied commands do not bubble to another target. | F03 and negative probes through each admitted route on real controls. |
| Secret or clipboard content in diagnostics | Never log payloads, recoverable payload hashes, transfer handles or secret selections; keep secret history scoped and memory-only, clearing it at session end. | Synthetic tracing, log review, lifecycle and protected-control tests. |
| Clipboard as executable or unbounded input | Plain Unicode text only, bounded incremental decode, reject invalid text and unsupported formats, normalize only CRLF/CR to LF, and validate before mutation. | F07, decoder fuzzing and memory/CPU bounds. |
| Stale asynchronous operation edits another field | Capture identity, epoch, focus, revision and selection; recheck after transfer, reserve local state and invalidate late callbacks. | F06/F14, cancellation and lifecycle fault injection. |
| Cut loses data on clipboard failure | Prevalidate and reserve; export before removal, then revalidate and commit. On observable intervening failure report partial export and retain text; never overwrite the clipboard as compensation. | F05 injection before and after acknowledgement and local commit. |
| History bypasses current policy or validation | Scope history to owner/epoch; revalidate undo/redo and leave rejected entries intact; bound and evict history. | F09 with changed policy and constraints, external replacement and eviction. |
| Target or surface silently drops restrictions | Explicit reachability and native/equivalent/deferred/rejected disposition; reject unsupported routes and unimplemented deferral. | F11/F12 and complete capability and route manifests. |
| Clipboard privacy prompt bypass or polling | Query only permitted metadata, keep unknown availability where needed, request data only through an admitted user action. | Route instrumentation, native privacy and cancellation tests. |
| Policy predicate has effects or exceeds budget | Use typed pure bounded expressions over declared dependencies; fail closed on unknown state, invalidation or budget exhaustion. | GEST-E008 and policy-budget negative tests. |

Open reviews include platform APIs and export routes, actor/thread ownership,
reservations and timeouts, production telemetry policy, fuzz corpus, security
owner and sign-off. This draft establishes no native enforcement.
