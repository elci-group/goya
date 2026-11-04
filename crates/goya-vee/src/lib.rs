//! Adapter to the ViCo Execution Environment (VEE).
//!
//! VEE serves a REST API on port 9987 (`POST /vee/submit` for submission).
//! We cannot link VEE's crate, so this module
//! is a self-contained network adapter over [`std::net::TcpStream`], std-only.
//!
//! Contract surface:
//! - [`VeeClient`] connects to a VEE host/port and [`VeeClient::submit`]s a
//!   [`SubmitRequest`], returning a [`VeeResponse`] or [`VeeError`].
//! - [`SubmitRequest::to_json`] is a tiny deterministic JSON writer
//!   (string/number fields only, quotes/backslashes escaped; no serde).
//! - Responses use a bounded JSON parser that checks object structure,
//!   duplicate keys, escapes, nesting, and value types before mapping submit
//!   and status responses into typed values.
//! - Offline degradation: a refused connection surfaces as
//!   [`VeeError::Connection`]; the adapter never panics. Use
//!   [`VeeClient::available`] to probe before submitting so the application
//!   can continue without VEE.

use goya_determinism::DeterminismClass;
use goya_executable::{
    execute, CompiledProgram, D1Error, D1Instruction, D1Program, D1Snapshot, D1Value, D2Error,
    D2Instruction, D2Program, EffectJournal, EffectResultType, InputSnapshot, Instruction, VmError,
    BYTECODE_VERSION,
};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Default TCP port VEE serves its REST API on.
pub const VEE_DEFAULT_PORT: u16 = 9987;

/// HTTP read/connect timeout applied per I/O operation.
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound untrusted VEE responses so a peer cannot exhaust the caller's memory.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_JSON_DEPTH: usize = 64;
const MAX_JSON_NODES: usize = 100_000;
/// The EDC bridge accepts only small declarative programs. These bounds are
/// checked before producing a source string, so untrusted artifacts cannot
/// cause unbounded allocation in this adapter.
const MAX_EDC_INSTRUCTIONS: usize = 2_048;
const MAX_EDC_STRING_BYTES: usize = 32 * 1024;
const MAX_EDC_SOURCE_BYTES: usize = 256 * 1024;
const MAX_D1_DATA_BYTES: usize = 32 * 1024;
const MAX_D2_PAYLOAD_BYTES: usize = 32 * 1024;
const MAX_D2_JOURNAL_BYTES: usize = 64 * 1024;
const MAX_D2_JOURNAL_ENTRIES: usize = 256;

