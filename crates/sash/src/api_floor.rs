//! Admitted Windows API floors.
//!
//! A floor selects which Win32 adapter the emitted program carries. The
//! floor is a build-time choice: it names the oldest Windows version the
//! image is expected to load on, and the verifier checks the image against
//! that declaration. A floor is not a support claim — nothing has been
//! launched on any Windows version, and import inspection is not a
//! clean-machine test.
//!
//! `win10` (default) is the per-monitor-v2 adapter the first proof linked.
//! `win8` is the system-DPI-aware adapter: it drops every Windows 10 1703+
//! per-monitor API from the import table and reads the process DPI once from
//! `GetDeviceCaps`. `win8` is the lowest floor the pinned stable toolchain
//! can produce, because the `pc-windows-msvc` standard library imports
//! `api-ms-win-core-synch-l1-2-0.dll` (`WaitOnAddress`, Windows 8+). Windows
//! Vista and 7 would need the tier-3 `*-win7-windows-msvc` targets built with
//! a nightly `-Z build-std`; Sash does not download a compiler, so those
//! floors are refused rather than approximated. ARM64 Windows hardware only
//! exists on Windows 10 or later, so `win8` and ARM64 are never combined.

/// One admitted API floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiFloor {
    /// Windows 10 1703+: per-monitor-v2 DPI, the adapter of the first proof.
    Win10,
    /// Windows 8.0+: system-DPI-aware adapter, no per-monitor imports.
    Win8,
}

/// CLI spelling of [`ApiFloor::Win10`].
pub const FLOOR_WIN10: &str = "win10";
/// CLI spelling of [`ApiFloor::Win8`].
pub const FLOOR_WIN8: &str = "win8";

pub const ADMITTED_FLOOR_SPELLINGS: &[&str] = &[FLOOR_WIN10, FLOOR_WIN8];

impl ApiFloor {
    /// The spelling used on the command line and in every report.
    pub fn label(self) -> &'static str {
        match self {
            ApiFloor::Win10 => FLOOR_WIN10,
            ApiFloor::Win8 => FLOOR_WIN8,
        }
    }

    /// True when this floor needs the system-DPI-aware adapter.
    pub fn is_legacy(self) -> bool {
        self == ApiFloor::Win8
    }
}

/// Admit a `--api-floor` spelling, or explain the refusal.
///
/// The error names the two admitted floors and, for `win7`, explains why the
/// pinned stable toolchain cannot reach that floor.
pub fn admit_floor(text: &str) -> Result<ApiFloor, &'static str> {
    match text {
        FLOOR_WIN10 => Ok(ApiFloor::Win10),
        FLOOR_WIN8 => Ok(ApiFloor::Win8),
        "win7" | "vista" | "xp" => Err(
            "Windows 7, Vista, and XP need the tier-3 *-win7-windows-msvc standard library, \
             which only a nightly toolchain with -Z build-std can produce. The pinned stable \
             toolchain cannot link that floor, and Sash does not download a compiler. \
             The lowest admitted floor is win8.",
        ),
        _ => Err("unknown api floor; admitted floors are win10 (default) and win8"),
    }
}

/// The lowest Windows version that can load an image linked against the
/// stable `pc-windows-msvc` standard library. The standard library imports
/// `api-ms-win-core-synch-l1-2-0.dll` (`WaitOnAddress`, Windows 8+), so the
/// `win8` floor is the lowest one this toolchain can honestly declare.
pub const STD_LIBRARY_FLOOR_NOTE: &str =
    "the pc-windows-msvc standard library imports api-ms-win-core-synch-l1-2-0.dll (Windows 8+)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_round_trip() {
        assert_eq!(admit_floor("win10"), Ok(ApiFloor::Win10));
        assert_eq!(admit_floor("win8"), Ok(ApiFloor::Win8));
        assert_eq!(admit_floor("win10").unwrap().label(), "win10");
        assert_eq!(admit_floor("win8").unwrap().label(), "win8");
    }

    #[test]
    fn legacy_floor_is_win8_only() {
        assert!(!ApiFloor::Win10.is_legacy());
        assert!(ApiFloor::Win8.is_legacy());
    }

    #[test]
    fn win7_vista_and_xp_are_refused_with_the_toolchain_reason() {
        for spelling in ["win7", "vista", "xp"] {
            let error = admit_floor(spelling).unwrap_err();
            assert!(error.contains("tier-3"), "{spelling}: {error}");
            assert!(error.contains("does not download a compiler"));
        }
    }

    #[test]
    fn unknown_floor_is_refused() {
        assert!(admit_floor("").is_err());
        assert!(admit_floor("win95").is_err());
        assert!(admit_floor("Win10").is_err());
        let error = admit_floor("win95").unwrap_err();
        assert!(error.contains("win10"));
        assert!(error.contains("win8"));
    }
}
