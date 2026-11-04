//! Bounded deterministic D1 state-transition VM.
//!
//! This VM accepts only an explicitly state-contract-backed D1 process. It
//! reads immutable, caller-supplied snapshots and produces a new snapshot;
//! it never mutates a store or performs host I/O.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use goya_determinism::proofs::ExecutionProof;
use goya_determinism::{classify, DeterminismClass, ProcessDecl, RejectReason};
use sha2::{Digest, Sha256};

/// D1 VM bytecode and outcome format version.
pub const D1_VM_VERSION: u32 = 1;

const MAX_CONTRACT_KEYS: usize = 256;
const MAX_INSTRUCTIONS: usize = 4096;
const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const MAX_PROGRAM_BYTES: usize = 2 * 1024 * 1024;
const MAX_REGISTER_BYTES: usize = 1024 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const DOMAIN_PROGRAM: &[u8] = b"GOYA-D1-PROGRAM/1\0";
const DOMAIN_SNAPSHOT: &[u8] = b"GOYA-D1-SNAPSHOT/1\0";
const DOMAIN_OUTCOME: &[u8] = b"GOYA-D1-OUTCOME/1\0";

/// Scalar deterministic state value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D1Value {
    Int(i64),
    Text(String),
    Bool(bool),
}

/// Explicit allowlist for all state and deterministic input access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateContract {
    read_keys: BTreeSet<String>,
    write_keys: BTreeSet<String>,
    input_keys: BTreeSet<String>,
}

impl StateContract {
    pub fn new(
        read_keys: impl IntoIterator<Item = String>,
        write_keys: impl IntoIterator<Item = String>,
        input_keys: impl IntoIterator<Item = String>,
    ) -> Result<Self, D1Error> {
        let read_keys = validate_keys(read_keys)?;
        let write_keys = validate_keys(write_keys)?;
        let input_keys = validate_keys(input_keys)?;
        let unique_count = read_keys
            .union(&write_keys)
            .chain(input_keys.iter())
            .collect::<BTreeSet<_>>()
            .len();
        if unique_count > MAX_CONTRACT_KEYS {
            return Err(D1Error::TooManyKeys);
        }
        Ok(Self {
            read_keys,
            write_keys,
            input_keys,
        })
    }

    pub fn read_keys(&self) -> &BTreeSet<String> {
        &self.read_keys
    }

    pub fn write_keys(&self) -> &BTreeSet<String> {
        &self.write_keys
    }

    pub fn input_keys(&self) -> &BTreeSet<String> {
        &self.input_keys
    }
}

/// Immutable state snapshot for a single deterministic transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D1Snapshot {
    pub revision: u64,
    values: BTreeMap<String, D1Value>,
}

impl D1Snapshot {
    pub fn new(revision: u64, values: BTreeMap<String, D1Value>) -> Result<Self, D1Error> {
        validate_snapshot_map(&values)?;
        Ok(Self { revision, values })
    }

    pub fn values(&self) -> &BTreeMap<String, D1Value> {
        &self.values
    }
}

/// Immutable declared input values. Exact schema matching prevents hidden
/// environment reads from entering a deterministic transition.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputSnapshot {
    values: BTreeMap<String, D1Value>,
}

impl InputSnapshot {
    pub fn new(values: BTreeMap<String, D1Value>) -> Result<Self, D1Error> {
        validate_snapshot_map(&values)?;
        Ok(Self { values })
    }

    pub fn values(&self) -> &BTreeMap<String, D1Value> {
        &self.values
    }
}

/// Supported D1 operations. There are no loops, system calls, clocks, random
/// sources, or implicit environment reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D1Instruction {
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
    WriteState {
        key: String,
        src: String,
    },
}

/// A compiled D1 program bound to its verified declaration, proof, and state
/// contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D1Program {
    process: String,
    proof: ExecutionProof,
    contract: StateContract,
    instructions: Vec<D1Instruction>,
    program_hash: [u8; 32],
}

