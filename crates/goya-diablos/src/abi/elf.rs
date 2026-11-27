//! BSD ELF verification (mirroring Emperor's ELF checks).

use crate::Fault;
use crate::sha256_hex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// BSD ELF information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BsdElf {
    /// Target triple
    pub triple: String,
    /// ELF class (64-bit = 2)
    pub class: u8,
    /// Data encoding (little-endian = 1)
    pub data: u8,
    /// ELF version
    pub version: u8,
    /// Machine architecture (e_machine)
    pub machine: u16,
    /// Object file type (e_type)
    pub obj_type: u16,
    /// Entry point address
    pub entry: u64,
    /// Program header offset
    pub phoff: u64,
    /// Section header offset
    pub shoff: u64,
    /// Flags
    pub flags: u32,
    /// ELF header size
    pub ehsize: u16,
    /// Program header entry size
    pub phentsize: u16,
    /// Number of program headers
    pub phnum: u16,
    /// Section header entry size
    pub shentsize: u16,
    /// Number of section headers
    pub shnum: u16,
    /// Section header string table index
    pub shstrndx: u16,
    /// Program headers
    pub program_headers: Vec<ProgramHeader>,
    /// Build ID (if present)
    pub build_id: Option<String>,
    /// SHA-256 of the ELF file
    pub sha256: String,
}

/// ELF program header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramHeader {
    /// Segment type
    pub p_type: u32,
    /// Segment flags
    pub p_flags: u32,
    /// Segment offset in file
    pub p_offset: u64,
    /// Virtual address in memory
    pub p_vaddr: u64,
    /// Physical address (reserved)
    pub p_paddr: u64,
    /// Size in file
    pub p_filesz: u64,
    /// Size in memory
    pub p_memsz: u64,
    /// Alignment
    pub p_align: u64,
}

/// ELF verification errors.
#[derive(Debug, thiserror::Error)]
pub enum ElfVerificationError {
    #[error("not an ELF file: invalid magic")]
    InvalidMagic,
    #[error("unsupported ELF class: {0} (expected 2 for 64-bit)")]
    InvalidClass(u8),
    #[error("unsupported data encoding: {0} (expected 1 for little-endian)")]
    InvalidData(u8),
    #[error("unsupported ELF version: {0}")]
    InvalidVersion(u8),
    #[error("machine mismatch: got {got:#x}, expected {expected:#x} for triple {triple}")]
    MachineMismatch { got: u16, expected: u16, triple: String },
    #[error("executable stack detected (PT_GNU_STACK with PF_X)")]
    ExecutableStack,
    #[error("missing PT_GNU_RELRO (required for release builds)")]
    MissingRelro,
    #[error("ELF too large: {0} bytes (max 64MB)")]
    TooLarge(usize),
    #[error("I/O error: {0}")]
    Io(std::io::Error),
}

