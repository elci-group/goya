//! One ARM data-processing instruction, with explicit unsupported boundaries.
//! This API has no memory bus, exceptions, or timing and is not a full CPU.

use crate::armv4t::{add_with_carry, shift_immediate, Arithmetic, Flags, Shift};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Step {
    Executed,
    ConditionFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NotDataProcessing,
    UnsupportedCondition,
    UnsupportedPcWrite,
    InvalidTestEncoding,
    MisalignedPc,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotDataProcessing => {
                "instruction is outside the supported ARM data-processing subset"
            }
            Self::UnsupportedCondition => "NV condition is not supported by the executor",
            Self::UnsupportedPcWrite => {
                "PC write requires control-flow and processor-mode semantics"
            }
            Self::InvalidTestEncoding => "test instruction requires S=1 and Rd=0",
            Self::MisalignedPc => "ARM instruction address must be word aligned",
        })
    }
}
impl std::error::Error for Error {}

/// Execute ARM AND/EOR/SUB/RSB/ADD/ADC/SBC/RSC/TST/TEQ/CMP/CMN/ORR/MOV/BIC/MVN.
/// Supported Operand2 forms: rotated immediate or register with immediate shift.
/// `registers[15]` is the current instruction address, not the pipelined PC;
/// reads of R15 use address+8. Success advances it by four, including condition
/// failure. On error both registers and flags remain unchanged. This does not
/// assign guest cycles; a future bus/scheduler owns instruction timing.
pub fn step(registers: &mut [u32; 16], flags: &mut Flags, word: u32) -> Result<Step, Error> {
    let pc = registers[15];
    if !pc.is_multiple_of(4) {
        return Err(Error::MisalignedPc);
    }
    if word & 0x0c00_0000 != 0 || (word & (1 << 25) == 0 && word & (1 << 4) != 0) {
        return Err(Error::NotDataProcessing);
    }
    let condition = (word >> 28) as u8;
    if condition == 15 {
        return Err(Error::UnsupportedCondition);
    }
    let opcode = (word >> 21) & 15;
    let set_flags = word & (1 << 20) != 0;
    let rd = ((word >> 12) & 15) as usize;
    let test = matches!(opcode, 8..=11);
    if test && (!set_flags || rd != 0) {
        return Err(Error::InvalidTestEncoding);
    }
    if !flags.condition(condition).expect("four-bit condition") {
        registers[15] = pc.wrapping_add(4);
        return Ok(Step::ConditionFailed);
    }
    if !test && rd == 15 {
        return Err(Error::UnsupportedPcWrite);
    }
    let read = |index: usize| {
        if index == 15 {
            pc.wrapping_add(8)
        } else {
            registers[index]
        }
    };
    let a = read(((word >> 16) & 15) as usize);
    let (b, shifter_carry) = if word & (1 << 25) != 0 {
        let rotation = ((word >> 8) & 15) * 2;
        let b = (word & 255).rotate_right(rotation);
        (b, if rotation == 0 { flags.c } else { b >> 31 != 0 })
    } else {
        let kind = match (word >> 5) & 3 {
            0 => Shift::Lsl,
            1 => Shift::Lsr,
            2 => Shift::Asr,
            _ => Shift::Ror,
        };
        let shifted = shift_immediate(
            read((word & 15) as usize),
            kind,
            ((word >> 7) & 31) as u8,
            flags.c,
        )
        .expect("five-bit amount");
        (shifted.value, shifted.carry)
    };
    let arithmetic: Option<Arithmetic> = match opcode {
        2 | 10 => Some(add_with_carry(a, !b, true)),
        3 => Some(add_with_carry(b, !a, true)),
        4 | 11 => Some(add_with_carry(a, b, false)),
        5 => Some(add_with_carry(a, b, flags.c)),
        6 => Some(add_with_carry(a, !b, flags.c)),
        7 => Some(add_with_carry(b, !a, flags.c)),
        _ => None,
    };
    let value = if let Some(result) = arithmetic {
        result.value
    } else {
        match opcode {
            0 | 8 => a & b,
            1 | 9 => a ^ b,
            12 => a | b,
            13 => b,
            14 => a & !b,
            15 => !b,
            _ => unreachable!("all arithmetic opcodes handled"),
        }
    };
    if set_flags {
        *flags = arithmetic.map(|a| a.flags).unwrap_or(Flags {
            n: value >> 31 != 0,
            z: value == 0,
            c: shifter_carry,
            v: flags.v,
        });
    }
    if !test {
        registers[rd] = value;
    }
    registers[15] = pc.wrapping_add(4);
    Ok(Step::Executed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_opcode_and_test_register_preservation() {
        // Rn=10, Operand2=3, carry set. Independently calculated outputs.
        let expected = [
            2,
            9,
            7,
            0xffff_fff9,
            13,
            14,
            7,
            0xffff_fff9,
            2,
            9,
            7,
            13,
            11,
            3,
            8,
            0xffff_fffc,
        ];
        for opcode in 0..16 {
            let test = (8..=11).contains(&opcode);
            let rd = if test { 0 } else { 2 };
            let mut regs = [0; 16];
            regs[0] = 77;
            regs[1] = 10;
            regs[15] = 0x0800_00c0;
            let mut flags = Flags {
                c: true,
                ..Flags::default()
            };
            let word = 0xe200_0003 | (opcode << 21) | (1 << 20) | (1 << 16) | (rd << 12);
            step(&mut regs, &mut flags, word).unwrap();
            if test {
                assert_eq!(regs[0], 77);
                assert_eq!(flags.z, expected[opcode as usize] == 0);
            } else {
                assert_eq!(regs[2], expected[opcode as usize], "opcode {opcode}");
            }
            assert_eq!(regs[15], 0x0800_00c4);
        }
    }

    #[test]
    fn pipeline_condition_and_error_atomicity() {
        let mut regs = [0; 16];
        regs[15] = 0x0800_00c0;
        let mut flags = Flags::default();
        step(&mut regs, &mut flags, 0xe28f_0004).unwrap(); // ADD r0,pc,#4
        assert_eq!(regs[0], 0x0800_00cc);
        let old = regs;
        assert_eq!(
            step(&mut regs, &mut flags, 0xe3a0_f001),
            Err(Error::UnsupportedPcWrite)
        );
        assert_eq!(regs, old);
        assert_eq!(
            step(&mut regs, &mut flags, 0x03a0_0001),
            Ok(Step::ConditionFailed)
        );
        assert_eq!(regs[0], old[0]);
        assert_eq!(regs[15], old[15] + 4);
        let old = regs;
        assert_eq!(
            step(&mut regs, &mut flags, 0xe129_f000),
            Err(Error::InvalidTestEncoding)
        ); // MSR
        assert_eq!(regs, old);
    }

    #[test]
    fn logical_flags_keep_overflow_and_use_shifter_carry() {
        let mut regs = [0; 16];
        regs[1] = 1;
        let mut flags = Flags {
            v: true,
            c: true,
            ..Flags::default()
        };
        step(&mut regs, &mut flags, 0xe1b0_0061).unwrap(); // MOVS r0,r1,RRX
        assert_eq!(regs[0], 0x8000_0000);
        assert!(flags.n && flags.c && flags.v && !flags.z);
    }
}