impl D1Program {
    pub fn compile(
        declaration: &ProcessDecl,
        proof: &ExecutionProof,
        contract: StateContract,
        instructions: Vec<D1Instruction>,
    ) -> Result<Self, D1Error> {
        let verified = classify(declaration).map_err(D1Error::Classification)?;
        if verified.class != DeterminismClass::StateDeterministic {
            return Err(D1Error::WrongClass);
        }
        if !declaration.declared_effects.is_empty() {
            return Err(D1Error::DeclaredEffects);
        }
        if proof.process != verified.process {
            return Err(D1Error::ProofProcessMismatch);
        }
        if proof.class != verified.class {
            return Err(D1Error::ProofClassMismatch);
        }
        if declaration.name.len() > 128
            || proof.process.len() > 128
            || proof.compiler_version.len() > 256
            || proof.target.len() > 256
        {
            return Err(D1Error::ProgramTooLarge);
        }
        if instructions.len() > MAX_INSTRUCTIONS {
            return Err(D1Error::TooManyInstructions);
        }
        validate_program(&contract, &instructions)?;
        let proof_bytes = proof.canonical();
        if encoded_program_size(&proof_bytes, &contract, &instructions)? > MAX_PROGRAM_BYTES {
            return Err(D1Error::ProgramTooLarge);
        }
        let mut program = Self {
            process: verified.process,
            proof: proof.clone(),
            contract,
            instructions,
            program_hash: [0; 32],
        };
        program.program_hash = hash(&program.canonical_bytes());
        Ok(program)
    }

    pub fn process(&self) -> &str {
        &self.process
    }

    pub fn program_hash(&self) -> [u8; 32] {
        self.program_hash
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(DOMAIN_PROGRAM);
        out.extend_from_slice(&D1_VM_VERSION.to_le_bytes());
        put_str(&mut out, &self.proof.canonical());
        put_key_set(&mut out, &self.contract.read_keys);
        put_key_set(&mut out, &self.contract.write_keys);
        put_key_set(&mut out, &self.contract.input_keys);
        put_u64(&mut out, self.instructions.len() as u64);
        for instruction in &self.instructions {
            match instruction {
                D1Instruction::Constant { dst, value } => {
                    out.push(0);
                    put_str(&mut out, dst);
                    put_value(&mut out, value);
                }
                D1Instruction::ReadState { dst, key } => {
                    out.push(1);
                    put_str(&mut out, dst);
                    put_str(&mut out, key);
                }
                D1Instruction::ReadInput { dst, key } => {
                    out.push(2);
                    put_str(&mut out, dst);
                    put_str(&mut out, key);
                }
                D1Instruction::AddInt { dst, left, right } => {
                    out.push(3);
                    put_str(&mut out, dst);
                    put_str(&mut out, left);
                    put_str(&mut out, right);
                }
                D1Instruction::WriteState { key, src } => {
                    out.push(4);
                    put_str(&mut out, key);
                    put_str(&mut out, src);
                }
            }
        }
        out
    }

