//! webOS TV web-engine caprichios: platform, engine and model-year facts.
//!
//! Contract `goya.platform.webos.engine.v1`. This is a fact table, not a
//! recipe. Arachnid (the webOS TV target) uses it for three things: looking
//! up which Chromium a TV runs, refusing generated code that needs a web
//! feature the target engine lacks, and decoding an LG model number to the
//! webOS release it shipped with.
//!
//! Every fact comes from a published source, recorded on 20 November 2026:
//!
//! * The engine per platform and the user-agent strings come from LG's
//!   "Web API and Web Engine" page (webostv.developer.lge.com).
//! * Version numbers as the TV reports them (for webOS 22 and later, the
//!   marketing name minus 15) come from the Glasshouse tested-TV list,
//!   which records versions reported by 47 real TVs.
//! * Model-year → platform and series lists come from tab-tv.com
//!   "LG TV operating system by year", the Glasshouse list, and LG press
//!   material for 2026.
//! * Re:New upgrade ceilings come from LG's schedule as reported by
//!   TechRadar/FlatpanelsHD (January 2025), extended only where a TV was
//!   *observed* on a later release in the Glasshouse list.
//! * The Chromium version that first shipped each web feature comes from
//!   Chromium release history.
//!
//! A model's **shipped** platform is the oldest software it can be running,
//! because an owner can decline updates. Arachnid therefore targets the
//! shipped engine unless the caller supplies the version the TV actually
//! reports. The upgrade ceiling is informational and is never used as a
//! build target.
//!
//! Lookups fail closed. An unknown, ambiguous or pre-floor model is an
//! error, never a guess. The floor is webOS TV 4.0 (2018).

use std::fmt;

pub const CONTRACT: &str = "goya.platform.webos.engine.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

/// Declares one caprichio module per catalogued series. Each module gets its
/// own contract `goya.platform.webos.<slug>.v1`. The macro also builds
/// `CATALOG`, the list of every declared series in declaration order.
#[macro_export]
macro_rules! webos_series {
    ($(($slug:ident, $name:literal, $code:literal, $year:literal, $family:ident, $shipped:ident, $ceiling:ident)),* $(,)?) => {
        $(
            /// One LG TV series' webOS caprichio.
            pub mod $slug {
                pub const CONTRACT: &str =
                    concat!("goya.platform.webos.", stringify!($slug), ".v1");
                pub const SERIES: $crate::webos_engine_caprichio::SeriesEntry =
                    $crate::webos_engine_caprichio::SeriesEntry {
                        contract: CONTRACT,
                        name: $name,
                        code: $code,
                        model_year: $year,
                        family: $crate::webos_engine_caprichio::Family::$family,
                        shipped: $crate::webos_engine_caprichio::WebosPlatform::$shipped,
                        upgrade_ceiling: $crate::webos_engine_caprichio::WebosPlatform::$ceiling,
                    };
            }
        )*
        pub const CATALOG: &[$crate::webos_engine_caprichio::SeriesEntry] = &[$($slug::SERIES),*];
    };
}

/// A webOS TV platform release that Arachnid supports, from oldest to newest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WebosPlatform {
    Tv4_0,
    Tv4_5,
    Tv5,
    Tv6,
    Tv22,
    Tv23,
    Tv24,
    Tv25,
    Tv26,
}

/// The oldest platform Arachnid admits.
pub const FLOOR: WebosPlatform = WebosPlatform::Tv4_0;

/// A web engine, identified by its Chromium major version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebEngine {
    pub chromium_major: u16,
    /// Full `Chrome/` token from LG's published user-agent string.
    pub chrome_version: &'static str,
}

/// Published facts for one platform release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformFacts {
    pub platform: WebosPlatform,
    /// LG's marketing name.
    pub marketing: &'static str,
    /// Model year of the TVs that first shipped this release.
    pub first_model_year: u16,
    /// Major version the TV reports (`/etc/issue`, Settings, `sdkVersion`).
    pub reported_major: u8,
    /// Lowest reported minor version that belongs to this release. It is
    /// only non-zero for 4.5, which shares major 4 with 4.0.
    pub reported_minor_min: u8,
    pub engine: WebEngine,
    /// Back with no history shows an exit popup (webOS TV 6.0 and later)
    /// rather than launching Home.
    pub back_exit_popup: bool,
    /// `appinfo.json` `cloudgame_active` is honoured (webOS TV 23 and later).
    pub gamepad_passthrough: bool,
}

const fn engine(chromium_major: u16, chrome_version: &'static str) -> WebEngine {
    WebEngine {
        chromium_major,
        chrome_version,
    }
}

