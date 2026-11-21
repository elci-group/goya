//! Experimental fail-closed semantic boundary for the Maja web target.
//! Inspection preserves the canonical scene and does not establish support.

mod interactive;
pub use interactive::{
    admit_local_interactive, InteractiveWebIr, WebCell, WebCellKind, INTERACTIVE_PROFILE,
    LIVE_PROFILE,
};

use goya_lang::{handoff, SemanticHandoff, SourceRange, StrictParseError};
use sha2::{Digest, Sha256};
use std::fmt;

pub const WEB_IR_CONTRACT: &str = "MAJA-WEB-IR/1-draft";
pub const WEB_PROFILE: &str = "maja.web.v1";
/// Experimental direct-browser static profile; no production qualification is implied.
pub const LOCAL_PROFILE: &str = "maja.web.local.v1";

/// Half-open UTF-8 byte offsets; an empty range may indicate an unavailable origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub range: Option<SourceRange>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Diagnostic {}
impl From<StrictParseError> for Diagnostic {
    fn from(error: StrictParseError) -> Self {
        Self {
            code: error.code,
            message: error.to_string(),
            range: Some(error.range),
        }
    }
}

/// Inspection result, not evidence of target admission.
#[derive(Debug, Clone)]
pub struct InspectedScene {
    handoff: SemanticHandoff,
    source: String,
}
impl InspectedScene {
    pub fn handoff(&self) -> &SemanticHandoff {
        &self.handoff
    }

    /// Exact UTF-8 source bytes bound to the canonical source-map digest.
    pub fn source(&self) -> &str {
        &self.source
    }
}

pub fn inspect(source: &str) -> Result<InspectedScene, Diagnostic> {
    let handoff = handoff::admit(source).map_err(Diagnostic::from)?;
    if handoff.contract != goya_lang::SEMANTIC_HANDOFF_CONTRACT {
        return Err(Diagnostic {
            code: "MAJA-E0001",
            message: "unsupported canonical handoff version".into(),
            range: None,
        });
    }
    let digest: [u8; 32] = Sha256::digest(source.as_bytes()).into();
    let valid = handoff.parsed.source_digest == digest
        && handoff.parsed.entries.iter().all(|entry| {
            entry.source_digest == digest
                && entry.range.start <= entry.range.end
                && entry.range.end <= source.len()
                && source.is_char_boundary(entry.range.start)
                && source.is_char_boundary(entry.range.end)
        });
    if !valid {
        return Err(Diagnostic {
            code: "MAJA-E0004",
            message: "canonical source map does not match source bytes".into(),
            range: None,
        });
    }
    Ok(InspectedScene {
        handoff,
        source: source.to_owned(),
    })
}

/// Future admitted web IR. This marker has no public constructor.
#[derive(Debug)]
pub struct WebIr {
    _private: (),
}

/// Source-grounded static text candidate. Geometry, typography and browser
/// rendering equivalence are not asserted by this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticWebIr {
    semantic_id: String,
    text: String,
    source_digest: [u8; 32],
    origin: SourceRange,
}

