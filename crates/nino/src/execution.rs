//! Bounded, diagnostic ARM execution from ROM. No guest timing or peripherals.
//! Unsupported state is returned to the caller, never treated as an instruction
//! that succeeded. This is a declared subset, not a compatibility fallback.

use crate::{analysis::Mode, armv4t::Flags};
use serde::Serialize;

pub const MAX_STEPS: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct State {
    /// R15 is the address of the next instruction to fetch.
    pub registers: [u32; 16],
    pub flags: Flags,
    pub mode: Mode,
}

impl Default for State {
    fn default() -> Self {
        let mut registers = [0; 16];
        registers[15] = 0x0800_0000;
        Self {
            registers,
            flags: Flags::default(),
            mode: Mode::Arm,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Transition {
    pub address: u32,
    pub mode: Mode,
    pub source_range: [usize; 2],
    pub opcode: u32,
    pub memory_access: Option<crate::arm_transfer::Transfer>,
    pub after: State,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Stop {
    InstructionLimit,
    InvalidFetch,
    MemoryFault(crate::memory::Error),
    UnsupportedInstruction { opcode: u32, detail: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Execution {
    pub source_sha256: String,
    pub initial: State,
    pub final_state: State,
    pub initial_ram: crate::memory::RamSnapshot,
    pub final_ram: crate::memory::RamSnapshot,
    pub transitions: Vec<Transition>,
    pub stop: Stop,
    pub guest_cycles_available: bool,
}

#[derive(Debug)]
pub enum Error {
    Rom(crate::Error),
    StepLimit,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rom(e) => e.fmt(f),
            Self::StepLimit => write!(f, "step limit must be between 1 and {MAX_STEPS}"),
        }
    }
}
impl std::error::Error for Error {}

/// Run from explicit state. The default zeroed register state is a fixture
/// entry condition, not a model of BIOS startup. Fetches use only the first
/// ROM mapping. B/BL and BX execute here; ARM data-processing delegates to
/// `arm_data`. Thumb arithmetic and branches delegate to `thumb_data`.
pub fn run(rom: &[u8], initial: State, steps: usize) -> Result<Execution, Error> {
    let mut memory = crate::memory::Memory::new(rom).map_err(Error::Rom)?;
    run_with_memory(&mut memory, initial, steps)
}

/// Continue a diagnostic session using the caller's authoritative RAM banks.
/// Keep this bus and pass the returned `final_state` when resuming. `run`
/// creates fresh zeroed RAM each time; it is intended for standalone fixtures.
pub fn run_with_memory(
    memory: &mut crate::memory::Memory<'_>,
    initial: State,
    steps: usize,
) -> Result<Execution, Error> {
    if steps == 0 || steps > MAX_STEPS {
        return Err(Error::StepLimit);
    }
    let identity = crate::inspect(memory.rom()).map_err(Error::Rom)?;
    let initial_ram = memory.snapshot();
    let mut state = initial.clone();
    let mut transitions = Vec::new();
    let mut stop = Stop::InstructionLimit;
    for _ in 0..steps {
        let rom = memory.rom();
        let pc = state.registers[15];
        if state.mode == Mode::Thumb {
            let offset = pc.checked_sub(0x0800_0000).map(|v| v as usize);
            let bytes = offset
                .and_then(|offset| offset.checked_add(2).and_then(|end| rom.get(offset..end)));
            let Some(bytes) = bytes.filter(|_| pc.is_multiple_of(2)) else {
                stop = Stop::InvalidFetch;
                break;
            };
            let opcode = u16::from_le_bytes(bytes.try_into().expect("halfword fetch"));
            let mut memory_access = None;
            if crate::thumb_transfer::is_transfer(opcode) {
                match crate::thumb_transfer::step(&mut state.registers, memory, opcode) {
                    Ok(access) => memory_access = Some(access),
                    Err(crate::thumb_transfer::Error::Bus(error)) => {
                        stop = Stop::MemoryFault(error);
                        break;
                    }
                    Err(error) => {
                        stop = Stop::UnsupportedInstruction {
                            opcode: u32::from(opcode),
                            detail: error.to_string(),
                        };
                        break;
                    }
                }
            } else {
                match crate::thumb_data::step(&mut state.registers, &mut state.flags, opcode) {
                    Ok(mode) => state.mode = mode,
                    Err(error) => {
                        stop = Stop::UnsupportedInstruction {
                            opcode: u32::from(opcode),
                            detail: error.to_string(),
                        };
                        break;
                    }
                }
            }
            let offset = offset.expect("validated fetch");
            transitions.push(Transition {
                address: pc,
                mode: Mode::Thumb,
                source_range: [offset, offset + 2],
                opcode: u32::from(opcode),
                memory_access,
                after: state.clone(),
            });
            continue;
        }
        let offset = pc.checked_sub(0x0800_0000).map(|v| v as usize);
        let bytes =
            offset.and_then(|offset| offset.checked_add(4).and_then(|end| rom.get(offset..end)));
        let Some(bytes) = bytes.filter(|_| pc.is_multiple_of(4)) else {
            stop = Stop::InvalidFetch;
            break;
        };
        let opcode = u32::from_le_bytes(bytes.try_into().expect("four-byte fetch"));
        let condition = (opcode >> 28) as u8;
        if condition == 15 {
            stop = Stop::UnsupportedInstruction {
                opcode,
                detail: "NV encoding is unsupported".into(),
            };
            break;
        }
        let branch = (opcode >> 25) & 7 == 5;
        let exchange = opcode & 0x0fff_fff0 == 0x012f_ff10;
        let mut memory_access = None;
        if branch || exchange {
            if state
                .flags
                .condition(condition)
                .expect("four-bit condition")
            {
                if branch {
                    let displacement = (((opcode & 0x00ff_ffff) << 8) as i32 >> 6) as u32;
                    if opcode & (1 << 24) != 0 {
                        state.registers[14] = pc.wrapping_add(4);
                    }
                    state.registers[15] = pc.wrapping_add(8).wrapping_add(displacement);
                } else {
                    let rm = (opcode & 15) as usize;
                    let target = if rm == 15 {
                        pc.wrapping_add(8)
                    } else {
                        state.registers[rm]
                    };
                    state.mode = if target & 1 != 0 {
                        Mode::Thumb
                    } else {
                        Mode::Arm
                    };
                    state.registers[15] = target & if state.mode == Mode::Thumb { !1 } else { !3 };
                }
            } else {
                state.registers[15] = pc.wrapping_add(4);
            }
        } else if opcode & 0x0c00_0000 == 0x0400_0000 {
            match crate::arm_transfer::step(&mut state.registers, state.flags, memory, opcode) {
                Ok(access) => memory_access = access,
                Err(crate::arm_transfer::Error::Bus(error)) => {
                    stop = Stop::MemoryFault(error);
                    break;
                }
                Err(error) => {
                    stop = Stop::UnsupportedInstruction {
                        opcode,
                        detail: error.to_string(),
                    };
                    break;
                }
            }
        } else if let Err(error) =
            crate::arm_data::step(&mut state.registers, &mut state.flags, opcode)
        {
            stop = Stop::UnsupportedInstruction {
                opcode,
                detail: error.to_string(),
            };
            break;
        }
        let offset = offset.expect("validated fetch");
        transitions.push(Transition {
            address: pc,
            mode: Mode::Arm,
            source_range: [offset, offset + 4],
            opcode,
            memory_access,
            after: state.clone(),
        });
    }
    Ok(Execution {
        source_sha256: identity.source_sha256,
        initial,
        final_state: state,
        initial_ram,
        final_ram: memory.snapshot(),
        transitions,
        stop,
        guest_cycles_available: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_sp_transfers_replay_with_shared_ram() {
        let rom: Vec<_> = [0x202au16, 0x9000, 0x9900, 0xdf00]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut initial = State {
            mode: Mode::Thumb,
            ..State::default()
        };
        initial.registers[13] = 0x0300_0100;
        let full = run(&rom, initial.clone(), 10).unwrap();
        assert_eq!(full.final_state.registers[1], 42);
        assert_eq!(full.final_ram.iwram[0x100], 42);
        assert_eq!(
            full.transitions[1].memory_access.as_ref().unwrap().address,
            0x0300_0100
        );
        let mut bus = crate::memory::Memory::new(&rom).unwrap();
        let partial = run_with_memory(&mut bus, initial, 2).unwrap();
        let resumed = run_with_memory(&mut bus, partial.final_state, 10).unwrap();
        assert_eq!(resumed.final_state, full.final_state);
        assert_eq!(resumed.final_ram, full.final_ram);
        assert_eq!(resumed.transitions, full.transitions[2..]);
    }

    #[test]
    fn ram_transfers_resume_and_bus_fault_preserves_instruction_state() {
        // MOV r0,#0x02000000; MOV r1,#42; STR r1,[r0]; LDR r2,[r0]; SWI
        let rom: Vec<_> = [
            0xe3a0_0402u32,
            0xe3a0_102a,
            0xe580_1000,
            0xe590_2000,
            0xef00_0000,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
        let full = run(&rom, State::default(), 20).unwrap();
        assert_eq!(full.final_state.registers[2], 42);
        assert_eq!(full.final_ram.ewram[0], 42);
        assert!(full.transitions[2].memory_access.as_ref().unwrap().write);
        let mut bus = crate::memory::Memory::new(&rom).unwrap();
        let first = run_with_memory(&mut bus, State::default(), 3).unwrap();
        let resumed = run_with_memory(&mut bus, first.final_state, 20).unwrap();
        assert_eq!(resumed.final_state, full.final_state);
        assert_eq!(resumed.final_ram, full.final_ram);
        assert_eq!(resumed.transitions, full.transitions[3..]);
        let mut initial = State::default();
        initial.registers[0] = 0x0400_0000;
        let failed = run(&0xe5b0_1004u32.to_le_bytes(), initial.clone(), 1).unwrap();
        assert!(matches!(failed.stop, Stop::MemoryFault(_)));
        assert_eq!(failed.final_state, initial);
        assert_eq!(failed.initial_ram, failed.final_ram);
        assert!(failed.transitions.is_empty());
    }

    #[test]
    fn mixed_modes_execute_thumb_loop_and_return_to_arm() {
        let mut rom = Vec::new();
        // ARM BX r0; ARM SWI; Thumb MOV/ADD/CMP/BNE/BX lr.
        rom.extend(0xe12f_ff10u32.to_le_bytes());
        rom.extend(0xef00_0000u32.to_le_bytes());
        for word in [0x2100u16, 0x3101, 0x2903, 0xd1fc, 0x4770] {
            rom.extend(word.to_le_bytes());
        }
        let mut initial = State::default();
        initial.registers[0] = 0x0800_0009;
        initial.registers[14] = 0x0800_0004;
        let result = run(&rom, initial.clone(), 50).unwrap();
        assert_eq!(result.final_state.registers[1], 3);
        assert_eq!(result.final_state.registers[15], 0x0800_0004);
        assert_eq!(result.final_state.mode, Mode::Arm);
        assert_eq!(result.transitions[1].source_range, [8, 10]);
        assert_eq!(result.transitions[1].mode, Mode::Thumb);
        assert!(matches!(
            result.stop,
            Stop::UnsupportedInstruction {
                opcode: 0xef00_0000,
                ..
            }
        ));
        let partial = run(&rom, initial, 5).unwrap();
        let resumed = run(&rom, partial.final_state, 50).unwrap();
        assert_eq!(resumed.final_state, result.final_state);
        assert_eq!(resumed.transitions, result.transitions[5..]);
    }

    #[test]
    fn loop_replays_and_resumes_at_the_same_state() {
        // MOV r0,#0; ADD r0,r0,#1; CMP r0,#3; BNE -16; SWI 0
        let words = [
            0xe3a0_0000u32,
            0xe280_0001,
            0xe350_0003,
            0x1aff_fffc,
            0xef00_0000,
        ];
        let rom: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let full = run(&rom, State::default(), 100).unwrap();
        assert_eq!(full.final_state.registers[0], 3);
        assert_eq!(full.final_state.registers[15], 0x0800_0010);
        assert_eq!(full.transitions.len(), 10);
        assert!(matches!(full.stop, Stop::UnsupportedInstruction { .. }));
        assert_eq!(full, run(&rom, State::default(), 100).unwrap());
        let partial = run(&rom, State::default(), 5).unwrap();
        assert_eq!(partial.stop, Stop::InstructionLimit);
        let resumed = run(&rom, partial.final_state, 100).unwrap();
        assert_eq!(resumed.final_state, full.final_state);
        assert_eq!(resumed.transitions, full.transitions[5..]);
    }

    #[test]
    fn branch_link_exchange_and_fetch_failures_are_explicit() {
        let rom: Vec<_> = [0xeb00_0000u32, 0xef00_0000, 0xe12f_ff1e]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let result = run(&rom, State::default(), 5).unwrap();
        assert_eq!(result.final_state.registers[14], 0x0800_0004);
        assert_eq!(result.final_state.registers[15], 0x0800_0004);
        let mut state = State::default();
        state.registers[0] = 0x0800_0009;
        let result = run(&0xe12f_ff10u32.to_le_bytes(), state, 2).unwrap();
        assert_eq!(result.stop, Stop::InvalidFetch);
        assert_eq!(result.final_state.mode, Mode::Thumb);
        assert_eq!(result.final_state.registers[15], 0x0800_0008);
        let result = run(&[0; 3], State::default(), 1).unwrap();
        assert_eq!(result.stop, Stop::InvalidFetch);
        assert!(result.transitions.is_empty());
        assert!(matches!(
            run(&rom, State::default(), MAX_STEPS + 1),
            Err(Error::StepLimit)
        ));
    }
}
