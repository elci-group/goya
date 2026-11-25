//! goya-graphics-container — the shared binary envelope for Goya's graphics
//! formats (`.gru`, `.gva`, `.gvi`, `.spc`).
//!
//! Contract: [`CONTAINER_CONTRACT`] (`GY-GFXC/1`), **ratified 2026-11-24**
//! under directive GOYA-GFX-001 Phase 0. Normative text lives
//! in `docs/graphics/container.md`; this crate is its reference
//! implementation and must not drift from it.
//!
//! The container knows nothing about geometry, inventories or
//! reconstruction. It provides:
//!
//! - an 80-byte little-endian header with a per-format magic;
//! - a sorted, canonical section directory (72-byte entries, each carrying
//!   the SHA-256 of its section's stored bytes);
//! - a header digest (SHA-256 over header + directory, digest field zeroed)
//!   that transitively commits to every byte of the file, so a reader can
//!   verify the envelope cheaply and then verify only the sections it loads;
//! - exactly one valid byte layout per logical content (no gaps, hidden
//!   bytes, reordering or non-zero padding), so identical inputs give
//!   identical bytes;
//! - resource limits checked before any allocation, with checked arithmetic
//!   on every offset, length and count.
//!
//! A digest is integrity evidence, never authenticity. Signing belongs to a
//! separate signed manifest (see `goya-artifact`).

use sha2::{Digest, Sha256};
use std::fmt;

/// Contract identifier implemented by this crate.
pub const CONTAINER_CONTRACT: &str = "GY-GFXC/1";

/// Specification version this crate writes.
pub const SPEC_VERSION: Version = Version {
    major: 0,
    minor: 0,
    patch: 1,
};

/// Fixed header length in bytes.
pub const HEADER_LEN: usize = 80;
/// Section directory entry length in bytes.
pub const DIR_ENTRY_LEN: usize = 72;
/// Byte range of the header's content digest field.
const DIGEST_FIELD: std::ops::Range<usize> = 40..72;
/// Byte range of the header's reserved field.
const RESERVED_FIELD: std::ops::Range<usize> = 72..80;

/// Smallest permitted section alignment, as log2 (8 bytes).
pub const MIN_ALIGN_LOG2: u8 = 3;
/// Largest permitted section alignment, as log2 (4096 bytes: one page, for
/// memory-mapped access).
pub const MAX_ALIGN_LOG2: u8 = 12;

/// Feature-flag bits 0–15 are *required*: a reader that does not know a set
/// bit must reject the file. Bits 16–31 are *advisory* and may be ignored.
pub const REQUIRED_FEATURE_MASK: u32 = 0x0000_FFFF;
/// Required feature bits defined by this version (none yet).
pub const KNOWN_REQUIRED_FEATURES: u32 = 0;

/// Section flag: a reader that does not understand this section must reject
/// the file. All other flag bits are reserved and must be zero.
pub const SECTION_MANDATORY: u32 = 1;

/// Section codec: bytes are stored verbatim (`decoded_len == stored_len`).
/// No other codec is defined in 0.0.x.
pub const CODEC_STORED: u16 = 0;

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// A `major.minor.patch` container version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Version {
    /// Reader compatibility rule. While the major version is 0, the minor
    /// version is also breaking, so both must match; any patch is accepted
    /// because patch releases never change the layout.
    pub const fn readable(self) -> bool {
        self.major == SPEC_VERSION.major && self.minor == SPEC_VERSION.minor
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The four graphics formats sharing this envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Graphical Rendering Unit.
    Gru,
    /// Graphical Vector Array.
    Gva,
    /// Graphical Vector Inventory.
    Gvi,
    /// Spatial Probabilistic Compression.
    Spc,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::Gru, Format::Gva, Format::Gvi, Format::Spc];

    /// The four magic bytes at offset 0.
    pub const fn magic(self) -> [u8; 4] {
        match self {
            Format::Gru => *b"GRU\0",
            Format::Gva => *b"GVA\0",
            Format::Gvi => *b"GVI\0",
            Format::Spc => *b"SPC\0",
        }
    }

    pub fn from_magic(magic: [u8; 4]) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.magic() == magic)
    }

    /// File extension without the dot.
    pub const fn extension(self) -> &'static str {
        match self {
            Format::Gru => "gru",
            Format::Gva => "gva",
            Format::Gvi => "gvi",
            Format::Spc => "spc",
        }
    }
}

/// Compatibility profile (header offset 36). Unknown profiles are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    /// Millipoint fixed-point geometry, sRGB-encoded 8-bit colour,
    /// straight (non-premultiplied) alpha at rest. See
    /// `docs/graphics/container.md` §5.
    Baseline,
}

impl Profile {
    pub const fn id(self) -> u32 {
        match self {
            Profile::Baseline => 0,
        }
    }

    pub const fn from_id(id: u32) -> Option<Profile> {
        match id {
            0 => Some(Profile::Baseline),
            _ => None,
        }
    }
}

/// A four-byte section tag made of `A–Z` and `0–9` (e.g. `GEOM`, `INST`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag([u8; 4]);

impl Tag {
    /// Build a tag in a `const` context. Invalid tags fail to compile there,
    /// and panic at runtime elsewhere; use [`Tag::parse`] for untrusted input.
    pub const fn new(bytes: &[u8; 4]) -> Tag {
        assert!(
            Tag::valid(bytes),
            "section tags are four bytes of A-Z or 0-9"
        );
        Tag(*bytes)
    }