    /// Execute against exactly the declared state and input snapshots.
    /// Returns a deterministic next-state value without mutating either
    /// snapshot or any host store.
    pub fn execute(
        &self,
        state: &D1Snapshot,
        inputs: &InputSnapshot,
    ) -> Result<D1Outcome, D1Error> {
        self.validate_snapshots(state, inputs)?;
        let mut registers = BTreeMap::<String, D1Value>::new();
        let mut register_bytes = 0usize;
        let mut next_state = state.values.clone();
        for instruction in &self.instructions {
            match instruction {
                D1Instruction::Constant { dst, value } => {
                    store_register(&mut registers, &mut register_bytes, dst, value.clone())?;
                }
                D1Instruction::ReadState { dst, key } => {
                    let value = state
                        .values
                        .get(key)
                        .ok_or_else(|| D1Error::MissingValue(key.clone()))?;
                    store_register(&mut registers, &mut register_bytes, dst, value.clone())?;
                }
                D1Instruction::ReadInput { dst, key } => {
                    let value = inputs
                        .values
                        .get(key)
                        .ok_or_else(|| D1Error::MissingValue(key.clone()))?;
                    store_register(&mut registers, &mut register_bytes, dst, value.clone())?;
                }
                D1Instruction::AddInt { dst, left, right } => {
                    let lhs = registers
                        .get(left)
                        .ok_or_else(|| D1Error::MissingRegister(left.clone()))?;
                    let rhs = registers
                        .get(right)
                        .ok_or_else(|| D1Error::MissingRegister(right.clone()))?;
                    let (D1Value::Int(lhs), D1Value::Int(rhs)) = (lhs, rhs) else {
                        return Err(D1Error::TypeMismatch);
                    };
                    let sum = lhs.checked_add(*rhs).ok_or(D1Error::ArithmeticOverflow)?;
                    store_register(&mut registers, &mut register_bytes, dst, D1Value::Int(sum))?;
                }
                D1Instruction::WriteState { key, src } => {
                    let value = registers
                        .get(src)
                        .ok_or_else(|| D1Error::MissingRegister(src.clone()))?;
                    next_state.insert(key.clone(), value.clone());
                }
            }
        }
        validate_snapshot_map(&next_state)?;
        let revision_after = state
            .revision
            .checked_add(1)
            .ok_or(D1Error::RevisionOverflow)?;
        let input_hash = snapshot_hash(state, inputs);
        let outcome_hash = outcome_hash(
            self.program_hash,
            input_hash,
            state.revision,
            revision_after,
            &next_state,
            &registers,
        );
        Ok(D1Outcome {
            program_hash: self.program_hash,
            input_hash,
            revision_before: state.revision,
            revision_after,
            next_state,
            registers,
            outcome_hash,
        })
    }

    /// Recompute the transition and require its outcome hash to match the
    /// recorded hash. This is deterministic replay, not effect execution.
    pub fn replay(
        &self,
        state: &D1Snapshot,
        inputs: &InputSnapshot,
        expected_hash: [u8; 32],
    ) -> Result<D1Outcome, D1Error> {
        let outcome = self.execute(state, inputs)?;
        if outcome.outcome_hash != expected_hash {
            return Err(D1Error::ReplayMismatch);
        }
        Ok(outcome)
    }

    fn validate_snapshots(
        &self,
        state: &D1Snapshot,
        inputs: &InputSnapshot,
    ) -> Result<(), D1Error> {
        let touched: BTreeSet<String> = self
            .contract
            .read_keys
            .union(&self.contract.write_keys)
            .cloned()
            .collect();
        if state.values.keys().cloned().collect::<BTreeSet<_>>() != touched {
            return Err(D1Error::SnapshotSchemaMismatch);
        }
        if inputs.values.keys().cloned().collect::<BTreeSet<_>>() != self.contract.input_keys {
            return Err(D1Error::InputSchemaMismatch);
        }
        let combined = map_size(&state.values)?
            .checked_add(map_size(&inputs.values)?)
            .ok_or(D1Error::SnapshotTooLarge)?;
        if combined > MAX_SNAPSHOT_BYTES {
            return Err(D1Error::SnapshotTooLarge);
        }
        Ok(())
    }
}

/// Replayable deterministic state transition result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D1Outcome {
    pub program_hash: [u8; 32],
    pub input_hash: [u8; 32],
    pub revision_before: u64,
    pub revision_after: u64,
    pub next_state: BTreeMap<String, D1Value>,
    pub registers: BTreeMap<String, D1Value>,
    pub outcome_hash: [u8; 32],
}

