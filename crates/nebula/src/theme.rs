//! COSMIC theme configuration, read-only, mapped onto Goya Chroma.
//!
//! COSMIC stores one RON file per theme key under
//! `<config>/cosmic/com.system76.CosmicTheme.{Dark,Light}/v1/` and the mode
//! under `<config>/cosmic/com.system76.CosmicTheme.Mode/v1/is_dark`. Only
//! format version [`COSMIC_THEME_FORMAT`] is read; any other version is
//! refused with [`ThemeError::UnsupportedFormat`].
//!
//! Reading is tolerant per key: unknown fields inside a file are ignored, a
//! missing or unparsable file degrades that key only and is listed in
//! [`LoadedTheme::issues`]. Nothing is ever written.
//!
//! Chroma themes are relational (an anchor plus relations); COSMIC stores
//! absolute colours. [`ChromaMapping`] therefore fills Chroma's render-ready
//! [`Palette`] slots directly from COSMIC values and exposes the accent as an
//! OKLCH anchor. It does not invent relations. COSMIC geometry, which Chroma
//! has no place for, goes to [`CosmicGeometry`].

use goya_chroma::{aa_ratio, srgb_to_oklch, Oklch, Palette, Polarity, Rgb8, WcagLevel};
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

/// The only COSMIC theme config format version Nebula reads.
pub const COSMIC_THEME_FORMAT: &str = "v1";

/// Maximum bytes Nebula will read from any one COSMIC theme key.
///
/// Real theme keys in the qualification fixture are under 4 KiB. The 64 KiB
/// limit leaves ample room for future palette data while bounding compiler
/// input memory even when a config file is unexpectedly large.
pub const MAX_THEME_FILE_BYTES: usize = 64 * 1024;

const DARK_DIR: &str = "com.system76.CosmicTheme.Dark";
const LIGHT_DIR: &str = "com.system76.CosmicTheme.Light";
const MODE_DIR: &str = "com.system76.CosmicTheme.Mode";

/// Keys Nebula reads. Other files in the directory are ignored.
pub const KEYS: &[&str] = &[
    "name",
    "is_dark",
    "is_high_contrast",
    "is_frosted",
    "accent",
    "background",
    "primary",
    "secondary",
    "destructive",
    "warning",
    "success",
    "corner_radii",
    "spacing",
    "gaps",
    "active_hint",
];

/// Theme variant directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    Dark,
    Light,
}

impl Variant {
    fn dir(self) -> &'static str {
        match self {
            Variant::Dark => DARK_DIR,
            Variant::Light => LIGHT_DIR,
        }
    }
}

/// Where COSMIC config lives. Defaults to `$XDG_CONFIG_HOME` or
/// `$HOME/.config`; tests pass a fixture root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSource {
    /// The XDG config root (the directory that contains `cosmic/`).
    pub config_root: PathBuf,
}

impl ThemeSource {
    pub fn new(config_root: impl Into<PathBuf>) -> Self {
        ThemeSource {
            config_root: config_root.into(),
        }
    }

    /// `$XDG_CONFIG_HOME` when set and absolute, else `$HOME/.config`.
    pub fn from_env() -> Option<Self> {
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            let p = PathBuf::from(x);
            if p.is_absolute() {
                return Some(Self::new(p));
            }
        }
        std::env::var_os("HOME").map(|h| Self::new(PathBuf::from(h).join(".config")))
    }

    fn cosmic(&self) -> PathBuf {
        self.config_root.join("cosmic")
    }

    /// Format version directories present for a component (sorted).
    pub fn versions(&self, component: &str) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.cosmic().join(component))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        v.sort();
        v
    }
}

/// Explicitly refuse any format version other than `v1`.
pub fn check_format(version: &str) -> Result<(), ThemeError> {
    if version == COSMIC_THEME_FORMAT {
        Ok(())
    } else {
        Err(ThemeError::UnsupportedFormat {
            requested: version.to_string(),
            supported: COSMIC_THEME_FORMAT,
        })
    }
}

/// COSMIC colour: non-linear sRGB components in 0..=1 with alpha.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Srgba {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    #[serde(default = "one")]
    pub alpha: f32,
}

fn one() -> f32 {
    1.0
}

impl Srgba {
    /// Opaque 8-bit sRGB, ignoring alpha.
    pub fn rgb8(self) -> Rgb8 {
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgb8::new(b(self.red), b(self.green), b(self.blue))
    }

