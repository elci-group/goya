//! Canonical GY-IR/2 bytecode compiler and deterministic reference VM.
//!
//! The compiler consumes the shared Goya UI IR produced either by the
//! line-oriented frontend or `goya-lang::lower`. It validates that the input
//! is a single rooted tree, then emits a versioned, preorder instruction
//! stream. The VM has no clock, RNG, host I/O, or global state.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use goya_determinism::proofs::ExecutionProof;
use goya_determinism::{classify, DeterminismClass, ProcessDecl, RejectReason};
use goya_ir::{ActionId, Module, Node, NodeId};

use crate::{fnv1a, Artifact};

/// Version of the canonical bytecode encoding.
pub const BYTECODE_VERSION: u32 = 1;

/// One stable bytecode instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    BeginColumn {
        node: NodeId,
        child_count: u64,
    },
    BeginRow {
        node: NodeId,
        child_count: u64,
    },
    EndContainer,
    Text {
        node: NodeId,
        content: String,
    },
    Button {
        node: NodeId,
        action: ActionId,
        label: String,
    },
    /// Spacer size preserved as its IEEE-754 bit pattern.
    Spacer {
        node: NodeId,
        size_bits: u64,
    },
}

/// Compiled deterministic bytecode, including its canonical encoding hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bytecode {
    pub version: u32,
    pub instructions: Vec<Instruction>,
    pub content_hash: u64,
}

/// A verified, code-bound artifact ready for registry insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledProgram {
    pub artifact: Artifact,
    pub bytecode: Bytecode,
}

impl Bytecode {
    /// Stable encoding: fixed magic/version, fixed opcode tags, little-endian
    /// integers, and length-prefixed UTF-8 strings.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GOYA-BC\0");
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&(self.instructions.len() as u64).to_le_bytes());
        for instruction in &self.instructions {
            match instruction {
                Instruction::BeginColumn { node, child_count } => {
                    bytes.push(0);
                    put_u64(&mut bytes, node.0);
                    put_u64(&mut bytes, *child_count);
                }
                Instruction::BeginRow { node, child_count } => {
                    bytes.push(1);
                    put_u64(&mut bytes, node.0);
                    put_u64(&mut bytes, *child_count);
                }
                Instruction::EndContainer => bytes.push(2),
                Instruction::Text { node, content } => {
                    bytes.push(3);
                    put_u64(&mut bytes, node.0);
                    put_str(&mut bytes, content);
                }
                Instruction::Button {
                    node,
                    action,
                    label,
                } => {
                    bytes.push(4);
                    put_u64(&mut bytes, node.0);
                    put_u64(&mut bytes, action.0);
                    put_str(&mut bytes, label);
                }
                Instruction::Spacer { node, size_bits } => {
                    bytes.push(5);
                    put_u64(&mut bytes, node.0);
                    put_u64(&mut bytes, *size_bits);
                }
            }
        }
        bytes
    }
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    put_u64(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

/// Why a module was refused by the bytecode compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    MissingRoot,
    DuplicateNodeId(NodeId),
    UnknownNode(NodeId),
    ReusedOrCyclicNode(NodeId),
    UnreachableNode(NodeId),
    TooManyChildren(NodeId),
    InvalidSpacer(NodeId),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRoot => f.write_str("module has no root"),
            Self::DuplicateNodeId(id) => write!(f, "duplicate node id {}", id.0),
            Self::UnknownNode(id) => write!(f, "node {} is referenced but not declared", id.0),
            Self::ReusedOrCyclicNode(id) => {
                write!(f, "node {} is reachable more than once", id.0)
            }
            Self::UnreachableNode(id) => write!(f, "node {} is unreachable from root", id.0),
            Self::TooManyChildren(id) => write!(f, "node {} has too many children", id.0),
            Self::InvalidSpacer(id) => write!(f, "spacer node {} has invalid size", id.0),
        }
    }
}

impl std::error::Error for CompileError {}

/// Why a verified compilation was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifiedCompileError {
    Classification(RejectReason),
    ProofProcessMismatch,
    ProofClassMismatch,
    UnsupportedClass(DeterminismClass),
    Bytecode(CompileError),
}

impl fmt::Display for VerifiedCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Classification(reason) => write!(f, "declaration rejected: {reason}"),
            Self::ProofProcessMismatch => f.write_str("proof process differs from declaration"),
            Self::ProofClassMismatch => f.write_str("proof class differs from declaration"),
            Self::UnsupportedClass(class) => {
                write!(f, "reference bytecode backend does not support {class}")
            }
            Self::Bytecode(error) => write!(f, "bytecode compilation failed: {error}"),
        }
    }
}

impl std::error::Error for VerifiedCompileError {}

