//! N0 ROM intelligence. No execution or translation capability is implied.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write;

pub mod analysis;
pub mod arm_data;
pub mod arm_transfer;
pub mod armv4t;
pub mod execution;
pub mod extraction;
pub mod graphics;
pub mod memory;
pub mod thumb_data;
pub mod thumb_transfer;

pub const MAX_ROM_BYTES: usize = 32 * 1024 * 1024;
const ROM_BASE: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooLarge { bytes: usize },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "ROM is empty"),
            Self::TooLarge { bytes } => {
                write!(f, "ROM is {bytes} bytes; maximum is {MAX_ROM_BYTES}")
            }
        }
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Header {
    pub title: Option<String>,
    pub game_code: Option<String>,
    pub maker_code: Option<String>,
    pub software_version: Option<u8>,
    pub fixed_byte_valid: Option<bool>,
    pub complement_valid: Option<bool>,
    pub logo_present: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Signature {
    pub technology: &'static str,
    pub marker: &'static str,
    pub rom_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub source_format: &'static str,
    pub source_sha256: String,
    pub rom_bytes: usize,
    pub header: Header,
    pub probable_entry_point: Option<u32>,
    pub entry_source_range: [usize; 2],
    pub save_signatures: Vec<Signature>,
    pub peripheral_hints: Vec<Signature>,
    pub warnings: Vec<Finding>,
    pub translation_eligible: bool,
    pub maturity: &'static str,
}

fn ascii_field(bytes: &[u8], start: usize, len: usize) -> Option<String> {
    let field = bytes.get(start..start.checked_add(len)?)?;
    if !field.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
        return None;
    }
    let value = std::str::from_utf8(field).ok()?.trim_end_matches(' ');
    (!value.is_empty()).then(|| value.to_owned())
}

fn scan(bytes: &[u8], technology: &'static str, marker: &'static str) -> Vec<Signature> {
    bytes
        .windows(marker.len())
        .enumerate()
        .filter_map(|(offset, window)| {
            (window == marker.as_bytes()).then_some(Signature {
                technology,
                marker,
                rom_offset: offset,
            })
        })
        .collect()
}

fn arm_branch_target(bytes: &[u8]) -> Option<u32> {
    let word = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
    if word >> 28 != 0xe || (word >> 25) & 0b111 != 0b101 {
        return None;
    }
    let displacement = (((word & 0x00ff_ffff) << 8) as i32 >> 6) as u32;
    let target = ROM_BASE.wrapping_add(8).wrapping_add(displacement);
    (target >= ROM_BASE && (target - ROM_BASE) < bytes.len() as u32 && target.is_multiple_of(4))
        .then_some(target)
}