    /// Composite over an opaque background (straight alpha, sRGB space,
    /// which is how COSMIC's own iced renderer blends by default).
    pub fn over(self, under: Rgb8) -> Rgb8 {
        let a = self.alpha.clamp(0.0, 1.0);
        let top = self.rgb8();
        let mix = |t: u8, u: u8| (t as f32 * a + u as f32 * (1.0 - a)).round() as u8;
        Rgb8::new(
            mix(top.r, under.r),
            mix(top.g, under.g),
            mix(top.b, under.b),
        )
    }
}

/// COSMIC `Component`: the state colours of one interactive widget kind.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Component {
    pub base: Option<Srgba>,
    pub hover: Option<Srgba>,
    pub pressed: Option<Srgba>,
    pub selected: Option<Srgba>,
    pub selected_text: Option<Srgba>,
    pub focus: Option<Srgba>,
    pub divider: Option<Srgba>,
    pub on: Option<Srgba>,
    pub disabled: Option<Srgba>,
    pub on_disabled: Option<Srgba>,
    pub border: Option<Srgba>,
    pub disabled_border: Option<Srgba>,
}

/// COSMIC `Container`: a surface level (background, primary, secondary).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Container {
    pub base: Option<Srgba>,
    pub component: Option<Component>,
    pub divider: Option<Srgba>,
    pub on: Option<Srgba>,
    pub small_widget: Option<Srgba>,
}

/// Corner radii in logical pixels, each `(top_left, top_right,
/// bottom_right, bottom_left)` as COSMIC stores them.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct CornerRadii {
    pub radius_0: Option<[f32; 4]>,
    pub radius_xs: Option<[f32; 4]>,
    pub radius_s: Option<[f32; 4]>,
    pub radius_m: Option<[f32; 4]>,
    pub radius_l: Option<[f32; 4]>,
    pub radius_xl: Option<[f32; 4]>,
}

/// Spacing scale in logical pixels.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Spacing {
    pub space_none: Option<u16>,
    pub space_xxxs: Option<u16>,
    pub space_xxs: Option<u16>,
    pub space_xs: Option<u16>,
    pub space_s: Option<u16>,
    pub space_m: Option<u16>,
    pub space_l: Option<u16>,
    pub space_xl: Option<u16>,
    pub space_xxl: Option<u16>,
    pub space_xxxl: Option<u16>,
}

/// The keys Nebula reads from one variant directory. `None` means the key
/// was missing or unreadable; the reason is in [`LoadedTheme::issues`].
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CosmicTheme {
    pub name: Option<String>,
    pub is_dark: Option<bool>,
    pub is_high_contrast: Option<bool>,
    pub is_frosted: Option<bool>,
    pub accent: Option<Component>,
    pub background: Option<Container>,
    pub primary: Option<Container>,
    pub secondary: Option<Container>,
    pub destructive: Option<Component>,
    pub warning: Option<Component>,
    pub success: Option<Component>,
    pub corner_radii: Option<CornerRadii>,
    pub spacing: Option<Spacing>,
    /// `(inner, outer)` tiling gaps as COSMIC stores them.
    pub gaps: Option<(u32, u32)>,
    pub active_hint: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyProblem {
    Missing,
    Unreadable(String),
    TooLarge { limit_bytes: usize },
    Unparsable(String),
}

/// One key that degraded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyIssue {
    pub key: String,
    pub problem: KeyProblem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ThemeError {
    /// A version other than `v1` was requested.
    UnsupportedFormat {
        requested: String,
        supported: &'static str,
    },
    /// `v1` is absent; the versions that are present are listed.
    FormatNotPresent {
        dir: String,
        found: Vec<String>,
        supported: &'static str,
    },
    NoConfigRoot,
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeError::UnsupportedFormat {
                requested,
                supported,
            } => write!(
                f,
                "COSMIC theme format {requested} is not supported (only {supported})"
            ),
            ThemeError::FormatNotPresent {
                dir,
                found,
                supported,
            } => write!(
                f,
                "{dir}/{supported} is absent; versions present: [{}]; Nebula reads {supported} only",
                found.join(", ")
            ),
            ThemeError::NoConfigRoot => {
                write!(f, "neither XDG_CONFIG_HOME nor HOME is set")
            }
        }
    }
}

impl std::error::Error for ThemeError {}

