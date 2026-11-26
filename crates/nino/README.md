# nino

Current status: **N0 ROM intelligence and explicit asset decoding**. `nino inspect` fingerprints a bounded
GBA ROM, reads cartridge header fields, checks the fixed byte and header
complement, recovers a probable initial ARM branch target, and reports save
technology and RTC string hints. `nino analyse` follows explicitly seeded ARM
and Thumb control flow, preserving instruction ranges and reporting unresolved
branches or opcode classes. It does not execute the ROM, decode the full
instruction set, produce NIR, or emit `.gscene`. A diagnostic subset executor
can run the declared ARM and Thumb arithmetic and branch subsets from ROM,
and ARM/Thumb single loads and stores over work RAM and ROM.

The `graphics::AssetView` library API decodes explicitly selected 4bpp and
8bpp 8×8 tiles, 15-bit palette entries, and 16-bit text-background map entries
from a bounded ROM, VRAM, or palette RAM snapshot. Each result includes a
SHA-256 digest of the supplied source and its exact source byte and guest
address range. Tile pixels remain palette indices; raw palette bit 15 and
map attributes remain available. Display transparency, colour conversion,
scrolling, priority, affine transforms, blending, and resource discovery
require observed display state. No automatic ROM-wide asset classification is
claimed.

`graphics::tile_to_gva` produces an opt-in GY-GVA/0 candidate using the shared
Goya encoder. Equal-colour horizontal runs become filled rectangles on an 8×8
logical canvas. Callers explicitly select opaque or index-zero transparency
and supply the selected palette bank. The candidate records tile and used
palette provenance, encoded size, and indexed source size. It is not selected
automatically: encoding can expand the resource and target rasterisers still
need fidelity checks. Five-bit colours expand to eight bits by bit replication.

```sh
cargo run -p nino -- inspect path/to/homebrew.gba --format json
cargo run -p nino -- analyse path/to/homebrew.gba --format json
cargo run -p nino -- execute path/to/authored-fixture.gba --steps 100
cargo run -p nino -- extract-tile path/to/homebrew.gba --offset 0x1000 --palette-offset 0x2000 --depth 4 --transparent-zero
```

`extract-tile` writes JSON to stdout, including indexed pixels, the complete
selected palette bank, resolved RGBA pixels, GVA candidate bytes as an integer
array, and provenance. Offsets are byte offsets in the ROM and accept decimal
or `0x` hexadecimal. The caller supplies the location of a 16-colour bank for
4bpp or a 256-colour table for 8bpp. Transparency is opaque unless explicitly
requested. Missing, duplicate, or invalid options fail with exit code 2;
out-of-range asset reads fail with exit code 4. `gva_smaller` compares encoded
GVA bytes with the tile plus complete palette bank; it certifies neither
rendering cost nor production fidelity. No output files are written.

Exit codes are 0 for a completed inspection, 2 for usage, 3 for I/O, 4 for
invalid input bounds, and 5 for output serialization. Header warnings do not
make inspection fail. Signature matches are weak evidence and may be data or
unused code. The logo field only reports whether the region is nonzero; it
does not validate Nintendo's logo bytes.

Static discovery starts at the ROM's ARM entry word only when it has a plausible
in-ROM branch target; otherwise it reports no trusted seed. The library can
also accept externally observed ARM or Thumb seeds. Its `complete` field remains
false even when no unresolved path was seen: unobserved paths, copied RAM
code, indirect targets, and interrupts may still exist. The instruction
budget is 16,384 records, and unsupported opcodes stop a path explicitly.

## Phase 0 reconciliation