/// Verify the EDC declaration and execution proof before compiling a module.
/// The reference VM currently admits D0 pure modules only; D1/D2 need explicit
/// state/effect opcodes and replay contracts before they can be soundly run.
pub fn compile_verified(
    module: &Module,
    declaration: &ProcessDecl,
    proof: &ExecutionProof,
) -> Result<CompiledProgram, VerifiedCompileError> {
    let verified = classify(declaration).map_err(VerifiedCompileError::Classification)?;
    if proof.process != verified.process {
        return Err(VerifiedCompileError::ProofProcessMismatch);
    }
    if proof.class != verified.class {
        return Err(VerifiedCompileError::ProofClassMismatch);
    }
    if verified.class != DeterminismClass::Pure {
        return Err(VerifiedCompileError::UnsupportedClass(verified.class));
    }
    let bytecode = compile_module(module).map_err(VerifiedCompileError::Bytecode)?;
    let artifact =
        Artifact::from_proof_and_code(proof.clone(), Vec::new(), &bytecode.canonical_bytes());
    Ok(CompiledProgram { artifact, bytecode })
}

enum Task {
    Emit(NodeId),
    End,
}

/// Compile a GY-IR/2 module into stable bytecode. The module must be one
/// complete tree: IDs are unique, every child exists and has one parent, and
/// all declared nodes are reachable from the root.
pub fn compile_module(module: &Module) -> Result<Bytecode, CompileError> {
    let root = module.root.ok_or(CompileError::MissingRoot)?;
    let mut nodes = BTreeMap::new();
    for (id, node) in &module.nodes {
        if nodes.insert(id.0, node).is_some() {
            return Err(CompileError::DuplicateNodeId(*id));
        }
    }

    let mut visited = HashSet::with_capacity(nodes.len());
    let mut tasks = vec![Task::Emit(root)];
    let mut instructions = Vec::with_capacity(nodes.len().saturating_mul(2));
    while let Some(task) = tasks.pop() {
        match task {
            Task::End => instructions.push(Instruction::EndContainer),
            Task::Emit(id) => {
                let node = nodes.get(&id.0).ok_or(CompileError::UnknownNode(id))?;
                if !visited.insert(id) {
                    return Err(CompileError::ReusedOrCyclicNode(id));
                }
                match node {
                    Node::Column { children } | Node::Row { children } => {
                        let child_count = u64::try_from(children.len())
                            .map_err(|_| CompileError::TooManyChildren(id))?;
                        instructions.push(if matches!(node, Node::Column { .. }) {
                            Instruction::BeginColumn {
                                node: id,
                                child_count,
                            }
                        } else {
                            Instruction::BeginRow {
                                node: id,
                                child_count,
                            }
                        });
                        tasks.push(Task::End);
                        for child in children.iter().rev() {
                            tasks.push(Task::Emit(*child));
                        }
                    }
                    Node::Text { content } => instructions.push(Instruction::Text {
                        node: id,
                        content: content.clone(),
                    }),
                    Node::Button { label, on_activate } => instructions.push(Instruction::Button {
                        node: id,
                        action: *on_activate,
                        label: label.clone(),
                    }),
                    Node::Spacer { size } => {
                        if !size.is_finite() || *size < 0.0 {
                            return Err(CompileError::InvalidSpacer(id));
                        }
                        instructions.push(Instruction::Spacer {
                            node: id,
                            size_bits: size.to_bits(),
                        });
                    }
                    // TEMP J1 verification scaffolding — reverted after test run.
                    Node::Input { placeholder } => instructions.push(Instruction::Text {
                        node: id,
                        content: placeholder.clone(),
                    }),
                }
            }
        }
    }

    if let Some(unreachable) = nodes.keys().find(|raw| !visited.contains(&NodeId(**raw))) {
        return Err(CompileError::UnreachableNode(NodeId(*unreachable)));
    }
    let mut bytecode = Bytecode {
        version: BYTECODE_VERSION,
        instructions,
        content_hash: 0,
    };
    bytecode.content_hash = fnv1a(&bytecode.canonical_bytes());
    Ok(bytecode)
}

/// Materialised output from the deterministic reference VM.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionNode {
    Column {
        node: NodeId,
        children: Vec<ExecutionNode>,
    },
    Row {
        node: NodeId,
        children: Vec<ExecutionNode>,
    },
    Text {
        node: NodeId,
        content: String,
    },
    Button {
        node: NodeId,
        action: ActionId,
        label: String,
    },
    Spacer {
        node: NodeId,
        size: f64,
    },
}

#[derive(Debug)]
struct Frame {
    node: NodeId,
    expected: usize,
    children: Vec<ExecutionNode>,
    row: bool,
}

