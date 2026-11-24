//! Version 1 local, synchronous JSON-lines inspection protocol.
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const MAX_REQUEST_BYTES: u64 = 256 * 1024;
pub const MAX_JOBS: usize = 1024;

const SWIFT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_SWIFT_DIAGNOSTICS: usize = 16 * 1024;

/// Run a bounded syntax-only check using the host's `swiftc` on PATH.
pub fn parse_swift_source(source: &str, id: &str) -> Value {
    swift_parse(
        std::process::Command::new("swiftc").args(["-frontend", "-parse", "-"]),
        source,
        id,
        SWIFT_TIMEOUT,
    )
}

// Pump both pipes concurrently: a compiler may fill stderr before it consumes
// stdin. Completion messages, rather than blocking joins, keep the deadline
// effective even if another process inherits a compiler pipe.
fn swift_parse(
    command: &mut std::process::Command,
    source: &str,
    id: &str,
    timeout: std::time::Duration,
) -> Value {
    use std::io::{Read, Write};
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Instant;

    if source.len() as u64 > MAX_REQUEST_BYTES {
        return error(Some(id), "swift_parse", "source exceeds request limit");
    }
    let mut child = match command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"swift_parse","status":"BLOCKED","reason":format!("swiftc unavailable: {e}"),"apple_sdk_validation":"NOT_RUN"})
        }
    };
    let start = Instant::now();
    let mut stdin = child.stdin.take().expect("piped compiler stdin");
    let mut stderr = child.stderr.take().expect("piped compiler stderr");
    let source = source.as_bytes().to_vec();
    let (write_tx, write_rx) = mpsc::sync_channel(1);
    if let Err(e) = std::thread::Builder::new()
        .name("neptune-swift-stdin".into())
        .spawn(move || {
            let result = stdin.write_all(&source);
            drop(stdin);
            let _ = write_tx.send(result);
        })
    {
        let _ = child.kill();
        let _ = child.wait();
        return error(
            Some(id),
            "swift_parse",
            &format!("could not start compiler input worker: {e}"),
        );
    }
    let (read_tx, read_rx) = mpsc::sync_channel(1);
    if let Err(e) = std::thread::Builder::new()
        .name("neptune-swift-stderr".into())
        .spawn(move || {
            let mut diagnostics = Vec::new();
            let mut truncated = false;
            let mut buffer = [0; 4096];
            let result = loop {
                match stderr.read(&mut buffer) {
                    Ok(0) => break Ok((diagnostics, truncated)),
                    Ok(count) => {
                        let kept = count.min(MAX_SWIFT_DIAGNOSTICS - diagnostics.len());
                        diagnostics.extend_from_slice(&buffer[..kept]);
                        truncated |= kept < count;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => break Err(e),
                }
            };
            let _ = read_tx.send(result);
        })
    {
        let _ = child.kill();
        let _ = child.wait();
        return error(
            Some(id),
            "swift_parse",
            &format!("could not start compiler diagnostic worker: {e}"),
        );
    }
    let mut status = None;
    let mut written = None;
    let mut diagnostics = None;
    loop {
        match child.try_wait() {
            Ok(value) => status = status.or(value),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return error(
                    Some(id),
                    "swift_parse",
                    &format!("could not wait for swiftc: {e}"),
                );
            }
        }
        if written.is_none() {
            if let Ok(result) = write_rx.try_recv() {
                written = Some(result);
            }
        }
        if diagnostics.is_none() {
            if let Ok(result) = read_rx.try_recv() {
                diagnostics = Some(result);
            }
        }
        if status.is_some() && written.is_some() && diagnostics.is_some() {
            break;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return error(Some(id), "swift_parse", "swiftc exceeded its runtime limit");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let (bytes, truncated) = match diagnostics.unwrap() {
        Ok(value) => value,
        Err(e) => {
            return error(
                Some(id),
                "swift_parse",
                &format!("could not read swiftc diagnostics: {e}"),
            )
        }
    };
    if !status.unwrap().success() {
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        let mut end = text.len().min(MAX_SWIFT_DIAGNOSTICS);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let truncated = truncated || end < text.len();
        text.truncate(end);
        return json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"swift_parse","status":"FAIL","diagnostics":text,"diagnostics_truncated":truncated,"apple_sdk_validation":"NOT_RUN"});
    }
    if written.unwrap().is_err() {
        return error(Some(id), "swift_parse", "could not send source to swiftc");
    }
    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"swift_parse","status":"PASS","validation":"swift_syntax_only","apple_sdk_validation":"NOT_RUN"})
}

