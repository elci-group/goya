#!/usr/bin/env python3
"""Verify Cortados architectural closure and its bounded resolver witness.

No Java input is executed. --record refreshes only the feasibility receipt.
This validator implements the schema keywords used by csir.schema.json, not a
general JSON Schema implementation or a Java semantic-equivalence verifier.
"""
import argparse
import copy
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
BASE = ROOT / "docs/cortados"
REVISION = "b408ee19a15037f440db49f1cabd5ad93f660452"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def read(name):
    return json.loads((BASE / name).read_text())


def safe_path(root, name):
    require(isinstance(name, str) and name and "\\" not in name and "\0" not in name,
            "invalid artifact path")
    relative = pathlib.PurePosixPath(name)
    require(not relative.is_absolute() and all(x not in (".", "..") for x in name.split("/")),
            "artifact path escapes its root")
    path = root
    for part in relative.parts:
        path = path / part
        require(not path.is_symlink(), "symlink in artifact path")
    require(path.is_file(), "artifact missing: " + name)
    require(path.resolve().is_relative_to(root.resolve()), "artifact outside root")
    return path


def schema_check(value, schema, path="$", top=True):
    supported = {"$schema", "title", "type", "properties", "required", "additionalProperties",
                 "items", "const", "enum", "minimum", "minLength", "pattern"}
    require(set(schema) <= supported, "validator does not implement schema keyword")
    types = schema.get("type", [])
    types = [types] if isinstance(types, str) else types
    predicates = {"object": lambda: isinstance(value, dict),
                  "array": lambda: isinstance(value, list),
                  "string": lambda: isinstance(value, str),
                  "integer": lambda: type(value) is int,
                  "boolean": lambda: type(value) is bool,
                  "null": lambda: value is None}
    require(not types or any(predicates[t]() for t in types), path + ": wrong type")
    if "const" in schema:
        require(value == schema["const"], path + ": contract mismatch")
    if "enum" in schema:
        require(value in schema["enum"], path + ": invalid enum")
    if isinstance(value, str):
        require(len(value) >= schema.get("minLength", 0), path + ": empty string")
        if "pattern" in schema:
            require(re.fullmatch(schema["pattern"], value) is not None, path + ": invalid format")
    if type(value) is int and "minimum" in schema:
        require(value >= schema["minimum"], path + ": below minimum")
    if isinstance(value, dict) and "properties" in schema:
        require(set(schema.get("required", [])) <= set(value), path + ": missing field")
        if schema.get("additionalProperties") is False:
            require(set(value) <= set(schema["properties"]), path + ": unknown field")
        for key, child in value.items():
            if key in schema["properties"]:
                schema_check(child, schema["properties"][key], path + "." + key, False)
    if isinstance(value, list) and "items" in schema:
        for i, child in enumerate(value):
            schema_check(child, schema["items"], f"{path}[{i}]", False)


