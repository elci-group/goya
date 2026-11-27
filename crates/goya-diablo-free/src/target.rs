//! FreeBSD target classification (mirroring Emperor's target.rs).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetClass {
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
    #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
    {
        Some("x86_64-unknown-freebsd")
    }
    #[cfg(all(target_arch = "aarch64", target_os = "freebsd"))]
    {
        Some("aarch64-unknown-freebsd")
    }
    #[cfg(not(any(
        all(target_arch = "x86_64", target_os = "freebsd"),
        all(target_arch = "aarch64", target_os = "freebsd")
    )))]
    {
        None
    }
}

pub fn classify_target(triple: &str) -> TargetClass {
    if is_p0(triple) {
        if host() == Some(triple) {
            TargetClass::Host
        } else {
            TargetClass::Cross
        }
    } else if is_named_blocked(triple) {
        TargetClass::Blocked
    } else {
        TargetClass::Unknown
    }
}

pub fn is_p0(triple: &str) -> bool {
    matches!(
        triple,
        "x86_64-unknown-freebsd" | "aarch64-unknown-freebsd"
    )
}

pub fn is_named_blocked(triple: &str) -> bool {
    matches!(
        triple,
        "i386-unknown-freebsd"
            | "armv7-unknown-freebsd"
            | "riscv64-unknown-freebsd"
    )
}

pub fn elf_machine(triple: &str) -> Option<u16> {
    match triple {
        "x86_64-unknown-freebsd" => Some(0x3E),   // EM_X86_64
        "aarch64-unknown-freebsd" => Some(0xB7),  // EM_AARCH64
        "i386-unknown-freebsd" => Some(0x03),     // EM_386
        "armv7-unknown-freebsd" => Some(0x28),    // EM_ARM
        "riscv64-unknown-freebsd" => Some(0xF3),  // EM_RISCV
        _ => None,
    }
}

/// Profiles named by the directive. Only `balanced` links.
pub fn profile_class(profile: &str) -> TargetClass {
    match profile {
        "balanced" => TargetClass::Host,
        "development" | "performance" | "small" | "low-power" | "constrained" => TargetClass::Blocked,
        _ => TargetClass::Unknown,
    }
}

/// Human-readable reason a triple is not linked this cycle.
pub fn blocked_reason(triple: &str) -> String {
    if is_p0(triple) {
        format!(
            "{triple} is not this host (host is {}). Cross-compilation is not a launch and is not attempted.",
            host().unwrap_or("unknown")
        )
    } else if is_named_blocked(triple) {
        format!(
            "{triple} is named by GOYA-DIA-TD-001 and is not linked in compiler 0.0.1. No ELF is produced."
        )
    } else {
        format!(
            "{triple} is neither an admitted P0 triple nor named by the directive. No ELF is produced."
        )
    }
}

/// Target triples for FreeBSD.
pub const FREEBSD_TRIPLES: &[&str] = &[
    "x86_64-unknown-freebsd",
    "aarch64-unknown-freebsd",
    "i386-unknown-freebsd",
    "armv7-unknown-freebsd",
    "riscv64-unknown-freebsd",
];

/// Check if a triple is a FreeBSD target.
pub fn is_freebsd_target(triple: &str) -> bool {
    FREEBSD_TRIPLES.contains(&triple)
}

/// Get the FreeBSD target class for a triple.
pub fn target_class(triple: &str) -> TargetClass {
    classify_target(triple)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_matches_host_only_on_this_triple() {
        let host = host();
        if let Some(host) = host {
            assert_eq!(classify_target(host), TargetClass::Host);
        }
        let other = if host == Some("x86_64-unknown-freebsd") || host.is_none() {
            "aarch64-unknown-freebsd"
        } else {
            "x86_64-unknown-freebsd"
        };
        assert_eq!(classify_target(other), TargetClass::Cross);
    }

    #[test]
    fn classify_named_blocked_and_unknown() {
        assert_eq!(classify_target("i386-unknown-freebsd"), TargetClass::Blocked);
        assert_eq!(classify_target("armv7-unknown-freebsd"), TargetClass::Blocked);
        assert_eq!(classify_target("riscv64-unknown-freebsd"), TargetClass::Blocked);
        assert_eq!(classify_target("x86_64-pc-windows-msvc"), TargetClass::Unknown);
        assert_eq!(classify_target("x86_64-unknown-linux-gnu"), TargetClass::Unknown);
    }

    #[test]
    fn blocked_reason_names_the_host_for_cross() {
        let reason = blocked_reason("aarch64-unknown-freebsd");
        assert!(reason.contains("host is"));
        assert!(reason.contains("Cross-compilation"));
    }

    #[test]
    fn blocked_reason_cites_directive_only_for_named_triples() {
        assert!(blocked_reason("i386-unknown-freebsd").contains("GOYA-DIA-TD-001"));
        let reason = blocked_reason("x86_64-pc-windows-msvc");
        assert!(!reason.contains("GOYA-DIA-TD-001"));
        assert!(reason.contains("nor named by the directive"));
    }

    #[test]
    fn elf_machine_only_for_p0() {
        assert_eq!(elf_machine("x86_64-unknown-freebsd"), Some(0x3E));
        assert_eq!(elf_machine("aarch64-unknown-freebsd"), Some(0xB7));
        assert_eq!(elf_machine("i386-unknown-freebsd"), Some(0x03));
    }
}