/// A variant read from disk.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadedTheme {
    pub variant: Variant,
    pub format: &'static str,
    pub dir: PathBuf,
    /// Version directories present besides `v1`; recorded, never read.
    pub other_versions_present: Vec<String>,
    pub theme: CosmicTheme,
    pub issues: Vec<KeyIssue>,
    /// SHA-256 of the bounded source files actually passed to the RON parser.
    /// The keys and byte lengths are framed into this aggregate digest.
    pub files_sha256: String,
    /// Per-file SHA-256 values for the exact bounded byte sequences parsed.
    pub source_files: Vec<ThemeFileDigest>,
}

/// Digest metadata for one bounded theme input that was passed to the parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeFileDigest {
    /// Stable path relative to the XDG config root.
    pub key: String,
    pub byte_len: usize,
    /// SHA-256 of the exact raw bytes passed to the parser.
    pub sha256: String,
}

#[derive(Debug)]
struct ParsedThemeFile {
    key: String,
    bytes: Vec<u8>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn source_file_key(dir: &Path, key: &str) -> String {
    let component = dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    format!("cosmic/{component}/{COSMIC_THEME_FORMAT}/{key}")
}

fn source_files_digest(files: &[ParsedThemeFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update((file.key.len() as u64).to_be_bytes());
        hasher.update(file.key.as_bytes());
        hasher.update((file.bytes.len() as u64).to_be_bytes());
        hasher.update(&file.bytes);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn public_file_digests(files: &[ParsedThemeFile]) -> Vec<ThemeFileDigest> {
    files
        .iter()
        .map(|file| ThemeFileDigest {
            key: file.key.clone(),
            byte_len: file.bytes.len(),
            sha256: sha256_hex(&file.bytes),
        })
        .collect()
}

fn ron_options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

/// Parse one key's RON text. Unknown fields are ignored.
pub fn parse_key<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, String> {
    ron_options().from_str(text).map_err(|e| e.to_string())
}

fn read_key<T: for<'de> Deserialize<'de>>(
    dir: &Path,
    key: &str,
    issues: &mut Vec<KeyIssue>,
    source_files: &mut Vec<ParsedThemeFile>,
) -> Option<T> {
    let path = dir.join(key);
    let mut file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            issues.push(KeyIssue {
                key: key.into(),
                problem: KeyProblem::Missing,
            });
            return None;
        }
        Err(e) => {
            issues.push(KeyIssue {
                key: key.into(),
                problem: KeyProblem::Unreadable(e.to_string()),
            });
            return None;
        }
    };
    let mut bytes = Vec::with_capacity(MAX_THEME_FILE_BYTES.min(4096));
    let mut bounded = (&mut file).take((MAX_THEME_FILE_BYTES + 1) as u64);
    if let Err(e) = bounded.read_to_end(&mut bytes) {
        issues.push(KeyIssue {
            key: key.into(),
            problem: KeyProblem::Unreadable(e.to_string()),
        });
        return None;
    }
    if bytes.len() > MAX_THEME_FILE_BYTES {
        issues.push(KeyIssue {
            key: key.into(),
            problem: KeyProblem::TooLarge {
                limit_bytes: MAX_THEME_FILE_BYTES,
            },
        });
        return None;
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => {
            issues.push(KeyIssue {
                key: key.into(),
                problem: KeyProblem::Unreadable(e.to_string()),
            });
            return None;
        }
    };
    source_files.push(ParsedThemeFile {
        key: source_file_key(dir, key),
        bytes: text.as_bytes().to_vec(),
    });
    match parse_key(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            issues.push(KeyIssue {
                key: key.into(),
                problem: KeyProblem::Unparsable(e),
            });
            None
        }
    }
}

fn format_dir(source: &ThemeSource, component: &str) -> Result<(PathBuf, Vec<String>), ThemeError> {
    let found = source.versions(component);
    let dir = source.cosmic().join(component).join(COSMIC_THEME_FORMAT);
    if !dir.is_dir() {
        return Err(ThemeError::FormatNotPresent {
            dir: source.cosmic().join(component).display().to_string(),
            found,
            supported: COSMIC_THEME_FORMAT,
        });
    }
    let others = found
        .into_iter()
        .filter(|v| v != COSMIC_THEME_FORMAT)
        .collect();
    Ok((dir, others))
}