#[derive(Default)]
pub struct Session {
    jobs: BTreeSet<String>,
}

impl Session {
    /// Handle one request. Job IDs are single-use within a session; clients
    /// must create a new ID for a retry. Each request emits one final event.
    pub fn handle(&mut self, request: &Value) -> Value {
        let Some(object) = request.as_object() else {
            return error(None, "protocol", "request must be an object");
        };
        let Some(id) = object.get("job_id").and_then(Value::as_str).filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        }) else {
            return error(
                None,
                "protocol",
                "job_id must be 1..128 ASCII letters, digits, underscores or hyphens",
            );
        };
        if self.jobs.contains(id) {
            return error(Some(id), "protocol", "job_id already used in this session");
        }
        if self.jobs.len() >= MAX_JOBS {
            return error(Some(id), "protocol", "session job limit reached");
        }
        self.jobs.insert(id.to_owned());
        if object.get("schema_version").and_then(Value::as_str) != Some("1") {
            return error(Some(id), "protocol", "unsupported schema version");
        }
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "schema_version"
                    | "job_id"
                    | "operation"
                    | "source"
                    | "module"
                    | "profile"
                    | "bundle"
                    | "line"
                    | "column"
                    | "column_encoding"
                    | "raw_diagnostic"
                    | "suite"
                    | "initial"
                    | "activations"
                    | "manifest"
                    | "usage_description"
                    | "contracts"
            )
        }) {
            return error(Some(id), "protocol", "unknown request field");
        }
        let Some(operation) = object.get("operation").and_then(Value::as_str) else {
            return error(Some(id), "protocol", "operation is required");
        };
        if !matches!(
            operation,
            "inspect"
                | "check"
                | "preview"
                | "preview-transition"
                | "map-swift"
                | "test"
                | "model-d1-state"
                | "preview-d1-owner"
                | "preview-native-button"
                | "preview-observable-button"
                | "preview-observable-column"
                | "preview-observable-buttons"
                | "trace-observable-buttons"
                | "trace-observable-column"
                | "preview-static-column"
                | "validate-deploy-manifest"
                | "plan-bluetooth-privacy"
                | "swift-parse"
        ) {
            return error(Some(id), "protocol", "operation is not supported");
        }
        if operation == "swift-parse" {
            let Some(source) = object.get("source").and_then(Value::as_str) else {
                return error(Some(id), "swift_parse", "source string is required");
            };
            return parse_swift_source(source, id);
        }
        if operation == "validate-deploy-manifest" {
            let Some(value) = object.get("manifest") else {
                return error(Some(id), "protocol", "manifest object is required");
            };
            let manifest: crate::dock::Manifest = match serde_json::from_value(value.clone()) {
                Ok(value) => value,
                Err(e) => {
                    return error(
                        Some(id),
                        "manifest",
                        &format!("invalid deployment manifest: {e}"),
                    )
                }
            };
            return match manifest.validate() {
                Ok(()) => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"manifest_validation","status":"PASS","result":{"manifest_schema":manifest.schema,"bundle_id":manifest.application.bundle_id,"artifact_sha256":manifest.artifact.sha256,"artifact_hash_verification":"NOT_RUN","signature_trust":"NOT_CHECKED","device_installation":"NOT_RUN"}})
                }
                Err(e) => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"manifest_validation","status":"FAIL","diagnostics":[{"code":"NPT-E6101","message":e.to_string()}]})
                }
            };
        }
        if operation == "plan-bluetooth-privacy" {
            let Some(usage_description) = object.get("usage_description").and_then(Value::as_str)
            else {
                return error(
                    Some(id),
                    "bluetooth_privacy",
                    "usage_description string is required",
                );
            };
            let contracts: Vec<String> = match object.get("contracts") {
                None | Some(Value::Null) => crate::bluetooth::contract_table()
                    .iter()
                    .map(|(contract, _)| (*contract).to_owned())
                    .collect(),
                Some(value) => match value.as_array() {
                    Some(array) => {
                        let mut contracts = Vec::with_capacity(array.len());
                        for contract in array {
                            let Some(contract) = contract.as_str() else {
                                return error(
                                    Some(id),
                                    "bluetooth_privacy",
                                    "contracts must be an array of strings",
                                );
                            };
                            contracts.push(contract.to_owned());
                        }
                        contracts
                    }
                    None => {
                        return error(
                            Some(id),
                            "bluetooth_privacy",
                            "contracts must be an array of strings",
                        )
                    }
                },
            };
            let plan = crate::bluetooth::plan_bluetooth_privacy(usage_description);
            let entries = crate::bluetooth::bluetooth_capability_entries(
                &contracts.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            return match (plan, entries) {
                (Ok(plan), Ok(entries)) => json!({
                    "schema_version": "1",
                    "job_id": id,
                    "sequence": 1,
                    "stage": "bluetooth_privacy",
                    "status": "PASS",
                    "result": {
                        "privacy_plan": plan.to_json(),
                        "capability_entries": entries,
                    }
                }),
                (Err(diagnostic), _) | (_, Err(diagnostic)) => json!({
                    "schema_version": "1",
                    "job_id": id,
                    "sequence": 1,
                    "stage": "bluetooth_privacy",
                    "status": "FAIL",
                    "diagnostics": [{"code": diagnostic.code, "message": diagnostic.message}]
                }),
            };
        }
        if operation == "test" {
            return match object.get("suite").and_then(Value::as_str) {
                Some("d1-corpus") => match crate::conformance::run_d1_corpus() {
                    Ok(report) => {
                        json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"d1_corpus","status":if report.passed {"PASS"} else {"FAIL"},"result":report.json})
                    }
                    Err(e) => error(Some(id), "d1_corpus", &e),
                },
                Some("admission") => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"admission","status":"BLOCKED","reason":"Apple worker and device admission are not implemented"})
                }
                _ => error(Some(id), "protocol", "unsupported test suite"),
            };
        }
        let Some(source) = object.get("source").and_then(Value::as_str) else {
            return error(Some(id), "ingest", "source must be a UTF-8 string");
        };
        if operation == "model-d1-state"
            || operation == "preview-d1-owner"
            || operation == "preview-native-button"
            || operation == "preview-observable-button"
            || operation == "preview-observable-column"
            || operation == "preview-observable-buttons"
            || operation == "trace-observable-buttons"
            || operation == "trace-observable-column"
        {
            if object.get("profile").and_then(Value::as_str) != Some("experimental") {
                return error(
                    Some(id),
                    "protocol",
                    "this operation requires profile experimental",
                );
            }
            let Some(module) = object.get("module").and_then(Value::as_str) else {
                return error(
                    Some(id),
                    "protocol",
                    "this operation requires a module path",
                );
            };
            let Some(initial) = object.get("initial") else {
                return error(
                    Some(id),
                    "protocol",
                    "this operation requires explicit initial values",
                );
            };
            let initial = match crate::application::parse_initial(initial) {
                Ok(value) => value,
                Err(d) => return failure(id, "state_validation", &d),
            };
            let result = if operation == "model-d1-state" {
                crate::application::lower(source, module, initial).map(|model| model.to_json())
            } else if operation == "preview-native-button" {
                crate::native_swift::preview(source, module, initial)
                    .map(|preview| preview.to_json())
            } else if operation == "preview-observable-button" {
                crate::native_swift::preview_observable(source, module, initial)
                    .map(|preview| preview.to_json())
            } else if operation == "preview-observable-column" {
                crate::observable_column::preview(source, module, initial)
                    .map(|preview| preview.to_json())
            } else if operation == "preview-observable-buttons" {
                crate::observable_buttons::preview(source, module, initial)
                    .map(|preview| preview.to_json())
            } else if operation == "trace-observable-buttons"
                || operation == "trace-observable-column"
            {
                let activations = match object.get("activations") {
                    Some(value) => match crate::observable_trace::parse_activations(value) {
                        Ok(actions) => actions,
                        Err(diagnostic) => return failure(id, "state_validation", &diagnostic),
                    },
                    None => {
                        return error(
                            Some(id),
                            "protocol",
                            "this operation requires an activation list",
                        )
                    }
                };
                if operation == "trace-observable-column" {
                    crate::observable_trace::trace_column(source, module, initial, &activations)
                } else {
                    crate::observable_trace::trace(source, module, initial, &activations)
                }
                .map(|trace| trace.to_json())
            } else {
                crate::application_swift::preview(source, module, initial)
                    .map(|preview| preview.to_json())
            };
            return match result {
                Ok(result) => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":if operation == "model-d1-state" {"state_model"} else {"preview"},"status":"PASS","result":result})
                }
                Err(d) => failure(id, "state_validation", &d),
            };
        }
        if operation == "map-swift" {
            if object.get("column_encoding").and_then(Value::as_str) != Some("utf8-byte") {
                return error(
                    Some(id),
                    "protocol",
                    "map-swift requires column_encoding utf8-byte",
                );
            }
            let number = |key: &str| {
                object
                    .get(key)
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .filter(|value| *value > 0)
            };
            let (Some(line), Some(column)) = (number("line"), number("column")) else {
                return error(
                    Some(id),
                    "protocol",
                    "map-swift requires positive line and column",
                );
            };
            let raw = match object.get("raw_diagnostic") {
                None => None,
                Some(value) => match value.as_str().filter(|text| text.len() <= 16384) {
                    Some(text) => Some(text),
                    None => {
                        return error(
                            Some(id),
                            "protocol",
                            "raw_diagnostic must be a string no longer than 16 KiB",
                        )
                    }
                },
            };
            let Some(bundle) = object.get("bundle") else {
                return error(Some(id), "protocol", "map-swift requires a preview bundle");
            };
            let preview = match crate::artifacts::verify_preview_bundle(bundle) {
                Ok(preview) => preview,
                Err(e) => {
                    return error(
                        Some(id),
                        "diagnostic_mapping",
                        &format!("preview integrity check failed: {e}"),
                    )
                }
            };
            return match crate::source_map::map_verified_location(&preview, source, line, column) {
                Ok(result) => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"diagnostic_mapping","status":"PASS","result":result,"raw_diagnostic":raw})
                }
                Err(e) => error(Some(id), "diagnostic_mapping", &e),
            };
        }
        if operation == "preview-transition" {
            if object.get("profile").and_then(Value::as_str) != Some("experimental") {
                return error(
                    Some(id),
                    "protocol",
                    "preview-transition requires profile experimental",
                );
            }
            let Some(module) = object.get("module").and_then(Value::as_str) else {
                return error(
                    Some(id),
                    "protocol",
                    "preview-transition requires a module path",
                );
            };
            return match crate::transition_swift::preview(source, module) {
                Ok(preview) => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"preview","status":"PASS","result":preview.to_json()})
                }
                Err(d) => failure(id, "frontend", &d),
            };
        }
        if operation == "preview-static-column" {
            if object.get("profile").and_then(Value::as_str) != Some("experimental") {
                return error(
                    Some(id),
                    "protocol",
                    "preview-static-column requires profile experimental",
                );
            }
            let Some(module) = object.get("module").and_then(Value::as_str) else {
                return error(
                    Some(id),
                    "protocol",
                    "preview-static-column requires a module path",
                );
            };
            return match crate::static_column::preview(source, module) {
                Ok(preview) => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"preview","status":"PASS","result":preview.to_json()})
                }
                Err(diagnostic) => failure(id, "frontend", &diagnostic),
            };
        }
        if operation == "preview" {
            let Some(module) = object.get("module").and_then(Value::as_str) else {
                return error(Some(id), "protocol", "preview requires a module path");
            };
            return match crate::preview_bundle(source, module) {
                Ok(bundle) => match serde_json::from_str::<Value>(&bundle) {
                    Ok(bundle) => {
                        json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"preview","status":"PASS","result":bundle})
                    }
                    Err(_) => error(Some(id), "compiler", "internal preview serialization error"),
                },
                Err(d) => failure(id, "frontend", &d),
            };
        }
        match crate::inspect(source) {
            Ok(diagnostics) => {
                let items: Vec<_> = diagnostics.iter().map(|d| json!({"code":d.code,"severity":"error","message":d.message,"source_range":d.range.map(|r| json!({"start_byte":r.start,"end_byte":r.end}))})).collect();
                json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"ios_validation","status":if operation == "inspect" {"PASS"} else {"FAIL"},"result":{"goya_frontend":"PASS","ios_validation":"FAIL","apple_validation":"NOT_RUN","diagnostics":items}})
            }
            Err(d) => failure(id, "frontend", &d),
        }
    }
}

