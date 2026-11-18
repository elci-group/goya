# Phase 0 Provisioning Runbook — Windows Execution Environment

**Directive:** SASH-ENT-001  
**Programme Record:** SASH-ENT-002  
**Owner:** Platform/Infra Team  
**Target Completion:** Week 2

---

## Objective

Provision **three clean Windows machines** (physical or dedicated VMs) capable of launching and observing Sash-generated executables for runtime qualification.

| Architecture | Machine Type | Purpose |
|--------------|--------------|---------|
| **x86_64** (x64) | Physical or dedicated VM | Primary qualification target |
| **i686** (x86) | VM on x64 host | 32-bit path validation (`SetWindowLongW`) |
| **aarch64** (ARM64) | **Physical ARM64 required** | Native ARM64 qualification (emulation invalid) |

---

## Machine Requirements

### All Machines (Common Baseline)

| Requirement | Specification |
|-------------|---------------|
| **OS** | Windows 11 23H2 or Windows 10 22H2 (fully patched) |
| **Install Type** | Clean install — no SDK, no Visual Studio, no dev tools, no Rust, no cargo-xwin |
| **User** | Standard (non-admin) account for test execution |
| **Network** | Internet access for Windows Update only; isolate after patching |
| **Snapshot** | Take clean snapshot **after** patching, **before** any test artifacts |
| **Hardware** | Dedicated resources (no shared GPU/CPU contention during test) |
| **Display** | Physical or virtual display ≥ 1920×1080; multi-monitor for DPI cross-move tests |

### Architecture-Specific

| Arch | Additional Requirements |
|------|------------------------|
| **x64** | PerMonitorV2 support (Win10 1703+); Narrator; high contrast themes |
| **i686** | 32-bit OS or 64-bit OS with WoW64; test `SetWindowLongW` code path |
| **ARM64** | **Physical ARM64 device** (Snapdragon Dev Kit, Surface Pro X, Ampere workstation, or equivalent). **No x64 emulation.** Must run native ARM64 PE32+. |

---

## Provisioning Checklist (Per Machine)

### 1. Clean Install & Patch
- [ ] Install Windows from official ISO (no OEM bloatware)
- [ ] Create standard user: `sash-test` / (strong password)
- [ ] Run Windows Update until "You're up to date"
- [ ] Disable automatic updates (pause for 35 days via Settings)
- [ ] Disable Windows Defender real-time protection (temporarily, for clean import capture)
- [ ] Disable SmartScreen (App & browser) — re-enable after import capture
- [ ] **Snapshot: `CLEAN_PATCHED`**

### 2. Test Tooling Installation (Minimal)
- [ ] Install **Windows SDK** (latest) — **only** for `dumpbin`, `depends.exe`, `signtool` — **not** for building
- [ ] Install **Sysinternals Suite** (`procexp`, `procmon`, `vmmap`, `strings`)
- [ ] Install **Windows Performance Toolkit** (WPT) — for ETW tracing
- [ ] Install **UI Automation tools**: `Inspect.exe` (from SDK), `AccScope` (optional)
- [ ] Install **Narrator** (built-in) — verify it starts with `Win+Ctrl+Enter`
- [ ] Install **Python 3.11+** (for test harness) — `Add to PATH`
- [ ] **Snapshot: `TOOLED`**