const fn facts(
    platform: WebosPlatform,
    marketing: &'static str,
    first_model_year: u16,
    reported_major: u8,
    reported_minor_min: u8,
    engine: WebEngine,
) -> PlatformFacts {
    let rank = platform as u8;
    PlatformFacts {
        platform,
        marketing,
        first_model_year,
        reported_major,
        reported_minor_min,
        engine,
        back_exit_popup: rank >= WebosPlatform::Tv6 as u8,
        gamepad_passthrough: rank >= WebosPlatform::Tv23 as u8,
    }
}

/// Every supported platform, oldest first. This is the quick lookup table.
pub const PLATFORMS: [PlatformFacts; 9] = [
    facts(
        WebosPlatform::Tv4_0,
        "webOS TV 4.0",
        2018,
        4,
        0,
        engine(53, "53.0.2785.34"),
    ),
    facts(
        WebosPlatform::Tv4_5,
        "webOS TV 4.5",
        2019,
        4,
        5,
        engine(53, "53.0.2785.34"),
    ),
    facts(
        WebosPlatform::Tv5,
        "webOS TV 5.0",
        2020,
        5,
        0,
        engine(68, "68.0.3440.106"),
    ),
    facts(
        WebosPlatform::Tv6,
        "webOS TV 6.0",
        2021,
        6,
        0,
        engine(79, "79.0.3945.79"),
    ),
    facts(
        WebosPlatform::Tv22,
        "webOS TV 22",
        2022,
        7,
        0,
        engine(87, "87.0.4280.88"),
    ),
    facts(
        WebosPlatform::Tv23,
        "webOS TV 23",
        2023,
        8,
        0,
        engine(94, "94.0.4606.128"),
    ),
    facts(
        WebosPlatform::Tv24,
        "webOS TV 24",
        2024,
        9,
        0,
        engine(108, "108.0.5359.211"),
    ),
    facts(
        WebosPlatform::Tv25,
        "webOS TV 25",
        2025,
        10,
        0,
        engine(120, "120.0.6099.270"),
    ),
    facts(
        WebosPlatform::Tv26,
        "webOS TV 26",
        2026,
        11,
        0,
        engine(132, "132.0.6834.207"),
    ),
];

/// Pre-floor releases, kept only so that refusals can say what a TV runs.
const BELOW_FLOOR: [(u16, &str, &str); 5] = [
    (2014, "webOS TV 1.0", "WebKit 537.41"),
    (2015, "webOS TV 2.0", "WebKit 538.2"),
    (2016, "webOS TV 3.0", "Chromium 38"),
    (2017, "webOS TV 3.5", "Chromium 38"),
    (2013, "NetCast", "not webOS"),
];

impl WebosPlatform {
    pub const ALL: [WebosPlatform; 9] = [
        Self::Tv4_0,
        Self::Tv4_5,
        Self::Tv5,
        Self::Tv6,
        Self::Tv22,
        Self::Tv23,
        Self::Tv24,
        Self::Tv25,
        Self::Tv26,
    ];

    pub fn facts(self) -> &'static PlatformFacts {
        &PLATFORMS[self as usize]
    }

    pub fn engine(self) -> WebEngine {
        self.facts().engine
    }

    /// Short identifier used on the command line and in reports: `4.0`,
    /// `4.5`, `5`, `6`, `22` … `26`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Tv4_0 => "4.0",
            Self::Tv4_5 => "4.5",
            Self::Tv5 => "5",
            Self::Tv6 => "6",
            Self::Tv22 => "22",
            Self::Tv23 => "23",
            Self::Tv24 => "24",
            Self::Tv25 => "25",
            Self::Tv26 => "26",
        }
    }

    pub fn supports(self, feature: WebFeature) -> bool {
        self.engine().chromium_major >= feature.min_chromium()
    }
}

impl fmt::Display for WebosPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.facts().marketing)
    }
}

/// Web-platform features that generated code may depend on. Each one is
/// gated by the first Chromium release that shipped it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WebFeature {
    /// `KeyboardEvent.key`.
    KeyboardEventKey,
    IntersectionObserver,
    AsyncAwait,
    PointerEvents,
    CssGrid,
    ObjectRestSpread,
    EsModules,
    ResizeObserver,
    OptionalChaining,
    NullishCoalescing,
    FlexGap,
    FocusVisible,
    AspectRatio,
}

impl WebFeature {
    pub const ALL: [WebFeature; 13] = [
        Self::KeyboardEventKey,
        Self::IntersectionObserver,
        Self::AsyncAwait,
        Self::PointerEvents,
        Self::CssGrid,
        Self::ObjectRestSpread,
        Self::EsModules,
        Self::ResizeObserver,
        Self::OptionalChaining,
        Self::NullishCoalescing,
        Self::FlexGap,
        Self::FocusVisible,
        Self::AspectRatio,
    ];