/// Load one variant, format `v1`.
pub fn load(source: &ThemeSource, variant: Variant) -> Result<LoadedTheme, ThemeError> {
    load_version(source, variant, COSMIC_THEME_FORMAT)
}

/// Load one variant at an explicit format version. Anything but `v1` is
/// refused before the filesystem is touched.
pub fn load_version(
    source: &ThemeSource,
    variant: Variant,
    version: &str,
) -> Result<LoadedTheme, ThemeError> {
    load_version_with_files(source, variant, version).map(|(loaded, _)| loaded)
}

fn load_version_with_files(
    source: &ThemeSource,
    variant: Variant,
    version: &str,
) -> Result<(LoadedTheme, Vec<ParsedThemeFile>), ThemeError> {
    check_format(version)?;
    let (dir, other_versions_present) = format_dir(source, variant.dir())?;
    let mut issues = Vec::new();
    let mut source_files = Vec::new();
    let i = &mut issues;
    let f = &mut source_files;
    let theme = CosmicTheme {
        name: read_key(&dir, "name", i, f),
        is_dark: read_key(&dir, "is_dark", i, f),
        is_high_contrast: read_key(&dir, "is_high_contrast", i, f),
        is_frosted: read_key(&dir, "is_frosted", i, f),
        accent: read_key(&dir, "accent", i, f),
        background: read_key(&dir, "background", i, f),
        primary: read_key(&dir, "primary", i, f),
        secondary: read_key(&dir, "secondary", i, f),
        destructive: read_key(&dir, "destructive", i, f),
        warning: read_key(&dir, "warning", i, f),
        success: read_key(&dir, "success", i, f),
        corner_radii: read_key(&dir, "corner_radii", i, f),
        spacing: read_key(&dir, "spacing", i, f),
        gaps: read_key(&dir, "gaps", i, f),
        active_hint: read_key(&dir, "active_hint", i, f),
    };
    let loaded = LoadedTheme {
        variant,
        format: COSMIC_THEME_FORMAT,
        dir,
        other_versions_present,
        theme,
        issues,
        files_sha256: source_files_digest(&source_files),
        source_files: public_file_digests(&source_files),
    };
    Ok((loaded, source_files))
}

/// The mode file, and which variant it selects.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mode {
    /// Value of `Mode/v1/is_dark`, when readable.
    pub is_dark: Option<bool>,
    pub selected: Variant,
    /// How `selected` was decided.
    pub basis: &'static str,
    pub issues: Vec<KeyIssue>,
}

/// Read the mode. A missing or bad mode key degrades to Dark, the COSMIC
/// default, and says so.
pub fn read_mode(source: &ThemeSource) -> Result<Mode, ThemeError> {
    read_mode_with_files(source).map(|(mode, _)| mode)
}

fn read_mode_with_files(source: &ThemeSource) -> Result<(Mode, Vec<ParsedThemeFile>), ThemeError> {
    let (dir, _) = format_dir(source, MODE_DIR)?;
    let mut issues = Vec::new();
    let mut source_files = Vec::new();
    let is_dark: Option<bool> = read_key(&dir, "is_dark", &mut issues, &mut source_files);
    let mode = match is_dark {
        Some(true) => Mode {
            is_dark,
            selected: Variant::Dark,
            basis: "Mode/v1/is_dark = true",
            issues,
        },
        Some(false) => Mode {
            is_dark,
            selected: Variant::Light,
            basis: "Mode/v1/is_dark = false",
            issues,
        },
        None => Mode {
            is_dark,
            selected: Variant::Dark,
            basis: "mode unreadable; COSMIC default (dark) assumed",
            issues,
        },
    };
    Ok((mode, source_files))
}

/// The active theme: mode plus the variant it selects.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActiveTheme {
    pub mode: Option<Mode>,
    /// Why the mode could not be read, when it could not.
    pub mode_error: Option<ThemeError>,
    pub loaded: LoadedTheme,
    /// SHA-256 over the mode and selected-variant files passed to the parser.
    pub files_sha256: String,
    /// Per-file SHA-256 values for the exact bounded byte sequences parsed.
    pub source_files: Vec<ThemeFileDigest>,
}