/// D1 compiler/runtime failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D1Error {
    Classification(RejectReason),
    WrongClass,
    DeclaredEffects,
    ProofProcessMismatch,
    ProofClassMismatch,
    InvalidKey(String),
    TooManyKeys,
    TooManyInstructions,
    InvalidRegister(String),
    UndeclaredRead(String),
    UndeclaredWrite(String),
    UndeclaredInput(String),
    DuplicateWrite(String),
    MissingRegister(String),
    MissingValue(String),
    TypeMismatch,
    ArithmeticOverflow,
    SnapshotSchemaMismatch,
    InputSchemaMismatch,
    SnapshotTooLarge,
    ProgramTooLarge,
    RegisterLimit,
    TextTooLarge,
    RevisionOverflow,
    ReplayMismatch,
}

impl fmt::Display for D1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Classification(e) => write!(f, "D1 declaration rejected: {e}"),
            Self::WrongClass => f.write_str("program is not classified D1"),
            Self::DeclaredEffects => f.write_str("D1 state VM does not admit host effects"),
            Self::ProofProcessMismatch => f.write_str("proof process differs from declaration"),
            Self::ProofClassMismatch => f.write_str("proof class differs from declaration"),
            Self::InvalidKey(k) => write!(f, "invalid state or input key {k:?}"),
            Self::TooManyKeys => f.write_str("state contract exceeds key limit"),
            Self::TooManyInstructions => f.write_str("D1 instruction limit exceeded"),
            Self::InvalidRegister(r) => write!(f, "invalid register {r:?}"),
            Self::UndeclaredRead(k) => write!(f, "state read {k} is not declared"),
            Self::UndeclaredWrite(k) => write!(f, "state write {k} is not declared"),
            Self::UndeclaredInput(k) => write!(f, "input read {k} is not declared"),
            Self::DuplicateWrite(k) => write!(f, "state key {k} is written more than once"),
            Self::MissingRegister(r) => write!(f, "register {r} has no value"),
            Self::MissingValue(k) => write!(f, "snapshot value {k} is missing"),
            Self::TypeMismatch => f.write_str("instruction operand has the wrong value type"),
            Self::ArithmeticOverflow => f.write_str("integer addition overflow"),
            Self::SnapshotSchemaMismatch => {
                f.write_str("state snapshot keys do not match contract")
            }
            Self::InputSchemaMismatch => f.write_str("input snapshot keys do not match contract"),
            Self::SnapshotTooLarge => f.write_str("snapshot exceeds the byte limit"),
            Self::ProgramTooLarge => f.write_str("canonical D1 program exceeds the byte limit"),
            Self::RegisterLimit => f.write_str("D1 registers exceed the byte limit"),
            Self::TextTooLarge => f.write_str("text state value exceeds the byte limit"),
            Self::RevisionOverflow => f.write_str("state revision counter exhausted"),
            Self::ReplayMismatch => f.write_str("deterministic replay outcome hash differs"),
        }
    }
}

impl std::error::Error for D1Error {}

fn validate_program(contract: &StateContract, code: &[D1Instruction]) -> Result<(), D1Error> {
    let mut registers = BTreeSet::new();
    let mut written = BTreeSet::new();
    for instruction in code {
        match instruction {
            D1Instruction::Constant { dst, value } => {
                validate_register(dst)?;
                validate_value(value)?;
                registers.insert(dst.clone());
            }
            D1Instruction::ReadState { dst, key } => {
                validate_register(dst)?;
                if !contract.read_keys.contains(key) {
                    return Err(D1Error::UndeclaredRead(key.clone()));
                }
                registers.insert(dst.clone());
            }
            D1Instruction::ReadInput { dst, key } => {
                validate_register(dst)?;
                if !contract.input_keys.contains(key) {
                    return Err(D1Error::UndeclaredInput(key.clone()));
                }
                registers.insert(dst.clone());
            }
            D1Instruction::AddInt { dst, left, right } => {
                validate_register(dst)?;
                require_register(&registers, left)?;
                require_register(&registers, right)?;
                registers.insert(dst.clone());
            }
            D1Instruction::WriteState { key, src } => {
                if !contract.write_keys.contains(key) {
                    return Err(D1Error::UndeclaredWrite(key.clone()));
                }
                require_register(&registers, src)?;
                if !written.insert(key.clone()) {
                    return Err(D1Error::DuplicateWrite(key.clone()));
                }
            }
        }
    }
    Ok(())
}

