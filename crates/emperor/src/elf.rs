//! ELF identity checks for one linked Emperor application.
//!
//! This reader does not audit glibc symbol versions or `DT_NEEDED`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub class: u8,
    pub data: u8,
    pub elf_type: u16,
    pub machine: u16,
    pub gnu_stack: bool,
    pub executable_stack: bool,
    pub relro: bool,
}

pub fn parse(bytes: &[u8]) -> Result<Summary, String> {
    if bytes.len() < 64 {
        return Err("file is shorter than an ELF64 header".into());
    }
    if bytes[0..4] != [0x7f, b'E', b'L', b'F'] {
        return Err("file does not start with the ELF magic".into());
    }
    let class = bytes[4];
    let data = bytes[5];
    let version = bytes[6];
    if class != 2 {
        return Err(format!("ELF class {class} is not 64-bit"));
    }
    if data != 1 {
        return Err(format!("ELF data {data} is not little-endian"));
    }
    if version != 1 {
        return Err(format!("ELF version {version} is not 1"));
    }
    let elf_type = read_u16(bytes, 16)?;
    let machine = read_u16(bytes, 18)?;
    let phoff = read_u64(bytes, 32)?;
    let phentsize = read_u16(bytes, 54)?;
    let phnum = read_u16(bytes, 56)?;
    if phentsize < 56 {
        return Err(format!("program header size {phentsize} is below 56"));
    }
    if phnum > 128 {
        return Err(format!("program header count {phnum} is above 128"));
    }
    let mut gnu_stack = false;
    let mut executable_stack = false;
    let mut relro = false;
    for index in 0..u64::from(phnum) {
        let start = phoff
            .checked_add(index * u64::from(phentsize))
            .ok_or_else(|| "program header offset overflowed".to_owned())?;
        let start = usize::try_from(start).map_err(|_| "program header offset does not fit")?;
        let end = start
            .checked_add(8)
            .ok_or_else(|| "program header is outside the file".to_owned())?;
        if end > bytes.len() {
            return Err("program header is outside the file".into());
        }
        let kind = read_u32(bytes, start)?;
        let flags = read_u32(bytes, start + 4)?;
        if kind == 0x6474_e551 {
            gnu_stack = true;
            executable_stack = flags & 1 == 1;
        }
        if kind == 0x6474_e552 {
            relro = true;
        }
    }
    Ok(Summary {
        class,
        data,
        elf_type,
        machine,
        gnu_stack,
        executable_stack,
        relro,
    })
}

/// Check one parsed ELF against the requested host triple.
///
/// Missing RELRO fails a release build and is named on a debug build.
/// An executable stack fails both. A missing `PT_GNU_STACK` fails both,
/// because the stack policy was not recorded.
pub fn check(summary: &Summary, machine: u16, release: bool) -> Result<String, String> {
    if summary.machine != machine {
        return Err(format!(
            "ELF machine {:#x} does not match target machine {machine:#x}",
            summary.machine
        ));
    }
    if summary.executable_stack {
        return Err("PT_GNU_STACK is executable".into());
    }
    if !summary.gnu_stack {
        return Err("PT_GNU_STACK is absent, so the stack policy is not recorded".into());
    }
    if !summary.relro {
        if release {
            return Err("PT_GNU_RELRO is absent from a release link".into());
        }
        return Ok("PT_GNU_RELRO is absent from this debug link".into());
    }
    Ok("PT_GNU_RELRO is present and PT_GNU_STACK is not executable".into())
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    let end = at.checked_add(2).ok_or("header offset overflowed")?;
    let slice = bytes
        .get(at..end)
        .ok_or("header field is outside the file")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    let end = at.checked_add(4).ok_or("header offset overflowed")?;
    let slice = bytes
        .get(at..end)
        .ok_or("header field is outside the file")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, String> {
    let end = at.checked_add(8).ok_or("header offset overflowed")?;
    let slice = bytes
        .get(at..end)
        .ok_or("header field is outside the file")?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = b"not an elf".to_vec();
        bytes.resize(64, b'.');
        let err = parse(&bytes).unwrap_err();
        assert!(err.contains("magic"), "{err}");
    }

    #[test]
    fn tiny_buffer_is_rejected() {
        assert!(parse(&[0x7f, b'E', b'L', b'F']).is_err());
    }

    #[test]
    fn non_executable_stack_and_relro_pass() {
        let bytes = sample(true, false);
        let summary = parse(&bytes).expect("parse");
        assert_eq!(summary.machine, 0x3E);
        assert!(summary.gnu_stack);
        assert!(!summary.executable_stack);
        assert!(summary.relro);
        let note = check(&summary, 0x3E, true).expect("check");
        assert!(note.contains("RELRO"), "{note}");
    }

    #[test]
    fn executable_stack_fails() {
        let summary = parse(&sample(true, true)).expect("parse");
        let err = check(&summary, 0x3E, false).unwrap_err();
        assert!(err.contains("executable"), "{err}");
    }

    fn sample(relro: bool, executable_stack: bool) -> Vec<u8> {
        let mut bytes = vec![0u8; 64 + 56 * 2];
        bytes[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[6] = 1;
        bytes[16] = 3;
        bytes[18] = 0x3E;
        bytes[32] = 64;
        bytes[54] = 56;
        bytes[56] = 2;
        write_u32(&mut bytes, 64, 0x6474_e551);
        write_u32(&mut bytes, 68, if executable_stack { 1 } else { 6 });
        if relro {
            write_u32(&mut bytes, 64 + 56, 0x6474_e552);
            write_u32(&mut bytes, 64 + 56 + 4, 4);
        }
        bytes
    }

    fn write_u32(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
}
