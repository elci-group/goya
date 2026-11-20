//! Versioned, externally pinned local model packages.

use crate::graph::{Graph, Node};
use ie_contract::errors::TypedError;
use ie_contract::tensors::{DType, TensorData, TensorDesc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;

const MAX_PACKAGE_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackage {
    #[serde(rename = "version")]
    _version: u32,
    inputs: Vec<RawInput>,
    nodes: Vec<RawNode>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInput {
    shape: Vec<u64>,
    #[serde(default, deserialize_with = "deserialize_values")]
    values: Option<Vec<f64>>,
}

#[derive(Deserialize)]
struct VersionHeader {
    version: u32,
}

fn deserialize_values<'de, D>(deserializer: D) -> Result<Option<Vec<f64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<f64>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNode {
    op: String,
    left: usize,
    right: usize,
}

/// Parsed graph and optional fixed tensors, bound to the exact package bytes.
#[derive(Debug)]
pub struct ModelPackage {
    digest: [u8; 32],
    graph: Graph,
    fixed_inputs: Vec<Option<TensorData>>,
}

impl ModelPackage {
    /// Read, hash, and parse a package no larger than 1 MiB.
    ///
    /// The expected digest must come from a caller-trusted policy or
    /// configuration source. It is compared before JSON parsing.
    pub fn load(reader: impl Read, expected_sha256: [u8; 32]) -> Result<Self, TypedError> {
        let mut bytes = Vec::new();
        let mut limited = reader.take(MAX_PACKAGE_BYTES + 1);
        limited
            .read_to_end(&mut bytes)
            .map_err(|error| TypedError::Internal {
                reason: format!("package read failed: {error}"),
            })?;
        if bytes.len() as u64 > MAX_PACKAGE_BYTES {
            return Err(TypedError::ResourceExhausted {
                reason: "model package exceeds 1 MiB".into(),
            });
        }
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        if digest != expected_sha256 {
            return Err(TypedError::PolicyDenied {
                reason: "model package SHA-256 does not match trusted pin".into(),
            });
        }

        let header: VersionHeader =
            serde_json::from_slice(&bytes).map_err(|error| TypedError::InvalidTensor {
                reason: format!("invalid model package JSON: {error}"),
            })?;
        if header.version != 1 {
            return Err(TypedError::VersionMismatch {
                local: 1,
                remote: header.version,
            });
        }
        let raw: RawPackage =
            serde_json::from_slice(&bytes).map_err(|error| TypedError::InvalidTensor {
                reason: format!("invalid model package JSON: {error}"),
            })?;

        let mut shapes = Vec::with_capacity(raw.inputs.len());
        let mut fixed_inputs = Vec::with_capacity(raw.inputs.len());
        for (index, input) in raw.inputs.into_iter().enumerate() {
            let desc = TensorDesc {
                dtype: DType::F32,
                dims: input.shape.clone(),
                quant_scale: None,
                quant_zero_point: None,
            };
            let bytes_len = desc.payload_len()?;
            let fixed = if let Some(values) = input.values {
                let expected_count =
                    usize::try_from(bytes_len / 4).map_err(|_| TypedError::ResourceExhausted {
                        reason: format!("fixed input {index} does not fit address space"),
                    })?;
                if values.len() != expected_count {
                    return Err(TypedError::InvalidTensor {
                        reason: format!("fixed input {index} value count does not match shape"),
                    });
                }
                let mut data = Vec::with_capacity(expected_count * 4);
                for value in values {
                    let narrowed = value as f32;
                    if !value.is_finite() || !narrowed.is_finite() {
                        return Err(TypedError::InvalidTensor {
                            reason: format!("fixed input {index} contains a non-finite FP32 value"),
                        });
                    }
                    data.extend_from_slice(&narrowed.to_le_bytes());
                }
                Some(TensorData::new(desc, data)?)
            } else {
                None
            };
            shapes.push(input.shape);
            fixed_inputs.push(fixed);
        }

        let nodes = raw
            .nodes
            .into_iter()
            .map(|node| {
                Ok(Node {
                    op: crate::op_from_name(&node.op)?,
                    left: node.left,
                    right: node.right,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let shape_refs: Vec<&[u64]> = shapes.iter().map(Vec::as_slice).collect();
        let graph = Graph::compile(&shape_refs, &nodes)?;

        Ok(Self {
            digest,
            graph,
            fixed_inputs,
        })
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    pub(crate) fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Merge caller-provided tensors into the declared input slots. Fixed
    /// package tensors occupy their declared positions and consume no caller
    /// input; remaining inputs are checked against the compiled graph.
    pub(crate) fn bind_inputs(
        &self,
        inputs: Vec<TensorData>,
    ) -> Result<Vec<TensorData>, TypedError> {
        let expected_runtime = self
            .fixed_inputs
            .iter()
            .filter(|item| item.is_none())
            .count();
        if inputs.len() != expected_runtime {
            return Err(TypedError::InvalidTensor {
                reason: format!("package requires {expected_runtime} runtime inputs"),
            });
        }
        let mut supplied = inputs.into_iter();
        let bound = self
            .fixed_inputs
            .iter()
            .map(|fixed| match fixed {
                Some(tensor) => Ok(tensor.clone()),
                None => supplied.next().ok_or_else(|| TypedError::Internal {
                    reason: "validated runtime input count changed".into(),
                }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.graph.validate_inputs(&bound)?;
        Ok(bound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_contract::fixture::GoldenFixture;
    use std::io::Cursor;

    fn load(bytes: &[u8]) -> Result<ModelPackage, TypedError> {
        ModelPackage::load(Cursor::new(bytes), Sha256::digest(bytes).into())
    }

    const VALID: &[u8] = br#"{"version":1,"inputs":[{"shape":[1,2]},{"shape":[2,2],"values":[3.0,4.0,5.0,6.0]}],"nodes":[{"op":"matmul","left":0,"right":1}]}"#;

    #[test]
    fn loads_pinned_package_and_binds_fixed_input_slots() {
        let package = load(VALID).unwrap();
        let expected: [u8; 32] = Sha256::digest(VALID).into();
        assert_eq!(package.digest(), expected);
        let bound = package
            .bind_inputs(vec![GoldenFixture::tensor(&[1.0, 2.0], &[1, 2])])
            .unwrap();
        assert_eq!(bound.len(), 2);
        assert_eq!(bound[1].desc.dims, vec![2, 2]);
        assert_eq!(
            bound[1],
            GoldenFixture::tensor(&[3.0, 4.0, 5.0, 6.0], &[2, 2])
        );
        assert!(package.bind_inputs(Vec::new()).is_err());
    }

    #[test]
    fn digest_mismatch_is_rejected_before_parsing() {
        let error = ModelPackage::load(Cursor::new(b"not json"), [0; 32]).unwrap_err();
        assert!(matches!(error, TypedError::PolicyDenied { .. }));
    }

    #[test]
    fn editing_weights_requires_a_new_trusted_pin() {
        let edited = String::from_utf8_lossy(VALID).replacen("3.0", "9.0", 1);
        let original_pin = Sha256::digest(VALID).into();
        assert!(matches!(
            ModelPackage::load(edited.as_bytes(), original_pin),
            Err(TypedError::PolicyDenied { .. })
        ));
        assert_ne!(load(edited.as_bytes()).unwrap().digest(), original_pin);
    }

    #[test]
    fn rejects_unknown_fields_wrong_version_and_unsupported_ops() {
        let unknown = br#"{"version":1,"inputs":[],"nodes":[],"extra":true}"#;
        assert!(matches!(
            load(unknown),
            Err(TypedError::InvalidTensor { .. })
        ));
        let version = br#"{"version":2,"inputs":[],"nodes":[]}"#;
        assert_eq!(
            load(version).unwrap_err(),
            TypedError::VersionMismatch {
                local: 1,
                remote: 2
            }
        );
        let null_values = br#"{"version":1,"inputs":[{"shape":[1],"values":null},{"shape":[1]}],"nodes":[{"op":"add","left":0,"right":1}]}"#;
        assert!(matches!(
            load(null_values),
            Err(TypedError::InvalidTensor { .. })
        ));
        let op = br#"{"version":1,"inputs":[{"shape":[1]},{"shape":[1]}],"nodes":[{"op":"gelu","left":0,"right":1}]}"#;
        assert!(matches!(load(op), Err(TypedError::UnsupportedOp { .. })));
    }

    #[test]
    fn rejects_bad_fixed_values_and_oversized_packages() {
        let count = br#"{"version":1,"inputs":[{"shape":[2],"values":[1.0]}],"nodes":[{"op":"add","left":0,"right":0}]}"#;
        assert!(matches!(load(count), Err(TypedError::InvalidTensor { .. })));
        let nonfinite = br#"{"version":1,"inputs":[{"shape":[1],"values":[1e300]},{"shape":[1]}],"nodes":[{"op":"add","left":0,"right":1}]}"#;
        assert!(matches!(
            load(nonfinite),
            Err(TypedError::InvalidTensor { .. })
        ));
        let oversized = vec![b' '; MAX_PACKAGE_BYTES as usize + 1];
        assert!(matches!(
            load(&oversized),
            Err(TypedError::ResourceExhausted { .. })
        ));
    }
}