/// Convert an already decoded and verified D1 program plus explicit snapshots
/// into a bounded VEE Python worker. The worker implements only D1's scalar
/// state/input instruction set and prints a typed, hash-bound outcome.
pub fn request_from_d1_program(
    program: &D1Program,
    state: &D1Snapshot,
    inputs: &InputSnapshot,
    agent_id: &str,
) -> Result<SubmitRequest, VeeCompileError> {
    validate_agent_id(agent_id)?;
    let expected = program
        .execute(state, inputs)
        .map_err(VeeCompileError::InvalidD1Program)?;
    if program.instructions().len() > MAX_EDC_INSTRUCTIONS {
        return Err(VeeCompileError::TooManyInstructions);
    }
    let mut data_bytes = 0usize;
    for map in [state.values(), inputs.values()] {
        for (key, value) in map {
            data_bytes = data_bytes
                .checked_add(key.len())
                .and_then(|n| n.checked_add(d1_value_size(value)))
                .ok_or(VeeCompileError::StringDataTooLarge)?;
        }
    }
    for instruction in program.instructions() {
        let strings = match instruction {
            D1Instruction::Constant { dst, value } => dst.len() + d1_value_size(value),
            D1Instruction::ReadState { dst, key } | D1Instruction::ReadInput { dst, key } => {
                dst.len() + key.len()
            }
            D1Instruction::AddInt { dst, left, right } => dst.len() + left.len() + right.len(),
            D1Instruction::WriteState { key, src } => key.len() + src.len(),
        };
        data_bytes = data_bytes
            .checked_add(strings)
            .ok_or(VeeCompileError::StringDataTooLarge)?;
        if data_bytes > MAX_D1_DATA_BYTES {
            return Err(VeeCompileError::StringDataTooLarge);
        }
    }

    let mut source = String::from("# Goya EDC D1 worker; deterministic snapshot transition.\nimport hashlib\nimport json\nGOYA_D1_VERSION = 1\nGOYA_D1_REVISION = ");
    source.push_str(&state.revision.to_string());
    source.push_str("\nGOYA_D1_PROGRAM_HASH = '");
    source.push_str(&hex_bytes(&program.program_hash()));
    source.push_str("'\nGOYA_D1_STATE = ");
    push_py_map(&mut source, state.values());
    source.push_str("\nGOYA_D1_INPUTS = ");
    push_py_map(&mut source, inputs.values());
    source.push_str("\nGOYA_D1_READ_KEYS = ");
    push_py_strings(&mut source, program.contract().read_keys());
    source.push_str("\nGOYA_D1_WRITE_KEYS = ");
    push_py_strings(&mut source, program.contract().write_keys());
    source.push_str("\nGOYA_D1_INPUT_KEYS = ");
    push_py_strings(&mut source, program.contract().input_keys());
    source.push_str("\nGOYA_D1_CODE = (\n");
    for instruction in program.instructions() {
        source.push_str("    ");
        match instruction {
            D1Instruction::Constant { dst, value } => {
                source.push_str("('const', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_py_value(&mut source, value);
                source.push_str("),");
            }
            D1Instruction::ReadState { dst, key } => {
                source.push_str("('state', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_python_string(&mut source, key);
                source.push_str("),");
            }
            D1Instruction::ReadInput { dst, key } => {
                source.push_str("('input', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_python_string(&mut source, key);
                source.push_str("),");
            }
            D1Instruction::AddInt { dst, left, right } => {
                source.push_str("('add', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_python_string(&mut source, left);
                source.push_str(", ");
                push_python_string(&mut source, right);
                source.push_str("),");
            }
            D1Instruction::WriteState { key, src } => {
                source.push_str("('write', ");
                push_python_string(&mut source, key);
                source.push_str(", ");
                push_python_string(&mut source, src);
                source.push_str("),");
            }
        }
        source.push('\n');
        if source.len() > MAX_EDC_SOURCE_BYTES {
            return Err(VeeCompileError::SourceTooLarge);
        }
    }
    source.push_str(")\n");
    source.push_str(D1_WORKER_PYTHON);
    if source.len() > MAX_EDC_SOURCE_BYTES {
        return Err(VeeCompileError::SourceTooLarge);
    }
    // Keep generation and Rust reference semantics coupled: the expected
    // outcome is not embedded in the worker; tests compare actual execution.
    let _ = expected;
    Ok(SubmitRequest {
        agent_id: agent_id.to_string(),
        language: "python".into(),
        source_code: source,
        capabilities: Vec::new(),
        budget: Some(VeeBudgetRequest {
            cpu_seconds: Some(3),
            memory_mb: Some(128),
            disk_mb: Some(64),
            token_budget: Some(1_024),
            wall_clock_seconds: Some(5),
        }),
    })
}

/// Convert a validated D2 program, exact snapshots, and caller-supplied
/// effect journal into a bounded VEE Python worker. The worker can consume
/// only the embedded journal records: it has no live effect or VEE capability
/// path, and fails on request/capability mismatch or unused records.
pub fn request_from_d2_program(
    program: &D2Program,
    state: &D1Snapshot,
    inputs: &InputSnapshot,
    journal: &EffectJournal,
    agent_id: &str,
) -> Result<SubmitRequest, VeeCompileError> {
    validate_agent_id(agent_id)?;
    let expected = program
        .execute(state, inputs, journal)
        .map_err(VeeCompileError::InvalidD2Program)?;
    if program.instructions().len() > MAX_EDC_INSTRUCTIONS {
        return Err(VeeCompileError::TooManyInstructions);
    }
    if journal.entries().len() > MAX_D2_JOURNAL_ENTRIES
        || journal.encoded_bytes() > MAX_D2_JOURNAL_BYTES
    {
        return Err(VeeCompileError::D2JournalTooLarge);
    }

    let mut payload_bytes = 0usize;
    for map in [state.values(), inputs.values()] {
        for (key, value) in map {
            payload_bytes = payload_bytes
                .checked_add(key.len())
                .and_then(|n| n.checked_add(d1_value_size(value)))
                .ok_or(VeeCompileError::StringDataTooLarge)?;
        }
    }
    for instruction in program.instructions() {
        let added = match instruction {
            D2Instruction::Constant { dst, value } => dst
                .len()
                .checked_add(d1_value_size(value))
                .ok_or(VeeCompileError::StringDataTooLarge)?,
            D2Instruction::ReadState { dst, key } | D2Instruction::ReadInput { dst, key } => {
                dst.len() + key.len()
            }
            D2Instruction::AddInt { dst, left, right } => dst.len() + left.len() + right.len(),
            D2Instruction::RequestEffect {
                dst,
                capability,
                request,
                ..
            } => dst.len() + capability.as_str().len() + request.len(),
            D2Instruction::WriteState { key, src } => key.len() + src.len(),
        };
        payload_bytes = payload_bytes
            .checked_add(added)
            .ok_or(VeeCompileError::StringDataTooLarge)?;
    }
    for entry in journal.entries() {
        payload_bytes = payload_bytes
            .checked_add(entry.capability.as_str().len())
            .and_then(|n| n.checked_add(entry.request.len()))
            .and_then(|n| n.checked_add(entry.result.len()))
            .ok_or(VeeCompileError::StringDataTooLarge)?;
    }
    if payload_bytes > MAX_D2_PAYLOAD_BYTES {
        return Err(VeeCompileError::D2PayloadTooLarge);
    }

    let mut source = String::from(
        "# Goya EDC D2 worker; journal replay only.\nimport hashlib\nimport json\nGOYA_D2_VERSION = 1\nGOYA_D2_REVISION = ",
    );
    source.push_str(&state.revision.to_string());
    source.push_str("\nGOYA_D2_PROGRAM_HASH = '");
    source.push_str(&hex_bytes(&program.program_hash()));
    source.push_str("'\nGOYA_D2_STATE = ");
    push_py_map(&mut source, state.values());
    source.push_str("\nGOYA_D2_INPUTS = ");
    push_py_map(&mut source, inputs.values());
    source.push_str("\nGOYA_D2_READ_KEYS = ");
    push_py_strings(&mut source, program.contract().read_keys());
    source.push_str("\nGOYA_D2_WRITE_KEYS = ");
    push_py_strings(&mut source, program.contract().write_keys());
    source.push_str("\nGOYA_D2_INPUT_KEYS = ");
    push_py_strings(&mut source, program.contract().input_keys());
    source.push_str("\nGOYA_D2_CODE = (\n");
    for instruction in program.instructions() {
        source.push_str("    ");
        match instruction {
            D2Instruction::Constant { dst, value } => {
                source.push_str("('const', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_py_value(&mut source, value);
                source.push_str("),");
            }
            D2Instruction::ReadState { dst, key } => {
                source.push_str("('state', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_python_string(&mut source, key);
                source.push_str("),");
            }
            D2Instruction::ReadInput { dst, key } => {
                source.push_str("('input', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_python_string(&mut source, key);
                source.push_str("),");
            }
            D2Instruction::AddInt { dst, left, right } => {
                source.push_str("('add', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_python_string(&mut source, left);
                source.push_str(", ");
                push_python_string(&mut source, right);
                source.push_str("),");
            }
            D2Instruction::RequestEffect {
                dst,
                capability,
                request,
                result_type,
            } => {
                source.push_str("('effect', ");
                push_python_string(&mut source, dst);
                source.push_str(", ");
                push_python_string(&mut source, capability.as_str());
                source.push_str(", bytes.fromhex('");
                source.push_str(&hex_bytes(request));
                source.push_str("'), ");
                source.push_str(&d2_result_type_tag(*result_type).to_string());
                source.push_str("),");
            }
            D2Instruction::WriteState { key, src } => {
                source.push_str("('write', ");
                push_python_string(&mut source, key);
                source.push_str(", ");
                push_python_string(&mut source, src);
                source.push_str("),");
            }
        }
        source.push('\n');
        if source.len() > MAX_EDC_SOURCE_BYTES {
            return Err(VeeCompileError::SourceTooLarge);
        }
    }
    source.push_str(")\nGOYA_D2_JOURNAL = (\n");
    for entry in journal.entries() {
        source.push_str("    (");
        source.push_str(&entry.sequence.to_string());
        source.push_str(", ");
        push_python_string(&mut source, entry.capability.as_str());
        source.push_str(", bytes.fromhex('");
        source.push_str(&hex_bytes(&entry.request));
        source.push_str("'), bytes.fromhex('");
        source.push_str(&hex_bytes(&entry.result));
        source.push_str("'), '");
        source.push_str(&hex_bytes(&entry.previous_hash));
        source.push_str("', '");
        source.push_str(&hex_bytes(&entry.hash));
        source.push_str("'),\n");
        if source.len() > MAX_EDC_SOURCE_BYTES {
            return Err(VeeCompileError::SourceTooLarge);
        }
    }
    source.push_str(")\n");
    source.push_str(D2_WORKER_PYTHON);
    if source.len() > MAX_EDC_SOURCE_BYTES {
        return Err(VeeCompileError::SourceTooLarge);
    }
    let _ = expected;
    Ok(SubmitRequest {
        agent_id: agent_id.to_string(),
        language: "python".into(),
        source_code: source,
        capabilities: Vec::new(),
        budget: Some(VeeBudgetRequest {
            cpu_seconds: Some(3),
            memory_mb: Some(128),
            disk_mb: Some(64),
            token_budget: Some(1_024),
            wall_clock_seconds: Some(5),
        }),
    })
}

const D1_WORKER_PYTHON: &str = r#"
def _u64(n): return int(n).to_bytes(8, 'little', signed=False)
def _str(s):
    b = s.encode('utf-8')
    return _u64(len(b)) + b
def _val(v):
    t, x = v
    if t == 'i': return b'\x00' + int(x).to_bytes(8, 'little', signed=True)
    if t == 's': return b'\x01' + _str(x)
    if t == 'b': return b'\x02' + bytes((1 if x else 0,))
    raise ValueError('invalid value tag')
def _map(m):
    return _u64(len(m)) + b''.join(_str(k) + _val(m[k]) for k in sorted(m))
def _value_size(v): return 8 if v[0] == 'i' else (len(v[1].encode('utf-8')) if v[0] == 's' else 1)
def _map_size(m): return 8 + sum(8 + len(k.encode('utf-8')) + (9 + len(v[1].encode('utf-8')) if v[0] == 's' else (9 if v[0] == 'i' else 2)) for k, v in m.items())
def _validate_map(m):
    if len(m) > 256: raise ValueError('snapshot key limit')
    for k, v in m.items():
        if not k or len(k.encode('utf-8')) > 65536: raise ValueError('invalid snapshot key')
        if v[0] == 's' and len(v[1].encode('utf-8')) > 65536: raise ValueError('text limit')
_validate_map(GOYA_D1_STATE)
_validate_map(GOYA_D1_INPUTS)
if set(GOYA_D1_STATE) != set(GOYA_D1_READ_KEYS) | set(GOYA_D1_WRITE_KEYS): raise ValueError('state schema mismatch')
if set(GOYA_D1_INPUTS) != set(GOYA_D1_INPUT_KEYS): raise ValueError('input schema mismatch')
if _map_size(GOYA_D1_STATE) + _map_size(GOYA_D1_INPUTS) > 1024 * 1024: raise ValueError('snapshot limit')
def _outval(v):
    t, x = v
    if t == 'i': return {'type':'int','value':str(x)}
    if t == 's': return {'type':'text','value':x}
    if t == 'b': return {'type':'bool','value':x}
    raise ValueError('invalid value tag')
_regs = {}
_reg_bytes = 0
_next = dict(GOYA_D1_STATE)
def _store(name, value):
    global _reg_bytes
    if value[0] == 's' and len(value[1].encode('utf-8')) > 65536: raise ValueError('text limit')
    size = len(name.encode('utf-8')) + _value_size(value)
    old = len(name.encode('utf-8')) + _value_size(_regs[name]) if name in _regs else 0
    if _reg_bytes - old + size > 1024 * 1024: raise ValueError('register limit')
    _regs[name] = value
    _reg_bytes = _reg_bytes - old + size
for _ins in GOYA_D1_CODE:
    _op = _ins[0]
    if _op == 'const': _store(_ins[1], _ins[2])
    elif _op == 'state': _store(_ins[1], GOYA_D1_STATE[_ins[2]])
    elif _op == 'input': _store(_ins[1], GOYA_D1_INPUTS[_ins[2]])
    elif _op == 'add':
        _a, _b = _regs[_ins[2]], _regs[_ins[3]]
        if _a[0] != 'i' or _b[0] != 'i': raise TypeError('integer operands required')
        _n = _a[1] + _b[1]
        if _n < -(1 << 63) or _n >= (1 << 63): raise OverflowError('i64 addition overflow')
        _store(_ins[1], ('i', _n))
    elif _op == 'write': _next[_ins[1]] = _regs[_ins[2]]
    else: raise ValueError('unknown verified D1 opcode')
_rev_after = GOYA_D1_REVISION + 1
if _rev_after >= (1 << 64): raise OverflowError('revision overflow')
_validate_map(_next)
if _map_size(_next) > 1024 * 1024: raise ValueError('output state limit')
_input_hash = hashlib.sha256(b'GOYA-D1-SNAPSHOT/1\x00' + _u64(GOYA_D1_REVISION) + _map(GOYA_D1_STATE) + _map(GOYA_D1_INPUTS)).digest()
_outcome = hashlib.sha256(b'GOYA-D1-OUTCOME/1\x00' + bytes.fromhex(GOYA_D1_PROGRAM_HASH) + _input_hash + _u64(GOYA_D1_REVISION) + _u64(_rev_after) + _map(_next) + _map(_regs)).hexdigest()
_result = {'program_hash':GOYA_D1_PROGRAM_HASH, 'input_hash':_input_hash.hex(), 'revision_before':str(GOYA_D1_REVISION), 'revision_after':str(_rev_after), 'next_state':{k:_outval(_next[k]) for k in sorted(_next)}, 'registers':{k:_outval(_regs[k]) for k in sorted(_regs)}, 'outcome_hash':_outcome}
print(json.dumps(_result, ensure_ascii=False, separators=(',', ':')))
"#;

const D2_WORKER_PYTHON: &str = r#"
MAX_JOURNAL_BYTES = 64 * 1024
MAX_JOURNAL_ENTRIES = 256
MAX_OUTPUT_BYTES = 2 * 1024 * 1024
def _u64(n):
    if n < 0 or n >= (1 << 64): raise ValueError('u64 range')
    return n.to_bytes(8, 'little', signed=False)
def _str(s):
    b = s.encode('utf-8')
    return _u64(len(b)) + b
def _field(b): return _u64(len(b)) + b
def _val(v):
    t, x = v
    if t == 'i': return b'\x00' + int(x).to_bytes(8, 'little', signed=True)
    if t == 's': return b'\x01' + _str(x)
    if t == 'b': return b'\x02' + bytes((1 if x else 0,))
    raise ValueError('invalid value tag')
def _map(m):
    return _u64(len(m)) + b''.join(_str(k) + _val(m[k]) for k in sorted(m))
def _map_size(m):
    return 8 + sum(8 + len(k.encode('utf-8')) + (9 + len(v[1].encode('utf-8')) if v[0] == 's' else (9 if v[0] == 'i' else 2)) for k, v in m.items())
def _validate_map(m):
    if len(m) > 256: raise ValueError('snapshot key limit')
    for k, v in m.items():
        if not k or len(k.encode('utf-8')) > 65536: raise ValueError('invalid snapshot key')
        if v[0] == 's' and len(v[1].encode('utf-8')) > 65536: raise ValueError('text limit')
        _val(v)
def _verify_journal():
    if len(GOYA_D2_JOURNAL) > MAX_JOURNAL_ENTRIES: raise ValueError('journal entry limit')
    previous = hashlib.sha256(b'GOYA-D2-EFFECT-JOURNAL-GENESIS/1\x00').digest()
    total = 0
    for i, entry in enumerate(GOYA_D2_JOURNAL):
        seq, capability, request, result, prev_hex, hash_hex = entry
        if seq != i or bytes.fromhex(prev_hex) != previous: raise ValueError('journal chain sequence')
        cap = capability.encode('utf-8')
        total += 80 + len(cap) + len(request) + len(result)
        if total > MAX_JOURNAL_BYTES: raise ValueError('journal byte limit')
        calculated = hashlib.sha256(b'GOYA-D2-EFFECT-JOURNAL/1\x00' + previous + _u64(seq) + _field(cap) + _field(request) + _field(result)).digest()
        if calculated.hex() != hash_hex: raise ValueError('journal hash mismatch')
        previous = calculated
_verify_journal()
_validate_map(GOYA_D2_STATE)
_validate_map(GOYA_D2_INPUTS)
if set(GOYA_D2_STATE) != set(GOYA_D2_READ_KEYS) | set(GOYA_D2_WRITE_KEYS): raise ValueError('state schema mismatch')
if set(GOYA_D2_INPUTS) != set(GOYA_D2_INPUT_KEYS): raise ValueError('input schema mismatch')
if _map_size(GOYA_D2_STATE) + _map_size(GOYA_D2_INPUTS) > 1024 * 1024: raise ValueError('snapshot limit')
_regs = {}
_reg_bytes = 0
_next = dict(GOYA_D2_STATE)
def _value_size(v): return 8 if v[0] == 'i' else (len(v[1].encode('utf-8')) if v[0] == 's' else 1)
def _store(name, value):
    global _reg_bytes
    if value[0] == 's' and len(value[1].encode('utf-8')) > 65536: raise ValueError('text limit')
    size = len(name.encode('utf-8')) + _value_size(value)
    old = len(name.encode('utf-8')) + _value_size(_regs[name]) if name in _regs else 0
    if _reg_bytes - old + size > 1024 * 1024: raise ValueError('register limit')
    _regs[name] = value
    _reg_bytes = _reg_bytes - old + size
def _effect_result(kind, raw):
    if len(raw) > 64 * 1024: raise ValueError('effect result limit')
    if kind == 1 and len(raw) == 9 and raw[0] == 1:
        return ('i', int.from_bytes(raw[1:], 'little', signed=True))
    if kind == 2 and len(raw) >= 5 and raw[0] == 2:
        size = int.from_bytes(raw[1:5], 'little', signed=False)
        if size != len(raw) - 5 or size > 64 * 1024: raise ValueError('malformed text effect result')
        return ('s', raw[5:].decode('utf-8'))
    if kind == 3 and raw in (b'\x03\x00', b'\x03\x01'):
        return ('b', raw[1] == 1)
    raise ValueError('malformed typed effect result')
_journal_cursor = 0
def _replay(capability, request):
    global _journal_cursor
    if _journal_cursor >= len(GOYA_D2_JOURNAL): raise ValueError('journal exhausted')
    seq, recorded_capability, recorded_request, result, _prev, _hash = GOYA_D2_JOURNAL[_journal_cursor]
    if seq != _journal_cursor or capability != recorded_capability or request != recorded_request:
        raise ValueError('journal request or capability mismatch')
    _journal_cursor += 1
    return result
for _ins in GOYA_D2_CODE:
    _op = _ins[0]
    if _op == 'const': _store(_ins[1], _ins[2])
    elif _op == 'state': _store(_ins[1], GOYA_D2_STATE[_ins[2]])
    elif _op == 'input': _store(_ins[1], GOYA_D2_INPUTS[_ins[2]])
    elif _op == 'add':
        _a, _b = _regs[_ins[2]], _regs[_ins[3]]
        if _a[0] != 'i' or _b[0] != 'i': raise TypeError('integer operands required')
        _n = _a[1] + _b[1]
        if _n < -(1 << 63) or _n >= (1 << 63): raise OverflowError('i64 addition overflow')
        _store(_ins[1], ('i', _n))
    elif _op == 'effect': _store(_ins[1], _effect_result(_ins[4], _replay(_ins[2], _ins[3])))
    elif _op == 'write': _next[_ins[1]] = _regs[_ins[2]]
    else: raise ValueError('unknown verified D2 opcode')
if _journal_cursor != len(GOYA_D2_JOURNAL): raise ValueError('journal has unused entries')
_rev_after = GOYA_D2_REVISION + 1
if _rev_after >= (1 << 64): raise OverflowError('revision overflow')
_validate_map(_next)
if _map_size(_next) > 1024 * 1024: raise ValueError('output state limit')
_input_hash = hashlib.sha256(b'GOYA-D2-VM-INPUT/1\x00' + _u64(GOYA_D2_REVISION) + _map(GOYA_D2_STATE) + _map(GOYA_D2_INPUTS)).digest()
_outcome = hashlib.sha256(b'GOYA-D2-VM-OUTCOME/1\x00' + bytes.fromhex(GOYA_D2_PROGRAM_HASH) + _input_hash + _u64(GOYA_D2_REVISION) + _u64(_rev_after) + _map(_next) + _map(_regs)).hexdigest()
def _outval(v):
    t, x = v
    if t == 'i': return {'type':'int','value':str(x)}
    if t == 's': return {'type':'text','value':x}
    if t == 'b': return {'type':'bool','value':x}
    raise ValueError('invalid value tag')
_result = {'program_hash':GOYA_D2_PROGRAM_HASH, 'input_hash':_input_hash.hex(), 'revision_before':str(GOYA_D2_REVISION), 'revision_after':str(_rev_after), 'next_state':{k:_outval(_next[k]) for k in sorted(_next)}, 'registers':{k:_outval(_regs[k]) for k in sorted(_regs)}, 'outcome_hash':_outcome}
_output = json.dumps(_result, ensure_ascii=False, separators=(',', ':'))
if len(_output.encode('utf-8')) > MAX_OUTPUT_BYTES: raise ValueError('worker output limit')
print(_output)
"#;

fn d2_result_type_tag(result_type: EffectResultType) -> u8 {
    match result_type {
        EffectResultType::Int64 => 1,
        EffectResultType::Text => 2,
        EffectResultType::Bool => 3,
    }
}

fn d1_value_size(value: &D1Value) -> usize {
    match value {
        D1Value::Int(_) => 9,
        D1Value::Text(v) => 9 + v.len(),
        D1Value::Bool(_) => 2,
    }
}
fn push_py_value(source: &mut String, value: &D1Value) {
    match value {
        D1Value::Int(n) => source.push_str(&format!("('i', {n})")),
        D1Value::Text(s) => {
            source.push_str("('s', ");
            push_python_string(source, s);
            source.push(')');
        }
        D1Value::Bool(b) => source.push_str(if *b { "('b', True)" } else { "('b', False)" }),
    }
}
fn push_py_map(source: &mut String, values: &std::collections::BTreeMap<String, D1Value>) {
    source.push_str("dict((");
    for (key, value) in values {
        source.push('(');
        push_python_string(source, key);
        source.push_str(", ");
        push_py_value(source, value);
        source.push_str("),");
    }
    source.push_str("))");
}
fn push_py_strings(source: &mut String, values: &std::collections::BTreeSet<String>) {
    source.push('(');
    for value in values {
        push_python_string(source, value);
        source.push(',');
    }
    source.push(')');
}
fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_agent_id(agent_id: &str) -> Result<(), VeeCompileError> {
    if agent_id.is_empty() || agent_id.len() > 128 || agent_id.chars().any(char::is_control) {
        Err(VeeCompileError::InvalidAgentId)
    } else {
        Ok(())
    }
}

fn json_string(value: &str) -> String {
    let mut output = String::new();
    push_json_string(&mut output, value);
    output
}

/// Convert a verified D0 bytecode artifact to a VEE Python request.
///
/// The generated Python interprets the validated UI instruction tuple and
/// prints a JSON serialization of the resulting tree. It uses only the
/// standard-library `json` module and stdout; it does not invoke a shell,
/// access files/network, or run arbitrary Goya code.
pub fn request_from_compiled_program(
    program: &CompiledProgram,
    agent_id: &str,
) -> Result<SubmitRequest, VeeCompileError> {
    if program.artifact.proof.class != DeterminismClass::Pure {
        return Err(VeeCompileError::UnsupportedClass);
    }
    if agent_id.is_empty() || agent_id.len() > 128 || agent_id.chars().any(char::is_control) {
        return Err(VeeCompileError::InvalidAgentId);
    }
    if program.bytecode.version != BYTECODE_VERSION {
        return Err(VeeCompileError::UnsupportedBytecodeVersion(
            program.bytecode.version,
        ));
    }
    if program.bytecode.instructions.len() > MAX_EDC_INSTRUCTIONS {
        return Err(VeeCompileError::TooManyInstructions);
    }
    let mut string_bytes = 0usize;
    for instruction in &program.bytecode.instructions {
        let added = match instruction {
            Instruction::Text { content, .. } => content.len(),
            Instruction::Button { label, .. } => label.len(),
            _ => 0,
        };
        string_bytes = string_bytes
            .checked_add(added)
            .ok_or(VeeCompileError::StringDataTooLarge)?;
        if string_bytes > MAX_EDC_STRING_BYTES {
            return Err(VeeCompileError::StringDataTooLarge);
        }
    }
    execute(&program.bytecode).map_err(VeeCompileError::InvalidBytecode)?;

    let mut source = String::from(
        "# Goya EDC D0 interpreter; bounded UI tree output.\n\
         GOYA_EDC_BYTECODE_VERSION = 1\nGOYA_EDC_INSTRUCTIONS = (\n",
    );
    for instruction in &program.bytecode.instructions {
        source.push_str("    ");
        match instruction {
            Instruction::BeginColumn { node, child_count } => {
                source.push_str(&format!("('column', {}, {}),", node.0, child_count));
            }
            Instruction::BeginRow { node, child_count } => {
                source.push_str(&format!("('row', {}, {}),", node.0, child_count));
            }
            Instruction::EndContainer => source.push_str("('end',),"),
            Instruction::Text { node, content } => {
                source.push_str(&format!("('text', {}, ", node.0));
                push_python_string(&mut source, content);
                source.push_str("),");
            }
            Instruction::Button {
                node,
                action,
                label,
            } => {
                source.push_str(&format!("('button', {}, {}, ", node.0, action.0));
                push_python_string(&mut source, label);
                source.push_str("),");
            }
            Instruction::Spacer { node, size_bits } => {
                source.push_str(&format!("('spacer_bits', {}, {}),", node.0, size_bits));
            }
        }
        source.push('\n');
        if source.len() > MAX_EDC_SOURCE_BYTES {
            return Err(VeeCompileError::SourceTooLarge);
        }
    }
    source.push_str(")\n");
    source.push_str(
        r#"import json
_frames = []
_root = None
for _ins in GOYA_EDC_INSTRUCTIONS:
    _op = _ins[0]
    if _op in ('column', 'row'):
        _frames.append([_op, _ins[1], _ins[2], []])
        continue
    if _op == 'end':
        _kind, _node, _expected, _children = _frames.pop()
        if len(_children) != _expected:
            raise ValueError('invalid child count')
        _item = {'kind': _kind, 'node': _node, 'children': _children}
    elif _op == 'text':
        _item = {'kind': 'text', 'node': _ins[1], 'content': _ins[2]}
    elif _op == 'button':
        _item = {'kind': 'button', 'node': _ins[1], 'action': _ins[2], 'label': _ins[3]}
    elif _op == 'spacer_bits':
        _item = {'kind': 'spacer', 'node': _ins[1], 'size_bits': _ins[2]}
    else:
        raise ValueError('invalid instruction')
    if _frames:
        _frames[-1][3].append(_item)
    elif _root is None:
        _root = _item
    else:
        raise ValueError('multiple roots')
if _frames or _root is None:
    raise ValueError('incomplete instruction stream')
print(json.dumps(_root, ensure_ascii=False, separators=(',', ':')))
"#,
    );
    if source.len() > MAX_EDC_SOURCE_BYTES {
        return Err(VeeCompileError::SourceTooLarge);
    }
    Ok(SubmitRequest {
        agent_id: agent_id.to_string(),
        language: "python".to_string(),
        source_code: source,
        capabilities: Vec::new(),
        budget: Some(VeeBudgetRequest {
            cpu_seconds: Some(3),
            memory_mb: Some(128),
            disk_mb: Some(64),
            token_budget: Some(1_024),
            wall_clock_seconds: Some(5),
        }),
    })
}

/// Why a compiled program could not be represented as a bounded VEE request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VeeCompileError {
    UnsupportedClass,
    UnsupportedBytecodeVersion(u32),
    InvalidAgentId,
    TooManyInstructions,
    StringDataTooLarge,
    SourceTooLarge,
    InvalidBytecode(VmError),
    InvalidD1Program(D1Error),
}

impl std::fmt::Display for VeeCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedClass => f.write_str("VEE bridge accepts D0 programs only"),
            Self::UnsupportedBytecodeVersion(version) => {
                write!(f, "unsupported bytecode version {version}")
            }
            Self::InvalidAgentId => {
                f.write_str("agent id must be 1-128 bytes without control characters")
            }
            Self::TooManyInstructions => {
                f.write_str("program exceeds the VEE bridge instruction limit")
            }
            Self::StringDataTooLarge => {
                f.write_str("program exceeds the VEE bridge string-data limit")
            }
            Self::SourceTooLarge => {
                f.write_str("generated Python exceeds the VEE bridge source limit")
            }
            Self::InvalidBytecode(error) => write!(f, "invalid bytecode: {error}"),
            Self::InvalidD1Program(error) => write!(f, "invalid D1 program or snapshots: {error}"),
        }
    }
}

impl std::error::Error for VeeCompileError {}

/// Append a Python single-quoted string literal without relying on `repr` or
/// external serializers. Non-ASCII characters are preserved as UTF-8.
fn push_python_string(out: &mut String, value: &str) {
    out.push('\'');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 || (0x7f..=0x9f).contains(&(ch as u32)) => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('\'');
}

/// Connection parameters for a VEE instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeeClient {
    pub host: String,
    pub port: u16,
    auth_token: Option<String>,
}

impl VeeClient {
    /// Build a client for the VEE instance at `host:port`.
    pub fn connect(host: &str, port: u16) -> VeeClient {
        VeeClient {
            host: host.to_string(),
            port,
            auth_token: None,
        }
    }

    /// Attach an API token for VEE deployments with authentication enabled.
    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Probe whether VEE is reachable. Never panics; returns `false` on any
    /// connection failure. Offline deployments use this to degrade gracefully.
    pub fn available(&mut self) -> bool {
        TcpStream::connect((self.host.as_str(), self.port)).is_ok()
    }

    /// Submit a task to VEE (`POST /vee/submit`).
    pub fn submit(&mut self, req: &SubmitRequest) -> Result<VeeResponse, VeeError> {
        let body = self.post_json("/vee/submit", &req.to_json())?;
        parse_submit_response(&body)
    }

    /// Fetch a full execution snapshot from VEE (`POST /vee/status`).
    ///
    /// The VEE server returns `{success:true,data:ExecutionResult}` while the
    /// execution exists, or a structured rejection when it does not.
    pub fn status(&mut self, execution_id: &str) -> Result<VeeExecutionResult, VeeError> {
        if execution_id.is_empty()
            || execution_id.len() > 256
            || execution_id.chars().any(char::is_control)
        {
            return Err(VeeError::InvalidRequest);
        }
        let mut body = String::from("{\"execution_id\":");
        push_json_string(&mut body, execution_id);
        body.push('}');
        let response = self.post_json("/vee/status", &body)?;
        parse_status_response(&response)
    }

    fn post_json(&mut self, path: &str, body: &str) -> Result<String, VeeError> {
        if self.host.chars().any(char::is_control)
            || self
                .auth_token
                .as_deref()
                .is_some_and(|token| token.is_empty() || token.chars().any(char::is_control))
        {
            return Err(VeeError::InvalidRequest);
        }
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| VeeError::Connection(e.to_string()))?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|e| VeeError::Connection(e.to_string()))?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|e| VeeError::Connection(e.to_string()))?;

        let auth = self
            .auth_token
            .as_ref()
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        let http = format!(
            "POST {path} HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             {auth}\
             Connection: close\r\n\
             \r\n\
             {}",
            self.host,
            self.port,
            body.len(),
            body
        );
        stream
            .write_all(http.as_bytes())
            .map_err(|e| VeeError::Connection(e.to_string()))?;
        stream
            .flush()
            .map_err(|e| VeeError::Connection(e.to_string()))?;

        let raw = read_response(&mut stream)?;
        let status_code = parse_status_code(&raw).ok_or(VeeError::Protocol)?;
        if !(200..300).contains(&status_code) {
            return Err(VeeError::Protocol);
        }
        extract_body(&raw).ok_or(VeeError::Protocol)
    }
}

