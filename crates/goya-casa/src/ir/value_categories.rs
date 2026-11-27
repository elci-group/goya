//! CSS value processing categories (directive §6).
//!
//! A property's value moves through the categories as the pipeline
//! resolves it. Phase 1 produces `Declared` values only; later phases
//! fill the remaining categories.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueCategory {
    /// As written in the stylesheet, before any resolution.
    Declared,
    /// Winner of the cascade for a given element.
    Cascaded,
    /// After inheritance and defaulting.
    Specified,
    /// After intra-value dependencies (custom properties, em, percentages) resolve.
    Computed,
    /// After layout-dependent resolution (used values, e.g. pixel-snapped).
    Used,
    /// After device quantisation — what is actually presented.
    Actual,
    /// Value guarded by a condition (media/container/supports) that may or may not apply.
    Conditional,
    /// Converted to a Goya-native scene representation.
    Native,
}

impl ValueCategory {
    pub const ALL: [ValueCategory; 8] = [
        ValueCategory::Declared,
        ValueCategory::Cascaded,
        ValueCategory::Specified,
        ValueCategory::Computed,
        ValueCategory::Used,
        ValueCategory::Actual,
        ValueCategory::Conditional,
        ValueCategory::Native,
    ];

    /// The pipeline stage that produces values of this category.
    pub fn pipeline_stage(&self) -> &'static str {
        match self {
            ValueCategory::Declared => "ingestion",
            ValueCategory::Cascaded | ValueCategory::Specified | ValueCategory::Computed => {
                "cascade"
            }
            ValueCategory::Used | ValueCategory::Actual => "geometry",
            ValueCategory::Conditional => "adaptive",
            ValueCategory::Native => "optimisation",
        }
    }
}

/// A value annotated with the pipeline category it was produced at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategorisedValue {
    pub category: ValueCategory,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_are_distinct_and_stage_mapped() {
        assert_eq!(ValueCategory::ALL.len(), 8);
        let stages: Vec<_> = ValueCategory::ALL
            .iter()
            .map(|c| c.pipeline_stage())
            .collect();
        assert_eq!(stages[0], "ingestion");
        assert_eq!(stages[ValueCategory::ALL.len() - 1], "optimisation");
    }

    #[test]
    fn categorised_value_serialises() {
        let value = CategorisedValue {
            category: ValueCategory::Declared,
            text: "10px".into(),
        };
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(json, r#"{"category":"declared","text":"10px"}"#);
        assert_eq!(
            serde_json::from_str::<CategorisedValue>(&json).unwrap(),
            value
        );
    }
}
