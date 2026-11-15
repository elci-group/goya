//! Version 1 local, synchronous JSON-lines inspection protocol.
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const MAX_REQUEST_BYTES: u64 = 256 * 1024;
pub const MAX_JOBS: usize = 1024;

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
                | "swift-parse"
        ) {
            return error(Some(id), "protocol", "operation is not supported");
        }
        if operation == "swift-parse" {
            let Some(source) = object.get("source").and_then(Value::as_str) else {
                return error(Some(id), "swift_parse", "source string is required");
            };
            let mut child = match std::process::Command::new("swiftc")
                .args(["-frontend", "-parse", "-"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    return json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"swift_parse","status":"BLOCKED","reason":format!("swiftc unavailable: {e}"),"apple_sdk_validation":"NOT_RUN"})
                }
            };
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                if stdin.write_all(source.as_bytes()).is_err() {
                    return error(Some(id), "swift_parse", "could not send source to swiftc");
                }
            }
            return match child.wait_with_output() {
                Ok(result) if result.status.success() => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"swift_parse","status":"PASS","validation":"swift_syntax_only","apple_sdk_validation":"NOT_RUN"})
                }
                Ok(result) => {
                    json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"swift_parse","status":"FAIL","diagnostics":String::from_utf8_lossy(&result.stderr)})
                }
                Err(e) => error(Some(id), "swift_parse", &e.to_string()),
            };
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
}
