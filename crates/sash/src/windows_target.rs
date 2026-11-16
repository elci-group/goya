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
    /// `assemblyIdentity` `processorArchitecture` for this machine.
    pub processor_architecture: &'static str,
    /// Import-library directory under the pinned `windows-msvc-sysroot`.
    pub sysroot_lib: &'static str,
}

/// PE32, 32-bit x86. `user32` exports `SetWindowLongW`, not `SetWindowLongPtrW`.
pub const X86: WindowsTarget = WindowsTarget {
    triple: "i686-pc-windows-msvc",
    machine: 0x014C,
    optional_magic: 0x010B,
    pointer_bits: 32,
    processor_architecture: "x86",
    sysroot_lib: "i686-unknown-windows-msvc",
};

/// PE32+, x64. This is the original linked target.
pub const X64: WindowsTarget = WindowsTarget {
    triple: "x86_64-pc-windows-msvc",
    machine: 0x8664,
    optional_magic: 0x020B,
    pointer_bits: 64,
    processor_architecture: "amd64",
    sysroot_lib: "x86_64-unknown-windows-msvc",
};

/// PE32+, native ARM64. This is not an x64 binary under emulation.
pub const ARM64: WindowsTarget = WindowsTarget {
    triple: "aarch64-pc-windows-msvc",
    machine: 0xAA64,
    optional_magic: 0x020B,
    pointer_bits: 64,
    processor_architecture: "arm64",
    sysroot_lib: "aarch64-unknown-windows-msvc",
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

/// Replace the template `processorArchitecture` with the selected machine.
///
/// The generators write `amd64` first. i686 and ARM64 links must not keep
/// that value. This is not a Windows launch.
pub fn stamp_processor_architecture(
    xml: &str,
    target: WindowsTarget,
) -> Result<String, &'static str> {
    const MARK: &str = "processorArchitecture=\"amd64\"";
    if xml.matches(MARK).count() != 1 {
        return Err("the generated manifest does not name processorArchitecture once");
    }
    Ok(xml.replacen(
        MARK,
        &format!(
            "processorArchitecture=\"{}\"",
            target.processor_architecture
        ),
        1,
    ))
}

/// `.cargo/config.toml` for one triple.
///
/// `/Brepro` asks lld to fill the COFF timestamp and the CodeView GUID from
/// the image and the PDB. The link keeps the build directory and the synthetic
/// `symbols.o` path stable so that hash does not change between two links of
/// the same crate. This is not a Windows launch.
pub fn cargo_config(target: WindowsTarget) -> String {
    format!(
        "[build]\nincremental = false\n\n[target.{}]\nrustflags = [\n  \"-C\", \"target-feature=+crt-static\",\n  \"-C\", \"control-flow-guard=checks\",\n  \"-C\", \"link-arg=/Brepro\",\n]\n",
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
            assert!(config.contains("link-arg=/Brepro"));
            assert!(target.sysroot_lib.ends_with("-unknown-windows-msvc"));
            assert!(!target.sysroot_lib.contains("linux"));
        }
        assert_ne!(X86.sysroot_lib, X64.sysroot_lib);
        assert_ne!(X64.sysroot_lib, ARM64.sysroot_lib);
        assert_ne!(X86.sysroot_lib, ARM64.sysroot_lib);
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
        assert_eq!(X86.processor_architecture, "x86");
        assert_eq!(X64.processor_architecture, "amd64");
        assert_eq!(ARM64.processor_architecture, "arm64");
    }

    #[test]
    fn stamp_rewrites_the_one_template_architecture() {
        let xml = r#"<assemblyIdentity processorArchitecture="amd64"/>"#;
        assert_eq!(
            stamp_processor_architecture(xml, X64).unwrap(),
            r#"<assemblyIdentity processorArchitecture="amd64"/>"#
        );
        assert_eq!(
            stamp_processor_architecture(xml, X86).unwrap(),
            r#"<assemblyIdentity processorArchitecture="x86"/>"#
        );
        assert_eq!(
            stamp_processor_architecture(xml, ARM64).unwrap(),
            r#"<assemblyIdentity processorArchitecture="arm64"/>"#
        );
        assert!(stamp_processor_architecture("<assembly/>", X64).is_err());
        let twice = r#"processorArchitecture="amd64" processorArchitecture="amd64""#;
        assert!(stamp_processor_architecture(twice, X86).is_err());
    }
}