    pub const fn parse(bytes: [u8; 4]) -> Option<Tag> {
        if Tag::valid(&bytes) {
            Some(Tag(bytes))
        } else {
            None
        }
    }

    const fn valid(bytes: &[u8; 4]) -> bool {
        let mut i = 0;
        while i < 4 {
            let c = bytes[i];
            if !(c.is_ascii_uppercase() || c.is_ascii_digit()) {
                return false;
            }
            i += 1;
        }
        true
    }

    pub const fn bytes(self) -> [u8; 4] {
        self.0
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Valid tags are ASCII by construction.
        for &b in &self.0 {
            write!(f, "{}", b as char)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({self})")
    }
}

// ---------------------------------------------------------------------------
// Errors (GFX-E1xxx structural, GFX-E4xxx budget)
// ---------------------------------------------------------------------------

/// What went wrong. Each kind maps to a stable `GFX-E` code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    Truncated,
    BadMagic,
    WrongFormat,
    UnsupportedVersion,
    BadHeaderLength,
    UnknownRequiredFeature,
    UnknownProfile,
    LengthMismatch,
    NonCanonicalLayout,
    UnsortedOrDuplicateSection,
    UnknownMandatorySection,
    UnsupportedCodec,
    NonZeroReserved,
    InvalidTag,
    BadAlignment,
    HeaderDigestMismatch,
    SectionDigestMismatch,
    NonZeroPadding,
    ArithmeticOverflow,
    FileTooLarge,
    TooManySections,
    SectionTooLarge,
}

impl ErrorKind {
    /// Numeric part of the `GFX-E` code.
    pub const fn code(self) -> u16 {
        match self {
            ErrorKind::Truncated => 1001,
            ErrorKind::BadMagic => 1002,
            ErrorKind::WrongFormat => 1003,
            ErrorKind::UnsupportedVersion => 1004,
            ErrorKind::BadHeaderLength => 1005,
            ErrorKind::UnknownRequiredFeature => 1006,
            ErrorKind::UnknownProfile => 1007,
            ErrorKind::LengthMismatch => 1008,
            ErrorKind::NonCanonicalLayout => 1009,
            ErrorKind::UnsortedOrDuplicateSection => 1010,
            ErrorKind::UnknownMandatorySection => 1011,
            ErrorKind::UnsupportedCodec => 1012,
            ErrorKind::NonZeroReserved => 1013,
            ErrorKind::InvalidTag => 1014,
            ErrorKind::BadAlignment => 1015,
            ErrorKind::HeaderDigestMismatch => 1016,
            ErrorKind::SectionDigestMismatch => 1017,
            ErrorKind::NonZeroPadding => 1018,
            ErrorKind::ArithmeticOverflow => 1019,
            ErrorKind::FileTooLarge => 4001,
            ErrorKind::TooManySections => 4002,
            ErrorKind::SectionTooLarge => 4003,
        }
    }

    /// Recommended remediation, for diagnostics.
    pub const fn remediation(self) -> &'static str {
        match self {
            ErrorKind::Truncated | ErrorKind::LengthMismatch => {
                "the file is incomplete or was altered after encoding; re-fetch or re-compile it"
            }
            ErrorKind::BadMagic => "this is not a Goya graphics file",
            ErrorKind::WrongFormat => "pass the file to the decoder for its own format",
            ErrorKind::UnsupportedVersion
            | ErrorKind::UnknownRequiredFeature
            | ErrorKind::UnknownProfile
            | ErrorKind::UnknownMandatorySection
            | ErrorKind::UnsupportedCodec => {
                "the file needs a newer decoder; upgrade Goya or recompile the asset for this target"
            }
            ErrorKind::HeaderDigestMismatch | ErrorKind::SectionDigestMismatch => {
                "the file is corrupt; re-fetch or re-compile it"
            }
            ErrorKind::FileTooLarge | ErrorKind::TooManySections | ErrorKind::SectionTooLarge => {
                "raise the decoder's resource limits for this target, or split the asset"
            }
            _ => "the file is malformed; re-compile it with a conforming encoder",
        }
    }
}

/// A container error with its code, the affected section (if any) and a
/// human-readable detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GfxError {
    pub kind: ErrorKind,
    pub section: Option<Tag>,
    pub detail: String,
}

impl GfxError {
    fn new(kind: ErrorKind, detail: impl Into<String>) -> GfxError {
        GfxError {
            kind,
            section: None,
            detail: detail.into(),
        }
    }

    fn at(kind: ErrorKind, section: Tag, detail: impl Into<String>) -> GfxError {
        GfxError {
            kind,
            section: Some(section),
            detail: detail.into(),
        }
    }

    /// The stable diagnostic code, e.g. `GFX-E1016`.
    pub fn code(&self) -> String {
        format!("GFX-E{}", self.kind.code())
    }
}

impl fmt::Display for GfxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.detail)?;
        if let Some(tag) = self.section {
            write!(f, " (section {tag})")?;
        }
        write!(f, "; {}", self.kind.remediation())
    }
}

impl std::error::Error for GfxError {}

fn overflow(what: &str) -> GfxError {
    GfxError::new(ErrorKind::ArithmeticOverflow, format!("{what} overflows"))
}

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

/// Resource limits enforced before allocation, by both reader and writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_file_len: u64,
    pub max_sections: u32,
    /// Applies to both stored and decoded section lengths.
    pub max_section_len: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_file_len: 256 << 20,
            max_sections: 64,
            max_section_len: 64 << 20,
        }
    }
}

