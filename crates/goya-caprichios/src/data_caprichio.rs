//! Typed, fail-closed construction of the bounded byte-envelope recipe.
//!
//! This narrow contract covers opaque bytes with the existing GYDB v1 wire
//! format. It makes no claim to implement the broader `data.binary_codec`
//! draft catalog definition.

use std::fmt;

pub const CONTRACT: &str = "goya.data.binary_envelope.v1";
pub const RECIPE: &str = "rust-std-byte-envelope";
pub const RECIPE_VERSION: &str = "0.1.0";
/// The envelope stores payload length as u32; targets may impose a lower cap.
pub const MAX_PAYLOAD_BYTES: u64 = u32::MAX as u64;
const WIRE_HEADER_BYTES: u64 = 17;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub max_payload_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerWidth {
    Bits32,
    Bits64,
}

/// These are caller-reported facts about the compilation/runtime target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    pub pointer_width: PointerWidth,
    pub standard_allocator: bool,
    pub byte_slices: bool,
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
    max_payload_bytes: u64,
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
        if self.max_payload_bytes == 0 {
            return Err(Diagnostic("maximum payload size must be positive"));
        }
        if self.max_payload_bytes > MAX_PAYLOAD_BYTES {
            return Err(Diagnostic(
                "maximum payload size exceeds the u32 wire limit",
            ));
        }
        Ok(Ir {
            max_payload_bytes: self.max_payload_bytes,
        })
    }
}