fn failure(id: &str, stage: &str, diagnostic: &crate::Diagnostic) -> Value {
    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":stage,"status":"FAIL","diagnostics":[{"code":diagnostic.code,"severity":"error","message":diagnostic.message,"source_range":diagnostic.range.map(|r| json!({"start_byte":r.start,"end_byte":r.end}))}]})
}

fn error(id: Option<&str>, stage: &str, message: &str) -> Value {
    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":stage,"status":"FAIL","diagnostics":[{"code":"NPT-E0001","severity":"error","message":message}]})
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reuse this test binary as a compiler; no PATH mutation or Swift install.
    #[test]
    fn controlled_compiler() {
        let Ok(mode) = std::env::var("NEPTUNE_TEST_COMPILER_MODE") else {
            return;
        };
        use std::io::{Read, Write};
        match mode.as_str() {
            "flood" => {
                std::io::stderr()
                    .write_all(&vec![b'x'; 512 * 1024])
                    .unwrap();
                let mut source = Vec::new();
                std::io::stdin().read_to_end(&mut source).unwrap();
                assert_eq!(source.len(), MAX_REQUEST_BYTES as usize);
                std::process::exit(1);
            }
            "hang" => loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            "reject" => std::process::exit(1),
            "accept" => {
                let mut source = String::new();
                std::io::stdin().read_to_string(&mut source).unwrap();
                assert_eq!(source, "let answer = 42");
                std::process::exit(0);
            }
            _ => panic!("unknown compiler helper mode"),
        }
    }

    fn compiler(mode: &str) -> std::process::Command {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "service::tests::controlled_compiler",
                "--nocapture",
            ])
            .env("NEPTUNE_TEST_COMPILER_MODE", mode);
        command
    }

    #[test]
    fn compiler_drains_stderr_while_writing_source_and_bounds_diagnostics() {
        let report = swift_parse(
            &mut compiler("flood"),
            &"s".repeat(MAX_REQUEST_BYTES as usize),
            "flood",
            std::time::Duration::from_secs(5),
        );
        assert_eq!(report["status"], "FAIL");
        assert_eq!(
            report["diagnostics"].as_str().unwrap().len(),
            MAX_SWIFT_DIAGNOSTICS
        );
        assert_eq!(report["diagnostics_truncated"], true);
        assert_eq!(report["apple_sdk_validation"], "NOT_RUN");
    }

    #[test]
    fn compiler_timeout_covers_blocked_stdin() {
        let start = std::time::Instant::now();
        let report = swift_parse(
            &mut compiler("hang"),
            &"s".repeat(MAX_REQUEST_BYTES as usize),
            "hang",
            std::time::Duration::from_millis(100),
        );
        assert_eq!(report["status"], "FAIL");
        assert_eq!(
            report["diagnostics"][0]["message"],
            "swiftc exceeded its runtime limit"
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn compiler_success_failure_and_missing_tool_have_distinct_results() {
        let timeout = std::time::Duration::from_secs(5);
        let accepted = swift_parse(
            &mut compiler("accept"),
            "let answer = 42",
            "accept",
            timeout,
        );
        assert_eq!(accepted["status"], "PASS");
        assert_eq!(accepted["validation"], "swift_syntax_only");
        assert_eq!(accepted["apple_sdk_validation"], "NOT_RUN");
        let rejected = swift_parse(
            &mut compiler("reject"),
            &"s".repeat(MAX_REQUEST_BYTES as usize),
            "reject",
            timeout,
        );
        assert_eq!(rejected["status"], "FAIL");
        let missing = swift_parse(
            &mut std::process::Command::new("/nonexistent/neptune-test-swiftc"),
            "",
            "missing",
            timeout,
        );
        assert_eq!(missing["status"], "BLOCKED");
    }

    #[test]
    fn failed_protocol_requests_consume_valid_job_ids() {
        let mut session = Session::default();
        for request in [
            json!({"schema_version":"2", "job_id":"bad_version"}),
            json!({"schema_version":"1", "job_id":"unknown_field", "extra":true}),
        ] {
            assert_eq!(session.handle(&request)["status"], "FAIL");
            let retry = json!({"schema_version":"1", "job_id":request["job_id"], "operation":"inspect", "source":"@ontology x { @cell root { id: \"root\" text: \"Hi\" } }"});
            let result = session.handle(&retry);
            assert_eq!(
                result["diagnostics"][0]["message"],
                "job_id already used in this session"
            );
        }
    }

    #[test]
    fn session_rejects_reused_ids_and_unsupported_operations() {
        let mut session = Session::default();
        let request = json!({"schema_version":"1","job_id":"one","operation":"inspect","source":"@ontology x { @cell root { id: \"root\" text: \"Hi\" } }"});
        let result = session.handle(&request);
        assert_eq!(result["status"], "PASS");
        assert_eq!(result["result"]["ios_validation"], "FAIL");
        assert_eq!(session.handle(&request)["status"], "FAIL");
        assert_eq!(
            session.handle(&json!({"schema_version":"1","job_id":"two","operation":"build"}))
                ["status"],
            "FAIL"
        );
        assert_eq!(
            session.handle(&json!({"schema_version":"2","job_id":"three"}))["status"],
            "FAIL"
        );
        let preview = session.handle(&json!({"schema_version":"1","job_id":"four","operation":"preview","module":"ui/main.gscene","source":"@ontology x { @cell root { id: \"root\" text: \"Hi\" } }"}));
        assert_eq!(preview["status"], "PASS");
        assert_eq!(preview["result"]["schema_version"], "2");
        let missing_module = session.handle(
            &json!({"schema_version":"1","job_id":"five","operation":"preview","source":"x"}),
        );
        assert_eq!(missing_module["status"], "FAIL");
    }

    #[test]
    fn bluetooth_privacy_operation_plans_entries_and_fails_closed() {
        let mut session = Session::default();
        let all = session.handle(&json!({
            "schema_version": "1",
            "job_id": "bm-1",
            "operation": "plan-bluetooth-privacy",
            "usage_description": "Connects to your temperature sensor to show readings."
        }));
        assert_eq!(all["status"], "PASS");
        assert_eq!(all["stage"], "bluetooth_privacy");
        assert_eq!(
            all["result"]["privacy_plan"]["usage_key"],
            "NSBluetoothAlwaysUsageDescription"
        );
        assert_eq!(
            all["result"]["privacy_plan"]["legacy_usage_key"],
            "NSBluetoothPeripheralUsageDescription"
        );
        assert_eq!(all["result"]["privacy_plan"]["background_modes"], json!([]));
        assert_eq!(
            all["result"]["capability_entries"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
        assert_eq!(
            all["result"]["capability_entries"][0]["semantic_id"],
            "goya.hardware.bluetooth.access.v1"
        );
        // Deterministic across sessions.
        let mut again = Session::default();
        assert_eq!(
            all,
            again.handle(&json!({
                "schema_version": "1",
                "job_id": "bm-1",
                "operation": "plan-bluetooth-privacy",
                "usage_description": "Connects to your temperature sensor to show readings."
            }))
        );

        let explicit = session.handle(&json!({
            "schema_version": "1",
            "job_id": "bm-2",
            "operation": "plan-bluetooth-privacy",
            "usage_description": "Connects to your temperature sensor to show readings.",
            "contracts": ["goya.hardware.bluetooth.guard.v1", "goya.hardware.bluetooth.central.v1"]
        }));
        assert_eq!(explicit["status"], "PASS");
        assert_eq!(
            explicit["result"]["capability_entries"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        let generic = session.handle(&json!({
            "schema_version": "1",
            "job_id": "bm-3",
            "operation": "plan-bluetooth-privacy",
            "usage_description": "uses bluetooth"
        }));
        assert_eq!(generic["status"], "FAIL");
        assert_eq!(generic["diagnostics"][0]["code"], "NPT-BM-E002");

        let unknown = session.handle(&json!({
            "schema_version": "1",
            "job_id": "bm-4",
            "operation": "plan-bluetooth-privacy",
            "usage_description": "Connects to your temperature sensor to show readings.",
            "contracts": ["goya.hardware.bluetooth.pairing.v1"]
        }));
        assert_eq!(unknown["status"], "FAIL");
        assert_eq!(unknown["diagnostics"][0]["code"], "NPT-BM-E001");

        let missing = session.handle(&json!({
            "schema_version": "1",
            "job_id": "bm-5",
            "operation": "plan-bluetooth-privacy"
        }));
        assert_eq!(missing["status"], "FAIL");

        let duplicate = session.handle(&json!({
            "schema_version": "1",
            "job_id": "bm-1",
            "operation": "plan-bluetooth-privacy",
            "usage_description": "Connects to your temperature sensor to show readings."
        }));
        assert_eq!(duplicate["status"], "FAIL");
    }
}