// ---------------------------------------------------------------------------
// Layout arithmetic shared by reader and writer
// ---------------------------------------------------------------------------

fn align_up(value: u64, align_log2: u8) -> Option<u64> {
    let mask = (1u64 << align_log2) - 1;
    value.checked_add(mask).map(|v| v & !mask)
}

fn directory_end(count: u32) -> Option<u64> {
    u64::from(count)
        .checked_mul(DIR_ENTRY_LEN as u64)?
        .checked_add(HEADER_LEN as u64)
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// SHA-256 over `header ‖ directory` with the digest field zeroed.
fn envelope_digest(envelope: &[u8]) -> [u8; 32] {
    sha256(&[
        &envelope[..DIGEST_FIELD.start],
        &[0u8; 32],
        &envelope[DIGEST_FIELD.end..],
    ])
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn u64_at(b: &[u8], off: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[off..off + 8]);
    u64::from_le_bytes(a)
}

fn to_usize(v: u64, what: &str) -> Result<usize, GfxError> {
    usize::try_from(v).map_err(|_| overflow(what))
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

/// Decoded header fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub format: Format,
    pub version: Version,
    pub features: u32,
    pub total_len: u64,
    pub section_count: u32,
    pub profile: Profile,
    pub digest: [u8; 32],
}

/// One validated directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionEntry {
    pub tag: Tag,
    pub mandatory: bool,
    pub offset: u64,
    pub stored_len: u64,
    pub decoded_len: u64,
    pub codec: u16,
    pub align_log2: u8,
    pub version: u16,
    pub digest: [u8; 32],
    /// Whether the consumer declared this tag in [`Expect::known_sections`].
    /// Unknown sections are always optional (unknown mandatory ones are
    /// rejected at open).
    pub known: bool,
}

/// What a consumer expects to read.
#[derive(Debug, Clone, Copy)]
pub struct Expect<'k> {
    pub format: Format,
    /// Section tags the consumer understands. An unknown *mandatory* section
    /// causes rejection; unknown optional sections are skipped.
    pub known_sections: &'k [Tag],
}

/// A validated container over borrowed (possibly memory-mapped) bytes.
///
/// [`Container::open`] validates the header, the directory and the header
/// digest without touching section bytes. Section bytes are verified when
/// they are fetched ([`Container::section`]) or all at once
/// ([`Container::verify_all`]).
#[derive(Debug, Clone)]
pub struct Container<'a> {
    bytes: &'a [u8],
    header: Header,
    entries: Vec<SectionEntry>,
}