/// Verify a BSD ELF file against policy.
pub fn verify_elf(path: &Path, triple: &str, release: bool) -> Result<BsdElf, ElfVerificationError> {
    let bytes = fs::read(path).map_err(ElfVerificationError::Io)?;
    
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(ElfVerificationError::TooLarge(bytes.len()));
    }

    // Parse ELF header
    if bytes.len() < 64 {
        return Err(ElfVerificationError::InvalidMagic);
    }

    // Check magic: 0x7F 'E' 'L' 'F'
    if &bytes[0..4] != b"\x7FELF" {
        return Err(ElfVerificationError::InvalidMagic);
    }

    let class = bytes[4];
    if class != 2 {
        return Err(ElfVerificationError::InvalidClass(class));
    }

    let data = bytes[5];
    if data != 1 {
        return Err(ElfVerificationError::InvalidData(data));
    }

    let version = bytes[6];
    if version != 1 {
        return Err(ElfVerificationError::InvalidVersion(version));
    }

    // Parse based on endianness (little-endian)
    let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
    let obj_type = u16::from_le_bytes([bytes[16], bytes[17]]);
    let entry = u64::from_le_bytes([
        bytes[24], bytes[25], bytes[26], bytes[27],
        bytes[28], bytes[29], bytes[30], bytes[31],
    ]);
    let phoff = u64::from_le_bytes([
        bytes[32], bytes[33], bytes[34], bytes[35],
        bytes[36], bytes[37], bytes[38], bytes[39],
    ]);
    let shoff = u64::from_le_bytes([
        bytes[40], bytes[41], bytes[42], bytes[43],
        bytes[44], bytes[45], bytes[46], bytes[47],
    ]);
    let flags = u32::from_le_bytes([bytes[48], bytes[49], bytes[50], bytes[51]]);
    let ehsize = u16::from_le_bytes([bytes[52], bytes[53]]);
    let phentsize = u16::from_le_bytes([bytes[54], bytes[55]]);
    let phnum = u16::from_le_bytes([bytes[56], bytes[57]]);
    let shentsize = u16::from_le_bytes([bytes[58], bytes[59]]);
    let shnum = u16::from_le_bytes([bytes[60], bytes[61]]);
    let shstrndx = u16::from_le_bytes([bytes[62], bytes[63]]);

    // Verify machine matches triple
    let expected_machine = elf_machine(triple).ok_or_else(|| ElfVerificationError::MachineMismatch {
        got: machine,
        expected: 0,
        triple: triple.to_string(),
    })?;

    if machine != expected_machine {
        return Err(ElfVerificationError::MachineMismatch {
            got: machine,
            expected: expected_machine,
            triple: triple.to_string(),
        });
    }

    // Parse program headers
    let mut program_headers = Vec::new();
    let mut has_gnu_stack = false;
    let mut has_gnu_relro = false;
    let mut executable_stack = false;

    if phnum > 0 && phoff > 0 && phentsize >= 56 {
        for i in 0..phnum {
            let offset = phoff as usize + i as usize * phentsize as usize;
            if offset + 56 > bytes.len() {
                break;
            }
            let ph = parse_program_header(&bytes[offset..offset + 56]);
            let is_gnu_stack = ph.p_type == 0x6474e551; // PT_GNU_STACK
            let is_gnu_relro = ph.p_type == 0x6474e552; // PT_GNU_RELRO

            if is_gnu_stack {
                has_gnu_stack = true;
                if ph.p_flags & 0x1 != 0 { // PF_X
                    executable_stack = true;
                }
            }
            if is_gnu_relro {
                has_gnu_relro = true;
            }

            program_headers.push(ph);
        }
    }

    // Check executable stack (fail in both debug and release)
    if executable_stack {
        return Err(ElfVerificationError::ExecutableStack);
    }

    // Check RELRO (fail closed in release)
    if release && !has_gnu_relro {
        return Err(ElfVerificationError::MissingRelro);
    }

    // Extract build ID from .note.gnu.build-id section (simplified)
    let build_id = extract_build_id(&bytes, &program_headers);

    Ok(BsdElf {
        triple: triple.to_string(),
        class,
        data,
        version,
        machine,
        obj_type,
        entry,
        phoff,
        shoff,
        flags,
        ehsize,
        phentsize,
        phnum,
        shentsize,
        shnum,
        shstrndx,
        program_headers,
        build_id,
        sha256: sha256_hex(&bytes),
    })
}

