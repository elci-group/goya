//! Run modes for Diablo Free runtime.

use serde::{Deserialize, Serialize};

/// Execution mode for the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunMode {
    /// Headless oracle verification (default)
    Oracle,
    /// Quick smoke test with window
    Smoke,
    /// Interactive exercise
    Exercise,
    /// Full live application
    Live,
}

impl Default for RunMode {
    fn default() -> Self {
        RunMode::Oracle
    }
}

impl std::str::FromStr for RunMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "oracle" => Ok(RunMode::Oracle),
            "smoke" => Ok(RunMode::Smoke),
            "exercise" => Ok(RunMode::Exercise),
            "live" => Ok(RunMode::Live),
            _ => Err(format!("Unknown run mode: {}", s)),
        }
    }
}

impl std::fmt::Display for RunMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunMode::Oracle => write!(f, "oracle"),
            RunMode::Smoke => write!(f, "smoke"),
            RunMode::Exercise => write!(f, "exercise"),
            RunMode::Live => write!(f, "live"),
        }
    }
}