/// Load the variant the mode selects (Dark when the mode is unreadable).
pub fn load_active(source: &ThemeSource) -> Result<ActiveTheme, ThemeError> {
    let (mode, mode_error, mut source_files) = match read_mode_with_files(source) {
        Ok((mode, files)) => (Some(mode), None, files),
        Err(e) => (None, Some(e), Vec::new()),
    };
    let variant = mode.as_ref().map_or(Variant::Dark, |m| m.selected);
    let (loaded, variant_files) = load_version_with_files(source, variant, COSMIC_THEME_FORMAT)?;
    source_files.extend(variant_files);
    let files_sha256 = source_files_digest(&source_files);
    let public_source_files = public_file_digests(&source_files);
    Ok(ActiveTheme {
        loaded,
        mode,
        mode_error,
        files_sha256,
        source_files: public_source_files,
    })
}

// ---------------------------------------------------------------------------
// Chroma mapping

fn ser_rgb<S: Serializer>(v: &Option<Rgb8>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(c) => s.serialize_str(&c.to_string()),
        None => s.serialize_none(),
    }
}

fn ser_oklch<S: Serializer>(v: &Option<Oklch>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(c) => {
            let round = |x: f64, p: f64| (x * p).round() / p;
            [round(c.l, 1e4), round(c.c, 1e4), round(c.h, 1e2)].serialize(s)
        }
        None => s.serialize_none(),
    }
}

fn ser_polarity<S: Serializer>(p: &Polarity, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&p.to_string())
}

fn ser_wcag<S: Serializer>(w: &WcagLevel, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(match w {
        WcagLevel::AA => "AA",
        WcagLevel::AAA => "AAA",
    })
}

/// Colours mapped onto Chroma's roles, plus the COSMIC roles Chroma's
/// palette has no slot for. Each is `None` when its source key degraded.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MappedRoles {
    /// Palette `bg` ← `background.base`.
    #[serde(serialize_with = "ser_rgb")]
    pub bg: Option<Rgb8>,
    /// Palette `fg` ← `background.on`.
    #[serde(serialize_with = "ser_rgb")]
    pub fg: Option<Rgb8>,
    /// Palette `muted` ← `fg` blended 55% toward `bg` (Chroma's documented
    /// rule; COSMIC has no muted key).
    #[serde(serialize_with = "ser_rgb")]
    pub muted: Option<Rgb8>,
    /// Palette `accent` ← `accent.base`.
    #[serde(serialize_with = "ser_rgb")]
    pub accent: Option<Rgb8>,
    /// Palette `border` ← `background.divider`.
    #[serde(serialize_with = "ser_rgb")]
    pub border: Option<Rgb8>,
    /// Palette `ok` ← `success.base`.
    #[serde(serialize_with = "ser_rgb")]
    pub ok: Option<Rgb8>,
    /// Palette `err` ← `destructive.base`.
    #[serde(serialize_with = "ser_rgb")]
    pub err: Option<Rgb8>,
    /// No palette slot ← `warning.base`.
    #[serde(serialize_with = "ser_rgb")]
    pub warning: Option<Rgb8>,
    /// No palette slot ← `accent.on`.
    #[serde(serialize_with = "ser_rgb")]
    pub on_accent: Option<Rgb8>,
    /// No palette slot ← `primary.base` / `primary.on`.
    #[serde(serialize_with = "ser_rgb")]
    pub primary_container: Option<Rgb8>,
    #[serde(serialize_with = "ser_rgb")]
    pub on_primary_container: Option<Rgb8>,
    /// No palette slot ← `secondary.base` / `secondary.on`.
    #[serde(serialize_with = "ser_rgb")]
    pub secondary_container: Option<Rgb8>,
    #[serde(serialize_with = "ser_rgb")]
    pub on_secondary_container: Option<Rgb8>,
}

/// Contrast of one mapped pair, measured, not enforced.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContrastCheck {
    pub pair: &'static str,
    pub ratio: f64,
    pub meets_target: bool,
}

/// The closest honest mapping of a COSMIC theme onto Goya Chroma.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChromaMapping {
    #[serde(serialize_with = "ser_polarity")]
    pub polarity: Polarity,
    pub polarity_basis: &'static str,
    /// COSMIC accent as an OKLCH anchor `[l, c, h]`, for callers that build
    /// a relational Chroma theme. Nebula does not build one.
    #[serde(serialize_with = "ser_oklch")]
    pub anchor: Option<Oklch>,
    pub roles: MappedRoles,
    /// True when all seven Chroma palette slots were filled.
    pub palette_complete: bool,
    pub high_contrast: Option<bool>,
    /// AAA under COSMIC high contrast, AA otherwise.
    #[serde(serialize_with = "ser_wcag")]
    pub wcag_target: WcagLevel,
    pub contrast: Vec<ContrastCheck>,
    /// Palette slots or extras that could not be filled, with the key.
    pub unmapped: Vec<String>,
    pub notes: Vec<&'static str>,
}