    /// First Chromium major version that shipped the feature by default.
    pub const fn min_chromium(self) -> u16 {
        match self {
            Self::KeyboardEventKey => 51,
            Self::IntersectionObserver => 51,
            Self::AsyncAwait => 55,
            Self::PointerEvents => 55,
            Self::CssGrid => 57,
            Self::ObjectRestSpread => 60,
            Self::EsModules => 61,
            Self::ResizeObserver => 64,
            Self::OptionalChaining => 80,
            Self::NullishCoalescing => 80,
            Self::FlexGap => 84,
            Self::FocusVisible => 86,
            Self::AspectRatio => 88,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::KeyboardEventKey => "keyboard-event-key",
            Self::IntersectionObserver => "intersection-observer",
            Self::AsyncAwait => "async-await",
            Self::PointerEvents => "pointer-events",
            Self::CssGrid => "css-grid",
            Self::ObjectRestSpread => "object-rest-spread",
            Self::EsModules => "es-modules",
            Self::ResizeObserver => "resize-observer",
            Self::OptionalChaining => "optional-chaining",
            Self::NullishCoalescing => "nullish-coalescing",
            Self::FlexGap => "flex-gap",
            Self::FocusVisible => "focus-visible",
            Self::AspectRatio => "aspect-ratio",
        }
    }
}

/// Product family, as encoded in the model number.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Family {
    Oled,
    Qned,
    Nanocell,
    Uhd,
    Fhd,
    Lifestyle,
}

impl Family {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Oled => "OLED",
            Self::Qned => "QNED",
            Self::Nanocell => "NanoCell",
            Self::Uhd => "UHD",
            Self::Fhd => "FHD",
            Self::Lifestyle => "Lifestyle",
        }
    }
}

/// One catalogued series.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SeriesEntry {
    pub contract: &'static str,
    pub name: &'static str,
    /// Series code as the model-number decoder produces it (`C3`, `UQ75`,
    /// `QNED80`, `SM90`, `LX3`, `ART10`). A row matches every decoded code
    /// that starts with it, so `UN8` covers `UN8000` through `UN8999`.
    pub code: &'static str,
    pub model_year: u16,
    pub family: Family,
    pub shipped: WebosPlatform,
    /// Highest release LG has scheduled or that has been observed for this
    /// series. Equal to `shipped` when no upgrade is known.
    pub upgrade_ceiling: WebosPlatform,
}

/// Why a lookup failed. Every variant is a refusal: nothing is guessed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupError {
    /// The model or version is older than [`FLOOR`].
    BelowFloor {
        model_year: u16,
        marketing: &'static str,
        engine: &'static str,
    },
    /// The model year is newer than this table.
    UnknownYear(u16),
    /// The string is not a recognisable LG model number or version.
    Unrecognised(String),
    /// The model number fits more than one model year.
    Ambiguous { model: String, years: [u16; 2] },
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BelowFloor {
                model_year,
                marketing,
                engine,
            } => write!(
                f,
                "{model_year} models run {marketing} ({engine}), below the supported floor {}",
                FLOOR
            ),
            Self::UnknownYear(year) => write!(f, "no webOS facts for model year {year}"),
            Self::Unrecognised(text) => {
                write!(f, "not a recognised LG TV model or webOS version: {text}")
            }
            Self::Ambiguous { model, years } => write!(
                f,
                "{model} could be a {} or a {} model; give the webOS version instead",
                years[0], years[1]
            ),
        }
    }
}

impl std::error::Error for LookupError {}

fn below_floor(model_year: u16) -> LookupError {
    let (_, marketing, engine) = BELOW_FLOOR
        .iter()
        .copied()
        .find(|(year, _, _)| *year == model_year)
        .unwrap_or((model_year, "pre-webOS software", "unknown engine"));
    LookupError::BelowFloor {
        model_year,
        marketing,
        engine,
    }
}

/// The platform that TVs of `model_year` shipped with.
pub fn platform_for_model_year(model_year: u16) -> Result<WebosPlatform, LookupError> {
    match model_year {
        2018 => Ok(WebosPlatform::Tv4_0),
        2019 => Ok(WebosPlatform::Tv4_5),
        2020 => Ok(WebosPlatform::Tv5),
        2021 => Ok(WebosPlatform::Tv6),
        2022 => Ok(WebosPlatform::Tv22),
        2023 => Ok(WebosPlatform::Tv23),
        2024 => Ok(WebosPlatform::Tv24),
        2025 => Ok(WebosPlatform::Tv25),
        2026 => Ok(WebosPlatform::Tv26),
        year if year < 2018 => Err(below_floor(year)),
        year => Err(LookupError::UnknownYear(year)),
    }
}

