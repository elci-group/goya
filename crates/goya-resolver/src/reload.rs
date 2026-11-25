//! Classification decides which published values survive a replacement.
//!
//! A kind change refuses the replacement. The caller has not swapped the
//! running instance yet.

use std::collections::BTreeMap;

use goya_lang::{InteractionValue, SourceRange};

use crate::diagnostics::{located, Diagnostic};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReloadClass {
    pub preserved_keys: Vec<String>,
    pub initialised_keys: Vec<String>,
    pub dropped_keys: Vec<String>,
    pub values: BTreeMap<String, InteractionValue>,
}

pub(crate) fn classify(
    previous: &BTreeMap<String, InteractionValue>,
    next: &BTreeMap<String, InteractionValue>,
    spans: &BTreeMap<String, SourceRange>,
) -> Result<ReloadClass, Diagnostic> {
    let mut preserved_keys = Vec::new();
    let mut initialised_keys = Vec::new();
    let mut values = BTreeMap::new();

    for (key, next_value) in next {
        if let Some(previous_value) = previous.get(key) {
            let old = kind_name(previous_value);
            let new = kind_name(next_value);
            if old != new {
                return Err(located(
                    "GY-RESOLVER-RELOAD",
                    format!("state key {key} changed from {old} to {new}"),
                    spans.get(key).copied(),
                ));
            }
            preserved_keys.push(key.clone());
            values.insert(key.clone(), previous_value.clone());
        } else {
            initialised_keys.push(key.clone());
            values.insert(key.clone(), next_value.clone());
        }
    }

    let dropped_keys = previous
        .keys()
        .filter(|key| !next.contains_key(key.as_str()))
        .cloned()
        .collect();

    Ok(ReloadClass {
        preserved_keys,
        initialised_keys,
        dropped_keys,
        values,
    })
}

fn kind_name(value: &InteractionValue) -> &'static str {
    match value {
        InteractionValue::Int(_) => "int",
        InteractionValue::Text(_) => "text",
        InteractionValue::Bool(_) => "bool",
    }
}