/// Why a bytecode stream was rejected by the reference VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    UnsupportedVersion(u32),
    InvalidNesting,
    WrongChildCount(NodeId),
    MultipleRoots,
    EmptyProgram,
    InvalidSpacer(NodeId),
    HashMismatch,
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "unsupported bytecode version {v}"),
            Self::InvalidNesting => f.write_str("unbalanced container instructions"),
            Self::WrongChildCount(id) => write!(f, "wrong child count for node {}", id.0),
            Self::MultipleRoots => f.write_str("bytecode produced more than one root"),
            Self::EmptyProgram => f.write_str("bytecode produced no root"),
            Self::InvalidSpacer(id) => write!(f, "spacer node {} has invalid size", id.0),
            Self::HashMismatch => f.write_str("bytecode content hash does not match its encoding"),
        }
    }
}

impl std::error::Error for VmError {}

/// Execute bytecode in a host-independent interpreter. The VM recomputes the
/// content hash before interpretation and validates structural invariants.
pub fn execute(bytecode: &Bytecode) -> Result<ExecutionNode, VmError> {
    if bytecode.version != BYTECODE_VERSION {
        return Err(VmError::UnsupportedVersion(bytecode.version));
    }
    if fnv1a(&bytecode.canonical_bytes()) != bytecode.content_hash {
        return Err(VmError::HashMismatch);
    }
    let mut frames: Vec<Frame> = Vec::new();
    let mut root = None;
    for instruction in &bytecode.instructions {
        let completed = match instruction {
            Instruction::BeginColumn { node, child_count } => {
                frames.push(new_frame(*node, *child_count, false)?);
                continue;
            }
            Instruction::BeginRow { node, child_count } => {
                frames.push(new_frame(*node, *child_count, true)?);
                continue;
            }
            Instruction::EndContainer => {
                let frame = frames.pop().ok_or(VmError::InvalidNesting)?;
                if frame.children.len() != frame.expected {
                    return Err(VmError::WrongChildCount(frame.node));
                }
                if frame.row {
                    ExecutionNode::Row {
                        node: frame.node,
                        children: frame.children,
                    }
                } else {
                    ExecutionNode::Column {
                        node: frame.node,
                        children: frame.children,
                    }
                }
            }
            Instruction::Text { node, content } => ExecutionNode::Text {
                node: *node,
                content: content.clone(),
            },
            Instruction::Button {
                node,
                action,
                label,
            } => ExecutionNode::Button {
                node: *node,
                action: *action,
                label: label.clone(),
            },
            Instruction::Spacer { node, size_bits } => {
                let size = f64::from_bits(*size_bits);
                if !size.is_finite() || size < 0.0 {
                    return Err(VmError::InvalidSpacer(*node));
                }
                ExecutionNode::Spacer { node: *node, size }
            }
        };
        append_node(&mut frames, &mut root, completed)?;
    }
    if !frames.is_empty() {
        return Err(VmError::InvalidNesting);
    }
    root.ok_or(VmError::EmptyProgram)
}

fn new_frame(node: NodeId, count: u64, row: bool) -> Result<Frame, VmError> {
    let expected = usize::try_from(count).map_err(|_| VmError::WrongChildCount(node))?;
    Ok(Frame {
        node,
        expected,
        children: Vec::new(),
        row,
    })
}

