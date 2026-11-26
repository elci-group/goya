//! Thumb arithmetic and direct control-flow subset. No memory or timing.

use crate::{
    analysis::Mode,
    armv4t::{add_with_carry, shift_immediate, Flags, Shift},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    MisalignedPc,
    UnsupportedOpcode,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MisalignedPc => "Thumb PC must be halfword aligned",
            Self::UnsupportedOpcode => "Thumb opcode outside supported subset",
        })
    }
}
impl std::error::Error for Error {}

/// Execute Thumb formats 1–3, conditional/unconditional branches, and BX.
/// R15 stores the fetch address. Errors preserve all state. Branch exchange
/// returns the resulting mode, for the caller to use at the next fetch.
pub fn step(registers: &mut [u32; 16], flags: &mut Flags, opcode: u16) -> Result<Mode, Error> {
    let pc = registers[15];
    if !pc.is_multiple_of(2) {
        return Err(Error::MisalignedPc);
    }
    let mut next_pc = pc.wrapping_add(2);
    let mut mode = Mode::Thumb;
    if opcode & 0xf800 <= 0x1000 {
        // LSL, LSR, ASR immediate
        let kind = match (opcode >> 11) & 3 {
            0 => Shift::Lsl,
            1 => Shift::Lsr,
            _ => Shift::Asr,
        };
        let source = registers[((opcode >> 3) & 7) as usize];
        let result = shift_immediate(source, kind, ((opcode >> 6) & 31) as u8, flags.c)
            .expect("five-bit shift");
        registers[(opcode & 7) as usize] = result.value;
        flags.n = result.value >> 31 != 0;
        flags.z = result.value == 0;
        flags.c = result.carry;
    } else if opcode & 0xf800 == 0x1800 {
        // add/subtract register or imm3
        let a = registers[((opcode >> 3) & 7) as usize];
        let operand = ((opcode >> 6) & 7) as usize;
        let b = if opcode & 0x0400 != 0 {
            operand as u32
        } else {
            registers[operand]
        };
        let result = if opcode & 0x0200 != 0 {
            add_with_carry(a, !b, true)
        } else {
            add_with_carry(a, b, false)
        };
        registers[(opcode & 7) as usize] = result.value;
        *flags = result.flags;
    } else if opcode & 0xe000 == 0x2000 {
        // MOV/CMP/ADD/SUB imm8
        let rd = ((opcode >> 8) & 7) as usize;
        let immediate = u32::from(opcode & 255);
        let operation = (opcode >> 11) & 3;
        if operation == 0 {
            registers[rd] = immediate;
            flags.n = false;
            flags.z = immediate == 0;
        } else {
            let result = if operation == 2 {
                add_with_carry(registers[rd], immediate, false)
            } else {
                add_with_carry(registers[rd], !immediate, true)
            };
            if operation != 1 {
                registers[rd] = result.value;
            }
            *flags = result.flags;
        }
    } else if opcode & 0xf000 == 0xd000 && (opcode >> 8) & 15 < 14 {
        if flags
            .condition(((opcode >> 8) & 15) as u8)
            .expect("condition field")
        {
            let displacement = i32::from(opcode as u8 as i8) * 2;
            next_pc = pc.wrapping_add(4).wrapping_add(displacement as u32);
        }
    } else if opcode & 0xf800 == 0xe000 {
        let displacement = (i32::from(opcode & 0x7ff) << 21) >> 20;
        next_pc = pc.wrapping_add(4).wrapping_add(displacement as u32);
    } else if opcode & 0xff87 == 0x4700 {
        // BX, excluding reserved H1/low bits
        let rm = ((opcode >> 3) & 15) as usize;
        let target = if rm == 15 {
            pc.wrapping_add(4)
        } else {
            registers[rm]
        };
        mode = if target & 1 != 0 {
            Mode::Thumb
        } else {
            Mode::Arm
        };
        next_pc = target & if mode == Mode::Thumb { !1 } else { !3 };
    } else {
        return Err(Error::UnsupportedOpcode);
    }
    registers[15] = next_pc;
    Ok(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immediate_arithmetic_and_shifts_preserve_required_flags() {
        let mut regs = [0; 16];
        let mut flags = Flags {
            c: true,
            v: true,
            ..Flags::default()
        };
        step(&mut regs, &mut flags, 0x2000).unwrap(); // MOV r0,#0
        assert!(flags.z && flags.c && flags.v);
        regs[0] = 0x8000_0000;
        step(&mut regs, &mut flags, 0x0801).unwrap(); // LSR r1,r0,#32
        assert_eq!(regs[1], 0);
        assert!(flags.c && flags.z && flags.v);
        step(&mut regs, &mut flags, 0x3801).unwrap(); // SUB r0,#1
        assert_eq!(regs[0], 0x7fff_ffff);
        assert!(flags.v && flags.c);
        let old = regs;
        assert_eq!(
            step(&mut regs, &mut flags, 0xdf00),
            Err(Error::UnsupportedOpcode)
        );
        assert_eq!(regs, old);
    }

    #[test]
    fn three_bit_add_sub_and_exchange() {
        let mut regs = [0; 16];
        regs[0] = 3;
        regs[1] = 4;
        let mut flags = Flags::default();
        step(&mut regs, &mut flags, 0x1842).unwrap(); // ADD r2,r0,r1
        assert_eq!(regs[2], 7);
        step(&mut regs, &mut flags, 0x1ed3).unwrap(); // SUB r3,r2,#3
        assert_eq!(regs[3], 4);
        regs[14] = 0x0800_0000;
        assert_eq!(step(&mut regs, &mut flags, 0x4770), Ok(Mode::Arm));
        assert_eq!(regs[15], 0x0800_0000);
    }
}