impl<'a> Container<'a> {
    pub fn open(bytes: &'a [u8], expect: &Expect<'_>, limits: &Limits) -> Result<Self, GfxError> {
        if bytes.len() < HEADER_LEN {
            return Err(GfxError::new(
                ErrorKind::Truncated,
                format!(
                    "{} bytes is shorter than the {HEADER_LEN}-byte header",
                    bytes.len()
                ),
            ));
        }
        let actual_len = u64::try_from(bytes.len()).map_err(|_| overflow("file length"))?;
        if actual_len > limits.max_file_len {
            return Err(GfxError::new(
                ErrorKind::FileTooLarge,
                format!(
                    "{actual_len} bytes exceeds the limit of {}",
                    limits.max_file_len
                ),
            ));
        }
        let h = &bytes[..HEADER_LEN];

        let magic = [h[0], h[1], h[2], h[3]];
        let format = Format::from_magic(magic)
            .ok_or_else(|| GfxError::new(ErrorKind::BadMagic, format!("magic {magic:02x?}")))?;
        if format != expect.format {
            return Err(GfxError::new(
                ErrorKind::WrongFormat,
                format!(
                    ".{} file given to a .{} decoder",
                    format.extension(),
                    expect.format.extension()
                ),
            ));
        }
        let version = Version {
            major: u16_at(h, 4),
            minor: u16_at(h, 6),
            patch: u16_at(h, 8),
        };
        if !version.readable() {
            return Err(GfxError::new(
                ErrorKind::UnsupportedVersion,
                format!(
                    "container {version}; this decoder reads {}.{}.x",
                    SPEC_VERSION.major, SPEC_VERSION.minor
                ),
            ));
        }
        let header_len = u16_at(h, 10);
        if usize::from(header_len) != HEADER_LEN {
            return Err(GfxError::new(
                ErrorKind::BadHeaderLength,
                format!("header length {header_len}, expected {HEADER_LEN}"),
            ));
        }
        let features = u32_at(h, 12);
        let unknown_required = features & REQUIRED_FEATURE_MASK & !KNOWN_REQUIRED_FEATURES;
        if unknown_required != 0 {
            return Err(GfxError::new(
                ErrorKind::UnknownRequiredFeature,
                format!("required feature bits {unknown_required:#06x}"),
            ));
        }
        let total_len = u64_at(h, 16);
        if total_len > actual_len {
            return Err(GfxError::new(
                ErrorKind::Truncated,
                format!("header declares {total_len} bytes, file has {actual_len}"),
            ));
        }
        if total_len < actual_len {
            return Err(GfxError::new(
                ErrorKind::LengthMismatch,
                format!("header declares {total_len} bytes, file has {actual_len}"),
            ));
        }
        let dir_offset = u64_at(h, 24);
        if dir_offset != HEADER_LEN as u64 {
            return Err(GfxError::new(
                ErrorKind::NonCanonicalLayout,
                format!("directory at {dir_offset}, must follow the header at {HEADER_LEN}"),
            ));
        }
        let section_count = u32_at(h, 32);
        if section_count > limits.max_sections {
            return Err(GfxError::new(
                ErrorKind::TooManySections,
                format!(
                    "{section_count} sections exceeds the limit of {}",
                    limits.max_sections
                ),
            ));
        }
        let profile_id = u32_at(h, 36);
        let profile = Profile::from_id(profile_id).ok_or_else(|| {
            GfxError::new(ErrorKind::UnknownProfile, format!("profile {profile_id}"))
        })?;
        if h[RESERVED_FIELD].iter().any(|&b| b != 0) {
            return Err(GfxError::new(
                ErrorKind::NonZeroReserved,
                "header reserved bytes are not zero",
            ));
        }

        let dir_end = directory_end(section_count).ok_or_else(|| overflow("directory end"))?;
        if dir_end > total_len {
            return Err(GfxError::new(
                ErrorKind::Truncated,
                format!("directory of {section_count} entries ends past the file"),
            ));
        }
        let envelope = &bytes[..to_usize(dir_end, "directory end")?];
        let mut digest = [0u8; 32];
        digest.copy_from_slice(&h[DIGEST_FIELD]);
        if envelope_digest(envelope) != digest {
            return Err(GfxError::new(
                ErrorKind::HeaderDigestMismatch,
                "header/directory digest does not match",
            ));
        }

        // Bounded by limits.max_sections, checked above.
        let mut entries = Vec::with_capacity(section_count as usize);
        let mut cursor = dir_end;
        let mut previous: Option<Tag> = None;
        for raw in envelope[HEADER_LEN..].chunks_exact(DIR_ENTRY_LEN) {
            let entry = parse_entry(raw, expect, limits)?;
            if previous.is_some_and(|p| p >= entry.tag) {
                return Err(GfxError::at(
                    ErrorKind::UnsortedOrDuplicateSection,
                    entry.tag,
                    "directory entries must be in strictly ascending tag order",
                ));
            }
            previous = Some(entry.tag);
            let expected =
                align_up(cursor, entry.align_log2).ok_or_else(|| overflow("section offset"))?;
            if entry.offset != expected {
                return Err(GfxError::at(
                    ErrorKind::NonCanonicalLayout,
                    entry.tag,
                    format!(
                        "section at {}, canonical offset is {expected}",
                        entry.offset
                    ),
                ));
            }
            cursor = expected
                .checked_add(entry.stored_len)
                .ok_or_else(|| overflow("section end"))?;
            if cursor > total_len {
                return Err(GfxError::at(
                    ErrorKind::Truncated,
                    entry.tag,
                    "section ends past the file",
                ));
            }
            entries.push(entry);
        }
        if cursor != total_len {
            return Err(GfxError::new(
                ErrorKind::NonCanonicalLayout,
                format!(
                    "{} trailing bytes after the last section",
                    total_len - cursor
                ),
            ));
        }

        Ok(Container {
            bytes,
            header: Header {
                format,
                version,
                features,
                total_len,
                section_count,
                profile,
                digest,
            },
            entries,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Directory entries in canonical (ascending tag) order.
    pub fn entries(&self) -> &[SectionEntry] {
        &self.entries
    }

    pub fn entry(&self, tag: Tag) -> Option<&SectionEntry> {
        self.entries
            .binary_search_by(|e| e.tag.cmp(&tag))
            .ok()
            .map(|i| &self.entries[i])
    }

    /// The verified stored bytes of a section, or `None` if absent. Fails if
    /// the section's digest does not match or its codec is unsupported.
    pub fn section(&self, tag: Tag) -> Result<Option<&'a [u8]>, GfxError> {
        let Some(entry) = self.entry(tag) else {
            return Ok(None);
        };
        if entry.codec != CODEC_STORED {
            return Err(GfxError::at(
                ErrorKind::UnsupportedCodec,
                tag,
                format!("codec {} cannot be decoded", entry.codec),
            ));
        }
        let data = self.stored_bytes(entry)?;
        if sha256(&[data]) != entry.digest {
            return Err(GfxError::at(
                ErrorKind::SectionDigestMismatch,
                tag,
                "section digest does not match",
            ));
        }
        Ok(Some(data))
    }

    /// Verify every section digest and every padding byte. After this
    /// succeeds, every byte of the file has been checked.
    pub fn verify_all(&self) -> Result<(), GfxError> {
        let mut cursor =
            directory_end(self.header.section_count).ok_or_else(|| overflow("directory end"))?;
        for entry in &self.entries {
            let pad = &self.bytes
                [to_usize(cursor, "padding start")?..to_usize(entry.offset, "section offset")?];
            if pad.iter().any(|&b| b != 0) {
                return Err(GfxError::at(
                    ErrorKind::NonZeroPadding,
                    entry.tag,
                    "padding before section is not zero",
                ));
            }
            if sha256(&[self.stored_bytes(entry)?]) != entry.digest {
                return Err(GfxError::at(
                    ErrorKind::SectionDigestMismatch,
                    entry.tag,
                    "section digest does not match",
                ));
            }
            cursor = entry.offset + entry.stored_len; // validated at open
        }
        Ok(())
    }

    fn stored_bytes(&self, entry: &SectionEntry) -> Result<&'a [u8], GfxError> {
        let start = to_usize(entry.offset, "section offset")?;
        let end = to_usize(entry.offset + entry.stored_len, "section end")?; // validated at open
        Ok(&self.bytes[start..end])
    }
}

fn parse_entry(raw: &[u8], expect: &Expect<'_>, limits: &Limits) -> Result<SectionEntry, GfxError> {
    let tag_bytes = [raw[0], raw[1], raw[2], raw[3]];
    let tag = Tag::parse(tag_bytes)
        .ok_or_else(|| GfxError::new(ErrorKind::InvalidTag, format!("tag {tag_bytes:02x?}")))?;
    let flags = u32_at(raw, 4);
    if flags & !SECTION_MANDATORY != 0 {
        return Err(GfxError::at(
            ErrorKind::NonZeroReserved,
            tag,
            format!("reserved flag bits {flags:#x}"),
        ));
    }
    if raw[35] != 0 || u16_at(raw, 38) != 0 {
        return Err(GfxError::at(
            ErrorKind::NonZeroReserved,
            tag,
            "entry reserved bytes are not zero",
        ));
    }
    let mandatory = flags & SECTION_MANDATORY != 0;
    let stored_len = u64_at(raw, 16);
    let decoded_len = u64_at(raw, 24);
    if stored_len > limits.max_section_len || decoded_len > limits.max_section_len {
        return Err(GfxError::at(
            ErrorKind::SectionTooLarge,
            tag,
            format!(
                "stored {stored_len} / decoded {decoded_len} bytes exceeds the limit of {}",
                limits.max_section_len
            ),
        ));
    }
    let align_log2 = raw[34];
    if !(MIN_ALIGN_LOG2..=MAX_ALIGN_LOG2).contains(&align_log2) {
        return Err(GfxError::at(
            ErrorKind::BadAlignment,
            tag,
            format!("alignment 2^{align_log2}"),
        ));
    }
    let codec = u16_at(raw, 32);
    if codec == CODEC_STORED && decoded_len != stored_len {
        return Err(GfxError::at(
            ErrorKind::NonCanonicalLayout,
            tag,
            "a stored section's decoded length must equal its stored length",
        ));
    }
    let known = expect.known_sections.contains(&tag);
    if mandatory && !known {
        return Err(GfxError::at(
            ErrorKind::UnknownMandatorySection,
            tag,
            "mandatory section is not understood by this decoder",
        ));
    }
    if mandatory && codec != CODEC_STORED {
        return Err(GfxError::at(
            ErrorKind::UnsupportedCodec,
            tag,
            format!("codec {codec}"),
        ));
    }
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&raw[40..72]);
    Ok(SectionEntry {
        tag,
        mandatory,
        offset: u64_at(raw, 8),
        stored_len,
        decoded_len,
        codec,
        align_log2,
        version: u16_at(raw, 36),
        digest,
        known,
    })
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// A section to encode.
#[derive(Debug, Clone, Copy)]
pub struct SectionInput<'d> {
    pub tag: Tag,
    pub mandatory: bool,
    pub version: u16,
    pub align_log2: u8,
    pub data: &'d [u8],
}