`memory::Memory` supplies a separate bounded bus for default-configuration
256 KiB EWRAM and 32 KiB IWRAM, with mirrors throughout their respective
16 MiB regions. It supports aligned little-endian byte, halfword, and word
accesses. Plain cartridge ROM is read-only across the three 32 MiB wait-state
windows. Reads beyond the supplied ROM do not invent bus values: they report
a fault. Unaligned accesses also fault at this interface; CPU-specific
rotation and sign-extension behavior belongs in the instruction layer.
The bus is connected to ARM immediate-offset LDR/STR/LDRB/STRB, including
pre/post indexing, subtracting offsets, literal loads, and writeback.
PC data registers, register offsets, user-mode transfers, and unaligned
transfers are explicitly unsupported. Faults preserve instruction state and
writeback. Zeroed RAM is a fixture initial condition. BIOS, I/O, display memory, save hardware, cartridge
GPIO/EEPROM overrides, RAM configuration changes, and access timing are not
implemented. Faults include address, width, direction, and reason. Memory
mapping follows [GBATEK](https://mgba-emu.github.io/gbatek/).

`execute` runs at most 4096 instructions and writes JSON transitions containing
the ROM digest, source ranges, opcodes, and register/flag/mode state. Initial
registers are zero except PC=0x08000000; this is an authored fixture entry
condition, not BIOS startup. B, BL, BX, and the declared ARM data-processing
subset execute. Thumb immediate shifts, register/imm3 add and subtract,
imm8 MOV/CMP/ADD/SUB, conditional and unconditional branches, and BX also
execute. Each transition records its instruction mode and exact 2- or 4-byte
source range. Fetching outside the supplied ROM or at an
unaligned ARM address, and unsupported instructions stop explicitly. Exit 0
means the requested instruction budget was consumed, not that an application
completed. Exit 6 signals an execution stop, with a JSON report still written.
`run_with_memory` accepts explicit CPU state and a caller-owned bus for
deterministic continuation; the caller keeps that same bus when resuming.
`run` and the CLI start fresh zeroed RAM on each invocation. Reports include
initial/final RAM snapshots and per-instruction transfer address, width,
direction, and value. These are diagnostic snapshots, not a versioned portable
checkpoint. Instruction fetch still uses the first ROM mapping; execution
from RAM and other cartridge windows is not supported yet.
There is no hardware, BIOS, cycle accounting, or claim of game compatibility.
Thumb formats 6–11 now support literal loads, register and immediate offset
word/byte/halfword transfers, signed register-offset loads, and SP-relative
word transfers. They use the same bus and traces as ARM transfers and preserve
flags. Signed trace values contain the extended register value. Encoding and
address calculations follow the [GNU CGEN Thumb reference](https://sourceware.org/cgen/gen-doc/arm-thumb-insn.html).
Unaligned halfword/word semantics remain unsupported, including odd-address
signed halfword behavior. Thumb stack operations, ALU format 4, high-register arithmetic,
long branches with link, and exceptions remain unsupported and stop explicitly.

`arm_data::step` executes one ARM data-processing instruction against explicit
registers and flags. All 16 data-processing opcodes are implemented for rotated
immediates and registers with immediate shifts. R15 stores the current
instruction address; reads use the ARM pipeline value (address+8), and a
successful step advances it by four. Failed conditions advance the PC without
changing other state. PC writes, NV conditions, register-controlled shifts,
and other instruction classes return explicit errors; errors preserve state.
This subset has no instruction timing, memory access, exception handling, or
ROM execution loop and does not establish ARM conformance.

The `armv4t` module now supplies condition evaluation, add-with-carry arithmetic
(also usable for subtraction and SBC), and immediate/register barrel-shifter
operations. It distinguishes carry from signed overflow, uses the low byte of
register shift amounts, and handles encoded-zero LSR/ASR and RRX. These are
machine-semantic building blocks; they do not provide a CPU execution loop,
exceptions, memory bus, or guest-cycle model. Shift semantics follow the
[Arm instruction reference](https://support.arm.com/documentation/100076/0100/A32-T32-Instruction-Set-Reference/A32-and-T32-Instructions/Shift-operations?lang=en).

| Directive area | Current Goya contract | Integration decision |
| --- | --- | --- |
| `.gscene` | `goya-lang` GY-LANG/1 and GSSH/1–4-live model cells, state, and bounded interactions; `goya-resolver` admits those strict profiles | A GBA CPU, guest memory, frame/audio streams, and checkpoint hooks have no admitted scene representation. Request a versioned executable capability before translation output. |
| Graphics | `goya-gvi` GY-GVI/0 is a parent-scoped precomputed projection inventory of a GVA drawing and finite state model | Evaluate extracted assets against actual GVA/GVI APIs and cost budget. Retain raster output by default; no direct tile-to-GVI assumption. |
| Continuity | `goya-romantico` GY-ROMANTICO/0.1 holds semantic state with an in-process checkpoint contract; it has no authenticated network transport adapter | Define guest checkpoint fields and consistent scheduler boundary before binding. Do not claim cross-device handoff. |
| Execution | `goya-executable` and Resolver expose bounded D1/D2 interaction programs | A versioned guest execution interface, platform admission, resource quotas, and state ownership contract are required. |

### Core capability request

Goya needs a declared application module that can carry a bounded guest
runtime with frame, audio, input, persistent storage, and checkpoint ports.
It must be admitted by `goya-lang`/Resolver and each target independently.
Until then, emitting a visual `.gscene` wrapper would misrepresent execution
and fail the directive's behaviour-equivalence requirement.

### Next gate

Build an independently authored homebrew fixture corpus, implement complete
ARM/Thumb semantics plus guest memory and timing, and compare deterministic
traces with mGBA. Only then add `.gscene` emission through an approved module
contract. No translated application or compatibility claim is made here.

Header layout and complement follow
[GBATEK](https://mgba-emu.github.io/gbatek/). The source treats malformed
headers as warnings so inspection remains separate from execution eligibility.
Tile, palette, and text map layouts follow the same [GBATEK graphics
sections](https://mgba-emu.github.io/gbatek/).
