//! ARM immediate-offset single data transfers over the bounded memory bus.
use crate::{
    armv4t::Flags,
    memory::{Memory, Width},
};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Transfer {
    pub address: u32,
    pub width: Width,
    pub write: bool,
    pub value: u32,
}

#[derive(Debug)]
pub enum Error {
    Unsupported,
    Bus(crate::memory::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => {
                f.write_str("unsupported ARM transfer encoding or register combination")
            }
            Self::Bus(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {}

/// LDR/STR/LDRB/STRB, immediate offset, pre/post indexing and writeback.
/// PC destinations/sources and user-mode transfers are explicitly unsupported.
/// A literal load may use PC as its base (address+8), without writeback.
/// Unaligned operations report the bus fault pending CPU rotation semantics.
pub fn step(
    regs: &mut [u32; 16],
    flags: Flags,
    memory: &mut Memory<'_>,
    word: u32,
) -> Result<Option<Transfer>, Error> {
    let pc = regs[15];
    if word & 0x0e00_0000 != 0x0400_0000 || word >> 28 == 15 || !pc.is_multiple_of(4) {
        return Err(Error::Unsupported);
    }
    if !flags
        .condition((word >> 28) as u8)
        .expect("condition field")
    {
        regs[15] = pc.wrapping_add(4);
        return Ok(None);
    }
    let rn = ((word >> 16) & 15) as usize;
    let rd = ((word >> 12) & 15) as usize;
    let pre = word & (1 << 24) != 0;
    let write_bit = word & (1 << 21) != 0;
    let writeback = !pre || write_bit;
    let load = word & (1 << 20) != 0;
    if (!pre && write_bit) || rd == 15 || (writeback && (rn == 15 || (load && rn == rd))) {
        return Err(Error::Unsupported);
    }
    let base = if rn == 15 {
        pc.wrapping_add(8)
    } else {
        regs[rn]
    };
    let offset = word & 0xfff;
    let updated = if word & (1 << 23) != 0 {
        base.wrapping_add(offset)
    } else {
        base.wrapping_sub(offset)
    };
    let address = if pre { updated } else { base };
    let width = if word & (1 << 22) != 0 {
        Width::Byte
    } else {
        Width::Word
    };
    let value = if load {
        memory.read(address, width).map_err(Error::Bus)?
    } else {
        let value = regs[rd];
        memory.write(address, width, value).map_err(Error::Bus)?;
        if width == Width::Byte {
            value & 255
        } else {
            value
        }
    };
    if load {
        regs[rd] = value;
    }
    if writeback {
        regs[rn] = updated;
    }
    regs[15] = pc.wrapping_add(4);
    Ok(Some(Transfer {
        address,
        width,
        write: !load,
        value,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addressing_and_failed_writeback_are_atomic() {
        let mut memory = Memory::new(&[0]).unwrap();
        let mut regs = [0; 16];
        regs[0] = 0x0200_0000;
        regs[1] = 0x1234_abcd;
        step(&mut regs, Flags::default(), &mut memory, 0xe480_1004).unwrap(); // STR r1,[r0],#4
        assert_eq!(regs[0], 0x0200_0004);
        step(&mut regs, Flags::default(), &mut memory, 0xe530_2004).unwrap(); // LDR r2,[r0,#-4]!
        assert_eq!(regs[0], 0x0200_0000);
        assert_eq!(regs[2], 0x1234_abcd);
        let old = regs;
        let ram = memory.snapshot();
        assert!(step(&mut regs, Flags::default(), &mut memory, 0xe5a0_1001).is_err()); // unaligned STR with writeback
        assert_eq!(regs, old);
        assert_eq!(memory.snapshot(), ram);
        assert!(step(&mut regs, Flags::default(), &mut memory, 0xe590_f000).is_err());
        assert_eq!(regs, old);
    }
}
