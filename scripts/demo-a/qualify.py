#!/usr/bin/env python3
"""Fail-closed orchestration for GY-DEMO-A-001.

Build success never promotes a native execution gate. A PASS row without
observed input evidence is rejected. This process does not invent a Windows
session, a Wayland seat event, or an independent review.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

DIRECTIVE = "GY-DEMO-A-001"
DIRECTIVE_VERSION = "1.0.0"
INSPECTED_BASELINE = "018a319dd384fb116f455cc628bb3146435ec05c"
SCENE_RELATIVE = Path("crates/sash/fixtures/interactive-counter.gscene")
WINDOWS_TRIPLE = "x86_64-pc-windows-msvc"
LINUX_TRIPLE = "x86_64-unknown-linux-gnu"

TESTS = [
    ("DA-T01", "Launch application", "Window visible, input 1, result 0"),
    ("DA-T02", "Click Add once", "Result 1"),
    ("DA-T03", "Click Add again", "Result 2"),
    ("DA-T04", "Focus button; activate by Space", "Result 3"),
    ("DA-T05", "Replace input 1 with 2; activate by Enter", "Result 5"),
    ("DA-T06", "Replace input with 00; activate", "Result remains 5"),
    ("DA-T07", "Paste a known sentinel into the numeric input", "Paste denied; input unchanged"),
    ("DA-T08", "Copy the focused input", "Clipboard matches the input text"),
    ("DA-T09", "Close the window", "Clean process termination"),
    ("DA-T10", "Launch again", "Initial state restored to 0"),
]

BASELINE_COMMANDS = [
    ["cargo", "build", "--workspace"],
    ["cargo", "test", "--workspace", "--lib", "--tests"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["cargo", "fmt", "--all", "--", "--check"],
]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def evidence_home() -> Path:
    override = os.environ.get("DEMO_A_HOME")
    if override:
        return Path(override)
    return repo_root() / "evidence" / "demo-a"


def read_json(path: Path) -> dict | None:
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def run_cmd(argv: list[str], cwd: Path, env: dict | None = None, timeout: int | None = None) -> dict:
    started = time.monotonic()
    merged = os.environ.copy()
    if env:
        merged.update(env)
    try:
        completed = subprocess.run(
            argv,
            cwd=cwd,
            env=merged,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        return {
            "argv": argv,
            "exit_code": 124,
            "stdout": exc.stdout or "",
            "stderr": (exc.stderr or "") + "\ncommand timed out\n",
            "seconds": round(time.monotonic() - started, 3),
        }
    return {
        "argv": argv,
        "exit_code": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
        "seconds": round(time.monotonic() - started, 3),
    }


def git_output(root: Path, args: list[str]) -> str:
    result = run_cmd(["git", *args], root)
    return result["stdout"].strip() if result["exit_code"] == 0 else ""


def current_run_id(required: bool = True) -> str:
    pointer = evidence_home() / "CURRENT"
    if pointer.is_file():
        run_id = pointer.read_text(encoding="utf-8").strip()
        if run_id:
            return run_id
    if required:
        raise SystemExit("no Demo A run; start with scripts/demo-a/preflight")
    return ""


def run_dir(run_id: str) -> Path:
    return evidence_home() / run_id


def scene_path(root: Path) -> Path:
    return root / SCENE_RELATIVE


def blank_tests(platform: str, reason: str) -> list[dict]:
    rows = []
    for test_id, procedure, expected in TESTS:
        rows.append(
            {
                "id": test_id,
                "platform": platform,
                "procedure": procedure,
                "expected": expected,
                "observed": "",
                "input_method": "",
                "evidence": reason,
                "status": "BLOCKED",
            }
        )
    return rows


def normalise_tests(rows: list[dict]) -> list[dict]:
    known = {test_id: (procedure, expected) for test_id, procedure, expected in TESTS}
    by_id = {row.get("id"): row for row in rows}
    normalised = []
    for test_id, procedure, expected in TESTS:
        row = dict(by_id.get(test_id) or {})
        row["id"] = test_id
        row["procedure"] = procedure
        row["expected"] = expected
        status = row.get("status") or "BLOCKED"
        if status not in {"PASS", "FAIL", "BLOCKED"}:
            status = "FAIL"
            row["fault"] = "DA-E013"
        if status == "PASS" and not (
            str(row.get("observed") or "").strip()
            and str(row.get("evidence") or "").strip()
            and str(row.get("input_method") or "").strip()
        ):
            status = "FAIL"
            row["fault"] = "DA-E013"
            row["evidence"] = "PASS rejected: observation, input method, or evidence is empty"
        row["status"] = status
        row["procedure"] = known[test_id][0]
        normalised.append(row)
    return normalised


def gate(gate_id: str, status: str, detail: str, fault: str | None = None) -> dict:
    if status not in {"PASS", "FAIL", "BLOCKED"}:
        raise ValueError(status)
    return {"id": gate_id, "status": status, "detail": detail, "fault": fault}


def decide(gates: list[dict]) -> dict:
    if len(gates) != 10:
        raise ValueError("Demo A has ten mandatory gates")
    statuses = [item["status"] for item in gates]
    if any(item not in {"PASS", "FAIL", "BLOCKED"} for item in statuses):
        qualification = "FAIL"
    elif "FAIL" in statuses:
        qualification = "FAIL"
    elif "BLOCKED" in statuses:
        qualification = "BLOCKED"
    else:
        qualification = "PASS"
    evidenced = any(
        item["status"] == "PASS" and item["id"] in {f"DA-G0{number}" for number in range(2, 10)}
        for item in gates
    )
    if qualification == "PASS":
        programme = "PASS"
    elif evidenced:
        programme = "PARTIAL"
    else:
        programme = qualification
    return {
        "directive": DIRECTIVE,
        "directive_version": DIRECTIVE_VERSION,
        "qualification_status": qualification,
        "programme_summary": programme,
        "completion_statement_issued": qualification == "PASS",
        "gates": gates,
    }


def core_status(gates: list[dict]) -> str:
    statuses = [item["status"] for item in gates]
    if any(item not in {"PASS", "FAIL", "BLOCKED"} for item in statuses) or "FAIL" in statuses:
        return "FAIL"
    if "BLOCKED" in statuses or not statuses:
        return "BLOCKED"
    return "PASS"


def windows_structure_gate(verify: dict | None) -> dict:
    if verify is None:
        return gate("DA-G03", "BLOCKED", "Sash verify has not run.", "DA-E013")
    code = verify.get("exit_code")
    body = verify.get("json") or {}
    runtime = body.get("runtime_qualification")
    structure = body.get("pe_structure")
    target = body.get("target")
    if code == 0 or runtime == "PASS":
        return gate(
            "DA-G03",
            "FAIL",
            "Sash verify reported a runtime pass or exit 0. Static verification stays exit 3 with runtime BLOCKED.",
            "DA-E013",
        )
    if code == 3 and structure == "PASS" and runtime == "BLOCKED" and target == WINDOWS_TRIPLE:
        return gate(
            "DA-G03",
            "PASS",
            "PE_STRUCTURE_PASS / WINDOWS_RUNTIME_BLOCKED. The static verifier did not launch Windows.",
        )
    if code == 3:
        return gate(
            "DA-G03",
            "FAIL",
            f"verify exit 3 did not match x64 structure: target={target} pe={structure} runtime={runtime}",
            "DA-E004",
        )
    return gate(
        "DA-G03",
        "FAIL" if code not in {None} else "BLOCKED",
        f"sash verify exit {code}: {verify.get('stderr', '')[:500]}",
        "DA-E004",
    )


def elf_gate(facts: dict | None) -> dict:
    if not facts:
        return gate("DA-G04", "BLOCKED", "ELF inspection has not run.", "DA-E013")
    if facts.get("missing_libraries"):
        return gate(
            "DA-G04",
            "FAIL",
            "dynamic libraries are missing: " + ", ".join(facts["missing_libraries"]),
            "DA-E015",
        )
    ok = (
        facts.get("magic") == "ELF"
        and facts.get("elf_class") == 64
        and facts.get("machine") == 62
        and facts.get("elf_type") in {2, 3}
        and facts.get("executable_stack") is False
    )
    if ok:
        return gate(
            "DA-G04",
            "PASS",
            f"ELF x64 type {facts.get('elf_type')} entry {facts.get('entry')} relro {facts.get('relro')}",
        )
    return gate("DA-G04", "FAIL", f"ELF facts do not match x64: {facts}", "DA-E004")


def launch_gate(gate_id: str, launch: dict | None, absent_detail: str) -> dict:
    if not launch:
        return gate(gate_id, "BLOCKED", absent_detail, "DA-E005")
    if launch.get("fault") in {"DA-E006", "DA-E007", "DA-E012"} or launch.get("status") == "FAIL":
        return gate(gate_id, "FAIL", launch.get("detail") or launch.get("status") or "launch failed", launch.get("fault"))
    window = launch.get("window")
    evidence = str(launch.get("window_evidence") or "").strip()
    if window == "VISIBLE" and evidence:
        return gate(gate_id, "PASS", evidence)
    if launch.get("status") == "BLOCKED":
        return gate(gate_id, "BLOCKED", launch.get("detail") or "execution environment unavailable", launch.get("fault") or "DA-E005")
    return gate(
        gate_id,
        "BLOCKED",
        "Process evidence does not establish a visible native window.",
        "DA-E007",
    )


def interaction_gate(gate_id: str, rows: list[dict] | None) -> dict:
    if not rows:
        return gate(gate_id, "BLOCKED", "Interaction matrix was not recorded.", "DA-E013")
    rows = normalise_tests(rows)
    if any(row["status"] == "FAIL" for row in rows):
        failed = [row["id"] for row in rows if row["status"] == "FAIL"]
        return gate(gate_id, "FAIL", "failed tests: " + ", ".join(failed), "DA-E009")
    if all(row["status"] == "PASS" for row in rows) and len(rows) == 10:
        return gate(gate_id, "PASS", "DA-T01 through DA-T10 passed with recorded observations.")
    blocked = [row["id"] for row in rows if row["status"] != "PASS"]
    return gate(gate_id, "BLOCKED", "not passed: " + ", ".join(blocked), "DA-E008")


def baseline_gate(record: dict | None) -> dict:
    if not record or not record.get("commands"):
        return gate("DA-G01", "BLOCKED", "Mandatory workspace baseline has not been recorded.", "DA-E014")
    bad = [item for item in record["commands"] if item.get("exit_code") != 0]
    if bad:
        rendered = ", ".join(f"{' '.join(item['argv'])} -> {item['exit_code']}" for item in bad)
        return gate("DA-G01", "FAIL", rendered, "DA-E014")
    if len(record["commands"]) != 4:
        return gate("DA-G01", "BLOCKED", "Baseline record does not contain the four mandatory commands.", "DA-E014")
    return gate("DA-G01", "PASS", "workspace build, lib/tests, clippy -D warnings, and fmt --check exited 0.")


def source_gate(fixture_sha: str, sash_sha: str | None, emperor_sha: str | None) -> dict:
    if not sash_sha or not emperor_sha:
        return gate("DA-G02", "BLOCKED", "One or both compilers have no recorded source digest.", "DA-E001")
    if fixture_sha == sash_sha == emperor_sha:
        return gate("DA-G02", "PASS", fixture_sha)
    return gate(
        "DA-G02",
        "FAIL",
        f"fixture {fixture_sha} sash {sash_sha} emperor {emperor_sha}",
        "DA-E001",
    )


def independent_gate(windows: dict | None, linux: dict | None) -> dict:
    def ready(record: dict | None) -> bool:
        if not record:
            return False
        return bool(record.get("independent_execution")) and record.get("status") in {
            "PROCESS_ALIVE",
            "CLEAN_EXIT",
        }

    if ready(windows) and ready(linux):
        return gate("DA-G09", "PASS", "Both artifacts were started without cargo, the compilers, or the scene file.")
    if (windows and windows.get("status") == "FAIL") or (linux and linux.get("status") == "FAIL"):
        return gate("DA-G09", "FAIL", "An independent launch was attempted and failed.", "DA-E006")
    return gate(
        "DA-G09",
        "BLOCKED",
        "Independent launch was not shown on both operating systems.",
        "DA-E005",
    )


def review_gate(review: dict | None, computed: str) -> dict:
    if not review or not review.get("independent"):
        return gate(
            "DA-G10",
            "BLOCKED",
            "No independent review file is present. The orchestrator does not review itself.",
            "DA-E013",
        )
    reviewed = review.get("decision")
    if reviewed != computed:
        return gate(
            "DA-G10",
            "FAIL",
            f"review decision {reviewed} does not match computed {computed}",
            "DA-E013",
        )
    if reviewed == "PASS" and computed != "PASS":
        return gate("DA-G10", "FAIL", "review promoted a PASS the evidence does not support", "DA-E013")
    return gate("DA-G10", "PASS", "Independent review matches the computed gate decision.")


def assemble_decision(path: Path) -> dict:
    fixture = repo_root() / SCENE_RELATIVE
    fixture_sha = sha256_file(fixture) if fixture.is_file() else ""
    sash_report = read_json(path / "builds" / "windows-x64" / "evidence" / "build-report.json")
    emperor_capability = read_json(path / "builds" / "linux-x64" / "capability-manifest.json")
    sash_sha = (sash_report or {}).get("source_digest")
    emperor_sha = (emperor_capability or {}).get("source_digest_sha256")
    windows_launch = read_json(path / "windows-x64" / "launch.json")
    linux_launch = read_json(path / "linux-x64" / "launch.json")
    windows_tests = normalise_tests((read_json(path / "windows-x64" / "interactions.json") or {}).get("tests") or [])
    linux_tests = normalise_tests((read_json(path / "linux-x64" / "interactions.json") or {}).get("tests") or [])
    preliminary = [
        baseline_gate(read_json(path / "conformance" / "baseline.json")),
        source_gate(fixture_sha, sash_sha, emperor_sha),
        windows_structure_gate(read_json(path / "windows-x64" / "verify.json")),
        elf_gate(read_json(path / "linux-x64" / "elf.json")),
        launch_gate("DA-G05", windows_launch, "Windows launch was not run on a Windows session."),
        launch_gate("DA-G06", linux_launch, "Linux Wayland launch was not recorded."),
        interaction_gate("DA-G07", windows_tests if (path / "windows-x64" / "interactions.json").is_file() else None),
        interaction_gate("DA-G08", linux_tests if (path / "linux-x64" / "interactions.json").is_file() else None),
        independent_gate(windows_launch, linux_launch),
    ]
    without_review = core_status(preliminary)
    reviewed = review_gate(read_json(path / "admission" / "review.json"), without_review)
    decision = decide(preliminary + [reviewed])
    decision["qualification_status_before_review"] = without_review
    return decision


def save_stream(path: Path, record: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    (path.parent / (path.name + ".stdout")).write_text(record.get("stdout") or "", encoding="utf-8")
    (path.parent / (path.name + ".stderr")).write_text(record.get("stderr") or "", encoding="utf-8")
    stored = {
        "argv": record["argv"],
        "exit_code": record["exit_code"],
        "seconds": record["seconds"],
        "stdout_path": str(path.parent / (path.name + ".stdout")),
        "stderr_path": str(path.parent / (path.name + ".stderr")),
    }
    stored["json"] = parse_json_document(record.get("stdout") or "")
    write_json(path, stored)


def parse_json_document(text: str) -> dict | None:
    stripped = text.strip()
    if not stripped:
        return None
    try:
        value = json.loads(stripped)
    except json.JSONDecodeError:
        value = None
    if isinstance(value, dict):
        return value
    decoder = json.JSONDecoder()
    for index, char in enumerate(stripped):
        if char != "{":
            continue
        try:
            found, _ = decoder.raw_decode(stripped[index:])
        except json.JSONDecodeError:
            continue
        if isinstance(found, dict):
            return found
    return None


def linker_env(path: Path) -> dict[str, str]:
    link = path / "link"
    link.mkdir(parents=True, exist_ok=True)
    source = Path("/usr/lib/x86_64-linux-gnu/libxkbcommon.so.0")
    destination = link / "libxkbcommon.so"
    if source.is_file() and not destination.exists():
        destination.symlink_to(source)
    env = {}
    if destination.is_symlink() or destination.is_file():
        previous = os.environ.get("LIBRARY_PATH", "")
        env["LIBRARY_PATH"] = str(link) if not previous else f"{link}:{previous}"
    return env


def cmd_preflight(args: argparse.Namespace) -> int:
    root = repo_root()
    revision = git_output(root, ["rev-parse", "HEAD"])
    if len(revision) != 40:
        raise SystemExit("git rev-parse HEAD did not return a revision")
    dirty = git_output(root, ["status", "--porcelain"])
    run_id = args.run_id or (
        datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        + "-"
        + revision[:12]
        + ("-dirty" if dirty else "")
    )
    path = run_dir(run_id)
    if path.exists() and not args.run_id:
        raise SystemExit(f"{path} already exists")
    path.mkdir(parents=True, exist_ok=True)
    for relative in [
        "source",
        "builds",
        "windows-x64/screenshots",
        "windows-x64/logs",
        "linux-x64/screenshots",
        "linux-x64/logs",
        "linux-x64/execution",
        "conformance",
        "admission",
    ]:
        (path / relative).mkdir(parents=True, exist_ok=True)
    scene = scene_path(root)
    digest = sha256_file(scene)
    shutil.copyfile(scene, path / "source" / "fixture.gscene")
    (path / "source" / "source.sha256").write_text(f"{digest}  {SCENE_RELATIVE}\n", encoding="utf-8")
    if sha256_file(path / "source" / "fixture.gscene") != digest:
        raise SystemExit("copied fixture digest changed")
    virt = run_cmd(["systemd-detect-virt"], root)
    os_release = Path("/etc/os-release").read_text(encoding="utf-8") if Path("/etc/os-release").is_file() else ""
    preflight = {
        "directive": DIRECTIVE,
        "directive_version": DIRECTIVE_VERSION,
        "inspected_baseline": INSPECTED_BASELINE,
        "source_revision": revision,
        "worktree_dirty": bool(dirty),
        "worktree_status": dirty,
        "source_sha256": digest,
        "scene": str(SCENE_RELATIVE),
        "recorded_at": now_iso(),
        "rustc": run_cmd(["rustc", "--version"], root)["stdout"].strip(),
        "cargo": run_cmd(["cargo", "--version"], root)["stdout"].strip(),
        "targets": run_cmd(["rustup", "target", "list", "--installed"], root)["stdout"].split(),
        "uname": run_cmd(["uname", "-a"], root)["stdout"].strip(),
        "os_release": os_release,
        "virt": {"exit_code": virt["exit_code"], "stdout": virt["stdout"].strip(), "stderr": virt["stderr"].strip()},
        "wayland_display": os.environ.get("WAYLAND_DISPLAY", ""),
        "session_type": os.environ.get("XDG_SESSION_TYPE", ""),
        "desktop": os.environ.get("XDG_CURRENT_DESKTOP", ""),
        "xwin_done": Path.home().joinpath(".cache/xwin/windows-msvc-sysroot/DONE").is_file(),
        "wine": shutil.which("wine") is not None,
        "qemu": shutil.which("qemu-system-x86_64") is not None,
        "virsh": shutil.which("virsh") is not None,
        "hostname": run_cmd(["hostname"], root)["stdout"].strip(),
    }
    write_json(path / "preflight.json", preflight)
    write_json(
        path / "manifest.json",
        {
            "directive": DIRECTIVE,
            "directive_version": DIRECTIVE_VERSION,
            "run_id": run_id,
            "inspected_baseline": INSPECTED_BASELINE,
            "source_revision": revision,
            "source_sha256": digest,
            "qualification_status": "NOT_ADMITTED",
            "note": "preflight is not an admission decision",
        },
    )
    (evidence_home() / "CURRENT").write_text(run_id + "\n", encoding="utf-8")
    print(run_id)
    return 0


def selected_run(args: argparse.Namespace) -> Path:
    run_id = args.run_id or current_run_id()
    path = run_dir(run_id)
    if not path.is_dir():
        raise SystemExit(f"missing run directory {path}")
    return path


def cmd_build_windows(args: argparse.Namespace) -> int:
    root = repo_root()
    path = selected_run(args)
    out = path / "builds" / "windows-x64"
    if out.exists():
        raise SystemExit(f"refusing to reuse {out}; Sash requires a new output directory")
    scene = str(scene_path(root))
    tests = run_cmd(["cargo", "test", "-p", "sash", "--offline", "--lib"], root, timeout=args.timeout)
    save_stream(path / "builds" / "sash-lib-tests.json", tests)
    check = run_cmd(
        [
            "cargo", "run", "-p", "sash", "--offline", "--",
            "check", scene, "--target", WINDOWS_TRIPLE, "--format", "json",
        ],
        root,
        timeout=args.timeout,
    )
    save_stream(path / "builds" / "sash-check.json", check)
    if check["exit_code"] != 0:
        write_json(path / "builds" / "sash.json", {"status": "FAIL", "stage": "check", "exit_code": check["exit_code"], "fault": "DA-E002"})
        return check["exit_code"] or 1
    build = run_cmd(
        [
            "cargo", "run", "-p", "sash", "--offline", "--",
            "build", scene,
            "--target", WINDOWS_TRIPLE,
            "--profile", "portable-native",
            "--release",
            "--out", str(out),
            "--format", "json",
        ],
        root,
        timeout=args.timeout,
    )
    save_stream(path / "builds" / "sash-build.json", build)
    exe = out / "App.exe"
    status = "LINKED" if build["exit_code"] == 0 and exe.is_file() else "FAIL"
    write_json(
        path / "builds" / "sash.json",
        {
            "status": status,
            "exit_code": build["exit_code"],
            "artifact": str(exe) if exe.is_file() else "",
            "sha256": sha256_file(exe) if exe.is_file() else "",
            "bytes": exe.stat().st_size if exe.is_file() else 0,
            "fault": None if status == "LINKED" else "DA-E003",
            "lib_tests_exit_code": tests["exit_code"],
        },
    )
    return 0 if status == "LINKED" else (build["exit_code"] or 1)


def cmd_build_linux(args: argparse.Namespace) -> int:
    root = repo_root()
    path = selected_run(args)
    out = path / "builds" / "linux-x64"
    if (out / "App").exists():
        raise SystemExit(f"refusing to overwrite {out / 'App'}")
    out.mkdir(parents=True, exist_ok=True)
    scene = str(scene_path(root))
    env = linker_env(path)
    tests = run_cmd(
        ["cargo", "test", "-p", "emperor", "-p", "emperor-runtime", "-p", "goya-parity", "--offline", "--lib", "--tests"],
        root,
        env=env,
        timeout=args.timeout,
    )
    save_stream(path / "builds" / "emperor-lib-tests.json", tests)
    check = run_cmd(
        ["cargo", "run", "-p", "emperor", "--offline", "--", "check", scene, "--target", LINUX_TRIPLE],
        root,
        env=env,
        timeout=args.timeout,
    )
    save_stream(path / "builds" / "emperor-check.json", check)
    if check["exit_code"] != 0:
        write_json(path / "builds" / "emperor.json", {"status": "FAIL", "stage": "check", "exit_code": check["exit_code"], "fault": "DA-E002"})
        return check["exit_code"] or 1
    interaction = run_cmd(
        ["cargo", "run", "-p", "emperor", "--offline", "--", "test", scene, "--suite", "interaction"],
        root,
        env=env,
        timeout=args.timeout,
    )
    save_stream(path / "builds" / "emperor-interaction.json", interaction)
    build = run_cmd(
        [
            "cargo", "run", "-p", "emperor", "--offline", "--",
            "build", scene, "--target", LINUX_TRIPLE, "--release", "--out", str(out),
        ],
        root,
        env=env,
        timeout=args.timeout,
    )
    save_stream(path / "builds" / "emperor-build.json", build)
    app = out / "App"
    status = "LINKED" if build["exit_code"] == 0 and app.is_file() else "FAIL"
    write_json(
        path / "builds" / "emperor.json",
        {
            "status": status,
            "exit_code": build["exit_code"],
            "interaction_exit_code": interaction["exit_code"],
            "interaction_is_native_input": False,
            "artifact": str(app) if app.is_file() else "",
            "sha256": sha256_file(app) if app.is_file() else "",
            "bytes": app.stat().st_size if app.is_file() else 0,
            "fault": None if status == "LINKED" else "DA-E003",
            "lib_tests_exit_code": tests["exit_code"],
            "library_path": env.get("LIBRARY_PATH", ""),
        },
    )
    return 0 if status == "LINKED" else (build["exit_code"] or 1)


def parse_elf(app: Path) -> dict:
    header = app.read_bytes()[:64]
    if header[:4] != b"\x7fELF":
        return {"magic": "NOT_ELF"}
    elf_type = int.from_bytes(header[16:18], "little")
    machine = int.from_bytes(header[18:20], "little")
    entry = int.from_bytes(header[24:32], "little")
    readelf = run_cmd(["readelf", "-W", "-l", "-d", str(app)], app.parent)
    text = readelf["stdout"]
    needed = []
    for line in text.splitlines():
        if "(NEEDED)" in line:
            needed.append(line.split("[", 1)[-1].rstrip("]").strip())
    ldd = run_cmd(["ldd", str(app)], app.parent)
    missing = [line.strip() for line in ldd["stdout"].splitlines() if "not found" in line]
    return {
        "magic": "ELF",
        "elf_class": {1: 32, 2: 64}.get(header[4], header[4]),
        "elf_type": elf_type,
        "machine": machine,
        "entry": entry,
        "needed": needed,
        "relro": "GNU_RELRO" in text,
        "executable_stack": any(
            "GNU_STACK" in line and any(token in {"RWE", "RE", "WE", "E"} for token in line.split())
            for line in text.splitlines()
        ),
        "ldd_exit_code": ldd["exit_code"],
        "ldd": ldd["stdout"],
        "missing_libraries": missing,
        "bytes": app.stat().st_size,
        "sha256": sha256_file(app),
    }


def cmd_verify(args: argparse.Namespace) -> int:
    root = repo_root()
    path = selected_run(args)
    exe = path / "builds" / "windows-x64" / "App.exe"
    app = path / "builds" / "linux-x64" / "App"
    worst = 0
    if exe.is_file():
        verify = run_cmd(
            [
                "cargo", "run", "-p", "sash", "--offline", "--",
                "verify", str(exe),
                "--target", WINDOWS_TRIPLE,
                "--format", "json",
                "--evidence", str(path / "builds" / "windows-x64" / "evidence"),
            ],
            root,
            timeout=args.timeout,
        )
        save_stream(path / "windows-x64" / "verify.json", verify)
        (path / "windows-x64" / "artifact.sha256").write_text(
            f"{sha256_file(exe)}  App.exe\n", encoding="utf-8"
        )
        if verify["exit_code"] != 3:
            worst = verify["exit_code"] or 1
    else:
        write_json(path / "windows-x64" / "verify.json", {"exit_code": None, "json": None, "detail": "App.exe missing"})
        worst = 1
    if app.is_file():
        facts = parse_elf(app)
        write_json(path / "linux-x64" / "elf.json", facts)
        (path / "linux-x64" / "artifact.sha256").write_text(
            f"{facts.get('sha256', '')}  App\n", encoding="utf-8"
        )
        if facts.get("magic") != "ELF" or facts.get("missing_libraries"):
            worst = worst or 1
    else:
        write_json(path / "linux-x64" / "elf.json", {"magic": "MISSING"})
        worst = worst or 1
    return worst


def host_machine(root: Path) -> dict:
    preflight = {}
    return {
        "recorded_at": now_iso(),
        "uname": run_cmd(["uname", "-a"], root)["stdout"].strip(),
        "session_type": os.environ.get("XDG_SESSION_TYPE", ""),
        "desktop": os.environ.get("XDG_CURRENT_DESKTOP", ""),
        "wayland_display": os.environ.get("WAYLAND_DISPLAY", ""),
        "kind": "manually supervised interactive test machine" if os.environ.get("WAYLAND_DISPLAY") else "host without a recorded desktop session",
        "preflight_present": preflight is not None,
    }


def cmd_run_windows(args: argparse.Namespace) -> int:
    path = selected_run(args)
    system = os.uname().sysname
    record = {
        "status": "BLOCKED",
        "fault": "DA-E005",
        "detail": "This process is not running in an interactive Windows x64 session. Wine, QEMU, and a Linux cross-link are not a Windows launch.",
        "operating_system": system,
        "wine_accepted": False,
        "independent_execution": False,
        "window": "ABSENT",
        "recorded_at": now_iso(),
    }
    if system == "Windows":
        record = {
            "status": "BLOCKED",
            "fault": "DA-E005",
            "detail": "A Windows host was detected, but this runner has no recorded interactive launch yet.",
            "operating_system": system,
            "independent_execution": False,
            "window": "UNVERIFIED",
            "recorded_at": now_iso(),
        }
    write_json(path / "windows-x64" / "machine.json", host_machine(repo_root()))
    write_json(path / "windows-x64" / "launch.json", record)
    interactions = path / "windows-x64" / "interactions.json"
    if not interactions.is_file():
        write_json(path / "windows-x64" / "interactions.json", {"tests": blank_tests("windows-x64", "Windows interactive session is not available")})
    print(record["status"])
    return 3


def cmd_run_linux(args: argparse.Namespace) -> int:
    root = repo_root()
    path = selected_run(args)
    app = path / "builds" / "linux-x64" / "App"
    machine = host_machine(root)
    machine["kind"] = "manually supervised interactive test machine" if os.environ.get("WAYLAND_DISPLAY") else "hosted environment without Wayland"
    write_json(path / "linux-x64" / "machine.json", machine)
    interactions = path / "linux-x64" / "interactions.json"
    if not interactions.is_file():
        write_json(
            path / "linux-x64" / "interactions.json",
            {"tests": blank_tests("linux-x64", "real Wayland seat input has not been recorded")},
        )
    if not os.environ.get("WAYLAND_DISPLAY"):
        write_json(
            path / "linux-x64" / "launch.json",
            {
                "status": "BLOCKED",
                "fault": "DA-E005",
                "detail": "WAYLAND_DISPLAY is unset. A headless process is not a native window launch.",
                "window": "ABSENT",
                "independent_execution": False,
                "recorded_at": now_iso(),
            },
        )
        print("BLOCKED")
        return 3
    if not app.is_file():
        write_json(
            path / "linux-x64" / "launch.json",
            {
                "status": "FAIL",
                "fault": "DA-E006",
                "detail": "linux artifact App is missing",
                "window": "ABSENT",
                "independent_execution": False,
                "recorded_at": now_iso(),
            },
        )
        print("FAIL")
        return 1
    execution = path / "linux-x64" / "execution"
    if any(execution.iterdir()):
        raise SystemExit(f"refusing to reuse non-empty execution directory {execution}")
    destination = execution / "App"
    shutil.copy2(app, destination)
    destination.chmod(destination.stat().st_mode | 0o111)
    if sha256_file(destination) != sha256_file(app):
        raise SystemExit("execution copy digest does not match the built App")
    if any(execution.glob("*.gscene")):
        raise SystemExit("execution directory must not contain a scene")
    stdout_path = path / "linux-x64" / "logs" / "live.stdout"
    stderr_path = path / "linux-x64" / "logs" / "live.stderr"
    stdout_path.parent.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.pop("EMPEROR_EMBED", None)
    with stdout_path.open("w", encoding="utf-8") as stdout, stderr_path.open("w", encoding="utf-8") as stderr:
        process = subprocess.Popen(
            [str(destination), "--live"],
            cwd=execution,
            env=env,
            stdout=stdout,
            stderr=stderr,
            start_new_session=True,
        )
    time.sleep(args.settle)
    alive = process.poll() is None
    launch = {
        "status": "PROCESS_ALIVE" if alive else "FAIL",
        "fault": None if alive else "DA-E006",
        "detail": "Started ./App --live from a directory that contains neither the compiler nor the scene.",
        "pid": process.pid,
        "argv": ["./App", "--live"],
        "cwd": str(execution),
        "artifact_sha256": sha256_file(destination),
        "compiler_invoked": False,
        "scene_in_cwd": False,
        "independent_execution": True,
        "window": "UNVERIFIED",
        "window_evidence": "",
        "wayland_display": os.environ.get("WAYLAND_DISPLAY", ""),
        "recorded_at": now_iso(),
        "stdout_log": str(stdout_path),
        "stderr_log": str(stderr_path),
    }
    if not alive:
        launch["detail"] = stderr_path.read_text(encoding="utf-8")[-2000:]
        launch["exit_code"] = process.returncode
    write_json(path / "linux-x64" / "launch.json", launch)
    print(launch["status"])
    return 0 if alive else 1


def matrix_from(path: Path) -> dict:
    platforms = {}
    for name in ("windows-x64", "linux-x64"):
        document = read_json(path / name / "interactions.json")
        rows = normalise_tests((document or {}).get("tests") or blank_tests(name, "not recorded"))
        platforms[name] = rows
    return {"directive": DIRECTIVE, "tests": TESTS and [{"id": item[0], "expected": item[2]} for item in TESTS], "platforms": platforms}


def cmd_collect(args: argparse.Namespace) -> int:
    path = selected_run(args)
    preflight = read_json(path / "preflight.json") or {}
    sash = read_json(path / "builds" / "sash.json") or {}
    emperor = read_json(path / "builds" / "emperor.json") or {}
    manifest = {
        "directive": DIRECTIVE,
        "directive_version": DIRECTIVE_VERSION,
        "run_id": path.name,
        "inspected_baseline": INSPECTED_BASELINE,
        "source_revision": preflight.get("source_revision", ""),
        "worktree_dirty": preflight.get("worktree_dirty"),
        "source_sha256": preflight.get("source_sha256", ""),
        "build_host": preflight.get("hostname", ""),
        "operating_system": preflight.get("uname", ""),
        "rust_toolchain": preflight.get("rustc", ""),
        "windows": {
            "compiler_name": "sash",
            "target_triple": WINDOWS_TRIPLE,
            "artifact_sha256": sash.get("sha256", ""),
            "artifact_size": sash.get("bytes", 0),
            "build_status": sash.get("status", "MISSING"),
        },
        "linux": {
            "compiler_name": "emperor",
            "target_triple": LINUX_TRIPLE,
            "artifact_sha256": emperor.get("sha256", ""),
            "artifact_size": emperor.get("bytes", 0),
            "build_status": emperor.get("status", "MISSING"),
            "runtime_dependencies_note": "declared dynamic libraries only; see linux-x64/elf.json",
        },
        "qualification_status": "NOT_ADMITTED",
        "note": "collection is not admission",
    }
    write_json(path / "manifest.json", manifest)
    write_json(path / "conformance" / "matrix.json", matrix_from(path))
    print(path)
    return 0


def failure_rows(decision: dict, path: Path) -> list[dict]:
    preflight = read_json(path / "preflight.json") or {}
    revision = preflight.get("source_revision", "")
    system = preflight.get("uname", "")
    rows = []
    for item in decision["gates"]:
        if item["status"] == "PASS":
            continue
        rows.append(
            {
                "gate": item["id"],
                "code": item.get("fault") or "DA-E013",
                "reproduction": f"scripts/demo-a/admit --run-id {path.name}",
                "observed": item["detail"],
                "expected": "mandatory gate PASS with evidence from this revision",
                "source_revision": revision,
                "operating_system": system,
                "target": {
                    "DA-G03": WINDOWS_TRIPLE,
                    "DA-G05": WINDOWS_TRIPLE,
                    "DA-G07": WINDOWS_TRIPLE,
                    "DA-G04": LINUX_TRIPLE,
                    "DA-G06": LINUX_TRIPLE,
                    "DA-G08": LINUX_TRIPLE,
                }.get(item["id"], "both"),
                "log": str(path),
                "hypothesis": item["detail"],
                "owner": "SW-7",
                "correction": "Collect the missing native evidence. Do not rewrite the gate as passed.",
                "regression_test": "scripts/demo-a/qualify.py self-test",
                "verification": item["status"],
            }
        )
    return rows


def render_summary(decision: dict, path: Path) -> str:
    lines = [
        f"# GY-DEMO-A-001 admission {path.name}",
        "",
        f"Qualification status: {decision['qualification_status']}.",
        f"Programme summary: {decision['programme_summary']}.",
        "",
        "The completion statement is not issued."
        if not decision["completion_statement_issued"]
        else "The completion statement is issued because every mandatory gate passed.",
        "",
        "| Gate | Status | Detail |",
        "|---|---|---|",
    ]
    for item in decision["gates"]:
        detail = item["detail"].replace("\n", " ")
        lines.append(f"| {item['id']} | {item['status']} | {detail} |")
    lines.extend(
        [
            "",
            "## Unqualified capabilities",
            "",
            "Full visual parity, accessibility certification, IME, multiwindow, persistence, theme switching, installers, signing, production release, x86 and ARM64 execution, X11, Android, iOS, and browser targets are outside this demonstration.",
            "",
            "Emperor still does not qualify full IME, sandboxing, advanced accessibility, or dynamic resize reflow. Those gaps are recorded and are not claimed as passed.",
            "",
            "A scripted interaction suite, a Wayland smoke frame, and a static PE verifier are not native input passes.",
            "",
        ]
    )
    return "\n".join(lines)


def cmd_admit(args: argparse.Namespace) -> int:
    path = selected_run(args)
    cmd_collect(args)
    decision = assemble_decision(path)
    decision["recorded_at"] = now_iso()
    decision["run_id"] = path.name
    decision["source_revision"] = (read_json(path / "preflight.json") or {}).get("source_revision", "")
    write_json(path / "admission" / "decision.json", decision)
    write_json(path / "conformance" / "failures.json", {"failures": failure_rows(decision, path)})
    (path / "admission" / "summary.md").write_text(render_summary(decision, path), encoding="utf-8")
    manifest = read_json(path / "manifest.json") or {}
    manifest["qualification_status"] = decision["qualification_status"]
    manifest["programme_summary"] = decision["programme_summary"]
    manifest["completion_statement_issued"] = decision["completion_statement_issued"]
    write_json(path / "manifest.json", manifest)
    print(decision["qualification_status"])
    return 0 if decision["qualification_status"] == "PASS" else 2


def cmd_baseline(args: argparse.Namespace) -> int:
    root = repo_root()
    path = selected_run(args)
    commands = []
    for argv in BASELINE_COMMANDS:
        record = run_cmd(argv, root, timeout=args.timeout)
        name = "baseline-" + argv[1]
        save_stream(path / "conformance" / f"{name}.json", record)
        commands.append({"argv": argv, "exit_code": record["exit_code"], "seconds": record["seconds"]})
        print(f"{record['exit_code']} {' '.join(argv)}", flush=True)
    write_json(
        path / "conformance" / "baseline.json",
        {"recorded_at": now_iso(), "commands": commands},
    )
    return 0 if all(item["exit_code"] == 0 for item in commands) else 1


def cmd_ci_guard(args: argparse.Namespace) -> int:
    path = selected_run(args)
    decision = read_json(path / "admission" / "decision.json")
    if not decision:
        print("BLOCKED missing decision")
        return 0
    status = decision.get("qualification_status")
    if status == "PASS":
        print("refusing PASS from this guard: a headless record must not admit Demo A")
        return 1
    print(f"Demo A admission is {status}. This job is not a qualification pass.")
    return 0


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"self-test failed: {message}")


def cmd_self_test(_args: argparse.Namespace) -> int:
    blocked = [gate(f"DA-G{index:02d}", "BLOCKED", "absent", "DA-E013") for index in range(1, 11)]
    result = decide(blocked)
    expect(result["qualification_status"] == "BLOCKED", "all blocked")
    expect(result["programme_summary"] == "BLOCKED", "no partial without evidence")
    expect(result["completion_statement_issued"] is False, "statement withheld")
    mixed = blocked.copy()
    mixed[2] = gate("DA-G03", "PASS", "structure")
    mixed_result = decide(mixed)
    expect(mixed_result["qualification_status"] == "BLOCKED", "one pass stays blocked")
    expect(mixed_result["programme_summary"] == "PARTIAL", "partial summary")
    failed = blocked.copy()
    failed[0] = gate("DA-G01", "FAIL", "baseline", "DA-E014")
    expect(decide(failed)["qualification_status"] == "FAIL", "fail wins")
    passed = [gate(f"DA-G{index:02d}", "PASS", "ok") for index in range(1, 11)]
    expect(decide(passed)["qualification_status"] == "PASS", "all pass")
    expect(decide(passed)["completion_statement_issued"] is True, "statement only on pass")
    verify_pass = windows_structure_gate(
        {"exit_code": 3, "json": {"pe_structure": "PASS", "runtime_qualification": "BLOCKED", "target": WINDOWS_TRIPLE}}
    )
    expect(verify_pass["status"] == "PASS", "exit 3 structure")
    false_green = windows_structure_gate(
        {"exit_code": 0, "json": {"pe_structure": "PASS", "runtime_qualification": "PASS", "target": WINDOWS_TRIPLE}}
    )
    expect(false_green["status"] == "FAIL", "exit 0 is false green")
    runtime_claim = windows_structure_gate(
        {"exit_code": 3, "json": {"pe_structure": "PASS", "runtime_qualification": "PASS", "target": WINDOWS_TRIPLE}}
    )
    expect(runtime_claim["status"] == "FAIL", "runtime PASS claim")
    rows = blank_tests("linux-x64", "missing")
    rows[0]["status"] = "PASS"
    normalised = normalise_tests(rows)
    expect(normalised[0]["status"] == "FAIL", "empty PASS rejected")
    expect(interaction_gate("DA-G08", rows)["status"] == "FAIL", "matrix fail closed")
    complete = []
    for test_id, procedure, expected in TESTS:
        complete.append(
            {
                "id": test_id,
                "procedure": procedure,
                "expected": expected,
                "observed": expected,
                "input_method": "wayland-seat",
                "evidence": "recorded",
                "status": "PASS",
            }
        )
    expect(interaction_gate("DA-G08", complete)["status"] == "PASS", "complete matrix")
    hidden = launch_gate("DA-G06", {"status": "PROCESS_ALIVE", "window": "UNVERIFIED", "window_evidence": ""}, "absent")
    expect(hidden["status"] == "BLOCKED", "alive process is not a window")
    visible = launch_gate(
        "DA-G06",
        {"status": "PROCESS_ALIVE", "window": "VISIBLE", "window_evidence": "compositor listed the toplevel"},
        "absent",
    )
    expect(visible["status"] == "PASS", "visible evidence")
    expect(review_gate(None, "BLOCKED")["status"] == "BLOCKED", "missing review")
    expect(review_gate({"independent": True, "decision": "PASS"}, "BLOCKED")["status"] == "FAIL", "review cannot upgrade")
    expect(review_gate({"independent": True, "decision": "BLOCKED"}, "BLOCKED")["status"] == "PASS", "matching review")
    elf_ok = elf_gate(
        {"magic": "ELF", "elf_class": 64, "machine": 62, "elf_type": 3, "entry": 0, "executable_stack": False, "relro": True, "missing_libraries": []}
    )
    expect(elf_ok["status"] == "PASS", "elf")
    elf_bad = elf_gate(
        {"magic": "ELF", "elf_class": 64, "machine": 62, "elf_type": 3, "executable_stack": False, "missing_libraries": ["libmissing.so.1 => not found"]}
    )
    expect(elf_bad["status"] == "FAIL", "missing library")
    print("self-test passed")
    return 0


def build_parser() -> argparse.ArgumentParser:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--run-id", default="")
    common.add_argument("--timeout", type=int, default=7200)
    parser = argparse.ArgumentParser(description="GY-DEMO-A-001 fail-closed qualification")
    sub = parser.add_subparsers(dest="command", required=True)
    for name in (
        "preflight",
        "build-windows",
        "build-linux",
        "verify-artifacts",
        "run-windows",
        "collect-evidence",
        "admit",
        "baseline",
        "ci-guard",
        "self-test",
    ):
        sub.add_parser(name, parents=[common])
    run_linux = sub.add_parser("run-linux", parents=[common])
    run_linux.add_argument("--settle", type=float, default=1.5)
    return parser


def main(argv: list[str]) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    commands = {
        "preflight": cmd_preflight,
        "build-windows": cmd_build_windows,
        "build-linux": cmd_build_linux,
        "verify-artifacts": cmd_verify,
        "run-windows": cmd_run_windows,
        "run-linux": cmd_run_linux,
        "collect-evidence": cmd_collect,
        "admit": cmd_admit,
        "baseline": cmd_baseline,
        "ci-guard": cmd_ci_guard,
        "self-test": cmd_self_test,
    }
    return commands[args.command](args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