fn require_register(registers: &BTreeSet<String>, name: &str) -> Result<(), D1Error> {
    validate_register(name)?;
    if registers.contains(name) {
        Ok(())
    } else {
        Err(D1Error::MissingRegister(name.to_string()))
    }
}

fn validate_register(name: &str) -> Result<(), D1Error> {
    if valid_symbol(name) {
        Ok(())
    } else {
        Err(D1Error::InvalidRegister(name.into()))
    }
}

fn validate_keys(keys: impl IntoIterator<Item = String>) -> Result<BTreeSet<String>, D1Error> {
    let mut validated = BTreeSet::new();
    for key in keys {
        if !valid_symbol(&key) {
            return Err(D1Error::InvalidKey(key));
        }
        validated.insert(key);
    }
    if validated.len() > MAX_CONTRACT_KEYS {
        return Err(D1Error::TooManyKeys);
    }
    Ok(validated)
}

fn valid_symbol(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
}

fn validate_snapshot_map(values: &BTreeMap<String, D1Value>) -> Result<(), D1Error> {
    if values.len() > MAX_CONTRACT_KEYS {
        return Err(D1Error::TooManyKeys);
    }
    for (key, value) in values {
        if !valid_symbol(key) {
            return Err(D1Error::InvalidKey(key.clone()));
        }
        validate_value(value)?;
    }
    if map_size(values)? > MAX_SNAPSHOT_BYTES {
        return Err(D1Error::SnapshotTooLarge);
    }
    Ok(())
}

fn map_size(values: &BTreeMap<String, D1Value>) -> Result<usize, D1Error> {
    values.iter().try_fold(8usize, |size, (key, value)| {
        size.checked_add(8)
            .and_then(|n| n.checked_add(key.len()))
            .and_then(|n| n.checked_add(value_size(value)))
            .ok_or(D1Error::SnapshotTooLarge)
    })
}

fn store_register(
    registers: &mut BTreeMap<String, D1Value>,
    current_bytes: &mut usize,
    name: &str,
    value: D1Value,
) -> Result<(), D1Error> {
    let old_size = registers.get(name).map_or(0, value_size);
    let next_size = current_bytes
        .checked_sub(old_size)
        .and_then(|n| n.checked_add(name.len()))
        .and_then(|n| n.checked_add(value_size(&value)))
        .ok_or(D1Error::RegisterLimit)?;
    if next_size > MAX_REGISTER_BYTES {
        return Err(D1Error::RegisterLimit);
    }
    registers.insert(name.to_string(), value);
    *current_bytes = next_size;
    Ok(())
}