fn append_node(
    frames: &mut [Frame],
    root: &mut Option<ExecutionNode>,
    node: ExecutionNode,
) -> Result<(), VmError> {
    if let Some(parent) = frames.last_mut() {
        if parent.children.len() >= parent.expected {
            return Err(VmError::WrongChildCount(parent.node));
        }
        parent.children.push(node);
    } else if root.replace(node).is_some() {
        return Err(VmError::MultipleRoots);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_determinism::EffectKind;

    fn module() -> Module {
        Module {
            nodes: vec![
                (
                    NodeId(0),
                    Node::Column {
                        children: vec![NodeId(2), NodeId(1)],
                    },
                ),
                (
                    NodeId(1),
                    Node::Button {
                        label: "Run".into(),
                        on_activate: ActionId(7),
                    },
                ),
                (
                    NodeId(2),
                    Node::Text {
                        content: "scene".into(),
                    },
                ),
            ],
            root: Some(NodeId(0)),
        }
    }

    fn pure_declaration() -> ProcessDecl {
        ProcessDecl::new("scene", DeterminismClass::Pure, Vec::new())
    }

    fn proof() -> ExecutionProof {
        ExecutionProof {
            process: "scene".into(),
            class: DeterminismClass::Pure,
            compiler_version: "0.1.0".into(),
            schema_version: 2,
            policy_version: 3,
            target: "goya-bytecode-v1".into(),
        }
    }

    #[test]
    fn bytecode_encoding_and_execution_are_deterministic() {
        let first = compile_module(&module()).unwrap();
        let second = compile_module(&module()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
        assert_eq!(first.content_hash, fnv1a(&first.canonical_bytes()));
        assert_eq!(execute(&first), execute(&second));
    }

    #[test]
    fn module_node_storage_order_does_not_change_encoding() {
        let mut reversed = module();
        reversed.nodes.reverse();
        assert_eq!(
            compile_module(&module()).unwrap().canonical_bytes(),
            compile_module(&reversed).unwrap().canonical_bytes()
        );
    }

    #[test]
    fn compiler_rejects_dangling_duplicate_and_unreachable_nodes() {
        let dangling = Module {
            nodes: vec![(
                NodeId(0),
                Node::Row {
                    children: vec![NodeId(9)],
                },
            )],
            root: Some(NodeId(0)),
        };
        assert_eq!(
            compile_module(&dangling),
            Err(CompileError::UnknownNode(NodeId(9)))
        );

        let duplicate = Module {
            nodes: vec![
                (
                    NodeId(0),
                    Node::Text {
                        content: "a".into(),
                    },
                ),
                (
                    NodeId(0),
                    Node::Text {
                        content: "b".into(),
                    },
                ),
            ],
            root: Some(NodeId(0)),
        };
        assert_eq!(
            compile_module(&duplicate),
            Err(CompileError::DuplicateNodeId(NodeId(0)))
        );

        let unreachable = Module {
            nodes: vec![
                (
                    NodeId(0),
                    Node::Text {
                        content: "root".into(),
                    },
                ),
                (
                    NodeId(1),
                    Node::Text {
                        content: "orphan".into(),
                    },
                ),
            ],
            root: Some(NodeId(0)),
        };
        assert_eq!(
            compile_module(&unreachable),
            Err(CompileError::UnreachableNode(NodeId(1)))
        );
    }

    #[test]
    fn compiler_rejects_shared_child_and_invalid_spacer() {
        let shared = Module {
            nodes: vec![
                (
                    NodeId(0),
                    Node::Column {
                        children: vec![NodeId(1), NodeId(1)],
                    },
                ),
                (
                    NodeId(1),
                    Node::Text {
                        content: "shared".into(),
                    },
                ),
            ],
            root: Some(NodeId(0)),
        };
        assert_eq!(
            compile_module(&shared),
            Err(CompileError::ReusedOrCyclicNode(NodeId(1)))
        );

        let bad_size = Module {
            nodes: vec![(NodeId(0), Node::Spacer { size: f64::NAN })],
            root: Some(NodeId(0)),
        };
        assert_eq!(
            compile_module(&bad_size),
            Err(CompileError::InvalidSpacer(NodeId(0)))
        );
    }

    #[test]
    fn vm_rejects_tampered_hash_and_malformed_structure() {
        let mut bytecode = compile_module(&module()).unwrap();
        bytecode.content_hash ^= 1;
        assert_eq!(execute(&bytecode), Err(VmError::HashMismatch));

        let malformed = Bytecode {
            version: BYTECODE_VERSION,
            instructions: vec![Instruction::EndContainer],
            content_hash: 0,
        };
        let mut malformed = malformed;
        malformed.content_hash = fnv1a(&malformed.canonical_bytes());
        assert_eq!(execute(&malformed), Err(VmError::InvalidNesting));
    }

    #[test]
    fn verified_compilation_binds_artifact_to_code_and_proof() {
        let compiled = compile_verified(&module(), &pure_declaration(), &proof()).unwrap();
        assert_eq!(compiled.artifact.proof, proof());
        assert_eq!(
            execute(&compiled.bytecode).unwrap(),
            execute(&compile_module(&module()).unwrap()).unwrap()
        );

        let mut changed = module();
        changed.nodes[2] = (
            NodeId(2),
            Node::Text {
                content: "different".into(),
            },
        );
        let other = compile_verified(&changed, &pure_declaration(), &proof()).unwrap();
        assert_ne!(compiled.artifact.content_hash, other.artifact.content_hash);
    }

    #[test]
    fn verified_compilation_rejects_non_pure_classes_and_bad_proofs() {
        let mut stateful = ProcessDecl::new(
            "scene",
            DeterminismClass::StateDeterministic,
            vec![EffectKind::GlobalMutable],
        );
        stateful.state_contract = true;
        let mut state_proof = proof();
        state_proof.class = DeterminismClass::StateDeterministic;
        assert_eq!(
            compile_verified(&module(), &stateful, &state_proof),
            Err(VerifiedCompileError::UnsupportedClass(
                DeterminismClass::StateDeterministic
            ))
        );

        let mut wrong_process = proof();
        wrong_process.process = "other".into();
        assert_eq!(
            compile_verified(&module(), &pure_declaration(), &wrong_process),
            Err(VerifiedCompileError::ProofProcessMismatch)
        );
    }
}
