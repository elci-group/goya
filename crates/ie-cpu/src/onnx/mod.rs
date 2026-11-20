//! Strict lowering of the bounded ONNX profile into an immutable package.

mod wire;

use crate::graph::{Graph, Node, MAX_INPUTS, MAX_NODES};
use crate::package::ModelPackage;
use crate::plan_pair;
use ie_contract::errors::TypedError;
use ie_contract::fixture::GoldenOp;
use ie_contract::tensors::{DType, TensorData, TensorDesc};
use prost::Message;
use std::collections::{HashMap, HashSet};

const MIN_IR_VERSION: i64 = 8;
const MAX_IR_VERSION: i64 = 10;
const ONNX_FLOAT: i32 = 1;

fn invalid(reason: impl Into<String>) -> TypedError {
    TypedError::InvalidTensor {
        reason: reason.into(),
    }
}

fn unsupported(reason: impl Into<String>) -> TypedError {
    TypedError::UnsupportedOp { op: reason.into() }
}

/// Import a digest-pinned ONNX model into the local FP32 graph profile.
/// The caller verifies the artifact length and digest before decoding.
pub(crate) fn import(bytes: &[u8], digest: [u8; 32]) -> Result<ModelPackage, TypedError> {
    let model = wire::ModelProto::decode(bytes)
        .map_err(|error| invalid(format!("malformed ONNX protobuf: {error}")))?;
    if !matches!(model.ir_version, Some(v) if (MIN_IR_VERSION..=MAX_IR_VERSION).contains(&v)) {
        return Err(TypedError::VersionMismatch {
            local: MAX_IR_VERSION as u32,
            remote: model
                .ir_version
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
        });
    }
    if !model.training_info.is_empty() {
        return Err(unsupported("ONNX training graphs are unsupported"));
    }
    if !model.functions.is_empty() {
        return Err(unsupported("ONNX functions are unsupported"));
    }
    if model.opset_import.len() != 1 {
        return Err(unsupported(
            "exactly one default-domain opset import is required",
        ));
    }
    let opset = &model.opset_import[0];
    if opset
        .domain
        .as_deref()
        .is_some_and(|domain| !domain.is_empty())
    {
        return Err(unsupported("custom ONNX operator domains are unsupported"));
    }
    if opset.version != Some(13) {
        return Err(TypedError::VersionMismatch {
            local: 13,
            remote: opset
                .version
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0),
        });
    }
    let graph = model
        .graph
        .ok_or_else(|| invalid("ONNX model has no graph"))?;
    if !graph.sparse_initializer.is_empty() {
        return Err(unsupported("sparse ONNX initializers are unsupported"));
    }
    if !graph.quantization_annotation.is_empty() {
        return Err(unsupported("ONNX quantization annotations are unsupported"));
    }
    if graph.node.is_empty() || graph.node.len() > MAX_NODES {
        return Err(invalid(format!(
            "ONNX graph must contain 1–{MAX_NODES} nodes"
        )));
    }
    if graph.output.len() != 1 {
        return Err(invalid("ONNX graph must declare exactly one output"));
    }
    let source_count = graph
        .input
        .len()
        .checked_add(graph.initializer.len())
        .filter(|count| (1..=MAX_INPUTS).contains(count))
        .ok_or_else(|| invalid(format!("ONNX graph must have 1–{MAX_INPUTS} source values")))?;
    let value_count = source_count
        .checked_add(graph.node.len())
        .ok_or_else(|| invalid("ONNX graph value count overflow"))?;
    if graph.value_info.len() > value_count {
        return Err(invalid("ONNX graph has too many value_info entries"));
    }

    let mut descs = Vec::<TensorDesc>::new();
    let mut fixed_inputs = Vec::<Option<TensorData>>::new();
    let mut values = HashMap::<&str, usize>::new();

    for value in &graph.input {
        let name = value_name(value)?;
        if values.contains_key(name) {
            return Err(invalid(format!("duplicate ONNX value name '{name}'")));
        }
        let desc = value_desc(value)?;
        let index = descs.len();
        values.insert(name, index);
        descs.push(desc);
        fixed_inputs.push(None);
    }

    for tensor in &graph.initializer {
        let name = tensor
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid("ONNX initializer has no name"))?;
        if values.contains_key(name) {
            return Err(invalid(format!(
                "initializer '{name}' overlaps an ONNX graph input or value"
            )));
        }
        let fixed = decode_initializer(tensor)?;
        let desc = fixed.desc.clone();
        let index = descs.len();
        values.insert(name, index);
        descs.push(desc);
        fixed_inputs.push(Some(fixed));
    }

    let mut nodes = Vec::with_capacity(graph.node.len());
    for (node_index, raw_node) in graph.node.iter().enumerate() {
        if !raw_node.attribute.is_empty() {
            return Err(unsupported(format!(
                "ONNX node {node_index} attributes are unsupported"
            )));
        }
        if raw_node
            .domain
            .as_deref()
            .is_some_and(|domain| !domain.is_empty())
        {
            return Err(unsupported(format!(
                "ONNX node {node_index} uses a custom domain"
            )));
        }
        if raw_node.overload.as_deref().is_some_and(|s| !s.is_empty()) {
            return Err(unsupported(format!(
                "ONNX node {node_index} overloads are unsupported"
            )));
        }
        if raw_node.input.len() != 2 || raw_node.output.len() != 1 {
            return Err(invalid(format!(
                "ONNX node {node_index} must have two inputs and one output"
            )));
        }
        let op_name = raw_node
            .op_type
            .as_deref()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid(format!("ONNX node {node_index} has no operator")))?;
        let op = match op_name {
            "Add" => GoldenOp::Add,
            "MatMul" => GoldenOp::MatMul,
            other => return Err(unsupported(other)),
        };
        let left_name = &raw_node.input[0];
        let right_name = &raw_node.input[1];
        let left = *values.get(left_name.as_str()).ok_or_else(|| {
            invalid(format!(
                "ONNX node {node_index} has unknown or forward input '{left_name}'"
            ))
        })?;
        let right = *values.get(right_name.as_str()).ok_or_else(|| {
            invalid(format!(
                "ONNX node {node_index} has unknown or forward input '{right_name}'"
            ))
        })?;
        let output_name = raw_node.output[0].as_str();
        if output_name.is_empty() || values.contains_key(output_name) {
            return Err(invalid(format!(
                "ONNX node {node_index} has an empty or duplicate output name"
            )));
        }
        let (output_desc, _) = plan_pair(op, &descs[left], &descs[right])?;
        let output_index = descs.len();
        values.insert(output_name, output_index);
        descs.push(output_desc);
        nodes.push(Node { op, left, right });
    }

    let output = &graph.output[0];
    let output_name = value_name(output)?;
    let final_node_output = graph
        .node
        .last()
        .and_then(|node| node.output.first())
        .map(String::as_str)
        .ok_or_else(|| invalid("ONNX graph has no final node output"))?;
    if output_name != final_node_output {
        return Err(invalid("ONNX graph output must name the final node output"));
    }
    let declared_output = value_desc(output)?;
    let output_index = *values
        .get(output_name)
        .ok_or_else(|| invalid("ONNX graph output is not produced by the graph"))?;
    if declared_output != descs[output_index] {
        return Err(invalid(
            "ONNX graph output shape does not match inferred shape",
        ));
    }

    let mut metadata_names = HashSet::new();
    for info in &graph.value_info {
        let name = value_name(info)?;
        if !metadata_names.insert(name) {
            return Err(invalid(format!("duplicate ONNX value_info name '{name}'")));
        }
        let declared = value_desc(info)?;
        let Some(index) = values.get(name) else {
            return Err(invalid(format!(
                "ONNX value_info names unknown value '{name}'"
            )));
        };
        match descs.get(*index) {
            Some(inferred) if inferred == &declared => {}
            Some(_) => {
                return Err(invalid(format!(
                    "ONNX value_info shape for '{name}' does not match inferred shape"
                )))
            }
            None => {
                return Err(invalid(format!(
                    "ONNX value_info names unknown value '{name}'"
                )))
            }
        }
    }

    let shape_refs: Vec<&[u64]> = descs[..source_count]
        .iter()
        .map(|desc| desc.dims.as_slice())
        .collect();
    let compiled = Graph::compile(&shape_refs, &nodes)?;
    Ok(ModelPackage::from_parts(digest, compiled, fixed_inputs))
}

