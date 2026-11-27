//! Versioned diagnostic registry: canonical identifiers, emoji presentation
//! mappings and the emoji↔ID split (ESC-INV-004, §5, §20).
//!
//! An emoji is presentation only. Machine meaning lives in the canonical
//! `<class>.<CODE>` identifier; consumers never interpret glyphs.

use serde::{Deserialize, Serialize};

/// Semantic version of this registry. Emoji↔ID mappings SHALL NOT be reused
/// with incompatible meanings across registry versions.
pub const REGISTRY_VERSION: &str = "escene-registry/1.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnnotationClass {
    /// Compilation — intent realisation.
    #[serde(rename = "C")]
    C,
    /// Interpretation — experienced behaviour.
    #[serde(rename = "I")]
    I,
    /// Substitution — Caprichio provenance.
    #[serde(rename = "S")]
    S,
}

/// Canonical diagnostic identifier `<class>.<CODE>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DiagnosticId(pub String);

impl DiagnosticId {
    pub fn new(class: AnnotationClass, code: &str) -> DiagnosticId {
        let prefix = match class {
            AnnotationClass::C => "C",
            AnnotationClass::I => "I",
            AnnotationClass::S => "S",
        };
        DiagnosticId(format!("{prefix}.{code}"))
    }

    /// Parse from `C.CODE` form, validating class/code agreement.
    pub fn parse(s: &str) -> Option<(AnnotationClass, &str)> {
        let (prefix, code) = s.split_once('.')?;
        if code.is_empty() || !code.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
            return None;
        }
        let class = match prefix {
            "C" => AnnotationClass::C,
            "I" => AnnotationClass::I,
            "S" => AnnotationClass::S,
            _ => return None,
        };
        Some((class, code))
    }

    pub fn class(&self) -> Option<AnnotationClass> {
        Self::parse(&self.0).map(|(c, _)| c)
    }
}

/// Presentation-level emoji mapped from a canonical identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emoji(pub &'static str);

/// One registry entry: canonical id, presentation emoji and human meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    pub id: DiagnosticId,
    pub emoji: Emoji,
    pub meaning: &'static str,
}

