//! Deterministic SPDX 2.3 document for one Dockyard static executable.
//!
//! The layer digest is only the document namespace. The package checksum is
//! the executable. Created time is the Unix epoch.

use crate::{CREATED, PROFILE};

/// Compact SPDX 2.3 JSON for one static executable.
///
/// `source_sha256` is the admitted source identity. This document does not
/// describe it as a package or a checksum.
pub fn spdx_json(
    service_name: &str,
    source_sha256: &str,
    executable_sha256: &str,
    layer_sha256: &str,
) -> String {
    // The source digest is not a package and is not a checksum.
    let _ = source_sha256;
    let creator = format!("Tool: dockyard-{PROFILE}");
    let namespace = format!("https://elci.goya.invalid/dockyard/{layer_sha256}");
    let document_name = format!("dockyard-{service_name}");
    let mut out = String::new();
    out.push('{');
    push_key(&mut out, "SPDXID");
    push_json_string(&mut out, "SPDXRef-DOCUMENT");
    out.push(',');
    push_key(&mut out, "creationInfo");
    out.push('{');
    push_key(&mut out, "created");
    push_json_string(&mut out, CREATED);
    out.push(',');
    push_key(&mut out, "creators");
    out.push('[');
    push_json_string(&mut out, &creator);
    out.push_str("]},");
    push_key(&mut out, "dataLicense");
    push_json_string(&mut out, "CC0-1.0");
    out.push(',');
    push_key(&mut out, "documentDescribes");
    out.push('[');
    push_json_string(&mut out, "SPDXRef-service");
    out.push_str("],");
    push_key(&mut out, "documentNamespace");
    push_json_string(&mut out, &namespace);
    out.push(',');
    push_key(&mut out, "name");
    push_json_string(&mut out, &document_name);
    out.push(',');
    push_key(&mut out, "packages");
    out.push_str("[{");
    push_key(&mut out, "SPDXID");
    push_json_string(&mut out, "SPDXRef-service");
    out.push(',');
    push_key(&mut out, "checksums");
    out.push_str("[{");
    push_key(&mut out, "algorithm");
    push_json_string(&mut out, "SHA256");
    out.push(',');
    push_key(&mut out, "checksumValue");
    push_json_string(&mut out, executable_sha256);
    out.push_str("}],");
    push_key(&mut out, "downloadLocation");
    push_json_string(&mut out, "NOASSERTION");
    out.push(',');
    push_key(&mut out, "filesAnalyzed");
    out.push_str("false,");
    push_key(&mut out, "licenseConcluded");
    push_json_string(&mut out, "NOASSERTION");
    out.push(',');
    push_key(&mut out, "licenseDeclared");
    push_json_string(&mut out, "NOASSERTION");
    out.push(',');
    push_key(&mut out, "name");
    push_json_string(&mut out, service_name);
    out.push_str("}],");
    push_key(&mut out, "spdxVersion");
    push_json_string(&mut out, "SPDX-2.3");
    out.push('}');
    out
}

fn push_key(out: &mut String, key: &str) {
    push_json_string(out, key);
    out.push(':');
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let code = u32::from(c);
                out.push_str(&format!("\\u{code:04x}"));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping() -> String {
        spdx_json("ping", "abc", "def", "123")
    }

    #[test]
    fn same_inputs_are_byte_identical() {
        let first = ping();
        let second = ping();
        assert_eq!(first.as_bytes(), second.as_bytes());
    }

    #[test]
    fn ping_sample_names_the_executable_without_host_paths() {
        let document = ping();
        assert!(document.contains("SPDX-2.3"));
        assert!(document.contains("1970-01-01T00:00:00Z"));
        assert!(document.contains("def"));
        assert!(!document.contains("/home/"));
        assert!(!document.contains("secret"));
        assert_eq!(
            document,
            r#"{"SPDXID":"SPDXRef-DOCUMENT","creationInfo":{"created":"1970-01-01T00:00:00Z","creators":["Tool: dockyard-dockyard.linux.http/0.0.1"]},"dataLicense":"CC0-1.0","documentDescribes":["SPDXRef-service"],"documentNamespace":"https://elci.goya.invalid/dockyard/123","name":"dockyard-ping","packages":[{"SPDXID":"SPDXRef-service","checksums":[{"algorithm":"SHA256","checksumValue":"def"}],"downloadLocation":"NOASSERTION","filesAnalyzed":false,"licenseConcluded":"NOASSERTION","licenseDeclared":"NOASSERTION","name":"ping"}],"spdxVersion":"SPDX-2.3"}"#
        );
    }

    #[test]
    fn namespace_ends_with_the_layer_sha() {
        let document = ping();
        let namespace = document
            .split("\"documentNamespace\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("namespace");
        assert_eq!(namespace, "https://elci.goya.invalid/dockyard/123");
        assert!(namespace.ends_with("123"));
    }
}
