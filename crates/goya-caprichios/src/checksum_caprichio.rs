//! Narrow construction of a non-cryptographic FNV-1a 64-bit checksum binding.
//! This does not claim a general checksum catalog capability or provide
//! collision resistance, cryptographic security, or message authentication.

use std::fmt;

pub const CONTRACT: &str = "goya.data.checksum.fnv1a64.v1";
pub const RECIPE: &str = "rust-std-fnv1a64";
pub const RECIPE_VERSION: &str = "0.1.0";
const OFFSET_BASIS: &str = "0xcbf29ce484222325";
const FNV_PRIME: &str = "0x00000100000001b3";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Algorithm {
    Fnv1a64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub algorithm: Algorithm,
    pub require_cryptographic_security: bool,
    pub require_collision_resistance: bool,
    pub require_authentication: bool,
}

/// Caller-supplied compilation/runtime facts required by the emitted binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    pub standard_library: bool,
    pub byte_slices: bool,
    pub u64_arithmetic: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Diagnostic(pub &'static str);

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ir {
    algorithm: Algorithm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl Declaration {
    pub fn normalize(self) -> Result<Ir, Diagnostic> {
        if self.require_cryptographic_security {
            return Err(Diagnostic(
                "FNV-1a is non-cryptographic and cannot satisfy cryptographic security",
            ));
        }
        if self.require_collision_resistance {
            return Err(Diagnostic(
                "FNV-1a is not collision-resistant and cannot satisfy that requirement",
            ));
        }
        if self.require_authentication {
            return Err(Diagnostic(
                "FNV-1a does not authenticate data and cannot satisfy that requirement",
            ));
        }
        match self.algorithm {
            Algorithm::Fnv1a64 => Ok(Ir {
                algorithm: self.algorithm,
            }),
        }
    }
}

impl Ir {
    pub fn canonical(&self) -> String {
        match self.algorithm {
            Algorithm::Fnv1a64 => format!(
                "contract={CONTRACT}\nrecipe={RECIPE}@{RECIPE_VERSION}\nalgorithm=FNV-1a-64\noffset_basis={OFFSET_BASIS}\nprime={FNV_PRIME}\nword_arithmetic=wrapping-u64\ninput=bytes-in-order\noutput=u64-value\ncryptographic=false\ncollision_resistance=false\nauthentication=false\n"
            ),
        }
    }

    /// Emit a deterministic binding only when the host reports each required
    /// language/runtime facility.
    pub fn lower_checked(&self, host: HostProfile) -> Result<Artifact, Diagnostic> {
        if !host.standard_library {
            return Err(Diagnostic("target requires the Rust standard library"));
        }
        if !host.byte_slices {
            return Err(Diagnostic("target requires byte-slice support"));
        }
        if !host.u64_arithmetic {
            return Err(Diagnostic("target requires 64-bit unsigned arithmetic"));
        }
        let canonical_ir = self.canonical();
        let source = format!(
            "// {CONTRACT}; recipe={RECIPE}@{RECIPE_VERSION}\n\
             pub fn checksum(bytes: &[u8]) -> u64 {{\n\
                 goya_caprichios::data_runtime::checksum(bytes)\n\
             }}\n"
        );
        Ok(Artifact {
            contract: CONTRACT,
            recipe: RECIPE,
            recipe_version: RECIPE_VERSION,
            canonical_ir,
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Declaration {
        Declaration {
            algorithm: Algorithm::Fnv1a64,
            require_cryptographic_security: false,
            require_collision_resistance: false,
            require_authentication: false,
        }
    }

    fn host() -> HostProfile {
        HostProfile {
            standard_library: true,
            byte_slices: true,
            u64_arithmetic: true,
        }
    }

    #[test]
    fn fails_closed_on_crypto_authentication_and_collision_requirements() {
        let mut request = declaration();
        request.require_cryptographic_security = true;
        assert_eq!(
            request.normalize(),
            Err(Diagnostic(
                "FNV-1a is non-cryptographic and cannot satisfy cryptographic security"
            ))
        );
        request = declaration();
        request.require_collision_resistance = true;
        assert_eq!(
            request.normalize(),
            Err(Diagnostic(
                "FNV-1a is not collision-resistant and cannot satisfy that requirement"
            ))
        );
        request = declaration();
        request.require_authentication = true;
        assert_eq!(
            request.normalize(),
            Err(Diagnostic(
                "FNV-1a does not authenticate data and cannot satisfy that requirement"
            ))
        );
    }

    #[test]
    fn canonical_ir_and_binding_are_reproducible_and_name_exact_algorithm() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.canonical(), ir.canonical());
        assert!(ir.canonical().contains("offset_basis=0xcbf29ce484222325\n"));
        assert!(ir.canonical().contains("prime=0x00000100000001b3\n"));
        let a = ir.lower_checked(host()).unwrap();
        let b = ir.lower_checked(host()).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.contract, CONTRACT);
        assert_eq!(a.recipe, RECIPE);
        assert_eq!(a.recipe_version, RECIPE_VERSION);
    }

    #[test]
    fn fails_closed_when_a_host_facility_is_missing() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(
            ir.lower_checked(HostProfile {
                standard_library: false,
                ..host()
            }),
            Err(Diagnostic("target requires the Rust standard library"))
        );
        assert_eq!(
            ir.lower_checked(HostProfile {
                byte_slices: false,
                ..host()
            }),
            Err(Diagnostic("target requires byte-slice support"))
        );
        assert_eq!(
            ir.lower_checked(HostProfile {
                u64_arithmetic: false,
                ..host()
            }),
            Err(Diagnostic("target requires 64-bit unsigned arithmetic"))
        );
    }

    #[test]
    fn emitted_binding_compiles_and_runs_against_the_real_runtime() {
        use std::fs;
        use std::process::Command;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "goya-checksum-binding-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&dir).unwrap();
        let result = (|| {
            let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let library = dir.join("libgoya_caprichios.rlib");
            let build_library = Command::new("rustc")
                .arg("--edition=2021")
                .arg("--crate-name=goya_caprichios")
                .arg("--crate-type=rlib")
                .arg(manifest.join("src/lib.rs"))
                .arg("-o")
                .arg(&library)
                .env("CARGO_MANIFEST_DIR", manifest)
                .output()
                .unwrap();
            assert!(
                build_library.status.success(),
                "building runtime rlib failed:\n{}",
                String::from_utf8_lossy(&build_library.stderr)
            );

            let artifact = declaration()
                .normalize()
                .unwrap()
                .lower_checked(host())
                .unwrap();
            let source_path = dir.join("binding.rs");
            let executable_path = dir.join("binding");
            fs::write(
                &source_path,
                format!(
                    "{}\nfn main() {{\n\
                     assert_eq!(checksum(b\"\"), 0xcbf29ce484222325);\n\
                     assert_eq!(checksum(b\"hello\"), 0xa430d84680aabd0b);\n\
                     }}\n",
                    artifact.source
                ),
            )
            .unwrap();
            let compile = Command::new("rustc")
                .arg("--edition=2021")
                .arg("--extern")
                .arg(format!("goya_caprichios={}", library.display()))
                .arg(&source_path)
                .arg("-o")
                .arg(&executable_path)
                .output()
                .unwrap();
            assert!(
                compile.status.success(),
                "compiling emitted binding failed:\n{}",
                String::from_utf8_lossy(&compile.stderr)
            );
            let run = Command::new(&executable_path).output().unwrap();
            assert!(
                run.status.success(),
                "emitted binding execution failed:\n{}",
                String::from_utf8_lossy(&run.stderr)
            );
        })();
        fs::remove_dir_all(dir).unwrap();
        result
    }

    /// Run explicitly with `cargo test -p goya-caprichios checksum_caprichio::tests::benchmark_construction -- --ignored --nocapture`.
    #[test]
    #[ignore = "manual checksum-recipe construction microbenchmark"]
    fn benchmark_construction() {
        use std::time::Instant;
        let ir = declaration().normalize().unwrap();
        let start = Instant::now();
        let iterations = 100_000;
        for _ in 0..iterations {
            std::hint::black_box(ir.lower_checked(host()).unwrap());
        }
        eprintln!(
            "{iterations} checksum recipe constructions in {:?}",
            start.elapsed()
        );
    }
}