const MUTED_TOWARD_BG: f64 = 0.55;

fn blend(a: Rgb8, b: Rgb8, t: f64) -> Rgb8 {
    let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    Rgb8::new(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b))
}

impl ChromaMapping {
    /// Chroma's render-ready palette, when every slot was filled.
    pub fn palette(&self) -> Option<Palette> {
        let r = &self.roles;
        Some(Palette {
            bg: r.bg?,
            fg: r.fg?,
            muted: r.muted?,
            accent: r.accent?,
            border: r.border?,
            ok: r.ok?,
            err: r.err?,
        })
    }
}

/// Map a loaded theme onto Chroma. Pure.
pub fn map_to_chroma(loaded: &LoadedTheme) -> ChromaMapping {
    let t = &loaded.theme;
    let (polarity, polarity_basis) = match t.is_dark {
        Some(true) => (Polarity::Dark, "is_dark = true"),
        Some(false) => (Polarity::Light, "is_dark = false"),
        None => match loaded.variant {
            Variant::Dark => (
                Polarity::Dark,
                "is_dark degraded; taken from the Dark directory",
            ),
            Variant::Light => (
                Polarity::Light,
                "is_dark degraded; taken from the Light directory",
            ),
        },
    };

    let mut unmapped = Vec::new();
    let bg = t.background.as_ref().and_then(|c| c.base).map(Srgba::rgb8);
    // Every other colour is composited over bg when it carries alpha; with no
    // bg it is taken opaque.
    let flat = |c: Option<Srgba>| c.map(|c| bg.map_or(c.rgb8(), |b| c.over(b)));
    let comp_base = |c: &Option<Component>| flat(c.as_ref().and_then(|c| c.base));

    let fg = flat(t.background.as_ref().and_then(|c| c.on));
    let mut roles = MappedRoles {
        bg,
        fg,
        muted: bg.zip(fg).map(|(b, f)| blend(f, b, MUTED_TOWARD_BG)),
        accent: comp_base(&t.accent),
        border: flat(t.background.as_ref().and_then(|c| c.divider)),
        ok: comp_base(&t.success),
        err: comp_base(&t.destructive),
        warning: comp_base(&t.warning),
        on_accent: None,
        primary_container: flat(t.primary.as_ref().and_then(|c| c.base)),
        on_primary_container: None,
        secondary_container: flat(t.secondary.as_ref().and_then(|c| c.base)),
        on_secondary_container: None,
    };
    // Text on a container composites over that container.
    let on = |text: Option<Srgba>, under: Option<Rgb8>| {
        text.map(|c| under.map_or(c.rgb8(), |u| c.over(u)))
    };
    roles.on_accent = on(t.accent.as_ref().and_then(|c| c.on), roles.accent);
    roles.on_primary_container = on(
        t.primary.as_ref().and_then(|c| c.on),
        roles.primary_container,
    );
    roles.on_secondary_container = on(
        t.secondary.as_ref().and_then(|c| c.on),
        roles.secondary_container,
    );

    for (name, v, key) in [
        ("bg", roles.bg, "background.base"),
        ("fg", roles.fg, "background.on"),
        ("muted", roles.muted, "background.base + background.on"),
        ("accent", roles.accent, "accent.base"),
        ("border", roles.border, "background.divider"),
        ("ok", roles.ok, "success.base"),
        ("err", roles.err, "destructive.base"),
        ("warning", roles.warning, "warning.base"),
        ("on_accent", roles.on_accent, "accent.on"),
        ("primary_container", roles.primary_container, "primary.base"),
        (
            "on_primary_container",
            roles.on_primary_container,
            "primary.on",
        ),
        (
            "secondary_container",
            roles.secondary_container,
            "secondary.base",
        ),
        (
            "on_secondary_container",
            roles.on_secondary_container,
            "secondary.on",
        ),
    ] {
        if v.is_none() {
            unmapped.push(format!("{name} (from {key})"));
        }
    }

    let wcag_target = if t.is_high_contrast == Some(true) {
        WcagLevel::AAA
    } else {
        WcagLevel::AA
    };
    let mut contrast = Vec::new();
    for (pair, a, b) in [
        ("fg/bg", roles.fg, roles.bg),
        ("muted/bg", roles.muted, roles.bg),
        ("accent/bg", roles.accent, roles.bg),
        ("ok/bg", roles.ok, roles.bg),
        ("err/bg", roles.err, roles.bg),
        ("warning/bg", roles.warning, roles.bg),
        ("on_accent/accent", roles.on_accent, roles.accent),
        (
            "on_primary/primary",
            roles.on_primary_container,
            roles.primary_container,
        ),
    ] {
        if let (Some(a), Some(b)) = (a, b) {
            let ratio = (aa_ratio(a, b) * 100.0).round() / 100.0;
            contrast.push(ContrastCheck {
                pair,
                ratio,
                meets_target: aa_ratio(a, b) >= wcag_target.ratio(),
            });
        }
    }

    let palette_complete = roles.bg.is_some()
        && roles.fg.is_some()
        && roles.muted.is_some()
        && roles.accent.is_some()
        && roles.border.is_some()
        && roles.ok.is_some()
        && roles.err.is_some();

    ChromaMapping {
        polarity,
        polarity_basis,
        anchor: roles.accent.map(srgb_to_oklch),
        roles,
        palette_complete,
        high_contrast: t.is_high_contrast,
        wcag_target,
        contrast,
        unmapped,
        notes: vec![
            "COSMIC colours are absolute; Chroma themes are relational. Values fill Chroma's Palette slots directly; no relations are synthesised.",
            "Contrast is measured and reported, not enforced: COSMIC's colours are kept as authored.",
            "muted uses Chroma's documented 55% fg-toward-bg blend; COSMIC has no muted key.",
            "warning, on_accent and the primary/secondary containers have no Chroma Palette slot and are carried as extras.",
            "Colours with alpha are composited over background.base; hover, pressed, selected and disabled states are not mapped.",
        ],
    }
}

