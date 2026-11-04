//! Replay-backed deterministic D2 state-transition VM.
//!
//! Declared effects are resolved exclusively from an `EffectJournal` replay
//! cursor. This module contains no host I/O and has no live-effect fallback.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use goya_determinism::proofs::ExecutionProof;
use goya_determinism::{classify, DeterminismClass, ProcessDecl, RejectReason};
use sha2::{Digest, Sha256};

use crate::{
    CapabilityLabel, D1Snapshot, D1Value, EffectJournal, InputSnapshot, JournalError, StateContract,
};

/// D2 VM program and outcome format version.
pub const D2_VM_VERSION: u32 = 1;

const MAX_INSTRUCTIONS: usize = 4096;
const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_PROGRAM_BYTES: usize = 2 * 1024 * 1024;
const MAX_REGISTER_BYTES: usize = 1024 * 1024;
const MAX_RESULT_BYTES: usize = 64 * 1024;
const DOMAIN_PROGRAM: &[u8] = b"GOYA-D2-PROGRAM/1\0";
const DOMAIN_INPUT: &[u8] = b"GOYA-D2-VM-INPUT/1\0";
const DOMAIN_OUTCOME: &[u8] = b"GOYA-D2-VM-OUTCOME/1\0";

/// Typed interpretation expected for one journal result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectResultType {
    Int64,
    Text,
    Bool,
}

/// A decoded deterministic host result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectResult {
    Int64(i64),
    Text(String),
    Bool(bool),
}

impl EffectResult {
    /// Encode a typed result as a one-byte tag plus exact bounded payload.
    /// Integers use 8-byte little-endian representation, text uses a u32
    /// little-endian byte length followed by UTF-8, and booleans use one byte.
    pub fn encode(&self) -> Result<Vec<u8>, D2Error> {
        let mut bytes = Vec::new();
        match self {
            Self::Int64(value) => {
                bytes.push(1);
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            Self::Text(value) => {
                if value.len() > MAX_RESULT_BYTES {
                    return Err(D2Error::ResultTooLarge);
                }
                let len = u32::try_from(value.len()).map_err(|_| D2Error::ResultTooLarge)?;
                bytes.push(2);
                bytes.extend_from_slice(&len.to_le_bytes());
                bytes.extend_from_slice(value.as_bytes());
            }
            Self::Bool(value) => {
                bytes.push(3);
                bytes.push(u8::from(*value));
            }
        }
        Ok(bytes)
    }

    fn decode(expected: EffectResultType, bytes: &[u8]) -> Result<Self, D2Error> {
        if bytes.len() > MAX_RESULT_BYTES {
            return Err(D2Error::ResultTooLarge);
        }
        match (expected, bytes) {
            (EffectResultType::Int64, [1, rest @ ..]) if rest.len() == 8 => Ok(Self::Int64(
                i64::from_le_bytes(rest.try_into().map_err(|_| D2Error::MalformedResult)?),
            )),
            (EffectResultType::Text, [2, length @ ..]) if length.len() >= 4 => {
                let size = u32::from_le_bytes(
                    length[..4]
                        .try_into()
                        .map_err(|_| D2Error::MalformedResult)?,
                ) as usize;
                if size > MAX_RESULT_BYTES || length.len() != 4 + size {
                    return Err(D2Error::MalformedResult);
                }
                let text =
                    std::str::from_utf8(&length[4..]).map_err(|_| D2Error::MalformedResult)?;
                Ok(Self::Text(text.to_string()))
            }
            (EffectResultType::Bool, [3, 0]) => Ok(Self::Bool(false)),
            (EffectResultType::Bool, [3, 1]) => Ok(Self::Bool(true)),
            _ => Err(D2Error::MalformedResult),
        }
    }

    fn into_state_value(self) -> D1Value {
        match self {
            Self::Int64(value) => D1Value::Int(value),
            Self::Text(value) => D1Value::Text(value),
            Self::Bool(value) => D1Value::Bool(value),
        }
    }
}

/// Bounded D2 operation set. Effect requests are static bytes embedded in the
/// hashed program; the response is consumed from the journal in sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D2Instruction {
    Constant {
        dst: String,
        value: D1Value,
    },
    ReadState {
        dst: String,
        key: String,
    },
    ReadInput {
        dst: String,
        key: String,
    },
    AddInt {
        dst: String,
        left: String,
        right: String,
    },
    RequestEffect {
        dst: String,
        capability: CapabilityLabel,
        request: Vec<u8>,
        result_type: EffectResultType,
    },
    WriteState {
        key: String,
        src: String,
    },
}

/// A D2 executable bound to its declaration, proof, contract, and stable
/// program hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D2Program {
    process: String,
    proof: ExecutionProof,
    contract: StateContract,
    instructions: Vec<D2Instruction>,
    program_hash: [u8; 32],
}