impl Ir {
    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe={RECIPE}@{RECIPE_VERSION}\nmax_payload_bytes={}\nwire_format=GYDB-v1\nwire_header_bytes={WIRE_HEADER_BYTES}\nchecksum=fnv1a-64-non-cryptographic\n",
            self.max_payload_bytes
        )
    }

    /// Emit a deterministic Rust binding only when the declared limit and
    /// required host facilities fit the supplied target profile.
    pub fn lower_checked(&self, host: HostProfile) -> Result<Artifact, Diagnostic> {
        if !host.standard_allocator {
            return Err(Diagnostic("target runtime requires a standard allocator"));
        }
        if !host.byte_slices {
            return Err(Diagnostic("target runtime requires byte-slice support"));
        }
        // Vec capacities are bounded by isize::MAX, even when usize can
        // represent a larger byte count. Include the complete wire header.
        let allocation_limit = match host.pointer_width {
            PointerWidth::Bits32 => i32::MAX as u64,
            PointerWidth::Bits64 => i64::MAX as u64,
        };
        let encoded_max = self
            .max_payload_bytes
            .checked_add(WIRE_HEADER_BYTES)
            .ok_or(Diagnostic(
                "encoded maximum size overflows target allocation limit",
            ))?;
        if encoded_max > allocation_limit {
            return Err(Diagnostic(
                "encoded maximum size exceeds target allocation limit",
            ));
        }

        let canonical_ir = self.canonical();
        let source = format!(
            "// {CONTRACT}; recipe={RECIPE}@{RECIPE_VERSION}\n\
             pub const MAX_PAYLOAD_BYTES: usize = {};\n\
             pub fn encode(payload: &[u8]) -> Result<Vec<u8>, goya_caprichios::data_runtime::CodecError> {{\n\
                 goya_caprichios::data_runtime::encode(payload, MAX_PAYLOAD_BYTES)\n\
             }}\n\
             pub fn decode(encoded: &[u8]) -> Result<&[u8], goya_caprichios::data_runtime::CodecError> {{\n\
                 goya_caprichios::data_runtime::decode(encoded, MAX_PAYLOAD_BYTES)\n\
             }}\n",
            self.max_payload_bytes
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

    fn host32() -> HostProfile {
        HostProfile {
            pointer_width: PointerWidth::Bits32,
            standard_allocator: true,
            byte_slices: true,
        }
    }

    fn host64() -> HostProfile {
        HostProfile {
            pointer_width: PointerWidth::Bits64,
            ..host32()
        }
    }

    #[test]
    fn rejects_invalid_limits_and_accepts_the_declared_ceiling() {
        assert_eq!(
            Declaration {
                max_payload_bytes: 0
            }
            .normalize(),
            Err(Diagnostic("maximum payload size must be positive"))
        );
        assert_eq!(
            Declaration {
                max_payload_bytes: MAX_PAYLOAD_BYTES + 1
            }
            .normalize(),
            Err(Diagnostic(
                "maximum payload size exceeds the u32 wire limit"
            ))
        );
        assert!(Declaration {
            max_payload_bytes: MAX_PAYLOAD_BYTES
        }
        .normalize()
        .is_ok());
    }

    #[test]
    fn emits_deterministic_versioned_artifacts_for_32_and_64_bit_targets() {
        let ir = Declaration {
            max_payload_bytes: 4096,
        }
        .normalize()
        .unwrap();
        for host in [host32(), host64()] {
            let artifact = ir.lower_checked(host).unwrap();
            assert_eq!(artifact.contract, CONTRACT);
            assert_eq!(artifact.recipe, RECIPE);
            assert_eq!(artifact.recipe_version, RECIPE_VERSION);
            assert!(artifact.canonical_ir.contains("wire_format=GYDB-v1\n"));
            assert!(artifact.source.contains("MAX_PAYLOAD_BYTES: usize = 4096"));
            assert_eq!(artifact, ir.lower_checked(host).unwrap());
        }
    }

    #[test]
    fn fails_closed_when_host_facilities_are_missing() {
        let ir = Declaration {
            max_payload_bytes: 512,
        }
        .normalize()
        .unwrap();
        assert_eq!(
            ir.lower_checked(HostProfile {
                standard_allocator: false,
                ..host64()
            }),
            Err(Diagnostic("target runtime requires a standard allocator"))
        );
        assert_eq!(
            ir.lower_checked(HostProfile {
                byte_slices: false,
                ..host64()
            }),
            Err(Diagnostic("target runtime requires byte-slice support"))
        );
    }

    #[test]
    fn target_width_limit_accounts_for_the_complete_wire_header() {
        let max_that_fits = i32::MAX as u64 - WIRE_HEADER_BYTES;
        let fits = Declaration {
            max_payload_bytes: max_that_fits,
        }
        .normalize()
        .unwrap();
        assert!(fits.lower_checked(host32()).is_ok());
        let exceeds = Declaration {
            max_payload_bytes: max_that_fits + 1,
        }
        .normalize()
        .unwrap();
        assert_eq!(
            exceeds.lower_checked(host32()),
            Err(Diagnostic(
                "encoded maximum size exceeds target allocation limit"
            ))
        );
        assert!(exceeds.lower_checked(host64()).is_ok());
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
        let dir =
            std::env::temp_dir().join(format!("goya-data-binding-{}-{nonce}", std::process::id()));
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

            let ir = Declaration {
                max_payload_bytes: 4,
            }
            .normalize()
            .unwrap();
            let artifact = ir.lower_checked(host64()).unwrap();
            let source_path = dir.join("binding.rs");
            let executable_path = dir.join("binding");
            let source = format!(
                "{}\nfn main() {{\n\
                 let input = [1u8, 0, 255, 4];\n\
                 let encoded = encode(&input).expect(\"encode\");\n\
                 assert_eq!(decode(&encoded).expect(\"decode\"), input);\n\
                 assert!(matches!(encode(&[0u8; 5]), Err(goya_caprichios::data_runtime::CodecError::PayloadTooLarge {{ .. }})));\n\
                 let mut corrupted = encoded;\n\
                 *corrupted.last_mut().expect(\"payload byte\") ^= 1;\n\
                 assert!(matches!(decode(&corrupted), Err(goya_caprichios::data_runtime::CodecError::ChecksumMismatch {{ .. }})));\n\
                 }}\n",
                artifact.source
            );
            fs::write(&source_path, source).unwrap();

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

    /// Run explicitly with `cargo test -p goya-caprichios data_caprichio::tests::benchmark_construction -- --ignored --nocapture`.
    #[test]
    #[ignore = "manual recipe-construction microbenchmark"]
    fn benchmark_construction() {
        use std::time::Instant;
        let ir = Declaration {
            max_payload_bytes: 4096,
        }
        .normalize()
        .unwrap();
        let start = Instant::now();
        let iterations = 100_000;
        for _ in 0..iterations {
            std::hint::black_box(ir.lower_checked(host64()).unwrap());
        }
        eprintln!(
            "{iterations} bounded data recipe constructions in {:?}",
            start.elapsed()
        );
    }
}