/// Inspect untrusted ROM bytes without executing them. Findings are hints;
/// an invalid header does not cause an otherwise readable image to fail.
pub fn inspect(bytes: &[u8]) -> Result<Report, Error> {
    if bytes.is_empty() {
        return Err(Error::Empty);
    }
    if bytes.len() > MAX_ROM_BYTES {
        return Err(Error::TooLarge { bytes: bytes.len() });
    }
    let mut fingerprint = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut fingerprint, "{byte:02x}").expect("String write");
    }
    let fixed_byte_valid = bytes.get(0xb2).map(|b| *b == 0x96);
    let complement_valid = bytes.get(0xbd).map(|actual| {
        let sum = bytes[0xa0..=0xbc]
            .iter()
            .fold(0x19u8, |sum, b| sum.wrapping_add(*b));
        *actual == 0u8.wrapping_sub(sum)
    });
    let logo_present = bytes
        .get(0x04..0xa0)
        .map(|logo| logo.iter().any(|b| *b != 0));
    let header = Header {
        title: ascii_field(bytes, 0xa0, 12),
        game_code: ascii_field(bytes, 0xac, 4),
        maker_code: ascii_field(bytes, 0xb0, 2),
        software_version: bytes.get(0xbc).copied(),
        fixed_byte_valid,
        complement_valid,
        logo_present,
    };
    let probable_entry_point = arm_branch_target(bytes);
    let mut warnings = Vec::new();
    if bytes.len() < 0xc0 {
        warnings.push(Finding {
            code: "NINO-H001",
            detail: "Cartridge header is incomplete".into(),
        });
    }
    if fixed_byte_valid == Some(false) {
        warnings.push(Finding {
            code: "NINO-H002",
            detail: "Fixed header byte is not 0x96".into(),
        });
    }
    if complement_valid == Some(false) {
        warnings.push(Finding {
            code: "NINO-H003",
            detail: "Header complement does not match".into(),
        });
    }
    if logo_present == Some(false) {
        warnings.push(Finding {
            code: "NINO-H004",
            detail: "Logo region is empty; Nintendo logo identity is not validated".into(),
        });
    }
    if probable_entry_point.is_none() {
        warnings.push(Finding {
            code: "NINO-E001",
            detail: "No in-ROM ARM branch target recovered at offset zero".into(),
        });
    }
    let mut save_signatures = Vec::new();
    for (technology, marker) in [
        ("SRAM", "SRAM_V"),
        ("FLASH", "FLASH_V"),
        ("FLASH", "FLASH512_V"),
        ("FLASH", "FLASH1M_V"),
        ("EEPROM", "EEPROM_V"),
    ] {
        save_signatures.extend(scan(bytes, technology, marker));
    }
    save_signatures.sort_by_key(|s| (s.rom_offset, s.marker));
    let mut peripheral_hints = scan(bytes, "RTC", "SIIRTC_V");
    peripheral_hints.extend(scan(bytes, "RTC", "RTC_V"));
    peripheral_hints.sort_by_key(|s| (s.rom_offset, s.marker));
    if save_signatures
        .iter()
        .map(|s| s.technology)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1
    {
        warnings.push(Finding {
            code: "NINO-S001",
            detail: "Conflicting save technology markers; hardware behaviour must decide".into(),
        });
    }
    Ok(Report {
        source_format: "gba",
        source_sha256: fingerprint,
        rom_bytes: bytes.len(),
        header,
        probable_entry_point,
        entry_source_range: [0, bytes.len().min(4)],
        save_signatures,
        peripheral_hints,
        warnings,
        translation_eligible: false,
        maturity: "N0",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_digest_and_short_header_are_safe() {
        let report = inspect(b"abc").unwrap();
        assert_eq!(
            report.source_sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(report.warnings[0].code, "NINO-H001");
        assert_eq!(report.header.complement_valid, None);
        assert!(!report.translation_eligible);
    }
    #[test]
    fn branch_and_header_are_deterministic() {
        let mut rom = vec![0u8; 0x100];
        rom[..4].copy_from_slice(&0xea00_002eu32.to_le_bytes());
        rom[0xa0..0xa4].copy_from_slice(b"TEST");
        rom[0xb2] = 0x96;
        let sum = rom[0xa0..=0xbc]
            .iter()
            .fold(0x19u8, |acc, b| acc.wrapping_add(*b));
        rom[0xbd] = 0u8.wrapping_sub(sum);
        rom[0xe0..0xe6].copy_from_slice(b"SRAM_V");
        let a = inspect(&rom).unwrap();
        assert_eq!(a, inspect(&rom).unwrap());
        assert_eq!(a.probable_entry_point, Some(0x0800_00c0));
        assert_eq!(a.header.complement_valid, Some(true));
        assert_eq!(a.save_signatures[0].rom_offset, 0xe0);
    }
    #[test]
    fn rejects_empty_and_oversized_inputs() {
        assert_eq!(inspect(&[]), Err(Error::Empty));
        assert!(matches!(
            inspect(&vec![0; MAX_ROM_BYTES + 1]),
            Err(Error::TooLarge { .. })
        ));
    }
}