/// Budget fields supported by VEE's submit endpoint. Missing fields use VEE defaults.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VeeBudgetRequest {
    pub cpu_seconds: Option<u64>,
    pub memory_mb: Option<u64>,
    pub disk_mb: Option<u64>,
    pub token_budget: Option<u64>,
    pub wall_clock_seconds: Option<u64>,
}

/// A task submission payload matching VEE's `VeeSubmitRequest` contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitRequest {
    pub agent_id: String,
    pub language: String,
    pub source_code: String,
    pub capabilities: Vec<String>,
    pub budget: Option<VeeBudgetRequest>,
}

impl SubmitRequest {
    /// Deterministic JSON for VEE's `/vee/submit` request schema.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(64 + self.source_code.len());
        out.push('{');
        out.push_str("\"agent_id\":");
        push_json_string(&mut out, &self.agent_id);
        out.push_str(",\"language\":");
        push_json_string(&mut out, &self.language);
        out.push_str(",\"source_code\":");
        push_json_string(&mut out, &self.source_code);
        out.push_str(",\"capabilities\":[");
        for (index, capability) in self.capabilities.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            push_json_string(&mut out, capability);
        }
        out.push(']');
        if let Some(budget) = &self.budget {
            out.push_str(",\"budget\":{");
            let fields = [
                ("cpu_seconds", budget.cpu_seconds),
                ("memory_mb", budget.memory_mb),
                ("disk_mb", budget.disk_mb),
                ("token_budget", budget.token_budget),
                ("wall_clock_seconds", budget.wall_clock_seconds),
            ];
            let mut first = true;
            for (name, value) in fields {
                if let Some(value) = value {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    push_json_string(&mut out, name);
                    out.push(':');
                    out.push_str(&value.to_string());
                }
            }
            out.push('}');
        }
        out.push('}');
        out
    }
}

