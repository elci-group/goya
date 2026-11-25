//! Scene platforms the resolver can select.
//!
//! The identifiers are the `@platform` tokens that have an in-tree compiler.
//! macOS is a persistence adapter only. `webos` and `tvos` are scene tokens
//! without a compiler. Quitasol is a proposal, not a host.
//!
//! Selecting a target does not open its surface and does not link the compiler.

/// One platform Goya compiles a scene for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Android,
    Television,
    Browser,
    Ios,
    Linux,
    Windows,
}

impl Target {
    pub const ALL: [Self; 6] = [
        Self::Android,
        Self::Television,
        Self::Browser,
        Self::Ios,
        Self::Linux,
        Self::Windows,
    ];

    /// `android`, `android_tv`, `browser`, `ios`, `linux`, or `windows`.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "android" => Self::Android,
            "android_tv" => Self::Television,
            "browser" => Self::Browser,
            "ios" => Self::Ios,
            "linux" => Self::Linux,
            "windows" => Self::Windows,
            _ => return None,
        })
    }

    /// The `@platform` token.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Television => "android_tv",
            Self::Browser => "browser",
            Self::Ios => "ios",
            Self::Linux => "linux",
            Self::Windows => "windows",
        }
    }

    /// Compiler product for this platform. The resolver does not link it.
    pub const fn compiler(self) -> &'static str {
        match self {
            Self::Android => "aragon",
            Self::Television => "nevada",
            Self::Browser => "maja",
            Self::Ios => "neptune",
            Self::Linux => "emperor",
            Self::Windows => "sash",
        }
    }

    /// Presentation surface a host would use. This build does not open it.
    pub const fn surface(self) -> &'static str {
        match self {
            Self::Android => "android.view",
            Self::Television => "remote",
            Self::Browser => "dom",
            Self::Ios => "uikit",
            Self::Linux => "wayland",
            Self::Windows => "win32",
        }
    }
}
