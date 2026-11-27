use std::env;
use std::process::Command;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || !matches!(args[1].as_str(), "analyse" | "graph" | "gaps") {
        eprintln!("usage: manuscrito <analyse|graph|gaps> <project>");
        return ExitCode::from(2);
    }
    let result = if args[1] == "analyse" {
        goya_manuscrito::analyse(&args[2]).and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map(|json| (json, true))
                .map_err(|error| error.to_string())
        })
    } else if args[1] == "graph" {
        run_graph_frontend(&args[2])
    } else {
        run_gap_report(&args[2])
    };
    match result {
        Ok((json, success)) => {
            println!("{json}");
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("analysis failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_graph_frontend(project: &str) -> Result<(String, bool), String> {
    let output = Command::new("node")
        .arg("-e")
        .arg(include_str!("../frontend/graph.cjs"))
        .arg(project)
        .output()
        .map_err(|error| format!("could not launch Node.js TypeScript frontend: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "TypeScript frontend exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let json = String::from_utf8(output.stdout)
        .map_err(|error| format!("frontend emitted invalid UTF-8: {error}"))?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|error| format!("frontend emitted invalid JSON: {error}"))?;
    let success = value.get("status").and_then(serde_json::Value::as_str) == Some("ok");
    if success {
        let mir: goya_manuscrito::MirReport = serde_json::from_value(
            value
                .get("mir")
                .cloned()
                .ok_or_else(|| "frontend report omitted MIR".to_owned())?,
        )
        .map_err(|error| format!("frontend emitted invalid MIR/1: {error}"))?;
        mir.validate_version()?;
    }
    serde_json::to_string_pretty(&value)
        .map(|json| (json, success))
        .map_err(|error| error.to_string())
}

fn run_gap_report(project: &str) -> Result<(String, bool), String> {
    let (graph_json, graph_ok) = run_graph_frontend(project)?;
    let graph: serde_json::Value = serde_json::from_str(&graph_json)
        .map_err(|error| format!("could not decode graph report: {error}"))?;
    let nodes = graph
        .get("mir")
        .and_then(|mir| mir.get("nodes"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let count_kind = |kind: &str| {
        nodes
            .iter()
            .filter(|node| node.get("kind").and_then(serde_json::Value::as_str) == Some(kind))
            .count()
    };
    let async_count = nodes
        .iter()
        .filter(|node| node.get("async").and_then(serde_json::Value::as_bool) == Some(true))
        .count()
        + count_kind("suspension");
    let capability_observations = graph
        .get("capability_observations")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let domains = serde_json::json!([
        {
            "domain": "backend_functions",
            "evidence_count": count_kind("function"),
            "support": "unsupported",
            "reason": "GY-LANG/1 and GSSH profiles model scenes and interaction transitions; they have no generic backend function declaration or invocation primitive."
        },
        {
            "domain": "control_flow",
            "evidence_count": count_kind("branch") + count_kind("iteration"),
            "support": "unsupported",
            "reason": "Generic branching and iteration in backend computation are not represented by the canonical scene or interaction AST."
        },
        {
            "domain": "asynchronous_tasks",
            "evidence_count": async_count,
            "support": "unsupported",
            "reason": "The canonical language has no promise, task, scheduling, cancellation, or concurrency model."
        },
        {
            "domain": "throw_statements",
            "evidence_count": count_kind("throw"),
            "support": "unsupported",
            "reason": "No backend exception, typed failure, or propagation construct exists in the current canonical scene grammar."
        },
        {
            "domain": "backend_invocation_sites",
            "evidence_count": capability_observations.len(),
            "support": "unsupported",
            "reason": "GSSH effect requests are static and interaction-scoped; they do not express general filesystem, process, network, stream, or service operations."
        },
        {
            "domain": "interaction_effect_requests",
            "evidence_count": 0,
            "support": "partial_domain_specific",
            "reason": "Canonical D2 requests exist only within button-triggered interactions and do not establish backend service semantics."
        },
        {
            "domain": "state_and_mutation",
            "evidence_count": count_kind("mutation"),
            "support": "partial_domain_specific",
            "reason": "Typed state and fixed assignments are available for interaction transitions; general backend object identity, ownership, and mutation are not represented."
        },
        {
            "domain": "resource_lifecycle",
            "evidence_count": count_kind("binding"),
            "support": "unsupported",
            "reason": "The canonical language has no acquisition, scope, finalizer, or resource-lifecycle semantics."
        }
    ]);
    let report = serde_json::json!({
        "schema": "goya.manuscrito.gscene-gaps/1",
        "status": if graph_ok { "analysis-only" } else { "incomplete" },
        "canonical_language": "GY-LANG/1",
        "basis": "maintained capability comparison against goya-lang scene AST and strict GSSH profiles; not live grammar introspection",
        "profiles_considered": ["GSSH/1", "GSSH/2", "GSSH/3", "GSSH/3-live", "GSSH/4", "GSSH/4-live"],
        "source_report_status": graph.get("status").cloned().unwrap_or(serde_json::Value::Null),
        "source_module_count": graph.get("modules").and_then(serde_json::Value::as_array).map_or(0, Vec::len),
        "translated": false,
        "canonical_output_emitted": false,
        "domains": domains,
        "limitations": [
            "This is a grammar capability comparison, not a semantic equivalence result.",
            "Evidence counts describe extracted MIR syntax and do not measure execution reachability or workload importance.",
            "No undocumented .gscene extension or fallback runtime is introduced."
        ]
    });
    serde_json::to_string_pretty(&report)
        .map(|json| (json, graph_ok))
        .map_err(|error| error.to_string())
}