impl D2Program {
    /// Decode a portable canonical D2 program, binding it back to the caller's
    /// declaration and proof before admitting it. The declaration is required
    /// because proof metadata intentionally does not carry capability grants.
    pub fn decode(
        bytes: &[u8],
        expected_hash: [u8; 32],
        declaration: &ProcessDecl,
        proof: &ExecutionProof,
    ) -> Result<Self, D2Error> {
        if bytes.len() > MAX_PROGRAM_BYTES {
            return Err(D2Error::ProgramTooLarge);
        }
        if digest(bytes) != expected_hash {
            return Err(D2Error::ProgramHashMismatch);
        }
        let mut reader = D2Reader::new(bytes);
        if reader.take(DOMAIN_PROGRAM.len())? != DOMAIN_PROGRAM {
            return Err(D2Error::MalformedProgram);
        }
        let version = reader.u32()?;
        if version != D2_VM_VERSION {
            return Err(D2Error::UnsupportedProgramVersion(version));
        }
        if reader.string(MAX_PROGRAM_BYTES)? != proof.canonical() {
            return Err(D2Error::ProofMismatch);
        }
        let contract = StateContract {
            read_keys: reader.key_set()?,
            write_keys: reader.key_set()?,
            input_keys: reader.key_set()?,
        };
        let count = reader.count(MAX_INSTRUCTIONS)?;
        let mut instructions = Vec::with_capacity(count);
        for _ in 0..count {
            instructions.push(match reader.byte()? {
                0 => D2Instruction::Constant { dst: reader.string(MAX_RESULT_BYTES)?, value: reader.value()? },
                1 => D2Instruction::ReadState { dst: reader.string(MAX_RESULT_BYTES)?, key: reader.string(MAX_RESULT_BYTES)? },
                2 => D2Instruction::ReadInput { dst: reader.string(MAX_RESULT_BYTES)?, key: reader.string(MAX_RESULT_BYTES)? },
                3 => D2Instruction::AddInt { dst: reader.string(MAX_RESULT_BYTES)?, left: reader.string(MAX_RESULT_BYTES)?, right: reader.string(MAX_RESULT_BYTES)? },
                4 => {
                    let dst = reader.string(MAX_RESULT_BYTES)?;
                    let capability = CapabilityLabel::new(reader.string(128)?).map_err(D2Error::Journal)?;
                    let request = reader.bytes(MAX_REQUEST_BYTES)?;
                    let result_type = match reader.byte()? {
                        1 => EffectResultType::Int64,
                        2 => EffectResultType::Text,
                        3 => EffectResultType::Bool,
                        _ => return Err(D2Error::MalformedProgram),
                    };
                    D2Instruction::RequestEffect { dst, capability, request, result_type }
                }
                5 => D2Instruction::WriteState { key: reader.string(MAX_RESULT_BYTES)?, src: reader.string(MAX_RESULT_BYTES)? },
                _ => return Err(D2Error::MalformedProgram),
            });
        }
        if !reader.is_empty() { return Err(D2Error::MalformedProgram); }
        let program = Self::compile(declaration, proof, contract, instructions)?;
        if program.canonical_bytes() != bytes { return Err(D2Error::NonCanonicalProgram); }
        if program.program_hash != expected_hash { return Err(D2Error::ProgramHashMismatch); }
        Ok(program)
    }

    pub fn compile(
        declaration: &ProcessDecl,
        proof: &ExecutionProof,
        contract: StateContract,
        instructions: Vec<D2Instruction>,
    ) -> Result<Self, D2Error> {
        let verified = classify(declaration).map_err(D2Error::Classification)?;
        if verified.class != DeterminismClass::EffectDeterministic {
            return Err(D2Error::WrongClass);
        }
        if proof.process != verified.process {
            return Err(D2Error::ProofProcessMismatch);
        }
        if proof.class != verified.class {
            return Err(D2Error::ProofClassMismatch);
        }
        if declaration.name.len() > 128
            || proof.process.len() > 128
            || proof.compiler_version.len() > 256
            || proof.target.len() > 256
        {
            return Err(D2Error::ProgramTooLarge);
        }
        if instructions.len() > MAX_INSTRUCTIONS {
            return Err(D2Error::TooManyInstructions);
        }
        let effects: BTreeSet<&str> = verified
            .declared_effects
            .iter()
            .map(|effect| effect.as_str())
            .collect();
        validate_program(&contract, &instructions, &effects)?;
        let proof_bytes = proof.canonical();
        if encoded_program_size(&proof_bytes, &contract, &instructions)? > MAX_PROGRAM_BYTES {
            return Err(D2Error::ProgramTooLarge);
        }
        let mut program = Self {
            process: verified.process,
            proof: proof.clone(),
            contract,
            instructions,
            program_hash: [0; 32],
        };
        program.program_hash = digest(&program.canonical_bytes());
        Ok(program)
    }

