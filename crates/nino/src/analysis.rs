//! Conservative static discovery of ROM instructions. This is not a CPU.
//!
//! Unknown control flow ends a path. Every emitted instruction has an exact
//! source byte range. Discovery never treats coverage as a completeness proof.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const BASE: u32 = 0x0800_0000;
pub const MAX_DISCOVERED: usize = 16_384;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Arm,
    Thumb,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Sequential,
    Branch,
    BranchLink,
    Indirect,
    Exception,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Seed {
    pub address: u32,
    pub mode: Mode,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Instruction {
    pub address: u32,
    pub mode: Mode,
    pub raw: u32,
    pub source_range: [usize; 2],
    pub kind: Kind,
    pub conditional: bool,
    pub successors: Vec<Seed>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Unresolved {
    pub address: u32,
    pub mode: Mode,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Analysis {
    pub source_sha256: String,
    pub instructions: Vec<Instruction>,
    pub unresolved: Vec<Unresolved>,
    pub complete: bool,
    pub maturity: &'static str,
}

fn in_rom(bytes: &[u8], seed: Seed) -> Option<usize> {
    let width = match seed.mode {
        Mode::Arm => 4,
        Mode::Thumb => 2,
    };
    let alignment = width as u32;
    let offset = seed.address.checked_sub(BASE)? as usize;
    (seed.address.is_multiple_of(alignment) && offset.checked_add(width)? <= bytes.len())
        .then_some(offset)
}

fn relative_target(address: u32, pipeline: u32, displacement: i32, mode: Mode) -> Seed {
    Seed {
        address: address
            .wrapping_add(pipeline)
            .wrapping_add(displacement as u32),
        mode,
    }
}

fn decode(bytes: &[u8], seed: Seed) -> Option<Instruction> {
    let offset = in_rom(bytes, seed)?;
    let (raw, width) = match seed.mode {
        Mode::Arm => (
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?),
            4,
        ),
        Mode::Thumb => (
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as u32,
            2,
        ),
    };
    let next = Seed {
        address: seed.address.wrapping_add(width as u32),
        mode: seed.mode,
    };
    let mut successors = Vec::new();
    let (kind, conditional) = match seed.mode {
        Mode::Arm => {
            let condition = raw >> 28;
            if condition == 0xf {
                (Kind::Unsupported, false)
            } else {
                let conditional = condition != 0xe;
                if raw & 0x0fff_fff0 == 0x012f_ff10 {
                    // BX register
                    if conditional {
                        successors.push(next);
                    }
                    (Kind::Indirect, conditional)
                } else if (raw >> 25) & 0b111 == 0b101 {
                    // B / BL
                    let displacement = (((raw & 0x00ff_ffff) << 8) as i32) >> 6;
                    let target = relative_target(seed.address, 8, displacement, Mode::Arm);
                    successors.push(target);
                    if conditional || raw & (1 << 24) != 0 {
                        successors.push(next);
                    }
                    (
                        if raw & (1 << 24) != 0 {
                            Kind::BranchLink
                        } else {
                            Kind::Branch
                        },
                        conditional,
                    )
                } else if (raw >> 24) & 0xf == 0xf {
                    // SWI
                    if conditional {
                        successors.push(next);
                    }
                    (Kind::Exception, conditional)
                } else if (raw >> 25) & 0b111 == 0b100 {
                    // block transfer
                    if raw & (1 << 20) != 0 && raw & (1 << 15) != 0 {
                        if conditional {
                            successors.push(next);
                        }
                        (Kind::Indirect, conditional)
                    } else {
                        successors.push(next);
                        (Kind::Sequential, conditional)
                    }
                } else if (raw >> 26) & 0b11 == 0b01 {
                    // single transfer
                    if raw & (1 << 20) != 0 && (raw >> 12) & 0xf == 15 {
                        if conditional {
                            successors.push(next);
                        }
                        (Kind::Indirect, conditional)
                    } else {
                        successors.push(next);
                        (Kind::Sequential, conditional)
                    }
                } else if (raw >> 26) & 0b11 == 0 && (raw & (1 << 25) != 0 || raw & (1 << 4) == 0) {
                    // Plain data processing. Test opcodes do not write Rd.
                    let opcode = (raw >> 21) & 0xf;
                    let writes_pc = !matches!(opcode, 8..=11) && (raw >> 12) & 0xf == 15;
                    if writes_pc {
                        if conditional {
                            successors.push(next);
                        }
                        (Kind::Indirect, conditional)
                    } else {
                        successors.push(next);
                        (Kind::Sequential, conditional)
                    }
                } else {
                    (Kind::Unsupported, conditional)
                }
            }
        }
        Mode::Thumb => {
            let op = raw as u16;
            if op & 0xf800 == 0xe000 {
                // B, sign-extended imm11
                let displacement = ((op as i32 & 0x7ff) << 21) >> 20;
                successors.push(relative_target(seed.address, 4, displacement, Mode::Thumb));
                (Kind::Branch, false)
            } else if op & 0xf000 == 0xd000 {
                let condition = (op >> 8) & 0xf;
                if condition < 0xe {
                    let displacement = ((op as i32 & 0xff) << 24) >> 23;
                    successors.push(relative_target(seed.address, 4, displacement, Mode::Thumb));
                    successors.push(next);
                    (Kind::Branch, true)
                } else if condition == 0xf {
                    (Kind::Exception, false)
                } else {
                    (Kind::Unsupported, false)
                }
            } else if op & 0xfc00 == 0x4400 {
                // high register operations / BX
                let operation = (op >> 8) & 0x3;
                let destination = (op & 0x7) | ((op >> 4) & 0x8);
                if operation == 3 || (operation != 1 && destination == 15) {
                    (Kind::Indirect, false)
                } else {
                    successors.push(next);
                    (Kind::Sequential, false)
                }
            } else if op & 0xff00 == 0xbd00 {
                // POP {...,PC}
                (Kind::Indirect, false)
            } else if op & 0xf800 == 0xf000 || op & 0xf800 == 0xf800 {
                // BL halfword
                (Kind::Unsupported, false)
            } else if op & 0xf800 <= 0xa800
                || op & 0xff00 == 0xb000
                || op & 0xfe00 == 0xb400
                || op & 0xfe00 == 0xbc00
                || op & 0xf000 == 0xc000
            {
                successors.push(next);
                (Kind::Sequential, false)
            } else {
                (Kind::Unsupported, false)
            }
        }
    };
    Some(Instruction {
        address: seed.address,
        mode: seed.mode,
        raw,
        source_range: [offset, offset + width],
        kind,
        conditional,
        successors,
    })
}

/// Discover only explicitly seeded code paths. `seeds` may come from a trace;
/// static discovery does not infer the target of register-dependent branches.
pub fn discover(bytes: &[u8], seeds: &[Seed]) -> Result<Analysis, crate::Error> {
    let report = crate::inspect(bytes)?;
    let mut pending: VecDeque<Seed> = seeds.iter().copied().collect();
    let mut found = BTreeMap::new();
    let mut unresolved = BTreeSet::new();
    while let Some(seed) = pending.pop_front() {
        if found.contains_key(&seed) {
            continue;
        }
        if found.len() == MAX_DISCOVERED {
            unresolved.insert((seed, "analysis instruction budget reached"));
            break;
        }
        let Some(instruction) = decode(bytes, seed) else {
            unresolved.insert((seed, "target outside ROM or misaligned"));
            continue;
        };
        match instruction.kind {
            Kind::Indirect => {
                unresolved.insert((seed, "register-dependent control transfer"));
            }
            Kind::Exception => {
                unresolved.insert((seed, "exception handler requires runtime state"));
            }
            Kind::Unsupported => {
                unresolved.insert((seed, "opcode class not yet decoded"));
            }
            _ => {}
        }
        pending.extend(instruction.successors.iter().copied());
        found.insert(seed, instruction);
    }
    Ok(Analysis {
        source_sha256: report.source_sha256,
        instructions: found.into_values().collect(),
        unresolved: unresolved
            .into_iter()
            .map(|(seed, reason)| Unresolved {
                address: seed.address,
                mode: seed.mode,
                reason,
            })
            .collect(),
        complete: false,
        maturity: "N0",
    })
}

/// Seed the cartridge's initial ARM instruction. The initial branch into the
/// application is discovered by following its decoded control flow.
pub fn analyse(bytes: &[u8]) -> Result<Analysis, crate::Error> {
    let report = crate::inspect(bytes)?;
    if report.probable_entry_point.is_none() {
        return Ok(Analysis {
            source_sha256: report.source_sha256,
            instructions: Vec::new(),
            unresolved: vec![Unresolved {
                address: BASE,
                mode: Mode::Arm,
                reason: "no trusted in-ROM entry branch seed",
            }],
            complete: false,
            maturity: "N0",
        });
    }
    discover(
        bytes,
        &[Seed {
            address: BASE,
            mode: Mode::Arm,
        }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_discovery_skips_embedded_header_data() {
        let mut rom = vec![0; 0xd0];
        rom[0..4].copy_from_slice(&0xea00_002eu32.to_le_bytes()); // B 0xC0
        rom[4..8].copy_from_slice(&0xe3a0_0009u32.to_le_bytes()); // unreachable
        rom[0xc0..0xc4].copy_from_slice(&0xe3a0_0001u32.to_le_bytes()); // MOV r0,#1
        rom[0xc4..0xc8].copy_from_slice(&0xeaff_fffeu32.to_le_bytes()); // B self
        let analysis = analyse(&rom).unwrap();
        let addresses: Vec<_> = analysis.instructions.iter().map(|i| i.address).collect();
        assert_eq!(addresses, [BASE, BASE + 0xc0, BASE + 0xc4]);
        assert_eq!(analysis.instructions[0].source_range, [0, 4]);
        assert!(analysis.unresolved.is_empty());
        assert!(!analysis.complete);
    }

    #[test]
    fn thumb_conditional_forks_and_register_branch_remains_unresolved() {
        let mut rom = vec![0; 0x100];
        rom[0xc0..0xc2].copy_from_slice(&0xd001u16.to_le_bytes()); // BEQ +2
        rom[0xc2..0xc4].copy_from_slice(&0x2001u16.to_le_bytes()); // MOV r0,#1
        rom[0xc4..0xc6].copy_from_slice(&0x4770u16.to_le_bytes()); // BX lr
        rom[0xc6..0xc8].copy_from_slice(&0xe7fdu16.to_le_bytes()); // B back to BX
        let analysis = discover(
            &rom,
            &[Seed {
                address: BASE + 0xc0,
                mode: Mode::Thumb,
            }],
        )
        .unwrap();
        assert_eq!(analysis.instructions.len(), 4);
        assert_eq!(analysis.unresolved.len(), 1);
        assert_eq!(analysis.unresolved[0].address, BASE + 0xc4);
    }

    #[test]
    fn branch_to_out_of_bounds_is_reported() {
        let rom = 0xea00_1000u32.to_le_bytes();
        let analysis = analyse(&rom).unwrap();
        assert!(analysis.instructions.is_empty());
        assert_eq!(
            analysis.unresolved[0].reason,
            "no trusted in-ROM entry branch seed"
        );
        let analysis = discover(
            &rom,
            &[Seed {
                address: BASE,
                mode: Mode::Arm,
            }],
        )
        .unwrap();
        assert_eq!(analysis.instructions.len(), 1);
        assert_eq!(
            analysis.unresolved[0].reason,
            "target outside ROM or misaligned"
        );
    }
}