fn value_name(value: &wire::ValueInfoProto) -> Result<&str, TypedError> {
    value
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("ONNX value has no name"))
}

fn value_desc(value: &wire::ValueInfoProto) -> Result<TensorDesc, TypedError> {
    let ty = value
        .r#type
        .as_ref()
        .ok_or_else(|| invalid("ONNX value has no type"))?;
    let tensor = match ty.value.as_ref() {
        Some(wire::type_proto::Value::Tensor(tensor)) => tensor,
        _ => return Err(unsupported("only ONNX tensor values are supported")),
    };
    if tensor.elem_type != Some(ONNX_FLOAT) {
        return Err(unsupported("only ONNX FP32 tensors are supported"));
    }
    let shape = tensor
        .shape
        .as_ref()
        .ok_or_else(|| invalid("ONNX tensor shape must be static and present"))?;
    if shape.dim.len() > ie_contract::tensors::MAX_RANK {
        return Err(invalid("ONNX tensor rank exceeds the contract limit"));
    }
    let mut dims = Vec::with_capacity(shape.dim.len());
    for dim in &shape.dim {
        let value = match dim.value.as_ref() {
            Some(wire::tensor_shape_dimension::Value::DimValue(value)) => *value,
            _ => return Err(invalid("ONNX tensor dimensions must be static values")),
        };
        dims.push(u64::try_from(value).map_err(|_| invalid("ONNX dimension is not positive"))?);
    }
    let desc = TensorDesc {
        dtype: DType::F32,
        dims,
        quant_scale: None,
        quant_zero_point: None,
    };
    desc.payload_len()?;
    Ok(desc)
}

