//! Bounded parser for the Windows PE facts Sash has to inspect.
//!
//! The parser does not load the image and does not follow untrusted paths.
//! Names that are not plain DLL file names are rejected without being echoed.

use std::fmt::Write as _;

const MAX_PE_BYTES: usize = 64 * 1024 * 1024;
const DOS_E_LFANEW: usize = 0x3C;
const PE_MAGIC: &[u8] = b"PE\0\0";
const PE32: u16 = 0x010B;
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
    let target = crate::windows_target::by_machine(info.machine);
    match target {
        None => errors.push(format!(
            "machine {:#06x} is not an admitted Windows target",
            info.machine
        )),
        Some(target) => {
            if info.magic != target.optional_magic {
                let kind = if target.optional_magic == PE32 {
                    "PE32"
                } else {
                    "PE32+"
                };
                errors.push(format!("optional magic {:#06x} is not {kind}", info.magic));
            }
        }
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
    let mut flags = Vec::with_capacity(4);
    if target.is_some_and(|target| target.pointer_bits == 64) {
        flags.push((HIGH_ENTROPY_VA, "high-entropy ASLR"));
    }
    flags.extend_from_slice(&[
        (DYNAMIC_BASE, "ASLR"),
        (NX_COMPAT, "DEP/NX"),
        (GUARD_CF, "control-flow guard"),
    ]);
    for (flag, name) in flags {
        if info.dll_characteristics & flag == 0 {
            errors.push(format!("{name} is not set in the PE optional header"));
        }
    }
    if info.manifest.is_empty() {
        errors.push("manifest resource is missing".into());
    } else {
        let text = String::from_utf8_lossy(&info.manifest);
        let legacy = manifest_is_legacy(&info.manifest);
        if legacy {
            // A win8-floor image is system DPI aware. It must not request
            // per-monitor awareness, which its adapter cannot honor.
            if !text.contains(">true</dpiAware>") {
                errors.push("legacy manifest is not system DPI aware".into());
            }
            if text.contains("PerMonitorV2") {
                errors.push("legacy manifest must not request PerMonitorV2".into());
            }
        } else {
            for required in ["level=\"asInvoker\"", "uiAccess=\"false\"", "PerMonitorV2"] {
                if !text.contains(required) {
                    errors.push(format!("manifest does not contain {required}"));
                }
            }
        }
        for required in ["level=\"asInvoker\"", "uiAccess=\"false\""] {
            if !text.contains(required) {
                errors.push(format!("manifest does not contain {required}"));
            }
        }
        if text.contains("requireAdministrator") || text.contains("highestAvailable") {
            errors.push("manifest requests more than asInvoker".into());
        }
        if let Some(target) = crate::windows_target::by_machine(info.machine) {
            let expected = format!(
                "processorArchitecture=\"{}\"",
                target.processor_architecture
            );
            if !text.contains(&expected) {
                errors.push(format!(
                    "manifest processorArchitecture is not {}",
                    target.processor_architecture
                ));
            }
        }
        if presentation_profile(&info.manifest).is_none() {
            errors.push("manifest assembly identity is not a Sash presentation profile".into());
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
    let (rva_count_off, dir_base) = match magic {
        PE32 => (92, 96),
        PE32_PLUS => (108, 112),
        _ => return Err("optional magic is not PE32 or PE32+".into()),
    };
    let entry = read_u32(bytes, optional + 16)?;
    let subsystem = read_u16(bytes, optional + 68)?;
    let dll_characteristics = read_u16(bytes, optional + 70)?;
    let dir_count = read_u32(bytes, optional + rva_count_off)? as usize;
    let table = optional + dir_base;
    if dir_count > 16 || table + dir_count * 8 > section_table {
        return Err("data directories are truncated".into());
    }
    let sections = read_sections(bytes, section_table, sections)?;
    let imports = read_import_dlls(
        bytes,
        &sections,
        directory(bytes, table, dir_count, DIR_IMPORT)?,
    )?;
    let delay_imports = read_delay_dlls(
        bytes,
        &sections,
        directory(bytes, table, dir_count, DIR_DELAY)?,
    )?;
    let resource = directory(bytes, table, dir_count, DIR_RESOURCE)?;
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
    [
        "vcruntime",
        "msvcp",
        "msvcrt",
        "ucrtbase",
        "api-ms-win-crt",
        "concrt",
    ]
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
    EXACT.contains(&lower)
        || (lower.starts_with("api-ms-win-") && !lower.contains("api-ms-win-crt"))
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

fn directory(bytes: &[u8], table: usize, count: usize, index: usize) -> Result<(u32, u32), String> {
    if index >= count {
        return Ok((0, 0));
    }
    let off = table + index * 8;
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

fn read_import_dlls(
    bytes: &[u8],
    sections: &[Section],
    dir: (u32, u32),
) -> Result<Vec<String>, String> {
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

fn read_delay_dlls(
    bytes: &[u8],
    sections: &[Section],
    dir: (u32, u32),
) -> Result<Vec<String>, String> {
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
    let slice = bytes.get(offset..offset + 2).ok_or("truncated u16")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes.get(offset..offset + 4).ok_or("truncated u32")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Map one embedded assembly identity to the presentation profile that emits it.
///
/// The static-window manifest version stays `0.0.1.0` for profile
/// `sash.win32.static-window/0.0.3`. Interactive profiles use `0.0.1.0` and
/// `0.0.2.0`. Any other name or version is not that profile.
pub fn presentation_profile(manifest: &[u8]) -> Option<&'static str> {
    let text = String::from_utf8_lossy(manifest);
    let static_name = text.matches("name=\"ELCI.Goya.Sash.StaticWindow\"").count();
    let interactive_name = text
        .matches("name=\"ELCI.Goya.Sash.InteractiveWindow\"")
        .count();
    let static_legacy = text
        .matches("name=\"ELCI.Goya.Sash.StaticWindow.Legacy\"")
        .count();
    let interactive_legacy = text
        .matches("name=\"ELCI.Goya.Sash.InteractiveWindow.Legacy\"")
        .count();
    let version_001 = text.matches("version=\"0.0.1.0\"").count();
    let version_002 = text.matches("version=\"0.0.2.0\"").count();
    if static_name == 1 && interactive_name == 0 && version_001 == 1 && version_002 == 0 {
        return Some(crate::admit::PROFILE);
    }
    if static_name == 0 && interactive_name == 1 && version_001 == 1 && version_002 == 0 {
        return Some(crate::interactive::PROFILE);
    }
    if static_name == 0 && interactive_name == 1 && version_001 == 0 && version_002 == 1 {
        return Some(crate::interactive::PROFILE_CLIPBOARD);
    }
    // The win8 API floor names its own assemblies, so the verifier can read
    // the floor the image was built for instead of guessing.
    if static_legacy == 1 && interactive_name == 0 && interactive_legacy == 0
        && version_001 == 1 && version_002 == 0
    {
        return Some(crate::admit::PROFILE_LEGACY);
    }
    if static_name == 0 && static_legacy == 0 && interactive_legacy == 1
        && version_001 == 1 && version_002 == 0
    {
        return Some(crate::interactive::PROFILE_LEGACY);
    }
    if static_name == 0 && static_legacy == 0 && interactive_legacy == 1
        && version_001 == 0 && version_002 == 1
    {
        return Some(crate::interactive::PROFILE_CLIPBOARD_LEGACY);
    }
    None
}

/// True when the manifest names a win8-floor presentation profile.
pub fn manifest_is_legacy(manifest: &[u8]) -> bool {
    presentation_profile(manifest)
        .is_some_and(|profile| profile.contains("-legacy"))
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
pub(crate) fn synthetic_gui_image(machine: u16, magic: u16, dll_characteristics: u16) -> Vec<u8> {
    tests::image(machine, magic, dll_characteristics)
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

    const MACHINE_AMD64: u16 = 0x8664;
    const MACHINE_I686: u16 = 0x014C;
    const MACHINE_ARM64: u16 = 0xAA64;

    fn synthetic() -> Vec<u8> {
        image(
            MACHINE_AMD64,
            PE32_PLUS,
            HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT | GUARD_CF,
        )
    }

    pub(super) fn image(machine: u16, magic: u16, dll_characteristics: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x800];
        bytes[0] = b'M';
        bytes[1] = b'Z';
        put_u32(&mut bytes, 0x3C, 0x80);
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        put_u16(&mut bytes, 0x84, machine);
        put_u16(&mut bytes, 0x86, 1);
        put_u16(&mut bytes, 0x94, 240);
        put_u16(&mut bytes, 0x96, FILE_EXECUTABLE | 0x0020);
        put_u16(&mut bytes, 0x98, magic);
        put_u32(&mut bytes, 0x98 + 16, 0x1000);
        put_u16(&mut bytes, 0x98 + 68, SUBSYSTEM_WINDOWS_GUI);
        put_u16(&mut bytes, 0x98 + 70, dll_characteristics);
        let (rva_count_off, dir_base) = if magic == PE32 { (92, 96) } else { (108, 112) };
        put_u32(&mut bytes, 0x98 + rva_count_off, 16);
        put_u32(&mut bytes, 0x98 + dir_base + 8, 0x1000);
        put_u32(&mut bytes, 0x98 + dir_base + 12, 40);
        put_u32(&mut bytes, 0x98 + dir_base + 16, 0x1200);
        put_u32(&mut bytes, 0x98 + dir_base + 20, 0x200);
        let section = 0x98 + 240;
        bytes[section..section + 6].copy_from_slice(b".rdata");
        put_u32(&mut bytes, section + 8, 0x400);
        put_u32(&mut bytes, section + 12, 0x1000);
        put_u32(&mut bytes, section + 16, 0x400);
        put_u32(&mut bytes, section + 20, 0x400);
        put_u32(&mut bytes, 0x40C, 0x1040);
        bytes[0x440..0x44C].copy_from_slice(b"kernel32.dll");
        let arch = match machine {
            MACHINE_I686 => "x86",
            MACHINE_ARM64 => "arm64",
            _ => "amd64",
        };
        let manifest = format!(
            "level=\"asInvoker\" uiAccess=\"false\" PerMonitorV2 processorArchitecture=\"{arch}\" name=\"ELCI.Goya.Sash.StaticWindow\" version=\"0.0.1.0\""
        );
        let manifest = manifest.as_bytes();
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
        assert!(std::str::from_utf8(&info.manifest)
            .unwrap()
            .contains("asInvoker"));
        assert!(
            policy_errors(&info).is_empty(),
            "{:?}",
            policy_errors(&info)
        );
        assert_eq!(
            presentation_profile(&info.manifest),
            Some(crate::admit::PROFILE)
        );
    }

    #[test]
    fn crt_and_console_fail_policy() {
        let mut info = parse_pe(&synthetic()).unwrap();
        info.subsystem = 3;
        info.imports.push("vcruntime140.dll".into());
        let errors = policy_errors(&info);
        assert!(errors.iter().any(|error| error.contains("subsystem")));
        assert!(errors
            .iter()
            .any(|error| error.contains("vcruntime140.dll")));
    }

    #[test]
    fn hostile_import_name_is_not_echoed() {
        let mut bytes = synthetic();
        bytes[0x440..0x44C].copy_from_slice(b"../secret\0\0\0");
        let error = parse_pe(&bytes).unwrap_err();
        assert!(!error.contains("secret"));
    }

    fn assert_same_payload(info: &PeInfo) {
        assert_eq!(info.imports, vec!["kernel32.dll".to_string()]);
        assert!(info.delay_imports.is_empty());
        assert!(info.has_version_resource);
        assert!(std::str::from_utf8(&info.manifest)
            .unwrap()
            .contains("asInvoker"));
    }

    #[test]
    fn i686_pe32_allows_policy_without_high_entropy_aslr() {
        let info = parse_pe(&image(
            MACHINE_I686,
            PE32,
            DYNAMIC_BASE | NX_COMPAT | GUARD_CF,
        ))
        .unwrap();
        assert_eq!(info.machine, MACHINE_I686);
        assert_eq!(info.magic, PE32);
        assert_eq!(info.dll_characteristics & HIGH_ENTROPY_VA, 0);
        assert_same_payload(&info);
        assert!(
            policy_errors(&info).is_empty(),
            "{:?}",
            policy_errors(&info)
        );
    }

    #[test]
    fn arm64_pe32_plus_passes_policy() {
        let info = parse_pe(&image(
            MACHINE_ARM64,
            PE32_PLUS,
            HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT | GUARD_CF,
        ))
        .unwrap();
        assert_eq!(info.machine, MACHINE_ARM64);
        assert_eq!(info.magic, PE32_PLUS);
        assert_same_payload(&info);
        assert!(
            policy_errors(&info).is_empty(),
            "{:?}",
            policy_errors(&info)
        );
    }

    #[test]
    fn amd64_without_high_entropy_aslr_fails_policy() {
        let info = parse_pe(&image(
            MACHINE_AMD64,
            PE32_PLUS,
            DYNAMIC_BASE | NX_COMPAT | GUARD_CF,
        ))
        .unwrap();
        let errors = policy_errors(&info);
        assert!(errors
            .iter()
            .any(|error| error.contains("high-entropy ASLR")));
    }

    #[test]
    fn unknown_machine_fails_policy() {
        let mut bytes = synthetic();
        put_u16(&mut bytes, 0x84, 0x01C4);
        let info = parse_pe(&bytes).unwrap();
        assert_eq!(info.machine, 0x01C4);
        let errors = policy_errors(&info);
        assert!(errors
            .iter()
            .any(|error| error.contains("not an admitted Windows target")));
    }

    #[test]
    fn assembly_identity_selects_the_presentation_profile() {
        assert_eq!(
            presentation_profile(br#"name="ELCI.Goya.Sash.InteractiveWindow" version="0.0.2.0""#),
            Some(crate::interactive::PROFILE_CLIPBOARD)
        );
        assert_eq!(
            presentation_profile(br#"name="ELCI.Goya.Sash.InteractiveWindow" version="0.0.1.0""#),
            Some(crate::interactive::PROFILE)
        );
        assert!(
            presentation_profile(br#"name="ELCI.Goya.Sash.StaticWindow" version="0.0.2.0""#)
                .is_none()
        );
        assert!(presentation_profile(
            br#"name="ELCI.Goya.Sash.StaticWindow" version="0.0.1.0" name="ELCI.Goya.Sash.InteractiveWindow" version="0.0.1.0""#
        )
        .is_none());
        let mut info = parse_pe(&synthetic()).unwrap();
        info.manifest =
            b"level=\"asInvoker\" uiAccess=\"false\" PerMonitorV2 processorArchitecture=\"amd64\""
                .to_vec();
        assert!(policy_errors(&info)
            .iter()
            .any(|error| error.contains("assembly identity")));
    }

    #[test]
    fn manifest_architecture_must_match_the_machine() {
        let mut info = parse_pe(&synthetic()).unwrap();
        info.manifest =
            b"level=\"asInvoker\" uiAccess=\"false\" PerMonitorV2 processorArchitecture=\"x86\""
                .to_vec();
        let errors = policy_errors(&info);
        assert!(errors
            .iter()
            .any(|error| error.contains("processorArchitecture is not amd64")));
    }

    #[test]
    fn i686_bytes_with_pe32_plus_magic_fail_policy() {
        let info = parse_pe(&image(
            MACHINE_I686,
            PE32_PLUS,
            DYNAMIC_BASE | NX_COMPAT | GUARD_CF,
        ))
        .unwrap();
        let errors = policy_errors(&info);
        assert!(errors.iter().any(|error| error.ends_with("is not PE32")));
    }
}
