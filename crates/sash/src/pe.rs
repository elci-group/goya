//! Bounded parser for the Windows PE facts Sash has to inspect.
//!
//! The parser does not load the image and does not follow untrusted paths.
//! Names that are not plain DLL file names are rejected without being echoed.

use std::fmt::Write as _;

const MAX_PE_BYTES: usize = 64 * 1024 * 1024;
const DOS_E_LFANEW: usize = 0x3C;
const PE_MAGIC: &[u8] = b"PE\0\0";
const MACHINE_AMD64: u16 = 0x8664;
const PE32_PLUS: u16 = 0x20B;
const SUBSYSTEM_WINDOWS_GUI: u16 = 2;
const FILE_EXECUTABLE: u16 = 0x0002;
const FILE_DLL: u16 = 0x2000;
const HIGH_ENTROPY_VA: u16 = 0x0020;
const DYNAMIC_BASE: u16 = 0x0040;
const NX_COMPAT: u16 = 0x0100;
const GUARD_CF: u16 = 0x4000;
const DIR_IMPORT: usize = 1;
const DIR_RESOURCE: usize = 2;
const DIR_DELAY: usize = 13;
const RT_VERSION: u32 = 16;
const RT_MANIFEST: u32 = 24;

/// Facts read from one executable image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeInfo {
    pub machine: u16,
    pub magic: u16,
    pub subsystem: u16,
    pub entry: u32,
    pub dll_characteristics: u16,
    pub characteristics: u16,
    pub imports: Vec<String>,
    pub delay_imports: Vec<String>,
    pub manifest: Vec<u8>,
    pub has_version_resource: bool,
}

