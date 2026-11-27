//! GVI-A reconstruction engine contract (directive §12.3).
//!
//! An Albit is a generative reconstruction: a named operation plus
//! parameters applied to a base payload. The engine is the registry of
//! operations; the derivation engine verifies each candidate's output
//! against the real canonical payload and certifies it:
//!
//! - byte-identical → **exact** Albit (`ProvenanceRelation::Albit { exact: true }`).
//!   An operation is never certified exact because its output *appears*
//!   similar (§12.3, AVR-C09).
//! - within the operation's declared error bound and the profile's
//!   fidelity policy → **bounded** Albit (AVR-C10).
//! - anything else → not certified; the observation keeps its raw or
//!   differential encoding.
//!
//! The engine is deliberately absent from the artifact format: an Albit
//! entry names the operation, and resolution requires an engine that
//! provides it. That is the point of generative reconstruction — the
//! payload is computed, not stored — so a resolution without the engine
//! is an explicit failure, never a silent substitution.

use crate::domain::ObservationState;
use crate::ingestion::ObservationPayload;

/// Outcome of applying one reconstruction operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpOutcome {
    pub payload: ObservationPayload,
    /// Declared upper bound on the operation's error, in the operation's
    /// own metric. `None` declares the operation exact under its contract.
    pub declared_error: Option<u32>,
}

/// Why an operation could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// The operation name is not registered with this engine.
    UnknownOperation(String),
    /// The parameters are invalid for the operation.
    InvalidParameters(String),
    /// The operation failed while computing.
    Failed(String),
}

/// Registry of generative reconstruction operations (GVI-A).
///
/// Implementations must be deterministic: the same op, params and base
/// always produce the same outcome (§7.3).
pub trait ReconstructionEngine {
    /// Candidate `(op, params)` reconstructions of the observation whose
    /// state is `target_state`, given the base observation's state.
    /// Order is engine-defined but must be deterministic.
    fn candidates(
        &self,
        target_state: &ObservationState,
        base_state: &ObservationState,
    ) -> Vec<(String, Vec<u8>)>;

    /// Apply one operation to a base payload.
    fn apply(&self, op: &str, params: &[u8], base: &ObservationPayload) -> Result<OpOutcome, EngineError>;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::representation::RepresentationClass;

    /// Test engine: one parametric exact op (`reconstruct` regenerates an
    /// `OBS <az> <el>` payload from its quantised state — byte-exact when
    /// verified), one non-verifiable nominal op (`uppercases` the payload,
    /// never byte-identical to a different target), and one approximate op
    /// (`truncate`, declaring the removed length as error).
    pub(crate) struct TestEngine;

    impl ReconstructionEngine for TestEngine {
        fn candidates(
            &self,
            target_state: &ObservationState,
            _base_state: &ObservationState,
        ) -> Vec<(String, Vec<u8>)> {
            let reconstruct = format!(
                "{} {}",
                target_state.orientation[0], target_state.orientation[1]
            )
            .into_bytes();
            vec![
                ("reconstruct".into(), reconstruct),
                ("uppercase".into(), Vec::new()),
                ("truncate".into(), vec![8]),
            ]
        }

        fn apply(&self, op: &str, params: &[u8], base: &ObservationPayload) -> Result<OpOutcome, EngineError> {
            match op {
                "reconstruct" => {
                    let text = std::str::from_utf8(params)
                        .map_err(|_| EngineError::InvalidParameters("utf8".into()))?;
                    let mut parts = text.split(' ');
                    let az: i64 = parts
                        .next()
                        .and_then(|p| p.parse().ok())
                        .ok_or_else(|| EngineError::InvalidParameters("azimuth".into()))?;
                    let el: i64 = parts
                        .next()
                        .and_then(|p| p.parse().ok())
                        .ok_or_else(|| EngineError::InvalidParameters("elevation".into()))?;
                    Ok(OpOutcome {
                        payload: ObservationPayload {
                            representation: base.representation,
                            bytes: format!("OBS {az} {el} dup {az} {el}").into_bytes(),
                        },
                        declared_error: None,
                    })
                }
                "uppercase" => Ok(OpOutcome {
                    payload: ObservationPayload {
                        representation: base.representation,
                        bytes: base.bytes.to_ascii_uppercase(),
                    },
                    declared_error: None,
                }),
                "truncate" => {
                    let keep = *params.first().unwrap_or(&0) as usize;
                    if keep > base.bytes.len() {
                        return Err(EngineError::InvalidParameters("keep > len".into()));
                    }
                    Ok(OpOutcome {
                        payload: ObservationPayload {
                            representation: base.representation,
                            bytes: base.bytes[..keep].to_vec(),
                        },
                        declared_error: Some((base.bytes.len() - keep) as u32),
                    })
                }
                other => Err(EngineError::UnknownOperation(other.into())),
            }
        }
    }

    #[test]
    fn engine_ops_are_deterministic() {
        let base = ObservationPayload {
            representation: RepresentationClass::Vec,
            bytes: b"<svg/>".to_vec(),
        };
        let a = TestEngine.apply("uppercase", &[], &base).expect("apply");
        let b = TestEngine.apply("uppercase", &[], &base).expect("apply");
        assert_eq!(a, b);
        assert_eq!(a.declared_error, None, "exact ops declare no error bound");
    }
}
