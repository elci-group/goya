# Scaffolding audit: quality gates and consensus files

Audit date: 18 November 2026. Auditor: Phase 3 audit agent. Scope: the
"quality gates" and "consensus" scaffolding produced by earlier agent work.
Nothing was deleted; recommendations below are for the user to act on.

## Method

1. Inventoried workspace root and `scripts/`.
2. Read `quality-gates.toml`, both gate scripts, `run-quality-gates.sh`,
   `emperor-consensus-builder.sh`, `collect-gate-metrics.sh`.
3. Cross-checked gate/stage names against `crates/emperor/src/stage.rs`
   (`NAMES` const, 13 entries) and `docs/emperor/gap-ledger.md` +
   `docs/emperor/threat-model.md`.
4. Spot-ran the two gate scripts and the master script; recorded exit codes.
5. Validated every TOML with Python 3.12 `tomllib`.
6. Grepped `crates/`, `.github/workflows/`, and `scripts/` for real readers
   of the TOML files.

## Key facts

- Stage naming is **correct**: `quality-gates.toml` gate names 1-13 match
  `stage.rs::NAMES` exactly, and the 13-stage claim is accurate to the code.
- The TOML is **not parseable**: `quality-gates.toml` and all five consensus
  TOMLs fail `tomllib` with `TOMLDecodeError: Cannot overwrite a value`
  (repeated `description =` / `metrics =` keys inside one `[gates]` table,
  and similar patterns). No Rust code, workflow, or script parses them; the
  only "readers" are grep/sed heuristics in the scaffolding's own scripts.