/// Parse a platform name: `4.0`, `4.5`, `5`, `5.0`, `6`, `6.0`, `22` … `26`,
/// optionally prefixed by `webOS`, `webOS TV` or `webos`.
pub fn platform_by_name(name: &str) -> Result<WebosPlatform, LookupError> {
    let lower = name.trim().to_ascii_lowercase();
    let bare = lower
        .strip_prefix("webos tv")
        .or_else(|| lower.strip_prefix("webos"))
        .unwrap_or(&lower)
        .trim();
    let platform = match bare {
        "4" | "4.0" => WebosPlatform::Tv4_0,
        "4.5" => WebosPlatform::Tv4_5,
        "5" | "5.0" => WebosPlatform::Tv5,
        "6" | "6.0" => WebosPlatform::Tv6,
        "22" => WebosPlatform::Tv22,
        "23" => WebosPlatform::Tv23,
        "24" => WebosPlatform::Tv24,
        "25" => WebosPlatform::Tv25,
        "26" => WebosPlatform::Tv26,
        "1" | "1.0" => return Err(below_floor(2014)),
        "2" | "2.0" => return Err(below_floor(2015)),
        "3" | "3.0" => return Err(below_floor(2016)),
        "3.5" => return Err(below_floor(2017)),
        _ => return Err(LookupError::Unrecognised(name.to_string())),
    };
    Ok(platform)
}

/// Map a version as the TV reports it (`9.2.0`, `4.10.2`, `11`) to its
/// platform release.
pub fn platform_by_reported_version(version: &str) -> Result<WebosPlatform, LookupError> {
    let unrecognised = || LookupError::Unrecognised(version.to_string());
    let mut parts = version.trim().split('.');
    let major: u8 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(unrecognised)?;
    let minor: u8 = match parts.next() {
        Some(p) => p.parse().map_err(|_| unrecognised())?,
        None => 0,
    };
    if parts.any(|p| p.parse::<u32>().is_err()) {
        return Err(unrecognised());
    }
    match major {
        1 => Err(below_floor(2014)),
        2 => Err(below_floor(2015)),
        3 if minor < 5 => Err(below_floor(2016)),
        3 => Err(below_floor(2017)),
        4..=11 => Ok(PLATFORMS
            .iter()
            .rev()
            .find(|f| f.reported_major == major && minor >= f.reported_minor_min)
            .map(|f| f.platform)
            .ok_or_else(unrecognised)?),
        _ => Err(unrecognised()),
    }
}

/// The engine a webOS user-agent string identifies, and every platform that
/// ships that engine (4.0 and 4.5 share Chromium 53). Strings that are not
/// webOS (`Web0S`) user agents, or that name an unknown engine, are refused.
pub fn platforms_by_user_agent(ua: &str) -> Result<Vec<WebosPlatform>, LookupError> {
    let unrecognised = || LookupError::Unrecognised(ua.to_string());
    if !ua.contains("Web0S") {
        return Err(unrecognised());
    }
    let Some(rest) = ua.split("Chrome/").nth(1) else {
        // webOS 1.x/2.x WebKit agents carry no Chrome token.
        return Err(below_floor(2015));
    };
    let major: u16 = rest
        .split('.')
        .next()
        .and_then(|m| m.parse().ok())
        .ok_or_else(unrecognised)?;
    if major == 38 {
        return Err(below_floor(2016));
    }
    let matches: Vec<WebosPlatform> = PLATFORMS
        .iter()
        .filter(|f| f.engine.chromium_major == major)
        .map(|f| f.platform)
        .collect();
    if matches.is_empty() {
        Err(unrecognised())
    } else {
        Ok(matches)
    }
}

/// A decoded LG model number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelMatch {
    pub model_year: u16,
    pub family: Family,
    /// Shipped platform: the oldest software this TV can be running.
    pub shipped: WebosPlatform,
    /// Catalogued series, when the decoded series code is in the catalog.
    /// `None` means the year and family were decoded but the series is not
    /// listed. The shipped platform is still exact, because LG ships one
    /// release per model year.
    pub series: Option<&'static SeriesEntry>,
}

impl ModelMatch {
    pub fn upgrade_ceiling(&self) -> WebosPlatform {
        self.series.map_or(self.shipped, |s| s.upgrade_ceiling)
    }
}