fn decode_initializer(tensor: &wire::TensorProto) -> Result<TensorData, TypedError> {
    let name = tensor
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("ONNX initializer has no name"))?;
    if tensor.data_type != Some(ONNX_FLOAT) {
        return Err(unsupported(format!("initializer '{name}' is not FP32")));
    }
    if tensor.segment.is_some()
        || !tensor.external_data.is_empty()
        || tensor.data_location.is_some_and(|location| location != 0)
    {
        return Err(unsupported(format!(
            "initializer '{name}' uses external or segmented data"
        )));
    }
    if !tensor.int32_data.is_empty()
        || !tensor.string_data.is_empty()
        || !tensor.int64_data.is_empty()
        || !tensor.double_data.is_empty()
        || !tensor.uint64_data.is_empty()
    {
        return Err(invalid(format!(
            "initializer '{name}' contains non-FP32 payload fields"
        )));
    }
    if tensor.dims.len() > ie_contract::tensors::MAX_RANK {
        return Err(invalid("ONNX initializer rank exceeds the contract limit"));
    }
    let dims = tensor
        .dims
        .iter()
        .map(|dim| {
            u64::try_from(*dim).map_err(|_| invalid("initializer dimension is not positive"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let desc = TensorDesc {
        dtype: DType::F32,
        dims,
        quant_scale: None,
        quant_zero_point: None,
    };
    let byte_len = usize::try_from(desc.payload_len()?)
        .map_err(|_| invalid("initializer does not fit address space"))?;
    let data = match tensor.raw_data.as_ref() {
        Some(raw) if tensor.float_data.is_empty() => {
            if raw.len() != byte_len {
                return Err(invalid(format!(
                    "initializer '{name}' raw_data length does not match its shape"
                )));
            }
            raw.clone()
        }
        None if !tensor.float_data.is_empty() => {
            if tensor.float_data.len().checked_mul(4) != Some(byte_len) {
                return Err(invalid(format!(
                    "initializer '{name}' float_data length does not match its shape"
                )));
            }
            let mut data = Vec::with_capacity(byte_len);
            for value in &tensor.float_data {
                data.extend_from_slice(&value.to_le_bytes());
            }
            data
        }
        _ => {
            return Err(invalid(format!(
                "initializer '{name}' must contain exactly one embedded FP32 payload"
            )))
        }
    };
    if data
        .chunks_exact(4)
        .any(|chunk| !f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]).is_finite())
    {
        return Err(invalid(format!(
            "initializer '{name}' contains non-finite data"
        )));
    }
    TensorData::new(desc, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onnx::wire::{
        tensor_shape_dimension, type_proto, GraphProto, ModelProto, NodeProto, OperatorSetIdProto,
        TensorProto, TensorShapeDimension, TensorShapeProto, TensorTypeProto, TypeProto,
        ValueInfoProto,
    };
    use ie_contract::fixture::GoldenFixture;

    fn value(name: &str, dims: &[i64]) -> ValueInfoProto {
        ValueInfoProto {
            name: Some(name.into()),
            r#type: Some(TypeProto {
                value: Some(type_proto::Value::Tensor(TensorTypeProto {
                    elem_type: Some(ONNX_FLOAT),
                    shape: Some(TensorShapeProto {
                        dim: dims
                            .iter()
                            .map(|dim| TensorShapeDimension {
                                value: Some(tensor_shape_dimension::Value::DimValue(*dim)),
                                denotation: None,
                            })
                            .collect(),
                    }),
                })),
                denotation: None,
            }),
        }
    }

    fn valid_model() -> ModelProto {
        ModelProto {
            ir_version: Some(10),
            graph: Some(GraphProto {
                node: vec![NodeProto {
                    input: vec!["x".into(), "weight".into()],
                    output: vec!["y".into()],
                    name: Some("add".into()),
                    op_type: Some("Add".into()),
                    attribute: Vec::new(),
                    domain: None,
                    overload: None,
                }],
                name: Some("fixed-weight-add".into()),
                initializer: vec![TensorProto {
                    dims: vec![2],
                    data_type: Some(ONNX_FLOAT),
                    segment: None,
                    float_data: vec![3.0, 4.0],
                    int32_data: Vec::new(),
                    string_data: Vec::new(),
                    int64_data: Vec::new(),
                    name: Some("weight".into()),
                    raw_data: None,
                    double_data: Vec::new(),
                    uint64_data: Vec::new(),
                    external_data: Vec::new(),
                    data_location: None,
                }],
                input: vec![value("x", &[2])],
                output: vec![value("y", &[2])],
                value_info: Vec::new(),
                quantization_annotation: Vec::new(),
                sparse_initializer: Vec::new(),
            }),
            opset_import: vec![OperatorSetIdProto {
                domain: None,
                version: Some(13),
            }],
            training_info: Vec::new(),
            functions: Vec::new(),
        }
    }

    fn import_model(model: &ModelProto) -> Result<ModelPackage, TypedError> {
        import(&model.encode_to_vec(), [0x5a; 32])
    }

    #[test]
    fn rejects_other_ir_versions_opsets_and_operators() {
        let mut model = valid_model();
        model.ir_version = Some(11);
        assert!(matches!(
            import_model(&model),
            Err(TypedError::VersionMismatch {
                local: 10,
                remote: 11
            })
        ));

        model.ir_version = Some(10);
        model.opset_import[0].version = Some(12);
        assert!(matches!(
            import_model(&model),
            Err(TypedError::VersionMismatch {
                local: 13,
                remote: 12
            })
        ));

        model.opset_import[0].version = Some(13);
        model.graph.as_mut().unwrap().node[0].op_type = Some("Relu".into());
        assert!(matches!(
            import_model(&model),
            Err(TypedError::UnsupportedOp { op }) if op == "Relu"
        ));
    }

    #[test]
    fn imports_pinned_embedded_initializer_and_executes_graph() {
        let package = import_model(&valid_model()).unwrap();
        assert_eq!(package.digest(), [0x5a; 32]);
        let inputs = package
            .bind_inputs(vec![GoldenFixture::tensor(&[1.0, 2.0], &[2])])
            .unwrap();
        let output = package.graph().execute(inputs, || Ok(())).unwrap();
        let got: Vec<f32> = output
            .data
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(got, vec![4.0, 6.0]);
    }

    #[test]
    fn imports_raw_data_and_allows_one_source_used_twice() {
        let mut model = valid_model();
        let graph = model.graph.as_mut().unwrap();
        graph.input[0] = value("x", &[2]);
        graph.initializer.clear();
        graph.node[0].input = vec!["x".into(), "x".into()];
        graph.output[0] = value("y", &[2]);
        let package = import_model(&model).unwrap();
        let inputs = package
            .bind_inputs(vec![GoldenFixture::tensor(&[2.0, 3.0], &[2])])
            .unwrap();
        let output = package.graph().execute(inputs, || Ok(())).unwrap();
        let got: Vec<f32> = output
            .data
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(got, vec![4.0, 6.0]);

        let mut model = valid_model();
        let initializer = &mut model.graph.as_mut().unwrap().initializer[0];
        initializer.float_data.clear();
        initializer.raw_data = Some([3.0f32.to_le_bytes(), 4.0f32.to_le_bytes()].concat());
        assert!(import_model(&model).is_ok());
    }

    #[test]
    fn rejects_forward_names_attributes_and_wrong_output_shape() {
        let mut model = valid_model();
        model.graph.as_mut().unwrap().node[0].input[0] = "later".into();
        assert!(matches!(
            import_model(&model),
            Err(TypedError::InvalidTensor { .. })
        ));

        let mut model = valid_model();
        model.graph.as_mut().unwrap().node[0]
            .attribute
            .push(vec![1]);
        assert!(matches!(
            import_model(&model),
            Err(TypedError::UnsupportedOp { .. })
        ));

        let mut model = valid_model();
        model.graph.as_mut().unwrap().output[0] = value("y", &[3]);
        assert!(matches!(
            import_model(&model),
            Err(TypedError::InvalidTensor { .. })
        ));
    }

    #[test]
    fn rejects_initializer_input_overlap_and_external_payloads() {
        let mut model = valid_model();
        model.graph.as_mut().unwrap().initializer[0].name = Some("x".into());
        assert!(matches!(
            import_model(&model),
            Err(TypedError::InvalidTensor { .. })
        ));

        let mut model = valid_model();
        model.graph.as_mut().unwrap().initializer[0]
            .external_data
            .push(vec![1]);
        model.graph.as_mut().unwrap().initializer[0].data_location = Some(1);
        assert!(matches!(
            import_model(&model),
            Err(TypedError::UnsupportedOp { .. })
        ));
    }

    #[test]
    fn rejects_custom_domains_and_dynamic_shapes() {
        let mut model = valid_model();
        model.graph.as_mut().unwrap().node[0].domain = Some("custom".into());
        assert!(matches!(
            import_model(&model),
            Err(TypedError::UnsupportedOp { .. })
        ));

        let mut model = valid_model();
        let x = &mut model.graph.as_mut().unwrap().input[0];
        let ty = x.r#type.as_mut().unwrap();
        let tensor = match ty.value.as_mut().unwrap() {
            type_proto::Value::Tensor(tensor) => tensor,
            _ => unreachable!(),
        };
        tensor.shape.as_mut().unwrap().dim[0].value =
            Some(tensor_shape_dimension::Value::DimParam("n".into()));
        assert!(matches!(
            import_model(&model),
            Err(TypedError::InvalidTensor { .. })
        ));
    }
}