/// Append a JSON string literal, escaping `"`, `\`, and control characters.
fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Result of a successful submit call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeeResponse {
    /// The application's success field, when returned by VEE.
    pub success: Option<bool>,
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub error: Option<String>,
    /// Full response body, kept so callers can re-parse if the naive scan
    /// extracted the wrong field (see module docs).
    pub raw: String,
}

/// VEE execution lifecycle states returned by `/vee/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VeeExecutionState {
    Pending,
    Queued,
    Simulating,
    Executing,
    Validating,
    Completed,
    Failed,
    Recovered,
    Cancelled,
}

/// VEE execution phases returned by `/vee/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VeeExecutionPhase {
    Hypothesis,
    Simulation,
    Execution,
    Validation,
    Feedback,
}

/// JSON data retained for result fields whose schema varies by artifact type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VeeJsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<VeeJsonValue>),
    Object(Vec<(String, VeeJsonValue)>),
}

/// Typed `/vee/status` execution snapshot. Artifact and validation payloads
/// remain bounded structured JSON because their sub-schemas are polymorphic.
#[derive(Debug, Clone, PartialEq)]
pub struct VeeExecutionResult {
    pub execution_id: String,
    pub project_id: Option<String>,
    pub status: VeeExecutionState,
    pub phase: VeeExecutionPhase,
    pub artifacts: Vec<VeeJsonValue>,
    pub validation: Option<VeeJsonValue>,
    pub confidence: f64,
    pub tokens_consumed: u64,
    pub cpu_seconds_used: f64,
    pub memory_peak_mb: f64,
    pub latency_ms: u64,
    pub error_log: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Errors from the adapter. The adapter never panics; all failures map here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VeeError {
    /// TCP connect/write failed (e.g. VEE not running / connection refused).
    Connection(String),
    /// Read timed out.
    Timeout,
    /// Non-2xx status, unparseable HTTP framing, or malformed body.
    Protocol,
    /// Request contains a value unsafe for an HTTP header.
    InvalidRequest,
    /// VEE returned a valid application-level task rejection.
    Rejected(String),
}

/// Read the HTTP response until `Content-Length` bytes of body are received
/// or the connection closes.
fn read_response(stream: &mut TcpStream) -> Result<String, VeeError> {
    let mut raw: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    // Header terminator within the accumulated bytes.
    let mut header_end: Option<usize> = None;
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                return match e.kind() {
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                        Err(VeeError::Timeout)
                    }
                    _ => Err(VeeError::Connection(e.to_string())),
                };
            }
        };
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > MAX_RESPONSE_BYTES {
            return Err(VeeError::Protocol);
        }
        if header_end.is_none() {
            if let Some(pos) = find_subslice(&raw, b"\r\n\r\n") {
                header_end = Some(pos + 4);
                if pos + 4 > MAX_HEADER_BYTES {
                    return Err(VeeError::Protocol);
                }
            } else if raw.len() > MAX_HEADER_BYTES {
                return Err(VeeError::Protocol);
            }
        }
        if let Some(start) = header_end {
            if let Some(len) = parse_content_length(&raw[..start]) {
                if raw.len() - start >= len {
                    break;
                }
            }
        }
    }
    String::from_utf8(raw).map_err(|_| VeeError::Protocol)
}