/// COSMIC geometry tokens Chroma has no place for.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CosmicGeometry {
    pub corner_radii: Option<CornerRadii>,
    pub spacing: Option<Spacing>,
    /// `(inner, outer)` tiling gaps.
    pub gaps: Option<(u32, u32)>,
    /// Active window hint (focus outline) width.
    pub active_hint: Option<u32>,
    pub is_frosted: Option<bool>,
    pub note: &'static str,
}

pub fn geometry(loaded: &LoadedTheme) -> CosmicGeometry {
    let t = &loaded.theme;
    CosmicGeometry {
        corner_radii: t.corner_radii.clone(),
        spacing: t.spacing.clone(),
        gaps: t.gaps,
        active_hint: t.active_hint,
        is_frosted: t.is_frosted,
        note: "COSMIC values in logical pixels; recorded only, not yet consumed by Goya layout or paint",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_v1_is_accepted() {
        assert!(check_format("v1").is_ok());
        for v in ["v2", "v0", "", "V1", "1"] {
            assert!(matches!(
                check_format(v),
                Err(ThemeError::UnsupportedFormat { .. })
            ));
        }
        let src = ThemeSource::new("/nonexistent");
        assert!(matches!(
            load_version(&src, Variant::Dark, "v2"),
            Err(ThemeError::UnsupportedFormat { .. })
        ));
    }

    #[test]
    fn unknown_fields_are_ignored_and_implicit_some_works() {
        let c: Component =
            parse_key("(base: (red: 1.0, green: 0.5, blue: 0.0, alpha: 1.0), future_field: 3)")
                .unwrap();
        assert_eq!(c.base.unwrap().rgb8(), Rgb8::new(255, 128, 0));
        assert!(c.on.is_none());
        let g: (u32, u32) = parse_key("(0, 8)").unwrap();
        assert_eq!(g, (0, 8));
        let r: CornerRadii = parse_key("(radius_s: (8.0, 8.0, 8.0, 8.0))").unwrap();
        assert_eq!(r.radius_s, Some([8.0; 4]));
        let hint: Option<Srgba> = parse_key("None").unwrap();
        assert!(hint.is_none());
    }

    #[test]
    fn alpha_composites_over_background() {
        let half = Srgba {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            alpha: 0.5,
        };
        assert_eq!(half.over(Rgb8::new(0, 0, 0)), Rgb8::new(128, 128, 128));
    }
}
