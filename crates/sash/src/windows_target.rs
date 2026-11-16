//! Admitted Windows link targets.
//!
//! A triple admitted here can be cross-linked. Admission is not a Windows
//! launch, and it is not release certification.

/// One MSVC Windows target Sash can ask the host linker to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsTarget {
    pub triple: &'static str,
    pub machine: u16,
    pub optional_magic: u16,
    pub pointer_bits: u16,
}

/// PE32, 32-bit x86. `user32` exports `SetWindowLongW`, not `SetWindowLongPtrW`.
pub const X86: WindowsTarget = WindowsTarget {
    triple: "i686-pc-windows-msvc",
    machine: 0x014C,
    optional_magic: 0x010B,
    pointer_bits: 32,
};

/// PE32+, x64. This is the original linked target.
pub const X64: WindowsTarget = WindowsTarget {
    triple: "x86_64-pc-windows-msvc",
    machine: 0x8664,
    optional_magic: 0x020B,
    pointer_bits: 64,
};

/// PE32+, native ARM64. This is not an x64 binary under emulation.
pub const ARM64: WindowsTarget = WindowsTarget {
    triple: "aarch64-pc-windows-msvc",
    machine: 0xAA64,
    optional_magic: 0x020B,
    pointer_bits: 64,
};

pub const ADMITTED: &[WindowsTarget] = &[X86, X64, ARM64];

pub fn admit_triple(text: &str) -> Option<WindowsTarget> {
    ADMITTED
        .iter()
        .copied()
        .find(|target| target.triple == text)
}

pub fn by_machine(machine: u16) -> Option<WindowsTarget> {
    ADMITTED
        .iter()
        .copied()
        .find(|target| target.machine == machine)
}

/// `.cargo/config.toml` for one triple. The flags match the x64 proof.
pub fn cargo_config(target: WindowsTarget) -> String {
    format!(
        "[build]\nincremental = false\n\n[target.{}]\nrustflags = [\n  \"-C\", \"target-feature=+crt-static\",\n  \"-C\", \"control-flow-guard=checks\",\n]\n",
        target.triple
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_triples_round_trip() {
        for target in ADMITTED {
            assert_eq!(admit_triple(target.triple), Some(*target));
            assert_eq!(by_machine(target.machine), Some(*target));
            let config = cargo_config(*target);
            assert!(config.contains(target.triple));
            assert!(config.contains("target-feature=+crt-static"));
            assert!(config.contains("control-flow-guard=checks"));
        }
    }

    #[test]
    fn unknown_triple_and_machine_are_rejected() {
        assert_eq!(admit_triple("aarch64-pc-windows-msvc "), None);
        assert_eq!(admit_triple("arm64-pc-windows-msvc"), None);
        assert_eq!(admit_triple("i686-pc-windows-gnu"), None);
        assert_eq!(by_machine(0x01C4), None);
    }

    #[test]
    fn pointer_width_matches_the_pe_kind() {
        assert_eq!(X86.pointer_bits, 32);
        assert_eq!(X86.optional_magic, 0x010B);
        assert_eq!(X64.pointer_bits, 64);
        assert_eq!(X64.optional_magic, 0x020B);
        assert_eq!(ARM64.pointer_bits, 64);
        assert_eq!(ARM64.optional_magic, 0x020B);
        assert_ne!(X64.machine, ARM64.machine);
    }
}
