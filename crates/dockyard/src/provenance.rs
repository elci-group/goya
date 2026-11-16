//! Unsigned build predicate for one Dockyard layout.
//!
//! Not a signed attestation and not a SLSA claim. `signature` stays null.

use crate::PROFILE;

/// Compact predicate. Each argument is hex and is written as `sha256:` plus that hex.
pub fn predicate(
    source_sha256: &str,
    executable_sha256: &str,
    layer_sha256: &str,
    config_sha256: &str,
    manifest_sha256: &str,
) -> String {
    format!(
        "{{\"builder\":\"dockyard {PROFILE}\",\"materials\":[{{\"digest\":\"sha256:{source_sha256}\",\"role\":\"source\"}}],\"signature\":null,\"subjects\":[{{\"digest\":\"sha256:{config_sha256}\",\"name\":\"config\"}},{{\"digest\":\"sha256:{executable_sha256}\",\"name\":\"executable\"}},{{\"digest\":\"sha256:{layer_sha256}\",\"name\":\"layer\"}},{{\"digest\":\"sha256:{manifest_sha256}\",\"name\":\"manifest\"}}]}}"
    )
}

#[cfg(test)]
mod tests {
    use super::predicate;

    const SAMPLE: &str = concat!(
        "{\"builder\":\"dockyard dockyard.linux.http/0.0.1\",",
        "\"materials\":[{\"digest\":\"sha256:aa\",\"role\":\"source\"}],",
        "\"signature\":null,",
        "\"subjects\":[",
        "{\"digest\":\"sha256:dd\",\"name\":\"config\"},",
        "{\"digest\":\"sha256:bb\",\"name\":\"executable\"},",
        "{\"digest\":\"sha256:cc\",\"name\":\"layer\"},",
        "{\"digest\":\"sha256:ee\",\"name\":\"manifest\"}",
        "]}"
    );

    #[test]
    fn same_inputs_produce_identical_bytes() {
        let left = predicate("aa", "bb", "cc", "dd", "ee");
        let right = predicate("aa", "bb", "cc", "dd", "ee");
        assert_eq!(left.as_bytes(), right.as_bytes());
    }

    #[test]
    fn sample_digests_order_subjects_by_name_and_leave_signature_null() {
        let text = predicate("aa", "bb", "cc", "dd", "ee");
        assert_eq!(text, SAMPLE);
        let subjects = text.split("\"subjects\":").nth(1).expect("subjects");
        let config = subjects.find("\"name\":\"config\"").expect("config");
        let executable = subjects
            .find("\"name\":\"executable\"")
            .expect("executable");
        let layer = subjects.find("\"name\":\"layer\"").expect("layer");
        let manifest = subjects.find("\"name\":\"manifest\"").expect("manifest");
        assert!(config < executable && executable < layer && layer < manifest);
        assert!(text.contains("\"signature\":null"));
        assert!(!text.contains("\"signed\""));
    }

    #[test]
    fn output_does_not_contain_a_home_path() {
        let text = predicate("aa", "bb", "cc", "dd", "ee");
        assert!(!text.contains("/home/"));
    }
}
