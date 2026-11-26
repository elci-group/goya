# Chromatics conformance fixtures

Shared by every Chromatics implementation (directive GOYA-INT-CHR-001 §14).
The reference runner is `goya_chromatics::fixture::run`. A generated target
runtime (for example Arachnid's JavaScript) must pass the same files.

## Format

One command per line. `#` starts a comment. Values that contain spaces are
double-quoted. A `-` value means "none". Unknown commands and keys fail.

| Command | Meaning |
| --- | --- |
| `capacity S…` | Slots, in order, all available |
| `reserved S…`, `unsupported S…`, `available S…` | Change slot status |
| `native K…` | Native-equivalence keys covered by dedicated inputs |
| `intents I…` | Registered intent handlers (before the first `resolve`) |
| `policy k=v…` | `primary`, `fallback`, `preserve`, `ordered`, `exclude_native`, `exclude_duplicates`, `min_improvement`, `min_residency`, `max_deferral` |
| `context C` | Context key for later resolves (include the profile). Refused once `push` has opened a scope |
| `push NAME [inherits]` | Enter a scope (a view; views nest). `inherits` declares the §6 policy: the scope resolves its own candidates composed onto its parent's pool, own first. Later `candidate`/`set`/`remove`/`clear` lines author the scope's own pool |
| `pop` | Leave the innermost scope; restores its pool and the parent context (§6) |
| `candidate ID k=v…` | Add a candidate: `intent`, `label`, `entity`, `available`, `authorised`, `risk`, `confirm`, `native`, `rank`, `favourite`, `position`, `last_used`, `uses`, `pinned`, `cost`, `probability`, `contextual`, `urgency`, `preference`, `expires`, `symbol`, `describe` |
| `set ID k=v…`, `remove ID`, `clear` | Change the active scope's candidate list |
| `resolve trigger=T now=N interacting=yes` | Allocate over the active scope's effective pool. Triggers: `context` (default), `policy`, `capability`, `background`, `arrival`, `invalidation` |
| `resolve-refused …` | Allocate and expect verification to refuse |
| `expect S=ID…` | Bindings of the named slots (`-` = empty) |
| `expect-empty` | No bindings |
| `expect-rejected ID CODE` | Rejection code, as in `Rejection::code` |
| `expect-selection ID KIND` | `pinned`, `primary`, `fallback` or `retained` |
| `expect-symbol S=V…`, `expect-describe S=V…` | Non-colour identification carried by a binding (§21); `-` = absent |
| `expect-note KIND` | `deferred`, `retained`, `displaced` or `pin-ignored` |
| `expect-generation N` | Generation id of the last allocation |
| `publish` / `stage` / `present` | Commit now, or stage and later commit. Both activate the context |
| `activate C` | Make C the only executable scope |
| `press S [repeat]` | Key down (`repeat`: expect an auto-repeat) |
| `release S expect=E` | Key up: `execute:ID`, `unassigned` or `rejected:CODE` |
| `revoke ID`, `reinstate ID` | Immediate invalidation |
| `deny I`, `allow I` | Execution-time permission for an intent |
| `cancel-presses` | Suspension: drop held keys |

Numbers are integers. Keep every value within ±2^53 so that a JavaScript
runtime computes the same utilities.

## Files

| File | Covers |
| --- | --- |
| 01 | Preferential allocation with ranked fallback, no duplicates (§8.1) |
| 02 | Capacity from 0 to 4, reserved and unsupported keys (§13.1, CHR-G12) |
| 03 | Native-input precedence and the accessibility override (§3.3) |
| 04 | Eligibility: availability, permission, expiry, intents, labels, risk, malformed ids (§7, §20, §21) |
| 05 | Slot preservation and ordered slots (§10.4) |
| 06 | Hysteresis, residency, maximum deferral, interaction quiet (§10.3) |
| 07 | Immediate invalidation (§10.2, §10.5) |
| 08 | Pins and manual allocation (§8, §8.3) |
| 09 | Scopes, modals and covered scopes (§6) |
| 10 | Generations, staging and dispatch (§11) |
| 11 | Profile isolation (§19) |
| 12 | Deterministic ties (§9) |
| 13 | Declared inheritance: composed pools, modal chains, parent restore (§6, §14) |
| 14 | Accessibility floor: labels, symbols, descriptions (§21) |