    pub fn process(&self) -> &str {
        &self.process
    }

    pub fn program_hash(&self) -> [u8; 32] {
        self.program_hash
    }

    /// Stable instruction encoding, with canonical sorted state-contract
    /// fields, fixed tags, and length-prefixed strings/requests.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(DOMAIN_PROGRAM);
        bytes.extend_from_slice(&D2_VM_VERSION.to_le_bytes());
        put_str(&mut bytes, &self.proof.canonical());
        put_set(&mut bytes, self.contract.read_keys());
        put_set(&mut bytes, self.contract.write_keys());
        put_set(&mut bytes, self.contract.input_keys());
        put_u64(&mut bytes, self.instructions.len() as u64);
        for instruction in &self.instructions {
            match instruction {
                D2Instruction::Constant { dst, value } => {
                    bytes.push(0);
                    put_str(&mut bytes, dst);
                    put_value(&mut bytes, value);
                }
                D2Instruction::ReadState { dst, key } => {
                    bytes.push(1);
                    put_str(&mut bytes, dst);
                    put_str(&mut bytes, key);
                }
                D2Instruction::ReadInput { dst, key } => {
                    bytes.push(2);
                    put_str(&mut bytes, dst);
                    put_str(&mut bytes, key);
                }
                D2Instruction::AddInt { dst, left, right } => {
                    bytes.push(3);
                    put_str(&mut bytes, dst);
                    put_str(&mut bytes, left);
                    put_str(&mut bytes, right);
                }
                D2Instruction::RequestEffect {
                    dst,
                    capability,
                    request,
                    result_type,
                } => {
                    bytes.push(4);
                    put_str(&mut bytes, dst);
                    put_str(&mut bytes, capability.as_str());
                    put_bytes(&mut bytes, request);
                    bytes.push(result_tag(*result_type));
                }
                D2Instruction::WriteState { key, src } => {
                    bytes.push(5);
                    put_str(&mut bytes, key);
                    put_str(&mut bytes, src);
                }
            }
        }
        bytes
    }

    /// Run using only the supplied journal. Every effect opcode must consume
    /// an exact capability/request match; the journal must be fully consumed.
    pub fn execute(
        &self,
        state: &D1Snapshot,
        inputs: &InputSnapshot,
        journal: &EffectJournal,
    ) -> Result<D2Outcome, D2Error> {
        validate_snapshots(&self.contract, state, inputs)?;
        let mut replay = journal.replay().map_err(D2Error::Journal)?;
        let mut registers = BTreeMap::<String, D1Value>::new();
        let mut register_bytes = 0usize;
        let mut next_state = state.values().clone();
        for instruction in &self.instructions {
            match instruction {
                D2Instruction::Constant { dst, value } => {
                    store_register(&mut registers, &mut register_bytes, dst, value.clone())?;
                }
                D2Instruction::ReadState { dst, key } => {
                    let value = state
                        .values()
                        .get(key)
                        .ok_or_else(|| D2Error::MissingValue(key.clone()))?;
                    store_register(&mut registers, &mut register_bytes, dst, value.clone())?;
                }
                D2Instruction::ReadInput { dst, key } => {
                    let value = inputs
                        .values()
                        .get(key)
                        .ok_or_else(|| D2Error::MissingValue(key.clone()))?;
                    store_register(&mut registers, &mut register_bytes, dst, value.clone())?;
                }
                D2Instruction::AddInt { dst, left, right } => {
                    let lhs = registers
                        .get(left)
                        .ok_or_else(|| D2Error::MissingRegister(left.clone()))?;
                    let rhs = registers
                        .get(right)
                        .ok_or_else(|| D2Error::MissingRegister(right.clone()))?;
                    let (D1Value::Int(lhs), D1Value::Int(rhs)) = (lhs, rhs) else {
                        return Err(D2Error::TypeMismatch);
                    };
                    let sum = lhs.checked_add(*rhs).ok_or(D2Error::ArithmeticOverflow)?;
                    store_register(&mut registers, &mut register_bytes, dst, D1Value::Int(sum))?;
                }
                D2Instruction::RequestEffect {
                    dst,
                    capability,
                    request,
                    result_type,
                } => {
                    let bytes = replay
                        .replay(capability, request)
                        .map_err(D2Error::Journal)?;
                    let result = EffectResult::decode(*result_type, bytes)?.into_state_value();
                    store_register(&mut registers, &mut register_bytes, dst, result)?;
                }
                D2Instruction::WriteState { key, src } => {
                    let value = registers
                        .get(src)
                        .ok_or_else(|| D2Error::MissingRegister(src.clone()))?;
                    next_state.insert(key.clone(), value.clone());
                }
            }
        }
        replay.finish().map_err(D2Error::Journal)?;
        validate_state_budget(&next_state)?;
        let revision_after = state
            .revision
            .checked_add(1)
            .ok_or(D2Error::RevisionOverflow)?;
        let input_hash = input_hash(state, inputs);
        let outcome_hash = outcome_hash(
            self.program_hash,
            input_hash,
            state.revision,
            revision_after,
            &next_state,
            &registers,
        );
        Ok(D2Outcome {
            program_hash: self.program_hash,
            input_hash,
            revision_before: state.revision,
            revision_after,
            next_state,
            registers,
            outcome_hash,
        })
    }
}

