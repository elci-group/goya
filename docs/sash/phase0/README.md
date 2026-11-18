# Sash Phase 0 — Windows Execution Environment

**Directive:** SASH-ENT-001  
**Programme Record:** SASH-ENT-002  
**Status:** Ready for execution (infrastructure pending)

---

## Quick Start

### 1. Provision Windows Machines
Follow [`PROVISIONING_RUNBOOK.md`](PROVISIONING_RUNBOOK.md) to set up:
- **x64**: Windows 11 physical or dedicated VM
- **i686**: Windows 10/11 VM (32-bit or WoW64)
- **ARM64**: **Physical ARM64 device required** (no emulation)

### 2. Transfer Artifacts
```bash
# From Linux build host
cd /home/sal/goya
./docs/sash/phase0/harness/transfer_artifacts.sh --method ssh
# Or: --method winrm, --method smb
```

The transfer script also generates `expectations.json` beside each `App.exe`
with `sash expectations <scene> --format json`. That file carries the window
class and title, the control table (native id, role, semantic id, text, DIP
rectangle), and the import closure — all projected from the lowered IR by the
same compiler revision that produced the binary. The launch harness refuses to
run without it: a hand-maintained expectations table could silently drift from
what the emitted program actually does.

### 3. Run Tests on Windows
On each Windows machine (as `sash-test` user):
```powershell
# Run all tests for one arch/profile
.\run_all_tests.ps1 -Arch x64 -Profile static-window

# Run all tests for interactive profile
.\run_all_tests.ps1 -Arch arm64 -Profile interactive-cut

# i686 skips DPI test automatically
.\run_all_tests.ps1 -Arch i686 -Profile static-window
```

### 4. Collect Evidence
```bash
# From Linux host (after tests complete)
python3 docs/sash/phase0/harness/collect_evidence.py \
  --results-dir //win-test-x64/sash-test/results \
  --output-dir ./phase0_evidence
```

### 5. CI/CD Gate
Trigger the Phase 0 validation workflow:
```bash
gh workflow run sash-phase0.yml -f arch=all -f profile=all
```

---

## Directory Structure

```
docs/sash/phase0/
├── PROVISIONING_RUNBOOK.md          # Human/platform team checklist
├── README.md                         # This file
└── harness/
    ├── launch_test.ps1              # Window launch + control validation
    ├── capture_a11y.ps1             # UI Automation tree capture
    ├── dpi_test.ps1                 # PerMonitorV2 DPI transition test
    ├── capture_imports.ps1          # Static/dynamic import closure
    ├── run_all_tests.ps1            # Master test orchestrator
    ├── collect_evidence.py          # Evidence packaging (Linux)
    ├── transfer_artifacts.sh        # Artifact transfer (Linux → Windows)
```

---

## Test Matrix

| Architecture | Profile | Launch | Import Closure | Accessibility | DPI Test |
|--------------|---------|--------|----------------|---------------|----------|
| x64 | static-window | ✅ | ✅ | ✅ | ✅ |
| x64 | interactive-cut | ✅ | ✅ | ✅ | ✅ |
| i686 | static-window | ✅ | ✅ | ✅ | ⏭️ Skipped |
| i686 | interactive-cut | ✅ | ✅ | ✅ | ⏭️ Skipped |
| ARM64 | static-window | ✅ | ✅ | ✅ | ✅ |
| ARM64 | interactive-cut | ✅ | ✅ | ✅ | ✅ |

**✅ Required** | **⏭️ Skipped** (i686 DPI not required)

---

## Evidence Artifacts (Per Arch/Profile)

| File | Purpose |
|------|---------|
| `*-launch.log` | Launch test console output |
| `*-evidence.json` | Structured launch checks (PASS/FAIL) |
| `*-imports.log` | Import capture console output |
| `*-import_closure.json` | Static/dynamic imports + PE info |
| `*-a11y.log` | Accessibility capture console output |
| `*-a11y_snapshot.json` | Full UIA tree with AutomationId, Name, ControlType, BoundingRectangle |
| `*-dpi_test.log` | DPI transition console output |
| `*-dpi_evidence.json` | Font recreation + reposition verification per DPI |
| `*-master.log` | Master orchestrator console output |
| `*-master_evidence.json` | Consolidated PASS/FAIL for all tests |

---

## Phase 0 Gate Criteria

**PASS** requires ALL of the following for **each** architecture/profile:

| Criterion | Validation |
|-----------|------------|
| Binary launches | Process starts, window appears, title matches |
| Window class correct | `SashStaticWindow` or `SashInteractiveWindow` |
| Controls at expected DIPs | Visual match to `goya_lang::layout` output |
| System message font used | `WM_GETFONT` returns non-zero on STATIC controls |
| Clean exit (code 0) | `UnregisterClassW`, font cleanup, handles released |
| Import closure correct | Only KERNEL32, USER32, GDI32, ntdll, api-ms-win-core-synch-l1-2-0 |
| **No CRT redistributables** | MSVCRT, VCRUNTIME, ucrtbase absent |
| Accessibility baseline | UIA tree has expected controls; AutomationId = semantic id |
| DPI transition (x64/ARM64) | Font recreated, controls repositioned on WM_DPICHANGED |
| PE structure valid | PE32/PE32+ GUI, correct machine, DLL characteristics |

---

## CI/CD Integration

The workflow [`.github/workflows/sash-phase0.yml`](../../../.github/workflows/sash-phase0.yml) provides:

1. **Linux verification** — artifacts exist, `sash verify` passes
2. **Windows execution** — runs on self-hosted runners per architecture
3. **Evidence consolidation** — collects all JSON evidence
4. **Gate decision** — `PASS` = proceed to Phase 1; `FAIL` = block

### Required GitHub Setup

1. **Self-hosted Windows runners** with labels: `x64`, `i686`, `arm64`
2. **Clean machine snapshots** on each runner (see provisioning runbook)
3. **Artifact transfer** configured (SSH/WinRM/SMB) or use `actions/download-artifact`

---

## Artifacts to Test (from SASH-ENT-002)

| Arch | Profile | Source Path (Linux) |
|------|---------|---------------------|
| x64 | static-window | `/tmp/sash-ent002/static-window/App.exe` |
| x64 | interactive-cut | `/tmp/sash-ent002/interactive-cut/App.exe` |
| i686 | static-window | `/tmp/sash-stamp-i686/App.exe` |
| i686 | interactive-cut | `/tmp/sash-stamp-ix86/App.exe` |
| ARM64 | static-window | `/tmp/sash-stamp-arm64/App.exe` |
| ARM64 | interactive-cut | `/tmp/sash-stamp-iarm64/App.exe` |

**Verify before transfer:**
```bash
sash verify /tmp/sash-stamp-i686/App.exe --format json
# Must show: target="i686-pc-windows-msvc", presentation_profile="sash.win32.static-window/0.0.3"
```

---

## Re-Linking (If Artifacts Stale)

```bash
# Static window, any arch
sash build crates/sash/fixtures/static-window.gscene \
  --profile portable-native --release \
  --out /tmp/sash-static-fresh \
  --target x86_64-pc-windows-msvc  # or i686-pc-windows-msvc, aarch64-pc-windows-msvc

# Interactive counter
sash build crates/sash/fixtures/interactive-counter.gscene \
  --profile portable-native --release \
  --out /tmp/sash-interactive-fresh \
  --target x86_64-pc-windows-msvc

# Verify
sash verify /tmp/sash-static-fresh/App.exe --evidence /tmp/sash-static-fresh/evidence
```

---

## Next Phase (After Phase 0 PASS)

| Swarm | Package | Objective |
|-------|---------|-----------|
| **J** | SASH-J02 | Execute static-window on Windows x64 |
| **B** | SASH-B01 | Qualify static-window runtime |
| **I** | SASH-I02 | Security & accessibility qualification |

See [Roadmap](../roadmap.md) for full Phase 1–5 plan.

---

## Troubleshooting

| Issue | Resolution |
|-------|------------|
| Window doesn't appear | Check `procmon` for `CreateWindowExW` failure; verify DPI awareness context |
| Font not recreated on DPI change | Verify `SystemParametersInfoForDpi` + `CreateFontIndirectW` in generated code |
| CRT imports present | Re-link with `panic = "abort"`, `lto = false`, static CRT (current config should prevent) |
| AutomationId missing | Check `SetPropW(hwnd, "Goya.SemanticId", ...)` in generated `setup()` |
| ARM64 runner not found | Provision physical ARM64; add self-hosted runner with label `arm64` |

---

## Contact

| Role | Contact |
|------|---------|
| Phase 0 Owner | Platform/Infra Team |
| Sash Compiler | @compiler-team |
| Directive | SASH-ENT-001 |

---

*Generated: 2026-11-18 | Directive: SASH-ENT-001*