def validate_csir(document):
    schema_check(document, read("csir.schema.json"))
    require(document["frontend"]["construction"] == "manual_fixture",
            "Phase 0 witness must not masquerade as static analysis")
    require(document["fair"] is None, "attached FAIR validation is a Phase 1 implementation gate")
    require(document["sources"], "empty source inventory")
    require(document["sources"] == sorted(document["sources"], key=lambda x: x["path"]),
            "source inventory not canonical")
    require(document["source_manifest_sha256"] == digest(canonical(document["sources"])),
            "source manifest digest mismatch")
    material = {}
    for source in document["sources"]:
        require(source["path"] not in material, "duplicate source path")
        raw = safe_path(BASE / "fixtures", source["path"]).read_bytes()
        require(digest(raw) == source["sha256"], "source digest mismatch")
        raw.decode("utf-8")
        material[source["path"]] = raw
    all_items = document["nodes"] + document["edges"] + document["transformations"]
    ids = [item["id"] for item in all_items]
    require(len(ids) == len(set(ids)), "duplicate graph ID")
    node_ids = {node["id"] for node in document["nodes"]}
    require(node_ids, "empty semantic graph")
    for group in ("nodes", "edges", "transformations"):
        require(document[group] == sorted(document[group], key=lambda x: x["id"]),
                "graph order not canonical")
    for item in all_items:
        evidence = item["evidence"]
        require(evidence == sorted(set(evidence)), "noncanonical evidence")
        require(set(evidence) <= node_ids and item["id"] not in evidence,
                "dangling or self evidence")
    for node in document["nodes"]:
        origin = node["origin"]
        require(origin["path"] in material, "unmanifested source origin")
        raw = material[origin["path"]]
        start, end = origin["start_byte"], origin["end_byte"]
        require(0 <= start < end <= len(raw), "invalid source span")
        raw[:start].decode("utf-8")
        raw[start:end].decode("utf-8")
        require(digest(raw[start:end]) == origin["raw_sha256"], "source span digest mismatch")
        require(node["fair_element_id"] is None, "detached fixture has FAIR reference")
        if node["level"] == "application":
            require(node["evidence"], "application intent lacks implementation evidence")
        if node["kind"] == "unresolved":
            require(node["unresolved"], "opaque semantics silently discarded")
    for edge in document["edges"]:
        require(edge["subject"] in node_ids and edge["object"] in node_ids,
                "dangling graph endpoint")
        require(edge["evidence"], "edge lacks evidence")
    dependencies = {node["id"]: node["evidence"] for node in document["nodes"]}
    def visit(key, stack):
        require(key not in stack, "cyclic proof dependency")
        for target in dependencies[key]:
            visit(target, stack | {key})
    for key in dependencies:
        visit(key, set())
    for transform in document["transformations"]:
        require(not transform["verified"], "manual pairing cannot certify equivalence")
        for field in ("preconditions", "postconditions", "equivalence_requirements", "limitations"):
            require(transform[field], "transformation contract omits " + field)


def negative_cases(document):
    cases = []
    def check(name, change):
        broken = copy.deepcopy(document)
        change(broken)
        try:
            validate_csir(broken)
        except (ValueError, UnicodeError):
            cases.append({"id": name, "status": "pass"})
        else:
            raise ValueError("tampered fixture accepted: " + name)
    check("unknown_contract", lambda x: x.update(contract="GY-CSIR-JAVA/999"))
    check("wrong_profile", lambda x: x.update(profile_id="java-latest"))
    check("missing_required_field", lambda x: x.pop("sources"))
    check("unknown_field", lambda x: x.update(confidence=1.0))
    check("manifest_digest", lambda x: x.update(source_manifest_sha256="0" * 64))
    check("source_digest", lambda x: x["sources"][0].update(sha256="0" * 64))
    def escaping(x):
        x["sources"][0]["path"] = "../Counter.java"
        x["source_manifest_sha256"] = digest(canonical(x["sources"]))
    check("path_traversal", escaping)
    check("out_of_bounds_span", lambda x: x["nodes"][0]["origin"].update(end_byte=10**9))
    check("boolean_byte_offset", lambda x: x["nodes"][0]["origin"].update(start_byte=True))
    check("span_digest", lambda x: x["nodes"][0]["origin"].update(raw_sha256="0" * 64))
    check("unmanifested_origin", lambda x: x["nodes"][0]["origin"].update(path="Other.java"))
    check("duplicate_node", lambda x: x["nodes"].append(copy.deepcopy(x["nodes"][0])))
    check("dangling_edge", lambda x: x["edges"][0].update(object="absent"))
    check("dangling_evidence", lambda x: x["nodes"][0].update(evidence=["absent"]))
    check("unsupported_fair_reference", lambda x: x["nodes"][0].update(fair_element_id="fake"))
    check("unproved_intent", lambda x: x["nodes"][0].update(evidence=[]))
    check("dropped_unknown", lambda x: x["nodes"][1].update(unresolved=[]))
    check("cyclic_evidence", lambda x: x["nodes"][1].update(evidence=["app.counter"]))
    check("false_equivalence", lambda x: x["transformations"][0].update(verified=True))
    check("masquerading_parser", lambda x: x["frontend"].update(construction="static_analysis"))
    return cases