/// Byte-oriented substring search.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Extract the `Content-Length` value from header bytes (case-insensitive).
fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(headers).ok()?;
    for line in text.split("\r\n") {
        if let Some(rest) = strip_prefix_ignore_case(line, "content-length:") {
            return rest.trim().parse::<usize>().ok();
        }
    }
    None
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Parse `HTTP/1.1 NNN ...` from the raw response.
fn parse_status_code(raw: &str) -> Option<u16> {
    let line = raw.split("\r\n").next()?;
    let mut parts = line.split_whitespace();
    let version = parts.next()?;
    if !version.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse::<u16>().ok()
}

/// Split off the body after the `\r\n\r\n` header terminator.
fn extract_body(raw: &str) -> Option<String> {
    let idx = raw.find("\r\n\r\n")?;
    Some(raw[idx + 4..].to_string())
}

fn parse_submit_response(body: &str) -> Result<VeeResponse, VeeError> {
    let value = parse_json(body).ok_or(VeeError::Protocol)?;
    let success = object_get(&value, "success")
        .and_then(as_bool)
        .ok_or(VeeError::Protocol)?;
    let error = optional_string_field(&value, "error").map_err(|_| VeeError::Protocol)?;
    if !success {
        return Err(VeeError::Rejected(
            error.unwrap_or_else(|| "VEE rejected the task".to_string()),
        ));
    }
    let task_id = optional_string_field(&value, "execution_id")
        .map_err(|_| VeeError::Protocol)?
        .or(optional_string_field(&value, "id").map_err(|_| VeeError::Protocol)?);
    let status = optional_string_field(&value, "status").map_err(|_| VeeError::Protocol)?;
    Ok(VeeResponse {
        success: Some(true),
        task_id,
        status,
        error,
        raw: body.to_string(),
    })
}

fn parse_status_response(body: &str) -> Result<VeeExecutionResult, VeeError> {
    let value = parse_json(body).ok_or(VeeError::Protocol)?;
    let success = object_get(&value, "success")
        .and_then(as_bool)
        .ok_or(VeeError::Protocol)?;
    if !success {
        let error = optional_string_field(&value, "error")
            .map_err(|_| VeeError::Protocol)?
            .unwrap_or_else(|| "VEE rejected the status request".to_string());
        return Err(VeeError::Rejected(error));
    }
    let data = object_get(&value, "data").ok_or(VeeError::Protocol)?;
    parse_execution_result(data).ok_or(VeeError::Protocol)
}

fn parse_execution_result(value: &VeeJsonValue) -> Option<VeeExecutionResult> {
    Some(VeeExecutionResult {
        execution_id: string_field(value, "execution_id")?.to_string(),
        project_id: optional_string_field(value, "project_id").ok()?,
        status: parse_execution_state(string_field(value, "status")?)?,
        phase: parse_execution_phase(string_field(value, "phase")?)?,
        artifacts: match object_get(value, "artifacts")? {
            VeeJsonValue::Array(items) => items.clone(),
            _ => return None,
        },
        validation: match object_get(value, "validation")? {
            VeeJsonValue::Null => None,
            value => Some(value.clone()),
        },
        confidence: number_field(value, "confidence")?,
        tokens_consumed: unsigned_field(value, "tokens_consumed")?,
        cpu_seconds_used: number_field(value, "cpu_seconds_used")?,
        memory_peak_mb: number_field(value, "memory_peak_mb")?,
        latency_ms: unsigned_field(value, "latency_ms")?,
        error_log: optional_string_field(value, "error_log").ok()?,
        created_at: string_field(value, "created_at")?.to_string(),
        started_at: optional_string_field(value, "started_at").ok()?,
        completed_at: optional_string_field(value, "completed_at").ok()?,
    })
}

fn parse_execution_state(value: &str) -> Option<VeeExecutionState> {
    Some(match value {
        "Pending" => VeeExecutionState::Pending,
        "Queued" => VeeExecutionState::Queued,
        "Simulating" => VeeExecutionState::Simulating,
        "Executing" => VeeExecutionState::Executing,
        "Validating" => VeeExecutionState::Validating,
        "Completed" => VeeExecutionState::Completed,
        "Failed" => VeeExecutionState::Failed,
        "Recovered" => VeeExecutionState::Recovered,
        "Cancelled" => VeeExecutionState::Cancelled,
        _ => return None,
    })
}

fn parse_execution_phase(value: &str) -> Option<VeeExecutionPhase> {
    Some(match value {
        "Hypothesis" => VeeExecutionPhase::Hypothesis,
        "Simulation" => VeeExecutionPhase::Simulation,
        "Execution" => VeeExecutionPhase::Execution,
        "Validation" => VeeExecutionPhase::Validation,
        "Feedback" => VeeExecutionPhase::Feedback,
        _ => return None,
    })
}

fn object_get<'a>(value: &'a VeeJsonValue, key: &str) -> Option<&'a VeeJsonValue> {
    match value {
        VeeJsonValue::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, v)| v),
        _ => None,
    }
}

fn as_bool(value: &VeeJsonValue) -> Option<bool> {
    match value {
        VeeJsonValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn string_field<'a>(value: &'a VeeJsonValue, key: &str) -> Option<&'a str> {
    match object_get(value, key)? {
        VeeJsonValue::String(value) => Some(value),
        _ => None,
    }
}

fn optional_string_field(value: &VeeJsonValue, key: &str) -> Result<Option<String>, ()> {
    match object_get(value, key) {
        None | Some(VeeJsonValue::Null) => Ok(None),
        Some(VeeJsonValue::String(value)) => Ok(Some(value.clone())),
        _ => Err(()),
    }
}