/// Policy failures for the static-window portable profile. Empty means the
/// image meets the structural checks this host can perform.
pub fn policy_errors(info: &PeInfo) -> Vec<String> {
    let mut errors = Vec::new();
    if info.machine != MACHINE_AMD64 {
        errors.push(format!("machine {:#06x} is not AMD64", info.machine));
    }
    if info.magic != PE32_PLUS {
        errors.push(format!("optional magic {:#06x} is not PE32+", info.magic));
    }
    if info.subsystem != SUBSYSTEM_WINDOWS_GUI {
        errors.push(format!(
            "subsystem {} is not the Windows GUI subsystem",
            info.subsystem
        ));
    }
    if info.entry == 0 {
        errors.push("entry point is zero".into());
    }
    if info.characteristics & FILE_EXECUTABLE == 0 || info.characteristics & FILE_DLL != 0 {
        errors.push("image is not an executable application".into());
    }
    for (flag, name) in [
        (HIGH_ENTROPY_VA, "high-entropy ASLR"),
        (DYNAMIC_BASE, "ASLR"),
        (NX_COMPAT, "DEP/NX"),
        (GUARD_CF, "control-flow guard"),
    ] {
        if info.dll_characteristics & flag == 0 {
            errors.push(format!("{name} is not set in the PE optional header"));
        }
    }
    if info.manifest.is_empty() {
        errors.push("manifest resource is missing".into());
    } else {
        let text = String::from_utf8_lossy(&info.manifest);
        for required in ["level=\"asInvoker\"", "uiAccess=\"false\"", "PerMonitorV2"] {
            if !text.contains(required) {
                errors.push(format!("manifest does not contain {required}"));
            }
        }
        if text.contains("requireAdministrator") || text.contains("highestAvailable") {
            errors.push("manifest requests more than asInvoker".into());
        }
    }
    if !info.has_version_resource {
        errors.push("version resource is missing".into());
    }
    let mut dlls = info.imports.clone();
    dlls.extend(info.delay_imports.iter().cloned());
    if dlls.is_empty() {
        errors.push("import table is empty".into());
    }
    for name in &dlls {
        if !plain_dll(name) {
            errors.push("an import name is not a plain DLL file name".into());
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if is_denied_runtime(&lower) {
            errors.push(format!("{lower} is a non-static CRT dependency"));
        } else if !is_system_dll(&lower) {
            errors.push(format!("{lower} is not an admitted Windows system DLL"));
        }
    }
    errors
}

pub fn parse_pe(bytes: &[u8]) -> Result<PeInfo, String> {
    if bytes.len() < 64 || bytes.len() > MAX_PE_BYTES {
        return Err("PE size is outside the bounded reader".into());
    }
    if bytes[0] != b'M' || bytes[1] != b'Z' {
        return Err("DOS signature is missing".into());
    }
    let lfanew = read_u32(bytes, DOS_E_LFANEW)? as usize;
    if lfanew > 1024 || lfanew + 4 + 20 > bytes.len() {
        return Err("PE header offset is not usable".into());
    }
    if &bytes[lfanew..lfanew + 4] != PE_MAGIC {
        return Err("PE signature is missing".into());
    }
    let coff = lfanew + 4;
    let machine = read_u16(bytes, coff)?;
    let sections = read_u16(bytes, coff + 2)? as usize;
    let optional_size = read_u16(bytes, coff + 16)? as usize;
    let characteristics = read_u16(bytes, coff + 18)?;
    if sections > 96 || optional_size > 1024 {
        return Err("PE header counts are not bounded".into());
    }
    let optional = coff + 20;
    let section_table = optional + optional_size;
    if section_table + sections * 40 > bytes.len() {
        return Err("section table is truncated".into());
    }
    if optional_size < 112 {
        return Err("optional header is too small".into());
    }
    let magic = read_u16(bytes, optional)?;
    let entry = read_u32(bytes, optional + 16)?;
    let subsystem = read_u16(bytes, optional + 68)?;
    let dll_characteristics = read_u16(bytes, optional + 70)?;
    let dir_count = read_u32(bytes, optional + 108)? as usize;
    if dir_count > 16 || optional + 112 + dir_count * 8 > section_table {
        return Err("data directories are truncated".into());
    }
    let sections = read_sections(bytes, section_table, sections)?;
    let imports = read_import_dlls(bytes, &sections, directory(bytes, optional, dir_count, DIR_IMPORT)?)?;
    let delay_imports =
        read_delay_dlls(bytes, &sections, directory(bytes, optional, dir_count, DIR_DELAY)?)?;
    let resource = directory(bytes, optional, dir_count, DIR_RESOURCE)?;
    let (manifest, has_version_resource) = read_resources(bytes, &sections, resource)?;
    Ok(PeInfo {
        machine,
        magic,
        subsystem,
        entry,
        dll_characteristics,
        characteristics,
        imports,
        delay_imports,
        manifest,
        has_version_resource,
    })
}

fn plain_dll(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && name.to_ascii_lowercase().ends_with(".dll")
}

fn is_denied_runtime(lower: &str) -> bool {
    ["vcruntime", "msvcp", "msvcrt", "ucrtbase", "api-ms-win-crt", "concrt"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn is_system_dll(lower: &str) -> bool {
    const EXACT: &[&str] = &[
        "kernel32.dll",
        "kernelbase.dll",
        "user32.dll",
        "gdi32.dll",
        "advapi32.dll",
        "bcrypt.dll",
        "bcryptprimitives.dll",
        "ntdll.dll",
        "ws2_32.dll",
        "imm32.dll",
        "ole32.dll",
        "oleaut32.dll",
        "combase.dll",
        "shell32.dll",
        "shlwapi.dll",
        "version.dll",
        "sechost.dll",
        "rpcrt4.dll",
    ];
    EXACT.contains(&lower) || (lower.starts_with("api-ms-win-") && !lower.contains("api-ms-win-crt"))
}

#[derive(Clone, Copy)]
struct Section {
    virtual_size: u32,
    virtual_address: u32,
    raw_size: u32,
    raw_ptr: u32,
}

fn read_sections(bytes: &[u8], table: usize, count: usize) -> Result<Vec<Section>, String> {
    let mut sections = Vec::with_capacity(count);
    for index in 0..count {
        let off = table + index * 40;
        sections.push(Section {
            virtual_size: read_u32(bytes, off + 8)?,
            virtual_address: read_u32(bytes, off + 12)?,
            raw_size: read_u32(bytes, off + 16)?,
            raw_ptr: read_u32(bytes, off + 20)?,
        });
    }
    Ok(sections)
}

fn directory(bytes: &[u8], optional: usize, count: usize, index: usize) -> Result<(u32, u32), String> {
    if index >= count {
        return Ok((0, 0));
    }
    let off = optional + 112 + index * 8;
    Ok((read_u32(bytes, off)?, read_u32(bytes, off + 4)?))
}

fn rva_to_offset(sections: &[Section], rva: u32) -> Result<usize, String> {
    for section in sections {
        let span = section.virtual_size.max(section.raw_size);
        if rva >= section.virtual_address && rva.saturating_sub(section.virtual_address) < span {
            let delta = rva - section.virtual_address;
            if delta >= section.raw_size {
                return Err("RVA is not present in the file".into());
            }
            let offset = section.raw_ptr as usize + delta as usize;
            return Ok(offset);
        }
    }
    Err("RVA is outside every section".into())
}

fn read_import_dlls(bytes: &[u8], sections: &[Section], dir: (u32, u32)) -> Result<Vec<String>, String> {
    if dir.0 == 0 || dir.1 == 0 {
        return Ok(Vec::new());
    }
    let mut offset = rva_to_offset(sections, dir.0)?;
    let mut names = Vec::new();
    for _ in 0..64 {
        if offset + 20 > bytes.len() {
            return Err("import descriptor is truncated".into());
        }
        let name_rva = read_u32(bytes, offset + 12)?;
        if name_rva == 0 && bytes[offset..offset + 20].iter().all(|byte| *byte == 0) {
            return Ok(names);
        }
        names.push(read_dll_name(bytes, sections, name_rva)?);
        offset += 20;
    }
    Err("import table is unbounded".into())
}

fn read_delay_dlls(bytes: &[u8], sections: &[Section], dir: (u32, u32)) -> Result<Vec<String>, String> {
    if dir.0 == 0 || dir.1 == 0 {
        return Ok(Vec::new());
    }
    let mut offset = rva_to_offset(sections, dir.0)?;
    let mut names = Vec::new();
    for _ in 0..64 {
        if offset + 32 > bytes.len() {
            return Err("delay-import descriptor is truncated".into());
        }
        let name_rva = read_u32(bytes, offset + 4)?;
        if name_rva == 0 {
            return Ok(names);
        }
        names.push(read_dll_name(bytes, sections, name_rva)?);
        offset += 32;
    }
    Err("delay-import table is unbounded".into())
}

fn read_dll_name(bytes: &[u8], sections: &[Section], rva: u32) -> Result<String, String> {
    let offset = rva_to_offset(sections, rva)?;
    let end = bytes[offset..]
        .iter()
        .position(|byte| *byte == 0)
        .ok_or("DLL name is not terminated")?;
    if end == 0 || end > 64 {
        return Err("DLL name length is not admitted".into());
    }
    let name = std::str::from_utf8(&bytes[offset..offset + end])
        .map_err(|_| "DLL name is not UTF-8".to_string())?;
    if !plain_dll(name) {
        return Err("DLL name is not a plain file name".into());
    }
    Ok(name.to_string())
}

fn read_resources(
    bytes: &[u8],
    sections: &[Section],
    dir: (u32, u32),
) -> Result<(Vec<u8>, bool), String> {
    if dir.0 == 0 || dir.1 == 0 {
        return Ok((Vec::new(), false));
    }
    let root = rva_to_offset(sections, dir.0)?;
    let entries = read_resource_dir(bytes, root, root)?;
    let has_version = entries.iter().any(|entry| entry.id == Some(RT_VERSION));
    let manifests: Vec<_> = entries
        .iter()
        .filter(|entry| entry.id == Some(RT_MANIFEST))
        .collect();
    if manifests.len() > 1 {
        return Err("more than one manifest type entry".into());
    }
    let Some(manifest) = manifests.first() else {
        return Ok((Vec::new(), has_version));
    };
    if !manifest.is_dir {
        return Err("manifest entry is not a directory".into());
    }
    let data = first_data(bytes, sections, root, manifest.child)?;
    Ok((data, has_version))
}

struct ResourceEntry {
    id: Option<u32>,
    is_dir: bool,
    child: usize,
}

fn read_resource_dir(bytes: &[u8], root: usize, dir: usize) -> Result<Vec<ResourceEntry>, String> {
    if dir + 16 > bytes.len() {
        return Err("resource directory is truncated".into());
    }
    let named = read_u16(bytes, dir + 12)? as usize;
    let ids = read_u16(bytes, dir + 14)? as usize;
    let count = named + ids;
    if count > 128 || dir + 16 + count * 8 > bytes.len() {
        return Err("resource directory is unbounded".into());
    }
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let off = dir + 16 + index * 8;
        let name = read_u32(bytes, off)?;
        let offset = read_u32(bytes, off + 4)?;
        let is_dir = offset & 0x8000_0000 != 0;
        let child = root + (offset & 0x7FFF_FFFF) as usize;
        entries.push(ResourceEntry {
            id: (name & 0x8000_0000 == 0).then_some(name),
            is_dir,
            child,
        });
    }
    Ok(entries)
}

fn first_data(
    bytes: &[u8],
    sections: &[Section],
    root: usize,
    dir: usize,
) -> Result<Vec<u8>, String> {
    let mut dir = dir;
    for _ in 0..4 {
        let entries = read_resource_dir(bytes, root, dir)?;
        let Some(entry) = entries.first() else {
            return Err("resource directory is empty".into());
        };
        if entry.is_dir {
            dir = entry.child;
            continue;
        }
        if entry.child + 16 > bytes.len() {
            return Err("resource data entry is truncated".into());
        }
        let rva = read_u32(bytes, entry.child)?;
        let size = read_u32(bytes, entry.child + 4)? as usize;
        if size > 1024 * 1024 {
            return Err("resource data is unbounded".into());
        }
        let offset = rva_to_offset(sections, rva)?;
        let end = offset + size;
        if end > bytes.len() {
            return Err("resource data is truncated".into());
        }
        return Ok(bytes[offset..end].to_vec());
    }
    Err("resource directory is too deep".into())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or("truncated u16")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or("truncated u32")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

pub fn format_flags(characteristics: u16) -> String {
    let mut names = Vec::new();
    for (flag, name) in [
        (HIGH_ENTROPY_VA, "HIGH_ENTROPY_VA"),
        (DYNAMIC_BASE, "DYNAMIC_BASE"),
        (NX_COMPAT, "NX_COMPAT"),
        (GUARD_CF, "GUARD_CF"),
    ] {
        if characteristics & flag != 0 {
            names.push(name);
        }
    }
    let mut out = String::new();
    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "{name}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
        buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn synthetic() -> Vec<u8> {
        let mut bytes = vec![0u8; 0x800];
        bytes[0] = b'M';
        bytes[1] = b'Z';
        put_u32(&mut bytes, 0x3C, 0x80);
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        put_u16(&mut bytes, 0x84, MACHINE_AMD64);
        put_u16(&mut bytes, 0x86, 1);
        put_u16(&mut bytes, 0x94, 240);
        put_u16(&mut bytes, 0x96, FILE_EXECUTABLE | 0x0020);
        put_u16(&mut bytes, 0x98, PE32_PLUS);
        put_u32(&mut bytes, 0x98 + 16, 0x1000);
        put_u16(&mut bytes, 0x98 + 68, SUBSYSTEM_WINDOWS_GUI);
        put_u16(
            &mut bytes,
            0x98 + 70,
            HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT | GUARD_CF,
        );
        put_u32(&mut bytes, 0x98 + 108, 16);
        put_u32(&mut bytes, 0x98 + 112 + 8, 0x1000);
        put_u32(&mut bytes, 0x98 + 112 + 12, 40);
        put_u32(&mut bytes, 0x98 + 112 + 16, 0x1200);
        put_u32(&mut bytes, 0x98 + 112 + 20, 0x200);
        let section = 0x98 + 240;
        bytes[section..section + 6].copy_from_slice(b".rdata");
        put_u32(&mut bytes, section + 8, 0x400);
        put_u32(&mut bytes, section + 12, 0x1000);
        put_u32(&mut bytes, section + 16, 0x400);
        put_u32(&mut bytes, section + 20, 0x400);
        put_u32(&mut bytes, 0x40C, 0x1040);
        bytes[0x440..0x44C].copy_from_slice(b"kernel32.dll");
        let manifest = b"level=\"asInvoker\" uiAccess=\"false\" PerMonitorV2";
        // Resource root at file 0x600 / RVA 0x1200.
        put_u16(&mut bytes, 0x600 + 14, 2);
        put_u32(&mut bytes, 0x610, RT_VERSION);
        put_u32(&mut bytes, 0x614, 0x8000_0000 | 160);
        put_u32(&mut bytes, 0x618, RT_MANIFEST);
        put_u32(&mut bytes, 0x61C, 0x8000_0000 | 32);
        put_u16(&mut bytes, 0x620 + 14, 1);
        put_u32(&mut bytes, 0x630, 1);
        put_u32(&mut bytes, 0x634, 0x8000_0000 | 64);
        put_u16(&mut bytes, 0x640 + 14, 1);
        put_u32(&mut bytes, 0x650, 0x409);
        put_u32(&mut bytes, 0x654, 96);
        put_u32(&mut bytes, 0x660, 0x1320);
        put_u32(&mut bytes, 0x664, manifest.len() as u32);
        bytes[0x720..0x720 + manifest.len()].copy_from_slice(manifest);
        put_u16(&mut bytes, 0x600 + 160 + 14, 1);
        put_u32(&mut bytes, 0x600 + 176, 1);
        put_u32(&mut bytes, 0x600 + 180, 200);
        put_u32(&mut bytes, 0x600 + 200, 0x1300);
        put_u32(&mut bytes, 0x600 + 204, 4);
        bytes[0x700..0x704].copy_from_slice(&[1, 0, 0, 0]);
        bytes
    }

    #[test]
    fn synthetic_pe_exposes_import_manifest_and_version() {
        let info = parse_pe(&synthetic()).unwrap();
        assert_eq!(info.imports, vec!["kernel32.dll".to_string()]);
        assert!(info.delay_imports.is_empty());
        assert!(info.has_version_resource);
        assert!(std::str::from_utf8(&info.manifest).unwrap().contains("asInvoker"));
        assert!(policy_errors(&info).is_empty(), "{:?}", policy_errors(&info));
    }

    #[test]
    fn crt_and_console_fail_policy() {
        let mut info = parse_pe(&synthetic()).unwrap();
        info.subsystem = 3;
        info.imports.push("vcruntime140.dll".into());
        let errors = policy_errors(&info);
        assert!(errors.iter().any(|error| error.contains("subsystem")));
        assert!(errors.iter().any(|error| error.contains("vcruntime140.dll")));
    }

    #[test]
    fn hostile_import_name_is_not_echoed() {
        let mut bytes = synthetic();
        bytes[0x440..0x44C].copy_from_slice(b"../secret\0\0\0");
        let error = parse_pe(&bytes).unwrap_err();
        assert!(!error.contains("secret"));
    }
}
