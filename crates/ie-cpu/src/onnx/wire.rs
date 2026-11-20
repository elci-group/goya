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
    #[prost(string, optional, tag = "2")]
    pub(super) producer_name: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub(super) producer_version: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub(super) domain: Option<String>,
    #[prost(int64, optional, tag = "5")]
    pub(super) model_version: Option<i64>,
    #[prost(string, optional, tag = "6")]
    pub(super) doc_string: Option<String>,
    #[prost(message, optional, tag = "7")]
    pub(super) graph: Option<GraphProto>,
    #[prost(message, repeated, tag = "8")]
    pub(super) opset_import: Vec<OperatorSetIdProto>,
    #[prost(message, repeated, tag = "14")]
    pub(super) metadata_props: Vec<StringStringEntryProto>,
    #[prost(message, repeated, tag = "20")]
    pub(super) training_info: Vec<TrainingInfoProto>,
    #[prost(message, repeated, tag = "25")]
    pub(super) functions: Vec<FunctionProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct GraphProto {
    #[prost(message, repeated, tag = "1")]
    pub(super) node: Vec<NodeProto>,
    #[prost(string, optional, tag = "2")]
    pub(super) name: Option<String>,
    #[prost(message, repeated, tag = "5")]
    pub(super) initializer: Vec<TensorProto>,
    #[prost(string, optional, tag = "10")]
    pub(super) doc_string: Option<String>,
    #[prost(message, repeated, tag = "11")]
    pub(super) input: Vec<ValueInfoProto>,
    #[prost(message, repeated, tag = "12")]
    pub(super) output: Vec<ValueInfoProto>,
    #[prost(message, repeated, tag = "13")]
    pub(super) value_info: Vec<ValueInfoProto>,
    #[prost(message, repeated, tag = "14")]
    pub(super) quantization_annotation: Vec<TensorAnnotation>,
    #[prost(message, repeated, tag = "15")]
    pub(super) sparse_initializer: Vec<SparseTensorProto>,
    #[prost(message, repeated, tag = "16")]
    pub(super) metadata_props: Vec<StringStringEntryProto>,
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
    #[prost(message, repeated, tag = "5")]
    pub(super) attribute: Vec<AttributeProto>,
    #[prost(string, optional, tag = "6")]
    pub(super) doc_string: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub(super) domain: Option<String>,
    #[prost(string, optional, tag = "8")]
    pub(super) overload: Option<String>,
    #[prost(message, repeated, tag = "9")]
    pub(super) metadata_props: Vec<StringStringEntryProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TensorProto {
    #[prost(int64, repeated, packed = "true", tag = "1")]
    pub(super) dims: Vec<i64>,
    #[prost(int32, optional, tag = "2")]
    pub(super) data_type: Option<i32>,
    #[prost(message, optional, tag = "3")]
    pub(super) segment: Option<TensorSegment>,
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
    #[prost(string, optional, tag = "12")]
    pub(super) doc_string: Option<String>,
    #[prost(message, repeated, tag = "13")]
    pub(super) external_data: Vec<StringStringEntryProto>,
    #[prost(enumeration = "DataLocation", optional, tag = "14")]
    pub(super) data_location: Option<i32>,
    #[prost(message, repeated, tag = "16")]
    pub(super) metadata_props: Vec<StringStringEntryProto>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub(super) enum DataLocation {
    Default = 0,
    External = 1,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TensorSegment {
    #[prost(int64, optional, tag = "1")]
    pub(super) begin: Option<i64>,
    #[prost(int64, optional, tag = "2")]
    pub(super) end: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct ValueInfoProto {
    #[prost(string, optional, tag = "1")]
    pub(super) name: Option<String>,
    #[prost(message, optional, tag = "2")]
    pub(super) r#type: Option<TypeProto>,
    #[prost(string, optional, tag = "3")]
    pub(super) doc_string: Option<String>,
    #[prost(message, repeated, tag = "4")]
    pub(super) metadata_props: Vec<StringStringEntryProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TypeProto {
    #[prost(oneof = "type_proto::Value", tags = "1, 4, 5, 8, 9")]
    pub(super) value: Option<type_proto::Value>,
    #[prost(string, optional, tag = "6")]
    pub(super) denotation: Option<String>,
}

pub(super) mod type_proto {
    use super::{
        MapTypeProto, OptionalTypeProto, SequenceTypeProto, SparseTensorTypeProto, TensorTypeProto,
    };
    use prost::Oneof;

    #[derive(Clone, PartialEq, Oneof)]
    pub(super) enum Value {
        #[prost(message, tag = "1")]
        TensorType(TensorTypeProto),
        #[prost(message, tag = "4")]
        SequenceType(SequenceTypeProto),
        #[prost(message, tag = "5")]
        MapType(MapTypeProto),
        #[prost(message, tag = "8")]
        SparseTensorType(SparseTensorTypeProto),
        #[prost(message, tag = "9")]
        OptionalType(OptionalTypeProto),
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
    pub(super) enum Value {
        #[prost(int64, tag = "1")]
        DimValue(i64),
        #[prost(string, tag = "2")]
        DimParam(String),
    }
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct SequenceTypeProto {
    #[prost(message, optional, tag = "1")]
    pub(super) elem_type: Option<TypeProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct MapTypeProto {
    #[prost(int32, optional, tag = "1")]
    pub(super) key_type: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub(super) value_type: Option<TypeProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct OptionalTypeProto {
    #[prost(message, optional, tag = "1")]
    pub(super) elem_type: Option<TypeProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct SparseTensorTypeProto {
    #[prost(int32, optional, tag = "1")]
    pub(super) elem_type: Option<i32>,
    #[prost(message, optional, tag = "2")]
    pub(super) shape: Option<TensorShapeProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct OperatorSetIdProto {
    #[prost(string, optional, tag = "1")]
    pub(super) domain: Option<String>,
    #[prost(int64, optional, tag = "2")]
    pub(super) version: Option<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TrainingInfoProto {
    #[prost(message, optional, tag = "1")]
    pub(super) initialization: Option<GraphProto>,
    #[prost(message, optional, tag = "2")]
    pub(super) algorithm: Option<GraphProto>,
    #[prost(message, repeated, tag = "3")]
    pub(super) initialization_binding: Vec<StringStringEntryProto>,
    #[prost(message, repeated, tag = "4")]
    pub(super) update_binding: Vec<StringStringEntryProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct AttributeProto {
    #[prost(string, optional, tag = "1")]
    pub(super) name: Option<String>,
    #[prost(float, optional, tag = "2")]
    pub(super) f: Option<f32>,
    #[prost(int64, optional, tag = "3")]
    pub(super) i: Option<i64>,
    #[prost(bytes = "vec", optional, tag = "4")]
    pub(super) s: Option<Vec<u8>>,
    #[prost(message, optional, tag = "5")]
    pub(super) t: Option<TensorProto>,
    #[prost(message, optional, tag = "6")]
    pub(super) g: Option<GraphProto>,
    #[prost(float, repeated, packed = "true", tag = "7")]
    pub(super) floats: Vec<f32>,
    #[prost(int64, repeated, packed = "true", tag = "8")]
    pub(super) ints: Vec<i64>,
    #[prost(bytes = "vec", repeated, tag = "9")]
    pub(super) strings: Vec<Vec<u8>>,
    #[prost(message, repeated, tag = "10")]
    pub(super) tensors: Vec<TensorProto>,
    #[prost(message, repeated, tag = "11")]
    pub(super) graphs: Vec<GraphProto>,
    #[prost(string, optional, tag = "13")]
    pub(super) doc_string: Option<String>,
    #[prost(message, optional, tag = "14")]
    pub(super) tp: Option<TypeProto>,
    #[prost(message, repeated, tag = "15")]
    pub(super) type_protos: Vec<TypeProto>,
    #[prost(enumeration = "AttributeType", optional, tag = "20")]
    pub(super) r#type: Option<i32>,
    #[prost(string, optional, tag = "21")]
    pub(super) ref_attr_name: Option<String>,
    #[prost(message, optional, tag = "22")]
    pub(super) sparse_tensor: Option<SparseTensorProto>,
    #[prost(message, repeated, tag = "23")]
    pub(super) sparse_tensors: Vec<SparseTensorProto>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub(super) enum AttributeType {
    Undefined = 0,
    Float = 1,
    Int = 2,
    String = 3,
    Tensor = 4,
    Graph = 5,
    Floats = 6,
    Ints = 7,
    Strings = 8,
    Tensors = 9,
    Graphs = 10,
    SparseTensor = 11,
    SparseTensors = 12,
    TypeProto = 13,
    TypeProtos = 14,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct SparseTensorProto {
    #[prost(message, optional, tag = "1")]
    pub(super) values: Option<TensorProto>,
    #[prost(message, optional, tag = "2")]
    pub(super) indices: Option<TensorProto>,
    #[prost(int64, repeated, packed = "true", tag = "3")]
    pub(super) dims: Vec<i64>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct TensorAnnotation {
    #[prost(string, optional, tag = "1")]
    pub(super) tensor_name: Option<String>,
    #[prost(message, repeated, tag = "2")]
    pub(super) quant_parameter_tensor_names: Vec<StringStringEntryProto>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct StringStringEntryProto {
    #[prost(string, optional, tag = "1")]
    pub(super) key: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub(super) value: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct FunctionProto {
    #[prost(string, optional, tag = "1")]
    pub(super) name: Option<String>,
    #[prost(string, repeated, tag = "4")]
    pub(super) input: Vec<String>,
    #[prost(string, repeated, tag = "5")]
    pub(super) output: Vec<String>,
    #[prost(string, repeated, tag = "6")]
    pub(super) attribute: Vec<String>,
    #[prost(message, repeated, tag = "11")]
    pub(super) attribute_proto: Vec<AttributeProto>,
    #[prost(message, repeated, tag = "7")]
    pub(super) node: Vec<NodeProto>,
    #[prost(string, optional, tag = "8")]
    pub(super) doc_string: Option<String>,
    #[prost(message, repeated, tag = "9")]
    pub(super) opset_import: Vec<OperatorSetIdProto>,
    #[prost(string, optional, tag = "10")]
    pub(super) domain: Option<String>,
    #[prost(string, optional, tag = "13")]
    pub(super) overload: Option<String>,
    #[prost(message, repeated, tag = "12")]
    pub(super) value_info: Vec<ValueInfoProto>,
    #[prost(message, repeated, tag = "14")]
    pub(super) metadata_props: Vec<StringStringEntryProto>,
}
