//! Deterministic archive primitives, written from scratch and std-only:
//! CRC-32, Adler-32, stored-block DEFLATE in gzip and zlib wrappers, ustar
//! tar, System V `ar`, and a minimal PNG encoder.
//!
//! "Deterministic" means that identical input gives identical bytes. All
//! timestamps are zero, owners are `0`/`root`, and entries are written in
//! the order given, which callers sort.
//!
//! DEFLATE output uses stored (uncompressed) blocks only. That is valid
//! RFC 1951, so every inflater accepts it. Packages are larger than
//! compressed ones, but the result does not depend on any compressor's
//! version or heuristics.

/// CRC-32 (IEEE 802.3, reflected, as used by gzip and PNG).
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0, data)
}

fn crc32_update(crc: u32, data: &[u8]) -> u32 {
    let mut c = !crc;
    for &byte in data {
        c ^= byte as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// Raw DEFLATE stream made of stored blocks.
fn deflate_stored(data: &[u8], out: &mut Vec<u8>) {
    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
        return;
    }
    let mut chunks = data.chunks(0xFFFF).peekable();
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        out.push(u8::from(last));
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
}

/// gzip (RFC 1952) with mtime 0, no name, OS = Unix.
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0x03];
    deflate_stored(data, &mut out);
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

/// zlib (RFC 1950) with no preset dictionary.
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    deflate_stored(data, &mut out);
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// One tar entry. Directory paths end in `/`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TarEntry {
    Dir(String),
    File(String, Vec<u8>),
    /// A file with mode 0755: the executable of a native app (Viuda).
    Exec(String, Vec<u8>),
}

impl TarEntry {
    pub fn path(&self) -> &str {
        match self {
            Self::Dir(p) | Self::File(p, _) | Self::Exec(p, _) => p,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveError {
    PathTooLong(String),
    NonAsciiPath(String),
    TooLarge(String),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathTooLong(p) => write!(f, "path does not fit a ustar header: {p}"),
            Self::NonAsciiPath(p) => write!(f, "opkg needs ASCII file names: {p}"),
            Self::TooLarge(p) => write!(f, "file too large for the archive: {p}"),
        }
    }
}

fn octal(field: &mut [u8], value: u64) {
    // Width-1 octal digits, zero-padded, then NUL.
    let digits = field.len() - 1;
    let text = format!("{value:0digits$o}");
    field[..digits].copy_from_slice(text.as_bytes());
    field[digits] = 0;
}

/// Split a path into ustar `prefix` (≤155) and `name` (≤100) at a `/`.
fn split_path(path: &str) -> Option<(&str, &str)> {
    if path.len() <= 100 {
        return Some(("", path));
    }
    let bytes = path.as_bytes();
    // Search for a '/' such that the name part (after it) fits in 100 and
    // the prefix fits in 155. A trailing '/' on a directory stays in name.
    (0..bytes.len().saturating_sub(1))
        .rev()
        .filter(|&i| bytes[i] == b'/')
        .find(|&i| i <= 155 && path.len() - i - 1 <= 100 && path.len() - i - 1 > 0)
        .map(|i| (&path[..i], &path[i + 1..]))
}

/// ustar archive: mode 0755 for directories and executables and 0644 for
/// other files, uid/gid 0,
/// owner `root`, mtime 0, and two zero blocks at the end.
pub fn tar(entries: &[TarEntry]) -> Result<Vec<u8>, ArchiveError> {
    let mut out = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_ascii() {
            return Err(ArchiveError::NonAsciiPath(path.to_string()));
        }
        let (prefix, name) =
            split_path(path).ok_or_else(|| ArchiveError::PathTooLong(path.to_string()))?;
        let (mode, size, typeflag, data): (u64, u64, u8, &[u8]) = match entry {
            TarEntry::Dir(_) => (0o755, 0, b'5', &[]),
            TarEntry::File(_, bytes) => (0o644, bytes.len() as u64, b'0', bytes),
            TarEntry::Exec(_, bytes) => (0o755, bytes.len() as u64, b'0', bytes),
        };
        if size > 0o77777777777 {
            return Err(ArchiveError::TooLarge(path.to_string()));
        }
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        octal(&mut header[100..108], mode);
        octal(&mut header[108..116], 0);
        octal(&mut header[116..124], 0);
        octal(&mut header[124..136], size);
        octal(&mut header[136..148], 0);
        header[148..156].copy_from_slice(b"        ");
        header[156] = typeflag;
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[265..269].copy_from_slice(b"root");
        header[297..301].copy_from_slice(b"root");
        header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        let sum: u32 = header.iter().map(|&b| b as u32).sum();
        let text = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(text.as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(data);
        let pad = (512 - data.len() % 512) % 512;
        out.extend(std::iter::repeat_n(0u8, pad));
    }
    out.extend(std::iter::repeat_n(0u8, 1024));
    Ok(out)
}

/// System V `ar` archive in the layout `ares-package` writes: member names
/// without a trailing `/`, mtime 0 (ares writes the build time), uid/gid 0,
/// mode 100644, and members padded to even length with `\n`.
pub fn ar(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    for (name, data) in members {
        let header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            name,
            0,
            0,
            0,
            "100644",
            data.len()
        );
        debug_assert_eq!(header.len(), 60);
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(b'\n');
        }
    }
    out
}

