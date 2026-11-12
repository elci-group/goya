//! Version 1 local, synchronous JSON-lines inspection protocol.
use std::collections::BTreeSet;
use serde_json::{json, Value};

pub const MAX_REQUEST_BYTES: u64 = 256 * 1024;
pub const MAX_JOBS: usize = 1024;

#[derive(Default)]
pub struct Session { jobs: BTreeSet<String> }

impl Session {
    /// Handle one request. Job IDs are single-use within a session; clients
    /// must create a new ID for a retry. Each request emits one final event.
    pub fn handle(&mut self, request: &Value) -> Value {
        let Some(object) = request.as_object() else { return error(None, "protocol", "request must be an object"); };
        let Some(id) = object.get("job_id").and_then(Value::as_str).filter(|id| !id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')) else {
            return error(None, "protocol", "job_id must be 1..128 ASCII letters, digits, underscores or hyphens");
        };
        if object.get("schema_version").and_then(Value::as_str) != Some("1") { return error(Some(id), "protocol", "unsupported schema version"); }
        if object.keys().any(|key| !matches!(key.as_str(), "schema_version" | "job_id" | "operation" | "source")) { return error(Some(id), "protocol", "unknown request field"); }
        if self.jobs.contains(id) { return error(Some(id), "protocol", "job_id already used in this session"); }
        if self.jobs.len() >= MAX_JOBS { return error(Some(id), "protocol", "session job limit reached"); }
        self.jobs.insert(id.to_owned());
        let Some(operation) = object.get("operation").and_then(Value::as_str) else { return error(Some(id), "protocol", "operation is required"); };
        if !matches!(operation, "inspect" | "check" | "preview") { return error(Some(id), "protocol", "operation is not supported"); }
        let Some(source) = object.get("source").and_then(Value::as_str) else { return error(Some(id), "ingest", "source must be a UTF-8 string"); };
        if operation == "preview" {
            return match crate::preview_bundle(source) {
                Ok(bundle) => match serde_json::from_str::<Value>(&bundle) {
                    Ok(bundle) => json!({"schema_version":"1","job_id":id,"sequence":1,"stage":"preview","status":"PASS","result":bundle}),
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
    fn session_rejects_reused_ids_and_unsupported_operations() {
        let mut session = Session::default();
        let request = json!({"schema_version":"1","job_id":"one","operation":"inspect","source":"@ontology x { @cell root { id: \"root\" text: \"Hi\" } }"});
        let result = session.handle(&request);
        assert_eq!(result["status"], "PASS");
        assert_eq!(result["result"]["ios_validation"], "FAIL");
        assert_eq!(session.handle(&request)["status"], "FAIL");
        assert_eq!(session.handle(&json!({"schema_version":"1","job_id":"two","operation":"build"}))["status"], "FAIL");
        assert_eq!(session.handle(&json!({"schema_version":"2","job_id":"three"}))["status"], "FAIL");
    }
}