fn encoded_program_size(
    proof: &str,
    contract: &StateContract,
    instructions: &[D1Instruction],
) -> Result<usize, D1Error> {
    let mut size = DOMAIN_PROGRAM
        .len()
        .checked_add(4 + 8 + proof.len())
        .ok_or(D1Error::ProgramTooLarge)?;
    for set in [
        &contract.read_keys,
        &contract.write_keys,
        &contract.input_keys,
    ] {
        size = size.checked_add(8).ok_or(D1Error::ProgramTooLarge)?;
        for key in set {
            size = size
                .checked_add(8 + key.len())
                .ok_or(D1Error::ProgramTooLarge)?;
        }
    }
    size = size.checked_add(8).ok_or(D1Error::ProgramTooLarge)?;
    for instruction in instructions {
        let instruction_size = match instruction {
            D1Instruction::Constant { dst, value } => 1 + 8 + dst.len() + encoded_value_size(value),
            D1Instruction::ReadState { dst, key } | D1Instruction::ReadInput { dst, key } => {
                1 + 8 + dst.len() + 8 + key.len()
            }
            D1Instruction::AddInt { dst, left, right } => {
                1 + 8 + dst.len() + 8 + left.len() + 8 + right.len()
            }
            D1Instruction::WriteState { key, src } => 1 + 8 + key.len() + 8 + src.len(),
        };
        size = size
            .checked_add(instruction_size)
            .ok_or(D1Error::ProgramTooLarge)?;
        if size > MAX_PROGRAM_BYTES {
            return Err(D1Error::ProgramTooLarge);
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

fn validate_value(value: &D1Value) -> Result<(), D1Error> {
    if matches!(value, D1Value::Text(text) if text.len() > MAX_TEXT_BYTES) {
        Err(D1Error::TextTooLarge)
    } else {
        Ok(())
    }
}

fn value_size(value: &D1Value) -> usize {
    match value {
        D1Value::Int(_) => 8,
        D1Value::Text(text) => text.len(),
        D1Value::Bool(_) => 1,
    }
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    put_u64(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

fn put_key_set(out: &mut Vec<u8>, values: &BTreeSet<String>) {
    put_u64(out, values.len() as u64);
    for value in values {
        put_str(out, value);
    }
}

fn put_map(out: &mut Vec<u8>, values: &BTreeMap<String, D1Value>) {
    put_u64(out, values.len() as u64);
    for (key, value) in values {
        put_str(out, key);
        put_value(out, value);
    }
}

fn put_value(out: &mut Vec<u8>, value: &D1Value) {
    match value {
        D1Value::Int(n) => {
            out.push(0);
            out.extend_from_slice(&n.to_le_bytes());
        }
        D1Value::Text(text) => {
            out.push(1);
            put_str(out, text);
        }
        D1Value::Bool(flag) => {
            out.push(2);
            out.push(u8::from(*flag));
        }
    }
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn snapshot_hash(state: &D1Snapshot, inputs: &InputSnapshot) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(DOMAIN_SNAPSHOT);
    put_u64(&mut bytes, state.revision);
    put_map(&mut bytes, &state.values);
    put_map(&mut bytes, &inputs.values);
    hash(&bytes)
}

fn outcome_hash(
    program_hash: [u8; 32],
    input_hash: [u8; 32],
    revision_before: u64,
    revision_after: u64,
    next_state: &BTreeMap<String, D1Value>,
    registers: &BTreeMap<String, D1Value>,
) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(DOMAIN_OUTCOME);
    bytes.extend_from_slice(&program_hash);
    bytes.extend_from_slice(&input_hash);
    put_u64(&mut bytes, revision_before);
    put_u64(&mut bytes, revision_after);
    put_map(&mut bytes, next_state);
    put_map(&mut bytes, registers);
    hash(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_determinism::EffectKind;

    fn declaration() -> ProcessDecl {
        ProcessDecl {
            name: "counter".into(),
            claims: DeterminismClass::StateDeterministic,
            declared_effects: Vec::new(),
            state_contract: true,
        }
    }

    fn proof() -> ExecutionProof {
        ExecutionProof {
            process: "counter".into(),
            class: DeterminismClass::StateDeterministic,
            compiler_version: "0.1.0".into(),
            schema_version: 2,
            policy_version: 3,
            target: "goya-d1-vm-v1".into(),
        }
    }

    fn contract() -> StateContract {
        StateContract::new(["count".into()], ["count".into()], ["increment".into()]).unwrap()
    }

    fn program() -> D1Program {
        D1Program::compile(
            &declaration(),
            &proof(),
            contract(),
            vec![
                D1Instruction::ReadState {
                    dst: "current".into(),
                    key: "count".into(),
                },
                D1Instruction::ReadInput {
                    dst: "delta".into(),
                    key: "increment".into(),
                },
                D1Instruction::AddInt {
                    dst: "next".into(),
                    left: "current".into(),
                    right: "delta".into(),
                },
                D1Instruction::WriteState {
                    key: "count".into(),
                    src: "next".into(),
                },
            ],
        )
        .unwrap()
    }

    fn snapshot(count: i64, revision: u64) -> (D1Snapshot, InputSnapshot) {
        let state = D1Snapshot::new(
            revision,
            BTreeMap::from([("count".into(), D1Value::Int(count))]),
        )
        .unwrap();
        let inputs =
            InputSnapshot::new(BTreeMap::from([("increment".into(), D1Value::Int(3))])).unwrap();
        (state, inputs)
    }

    #[test]
    fn d1_transition_is_bounded_deterministic_and_replayable() {
        let program = program();
        let (state, inputs) = snapshot(7, 4);
        let first = program.execute(&state, &inputs).unwrap();
        let second = program.execute(&state, &inputs).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.next_state["count"], D1Value::Int(10));
        assert_eq!(first.revision_before, 4);
        assert_eq!(first.revision_after, 5);
        assert_eq!(
            program.replay(&state, &inputs, first.outcome_hash).unwrap(),
            first
        );
        assert_eq!(
            program.replay(&state, &inputs, [0; 32]),
            Err(D1Error::ReplayMismatch)
        );
    }

    #[test]
    fn compile_rejects_wrong_class_effects_and_proof_mismatches() {
        let mut d0 = declaration();
        d0.claims = DeterminismClass::Pure;
        d0.state_contract = false;
        let mut d0_proof = proof();
        d0_proof.class = DeterminismClass::Pure;
        assert_eq!(
            D1Program::compile(&d0, &d0_proof, contract(), vec![]).err(),
            Some(D1Error::WrongClass)
        );

        let mut with_effect = declaration();
        with_effect.declared_effects = vec![EffectKind::Clock];
        assert_eq!(
            D1Program::compile(&with_effect, &proof(), contract(), vec![]).err(),
            Some(D1Error::DeclaredEffects)
        );

        let mut wrong_process = proof();
        wrong_process.process = "other".into();
        assert_eq!(
            D1Program::compile(&declaration(), &wrong_process, contract(), vec![]).err(),
            Some(D1Error::ProofProcessMismatch)
        );
    }

    #[test]
    fn compiler_enforces_explicit_reads_writes_inputs_and_register_order() {
        let invalid = vec![D1Instruction::ReadState {
            dst: "value".into(),
            key: "secret".into(),
        }];
        assert_eq!(
            D1Program::compile(&declaration(), &proof(), contract(), invalid).err(),
            Some(D1Error::UndeclaredRead("secret".into()))
        );

        let forward_ref = vec![D1Instruction::WriteState {
            key: "count".into(),
            src: "later".into(),
        }];
        assert_eq!(
            D1Program::compile(&declaration(), &proof(), contract(), forward_ref).err(),
            Some(D1Error::MissingRegister("later".into()))
        );
    }

    #[test]
    fn runtime_rejects_snapshot_schema_type_overflow_and_revision_overflow() {
        let program = program();
        let (state, inputs) = snapshot(1, 0);
        let wrong_inputs = InputSnapshot::default();
        assert_eq!(
            program.execute(&state, &wrong_inputs),
            Err(D1Error::InputSchemaMismatch)
        );

        let bad_inputs =
            InputSnapshot::new(BTreeMap::from([("increment".into(), D1Value::Bool(true))]))
                .unwrap();
        assert_eq!(
            program.execute(&state, &bad_inputs),
            Err(D1Error::TypeMismatch)
        );

        let (overflow_state, _) = snapshot(1, u64::MAX);
        assert_eq!(
            program.execute(&overflow_state, &inputs),
            Err(D1Error::RevisionOverflow)
        );
    }

    #[test]
    fn encoding_is_stable_and_checked_integer_overflow_fails() {
        let compiled = program();
        let rebuilt = program();
        assert_eq!(compiled.canonical_bytes(), rebuilt.canonical_bytes());
        assert_eq!(compiled.program_hash(), rebuilt.program_hash());
        let (state, inputs) = snapshot(i64::MAX, 1);
        assert_eq!(
            compiled.execute(&state, &inputs),
            Err(D1Error::ArithmeticOverflow)
        );
    }
}