/// 8-bit RGB PNG from row-major pixels (`width * height * 3` bytes).
pub fn png_rgb(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    assert_eq!(rgb.len(), width as usize * height as usize * 3);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32_update(crc32(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    let mut raw = Vec::with_capacity(rgb.len() + height as usize);
    for row in rgb.chunks(width as usize * 3) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    chunk(&mut out, b"IDAT", &zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Width and height from a PNG's IHDR, or `None` if the bytes are not a PNG.
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

/// Readers used by tests and `arachnid inspect-ipk`. They accept only what
/// the writers above produce (stored DEFLATE), and fail on anything else.
pub mod read {
    use super::crc32;

    pub fn ar(bytes: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
        let mut rest = bytes.strip_prefix(b"!<arch>\n")?;
        let mut members = Vec::new();
        while !rest.is_empty() {
            if rest.len() < 60 || &rest[58..60] != b"`\n" {
                return None;
            }
            let name = std::str::from_utf8(&rest[..16])
                .ok()?
                .trim_end()
                .trim_end_matches('/')
                .to_string();
            let size: usize = std::str::from_utf8(&rest[48..58])
                .ok()?
                .trim()
                .parse()
                .ok()?;
            let data = rest.get(60..60 + size)?.to_vec();
            let advance = 60 + size + size % 2;
            rest = rest.get(advance..).unwrap_or(&[]);
            members.push((name, data));
        }
        Some(members)
    }

    /// Inflate a gzip stream made only of stored blocks; verify CRC and size.
    pub fn gunzip_stored(bytes: &[u8]) -> Option<Vec<u8>> {
        if bytes.len() < 18 || bytes[..4] != [0x1F, 0x8B, 0x08, 0x00] {
            return None;
        }
        let mut i = 10;
        let mut out = Vec::new();
        loop {
            let header = *bytes.get(i)?;
            if header & 0b110 != 0 {
                return None; // not a stored block
            }
            let len = u16::from_le_bytes(bytes.get(i + 1..i + 3)?.try_into().ok()?) as usize;
            let nlen = u16::from_le_bytes(bytes.get(i + 3..i + 5)?.try_into().ok()?);
            if nlen != !(len as u16) {
                return None;
            }
            out.extend_from_slice(bytes.get(i + 5..i + 5 + len)?);
            i += 5 + len;
            if header & 1 == 1 {
                break;
            }
        }
        let crc = u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?);
        let size = u32::from_le_bytes(bytes.get(i + 4..i + 8)?.try_into().ok()?);
        (crc == crc32(&out) && size as usize == out.len() && i + 8 == bytes.len()).then_some(out)
    }

    /// One tar entry as read back: (path, typeflag, mode, data).
    pub type Entry = (String, u8, u32, Vec<u8>);

    pub fn tar(bytes: &[u8]) -> Option<Vec<Entry>> {
        let mut entries = Vec::new();
        let mut i = 0;
        loop {
            let header = bytes.get(i..i + 512)?;
            if header.iter().all(|&b| b == 0) {
                return Some(entries);
            }
            let field = |r: std::ops::Range<usize>| {
                let raw = &header[r];
                let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
                std::str::from_utf8(&raw[..end]).ok().map(str::to_string)
            };
            let octal = |r: std::ops::Range<usize>| u64::from_str_radix(field(r)?.trim(), 8).ok();
            let stored: u32 = octal(148..156)? as u32;
            let mut copy = [0u8; 512];
            copy.copy_from_slice(header);
            copy[148..156].copy_from_slice(b"        ");
            if copy.iter().map(|&b| b as u32).sum::<u32>() != stored {
                return None;
            }
            let name = field(0..100)?;
            let prefix = field(345..500)?;
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let size = octal(124..136)? as usize;
            let mode = octal(100..108)? as u32;
            let data = bytes.get(i + 512..i + 512 + size)?.to_vec();
            entries.push((path, header[156], mode, data));
            i += 512 + size.div_ceil(512) * 512;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executables_get_mode_0755_and_files_0644() {
        let out = tar(&[
            TarEntry::Exec("app/viuda".into(), b"\x7fELF".to_vec()),
            TarEntry::File("app/scene.gscene".into(), b"x".to_vec()),
        ])
        .unwrap_or_default();
        assert_eq!(&out[100..108], b"0000755\0");
        assert_eq!(&out[1024 + 100..1024 + 108], b"0000644\0");
    }

    #[test]
    fn crc_and_adler_match_reference_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn gzip_round_trips_across_block_boundaries() {
        for len in [0usize, 1, 65_535, 65_536, 200_000] {
            let data: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
            let gz = gzip(&data);
            assert_eq!(read::gunzip_stored(&gz).unwrap(), data, "{len}");
        }
    }

    #[test]
    fn tar_round_trips_with_long_paths() {
        let long = format!("usr/palm/applications/{}/index.html", "a".repeat(90));
        let entries = vec![
            TarEntry::Dir("usr/".into()),
            TarEntry::File("usr/x.txt".into(), b"hello".to_vec()),
            TarEntry::File(long.clone(), vec![7; 1000]),
        ];
        let bytes = tar(&entries).unwrap();
        assert_eq!(bytes.len() % 512, 0);
        let back = read::tar(&bytes).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(
            (back[0].0.as_str(), back[0].1, back[0].2),
            ("usr/", b'5', 0o755)
        );
        assert_eq!(
            (back[1].0.as_str(), back[1].2, back[1].3.as_slice()),
            ("usr/x.txt", 0o644, &b"hello"[..])
        );
        assert_eq!(back[2].0, long);
        assert!(tar(&[TarEntry::Dir(format!("{}/", "b".repeat(300)))]).is_err());
        assert!(tar(&[TarEntry::Dir("usr/é/".into())]).is_err());
    }

    #[test]
    fn ar_round_trips_and_pads() {
        let bytes = ar(&[("debian-binary", b"2.0\n"), ("odd", b"abc")]);
        assert!(bytes.starts_with(
            b"!<arch>\ndebian-binary   0           0     0     100644  4         `\n2.0\n"
        ));
        let back = read::ar(&bytes).unwrap();
        assert_eq!(
            back,
            vec![
                ("debian-binary".into(), b"2.0\n".to_vec()),
                ("odd".into(), b"abc".to_vec())
            ]
        );
    }

    #[test]
    fn png_has_dimensions_and_valid_crcs() {
        let png = png_rgb(2, 2, &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
        assert_eq!(png_dimensions(&png), Some((2, 2)));
        // Walk the chunks and check every CRC.
        let mut i = 8;
        let mut kinds = Vec::new();
        while i < png.len() {
            let len = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
            let kind = &png[i + 4..i + 8];
            let crc = u32::from_be_bytes(png[i + 8 + len..i + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(&png[i + 4..i + 8 + len]));
            kinds.push(String::from_utf8(kind.to_vec()).unwrap());
            i += 12 + len;
        }
        assert_eq!(kinds, ["IHDR", "IDAT", "IEND"]);
        assert_eq!(png_dimensions(b"not a png at all, definitely"), None);
    }
}