/// Replayable output of a D2 state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D2Outcome {
    pub program_hash: [u8; 32],
    pub input_hash: [u8; 32],
    pub revision_before: u64,
    pub revision_after: u64,
    pub next_state: BTreeMap<String, D1Value>,
    pub registers: BTreeMap<String, D1Value>,
    pub outcome_hash: [u8; 32],
}

/// D2 compilation and execution failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D2Error {
    Classification(RejectReason),
    WrongClass,
    ProofProcessMismatch,
    ProofClassMismatch,
    TooManyInstructions,
    ProgramTooLarge,
    ProgramHashMismatch,
    UnsupportedProgramVersion(u32),
    MalformedProgram,
    NonCanonicalProgram,
    ProofMismatch,
    InvalidRegister(String),
    UndeclaredRead(String),
    UndeclaredInput(String),
    UndeclaredWrite(String),
    UndeclaredEffect(String),
    DuplicateWrite(String),
    RequestTooLarge,
    ResultTooLarge,
    MalformedResult,
    MissingRegister(String),
    MissingValue(String),
    TypeMismatch,
    ArithmeticOverflow,
    SnapshotSchemaMismatch,
    InputSchemaMismatch,
    SnapshotTooLarge,
    RegisterLimit,
    RevisionOverflow,
    Journal(JournalError),
}

impl fmt::Display for D2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Classification(e) => write!(f, "D2 declaration rejected: {e}"),
            Self::WrongClass => f.write_str("program is not classified D2"),
            Self::ProofProcessMismatch => f.write_str("proof process differs from declaration"),
            Self::ProofClassMismatch => f.write_str("proof class differs from declaration"),
            Self::TooManyInstructions => f.write_str("D2 instruction limit exceeded"),
            Self::ProgramTooLarge => f.write_str("canonical D2 program exceeds the byte limit"),
            Self::ProgramHashMismatch => f.write_str("D2 program hash does not match its encoding"),
            Self::UnsupportedProgramVersion(v) => write!(f, "unsupported D2 program version {v}"),
            Self::MalformedProgram => f.write_str("canonical D2 program is malformed"),
            Self::NonCanonicalProgram => f.write_str("D2 program encoding is not canonical"),
            Self::ProofMismatch => f.write_str("embedded D2 proof differs from supplied proof"),
            Self::InvalidRegister(r) => write!(f, "invalid register {r:?}"),
            Self::UndeclaredRead(k) => write!(f, "state read {k} is not declared"),
            Self::UndeclaredInput(k) => write!(f, "input read {k} is not declared"),
            Self::UndeclaredWrite(k) => write!(f, "state write {k} is not declared"),
            Self::UndeclaredEffect(k) => write!(f, "effect capability {k} is not declared"),
            Self::DuplicateWrite(k) => write!(f, "state key {k} is written more than once"),
            Self::RequestTooLarge => f.write_str("effect request exceeds byte limit"),
            Self::ResultTooLarge => f.write_str("effect result exceeds byte limit"),
            Self::MalformedResult => f.write_str("effect result encoding is malformed"),
            Self::MissingRegister(r) => write!(f, "register {r} has no value"),
            Self::MissingValue(k) => write!(f, "snapshot value {k} is missing"),
            Self::TypeMismatch => f.write_str("instruction operand has wrong value type"),
            Self::ArithmeticOverflow => f.write_str("integer addition overflow"),
            Self::SnapshotSchemaMismatch => {
                f.write_str("state snapshot keys do not match contract")
            }
            Self::InputSchemaMismatch => f.write_str("input snapshot keys do not match contract"),
            Self::SnapshotTooLarge => f.write_str("D2 snapshot exceeds byte limit"),
            Self::RegisterLimit => f.write_str("D2 registers exceed byte limit"),
            Self::RevisionOverflow => f.write_str("state revision counter exhausted"),
            Self::Journal(e) => write!(f, "effect replay failed: {e}"),
        }
    }
}

