//! Thumb single transfers (formats 6–11), over the shared aligned bus.
use crate::{
    arm_transfer::Transfer,
    memory::{Memory, Width},
};

#[derive(Debug)]
pub enum Error {
    Unsupported,
    Bus(crate::memory::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => {
                f.write_str("unsupported Thumb transfer encoding or misaligned PC")
            }
            Self::Bus(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {}

pub fn is_transfer(opcode: u16) -> bool {
    opcode & 0xf800 == 0x4800
        || opcode & 0xf000 == 0x5000
        || opcode & 0xe000 == 0x6000
        || opcode & 0xe000 == 0x8000
}

/// Execute a single load/store. Signed results are extended to 32 bits and
/// recorded as the value delivered to the destination register. Errors leave
/// CPU and memory unchanged; unaligned halfword/word behavior is not emulated.
pub fn step(regs: &mut [u32; 16], memory: &mut Memory<'_>, opcode: u16) -> Result<Transfer, Error> {
    let pc = regs[15];
    if !pc.is_multiple_of(2) || !is_transfer(opcode) {
        return Err(Error::Unsupported);
    }
    let (rd, address, width, load, signed) = if opcode & 0xf800 == 0x4800 {
        let base = pc.wrapping_add(4) & !3;
        (
            ((opcode >> 8) & 7) as usize,
            base.wrapping_add(u32::from(opcode & 255) * 4),
            Width::Word,
            true,
            false,
        )
    } else if opcode & 0xf000 == 0x5000 {
        let base = regs[((opcode >> 3) & 7) as usize];
        let offset = regs[((opcode >> 6) & 7) as usize];
        let (width, load, signed) = match (opcode >> 9) & 7 {
            0 => (Width::Word, false, false),
            1 => (Width::Halfword, false, false),
            2 => (Width::Byte, false, false),
            3 => (Width::Byte, true, true),
            4 => (Width::Word, true, false),
            5 => (Width::Halfword, true, false),
            6 => (Width::Byte, true, false),
            _ => (Width::Halfword, true, true),
        };
        (
            (opcode & 7) as usize,
            base.wrapping_add(offset),
            width,
            load,
            signed,
        )
    } else if opcode & 0xe000 == 0x6000 {
        let byte = opcode & 0x1000 != 0;
        let offset = u32::from((opcode >> 6) & 31) * if byte { 1 } else { 4 };
        (
            (opcode & 7) as usize,
            regs[((opcode >> 3) & 7) as usize].wrapping_add(offset),
            if byte { Width::Byte } else { Width::Word },
            opcode & 0x0800 != 0,
            false,
        )
    } else if opcode & 0xf000 == 0x8000 {
        let offset = u32::from((opcode >> 6) & 31) * 2;
        (
            (opcode & 7) as usize,
            regs[((opcode >> 3) & 7) as usize].wrapping_add(offset),
            Width::Halfword,
            opcode & 0x0800 != 0,
            false,
        )
    } else {
        // SP-relative format 11
        (
            ((opcode >> 8) & 7) as usize,
            regs[13].wrapping_add(u32::from(opcode & 255) * 4),
            Width::Word,
            opcode & 0x0800 != 0,
            false,
        )
    };
    let value = if load {
        let raw = memory.read(address, width).map_err(Error::Bus)?;
        match (signed, width) {
            (true, Width::Byte) => (raw as u8 as i8 as i32) as u32,
            (true, Width::Halfword) => (raw as u16 as i16 as i32) as u32,
            _ => raw,
        }
    } else {
        let value = match width {
            Width::Byte => regs[rd] & 255,
            Width::Halfword => regs[rd] & 65535,
            Width::Word => regs[rd],
        };
        memory.write(address, width, value).map_err(Error::Bus)?;
        value
    };
    if load {
        regs[rd] = value;
    }
    regs[15] = pc.wrapping_add(2);
    Ok(Transfer {
        address,
        width,
        write: !load,
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_loads_literal_alignment_and_fault_atomicity() {
        let rom = [0, 0, 0, 0, 0x78, 0x56, 0x34, 0x12];
        let mut bus = Memory::new(&rom).unwrap();
        let mut regs = [0; 16];
        regs[15] = 0x0800_0002;
        step(&mut regs, &mut bus, 0x4800).unwrap(); // LDR r0,[pc,#0] -> aligned PC+4
        assert_eq!(regs[0], 0x1234_5678);
        regs[1] = 0x0200_0000;
        regs[2] = 0;
        bus.write(regs[1], Width::Halfword, 0x80ff).unwrap();
        step(&mut regs, &mut bus, 0x5688).unwrap(); // LDSB r0,[r1,r2]
        assert_eq!(regs[0], 0xffff_ffff);
        step(&mut regs, &mut bus, 0x5e88).unwrap(); // LDSH r0,[r1,r2]
        assert_eq!(regs[0], 0xffff_80ff);
        regs[2] = 1;
        let old = regs;
        let ram = bus.snapshot();
        assert!(step(&mut regs, &mut bus, 0x5288).is_err()); // STRH to odd address
        assert_eq!(regs, old);
        assert_eq!(bus.snapshot(), ram);
    }
    #[test]
    fn immediate_and_sp_offsets_scale_correctly() {
        let mut bus = Memory::new(&[0]).unwrap();
        let mut regs = [0; 16];
        regs[0] = 0xaabb_ccdd;
        regs[1] = 0x0300_0000;
        regs[13] = 0x0200_0000;
        step(&mut regs, &mut bus, 0x6048).unwrap(); // STR r0,[r1,#4]
        step(&mut regs, &mut bus, 0x684a).unwrap(); // LDR r2,[r1,#4]
        assert_eq!(regs[2], regs[0]);
        step(&mut regs, &mut bus, 0x7048).unwrap(); // STRB r0,[r1,#1]
        step(&mut regs, &mut bus, 0x784b).unwrap(); // LDRB r3,[r1,#1]
        assert_eq!(regs[3], 0xdd);
        step(&mut regs, &mut bus, 0x8048).unwrap(); // STRH r0,[r1,#2]
        step(&mut regs, &mut bus, 0x884c).unwrap(); // LDRH r4,[r1,#2]
        assert_eq!(regs[4], 0xccdd);
        step(&mut regs, &mut bus, 0x9002).unwrap(); // STR r0,[sp,#8]
        step(&mut regs, &mut bus, 0x9d02).unwrap(); // LDR r5,[sp,#8]
        assert_eq!(regs[5], regs[0]);
    }
}