fn number_field(value: &VeeJsonValue, key: &str) -> Option<f64> {
    let VeeJsonValue::Number(number) = object_get(value, key)? else {
        return None;
    };
    let value = number.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn unsigned_field(value: &VeeJsonValue, key: &str) -> Option<u64> {
    let VeeJsonValue::Number(number) = object_get(value, key)? else {
        return None;
    };
    number.parse::<u64>().ok()
}

fn parse_json(input: &str) -> Option<VeeJsonValue> {
    let mut parser = JsonParser {
        input,
        pos: 0,
        nodes: 0,
    };
    let value = parser.value(0)?;
    parser.whitespace();
    (parser.pos == input.len()).then_some(value)
}

struct JsonParser<'a> {
    input: &'a str,
    pos: usize,
    nodes: usize,
}

impl JsonParser<'_> {
    fn whitespace(&mut self) {
        while self.pos < self.input.len()
            && matches!(
                self.input.as_bytes()[self.pos],
                b' ' | b'\n' | b'\r' | b'\t'
            )
        {
            self.pos += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Option<VeeJsonValue> {
        if depth > MAX_JSON_DEPTH || self.nodes >= MAX_JSON_NODES {
            return None;
        }
        self.nodes += 1;
        self.whitespace();
        match *self.input.as_bytes().get(self.pos)? {
            b'n' => {
                self.literal("null")?;
                Some(VeeJsonValue::Null)
            }
            b't' => {
                self.literal("true")?;
                Some(VeeJsonValue::Bool(true))
            }
            b'f' => {
                self.literal("false")?;
                Some(VeeJsonValue::Bool(false))
            }
            b'"' => Some(VeeJsonValue::String(self.string()?)),
            b'[' => self.array(depth + 1),
            b'{' => self.object(depth + 1),
            b'-' | b'0'..=b'9' => Some(VeeJsonValue::Number(self.number()?)),
            _ => None,
        }
    }

    fn literal(&mut self, expected: &str) -> Option<()> {
        let end = self.pos.checked_add(expected.len())?;
        if self.input.get(self.pos..end)? != expected {
            return None;
        }
        self.pos = end;
        Some(())
    }

    fn array(&mut self, depth: usize) -> Option<VeeJsonValue> {
        self.pos += 1;
        self.whitespace();
        let mut values = Vec::new();
        if self.input.as_bytes().get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Some(VeeJsonValue::Array(values));
        }
        loop {
            values.push(self.value(depth)?);
            self.whitespace();
            match self.input.as_bytes().get(self.pos)? {
                b',' => {
                    self.pos += 1;
                }
                b']' => {
                    self.pos += 1;
                    break;
                }
                _ => return None,
            }
        }
        Some(VeeJsonValue::Array(values))
    }

    fn object(&mut self, depth: usize) -> Option<VeeJsonValue> {
        self.pos += 1;
        self.whitespace();
        let mut fields = Vec::new();
        let mut keys = HashSet::new();
        if self.input.as_bytes().get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Some(VeeJsonValue::Object(fields));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            if !keys.insert(key.clone()) {
                return None;
            }
            self.whitespace();
            if self.input.as_bytes().get(self.pos) != Some(&b':') {
                return None;
            }
            self.pos += 1;
            let value = self.value(depth)?;
            fields.push((key, value));
            self.whitespace();
            match self.input.as_bytes().get(self.pos)? {
                b',' => {
                    self.pos += 1;
                }
                b'}' => {
                    self.pos += 1;
                    break;
                }
                _ => return None,
            }
        }
        Some(VeeJsonValue::Object(fields))
    }

    fn string(&mut self) -> Option<String> {
        if self.input.as_bytes().get(self.pos) != Some(&b'"') {
            return None;
        }
        self.pos += 1;
        let mut out = String::new();
        loop {
            let byte = *self.input.as_bytes().get(self.pos)?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Some(out);
                }
                b'\\' => {
                    self.pos += 1;
                    match *self.input.as_bytes().get(self.pos)? {
                        b'"' => {
                            out.push('"');
                            self.pos += 1;
                        }
                        b'\\' => {
                            out.push('\\');
                            self.pos += 1;
                        }
                        b'/' => {
                            out.push('/');
                            self.pos += 1;
                        }
                        b'b' => {
                            out.push('\u{0008}');
                            self.pos += 1;
                        }
                        b'f' => {
                            out.push('\u{000c}');
                            self.pos += 1;
                        }
                        b'n' => {
                            out.push('\n');
                            self.pos += 1;
                        }
                        b'r' => {
                            out.push('\r');
                            self.pos += 1;
                        }
                        b't' => {
                            out.push('\t');
                            self.pos += 1;
                        }
                        b'u' => {
                            self.pos += 1;
                            let first = self.hex4()?;
                            let scalar = if (0xd800..=0xdbff).contains(&first) {
                                if self.input.as_bytes().get(self.pos..self.pos + 2)? != b"\\u" {
                                    return None;
                                }
                                self.pos += 2;
                                let second = self.hex4()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return None;
                                }
                                0x10000
                                    + ((u32::from(first) - 0xd800) << 10)
                                    + (u32::from(second) - 0xdc00)
                            } else if (0xdc00..=0xdfff).contains(&first) {
                                return None;
                            } else {
                                u32::from(first)
                            };
                            out.push(char::from_u32(scalar)?);
                        }
                        _ => return None,
                    }
                }
                0x00..=0x1f => return None,
                _ => {
                    let ch = self.input.get(self.pos..)?.chars().next()?;
                    self.pos += ch.len_utf8();
                    out.push(ch);
                }
            }
        }
    }

    fn hex4(&mut self) -> Option<u16> {
        let end = self.pos.checked_add(4)?;
        let digits = self.input.get(self.pos..end)?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        self.pos = end;
        u16::from_str_radix(digits, 16).ok()
    }

    fn number(&mut self) -> Option<String> {
        let start = self.pos;
        if self.input.as_bytes().get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        match self.input.as_bytes().get(self.pos)? {
            b'0' => self.pos += 1,
            b'1'..=b'9' => {
                self.pos += 1;
                while self
                    .input
                    .as_bytes()
                    .get(self.pos)
                    .is_some_and(u8::is_ascii_digit)
                {
                    self.pos += 1;
                }
            }
            _ => return None,
        }
        if self.input.as_bytes().get(self.pos) == Some(&b'.') {
            self.pos += 1;
            let fraction_start = self.pos;
            while self
                .input
                .as_bytes()
                .get(self.pos)
                .is_some_and(u8::is_ascii_digit)
            {
                self.pos += 1;
            }
            if self.pos == fraction_start {
                return None;
            }
        }
        if matches!(self.input.as_bytes().get(self.pos), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.input.as_bytes().get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exponent_start = self.pos;
            while self
                .input
                .as_bytes()
                .get(self.pos)
                .is_some_and(u8::is_ascii_digit)
            {
                self.pos += 1;
            }
            if self.pos == exponent_start {
                return None;
            }
        }
        Some(self.input.get(start..self.pos)?.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_determinism::proofs::ExecutionProof;
    use goya_determinism::ProcessDecl;
    use goya_ir::{ActionId, Module, Node, NodeId};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn test_request(source: &str) -> SubmitRequest {
        SubmitRequest {
            agent_id: "goya-test".to_string(),
            capabilities: vec!["process_spawn".to_string(), "network_read".to_string()],
            language: "python".to_string(),
            source_code: source.to_string(),
            budget: Some(VeeBudgetRequest {
                cpu_seconds: Some(3),
                memory_mb: Some(128),
                disk_mb: Some(64),
                token_budget: Some(800),
                wall_clock_seconds: Some(5),
            }),
        }
    }

    fn compiled_module(module: &Module) -> CompiledProgram {
        goya_executable::compile_verified(
            module,
            &ProcessDecl::new("vee-export", DeterminismClass::Pure, vec![]),
            &ExecutionProof {
                process: "vee-export".into(),
                class: DeterminismClass::Pure,
                compiler_version: "test".into(),
                schema_version: 1,
                policy_version: 1,
                target: "test".into(),
            },
        )
        .expect("compile test program")
    }

    fn compiled_text(content: &str) -> CompiledProgram {
        compiled_module(&Module {
            nodes: vec![(
                NodeId(1),
                Node::Text {
                    content: content.into(),
                },
            )],
            root: Some(NodeId(1)),
        })
    }

    fn d1_program() -> D1Program {
        let declaration = ProcessDecl {
            name: "counter".into(),
            claims: DeterminismClass::StateDeterministic,
            declared_effects: vec![],
            state_contract: true,
        };
        let proof = ExecutionProof {
            process: "counter".into(),
            class: DeterminismClass::StateDeterministic,
            compiler_version: "test".into(),
            schema_version: 1,
            policy_version: 1,
            target: "vee-d1-v1".into(),
        };
        let contract = goya_executable::StateContract::new(
            ["count".into()],
            ["count".into()],
            ["increment".into()],
        )
        .unwrap();
        let code = vec![
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
        ];
        let compiled = D1Program::compile(&declaration, &proof, contract, code).unwrap();
        D1Program::decode(&compiled.canonical_bytes(), compiled.program_hash()).unwrap()
    }

    fn d1_snapshot() -> (D1Snapshot, InputSnapshot) {
        (
            D1Snapshot::new(
                4,
                std::collections::BTreeMap::from([("count".into(), D1Value::Int(7))]),
            )
            .unwrap(),
            InputSnapshot::new(std::collections::BTreeMap::from([(
                "increment".into(),
                D1Value::Int(3),
            )]))
            .unwrap(),
        )
    }

    fn expected_execution_json(node: &goya_executable::ExecutionNode, out: &mut String) {
        use goya_executable::ExecutionNode;
        match node {
            ExecutionNode::Column { node, children } => {
                expected_container_json("column", node.0, children, out);
            }
            ExecutionNode::Row { node, children } => {
                expected_container_json("row", node.0, children, out);
            }
            ExecutionNode::Text { node, content } => {
                out.push_str("{\"kind\":\"text\",\"node\":");
                out.push_str(&node.0.to_string());
                out.push_str(",\"content\":");
                push_json_string(out, content);
                out.push('}');
            }
            ExecutionNode::Button {
                node,
                action,
                label,
            } => {
                out.push_str("{\"kind\":\"button\",\"node\":");
                out.push_str(&node.0.to_string());
                out.push_str(",\"action\":");
                out.push_str(&action.0.to_string());
                out.push_str(",\"label\":");
                push_json_string(out, label);
                out.push('}');
            }
            ExecutionNode::Spacer { node, size } => {
                out.push_str("{\"kind\":\"spacer\",\"node\":");
                out.push_str(&node.0.to_string());
                out.push_str(",\"size_bits\":");
                out.push_str(&size.to_bits().to_string());
                out.push('}');
            }
        }
    }

    fn expected_container_json(
        kind: &str,
        node: u64,
        children: &[goya_executable::ExecutionNode],
        out: &mut String,
    ) {
        out.push_str("{\"kind\":\"");
        out.push_str(kind);
        out.push_str("\",\"node\":");
        out.push_str(&node.to_string());
        out.push_str(",\"children\":[");
        for (index, child) in children.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            expected_execution_json(child, out);
        }
        out.push_str("]}");
    }

    /// Spawn a one-shot mock server: accepts one connection, optionally
    /// checks the request via `inspect`, replies with `response`, and drains
    /// remaining request bytes first.
    fn spawn_mock(response: &'static str) -> u16 {
        spawn_mock_with(response, |_| {})
    }

    fn spawn_mock_with<F: FnOnce(String) + Send + 'static>(
        response: &'static str,
        inspect: F,
    ) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local addr").port();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            // Read until end of POST body using Content-Length.
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let header_end = loop {
                let n = stream.read(&mut buf).expect("read request");
                assert!(n > 0, "client closed before sending headers");
                raw.extend_from_slice(&buf[..n]);
                if let Some(pos) = find_subslice(&raw, b"\r\n\r\n") {
                    break pos + 4;
                }
            };
            let headers = String::from_utf8_lossy(&raw[..header_end]).to_string();
            let len: usize = headers
                .lines()
                .find_map(|l| {
                    let lower = l.to_ascii_lowercase();
                    lower
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().expect("content-length"))
                })
                .expect("content-length header");
            while raw.len() - header_end < len {
                let n = stream.read(&mut buf).expect("read body");
                assert!(n > 0, "client closed before sending body");
                raw.extend_from_slice(&buf[..n]);
            }
            inspect(String::from_utf8_lossy(&raw).to_string());
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        port
    }

    const CANNED_OK: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"success\":true,\"execution_id\":\"t-1\",\"status\":\"pending\",\"message\":\"ok\"}";

    #[test]
    fn json_writer_basic_and_field_order() {
        let json = test_request("HALT").to_json();
        assert_eq!(
            json,
            "{\"agent_id\":\"goya-test\",\"language\":\"python\",\"source_code\":\"HALT\",\"capabilities\":[\"process_spawn\",\"network_read\"],\"budget\":{\"cpu_seconds\":3,\"memory_mb\":128,\"disk_mb\":64,\"token_budget\":800,\"wall_clock_seconds\":5}}"
        );
    }

    #[test]
    fn d0_program_becomes_deterministic_inert_python_request() {
        let program = compiled_text("hi 'there'\\\nworld");
        let first = request_from_compiled_program(&program, "goya-test").unwrap();
        let second = request_from_compiled_program(&program, "goya-test").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.language, "python");
        assert!(first.capabilities.is_empty());
        assert_eq!(first.budget.as_ref().unwrap().cpu_seconds, Some(3));
        assert!(first
            .source_code
            .starts_with("# Goya EDC D0 interpreter; bounded UI tree output."));
        assert!(first
            .source_code
            .contains("('text', 1, 'hi \\'there\\'\\\\\\nworld'),"));
        assert!(first.source_code.contains("GOYA_EDC_BYTECODE_VERSION = 1"));
        assert!(first.source_code.contains("print(json.dumps(_root"));
        assert!(!first.source_code.contains("exec("));
        assert!(!first.source_code.contains("subprocess"));
    }

    #[test]
    fn decoded_d1_worker_matches_reference_outcome_and_is_inert() {
        use std::io::Write as _;
        use std::process::{Command, Stdio};

        let program = d1_program();
        let (state, inputs) = d1_snapshot();
        let expected = program.execute(&state, &inputs).unwrap();
        let request = request_from_d1_program(&program, &state, &inputs, "goya-d1-test").unwrap();
        assert_eq!(request.language, "python");
        assert!(request.capabilities.is_empty());
        assert!(request.source_code.contains("GOYA_D1_VERSION = 1"));
        assert!(!request.source_code.contains("subprocess"));
        assert!(!request.source_code.contains("open("));
        let probe = Command::new("python3").arg("--version").output();
        match probe {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("failed to probe python3: {error}"),
            Ok(output) => assert!(output.status.success()),
        }
        let mut child = Command::new("python3")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request.source_code.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "worker failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = String::from_utf8(output.stdout).unwrap();
        let typed = |values: &std::collections::BTreeMap<String, D1Value>| -> String {
            values
                .iter()
                .map(|(key, value)| {
                    let val = match value {
                        D1Value::Int(n) => format!("{{\"type\":\"int\",\"value\":\"{n}\"}}"),
                        D1Value::Text(s) => {
                            format!("{{\"type\":\"text\",\"value\":{}}}", json_string(s))
                        }
                        D1Value::Bool(b) => format!("{{\"type\":\"bool\",\"value\":{b}}}"),
                    };
                    format!("{}:{val}", json_string(key))
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let expected_json = format!("{{\"program_hash\":\"{}\",\"input_hash\":\"{}\",\"revision_before\":\"{}\",\"revision_after\":\"{}\",\"next_state\":{{{}}},\"registers\":{{{}}},\"outcome_hash\":\"{}\"}}",
            hex_bytes(&expected.program_hash), hex_bytes(&expected.input_hash), expected.revision_before, expected.revision_after,
            typed(&expected.next_state), typed(&expected.registers), hex_bytes(&expected.outcome_hash));
        assert_eq!(actual.trim_end(), expected_json);
    }

    #[test]
    fn generated_python_interpreter_matches_reference_vm_json() {
        use std::io::Write as _;
        use std::process::{Command, Stdio};

        let program = compiled_module(&Module {
            nodes: vec![
                (
                    NodeId(1),
                    Node::Column {
                        children: vec![NodeId(2), NodeId(5)],
                    },
                ),
                (
                    NodeId(2),
                    Node::Row {
                        children: vec![NodeId(3), NodeId(4)],
                    },
                ),
                (
                    NodeId(3),
                    Node::Text {
                        content: "hello 'VEE'\nλ".into(),
                    },
                ),
                (
                    NodeId(4),
                    Node::Button {
                        label: "activate\\now".into(),
                        on_activate: ActionId(9),
                    },
                ),
                (NodeId(5), Node::Spacer { size: 2.5 }),
            ],
            root: Some(NodeId(1)),
        });
        let reference = execute(&program.bytecode).expect("reference VM");
        let mut expected = String::new();
        expected_execution_json(&reference, &mut expected);

        let probe = Command::new("python3").arg("--version").output();
        match probe {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("failed to probe python3: {error}"),
            Ok(output) => assert!(output.status.success(), "python3 --version failed"),
        }

        let request = request_from_compiled_program(&program, "goya-test").unwrap();
        let mut child = Command::new("python3")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn python3");
        child
            .stdin
            .take()
            .expect("python stdin")
            .write_all(request.source_code.as_bytes())
            .expect("write generated source");
        let output = child.wait_with_output().expect("wait for python3");
        assert!(
            output.status.success(),
            "python interpreter failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = String::from_utf8(output.stdout).expect("python output is UTF-8");
        assert_eq!(actual.strip_suffix('\n').unwrap_or(&actual), expected);
    }

    #[test]
    fn d0_bridge_rejects_non_d0_bad_bytecode_and_oversized_strings() {
        let mut non_d0 = compiled_text("small");
        non_d0.artifact.proof.class = DeterminismClass::StateDeterministic;
        assert_eq!(
            request_from_compiled_program(&non_d0, "agent"),
            Err(VeeCompileError::UnsupportedClass)
        );

        let mut tampered = compiled_text("small");
        tampered.bytecode.content_hash ^= 1;
        assert_eq!(
            request_from_compiled_program(&tampered, "agent"),
            Err(VeeCompileError::InvalidBytecode(VmError::HashMismatch))
        );
        assert_eq!(
            request_from_compiled_program(
                &compiled_text(&"x".repeat(MAX_EDC_STRING_BYTES + 1)),
                "agent"
            ),
            Err(VeeCompileError::StringDataTooLarge)
        );
    }

    #[test]
    fn d0_bridge_validates_agent_id() {
        let program = compiled_text("small");
        assert_eq!(
            request_from_compiled_program(&program, "bad\nid"),
            Err(VeeCompileError::InvalidAgentId)
        );
        assert_eq!(
            request_from_compiled_program(&program, ""),
            Err(VeeCompileError::InvalidAgentId)
        );
    }

    #[test]
    fn json_writer_escapes_quotes_and_backslashes() {
        let req = test_request("say \"hi\"\\end\nline");
        let json = req.to_json();
        assert!(json.contains("\"source_code\":\"say \\\"hi\\\"\\\\end\\nline\""));
        // Must still contain the literal characters after structured parsing.
        assert_eq!(
            string_field(&parse_json(&json).unwrap(), "source_code"),
            Some("say \"hi\"\\end\nline")
        );
    }

    #[test]
    fn json_writer_control_chars_are_escaped() {
        let req = test_request("\t\r");
        let json = req.to_json();
        assert!(json.contains("\\t\\r"));
        assert!(!json.contains('\t'));
    }

    #[test]
    fn submit_roundtrip_via_mock() {
        let port = spawn_mock(CANNED_OK);
        let mut client = VeeClient::connect("127.0.0.1", port);
        let resp = client.submit(&test_request("HALT")).expect("submit ok");
        assert_eq!(resp.task_id.as_deref(), Some("t-1"));
        assert_eq!(resp.success, Some(true));
        assert_eq!(resp.status.as_deref(), Some("pending"));
        assert_eq!(
            resp.raw,
            "{\"success\":true,\"execution_id\":\"t-1\",\"status\":\"pending\",\"message\":\"ok\"}"
        );
    }

    #[test]
    fn request_bytes_have_path_and_content_length() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = captured.clone();
        let port = spawn_mock_with(CANNED_OK, move |req| {
            *sink.lock().expect("lock") = req;
        });
        let mut client = VeeClient::connect("127.0.0.1", port);
        let body = test_request("HALT").to_json();
        client.submit(&test_request("HALT")).expect("submit ok");

        let req = captured.lock().expect("lock").clone();
        assert!(
            req.starts_with("POST /vee/submit HTTP/1.1\r\n"),
            "wrong request line: {req}"
        );
        let expected_cl = format!("Content-Length: {}\r\n", body.len());
        assert!(
            req.contains(&expected_cl),
            "missing/incorrect Content-Length in: {req}"
        );
        assert!(req.contains("Content-Type: application/json"));
        assert!(req.ends_with(&body));
    }

    #[test]
    fn request_uses_current_vee_fields_and_optional_bearer_auth() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = captured.clone();
        let port = spawn_mock_with(CANNED_OK, move |req| {
            *sink.lock().expect("lock") = req;
        });
        let mut client = VeeClient::connect("127.0.0.1", port).with_bearer_token("submit-token");
        client.submit(&test_request("AQID")).expect("submit ok");
        let req = captured.lock().expect("lock").clone();
        assert!(req.contains("Authorization: Bearer submit-token\r\n"));
        assert!(req.contains("\"agent_id\":\"goya-test\""));
        assert!(req.contains("\"capabilities\":[\"process_spawn\",\"network_read\"]"));
        assert!(req.contains("\"wall_clock_seconds\":5"));
        assert!(req.contains("\"cpu_seconds\":3"));
    }

    #[test]
    fn application_rejection_is_reported_and_header_injection_refused() {
        let port = spawn_mock("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"success\":false,\"error\":\"unsupported language\"}");
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert_eq!(
            client.submit(&test_request("HALT")),
            Err(VeeError::Rejected("unsupported language".into()))
        );
        let mut bad =
            VeeClient::connect("127.0.0.1", 9987).with_bearer_token("ok\r\nX-Injected: yes");
        assert_eq!(
            bad.submit(&test_request("HALT")),
            Err(VeeError::InvalidRequest)
        );
    }

    #[test]
    fn absent_budget_is_omitted_and_response_execution_id_is_used() {
        let mut req = test_request("print('hello')");
        req.budget = None;
        assert_eq!(
            req.to_json(),
            "{\"agent_id\":\"goya-test\",\"language\":\"python\",\"source_code\":\"print('hello')\",\"capabilities\":[\"process_spawn\",\"network_read\"]}"
        );
        let port = spawn_mock(CANNED_OK);
        let mut client = VeeClient::connect("127.0.0.1", port);
        let resp = client.submit(&req).expect("submit ok");
        assert_eq!(resp.task_id.as_deref(), Some("t-1"));
        assert_eq!(resp.success, Some(true));
    }

    #[test]
    fn connection_refused_is_error_not_panic() {
        // Bind then immediately drop a listener to get a free, unused port.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").port()
        };
        let mut client = VeeClient::connect("127.0.0.1", port);
        match client.submit(&test_request("HALT")) {
            Err(VeeError::Connection(_)) => {}
            other => panic!("expected Connection error, got {other:?}"),
        }
        // available() must report false without panicking.
        assert!(!client.available());
    }

    #[test]
    fn malformed_http_is_protocol_error() {
        let port = spawn_mock("not http at all\r\n\r\n");
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert_eq!(
            client.submit(&test_request("HALT")),
            Err(VeeError::Protocol)
        );
    }

    #[test]
    fn http_error_status_is_protocol_error() {
        let port = spawn_mock("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 2\r\n\r\n{}");
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert_eq!(
            client.submit(&test_request("HALT")),
            Err(VeeError::Protocol)
        );
    }

    #[test]
    fn malformed_submit_response_is_protocol_error() {
        let port = spawn_mock("HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"other\":1}");
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert_eq!(
            client.submit(&test_request("HALT")),
            Err(VeeError::Protocol)
        );
    }

    #[test]
    fn available_true_against_mock() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        // Listener accepted the probe then closed is fine; probe just needs
        // the TCP handshake, so spawn a tiny accept loop.
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                drop(stream);
            }
        });
        let mut client = VeeClient::connect("127.0.0.1", port);
        assert!(client.available());
    }

    #[test]
    fn structured_json_parser_handles_escapes_nesting_and_duplicate_rejection() {
        let value =
            parse_json("{\"id\":\"a\\\"b\\u03bb\",\"nested\":{\"id\":\"c\"},\"status\":\"ok\"}")
                .unwrap();
        assert_eq!(string_field(&value, "id"), Some("a\"bλ"));
        assert_eq!(string_field(&value, "status"), Some("ok"));
        assert!(parse_json("{\"success\":true,\"success\":false}").is_none());
        assert!(parse_json("[1,]").is_none());
        assert!(parse_json("\"\\uD800\"").is_none());
    }

    #[test]
    fn status_roundtrip_via_mock_returns_typed_execution_snapshot() {
        const STATUS_OK: &str = "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"success\":true,\"data\":{\"execution_id\":\"exec-7\",\"project_id\":\"project-a\",\"status\":\"Completed\",\"phase\":\"Validation\",\"artifacts\":[{\"type\":\"Text\",\"data\":{\"content\":\"done\"}}],\"validation\":null,\"confidence\":0.9,\"tokens_consumed\":12,\"cpu_seconds_used\":0.25,\"memory_peak_mb\":8.0,\"latency_ms\":251,\"error_log\":null,\"created_at\":\"2026-11-04T12:00:00Z\",\"started_at\":null,\"completed_at\":\"2026-11-04T12:00:01Z\"}}";
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let sink = captured.clone();
        let port = spawn_mock_with(STATUS_OK, move |req| *sink.lock().unwrap() = req);
        let mut client = VeeClient::connect("127.0.0.1", port);
        let status = client.status("exec-7").expect("status ok");
        let req = captured.lock().unwrap().clone();
        assert!(req.starts_with("POST /vee/status HTTP/1.1\r\n"));
        assert!(req.ends_with("{\"execution_id\":\"exec-7\"}"));
        assert_eq!(status.execution_id, "exec-7");
        assert_eq!(status.status, VeeExecutionState::Completed);
        assert_eq!(status.phase, VeeExecutionPhase::Validation);
        assert_eq!(status.artifacts.len(), 1);
        assert_eq!(status.confidence, 0.9);
        assert_eq!(status.tokens_consumed, 12);
        assert_eq!(status.latency_ms, 251);
        assert_eq!(status.started_at, None);
        assert_eq!(status.completed_at.as_deref(), Some("2026-11-04T12:00:01Z"));
    }

    #[test]
    fn status_errors_and_unknown_states_are_rejected() {
        let missing = spawn_mock("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"success\":false,\"error\":\"Execution not found\"}");
        let mut client = VeeClient::connect("127.0.0.1", missing);
        assert_eq!(
            client.status("nope"),
            Err(VeeError::Rejected("Execution not found".into()))
        );

        let unknown = spawn_mock("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{\"success\":true,\"data\":{\"execution_id\":\"x\",\"status\":\"future-status\"}}");
        let mut client = VeeClient::connect("127.0.0.1", unknown);
        assert_eq!(client.status("x"), Err(VeeError::Protocol));
    }

    #[test]
    fn status_validates_execution_id_before_connecting() {
        let mut client = VeeClient::connect("127.0.0.1", 1);
        assert_eq!(client.status("bad\nid"), Err(VeeError::InvalidRequest));
        assert_eq!(client.status(""), Err(VeeError::InvalidRequest));
    }

    #[test]
    fn response_read_respects_content_length_or_close() {
        // Response without Content-Length: adapter reads to connection close.
        let port = spawn_mock(
            "HTTP/1.1 200 OK\r\n\r\n{\"success\":true,\"id\":\"t-2\",\"status\":\"queued\"}",
        );
        let mut client = VeeClient::connect("127.0.0.1", port);
        let resp = client.submit(&test_request("HALT")).expect("submit ok");
        assert_eq!(resp.task_id.as_deref(), Some("t-2"));
    }
}