impl std::error::Error for D2Error {}

fn validate_program(
    contract: &StateContract,
    code: &[D2Instruction],
    effects: &BTreeSet<&str>,
) -> Result<(), D2Error> {
    let mut registers = BTreeSet::new();
    let mut written = BTreeSet::new();
    for instruction in code {
        match instruction {
            D2Instruction::Constant { dst, value } => {
                validate_register(dst)?;
                if matches!(value, D1Value::Text(text) if text.len() > MAX_RESULT_BYTES) {
                    return Err(D2Error::ProgramTooLarge);
                }
                registers.insert(dst.clone());
            }
            D2Instruction::ReadState { dst, key } => {
                validate_register(dst)?;
                if !contract.read_keys().contains(key) {
                    return Err(D2Error::UndeclaredRead(key.clone()));
                }
                registers.insert(dst.clone());
            }
            D2Instruction::ReadInput { dst, key } => {
                validate_register(dst)?;
                if !contract.input_keys().contains(key) {
                    return Err(D2Error::UndeclaredInput(key.clone()));
                }
                registers.insert(dst.clone());
            }
            D2Instruction::AddInt { dst, left, right } => {
                validate_register(dst)?;
                require_register(&registers, left)?;
                require_register(&registers, right)?;
                registers.insert(dst.clone());
            }
            D2Instruction::RequestEffect {
                dst,
                capability,
                request,
                ..
            } => {
                validate_register(dst)?;
                if request.len() > MAX_REQUEST_BYTES {
                    return Err(D2Error::RequestTooLarge);
                }
                if !effects.contains(capability.as_str()) {
                    return Err(D2Error::UndeclaredEffect(capability.as_str().to_string()));
                }
                registers.insert(dst.clone());
            }
            D2Instruction::WriteState { key, src } => {
                if !contract.write_keys().contains(key) {
                    return Err(D2Error::UndeclaredWrite(key.clone()));
                }
                require_register(&registers, src)?;
                if !written.insert(key.clone()) {
                    return Err(D2Error::DuplicateWrite(key.clone()));
                }
            }
        }
    }
    Ok(())
}

fn validate_snapshots(
    contract: &StateContract,
    state: &D1Snapshot,
    inputs: &InputSnapshot,
) -> Result<(), D2Error> {
    let touched: BTreeSet<String> = contract
        .read_keys()
        .union(contract.write_keys())
        .cloned()
        .collect();
    if state.values().keys().cloned().collect::<BTreeSet<_>>() != touched {
        return Err(D2Error::SnapshotSchemaMismatch);
    }
    if inputs.values().keys().cloned().collect::<BTreeSet<_>>() != *contract.input_keys() {
        return Err(D2Error::InputSchemaMismatch);
    }
    let bytes = map_encoded_size(state.values())?
        .checked_add(map_encoded_size(inputs.values())?)
        .ok_or(D2Error::SnapshotTooLarge)?;
    if bytes > MAX_SNAPSHOT_BYTES {
        return Err(D2Error::SnapshotTooLarge);
    }
    Ok(())
}

fn validate_state_budget(state: &BTreeMap<String, D1Value>) -> Result<(), D2Error> {
    if map_encoded_size(state)? > MAX_SNAPSHOT_BYTES {
        Err(D2Error::SnapshotTooLarge)
    } else {
        Ok(())
    }
}

fn map_encoded_size(state: &BTreeMap<String, D1Value>) -> Result<usize, D2Error> {
    state.iter().try_fold(8usize, |total, (key, value)| {
        total
            .checked_add(8)
            .and_then(|n| n.checked_add(key.len()))
            .and_then(|n| n.checked_add(encoded_value_size(value)))
            .ok_or(D2Error::SnapshotTooLarge)
    })
}

fn validate_register(name: &str) -> Result<(), D2Error> {
    if !name.is_empty()
        && name.len() <= 128
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
    {
        Ok(())
    } else {
        Err(D2Error::InvalidRegister(name.into()))
    }
}

fn require_register(registers: &BTreeSet<String>, name: &str) -> Result<(), D2Error> {
    validate_register(name)?;
    if registers.contains(name) {
        Ok(())
    } else {
        Err(D2Error::MissingRegister(name.into()))
    }
}

fn store_register(
    registers: &mut BTreeMap<String, D1Value>,
    current_bytes: &mut usize,
    name: &str,
    value: D1Value,
) -> Result<(), D2Error> {
    let old_size = registers.get(name).map_or(0, state_value_size);
    let next_size = current_bytes
        .checked_sub(old_size)
        .and_then(|n| n.checked_add(name.len()))
        .and_then(|n| n.checked_add(state_value_size(&value)))
        .ok_or(D2Error::RegisterLimit)?;
    if next_size > MAX_REGISTER_BYTES {
        return Err(D2Error::RegisterLimit);
    }
    registers.insert(name.to_string(), value);
    *current_bytes = next_size;
    Ok(())
}