/// Parse a program header (56 bytes, little-endian).
fn parse_program_header(bytes: &[u8]) -> ProgramHeader {
    ProgramHeader {
        p_type: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        p_flags: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        p_offset: u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15],
        ]),
        p_vaddr: u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19],
            bytes[20], bytes[21], bytes[22], bytes[23],
        ]),
        p_paddr: u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27],
            bytes[28], bytes[29], bytes[30], bytes[31],
        ]),
        p_filesz: u64::from_le_bytes([
            bytes[32], bytes[33], bytes[34], bytes[35],
            bytes[36], bytes[37], bytes[38], bytes[39],
        ]),
        p_memsz: u64::from_le_bytes([
            bytes[40], bytes[41], bytes[42], bytes[43],
            bytes[44], bytes[45], bytes[46], bytes[47],
        ]),
        p_align: u64::from_le_bytes([
            bytes[48], bytes[49], bytes[50], bytes[51],
            bytes[52], bytes[53], bytes[54], bytes[55],
        ]),
    }
}

/// Extract build ID from ELF notes.
fn extract_build_id(bytes: &[u8], program_headers: &[ProgramHeader]) -> Option<String> {
    // Simplified: scan for NT_GNU_BUILD_ID (3) in NOTE segments
    for ph in program_headers {
        if ph.p_type == 4 { // PT_NOTE
            let offset = ph.p_offset as usize;
            let size = ph.p_filesz as usize;
            if offset + size <= bytes.len() {
                if let Some(id) = parse_build_id_from_notes(&bytes[offset..offset + size]) {
                    return Some(id);
                }
            }
        }
    }
    None
}

/// Parse build ID from NOTE segment data.
fn parse_build_id_from_notes(data: &[u8]) -> Option<String> {
    let mut offset = 0;
    while offset + 12 <= data.len() {
        let namesz = u32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as usize;
        let descsz = u32::from_le_bytes([data[offset+4], data[offset+5], data[offset+6], data[offset+7]]) as usize;
        let ntype = u32::from_le_bytes([data[offset+8], data[offset+9], data[offset+10], data[offset+11]]);

        offset += 12;
        let name_start = offset;
        offset += (namesz + 3) & !3; // align to 4
        let desc_start = offset;
        offset += (descsz + 3) & !3;

        if ntype == 3 && descsz == 20 { // NT_GNU_BUILD_ID, 20 bytes = 160 bits
            if desc_start + descsz <= data.len() {
                return Some(hex::encode(&data[desc_start..desc_start + descsz]));
            }
        }

        if offset > data.len() {
            break;
        }
    }
    None
}

/// Get expected ELF machine value for a triple.
pub fn elf_machine(triple: &str) -> Option<u16> {
    match triple {
        "x86_64-unknown-freebsd" => Some(0x3E),   // EM_X86_64
        "aarch64-unknown-freebsd" => Some(0xB7),  // EM_AARCH64
        "i386-unknown-freebsd" => Some(0x03),     // EM_386
        "armv7-unknown-freebsd" => Some(0x28),    // EM_ARM
        "riscv64-unknown-freebsd" => Some(0xF3),  // EM_RISCV
        "x86_64-unknown-openbsd" => Some(0x3E),
        "aarch64-unknown-openbsd" => Some(0xB7),
        "x86_64-unknown-netbsd" => Some(0x3E),
        "aarch64-unknown-netbsd" => Some(0xB7),
        "x86_64-unknown-dragonfly" => Some(0x3E),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elf_machine_freebsd() {
        assert_eq!(elf_machine("x86_64-unknown-freebsd"), Some(0x3E));
        assert_eq!(elf_machine("aarch64-unknown-freebsd"), Some(0xB7));
        assert_eq!(elf_machine("i386-unknown-freebsd"), Some(0x03));
    }

    #[test]
    fn elf_machine_other_bsd() {
        assert_eq!(elf_machine("x86_64-unknown-openbsd"), Some(0x3E));
        assert_eq!(elf_machine("x86_64-unknown-netbsd"), Some(0x3E));
        assert_eq!(elf_machine("x86_64-unknown-dragonfly"), Some(0x3E));
    }

    #[test]
    fn elf_machine_unknown() {
        assert_eq!(elf_machine("x86_64-pc-windows-msvc"), None);
        assert_eq!(elf_machine("x86_64-unknown-linux-gnu"), None);
    }
}