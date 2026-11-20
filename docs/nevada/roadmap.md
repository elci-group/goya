# Nevada roadmap

Nevada is Goya's Android TV compiler target. It translates supported GSSH/3 interactions to D-pad navigation, D1 activation, and text entry through the system TV keyboard. A blocking static gate verifies the authored actions and the focus graph that the generated project uses.

## Delivered

| Phase | Delivery |
| --- | --- |
| 0 | Device capability resolution, exact-version OEM profiles, interaction translation, and spatial focus. See [NEVADA-1](NEVADA-1.md). |
| 1 | Strict GSSH/3 scene lowering for the supported Android TV surface, plus a typed contract for rechecking runtime capabilities against the emitted routes. |
| 2 | Library orchestration over the 13 Emperor stages, target negotiation, and stable incompatibility diagnostics. See [NEVADA-2](NEVADA-2.md). |
| 3 | Locked Aragon Gradle project with a Leanback launcher, banner, generated D1 and focus runtime, source map, and a passing static receipt linked to the source digest. The TV receipt uses gate `GY-NV-GATE-11` at Emperor's static verification stage. See [NEVADA-3](NEVADA-3.md). |

## Qualification and future language work

- Qualify the launcher, remote, keyboard, lifecycle and focus behavior on Android TV. The release device gate remains blocked until an artifact runs on a TV device or TV system image.
- Supply real OEM censuses with provenance and wire any additional device-specific behavior through the typed policy and runtime observation boundary.
- Expand the admitted interaction surface when there is approved language grammar, an implemented Android handler, and parity evidence for that handler. Scroll viewports, draft intents, D2, and optional OEM routes remain outside the supported compiler surface.
- Apply signing and release distribution under the target application's release configuration.