def validate_architecture():
    profile = read("profile.json")
    require(profile["id"] == "CORTADOS-JAVA-SOURCE/17.0", "profile version drift")
    require(profile["implementation_available"] is False, "false parser claim")
    require(profile["source"]["release"] == 17 and not profile["source"]["preview"], "Java scope drift")
    require(profile["security"]["offline"] and not profile["security"]["execute_input"], "unsafe ingestion policy")
    require(profile["native"]["fallback"] == "none", "implicit fallback")
    rows = profile["native"]["requirements"]
    by_id = {row["id"]: row for row in rows}
    require({r["id"] for r in rows} == {f"COR-S{i:03}" for i in range(1, 35)}, "incomplete semantic mapping")
    require(len(rows) == 34, "duplicate mapping")
    require(all(r["disposition"] in {"conditional_mapping", "reject_native"} and r["rule"] and r["diagnostic"] for r in rows), "unresolved architecture requirement")
    require(all(by_id[f"COR-S{i:03}"]["disposition"] == "reject_native"
                for i in (13, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 34)),
            "unsupported behavior admitted")
    audit = read("phase0-audit.json")
    require(audit["phase0_complete"] is True and audit["implementation_status"] == "not_started", "phase boundary mismatch")
    require(audit["certification"] is None, "architecture audit cannot certify an application")
    require(len(audit["semantic_domains"]) == 11, "missing CSIR domain")
    require(len(audit["release_gates"]) == 14 and all(g["status"] == "not_assessed" for g in audit["release_gates"]), "false release claim")
    for path, entry in audit["sources"].items():
        if not entry["available"]:
            require(entry["origin"] == "historical_observation_unavailable" and entry["snapshot"] is None,
                    "unavailable source misclassified")
            require(by_id["COR-S034"]["disposition"] == "reject_native",
                    "missing graphics evidence cannot admit native mapping")
            continue
        require(digest(safe_path(ROOT, entry["snapshot"]).read_bytes()) == entry["sha256"],
                "stale archived audit source: " + path)
    closure = read("phase0-closure.json")
    require(closure["status"] == "closed_architecture_only", "incorrect phase verdict")
    require({d["id"] for d in closure["decisions"]} == {f"COR-D{i:02}" for i in range(1, 8)}, "unresolved decision set")
    require(all(d["status"] == "closed" for d in closure["decisions"]), "open architecture decision")
    require(closure["certification"] is None, "false application certification")
    for path, sha in closure["artifacts"].items():
        require(digest(safe_path(BASE, path).read_bytes()) == sha, "stale closure artifact: " + path)
    for name in ("README.md", "architecture.md"):
        for link in re.findall(r"\[[^\]]+\]\(([^)]+)\)", (BASE / name).read_text()):
            if not link.startswith(("https:", "http:")):
                linked = BASE / link.split("#")[0]
                relative = str(linked.resolve().relative_to(ROOT))
                require(linked.exists() or relative in audit["sources"], "broken document link: " + link)


