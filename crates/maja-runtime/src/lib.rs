//! Experimental, narrowly admitted D1 action code generation.
//! This module does not own state, bind controls, or admit a web profile.

use goya_lang::{InteractionClass, InteractionExpr, InteractionType, InteractionValue, Scene};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    CanonicalValidation(String),
    UnsupportedShape,
    MissingActionIdentity,
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CanonicalValidation(reason) => {
                write!(f, "canonical D1 validation failed: {reason}")
            }
            Self::UnsupportedShape => f.write_str("unsupported D1 action shape"),
            Self::MissingActionIdentity => f.write_str("D1 action requires an authored identity"),
        }
    }
}
impl std::error::Error for CodegenError {}

/// JavaScript is a pure function expression `(value, revision) => result`.
/// Both arguments and successful output values are BigInts. On failure the
/// result contains only an error code; it does not publish a new snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedAction {
    pub action_id: String,
    pub state_key: String,
    pub javascript: String,
}

/// Compile one canonical D1 action shaped exactly as `set key = state.key + N`.
/// The caller must separately validate and supply the initial typed state and
/// the mapping from authored controls to action and visible state.
pub fn emit_checked_increment(scene: &Scene) -> Result<GeneratedAction, CodegenError> {
    let compiled = goya_lang::compile_d1_interactions(scene)
        .map_err(|e| CodegenError::CanonicalValidation(e.to_string()))?;
    if scene.interactions.len() != 1 || compiled.len() != 1 {
        return Err(CodegenError::UnsupportedShape);
    }
    let action = &scene.interactions[0];
    if action.class != InteractionClass::D1
        || !action.declared_effects.is_empty()
        || !action.requests.is_empty()
        || !action.inputs.is_empty()
        || action.state_reads.len() != 1
        || action.state_writes.len() != 1
        || action.assignments.len() != 1
        || action.state_reads[0].value_type != InteractionType::Int
        || action.state_writes[0].value_type != InteractionType::Int
        || action.state_reads[0].name != action.state_writes[0].name
        || action.assignments[0].key != action.state_reads[0].name
    {
        return Err(CodegenError::UnsupportedShape);
    }
    let id = action
        .authored_id
        .as_ref()
        .filter(|id| !id.is_empty())
        .ok_or(CodegenError::MissingActionIdentity)?;
    let InteractionExpr::Add(left, right) = &action.assignments[0].value else {
        return Err(CodegenError::UnsupportedShape);
    };
    let (InteractionExpr::State(key), InteractionExpr::Literal(InteractionValue::Int(delta))) =
        (&**left, &**right)
    else {
        return Err(CodegenError::UnsupportedShape);
    };
    if key != &action.state_reads[0].name {
        return Err(CodegenError::UnsupportedShape);
    }
    let javascript = format!(
        r#"(value, revision) => {{
  if (typeof value !== "bigint" || value < -9223372036854775808n || value > 9223372036854775807n || typeof revision !== "bigint" || revision < 0n || revision > 18446744073709551615n) return {{ok:false, error:"invalid-snapshot"}};
  const next = value + ({delta}n);
  if (next < -9223372036854775808n || next > 9223372036854775807n) return {{ok:false, error:"arithmetic-overflow"}};
  if (revision === 18446744073709551615n) return {{ok:false, error:"revision-overflow"}};
  return {{ok:true, value:next, revision:revision + 1n}};
}}"#
    );
    Ok(GeneratedAction {
        action_id: id.clone(),
        state_key: key.clone(),
        javascript,
    })
}