/// The v1.0.0 diagnostic and emoji registry (directive §5).
pub fn registry() -> Registry {
    use AnnotationClass::*;
    let entries = vec![
        // §5.1 Compilation
        (C, "NATIVE_LOWERING", "⚙️", "Intent lowered into target-executable implementation"),
        (C, "PLATFORM_PRIMITIVE", "🏠", "Native platform facility employed"),
        (C, "SYNTHESISED", "🧱", "Implementation synthesised by Goya"),
        (C, "DECOMPOSED", "🧩", "Construct resolved through multiple implementation units"),
        (C, "PRECOMPUTED", "🧮", "Computation resolved during compilation"),
        (C, "ELIMINATED", "✂️", "Construct removed under verified optimisation conditions"),
        (C, "DEPENDENCY_INTRODUCED", "🔗", "Target-specific dependency introduced"),
        (C, "PACKAGED", "📦", "Implementation incorporated into output artefact"),
        (C, "REPRESENTATION_COMPRESSED", "🗜️", "Compressed representation selected"),
        (C, "EQUIVALENCE_VERIFIED", "🧬", "Required semantic equivalence established"),
        (C, "TRANSLATION_LIMITATION", "⚠️", "Translation limitation encountered"),
        (C, "TRANSLATION_FAILED", "🚫", "Construct could not be translated"),
        (C, "FALLBACK_LOWERING", "🔄", "Alternative lowering mechanism employed"),
        (C, "MAPPING_INCOMPLETE", "🗺️", "Implementation-to-source attribution unavailable"),
        // §5.2 Interpretation
        (I, "HOT_PATH", "🔥", "Elevated observed execution frequency"),
        (I, "COLD_PATH", "❄️", "Low observed execution frequency"),
        (I, "RESOURCE_SENSITIVE", "🐘", "Sensitivity to one or more resource constraints"),
        (I, "REDUNDANT_CALL", "🔁", "Evidence of semantically redundant invocation"),
        (I, "PREEXPOSURE_OPPORTUNITY", "🔮", "Evidence supporting anticipatory preparation"),
        (I, "THERMAL_PRESSURE", "🌡️", "Thermal conditions affecting execution"),
        (I, "ENERGY_PRESSURE", "🔋", "Elevated or constrained energy consumption"),
        (I, "LATENCY_EXCEEDED", "⏳", "Execution latency exceeded defined budget"),
        (I, "CONTENTION", "🧵", "Concurrent execution contention"),
        (I, "CACHE_INEFFICIENCY", "🧊", "Inefficient caching or residency behaviour"),
        (I, "DEPENDENCY_ISSUE", "🔗", "Dependency availability or compatibility problem"),
        (I, "NETWORK_SENSITIVITY", "🌐", "Execution sensitive to network conditions"),
        (I, "IO_BOTTLENECK", "💾", "Storage or I/O constraint encountered"),
        (I, "BOTTLENECK", "🚧", "Evidence-supported computational bottleneck"),
        (I, "RESOURCE_EFFICIENT", "🪶", "Defined resource-efficiency criteria satisfied"),
        (I, "PERFORMANCE_EFFICIENT", "⚡", "Defined execution-efficiency criteria satisfied"),
        (I, "UNOBSERVED", "👻", "Construct not observed within the specified scope"),
        (I, "EXECUTION_FAILED", "💥", "Runtime or interpretation failure"),
        (I, "INSUFFICIENT_EVIDENCE", "❓", "Evidence inadequate for classification"),
        // §5.3 Substitution
        (S, "CAPRICHO_APPLIED", "🔄", "General Caprichio substitution applied"),
        (S, "GRAPHICAL_REPRESENTATION", "🎨", "Graphical representation substituted"),
        (S, "COMPRESSION", "🗜️", "Compression strategy applied"),
        (S, "COMPUTATIONAL_REUSE", "♻️", "Existing computational result reused"),
        (S, "PRECOMPUTATION", "🧮", "Runtime computation substituted by precomputation"),
        (S, "PLATFORM_NATIVE", "🏠", "Platform-native implementation substituted"),
        (S, "PARALLELISATION", "🧩", "Concurrent implementation substituted"),
        (S, "NEURAL_EXECUTION", "🧠", "Neural execution strategy substituted"),
        (S, "DISTRIBUTED_EXECUTION", "🌐", "Distributed computation strategy applied"),
        (S, "DATA_REPRESENTATION", "🗃️", "Alternative data representation employed"),
        (S, "RESIDENCY", "💾", "Alternative resource residency strategy employed"),
        (S, "RESOURCE_REDUCTION", "🪶", "Resource-reduction Caprichio applied"),
        (S, "FALLBACK_APPLIED", "↩️", "Fallback Caprichio employed"),
        (S, "CAPRICHO_REJECTED", "🚫", "Candidate Caprichio rejected"),
        (S, "CAPRICHO_REVERTED", "⏪", "Previously applied substitution reverted"),
    ];
    let entries: Vec<RegistryEntry> = entries
        .into_iter()
        .map(|(class, code, emoji, meaning)| RegistryEntry {
            id: DiagnosticId::new(class, code),
            emoji: Emoji(emoji),
            meaning,
        })
        .collect();
    Registry { entries }
}

#[derive(Debug, Clone)]
pub struct Registry {
    pub entries: Vec<RegistryEntry>,
}

impl Registry {
    pub fn lookup(&self, id: &DiagnosticId) -> Option<&RegistryEntry> {
        self.entries.iter().find(|e| &e.id == id)
    }

    pub fn emoji_for(&self, id: &DiagnosticId) -> Option<Emoji> {
        self.lookup(id).map(|e| e.emoji)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_is_presentation_not_meaning() {
        let r = registry();
        // 🔗 appears under two classes with distinct canonical identities.
        let c_dep = r.lookup(&DiagnosticId::new(AnnotationClass::C, "DEPENDENCY_INTRODUCED")).unwrap();
        let i_dep = r.lookup(&DiagnosticId::new(AnnotationClass::I, "DEPENDENCY_ISSUE")).unwrap();
        assert_eq!(c_dep.emoji.0, i_dep.emoji.0);
        assert_ne!(c_dep.id, i_dep.id);
        // 🧮 likewise spans C and S.
        assert_eq!(
            r.emoji_for(&DiagnosticId::new(AnnotationClass::C, "PRECOMPUTED")).unwrap().0,
            r.emoji_for(&DiagnosticId::new(AnnotationClass::S, "PRECOMPUTATION")).unwrap().0
        );
    }

    #[test]
    fn parse_validates_class_code_agreement() {
        assert!(DiagnosticId::parse("C.NATIVE_LOWERING").is_some());
        assert!(DiagnosticId::parse("c.native_lowering").is_none());
        assert!(DiagnosticId::parse("X.NOTHING").is_none());
        assert!(DiagnosticId::parse("C.").is_none());
    }

    #[test]
    fn registry_ids_match_class() {
        for e in registry().entries {
            assert_eq!(DiagnosticId::parse(&e.id.0).unwrap().0, e.id.class().unwrap());
        }
    }
}