def execute_witness(workspace):
    cases = read("fixtures/scenarios.json")
    observed = subprocess.run(["git", "rev-parse", "HEAD"], cwd=workspace, capture_output=True, text=True, check=True).stdout.strip()
    require(observed == REVISION, "witness workspace is not the frozen baseline")
    for command in (["git", "diff", "--quiet"], ["git", "diff", "--cached", "--quiet"]):
        require(subprocess.run(command, cwd=workspace).returncode == 0, "witness baseline has tracked edits")
    require((workspace / "crates/sash/fixtures/interactive-live.gscene").read_bytes() ==
            (BASE / "fixtures/counter.gscene").read_bytes(), "witness scene differs from baseline fixture")
    base = ["cargo", "run", "--offline", "--locked", "--quiet", "--manifest-path", str(workspace / "Cargo.toml"),
            "-p", "goya-resolver", "--bin", "goya-resolve", "--",
            str(BASE / "fixtures/counter.gscene"), "--contract", "GSSH/4-live", "--viewport", "320x160"]
    receipts = []
    for i, expected in enumerate(cases["prefix_results"], 1):
        command = base.copy()
        for value in cases["inputs"][:i]:
            command += ["--set", "window.amount=" + value, "--dispatch", "action.add"]
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=120)
        require(result.returncode == 0, "resolver witness failed: " + result.stderr[-2000:])
        for text in ("contract: GSSH/4-live", "phase: running", f"revision: {expected['revision']}",
                     f"state count int {expected['count']} session", f"display window.total {expected['display']}"):
            require(text in result.stdout.splitlines(), "missing observation: " + text)
        receipts.append(dict(id=f"counter_prefix_{i}", status="pass", command=command,
                             exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr))
    for case in cases["goya_negative_cases"]:
        command = base.copy()
        for value in case["inputs"]:
            command += ["--set", "window.amount=" + value, "--dispatch", "action.add"]
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=120)
        require(result.returncode != 0 and case["diagnostic"] in result.stderr,
                "resolver accepted negative case: " + case["id"])
        receipts.append(dict(id=case["id"], status="pass", command=command,
                             exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr))
    return receipts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record", action="store_true", help="refresh feasibility receipt after live checks")
    parser.add_argument("--workspace", type=pathlib.Path, default=pathlib.Path("/tmp/goya-cortados-phase0"),
                        help="clean detached baseline worktree, recreated with git worktree add --detach PATH REVISION")
    args = parser.parse_args()
    validate_architecture()
    document = read("fixtures/counter.csir.json")
    validate_csir(document)
    require(canonical(document) == (BASE / "fixtures/counter.csir.json").read_bytes(), "noncanonical CSIR")
    require(json.loads(canonical(document)) == document, "CSIR round-trip changed content")
    negatives = negative_cases(document)
    receipts = execute_witness(args.workspace.resolve())
    source_paths = ("profile.json", "csir.schema.json", "verify_phase0.py", "fixtures/Counter.java",
                    "fixtures/counter.gscene", "fixtures/counter.csir.json", "fixtures/scenarios.json")
    report = dict(contract="GY-CORTADOS-PHASE0-EVIDENCE/0.1", verdict="pass",
                  scope="architectural closure and hand-paired headless resolver feasibility only",
                  execution_revision=REVISION,
                  artifacts={p: digest((BASE / p).read_bytes()) for p in source_paths},
                  structural_checks="schema subset, canonical round-trip, hashes, spans, graph references, opaque preservation",
                  negative_cases=negatives, resolver_cases=receipts,
                  java_reference=dict(status="not_run", java_available=bool(shutil.which("java")),
                                      javac_available=bool(shutil.which("javac")),
                                      reason="No reference execution requested by this harness; JDK absent in recorded environment."),
                  differential_equivalence="not_established", certification=None)
    if args.record:
        (BASE / "feasibility-results.json").write_bytes(canonical(report))
    else:
        saved = read("feasibility-results.json")
        require(saved["verdict"] == "pass" and saved["certification"] is None, "invalid saved verdict")
        require(saved["artifacts"] == report["artifacts"], "stale feasibility receipt")
        require(saved["negative_cases"] == negatives, "negative-case evidence mismatch")
        require(len(saved["resolver_cases"]) == len(receipts), "missing execution receipt")
        for saved_case, live in zip(saved["resolver_cases"], receipts):
            require(all(saved_case[key] == live[key] for key in ("id", "status", "command", "exit_code", "stdout")),
                    "execution receipt differs from live observations")
    print(f"PASS: 34 semantic dispositions, 7 decisions, 20 tamper rejections, {len(receipts)} live resolver scenarios; no Java certification")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.TimeoutExpired) as error:
        print("FAIL:", error, file=sys.stderr)
        sys.exit(1)
