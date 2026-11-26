//! Material ingestion: a directory or an `app.asar` archive, read into memory
//! under resource budgets. Nothing is written to disk, so archive paths can
//! never escape a root. The source is never modified.

use crate::ExtractError;
use std::fs;
use std::path::Path;

/// Resource limits for ingestion (directive §5.2).
#[derive(Clone, Debug)]
pub struct Budgets {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_depth: usize,
    pub max_asar_header_bytes: u64,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            max_file_bytes: 32 << 20,
            max_total_bytes: 512 << 20,
            max_depth: 64,
            max_asar_header_bytes: 64 << 20,
        }
    }
}

/// One file read from the source.
pub struct Material {
    /// Path relative to the source root, `/`-separated.
    pub path: String,
    pub bytes: Vec<u8>,
}

/// An entry that was seen but not read. It becomes an `unknown` element, so
/// nothing is dropped silently.
pub struct Skipped {
    pub path: String,
    pub observed: &'static str,
    pub reason: &'static str,
    pub detail: String,
}

#[derive(Default)]
pub struct Inventory {
    pub files: Vec<Material>,
    pub skipped: Vec<Skipped>,
}

struct Tally<'b> {
    budgets: &'b Budgets,
    files: usize,
    bytes: u64,
}

impl Tally<'_> {
    fn admit(&mut self, path: &str, size: u64) -> Result<(), ExtractError> {
        self.files += 1;
        self.bytes = self.bytes.saturating_add(size);
        if self.files > self.budgets.max_files {
            return Err(ExtractError::Budget(format!(
                "more than {} files",
                self.budgets.max_files
            )));
        }
        if size > self.budgets.max_file_bytes {
            return Err(ExtractError::Budget(format!(
                "{path} is {size} bytes, over the {} byte file limit",
                self.budgets.max_file_bytes
            )));
        }
        if self.bytes > self.budgets.max_total_bytes {
            return Err(ExtractError::Budget(format!(
                "source exceeds {} bytes",
                self.budgets.max_total_bytes
            )));
        }
        Ok(())
    }
}

/// Directories that are recorded but not descended into.
fn excluded_directory(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match name {
        "node_modules" => Some((
            "dependency_directory",
            "DEPENDENCY_TREE_NOT_ANALYSED",
            "installed dependencies are not inventoried by this extractor version",
        )),
        ".git" => Some((
            "vcs_directory",
            "EXCLUDED_DIRECTORY",
            "version-control metadata is not application material",
        )),
        _ => None,
    }
}

pub fn from_directory(root: &Path, budgets: &Budgets) -> Result<Inventory, ExtractError> {
    let mut inventory = Inventory::default();
    let mut tally = Tally {
        budgets,
        files: 0,
        bytes: 0,
    };
    walk(root, "", 0, &mut tally, &mut inventory)?;
    Ok(inventory)
}

fn walk(
    dir: &Path,
    prefix: &str,
    depth: usize,
    tally: &mut Tally<'_>,
    inventory: &mut Inventory,
) -> Result<(), ExtractError> {
    if depth > tally.budgets.max_depth {
        return Err(ExtractError::Budget(format!(
            "directory nesting deeper than {}",
            tally.budgets.max_depth
        )));
    }
    let mut entries = Vec::new();
    for entry in
        fs::read_dir(dir).map_err(|e| ExtractError::Io(format!("{}: {e}", dir.display())))?
    {
        let entry = entry.map_err(|e| ExtractError::Io(e.to_string()))?;
        let name = entry.file_name().into_string().map_err(|raw| {
            ExtractError::Invalid(format!(
                "non-UTF-8 file name {raw:?} under {}",
                dir.display()
            ))
        })?;
        entries.push((name, entry.path()));
    }
    entries.sort();
    for (name, path) in entries {
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let meta = fs::symlink_metadata(&path)
            .map_err(|e| ExtractError::Io(format!("{}: {e}", path.display())))?;
        if meta.file_type().is_symlink() {
            inventory.skipped.push(Skipped {
                path: rel,
                observed: "symbolic_link",
                reason: "SYMLINK_NOT_FOLLOWED",
                detail: "symbolic links are recorded and never followed".into(),
            });
        } else if meta.is_dir() {
            if let Some((observed, reason, detail)) = excluded_directory(&name) {
                inventory.skipped.push(Skipped {
                    path: rel,
                    observed,
                    reason,
                    detail: detail.into(),
                });
            } else {
                walk(&path, &rel, depth + 1, tally, inventory)?;
            }
        } else if meta.is_file() {
            tally.admit(&rel, meta.len())?;
            let bytes = fs::read(&path).map_err(|e| ExtractError::Io(format!("{rel}: {e}")))?;
            inventory.files.push(Material { path: rel, bytes });
        } else {
            inventory.skipped.push(Skipped {
                path: rel,
                observed: "special_file",
                reason: "EXCLUDED_DIRECTORY",
                detail: "devices, sockets and pipes are not application material".into(),
            });
        }
    }
    Ok(())
}

// ---- ASAR -----------------------------------------------------------------
//
// Layout: an 8-byte pickle holding the header size, then the header pickle
// (payload size, JSON length, JSON, padding), then file data. File offsets in
// the JSON are decimal strings relative to the end of the header.

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, ExtractError> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| ExtractError::Invalid("truncated ASAR header".into()))
}