fn state_value_size(value: &D1Value) -> usize {
    match value {
        D1Value::Int(_) => 8,
        D1Value::Text(text) => text.len(),
        D1Value::Bool(_) => 1,
    }
}

fn encoded_program_size(
    proof: &str,
    contract: &StateContract,
    code: &[D2Instruction],
) -> Result<usize, D2Error> {
    let mut size = DOMAIN_PROGRAM.len() + 4 + 8 + proof.len() + 8;
    for set in [
        contract.read_keys(),
        contract.write_keys(),
        contract.input_keys(),
    ] {
        size = size.checked_add(8).ok_or(D2Error::ProgramTooLarge)?;
        for key in set {
            size = size
                .checked_add(8 + key.len())
                .ok_or(D2Error::ProgramTooLarge)?;
        }
    }
    for op in code {
        let op_size = match op {
            D2Instruction::Constant { dst, value } => 1 + 8 + dst.len() + encoded_value_size(value),
            D2Instruction::ReadState { dst, key } | D2Instruction::ReadInput { dst, key } => {
                1 + 8 + dst.len() + 8 + key.len()
            }
            D2Instruction::AddInt { dst, left, right } => {
                1 + 8 + dst.len() + 8 + left.len() + 8 + right.len()
            }
            D2Instruction::RequestEffect {
                dst,
                capability,
                request,
                ..
            } => 1 + 8 + dst.len() + 8 + capability.as_str().len() + 8 + request.len() + 1,
            D2Instruction::WriteState { key, src } => 1 + 8 + key.len() + 8 + src.len(),
        };
        size = size.checked_add(op_size).ok_or(D2Error::ProgramTooLarge)?;
        if size > MAX_PROGRAM_BYTES {
            return Err(D2Error::ProgramTooLarge);
        }
    }
    Ok(size)
}

fn encoded_value_size(value: &D1Value) -> usize {
    match value {
        D1Value::Int(_) => 9,
        D1Value::Text(text) => 1 + 8 + text.len(),
        D1Value::Bool(_) => 2,
    }
}

fn input_hash(state: &D1Snapshot, inputs: &InputSnapshot) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(DOMAIN_INPUT);
    put_u64(&mut bytes, state.revision);
    put_map(&mut bytes, state.values());
    put_map(&mut bytes, inputs.values());
    digest(&bytes)
}

fn outcome_hash(
    program_hash: [u8; 32],
    input_hash: [u8; 32],
    revision_before: u64,
    revision_after: u64,
    state: &BTreeMap<String, D1Value>,
    registers: &BTreeMap<String, D1Value>,
) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(DOMAIN_OUTCOME);
    bytes.extend_from_slice(&program_hash);
    bytes.extend_from_slice(&input_hash);
    put_u64(&mut bytes, revision_before);
    put_u64(&mut bytes, revision_after);
    put_map(&mut bytes, state);
    put_map(&mut bytes, registers);
    digest(&bytes)
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

struct D2Reader<'a> { bytes: &'a [u8], offset: usize }
impl<'a> D2Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, offset: 0 } }
    fn take(&mut self, len: usize) -> Result<&'a [u8], D2Error> {
        let end = self.offset.checked_add(len).ok_or(D2Error::MalformedProgram)?;
        let value = self.bytes.get(self.offset..end).ok_or(D2Error::MalformedProgram)?;
        self.offset = end;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, D2Error> { Ok(self.take(1)?[0]) }
    fn u32(&mut self) -> Result<u32, D2Error> { Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| D2Error::MalformedProgram)?)) }
    fn u64(&mut self) -> Result<u64, D2Error> { Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| D2Error::MalformedProgram)?)) }
    fn count(&mut self, max: usize) -> Result<usize, D2Error> {
        let count = usize::try_from(self.u64()?).map_err(|_| D2Error::MalformedProgram)?;
        if count > max { Err(D2Error::MalformedProgram) } else { Ok(count) }
    }
    fn bytes(&mut self, max: usize) -> Result<Vec<u8>, D2Error> {
        let len = self.count(max)?; Ok(self.take(len)?.to_vec())
    }
    fn string(&mut self, max: usize) -> Result<String, D2Error> {
        String::from_utf8(self.bytes(max)?).map_err(|_| D2Error::MalformedProgram)
    }
    fn key_set(&mut self) -> Result<BTreeSet<String>, D2Error> {
        let count = self.count(256)?;
        let mut values = BTreeSet::new();
        let mut previous: Option<String> = None;
        for _ in 0..count {
            let key = self.string(64 * 1024)?;
            if key.is_empty() || key.chars().any(char::is_control) || previous.as_ref().is_some_and(|p| p >= &key) {
                return Err(D2Error::NonCanonicalProgram);
            }
            previous = Some(key.clone()); values.insert(key);
        }
        Ok(values)
    }
    fn value(&mut self) -> Result<D1Value, D2Error> {
        match self.byte()? {
            0 => Ok(D1Value::Int(i64::from_le_bytes(self.take(8)?.try_into().map_err(|_| D2Error::MalformedProgram)?))),
            1 => Ok(D1Value::Text(self.string(MAX_RESULT_BYTES)?)),
            2 => match self.byte()? { 0 => Ok(D1Value::Bool(false)), 1 => Ok(D1Value::Bool(true)), _ => Err(D2Error::MalformedProgram) },
            _ => Err(D2Error::MalformedProgram),
        }
    }
    fn is_empty(&self) -> bool { self.offset == self.bytes.len() }
}