/// Year letter in prefix-style codes: `55UQ7570`, `65SM9000`, `43LM5760`.
fn prefix_year(letter: u8) -> Option<u16> {
    Some(match letter {
        b'B' => 2014,
        b'F' => 2015,
        b'H' => 2016,
        b'J' => 2017,
        b'K' => 2018,
        b'M' => 2019,
        b'N' => 2020,
        b'P' => 2021,
        b'Q' => 2022,
        b'R' => 2023,
        b'T' => 2024,
        b'A' => 2025,
        _ => return None,
    })
}

/// Year letter in suffix-style codes, after the series digits:
/// `65QNED85TUA`, `65QNED80URA`, `55QNED826QB`, `43NU800BPSC`, `42LX3Q6LA`.
fn suffix_year(letter: u8) -> Option<u16> {
    Some(match letter {
        b'N' => 2020,
        b'P' => 2021,
        b'Q' => 2022,
        b'R' => 2023,
        b'T' => 2024,
        b'A' => 2025,
        b'B' => 2026,
        _ => return None,
    })
}

const OLED_SERIES: &[u8] = b"ABCEGMRWZ";

/// Normalise and decode an LG model number to its model year and series.
///
/// Accepts full model numbers with or without size, region and `-` suffix:
/// `OLED65C3PUA`, `OLED77CX6LA`, `55UQ7570PUJ`, `65QNED85TUA`,
/// `55NANO85UNA`, `65SM9000PUA`, `43LM5760PTC`, `42LX3Q6LA`,
/// `27ART10AKPL`, `43NU800BPSC`.
pub fn decode_model(model: &str) -> Result<ModelMatch, LookupError> {
    let normalized: String = model
        .trim()
        .split('-')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    let unrecognised = || LookupError::Unrecognised(model.to_string());
    let bytes = normalized.as_bytes();
    if !bytes.iter().all(u8::is_ascii_alphanumeric) || bytes.is_empty() {
        return Err(unrecognised());
    }

    let (year, family, code) = if let Some(rest) = normalized.strip_prefix("OLED") {
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        let r = rest.as_bytes();
        if r.len() >= 2 && r[0] == b'C' && r[1] == b'S' {
            // CS: a 2022 C-series variant; no year character follows.
            (2022, Family::Oled, "CS".to_string())
        } else {
            if r.len() < 2 || !OLED_SERIES.contains(&r[0]) {
                return Err(unrecognised());
            }
            let year = match r[1] {
                b'6' => {
                    return Err(LookupError::Ambiguous {
                        model: model.to_string(),
                        years: [2016, 2026],
                    })
                }
                b'7' => 2017,
                b'8' => 2018,
                b'9' => 2019,
                b'X' => 2020,
                b'1' => 2021,
                b'2' => 2022,
                b'3' => 2023,
                b'4' => 2024,
                b'5' => 2025,
                _ => return Err(unrecognised()),
            };
            (
                year,
                Family::Oled,
                format!("{}{}", r[0] as char, r[1] as char),
            )
        }
    } else {
        let rest = normalized.trim_start_matches(|c: char| c.is_ascii_digit());
        if rest.len() == normalized.len() {
            // Non-OLED model numbers start with the screen size.
            return Err(unrecognised());
        }
        decode_lcd(rest).ok_or_else(unrecognised)?
    };

    let shipped = platform_for_model_year(year)?;
    let series = crate::webos_tv_models::CATALOG
        .iter()
        .filter(|s| s.model_year == year && s.family == family && code.starts_with(s.code))
        .max_by_key(|s| s.code.len());
    Ok(ModelMatch {
        model_year: year,
        family,
        shipped,
        series,
    })
}

/// Decode a non-OLED model number with its size digits already removed.
fn decode_lcd(rest: &str) -> Option<(u16, Family, String)> {
    let digits_after = |s: &str| -> (String, usize) {
        let d: String = s.chars().take_while(char::is_ascii_digit).collect();
        let n = d.len();
        (d, n)
    };
    // Suffix style: family word, series digits, year letter.
    for (word, family) in [
        ("QNED", Family::Qned),
        ("NANO", Family::Nanocell),
        ("NU", Family::Uhd),
        ("LX", Family::Lifestyle),
    ] {
        if let Some(after) = rest.strip_prefix(word) {
            let (digits, n) = digits_after(after);
            if n == 0 {
                return None;
            }
            let tail = &after.as_bytes()[n..];
            let year = match tail {
                // US codes 2020–2023: `80URA`, `85UNA`.
                [b'U', y @ (b'N' | b'P' | b'Q' | b'R'), ..] => suffix_year(*y)?,
                [y, ..] => suffix_year(*y)?,
                [] => return None,
            };
            let series = digits.get(..2.min(digits.len())).unwrap_or(&digits);
            let series = if word == "LX" { &digits[..1] } else { series };
            return Some((year, family, format!("{word}{series}")));
        }
    }
    if let Some(after) = rest.strip_prefix("ART") {
        // StanbyME 27ART10: the only model of its line, 2021.
        let (digits, n) = digits_after(after);
        return (n > 0).then(|| (2021, Family::Lifestyle, format!("ART{digits}")));
    }
    // Prefix style: U (UHD), S (Super UHD / NanoCell), L (FHD/HD), then the
    // year letter, then the series digits.
    let b = rest.as_bytes();
    if b.len() < 3 {
        return None;
    }
    let family = match b[0] {
        b'U' => Family::Uhd,
        b'S' => Family::Nanocell,
        b'L' => Family::Fhd,
        _ => return None,
    };
    let year = prefix_year(b[1])?;
    let (digits, n) = digits_after(&rest[2..]);
    if n < 2 {
        return None;
    }
    Some((
        year,
        family,
        format!("{}{}{}", b[0] as char, b[1] as char, &digits[..2]),
    ))
}

