//! Linux triples Emperor will name. Only the host glibc triple can be linked.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Admitted P0 triple that matches this compiler host.
    Host,
    /// Admitted P0 triple that would require a cross link.
    Cross,
    /// Named by the directive and not linked in this cycle.
    Blocked,
    /// Not a directive triple.
    Unknown,
}

pub fn host() -> Option<&'static str> {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        Some("x86_64-unknown-linux-gnu")
    }
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    {
        Some("aarch64-unknown-linux-gnu")
    }
    #[cfg(not(any(
        all(target_arch = "x86_64", target_os = "linux"),
        all(target_arch = "aarch64", target_os = "linux")
    )))]
    {
        None
    }
}

pub fn classify(triple: &str) -> Class {
    if is_p0(triple) {
        if host() == Some(triple) {
            Class::Host
        } else {
            Class::Cross
        }
    } else if is_named_blocked(triple) {
        Class::Blocked
    } else {
        Class::Unknown
    }
}

pub fn is_p0(triple: &str) -> bool {
    matches!(
        triple,
        "x86_64-unknown-linux-gnu" | "aarch64-unknown-linux-gnu"
    )
}

pub fn is_named_blocked(triple: &str) -> bool {
    matches!(
        triple,
        "i686-unknown-linux-gnu"
            | "x86_64-unknown-linux-musl"
            | "aarch64-unknown-linux-musl"
            | "riscv64gc-unknown-linux-gnu"
    )
}

pub fn elf_machine(triple: &str) -> Option<u16> {
    match triple {
        "x86_64-unknown-linux-gnu" => Some(0x3E),
        "aarch64-unknown-linux-gnu" => Some(0xB7),
        _ => None,
    }
}

/// Profiles named by the directive. Only `balanced` links.
pub fn profile_class(profile: &str) -> Class {
    match profile {
        "balanced" => Class::Host,
        "development" | "performance" | "small" | "low-power" => Class::Blocked,
        _ => Class::Unknown,
    }
}

/// Human-readable reason a triple is not linked this cycle.
///
/// The reason distinguishes the three non-`Host` classes so the fault text
/// never claims a triple is "named by GY-EMP-001" when it is not. There is
/// no third-party triple parser; matching stays exact-string on the same
/// literals as [`is_p0`] and [`is_named_blocked`], so a misspelled triple
/// is reported as unknown, never as blocked.
pub fn blocked_reason(triple: &str) -> String {
    if is_p0(triple) {
        format!(
            "{triple} is not this host (host is {}). Cross-compilation is not a launch and is not attempted.",
            host().unwrap_or("unknown")
        )
    } else if is_named_blocked(triple) {
        format!(
            "{triple} is named by GY-EMP-001 and is not linked in compiler 0.0.1. No ELF is produced."
        )
    } else {
        format!(
            "{triple} is neither an admitted P0 triple nor named by the directive. No ELF is produced."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_matches_host_only_on_this_triple() {
        let host = host();
        if let Some(host) = host {
            assert_eq!(classify(host), Class::Host);
        }
        let other = if host == Some("x86_64-unknown-linux-gnu") || host.is_none() {
            "aarch64-unknown-linux-gnu"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        assert_eq!(classify(other), Class::Cross);
    }

    #[test]
    fn classify_named_blocked_and_unknown() {
        assert_eq!(classify("x86_64-unknown-linux-musl"), Class::Blocked);
        assert_eq!(classify("i686-unknown-linux-gnu"), Class::Blocked);
        assert_eq!(classify("x86_64-pc-windows-msvc"), Class::Unknown);
        assert_eq!(classify("x86_64-unknown-linux_gnu"), Class::Unknown);
    }

    #[test]
    fn blocked_reason_names_the_host_for_cross() {
        let reason = blocked_reason("aarch64-unknown-linux-gnu");
        assert!(reason.contains("host is"));
        assert!(reason.contains("Cross-compilation"));
    }

    #[test]
    fn blocked_reason_cites_directive_only_for_named_triples() {
        assert!(blocked_reason("x86_64-unknown-linux-musl").contains("GY-EMP-001"));
        // An unknown triple must not be described as named by the directive.
        let reason = blocked_reason("x86_64-pc-windows-msvc");
        assert!(!reason.contains("GY-EMP-001"));
        assert!(reason.contains("nor named by the directive"));
    }

    #[test]
    fn elf_machine_only_for_p0() {
        assert_eq!(elf_machine("x86_64-unknown-linux-gnu"), Some(0x3E));
        assert_eq!(elf_machine("aarch64-unknown-linux-gnu"), Some(0xB7));
        assert_eq!(elf_machine("x86_64-unknown-linux-musl"), None);
    }
}