impl<'d> SectionInput<'d> {
    /// A mandatory, 8-byte-aligned, version-0 section.
    pub fn new(tag: Tag, data: &'d [u8]) -> SectionInput<'d> {
        SectionInput {
            tag,
            mandatory: true,
            version: 0,
            align_log2: MIN_ALIGN_LOG2,
            data,
        }
    }

    pub fn optional(mut self) -> Self {
        self.mandatory = false;
        self
    }

    pub fn align_log2(mut self, align_log2: u8) -> Self {
        self.align_log2 = align_log2;
        self
    }

    pub fn version(mut self, version: u16) -> Self {
        self.version = version;
        self
    }
}

/// Encode a container. The output depends only on the arguments (section
/// order in `sections` does not matter), so equal inputs give equal bytes.
pub fn encode(
    format: Format,
    profile: Profile,
    features: u32,
    sections: &[SectionInput<'_>],
    limits: &Limits,
) -> Result<Vec<u8>, GfxError> {
    let unknown_required = features & REQUIRED_FEATURE_MASK & !KNOWN_REQUIRED_FEATURES;
    if unknown_required != 0 {
        return Err(GfxError::new(
            ErrorKind::UnknownRequiredFeature,
            format!("required feature bits {unknown_required:#06x} are not defined"),
        ));
    }
    let count = u32::try_from(sections.len())
        .ok()
        .filter(|&c| c <= limits.max_sections)
        .ok_or_else(|| {
            GfxError::new(
                ErrorKind::TooManySections,
                format!(
                    "{} sections exceeds the limit of {}",
                    sections.len(),
                    limits.max_sections
                ),
            )
        })?;

    let mut order: Vec<&SectionInput<'_>> = sections.iter().collect();
    order.sort_by_key(|s| s.tag);
    for pair in order.windows(2) {
        if pair[0].tag == pair[1].tag {
            return Err(GfxError::at(
                ErrorKind::UnsortedOrDuplicateSection,
                pair[0].tag,
                "duplicate section tag",
            ));
        }
    }

    let dir_end = directory_end(count).ok_or_else(|| overflow("directory end"))?;
    let mut offsets = Vec::with_capacity(order.len());
    let mut cursor = dir_end;
    for s in &order {
        if !(MIN_ALIGN_LOG2..=MAX_ALIGN_LOG2).contains(&s.align_log2) {
            return Err(GfxError::at(
                ErrorKind::BadAlignment,
                s.tag,
                format!("alignment 2^{}", s.align_log2),
            ));
        }
        let len = u64::try_from(s.data.len()).map_err(|_| overflow("section length"))?;
        if len > limits.max_section_len {
            return Err(GfxError::at(
                ErrorKind::SectionTooLarge,
                s.tag,
                format!(
                    "{len} bytes exceeds the limit of {}",
                    limits.max_section_len
                ),
            ));
        }
        let offset = align_up(cursor, s.align_log2).ok_or_else(|| overflow("section offset"))?;
        cursor = offset
            .checked_add(len)
            .ok_or_else(|| overflow("section end"))?;
        offsets.push(offset);
    }
    let total_len = cursor;
    if total_len > limits.max_file_len {
        return Err(GfxError::new(
            ErrorKind::FileTooLarge,
            format!(
                "{total_len} bytes exceeds the limit of {}",
                limits.max_file_len
            ),
        ));
    }

    let mut out = vec![0u8; to_usize(total_len, "file length")?];
    out[0..4].copy_from_slice(&format.magic());
    out[4..6].copy_from_slice(&SPEC_VERSION.major.to_le_bytes());
    out[6..8].copy_from_slice(&SPEC_VERSION.minor.to_le_bytes());
    out[8..10].copy_from_slice(&SPEC_VERSION.patch.to_le_bytes());
    out[10..12].copy_from_slice(&(HEADER_LEN as u16).to_le_bytes());
    out[12..16].copy_from_slice(&features.to_le_bytes());
    out[16..24].copy_from_slice(&total_len.to_le_bytes());
    out[24..32].copy_from_slice(&(HEADER_LEN as u64).to_le_bytes());
    out[32..36].copy_from_slice(&count.to_le_bytes());
    out[36..40].copy_from_slice(&profile.id().to_le_bytes());

    for (i, (s, &offset)) in order.iter().zip(&offsets).enumerate() {
        let len = s.data.len() as u64; // checked above
        let e = HEADER_LEN + i * DIR_ENTRY_LEN;
        let flags = if s.mandatory { SECTION_MANDATORY } else { 0 };
        out[e..e + 4].copy_from_slice(&s.tag.bytes());
        out[e + 4..e + 8].copy_from_slice(&flags.to_le_bytes());
        out[e + 8..e + 16].copy_from_slice(&offset.to_le_bytes());
        out[e + 16..e + 24].copy_from_slice(&len.to_le_bytes());
        out[e + 24..e + 32].copy_from_slice(&len.to_le_bytes());
        out[e + 32..e + 34].copy_from_slice(&CODEC_STORED.to_le_bytes());
        out[e + 34] = s.align_log2;
        out[e + 36..e + 38].copy_from_slice(&s.version.to_le_bytes());
        out[e + 40..e + 72].copy_from_slice(&sha256(&[s.data]));
        let start = offset as usize; // offset < total_len, which fits usize
        out[start..start + s.data.len()].copy_from_slice(s.data);
    }

    let digest = envelope_digest(&out[..dir_end as usize]);
    out[DIGEST_FIELD].copy_from_slice(&digest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEOM: Tag = Tag::new(b"GEOM");
    const INST: Tag = Tag::new(b"INST");
    const META: Tag = Tag::new(b"META");
    const XTRA: Tag = Tag::new(b"XTRA");

    fn expect(format: Format) -> Expect<'static> {
        Expect {
            format,
            known_sections: &[GEOM, INST, META],
        }
    }

    fn sample() -> Vec<u8> {
        encode(
            Format::Gva,
            Profile::Baseline,
            0,
            &[
                SectionInput::new(INST, b"instances"),
                SectionInput::new(GEOM, b"geometry pool").align_log2(4),
                SectionInput::new(META, b"built by tests").optional(),
            ],
            &Limits::default(),
        )
        .unwrap()
    }

    fn open(bytes: &[u8]) -> Result<Container<'_>, GfxError> {
        Container::open(bytes, &expect(Format::Gva), &Limits::default())
    }

    /// Recompute the envelope digest after a deliberate header/directory edit,
    /// so a test reaches the check it targets instead of the digest check.
    fn reseal(bytes: &mut [u8]) {
        let count = u32_at(bytes, 32);
        let end = (HEADER_LEN + count as usize * DIR_ENTRY_LEN).min(bytes.len());
        let d = envelope_digest(&bytes[..end]);
        bytes[DIGEST_FIELD].copy_from_slice(&d);
    }

    fn kind(r: Result<Container<'_>, GfxError>) -> ErrorKind {
        r.expect_err("expected rejection").kind
    }

    #[test]
    fn round_trip_and_canonical_order() {
        let bytes = sample();
        let c = open(&bytes).unwrap();
        c.verify_all().unwrap();
        assert_eq!(c.header().format, Format::Gva);
        assert_eq!(c.header().version, SPEC_VERSION);
        assert_eq!(c.header().total_len, bytes.len() as u64);
        let tags: Vec<Tag> = c.entries().iter().map(|e| e.tag).collect();
        assert_eq!(tags, vec![GEOM, INST, META]);
        assert_eq!(c.section(GEOM).unwrap(), Some(&b"geometry pool"[..]));
        assert_eq!(c.section(INST).unwrap(), Some(&b"instances"[..]));
        assert_eq!(c.section(XTRA).unwrap(), None);
        assert!(!c.entry(META).unwrap().mandatory);
        assert_eq!(c.entry(GEOM).unwrap().offset % 16, 0);
    }

    #[test]
    fn encoding_is_deterministic_and_order_independent() {
        let a = sample();
        let b = encode(
            Format::Gva,
            Profile::Baseline,
            0,
            &[
                SectionInput::new(META, b"built by tests").optional(),
                SectionInput::new(GEOM, b"geometry pool").align_log2(4),
                SectionInput::new(INST, b"instances"),
            ],
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn golden_fixture_is_stable() {
        // Pins the byte layout. Regenerate only with a spec version bump:
        //   GOYA_GFX_BLESS=1 cargo test -p goya-graphics-container golden
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("fixtures")
            .join("graphics")
            .join("container")
            .join("sample.gva");
        let bytes = sample();
        if std::env::var_os("GOYA_GFX_BLESS").is_some() {
            std::fs::write(&path, &bytes).unwrap();
        }
        let golden = std::fs::read(&path).expect("golden fixture missing; see comment to bless");
        assert_eq!(bytes, golden, "container byte layout changed");
    }

    #[test]
    fn empty_container() {
        let bytes = encode(Format::Spc, Profile::Baseline, 0, &[], &Limits::default()).unwrap();
        assert_eq!(bytes.len(), HEADER_LEN);
        let c = Container::open(&bytes, &expect(Format::Spc), &Limits::default()).unwrap();
        c.verify_all().unwrap();
        assert!(c.entries().is_empty());
    }

    #[test]
    fn every_format_has_distinct_magic() {
        for f in Format::ALL {
            assert_eq!(Format::from_magic(f.magic()), Some(f));
            let bytes = encode(f, Profile::Baseline, 0, &[], &Limits::default()).unwrap();
            for other in Format::ALL.into_iter().filter(|&o| o != f) {
                let r = Container::open(&bytes, &expect(other), &Limits::default());
                assert_eq!(kind(r), ErrorKind::WrongFormat);
            }
        }
    }

    #[test]
    fn digest_covers_header_and_directory() {
        let bytes = sample();
        let c = open(&bytes).unwrap();
        let dir_end = HEADER_LEN + 3 * DIR_ENTRY_LEN;
        let mut zeroed = bytes[..dir_end].to_vec();
        zeroed[DIGEST_FIELD].fill(0);
        assert_eq!(sha256(&[&zeroed]), c.header().digest);
    }

    #[test]
    fn unknown_sections() {
        let optional = encode(
            Format::Gva,
            Profile::Baseline,
            0,
            &[
                SectionInput::new(GEOM, b"g"),
                SectionInput::new(XTRA, b"future").optional(),
            ],
            &Limits::default(),
        )
        .unwrap();
        let c = open(&optional).unwrap();
        assert!(!c.entry(XTRA).unwrap().known);
        c.verify_all().unwrap();

        let mandatory = encode(
            Format::Gva,
            Profile::Baseline,
            0,
            &[SectionInput::new(XTRA, b"future")],
            &Limits::default(),
        )
        .unwrap();
        let err = open(&mandatory).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnknownMandatorySection);
        assert_eq!(err.section, Some(XTRA));
        assert!(err.to_string().starts_with("GFX-E1011"));
    }

    #[test]
    fn writer_rejects_bad_input() {
        let l = Limits::default();
        let dup = encode(
            Format::Gru,
            Profile::Baseline,
            0,
            &[SectionInput::new(GEOM, b"a"), SectionInput::new(GEOM, b"b")],
            &l,
        );
        assert_eq!(dup.unwrap_err().kind, ErrorKind::UnsortedOrDuplicateSection);
        let align = encode(
            Format::Gru,
            Profile::Baseline,
            0,
            &[SectionInput::new(GEOM, b"a").align_log2(2)],
            &l,
        );
        assert_eq!(align.unwrap_err().kind, ErrorKind::BadAlignment);
        let feat = encode(Format::Gru, Profile::Baseline, 1, &[], &l);
        assert_eq!(feat.unwrap_err().kind, ErrorKind::UnknownRequiredFeature);
        // Advisory feature bits are allowed and ignored by readers.
        let adv = encode(Format::Gru, Profile::Baseline, 1 << 16, &[], &l).unwrap();
        Container::open(&adv, &expect(Format::Gru), &l).unwrap();
        let tight = Limits {
            max_section_len: 4,
            ..l
        };
        let big = encode(
            Format::Gru,
            Profile::Baseline,
            0,
            &[SectionInput::new(GEOM, b"12345")],
            &tight,
        );
        assert_eq!(big.unwrap_err().kind, ErrorKind::SectionTooLarge);
        assert!(Tag::parse(*b"geom").is_none());
    }

    #[test]
    fn header_field_violations() {
        let base = sample();
        let case = |edit: &dyn Fn(&mut Vec<u8>), want: ErrorKind| {
            let mut b = base.clone();
            edit(&mut b);
            reseal(&mut b);
            assert_eq!(kind(open(&b)), want);
        };
        case(&|b| b[0] = b'X', ErrorKind::BadMagic);
        case(&|b| b[6] = 1, ErrorKind::UnsupportedVersion);
        case(&|b| b[4] = 1, ErrorKind::UnsupportedVersion);
        case(&|b| b[10] = 88, ErrorKind::BadHeaderLength);
        case(&|b| b[12] = 0x80, ErrorKind::UnknownRequiredFeature);
        case(&|b| b[36] = 9, ErrorKind::UnknownProfile);
        case(&|b| b[72] = 1, ErrorKind::NonZeroReserved);
        case(&|b| b[24] = 88, ErrorKind::NonCanonicalLayout);
        case(&|b| b.push(0), ErrorKind::LengthMismatch);
        case(
            &|b| b[16..24].copy_from_slice(&u64::MAX.to_le_bytes()),
            ErrorKind::Truncated,
        );
        case(
            &|b| b[32..36].copy_from_slice(&1000u32.to_le_bytes()),
            ErrorKind::TooManySections,
        );
        // Patch versions are readable.
        let mut patched = base.clone();
        patched[8] = 7;
        reseal(&mut patched);
        open(&patched).unwrap();
    }

    #[test]
    fn directory_violations() {
        let base = sample();
        let e0 = HEADER_LEN; // GEOM entry
        let e1 = HEADER_LEN + DIR_ENTRY_LEN; // INST entry
        let case = |edit: &dyn Fn(&mut Vec<u8>), want: ErrorKind| {
            let mut b = base.clone();
            edit(&mut b);
            reseal(&mut b);
            assert_eq!(kind(open(&b)), want);
        };
        case(&|b| b[e0] = b'g', ErrorKind::InvalidTag);
        case(
            &|b| b[e1..e1 + 4].copy_from_slice(b"GEOM"),
            ErrorKind::UnsortedOrDuplicateSection,
        );
        case(&|b| b[e0 + 4] = 0b11, ErrorKind::NonZeroReserved);
        case(&|b| b[e0 + 35] = 1, ErrorKind::NonZeroReserved);
        case(&|b| b[e0 + 38] = 1, ErrorKind::NonZeroReserved);
        case(&|b| b[e0 + 34] = 13, ErrorKind::BadAlignment);
        case(&|b| b[e0 + 8] += 16, ErrorKind::NonCanonicalLayout);
        case(&|b| b[e0 + 24] += 1, ErrorKind::NonCanonicalLayout);
        case(&|b| b[e0 + 32] = 1, ErrorKind::UnsupportedCodec);
        case(
            &|b| b[e0 + 16..e0 + 24].copy_from_slice(&u64::MAX.to_le_bytes()),
            ErrorKind::SectionTooLarge,
        );
        case(
            &|b| {
                b[e0 + 16..e0 + 24].copy_from_slice(&(1u64 << 20).to_le_bytes());
                b[e0 + 24..e0 + 32].copy_from_slice(&(1u64 << 20).to_le_bytes());
            },
            ErrorKind::Truncated,
        );
    }

    #[test]
    fn budgets_apply_before_work() {
        let bytes = sample();
        let tiny = Limits {
            max_file_len: 100,
            ..Limits::default()
        };
        assert_eq!(
            kind(Container::open(&bytes, &expect(Format::Gva), &tiny)),
            ErrorKind::FileTooLarge
        );
        let few = Limits {
            max_sections: 2,
            ..Limits::default()
        };
        assert_eq!(
            kind(Container::open(&bytes, &expect(Format::Gva), &few)),
            ErrorKind::TooManySections
        );
    }

    #[test]
    fn every_truncation_is_rejected() {
        let bytes = sample();
        for len in 0..bytes.len() {
            let k = kind(open(&bytes[..len]));
            assert!(
                matches!(k, ErrorKind::Truncated),
                "truncation to {len} gave {k:?}"
            );
        }
    }

    #[test]
    fn every_single_byte_corruption_is_detected() {
        let bytes = sample();
        for i in 0..bytes.len() {
            let mut b = bytes.clone();
            b[i] ^= 0x01;
            let detected = match open(&b) {
                Err(_) => true,
                Ok(c) => c.verify_all().is_err(),
            };
            assert!(detected, "flip at byte {i} went unnoticed");
        }
    }

    #[test]
    fn lazy_section_verification() {
        let bytes = sample();
        let c = open(&bytes).unwrap();
        let inst = c.entry(INST).unwrap().offset as usize;
        let mut b = bytes.clone();
        b[inst] ^= 0xff;
        // The envelope still opens; only the damaged section fails.
        let c = open(&b).unwrap();
        assert_eq!(c.section(GEOM).unwrap(), Some(&b"geometry pool"[..]));
        assert_eq!(
            c.section(INST).unwrap_err().kind,
            ErrorKind::SectionDigestMismatch
        );
        assert_eq!(
            c.verify_all().unwrap_err().kind,
            ErrorKind::SectionDigestMismatch
        );
    }

    #[test]
    fn padding_must_be_zero() {
        let bytes = sample();
        let c = open(&bytes).unwrap();
        let dir_end = HEADER_LEN + 3 * DIR_ENTRY_LEN;
        let geom = c.entry(GEOM).unwrap().offset as usize;
        assert!(geom > dir_end, "sample should have padding before GEOM");
        let mut b = bytes.clone();
        b[dir_end] = 1;
        let c = open(&b).unwrap();
        assert_eq!(c.verify_all().unwrap_err().kind, ErrorKind::NonZeroPadding);
    }

    /// Deterministic mutation fuzzing: random multi-byte edits, resealed half
    /// the time so structural checks behind the digest are exercised too. The
    /// decoder must never panic.
    #[test]
    fn mutation_fuzz_never_panics() {
        let base = sample();
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..20_000 {
            let mut b = base.clone();
            let edits = 1 + next() % 4;
            for _ in 0..edits {
                let i = (next() % b.len() as u64) as usize;
                b[i] = next() as u8;
            }
            if next() % 3 == 0 {
                b.truncate((next() % b.len() as u64) as usize);
            }
            if round % 2 == 0 && b.len() >= HEADER_LEN {
                reseal(&mut b);
            }
            if let Ok(c) = open(&b) {
                let _ = c.verify_all();
                for e in c.entries() {
                    let _ = c.section(e.tag);
                }
            }
        }
    }
}
