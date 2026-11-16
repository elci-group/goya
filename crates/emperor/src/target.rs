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

pub fn blocked_reason(triple: &str) -> String {
    if is_p0(triple) {
        format!(
            "{triple} is not this host. Cross-compilation is not a launch and is not attempted."
        )
    } else {
        format!(
            "{triple} is named by GY-EMP-001 and is not linked in compiler 0.0.1. No ELF is produced."
        )
    }
}
