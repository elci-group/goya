//! Minimal prost representation of the ONNX v1.16.2 protobuf wire schema.
//!
//! Field numbers follow `onnx/onnx.proto` from the ONNX v1.16.2 release.
//! Fields are retained when the importer needs their presence to reject an
//! unsupported model feature; descriptive metadata may be ignored.

use prost::Message;

#[derive(Clone, PartialEq, Message)]
pub(super) struct ModelProto {
    #[prost(int64, optional, tag = "1")]
    pub(super) ir_version: Option<i64>,
    #[prost(message, optional, tag = "7")]
    pub(super) graph: Option<GraphProto>,
    #[prost(message, repeated, tag = "8")]
    pub(super) opset_import: Vec<OperatorSetIdProto>,
    #[prost(bytes = "vec", repeated, tag = "20")]
    pub(super) training_info: Vec<Vec<u8>>,
    #[prost(bytes = "vec", repeated, tag = "25")]
    pub(super) functions: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct GraphProto {
    #[prost(message, repeated, tag = "1")]
    pub(super) node: Vec<NodeProto>,
    #[prost(string, optional, tag = "2")]
    pub(super) name: Option<String>,
    #[prost(message, repeated, tag = "5")]
    pub(super) initializer: Vec<TensorProto>,
    #[prost(message, repeated, tag = "11")]
    pub(super) input: Vec<ValueInfoProto>,
    #[prost(message, repeated, tag = "12")]
    pub(super) output: Vec<ValueInfoProto>,
    #[prost(message, repeated, tag = "13")]
    pub(super) value_info: Vec<ValueInfoProto>,
    #[prost(bytes = "vec", repeated, tag = "14")]
    pub(super) quantization_annotation: Vec<Vec<u8>>,
    #[prost(bytes = "vec", repeated, tag = "15")]
    pub(super) sparse_initializer: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct NodeProto {
    #[prost(string, repeated, tag = "1")]
    pub(super) input: Vec<String>,
    #[prost(string, repeated, tag = "2")]
    pub(super) output: Vec<String>,
    #[prost(string, optional, tag = "3")]
    pub(super) name: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub(super) op_type: Option<String>,
    #[prost(bytes = "vec", repeated, tag = "5")]
    pub(super) attribute: Vec<Vec<u8>>,
    #[prost(string, optional, tag = "7")]
    pub(super) domain: Option<String>,
    #[prost(string, optional, tag = "8")]
    pub(super) overload: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TensorProto {
    #[prost(int64, repeated, packed = "true", tag = "1")]
    pub(super) dims: Vec<i64>,
    #[prost(int32, optional, tag = "2")]
    pub(super) data_type: Option<i32>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub(super) segment: Option<Vec<u8>>,
    #[prost(float, repeated, packed = "true", tag = "4")]
    pub(super) float_data: Vec<f32>,
    #[prost(int32, repeated, packed = "true", tag = "5")]
    pub(super) int32_data: Vec<i32>,
    #[prost(bytes = "vec", repeated, tag = "6")]
    pub(super) string_data: Vec<Vec<u8>>,
    #[prost(int64, repeated, packed = "true", tag = "7")]
    pub(super) int64_data: Vec<i64>,
    #[prost(string, optional, tag = "8")]
    pub(super) name: Option<String>,
    #[prost(bytes = "vec", optional, tag = "9")]
    pub(super) raw_data: Option<Vec<u8>>,
    #[prost(double, repeated, packed = "true", tag = "10")]
    pub(super) double_data: Vec<f64>,
    #[prost(uint64, repeated, packed = "true", tag = "11")]
    pub(super) uint64_data: Vec<u64>,
    #[prost(bytes = "vec", repeated, tag = "13")]
    pub(super) external_data: Vec<Vec<u8>>,
    #[prost(int32, optional, tag = "14")]
    pub(super) data_location: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct ValueInfoProto {
    #[prost(string, optional, tag = "1")]
    pub(super) name: Option<String>,
    #[prost(message, optional, tag = "2")]
    pub(super) r#type: Option<TypeProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TypeProto {
    #[prost(oneof = "type_proto::Value", tags = "1, 4, 5, 8, 9")]
    pub(super) value: Option<type_proto::Value>,
    #[prost(string, optional, tag = "6")]
    pub(super) denotation: Option<String>,
}

pub(super) mod type_proto {
    use super::TensorTypeProto;
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub(in crate::onnx) enum Value {
        #[prost(message, tag = "1")]
        TensorType(TensorTypeProto),
        #[prost(bytes = "vec", tag = "4")]
        SequenceType(Vec<u8>),
        #[prost(bytes = "vec", tag = "5")]
        MapType(Vec<u8>),
        #[prost(bytes = "vec", tag = "8")]
        SparseTensorType(Vec<u8>),
        #[prost(bytes = "vec", tag = "9")]
        OptionalType(Vec<u8>),
    }
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TensorTypeProto {
    #[prost(int32, optional, tag = "1")]
    pub(super) elem_type: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub(super) shape: Option<TensorShapeProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TensorShapeProto {
    #[prost(message, repeated, tag = "1")]
    pub(super) dim: Vec<TensorShapeDimension>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TensorShapeDimension {
    #[prost(oneof = "tensor_shape_dimension::Value", tags = "1, 2")]
    pub(super) value: Option<tensor_shape_dimension::Value>,
    #[prost(string, optional, tag = "3")]
    pub(super) denotation: Option<String>,
}

pub(super) mod tensor_shape_dimension {
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub(in crate::onnx) enum Value {
        #[prost(int64, tag = "1")]
        DimValue(i64),
        #[prost(string, tag = "2")]
        DimParam(String),
    }
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct OperatorSetIdProto {
    #[prost(string, optional, tag = "1")]
    pub(super) domain: Option<String>,
    #[prost(int64, optional, tag = "2")]
    pub(super) version: Option<i64>,
}