/// Canonical, deterministic text of every fact in this module, for build
/// provenance. Any change to a fact changes this text.
pub fn canonical_ir() -> String {
    let mut out = format!("{CONTRACT} recipe {RECIPE_VERSION}\n");
    for f in &PLATFORMS {
        out.push_str(&format!(
            "platform {} marketing={:?} year={} reported={}.{} chromium={} chrome={} back_exit_popup={} gamepad_passthrough={}\n",
            f.platform.id(),
            f.marketing,
            f.first_model_year,
            f.reported_major,
            f.reported_minor_min,
            f.engine.chromium_major,
            f.engine.chrome_version,
            f.back_exit_popup,
            f.gamepad_passthrough
        ));
    }
    for feature in WebFeature::ALL {
        out.push_str(&format!(
            "feature {} chromium>={}\n",
            feature.name(),
            feature.min_chromium()
        ));
    }
    for s in crate::webos_tv_models::CATALOG {
        out.push_str(&format!(
            "series {} {:?} code={} year={} family={} shipped={} ceiling={}\n",
            s.contract,
            s.name,
            s.code,
            s.model_year,
            s.family.name(),
            s.shipped.id(),
            s.upgrade_ceiling.id()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webos_tv_models::CATALOG;

    #[test]
    fn platform_table_is_ordered_and_indexed() {
        for (i, f) in PLATFORMS.iter().enumerate() {
            assert_eq!(f.platform as usize, i);
            assert_eq!(WebosPlatform::ALL[i], f.platform);
        }
        for pair in PLATFORMS.windows(2) {
            assert!(pair[0].engine.chromium_major <= pair[1].engine.chromium_major);
            assert!(pair[0].first_model_year < pair[1].first_model_year);
        }
    }

    #[test]
    fn engines_match_lg_published_table() {
        let expected = [53, 53, 68, 79, 87, 94, 108, 120, 132];
        let got: Vec<u16> = PLATFORMS.iter().map(|f| f.engine.chromium_major).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn reported_versions_map_to_marketing_names() {
        let cases = [
            ("4.4.0", WebosPlatform::Tv4_0),
            ("4.5", WebosPlatform::Tv4_5),
            ("4.10.2", WebosPlatform::Tv4_5),
            ("5.6.2", WebosPlatform::Tv5),
            ("6.5.3", WebosPlatform::Tv6),
            ("7.4.0", WebosPlatform::Tv22),
            ("8", WebosPlatform::Tv23),
            ("9.24.8", WebosPlatform::Tv24),
            ("10.3.1", WebosPlatform::Tv25),
            ("11.2.0", WebosPlatform::Tv26),
        ];
        for (version, platform) in cases {
            assert_eq!(
                platform_by_reported_version(version),
                Ok(platform),
                "{version}"
            );
        }
        assert!(matches!(
            platform_by_reported_version("3.9.3"),
            Err(LookupError::BelowFloor {
                model_year: 2017,
                ..
            })
        ));
        assert!(matches!(
            platform_by_reported_version("12.0"),
            Err(LookupError::Unrecognised(_))
        ));
        assert!(platform_by_reported_version("9.x").is_err());
        assert!(platform_by_reported_version("").is_err());
    }

    #[test]
    fn names_parse() {
        assert_eq!(platform_by_name("webOS TV 24"), Ok(WebosPlatform::Tv24));
        assert_eq!(platform_by_name("4.5"), Ok(WebosPlatform::Tv4_5));
        assert_eq!(platform_by_name("6.0"), Ok(WebosPlatform::Tv6));
        assert!(matches!(
            platform_by_name("3.5"),
            Err(LookupError::BelowFloor { .. })
        ));
        assert!(platform_by_name("27").is_err());
        for p in WebosPlatform::ALL {
            assert_eq!(platform_by_name(p.id()), Ok(p));
        }
    }

    #[test]
    fn user_agents_resolve_to_engines() {
        let ua = |chrome: &str| {
            format!("Mozilla/5.0 (Web0S; Linux/SmartTV) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{chrome} Safari/537.36 WebAppManager")
        };
        assert_eq!(
            platforms_by_user_agent(&ua("53.0.2785.34")),
            Ok(vec![WebosPlatform::Tv4_0, WebosPlatform::Tv4_5])
        );
        for f in &PLATFORMS[2..] {
            assert_eq!(
                platforms_by_user_agent(&ua(f.engine.chrome_version)),
                Ok(vec![f.platform])
            );
        }
        assert!(matches!(
            platforms_by_user_agent(&ua("38.0.2125.122")),
            Err(LookupError::BelowFloor { .. })
        ));
        assert!(platforms_by_user_agent("Mozilla/5.0 (Web0S; Linux/SmartTV) AppleWebKit/538.2 (KHTML, like Gecko) Large Screen WebAppManager Safari/538.2").is_err());
        assert!(platforms_by_user_agent(&ua("99.0")).is_err());
        assert!(platforms_by_user_agent("Mozilla/5.0 (X11; Linux) Chrome/120.0").is_err());
    }

    #[test]
    fn feature_gates_follow_engine() {
        assert!(!WebosPlatform::Tv4_0.supports(WebFeature::AsyncAwait));
        assert!(!WebosPlatform::Tv4_0.supports(WebFeature::CssGrid));
        assert!(WebosPlatform::Tv4_0.supports(WebFeature::KeyboardEventKey));
        assert!(WebosPlatform::Tv5.supports(WebFeature::EsModules));
        assert!(!WebosPlatform::Tv6.supports(WebFeature::OptionalChaining));
        assert!(WebosPlatform::Tv22.supports(WebFeature::OptionalChaining));
        assert!(!WebosPlatform::Tv22.supports(WebFeature::AspectRatio));
        assert!(WebosPlatform::Tv23.supports(WebFeature::AspectRatio));
        for feature in WebFeature::ALL {
            assert!(WebosPlatform::Tv26.supports(feature));
        }
    }

    #[test]
    fn behaviour_flags_follow_platform() {
        assert!(!WebosPlatform::Tv5.facts().back_exit_popup);
        assert!(WebosPlatform::Tv6.facts().back_exit_popup);
        assert!(!WebosPlatform::Tv22.facts().gamepad_passthrough);
        assert!(WebosPlatform::Tv23.facts().gamepad_passthrough);
    }

    /// Every model number that was found on a real TV (Glasshouse list and
    /// tab-tv) decodes to the right year.
    #[test]
    fn observed_model_numbers_decode() {
        let cases: &[(&str, u16, Family)] = &[
            ("OLED65C8PUA", 2018, Family::Oled),
            ("OLED65B8SLC", 2018, Family::Oled),
            ("43LM5760PTC", 2019, Family::Fhd),
            ("OLED55C9PLA", 2019, Family::Oled),
            ("OLED65C9AUA", 2019, Family::Oled),
            ("50UM7300AUE", 2019, Family::Uhd),
            ("65SM9000PUA", 2019, Family::Nanocell),
            ("55SK8000PUA", 2018, Family::Nanocell),
            ("55UK6300PUE", 2018, Family::Uhd),
            ("OLED77CX6LA", 2020, Family::Oled),
            ("OLED48CXPUB", 2020, Family::Oled),
            ("OLED55GX6LA", 2020, Family::Oled),
            ("55NANO85UNA", 2020, Family::Nanocell),
            ("OLED55C17LB", 2021, Family::Oled),
            ("OLED55C1PUB", 2021, Family::Oled),
            ("50UP81006LR", 2021, Family::Uhd),
            ("43UP80006LA", 2021, Family::Uhd),
            ("55NANO75UPA", 2021, Family::Nanocell),
            ("27ART10AKPL", 2021, Family::Lifestyle),
            ("OLED65G29LA", 2022, Family::Oled),
            ("55QNED826QB", 2022, Family::Qned),
            ("OLED42C24LA", 2022, Family::Oled),
            ("OLED65CSPSA", 2022, Family::Oled),
            ("42LX3Q6LA", 2022, Family::Lifestyle),
            ("55UQ7570PUJ", 2022, Family::Uhd),
            ("65QNED80URA", 2023, Family::Qned),
            ("OLED55C31LA", 2023, Family::Oled),
            ("OLED65C3PUA", 2023, Family::Oled),
            ("OLED65G36LA", 2023, Family::Oled),
            ("OLED42C35LA", 2023, Family::Oled),
            ("OLED55C4PUA", 2024, Family::Oled),
            ("OLED55B46LA", 2024, Family::Oled),
            ("OLED55G42LW", 2024, Family::Oled),
            ("50UT81006LA", 2024, Family::Uhd),
            ("43UT73006LA", 2024, Family::Uhd),
            ("OLED65C41LA", 2024, Family::Oled),
            ("65QNED85TUA", 2024, Family::Qned),
            ("OLED48C55LA", 2025, Family::Oled),
            ("OLED65G5WUA", 2025, Family::Oled),
            ("OLED83G5WUA", 2025, Family::Oled),
            ("55UA73006LA", 2025, Family::Uhd),
            ("65QNED85AUA", 2025, Family::Qned),
            ("43NU800BPSC", 2026, Family::Uhd),
        ];
        for (model, year, family) in cases {
            let m = decode_model(model).unwrap_or_else(|e| panic!("{model}: {e}"));
            assert_eq!((m.model_year, m.family), (*year, *family), "{model}");
            assert_eq!(
                m.shipped,
                platform_for_model_year(*year).unwrap(),
                "{model}"
            );
            assert!(m.series.is_some(), "{model} should be catalogued");
        }
    }

    #[test]
    fn decoder_fails_closed() {
        for model in ["OLED65B7V-Z", "55UH600V-ZA", "49LF632V-ZC", "58UH635V"] {
            assert!(
                matches!(decode_model(model), Err(LookupError::BelowFloor { .. })),
                "{model}"
            );
        }
        assert!(matches!(
            decode_model("OLED65C6PUA"),
            Err(LookupError::Ambiguous {
                years: [2016, 2026],
                ..
            })
        ));
        for junk in [
            "",
            "OLED",
            "OLED65Q1",
            "XYZ",
            "55",
            "55UZ7000",
            "65QNED85",
            "LG C3",
            "OLED65C3PUA?",
        ] {
            assert!(decode_model(junk).is_err(), "{junk:?}");
        }
    }

    #[test]
    fn decoding_is_case_and_suffix_insensitive() {
        let a = decode_model("oled65c3pua").unwrap();
        let b = decode_model(" OLED65C3PUA-AUS ").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.series.unwrap().contract, "goya.platform.webos.oled_c3.v1");
    }

    #[test]
    fn uncatalogued_series_still_resolve_year() {
        let m = decode_model("55UQ9000PUD").unwrap();
        assert_eq!(m.model_year, 2022);
        assert_eq!(m.shipped, WebosPlatform::Tv22);
        let m = decode_model("65UR6400").unwrap();
        assert_eq!((m.model_year, m.series), (2023, None));
    }

    #[test]
    fn catalog_is_consistent() {
        let mut contracts = std::collections::BTreeSet::new();
        for s in CATALOG {
            assert!(contracts.insert(s.contract), "duplicate {}", s.contract);
            assert_eq!(
                platform_for_model_year(s.model_year),
                Ok(s.shipped),
                "{}",
                s.name
            );
            assert!(s.upgrade_ceiling >= s.shipped, "{}", s.name);
            assert!(s.contract.starts_with("goya.platform.webos.") && s.contract.ends_with(".v1"));
        }
        // Every supported model year has at least one catalogued series.
        for year in 2018..=2026 {
            assert!(CATALOG.iter().any(|s| s.model_year == year), "{year}");
        }
    }

    #[test]
    fn upgrade_ceilings_follow_renew_schedule() {
        assert_eq!(
            decode_model("OLED65C3PUA").unwrap().upgrade_ceiling(),
            WebosPlatform::Tv26
        );
        assert_eq!(
            decode_model("OLED42C24LA").unwrap().upgrade_ceiling(),
            WebosPlatform::Tv25
        );
        assert_eq!(
            decode_model("55UQ7570PUJ").unwrap().upgrade_ceiling(),
            WebosPlatform::Tv22
        );
        assert_eq!(
            decode_model("OLED65C8PUA").unwrap().upgrade_ceiling(),
            WebosPlatform::Tv4_0
        );
    }

    #[test]
    fn canonical_ir_is_deterministic() {
        let a = canonical_ir();
        assert_eq!(a, canonical_ir());
        assert!(a.starts_with("goya.platform.webos.engine.v1 recipe 0.0.1\n"));
        assert!(
            a.contains("platform 24 marketing=\"webOS TV 24\" year=2024 reported=9.0 chromium=108")
        );
        assert_eq!(
            a.lines().filter(|l| l.starts_with("series ")).count(),
            CATALOG.len()
        );
    }
}