### 3. Artifact Staging Directory
- [ ] Create `C:\sash-test\artifacts\` (writable by `sash-test`)
- [ ] Create `C:\sash-test\results\` (writable by `sash-test`)
- [ ] Create `C:\sash-test\logs\` (writable by `sash-test`)
- [ ] Share `C:\sash-test\artifacts\` as read-only network share for Linux host transfer (or use WinRM/SSH)

### 4. Verification of Clean Import Closure
- [ ] Run `depends.exe /c /ot:C:\sash-test\results\clean-imports.txt C:\Windows\System32\notepad.exe`
- [ ] Verify no unexpected DLLs (no `MSVCRT*.dll`, no `VCRUNTIME*.dll`, no `ucrtbase.dll` in Notepad's closure on clean Windows)
- [ ] Record baseline system DLL set

---

## Artifacts to Transfer (from Linux Host)

**Source:** SASH-ENT-002 build outputs (or re-link if stale)

| Architecture | Binary | Source Path (Linux) | Target Path (Windows) |
|--------------|--------|---------------------|----------------------|
| x64 | `App.exe` (static) | `/tmp/sash-ent002/static-window/App.exe` | `C:\sash-test\artifacts\x64\static-window\App.exe` |
| x64 | `App.exe` (interactive) | `/tmp/sash-ent002/interactive-cut\App.exe` | `C:\sash-test\artifacts\x64\interactive-cut\App.exe` |
| i686 | `App.exe` (static) | `/tmp/sash-stamp-i686/App.exe` | `C:\sash-test\artifacts\i686\static-window\App.exe` |
| i686 | `App.exe` (interactive) | `/tmp/sash-stamp-ix86/App.exe` | `C:\sash-test\artifacts\i686\interactive-cut\App.exe` |
| ARM64 | `App.exe` (static) | `/tmp/sash-stamp-arm64/App.exe` | `C:\sash-test\artifacts\arm64\static-window\App.exe` |
| ARM64 | `App.exe` (interactive) | `/tmp/sash-stamp-iarm64\App.exe` | `C:\sash-test\artifacts\arm64\interactive-cut\App.exe` |

**Verify before transfer:**
```bash
# On Linux host
sash verify /tmp/sash-stamp-i686/App.exe --format json
# Must report: target="i686-pc-windows-msvc", presentation_profile="sash.win32.static-window/0.0.3", pe_structure=PASS
```

---

## Test Execution Protocol (Per Binary)

### Pre-Launch (Each Binary)
1. Copy binary to `C:\sash-test\artifacts\<arch>\<profile>\App.exe`
2. Run **import capture**:
   ```powershell
   # Capture dynamic imports
   procmon.exe /AcceptEula /Quiet /BackingFile C:\sash-test\logs\<arch>-<profile>-procmon.pml
   # Launch App.exe, wait 5s, close procmon
   ```
3. Run **static import analysis**:
   ```powershell
   dumpbin /imports C:\sash-test\artifacts\<arch>\<profile>\App.exe > C:\sash-test\results\<arch>-<profile>-imports.txt
   depends.exe /c /ot:C:\sash-test\results\<arch>-<profile>-depends.txt C:\sash-test\artifacts\<arch>\<profile>\App.exe
   ```

### Launch Test (Each Binary)
1. Start **ETW trace** (WPR):
   ```powershell
   wpr -start CPU -filemode C:\sash-test\logs\<arch>-<profile>-cpu.etl
   wpr -start ReferenceSet -filemode C:\sash-test\logs\<arch>-<profile>-mem.etl
   ```
2. Launch binary via test harness (see `launch_test.ps1`):
   ```powershell
   C:\sash-test\harness\launch_test.ps1 -Arch <arch> -Profile <profile> -Binary App.exe
   ```
3. Stop ETW trace:
   ```powershell
   wpr -stop C:\sash-test\logs\<arch>-<profile>-cpu.etl
   wpr -stop C:\sash-test\logs\<arch>-<profile>-mem.etl
   ```

### Post-Launch Evidence Collection
1. Run **accessibility snapshot**:
   ```powershell
   C:\sash-test\harness\capture_a11y.ps1 -Arch <arch> -Profile <profile>
   ```
2. Run **DPI transition test** (x64 only):
   ```powershell
   C:\sash-test\harness\dpi_test.ps1 -Arch x64 -Profile <profile>
   ```
3. Collect all logs/results to `C:\sash-test\results\<arch>-<profile>\`

---

## Sign-Off Criteria (Per Architecture)

| Architecture | Static-Window | Interactive | DPI Test | A11y Test | Import Closure |
|--------------|---------------|-------------|----------|-----------|----------------|
| **x64** | ✅ PASS | ✅ PASS | ✅ PASS | ✅ PASS | ✅ Documented |
| **i686** | ✅ PASS | ✅ PASS | N/A | ✅ PASS | ✅ Documented |
| **ARM64** | ✅ PASS | ✅ PASS | ✅ PASS | ✅ PASS | ✅ Documented |

**PASS Definition:**
- Process starts, window appears, title matches scene name
- No crash, no unhandled exception (exit code 0)
- Controls render at expected DIP positions (visual match)
- System message font used (not default GUI font)
- DPI change (96→120→144→192) recreates font, repositions controls
- Narrator reads window title and all labels
- High contrast mode renders correctly
- Keyboard focus order correct (Tab for interactive)
- Import closure matches expected set (no CRT redistributables)
- Clean exit: `UnregisterClassW`, font cleanup, handle count baseline

---

## Delivery to Linux Host

Package and transfer back:
```
C:\sash-test\results\<arch>-<profile>\
  ├── launch.log              # Test harness output
  ├── imports.txt             # dumpbin /imports
  ├── depends.txt             # depends.exe output
  ├── procmon.pml             # Process Monitor trace (optional, large)
  ├── cpu.etl                 # ETW CPU trace
  ├── mem.etl                 # ETW memory trace
  ├── a11y_snapshot.json      # UIA tree dump
  ├── dpi_test.log            # DPI transition log
  └── evidence.json           # Structured PASS/FAIL per criterion
```

Transfer via network share, WinRM, or physical media.

---

## Re-Linking Protocol (If Binary Stale)

If SASH-ENT-002 artifacts are unavailable or machine changed:
1. On Linux host: `sash build crates/sash/fixtures/static-window.gscene --profile portable-native --release --out /tmp/sash-static-fresh --target <triple>`
2. `sash verify /tmp/sash-static-fresh/App.exe --evidence /tmp/sash-static-fresh/evidence --format json`
3. Transfer new `App.exe` + `evidence/` to Windows machine
4. Repeat test protocol

---

## Timeline

| Week | Activity |
|------|----------|
| **Day 1–2** | Provision x64 machine, complete checklist |
| **Day 3–4** | Provision i686 VM, complete checklist |
| **Day 1–5** | **Acquire physical ARM64 device** (longest lead) |
| **Day 5–7** | Complete ARM64 provisioning |
| **Day 8–10** | Transfer artifacts, run static-window tests all arches |
| **Day 11–13** | Run interactive tests all arches |
| **Day 14** | DPI + accessibility deep dive (x64, ARM64) |
| **Day 15** | Package results, deliver to Linux host |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| ARM64 hardware unavailable | Start procurement Day 1; document as known limitation for v1 if truly blocked |
| DPI test fails on VM | Use physical monitor or Hyper-V enhanced session mode with DPI scaling |
| SmartScreen blocks launch | Disable during test; document as deployment gap for Phase 5 |
| Import closure differs | Capture on multiple clean machines; report variance |
| Narrator not available | Verify Windows edition includes accessibility (Pro/Enterprise) |

---

## Escalation Contacts

| Role | Name | Channel |
|------|------|---------|
| Platform Lead | TBD | — |
| ARM64 Hardware Owner | TBD | — |
| Sash Compiler Team | @compiler-team | — |

---

**Runbook Version:** 1.0  
**Last Updated:** 2026-11-18  
**Directive:** SASH-ENT-001