fn put_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_str(bytes: &mut Vec<u8>, value: &str) {
    put_bytes(bytes, value.as_bytes());
}

fn put_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    put_u64(bytes, value.len() as u64);
    bytes.extend_from_slice(value);
}

fn put_set(bytes: &mut Vec<u8>, values: &BTreeSet<String>) {
    put_u64(bytes, values.len() as u64);
    for value in values {
        put_str(bytes, value);
    }
}

fn put_map(bytes: &mut Vec<u8>, values: &BTreeMap<String, D1Value>) {
    put_u64(bytes, values.len() as u64);
    for (key, value) in values {
        put_str(bytes, key);
        put_value(bytes, value);
    }
}

fn put_value(bytes: &mut Vec<u8>, value: &D1Value) {
    match value {
        D1Value::Int(value) => {
            bytes.push(0);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        D1Value::Text(value) => {
            bytes.push(1);
            put_str(bytes, value);
        }
        D1Value::Bool(value) => {
            bytes.push(2);
            bytes.push(u8::from(*value));
        }
    }
}

fn result_tag(result_type: EffectResultType) -> u8 {
    match result_type {
        EffectResultType::Int64 => 1,
        EffectResultType::Text => 2,
        EffectResultType::Bool => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_determinism::EffectKind;

    fn declaration() -> ProcessDecl {
        ProcessDecl::new(
            "remote-counter",
            DeterminismClass::EffectDeterministic,
            vec![EffectKind::Network],
        )
    }

    fn proof() -> ExecutionProof {
        ExecutionProof {
            process: "remote-counter".into(),
            class: DeterminismClass::EffectDeterministic,
            compiler_version: "0.1.0".into(),
            schema_version: 2,
            policy_version: 3,
            target: "goya-d2-vm-v1".into(),
        }
    }

    fn capability() -> CapabilityLabel {
        CapabilityLabel::new("network").unwrap()
    }

    fn contract() -> StateContract {
        StateContract::new(["count".into()], ["count".into()], []).unwrap()
    }

    fn program() -> D2Program {
        D2Program::compile(
            &declaration(),
            &proof(),
            contract(),
            vec![
                D2Instruction::RequestEffect {
                    dst: "remote".into(),
                    capability: capability(),
                    request: b"GET /increment".to_vec(),
                    result_type: EffectResultType::Int64,
                },
                D2Instruction::ReadState {
                    dst: "old".into(),
                    key: "count".into(),
                },
                D2Instruction::AddInt {
                    dst: "next".into(),
                    left: "old".into(),
                    right: "remote".into(),
                },
                D2Instruction::WriteState {
                    key: "count".into(),
                    src: "next".into(),
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn canonical_d2_artifact_decodes_only_with_matching_declaration_and_proof() {
        let original = program();
        let bytes = original.canonical_bytes();
        let restored = D2Program::decode(&bytes, original.program_hash(), &declaration(), &proof())
            .expect("portable D2 artifact should validate");
        assert_eq!(restored, original);
        assert_eq!(
            D2Program::decode(&bytes, original.program_hash(), &declaration(), &ExecutionProof { target: "other".into(), ..proof() }),
            Err(D2Error::ProofMismatch)
        );
        let mut tampered = bytes;
        tampered.push(0);
        assert_eq!(
            D2Program::decode(&tampered, original.program_hash(), &declaration(), &proof()),
            Err(D2Error::ProgramHashMismatch)
        );
    }

    fn state() -> (D1Snapshot, InputSnapshot) {
        (
            D1Snapshot::new(5, BTreeMap::from([("count".into(), D1Value::Int(8))])).unwrap(),
            InputSnapshot::default(),
        )
    }

    fn journal(result: &[u8]) -> EffectJournal {
        let mut journal = EffectJournal::new_d2(
            &declaration(),
            &proof(),
            [capability()],
            crate::JournalLimits::default(),
        )
        .unwrap();
        journal
            .record(&capability(), b"GET /increment", result)
            .unwrap();
        journal
    }

    #[test]
    fn recorded_effect_result_deterministically_changes_state() {
        let program = program();
        let (state, inputs) = state();
        let encoded = EffectResult::Int64(4).encode().unwrap();
        let journal = journal(&encoded);
        let first = program.execute(&state, &inputs, &journal).unwrap();
        let second = program.execute(&state, &inputs, &journal).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.next_state["count"], D1Value::Int(12));
        assert_eq!(first.revision_after, 6);
    }

    #[test]
    fn rejects_wrong_class_and_bad_proof() {
        let mut d1 = declaration();
        d1.claims = DeterminismClass::StateDeterministic;
        d1.state_contract = true;
        d1.declared_effects.clear();
        let mut d1_proof = proof();
        d1_proof.class = DeterminismClass::StateDeterministic;
        assert_eq!(
            D2Program::compile(&d1, &d1_proof, contract(), vec![]).err(),
            Some(D2Error::WrongClass)
        );
        let mut bad_proof = proof();
        bad_proof.process = "other".into();
        assert_eq!(
            D2Program::compile(&declaration(), &bad_proof, contract(), vec![]).err(),
            Some(D2Error::ProofProcessMismatch)
        );
    }

    #[test]
    fn rejects_undeclared_effect_and_invalid_typed_result() {
        let undeclared = D2Instruction::RequestEffect {
            dst: "value".into(),
            capability: CapabilityLabel::new("clock").unwrap(),
            request: b"now".to_vec(),
            result_type: EffectResultType::Int64,
        };
        assert_eq!(
            D2Program::compile(&declaration(), &proof(), contract(), vec![undeclared],).err(),
            Some(D2Error::UndeclaredEffect("clock".into()))
        );
        let (state, inputs) = state();
        assert_eq!(
            program().execute(&state, &inputs, &journal(&[1, 2, 3])),
            Err(D2Error::MalformedResult)
        );
        assert_eq!(
            EffectResult::decode(EffectResultType::Bool, &[3, 2]),
            Err(D2Error::MalformedResult)
        );
    }

    #[test]
    fn rejects_request_mismatch_and_unused_journal_entries() {
        let (state, inputs) = state();
        let mut bad = EffectJournal::new_d2(
            &declaration(),
            &proof(),
            [capability()],
            crate::JournalLimits::default(),
        )
        .unwrap();
        let encoded = EffectResult::Int64(4).encode().unwrap();
        bad.record(&capability(), b"different", &encoded).unwrap();
        assert!(matches!(
            program().execute(&state, &inputs, &bad),
            Err(D2Error::Journal(JournalError::RequestMismatch {
                sequence: 0
            }))
        ));

        let mut extra = journal(&encoded);
        extra.record(&capability(), b"second", &encoded).unwrap();
        assert_eq!(
            program().execute(&state, &inputs, &extra),
            Err(D2Error::Journal(JournalError::UnconsumedEntries {
                remaining: 1
            }))
        );
    }

    #[test]
    fn result_encodings_are_typed_and_exact() {
        assert_eq!(
            EffectResult::decode(
                EffectResultType::Int64,
                &EffectResult::Int64(-2).encode().unwrap()
            ),
            Ok(EffectResult::Int64(-2))
        );
        assert_eq!(
            EffectResult::decode(
                EffectResultType::Text,
                &EffectResult::Text("ok".into()).encode().unwrap()
            ),
            Ok(EffectResult::Text("ok".into()))
        );
        assert_eq!(
            EffectResult::decode(
                EffectResultType::Bool,
                &EffectResult::Bool(true).encode().unwrap()
            ),
            Ok(EffectResult::Bool(true))
        );
        assert_eq!(
            EffectResult::decode(EffectResultType::Int64, &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Err(D2Error::MalformedResult)
        );
    }

    #[test]
    fn rejects_state_revision_and_arithmetic_overflow() {
        let encoded = EffectResult::Int64(1).encode().unwrap();
        let journal = journal(&encoded);
        let inputs = InputSnapshot::default();
        let max_state = D1Snapshot::new(
            1,
            BTreeMap::from([("count".into(), D1Value::Int(i64::MAX))]),
        )
        .unwrap();
        assert_eq!(
            program().execute(&max_state, &inputs, &journal),
            Err(D2Error::ArithmeticOverflow)
        );

        let (mut state, inputs) = state();
        state.revision = u64::MAX;
        assert_eq!(
            program().execute(&state, &inputs, &journal),
            Err(D2Error::RevisionOverflow)
        );
    }
}
