//! Statically shaped FP32 graphs for the local Iberia runtime.

use ie_contract::errors::TypedError;
use ie_contract::fixture::GoldenOp;
use ie_contract::tensors::{DType, TensorData, TensorDesc};
use sha2::{Digest, Sha256};

use crate::{execute_pair, plan_pair};

const MAX_INPUTS: usize = 16;
const MAX_NODES: usize = 32;

/// An instruction referring to earlier values. Inputs occupy the first
/// indices, followed by node outputs in execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub op: GoldenOp,
    pub left: usize,
    pub right: usize,
}

/// An immutable execution plan. The final node supplies the single output.
#[derive(Clone, Debug)]
pub struct Graph {
    inputs: Vec<TensorDesc>,
    nodes: Vec<Node>,
    digest: [u8; 32],
    payload_bytes: u64,
    work: u64,
}

impl Graph {
    /// Validate a graph's shapes, topology, and resource requirements.
    ///
    /// The digest pins the FP32 input shapes, instructions, and value
    /// references. Tensor values, including weights supplied as inputs,
    /// belong to a request and are outside this digest.
    pub fn compile(input_shapes: &[&[u64]], nodes: &[Node]) -> Result<Self, TypedError> {
        if !(2..=MAX_INPUTS).contains(&input_shapes.len())
            || !(1..=MAX_NODES).contains(&nodes.len())
        {
            return Err(TypedError::ResourceExhausted {
                reason: format!("a graph requires 2–{MAX_INPUTS} inputs and 1–{MAX_NODES} nodes"),
            });
        }

        let mut values = Vec::with_capacity(input_shapes.len() + nodes.len());
        let mut hasher = Sha256::new();
        hasher.update(b"iberia/local-fp32-graph/v1\0");
        hasher.update([input_shapes.len() as u8, nodes.len() as u8]);
        let mut payload_bytes = 0;
        let mut work = 0u64;

        for &shape in input_shapes {
            // Check before copying the caller's shape into the plan.
            if shape.len() > ie_contract::tensors::MAX_RANK {
                return Err(TypedError::InvalidTensor {
                    reason: "graph input rank exceeds the FP32 contract limit".into(),
                });
            }
            let desc = TensorDesc {
                dtype: DType::F32,
                dims: shape.to_vec(),
                quant_scale: None,
                quant_zero_point: None,
            };
            payload_bytes += desc.payload_len()?;
            hasher.update([shape.len() as u8]);
            for dim in shape {
                hasher.update(dim.to_le_bytes());
            }
            values.push(desc);
        }

        for (index, node) in nodes.iter().enumerate() {
            let (Some(left), Some(right)) = (values.get(node.left), values.get(node.right)) else {
                return Err(TypedError::InvalidTensor {
                    reason: format!("graph node {index} must reference preceding values"),
                });
            };
            let (output, node_work) = plan_pair(node.op, left, right)?;
            payload_bytes += output.payload_len()?;
            work = work
                .checked_add(node_work)
                .ok_or_else(|| TypedError::ResourceExhausted {
                    reason: "graph work estimate overflow".into(),
                })?;
            let op_code = match node.op {
                GoldenOp::Add => 1,
                GoldenOp::MatMul => 2,
            };
            hasher.update([op_code, node.left as u8, node.right as u8]);
            values.push(output);
        }

        values.truncate(input_shapes.len());
        Ok(Self {
            inputs: values,
            nodes: nodes.to_vec(),
            digest: hasher.finalize().into(),
            payload_bytes,
            work,
        })
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    pub(crate) fn payload_bytes(&self) -> u64 {
        self.payload_bytes
    }

    pub(crate) fn work(&self) -> u64 {
        self.work
    }

    pub(crate) fn validate_inputs(&self, inputs: &[TensorData]) -> Result<(), TypedError> {
        if inputs.len() != self.inputs.len() {
            return Err(TypedError::InvalidTensor {
                reason: format!("expected {} graph inputs", self.inputs.len()),
            });
        }
        for (index, (input, expected)) in inputs.iter().zip(&self.inputs).enumerate() {
            if &input.desc != expected {
                return Err(TypedError::InvalidTensor {
                    reason: format!("input {index} does not match the model descriptor"),
                });
            }
            expected.validate(input.data.len())?;
        }
        Ok(())
    }

    pub(crate) fn execute(
        &self,
        mut inputs: Vec<TensorData>,
        mut checkpoint: impl FnMut() -> Result<(), TypedError>,
    ) -> Result<TensorData, TypedError> {
        checkpoint()?;
        inputs
            .try_reserve_exact(self.nodes.len())
            .map_err(|_| TypedError::ResourceExhausted {
                reason: "graph value allocation failed".into(),
            })?;
        for node in &self.nodes {
            checkpoint()?;
            // Compilation guarantees that both values already exist.
            let output = execute_pair(
                node.op,
                &inputs[node.left],
                &inputs[node.right],
                &mut checkpoint,
            )?;
            inputs.push(output);
        }
        checkpoint()?;
        inputs.pop().ok_or_else(|| TypedError::Internal {
            reason: "graph has no output".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_contract::fixture::GoldenFixture;

    #[test]
    fn graph_digest_pins_operator_and_shape() {
        let node = Node {
            op: GoldenOp::Add,
            left: 0,
            right: 1,
        };
        let shapes: &[&[u64]] = &[&[2, 2], &[2, 2]];
        let digest = Graph::compile(shapes, &[node]).unwrap().digest();
        assert_eq!(
            digest,
            [
                0xd0, 0x53, 0xc0, 0x4e, 0x79, 0x52, 0x64, 0xd3, 0xa8, 0x39, 0x87, 0x33, 0xfc, 0xb0,
                0x2d, 0xc7, 0x6e, 0x6c, 0xb7, 0x5d, 0xba, 0xfe, 0xe1, 0x08, 0xbc, 0x3e, 0x71, 0xe0,
                0xf9, 0x6c, 0x47, 0x14,
            ]
        );
        assert_eq!(Graph::compile(shapes, &[node]).unwrap().digest(), digest);
        assert_ne!(
            Graph::compile(&[&[4], &[4]], &[node]).unwrap().digest(),
            digest
        );
        let other = Graph::compile(
            shapes,
            &[Node {
                op: GoldenOp::MatMul,
                ..node
            }],
        )
        .unwrap();
        assert_ne!(other.digest(), digest);
    }

    #[test]
    fn graph_rejects_forward_reference_and_malformed_inputs() {
        let node = Node {
            op: GoldenOp::Add,
            left: 0,
            right: 2,
        };
        assert!(matches!(
            Graph::compile(&[&[2], &[2]], &[node]),
            Err(TypedError::InvalidTensor { .. })
        ));
        let graph = Graph::compile(&[&[2], &[2]], &[Node { right: 1, ..node }]).unwrap();
        let mut bad = GoldenFixture::tensor(&[1.0, 2.0], &[2]);
        bad.data.pop();
        let valid = GoldenFixture::tensor(&[3.0, 4.0], &[2]);
        assert!(matches!(
            graph.validate_inputs(&[bad, valid]),
            Err(TypedError::InvalidTensor { .. })
        ));
    }
}
