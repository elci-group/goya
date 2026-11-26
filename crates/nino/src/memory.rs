//! Plain-ROM and default-configuration work RAM bus. Device regions are explicit
//! unsupported accesses. CPU rotation/sign extension and waitstates belong above
//! this aligned bus interface. Cartridge GPIO/EEPROM overrides are not installed.

use serde::Serialize;

pub const EWRAM_BYTES: usize = 256 * 1024;
pub const IWRAM_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Width {
    Byte,
    Halfword,
    Word,
}
impl Width {
    fn bytes(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Halfword => 2,
            Self::Word => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Fault {
    Unaligned,
    UnsupportedRegion,
    RomOutOfBounds,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Error {
    pub address: u32,
    pub width: Width,
    pub write: bool,
    pub fault: Fault,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} at 0x{:08x} ({:?}, write={})",
            self.fault, self.address, self.width, self.write
        )
    }
}
impl std::error::Error for Error {}

/// One authoritative pair of RAM banks. Guest addresses are decoded into
/// bounded slices; they are never cast into host pointers. RAM starts zeroed
/// for authored fixtures, not as a model of BIOS or power-on memory contents.
pub struct Memory<'a> {
    rom: &'a [u8],
    ewram: Vec<u8>,
    iwram: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RamSnapshot {
    pub ewram: Vec<u8>,
    pub iwram: Vec<u8>,
}

impl<'a> Memory<'a> {
    pub fn rom(&self) -> &[u8] {
        self.rom
    }
    pub fn snapshot(&self) -> RamSnapshot {
        RamSnapshot {
            ewram: self.ewram.clone(),
            iwram: self.iwram.clone(),
        }
    }
    pub fn new(rom: &'a [u8]) -> Result<Self, crate::Error> {
        if rom.is_empty() {
            return Err(crate::Error::Empty);
        }
        if rom.len() > crate::MAX_ROM_BYTES {
            return Err(crate::Error::TooLarge { bytes: rom.len() });
        }
        Ok(Self {
            rom,
            ewram: vec![0; EWRAM_BYTES],
            iwram: vec![0; IWRAM_BYTES],
        })
    }

    pub fn ewram(&self) -> &[u8] {
        &self.ewram
    }
    pub fn iwram(&self) -> &[u8] {
        &self.iwram
    }

    pub fn read(&self, address: u32, width: Width) -> Result<u32, Error> {
        self.validate(address, width, false)?;
        let length = width.bytes();
        let slice = match address >> 24 {
            2 => {
                let offset = address as usize & (EWRAM_BYTES - 1);
                &self.ewram[offset..offset + length]
            }
            3 => {
                let offset = address as usize & (IWRAM_BYTES - 1);
                &self.iwram[offset..offset + length]
            }
            8..=13 => {
                let offset = address as usize & 0x01ff_ffff;
                self.rom.get(offset..offset + length).ok_or(Error {
                    address,
                    width,
                    write: false,
                    fault: Fault::RomOutOfBounds,
                })?
            }
            _ => {
                return Err(Error {
                    address,
                    width,
                    write: false,
                    fault: Fault::UnsupportedRegion,
                })
            }
        };
        let mut bytes = [0; 4];
        bytes[..length].copy_from_slice(slice);
        Ok(u32::from_le_bytes(bytes))
    }

    pub fn write(&mut self, address: u32, width: Width, value: u32) -> Result<(), Error> {
        self.validate(address, width, true)?;
        let length = width.bytes();
        let slice = match address >> 24 {
            2 => {
                let offset = address as usize & (EWRAM_BYTES - 1);
                &mut self.ewram[offset..offset + length]
            }
            3 => {
                let offset = address as usize & (IWRAM_BYTES - 1);
                &mut self.iwram[offset..offset + length]
            }
            8..=13 => {
                return Err(Error {
                    address,
                    width,
                    write: true,
                    fault: Fault::ReadOnly,
                })
            }
            _ => {
                return Err(Error {
                    address,
                    width,
                    write: true,
                    fault: Fault::UnsupportedRegion,
                })
            }
        };
        slice.copy_from_slice(&value.to_le_bytes()[..length]);
        Ok(())
    }

    fn validate(&self, address: u32, width: Width, write: bool) -> Result<(), Error> {
        if !address.is_multiple_of(width.bytes() as u32) {
            Err(Error {
                address,
                width,
                write,
                fault: Fault::Unaligned,
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_ram_mirrors_share_authoritative_bytes() {
        let mut memory = Memory::new(&[0]).unwrap();
        memory.write(0x0203_fffc, Width::Word, 0x1234_abcd).unwrap();
        assert_eq!(memory.read(0x02ff_fffc, Width::Word).unwrap(), 0x1234_abcd);
        assert_eq!(memory.read(0x0203_fffc, Width::Byte).unwrap(), 0xcd);
        assert_eq!(memory.read(0x0207_fffe, Width::Halfword).unwrap(), 0x1234);
        memory
            .write(0x03ff_8000, Width::Halfword, 0xdead_beef)
            .unwrap();
        assert_eq!(memory.read(0x0300_0000, Width::Halfword).unwrap(), 0xbeef);
        assert_eq!(memory.iwram()[..2], [0xef, 0xbe]);
        assert_eq!(memory.ewram().len(), EWRAM_BYTES);
    }

    #[test]
    fn rom_waitstate_windows_and_device_boundaries() {
        let mut memory = Memory::new(&[1, 2, 3, 4]).unwrap();
        for base in [0x0800_0000, 0x0a00_0000, 0x0c00_0000] {
            assert_eq!(memory.read(base, Width::Word).unwrap(), 0x0403_0201);
            assert_eq!(
                memory.read(base + 4, Width::Byte).unwrap_err().fault,
                Fault::RomOutOfBounds
            );
            assert_eq!(
                memory.write(base, Width::Word, 0).unwrap_err().fault,
                Fault::ReadOnly
            );
        }
        for base in [
            0,
            0x0400_0000,
            0x0500_0000,
            0x0600_0000,
            0x0700_0000,
            0x0e00_0000,
            0xffff_fffc,
        ] {
            assert_eq!(
                memory.read(base, Width::Word).unwrap_err().fault,
                Fault::UnsupportedRegion
            );
            assert_eq!(
                memory.write(base, Width::Word, 0).unwrap_err().fault,
                Fault::UnsupportedRegion
            );
        }
    }

    #[test]
    fn failed_writes_preserve_memory() {
        let mut memory = Memory::new(&[0]).unwrap();
        memory.write(0x0300_0000, Width::Word, 0x1234_5678).unwrap();
        assert_eq!(
            memory.write(0x0300_0001, Width::Word, 0).unwrap_err().fault,
            Fault::Unaligned
        );
        assert_eq!(memory.read(0x0300_0000, Width::Word).unwrap(), 0x1234_5678);
        assert_eq!(
            memory.read(0xffff_ffff, Width::Word).unwrap_err().fault,
            Fault::Unaligned
        );
    }
}
