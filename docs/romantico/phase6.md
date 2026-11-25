# romántico Phase 6 — distributed interfaces

Status: two roles on one Linux host share one session. The directive exit gate, a television board with an independent Android controller, is not met. No board was drawn. The controller was not a second device. Date: 25 November 2026.

## Roles

Assignment uses capabilities, not an operating-system name. Primary receives mutate and transfer, and it requires a display and interactive input. Controller receives read and submit, and it requires interactive input. Auxiliary receives read, and it requires a display. Touch and a large display are preferences. A missing preference still assigns and sets `preferences_met` to false, so a remote can be a controller. A missing requirement is `RMT-E7001`.

`adopt_surface` will not give mutate or transfer to a replica that is not the authority. It will not lower the active writer to a controller or a reader. Both refusals are `RMT-E4006` and leave the permission set unchanged. Grant still clears mutate directly. That path is the handoff fence, not a surface adoption.

## Capability record

Kind 9 is an RCB/1 record of four booleans: display, interactive input, touch, and large display. It carries no resolution, refresh rate, sensor, or operating-system name. HELLO and schema negotiation are unchanged. The record is checked before it is sealed. A missing requirement sends nothing, and the channel counters stay where they were. A boolean payload other than 0 or 1 is `RMT-E2001`. An extra field is the same code.

## One session, two presentations

The television adopts primary. The phone adopts controller. Each sets its own viewport, 1920 and 1080. Those values stay on the replica. Field 9 is absent from the portable document. The phone submits card 0 and then card 7 as intent action 1. The intent bytes do not name a remote, a touch screen, a mouse, or a keyboard. The television commits both. Delivery leaves the phone's pending intent in place until the phone acknowledges it. Both replicas finish at revision 2, score 1, completion 250, and the same fingerprint. The phone is not the writer.

An empty commit on the television is `RMT-E5004`. A score write and a handoff prepare on the phone are `RMT-E5005`. A third replica joins the post-match snapshot as an auxiliary display, reads completion 250, and cannot enqueue (`RMT-E4006`). Its fingerprint matches the board.

## Cross-check

Node.js 18.19.1 rebuilds the controller record: display false, interactive input true, touch false, large display false. The fixture is `crates/goya-romantico/fixtures/controller.rcb.hex`. Rust and `conformance/surface.mjs` agree. The script prints `capabilities` and then `equivalent`.

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 79 tests. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed. `cargo tree -p goya-romantico -i getrandom` found no such package. Library code does not reference `std::net` or `std::fs`. `std::fs` appears only in the conformance hex reader. The temporary target directory was removed after that run.

## What remains open

Two roles on one Linux host shared one Goya Pairs session. A television board was not drawn, and an Android phone was not an independent controller. There is no socket, no WebSocket, and no TLS session. No certificate was verified, and none was ignored. The core does not open a file. `RCB/1` is still the interim codec. Unplanned failover is not implemented. The 15 P0 device gates stay open. The compiled and interpreted contract is recorded in `phase7.md`.