pub fn from_asar(archive: &[u8], budgets: &Budgets) -> Result<Inventory, ExtractError> {
    if u32_at(archive, 0)? != 4 {
        return Err(ExtractError::Invalid(
            "not an ASAR archive (bad size pickle)".into(),
        ));
    }
    let header_size = u64::from(u32_at(archive, 4)?);
    if header_size > budgets.max_asar_header_bytes {
        return Err(ExtractError::Budget(format!(
            "ASAR header of {header_size} bytes exceeds {}",
            budgets.max_asar_header_bytes
        )));
    }
    let header_end = 8usize
        .checked_add(header_size as usize)
        .filter(|end| *end <= archive.len())
        .ok_or_else(|| ExtractError::Invalid("ASAR header runs past the archive".into()))?;
    let json_len = u32_at(archive, 12)? as usize;
    let json = archive
        .get(16..16usize.saturating_add(json_len))
        .filter(|_| 16 + json_len <= header_end)
        .ok_or_else(|| ExtractError::Invalid("ASAR header JSON runs past the header".into()))?;
    let header: serde_json::Value = serde_json::from_slice(json)
        .map_err(|e| ExtractError::Invalid(format!("ASAR header is not JSON: {e}")))?;
    let data = &archive[header_end..];
    let mut inventory = Inventory::default();
    let mut tally = Tally {
        budgets,
        files: 0,
        bytes: 0,
    };
    let root = header
        .get("files")
        .ok_or_else(|| ExtractError::Invalid("ASAR header has no files".into()))?;
    asar_dir(root, "", 0, data, &mut tally, &mut inventory)?;
    Ok(inventory)
}

fn valid_asar_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

fn asar_dir(
    files: &serde_json::Value,
    prefix: &str,
    depth: usize,
    data: &[u8],
    tally: &mut Tally<'_>,
    inventory: &mut Inventory,
) -> Result<(), ExtractError> {
    if depth > tally.budgets.max_depth {
        return Err(ExtractError::Budget(format!(
            "ASAR nesting deeper than {}",
            tally.budgets.max_depth
        )));
    }
    let map = files.as_object().ok_or_else(|| {
        ExtractError::Invalid(format!("ASAR directory {prefix:?} is not an object"))
    })?;
    // Sorted explicitly: serde_json keeps insertion order when another crate
    // in the build enables `preserve_order`.
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (name, node) in entries {
        if !valid_asar_name(name) {
            return Err(ExtractError::Invalid(format!(
                "ASAR entry name {name:?} under {prefix:?} is not a single path component"
            )));
        }
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if let Some(children) = node.get("files") {
            asar_dir(children, &rel, depth + 1, data, tally, inventory)?;
        } else if node.get("link").is_some() {
            inventory.skipped.push(Skipped {
                path: rel,
                observed: "symbolic_link",
                reason: "SYMLINK_NOT_FOLLOWED",
                detail: "ASAR links are recorded and never followed".into(),
            });
        } else if node.get("unpacked").and_then(|v| v.as_bool()) == Some(true) {
            inventory.skipped.push(Skipped {
                path: rel,
                observed: "asar_unpacked_file",
                reason: "ASAR_UNPACKED_NOT_SUPPLIED",
                detail: "the file lives in app.asar.unpacked, which was not supplied".into(),
            });
        } else {
            let size = node
                .get("size")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| ExtractError::Invalid(format!("ASAR file {rel} has no size")))?;
            let offset: u64 = node
                .get("offset")
                .and_then(|v| v.as_str())
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| ExtractError::Invalid(format!("ASAR file {rel} has no offset")))?;
            tally.admit(&rel, size)?;
            let start = usize::try_from(offset)
                .map_err(|_| ExtractError::Invalid(format!("ASAR file {rel} offset overflows")))?;
            let bytes = start
                .checked_add(size as usize)
                .and_then(|end| data.get(start..end))
                .ok_or_else(|| {
                    ExtractError::Invalid(format!("ASAR file {rel} runs past the archive"))
                })?;
            if let Some(integrity) = node.get("integrity") {
                check_integrity(&rel, integrity, bytes)?;
            }
            inventory.files.push(Material {
                path: rel,
                bytes: bytes.to_vec(),
            });
        }
    }
    Ok(())
}

fn check_integrity(
    rel: &str,
    integrity: &serde_json::Value,
    bytes: &[u8],
) -> Result<(), ExtractError> {
    let algorithm = integrity.get("algorithm").and_then(|v| v.as_str());
    let expected = integrity.get("hash").and_then(|v| v.as_str());
    match (algorithm, expected) {
        (Some("SHA256"), Some(expected)) => {
            let actual = crate::sha256_hex(bytes);
            if actual != expected.to_ascii_lowercase() {
                return Err(ExtractError::Invalid(format!(
                    "ASAR integrity mismatch for {rel}: header says {expected}, bytes hash to {actual}"
                )));
            }
            Ok(())
        }
        _ => Err(ExtractError::Invalid(format!(
            "ASAR file {rel} has an integrity record this extractor cannot check"
        ))),
    }
}