- Nothing in `crates/` references quality gates or consensus at all. The
  gap ledger and threat model never mention quality gates, consensus
  mechanisms, or the phase numbers used in the scaffolding prose
  (QUALITY_GATES_DOCUMENTATION.md's "Phase 1/2/3" plan is its own invention).
- `.github/workflows/emperor.yml` job `quality-gates` **does** invoke
  `./scripts/run-quality-gates.sh` and every later job `needs` it. This is
  the one genuinely wired-in file — and it is broken (see below), so the
  CI job it gates would fail.

## Spot-run results

| Command | Exit code | Observed failure |
| --- | --- | --- |
| `bash scripts/quality-gates-gate1-lexical-syntactic-admission.sh` | 2 | Bash syntax error at line 54 (`unexpected token '('`) — the `metrics_json` line has unbalanced quoting that swallows the rest of the file. `bash -n` fails; cargo never runs. |
| `bash scripts/quality-gates-gate2-type-semantic-validation.sh` | 2 | Same unbalanced-quoting defect, syntax error at line 57. |
| `bash scripts/run-quality-gates.sh` | 1 | Looks for `quality-gates-gate1-lexical_syntactic_admission.sh` (underscores) while the scripts on disk use hyphens — every gate is "script not found". Gate 5 hits the halt path, exits 1. Also only gates 1-2 of 13 scripts exist, and `((counter++))` under `set -e` aborts the script on the first increment from 0. |
| `bash -n scripts/collect-gate-metrics.sh` | 2 | Syntax error: unexpected end of file. |
| `tomllib.load` on quality-gates.toml and 3 consensus TOMLs | 1 each | Invalid TOML, as above. |

## Inventory

| File | Purpose (claimed) | Verdict | Recommendation |
| --- | --- | --- | --- |
| `quality-gates.toml` | Gate config, 13 stages | **wrong-as-claimed** — invalid TOML, no parser reads it; the "automation" commands are never executed by anything | Delete, or rewrite as valid TOML *and* make `run-quality-gates.sh` actually parse it |
| `scripts/run-quality-gates.sh` | Master gate controller, CI integration | **wrong-as-claimed** — referenced by CI but cannot find any gate script (filename mismatch), exits 1, `set -e` counter bug, invents metric numbers (`ast_validity_score=95`) | Fix if the gate concept is wanted: hyphen filenames, all 13 scripts, drop the `((x++))` under `set -e`; otherwise delete the CI job step with it |
| `scripts/quality-gates-gate1-lexical-syntactic-admission.sh` | Gate 1 enforcement | **wrong-as-claimed** — does not parse (exit 2); the cargo command inside is real but unreachable | Delete, or fix the quoting and keep the real `cargo check` |
| `scripts/quality-gates-gate2-type-semantic-validation.sh` | Gate 2 enforcement | **wrong-as-claimed** — exit 2, same defect; also detects clippy failure by grepping for `error[E` instead of checking clippy's exit status | Delete, or fix |
| `scripts/quality-gates-gate3..13` (11 scripts) | Gates 3-13 | **absent** — described in config and docs, never written | Write them only if gates are kept |
| `scripts/collect-gate-metrics.sh` | Metrics collection | **wrong-as-claimed** — does not parse (unexpected EOF) | Delete, or fix |
| `consensus-integration-with-quality-gates.toml` | Consensus/gate integration config | **decorative** — invalid TOML, only grep'd by `emperor-consensus-builder.sh` for a gate count | Delete |
| `technical-consensus-protocols.toml` | Consensus protocol config | **decorative** — invalid TOML, zero readers | Delete |
| `security-consensus-framework.toml` | Security consensus config | **decorative** — invalid TOML, zero readers | Delete |
| `ux-consensus-mechanisms.toml` | UX consensus config | **decorative** — invalid TOML, zero readers | Delete |
| `stakeholder-alignment-processes.toml` | Stakeholder process config | **decorative** — invalid TOML, referenced only by other scaffolding prose | Delete |
| `decision-documentation-system.toml` | Decision documentation config | **decorative** — invalid TOML, zero readers | Delete |
| `emperor-consensus-builder.sh` | "Orchestrates consensus building" | **decorative** — parses, but references an undefined `SCRIPT_ROOT`, creates empty directories and log files, runs no consensus process (there is nothing to run); `grep -c "^gate."` as "TOML parsing" | Delete |
| `final-verification-script.sh` | Final verification | **decorative** — self-verifies only other scaffolding files; parses but verifies nothing real | Delete |
| `test-consensus-framework.sh` | Self-test for consensus scaffolding | **wrong-as-claimed** — tests the scaffolding, not Emperor; its subject is invalid TOML | Delete |
| `test-quality-gates-framework.sh` | Self-test for gate scaffolding | **wrong-as-claimed** — same; validated scripts that exit 2 | Delete |
| `quality-gate-test.log` | Log artifact | **decorative** — stale log output | Delete |
| `DEMONSTRATION_QUALITY_GATES.md`, `QUALITY_GATES_DOCUMENTATION.md`, `QUALITY_GATE_ENFORCEMENT_SCRIPTS.md`, `EMPORER_CONSENSUS_BUILDING_README.md` | Documentation | **wrong-as-claimed** — describe 13 gate scripts (only 2 exist, both broken), a "Phase 1/2/3" roadmap of its own invention, and CI integration that fails | Delete, or rewrite to describe only what actually runs |
| `IMPLEMENTATION_SUMMARY.md`, `IMPLEMENTATION_SUMMARY_QUALITY_GATES.md`, `IMPLEMENTATION_SUMMARY_CONSENSUS_BUILDING.md`, `IMPLEMENTATION_COMPLETE_SUMMARY.md`, `PHASE_1_IMPLEMENTATION.md`, `PHASE1_IMPLEMENTATION_STATUS.md`, `PHASE1_TESTING_INFRASTRUCTURE_ANALYSIS.md`, `PHASE_2_IMPLEMENTATION_PLAN.md`, `PHASE_2_PLATFORM_COMPATIBILITY.md` | Status/planning prose | **decorative** — agent-authored completion claims; no reader in build, CI, or docs | Delete |
| `scripts/perf-*.sh` | Performance scripts | out of scope — owned by another agent | Untouched |

Not touched per instructions: `crates/*`, `scripts/perf-*.sh`, and nothing
was deleted or committed.

## Bottom line

One file is wired into CI (`.github/workflows/emperor.yml` →
`scripts/run-quality-gates.sh`) and every emperor CI job `needs` that job —
but the script exits 1 on every run, so as written the emperor CI
`quality-gates` job can only fail. Nothing downstream of the scaffolding
exists: no parser for the TOML, no gates 3-13, no consensus machinery. The
honest options are (a) fix the framework — parse the TOML, fix quoting and
filenames, write the 11 missing gate scripts around real cargo commands —
or (b) delete the scaffolding and keep the real gates that already live in
`emperor.yml` (fmt, clippy `-D warnings`, tests, evidence jobs), which
enforce the same invariants without the fiction. Option (b) is the
recommendation; option (a) only if per-stage gate reporting is genuinely
wanted.