impl StaticWebIr {
    pub fn semantic_id(&self) -> &str {
        &self.semantic_id
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn source_digest(&self) -> [u8; 32] {
        self.source_digest
    }
    pub fn origin(&self) -> SourceRange {
        self.origin
    }
}

/// Admit only a single authored cell containing static text for the local
/// experimental profile. This is not production or browser qualification.
/// Every other authored construct is rejected before projection.
pub fn admit_local_static(
    scene: &InspectedScene,
    profile: &str,
) -> Result<StaticWebIr, Diagnostic> {
    if profile != LOCAL_PROFILE {
        return Err(Diagnostic {
            code: "MAJA-E0002",
            message: format!("unknown local profile: {profile}"),
            range: None,
        });
    }
    let parsed = &scene.handoff.parsed;
    let reject = |message: String, range: Option<SourceRange>| Diagnostic {
        code: "MAJA-E0100",
        message,
        range,
    };
    // Enumerate source constructs, not just non-default AST values: even an
    // explicitly authored default must not disappear without admission.
    for entry in &parsed.entries {
        let allowed = match entry.kind.as_str() {
            "ontology" => true,
            "cell" => entry.scene_path.as_deref() == Some(&[]),
            "property" => {
                entry.scene_path.as_deref() == Some(&[])
                    && matches!(entry.name.as_str(), "id" | "text")
            }
            _ => false,
        };
        if !allowed {
            return Err(reject(
                format!(
                    "unsupported local static construct: {} {}",
                    entry.kind, entry.name
                ),
                Some(entry.range),
            ));
        }
    }
    let root = &parsed.scene.root;
    if root.kind != goya_lang::CellKind::Cell
        || !root.children.is_empty()
        || !parsed.scene.interactions.is_empty()
    {
        return Err(reject("only one static cell is admitted".into(), None));
    }
    let id = root
        .authored_id
        .as_ref()
        .ok_or_else(|| reject("static cell requires an authored identity".into(), None))?;
    let text = root
        .text
        .as_ref()
        .ok_or_else(|| reject("static cell requires text".into(), None))?;
    let origin = parsed
        .entries
        .iter()
        .find(|entry| {
            entry.kind == "property"
                && entry.name == "text"
                && entry.scene_path.as_deref() == Some(&[])
        })
        .map(|entry| entry.range)
        .ok_or_else(|| reject("static text source origin is unavailable".into(), None))?;
    if text.contains(['\0', '\r']) {
        return Err(reject(
            "static text contains a character normalized by HTML parsing".into(),
            Some(origin),
        ));
    }
    if id.contains(['\0', '\r', '\n', '\t']) {
        let id_origin = parsed
            .entries
            .iter()
            .find(|entry| {
                entry.kind == "property"
                    && entry.name == "id"
                    && entry.scene_path.as_deref() == Some(&[])
            })
            .map(|entry| entry.range);
        return Err(reject(
            "authored identity contains a character normalized by HTML parsing".into(),
            id_origin,
        ));
    }
    Ok(StaticWebIr {
        semantic_id: id.clone(),
        text: text.clone(),
        source_digest: parsed.source_digest,
        origin,
    })
}

/// Reject every build until the profile and target lowering are qualified.
pub fn admit_web(_scene: &InspectedScene, profile: &str) -> Result<WebIr, Diagnostic> {
    if profile != WEB_PROFILE {
        return Err(Diagnostic {
            code: "MAJA-E0002",
            message: format!("unknown web profile: {profile}"),
            range: None,
        });
    }
    Err(Diagnostic {
        code: "MAJA-E0003",
        message: "web profile is not qualified; no production features are admitted".into(),
        range: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SCENE: &str = "@ontology demo { @cell root { id: \"root\"; text: \"Hello\" } }";

    #[test]
    fn inspection_preserves_canonical_identity_and_revision() {
        let inspected = inspect(SCENE).unwrap();
        let parsed = &inspected.handoff().parsed;
        assert_eq!(parsed.scene.root.authored_id.as_deref(), Some("root"));
        assert_eq!(parsed.scene.root.text.as_deref(), Some("Hello"));
        assert_eq!(inspected.source().as_bytes(), SCENE.as_bytes());
        assert!(parsed
            .entries
            .iter()
            .all(|entry| entry.source_digest == parsed.source_digest));
    }
    #[test]
    fn unicode_source_ranges_remain_bound_to_original_utf8() {
        let source = "@ontology demo { @cell root { id: \"root\"; text: \"γειά 🌍\" } }";
        let inspected = inspect(source).unwrap();
        assert_eq!(inspected.source(), source);
        for entry in &inspected.handoff().parsed.entries {
            assert!(source.get(entry.range.start..entry.range.end).is_some());
        }
    }
    #[test]
    fn unsupported_source_is_rejected_with_canonical_origin() {
        let source = "@ontology demo { @cell root { id: \"root\"; mystery: 1 } }";
        let error = inspect(source).unwrap_err();
        assert_eq!(error.code, "GY-E1001");
        let range = error.range.unwrap();
        assert!(source[range.start..range.end].contains("mystery"));
    }
    #[test]
    fn valid_source_cannot_enter_unqualified_profile() {
        let inspected = inspect(SCENE).unwrap();
        assert_eq!(
            admit_web(&inspected, WEB_PROFILE).unwrap_err().code,
            "MAJA-E0003"
        );
        assert_eq!(
            admit_web(&inspected, "other").unwrap_err().code,
            "MAJA-E0002"
        );
    }

    #[test]
    fn local_static_preserves_identity_text_and_origin() {
        let source = "@ontology demo { @cell root { id: \"root\"; text: \"γειά 🌍\" } }";
        let scene = inspect(source).unwrap();
        let ir = admit_local_static(&scene, LOCAL_PROFILE).unwrap();
        assert_eq!(ir.semantic_id(), "root");
        assert_eq!(ir.text(), "γειά 🌍");
        assert_eq!(ir.source_digest(), scene.handoff().parsed.source_digest);
        assert!(source[ir.origin().start..ir.origin().end].contains("γειά"));
    }

    #[test]
    fn local_static_rejects_authored_properties_and_nested_nodes() {
        for source in [
            "@ontology x { @cell r { id: \"r\"; text: \"Hi\"; scroll: false } }",
            "@ontology x { @cell r { id: \"r\"; text: \"Hi\"; extent: (10,10,0) } }",
            "@ontology x { @cell r { id: \"r\"; text: \"Hi\"; @cell c { id: \"c\" } } }",
            "@ontology x { @button r { id: \"r\"; text: \"Hi\" } }",
        ] {
            let scene = inspect(source).unwrap();
            assert_eq!(
                admit_local_static(&scene, LOCAL_PROFILE).unwrap_err().code,
                "MAJA-E0100"
            );
        }
    }

    #[test]
    fn local_static_rejects_html_normalized_characters_with_source_origin() {
        for character in ['\0', '\r'] {
            let source =
                format!("@ontology x {{ @cell r {{ id: \"r\"; text: \"a{character}b\" }} }}");
            let scene = inspect(&source).unwrap();
            let error = admit_local_static(&scene, LOCAL_PROFILE).unwrap_err();
            assert_eq!(error.code, "MAJA-E0100");
            let range = error.range.unwrap();
            assert!(source[range.start..range.end].contains("text"));
        }
        for character in ['\0', '\r', '\n', '\t'] {
            let source =
                format!("@ontology x {{ @cell r {{ id: \"a{character}b\"; text: \"Hi\" }} }}");
            let scene = inspect(&source).unwrap();
            let error = admit_local_static(&scene, LOCAL_PROFILE).unwrap_err();
            assert_eq!(error.code, "MAJA-E0100");
            let range = error.range.unwrap();
            assert!(source[range.start..range.end].contains("id"));
        }
    }
}
