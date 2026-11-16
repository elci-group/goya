//! CLI for experimental source inspection and standalone static output.

mod pack;

/// Experimental compiler-facing construction API. This does not enable
/// network effects in Goya source or in Maja's static application profile.
pub mod capability {
    use goya_caprichios::{Artifact, Diagnostic, NetworkDeclaration, Target};
    use sha2::{Digest, Sha256};
    use std::{fs, io, io::Write, path::Path};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct NetworkOutput {
        pub artifact: Artifact,
        pub source_sha256: [u8; 32],
        pub ir_sha256: [u8; 32],
        /// Canonical build-record snapshot. It is not a signed attestation.
        pub provenance: String,
    }

    impl NetworkOutput {
        /// Verify the record by independently reconstructing this version's
        /// browser recipe from canonical IR, then return ordered bundle entries.
        /// This is consistency checking, not a signature or toolchain proof.
        pub fn verified_bundle(&self) -> Result<[(&'static str, &[u8]); 3], Diagnostic> {
            let expected = from_canonical_ir(&self.artifact.canonical_ir)?;
            if *self != expected {
                return Err(Diagnostic(
                    "capability output does not match its canonical recipe and provenance",
                ));
            }
            Ok([
                ("network.mjs", self.artifact.source.as_bytes()),
                ("network.ir", self.artifact.canonical_ir.as_bytes()),
                ("provenance.txt", self.provenance.as_bytes()),
            ])
        }

        /// Write only to a new private directory; existing paths are preserved.
        pub fn write_bundle(&self, output: &Path) -> io::Result<()> {
            let entries = self.verified_bundle().map_err(invalid_data)?;
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(output)?;
            let mut created = Vec::new();
            let result = (|| {
                for (name, bytes) in entries {
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(output.join(name))?;
                    created.push(name);
                    file.write_all(bytes)?;
                }
                Ok(())
            })();
            if result.is_err() {
                for name in created {
                    let _ = fs::remove_file(output.join(name));
                }
                let _ = fs::remove_dir(output);
            }
            result
        }
    }

    fn invalid_data(error: Diagnostic) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, error)
    }

    fn from_canonical_ir(ir: &str) -> Result<NetworkOutput, Diagnostic> {
        let mut lines = ir.lines();
        if lines.next() != Some("contract=goya.network.fetch.v1") {
            return Err(Diagnostic("unsupported capability contract"));
        }
        let timeout_ms = lines
            .next()
            .and_then(|s| s.strip_prefix("timeout_ms="))
            .and_then(|s| s.parse().ok())
            .ok_or(Diagnostic("invalid canonical timeout"))?;
        let concurrency = lines
            .next()
            .and_then(|s| s.strip_prefix("concurrency="))
            .and_then(|s| s.parse().ok())
            .ok_or(Diagnostic("invalid canonical concurrency"))?;
        if lines.next().is_some() {
            return Err(Diagnostic("unexpected capability IR fields"));
        }
        let expected = construct_network(NetworkDeclaration {
            timeout_ms,
            concurrency,
            retries: 0,
            offline_queue: false,
        })?;
        if ir != expected.artifact.canonical_ir {
            return Err(Diagnostic("capability IR is not canonical"));
        }
        Ok(expected)
    }

    /// Check a private bundle directory against the linked compiler recipe.
    /// This is not a signature or a substitute for trusted inputs/toolchains.
    pub fn verify_bundle_directory(directory: &Path) -> io::Result<()> {
        let metadata = fs::symlink_metadata(directory)?;
        if !metadata.file_type().is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bundle is not a directory",
            ));
        }
        let names = ["network.mjs", "network.ir", "provenance.txt"];
        let mut found = fs::read_dir(directory)?
            .map(|entry| entry.map(|e| e.file_name()))
            .collect::<io::Result<Vec<_>>>()?;
        found.sort();
        let mut expected_names = names.map(std::ffi::OsString::from);
        expected_names.sort();
        if found != expected_names {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected bundle entries",
            ));
        }
        let mut contents = Vec::new();
        for name in names {
            let path = directory.join(name);
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.file_type().is_file() || metadata.len() > 1_048_576 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "bundle entry is not a bounded regular file",
                ));
            }
            contents.push(fs::read(path)?);
        }
        let ir = std::str::from_utf8(&contents[1])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let reconstructed = from_canonical_ir(ir).map_err(invalid_data)?;
        for ((_, expected), actual) in reconstructed
            .verified_bundle()
            .map_err(invalid_data)?
            .iter()
            .zip(&contents)
        {
            if *expected != actual.as_slice() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "bundle content does not match recipe",
                ));
            }
        }
        Ok(())
    }

    pub fn construct_network(declaration: NetworkDeclaration) -> Result<NetworkOutput, Diagnostic> {
        let artifact = declaration.normalize()?.lower(Target::Browser);
        let source_sha256: [u8; 32] = Sha256::digest(artifact.source.as_bytes()).into();
        let ir_sha256: [u8; 32] = Sha256::digest(artifact.canonical_ir.as_bytes()).into();
        let hex = |bytes: &[u8; 32]| -> String {
            use std::fmt::Write;
            let mut result = String::with_capacity(64);
            for byte in bytes {
                write!(&mut result, "{byte:02x}").expect("writing to a string cannot fail");
            }
            result
        };
        let provenance = format!(
            "schema=maja.caprichio.provenance.v1\ncontract={}\nrecipe={}\nrecipe_version={}\ntarget=browser\nsource_sha256={}\nir_sha256={}\n",
            artifact.contract, artifact.recipe, artifact.recipe_version, hex(&source_sha256), hex(&ir_sha256)
        );
        Ok(NetworkOutput {
            source_sha256,
            ir_sha256,
            artifact,
            provenance,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn browser_construction_tracks_inputs_and_rejects_unsupported_policy() {
            let mut declaration = NetworkDeclaration {
                timeout_ms: 1000,
                concurrency: 2,
                retries: 0,
                offline_queue: false,
            };
            let first = construct_network(declaration).unwrap();
            assert_eq!(first, construct_network(declaration).unwrap());
            assert_eq!(first.artifact.recipe, "browser-fetch");
            assert!(first.provenance.starts_with("schema=maja.caprichio.provenance.v1\ncontract=goya.network.fetch.v1\nrecipe=browser-fetch\n"));
            declaration.timeout_ms += 1;
            let changed = construct_network(declaration).unwrap();
            assert_ne!(first.ir_sha256, changed.ir_sha256);
            assert_ne!(first.source_sha256, changed.source_sha256);
            assert_ne!(first.provenance, changed.provenance);
            declaration.offline_queue = true;
            assert!(construct_network(declaration).is_err());
        }

        #[test]
        fn bundle_is_repeatable_and_rejects_tampering() {
            let declaration = NetworkDeclaration {
                timeout_ms: 1000,
                concurrency: 2,
                retries: 0,
                offline_queue: false,
            };
            let original = construct_network(declaration).unwrap();
            assert_eq!(
                original.verified_bundle().unwrap(),
                construct_network(declaration)
                    .unwrap()
                    .verified_bundle()
                    .unwrap()
            );
            let mut changed = original.clone();
            changed.artifact.source.push_str("// modified\n");
            assert!(changed.verified_bundle().is_err());
            let mut changed = original.clone();
            changed.artifact.canonical_ir = changed.artifact.canonical_ir.replace("1000", "01000");
            assert!(changed.verified_bundle().is_err());
            let mut changed = original.clone();
            changed.source_sha256[0] ^= 1;
            assert!(changed.verified_bundle().is_err());
            let mut changed = original.clone();
            changed.provenance.push_str("extra=value\n");
            assert!(changed.verified_bundle().is_err());
            let mut changed = original;
            changed.artifact.recipe = "node-fetch";
            assert!(changed.verified_bundle().is_err());
        }

        #[test]
        fn disk_bundle_round_trip_and_tampering() {
            let root = std::env::temp_dir().join(format!(
                "maja-caprichio-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let first = root.with_extension("first");
            let second = root.with_extension("second");
            let output = construct_network(NetworkDeclaration {
                timeout_ms: 1000,
                concurrency: 2,
                retries: 0,
                offline_queue: false,
            })
            .unwrap();
            output.write_bundle(&first).unwrap();
            output.write_bundle(&second).unwrap();
            verify_bundle_directory(&first).unwrap();
            verify_bundle_directory(&second).unwrap();
            for name in ["network.mjs", "network.ir", "provenance.txt"] {
                assert_eq!(
                    fs::read(first.join(name)).unwrap(),
                    fs::read(second.join(name)).unwrap()
                );
            }
            assert_eq!(
                output.write_bundle(&first).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            let mut invalid = output.clone();
            invalid.provenance.push_str("altered\n");
            let rejected = root.with_extension("rejected");
            assert!(invalid.write_bundle(&rejected).is_err());
            assert!(!rejected.exists());
            fs::write(second.join("provenance.txt"), "altered").unwrap();
            assert_eq!(
                verify_bundle_directory(&second).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            fs::write(first.join("network.mjs"), "changed").unwrap();
            assert_eq!(
                verify_bundle_directory(&first).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            fs::remove_file(first.join("network.ir")).unwrap();
            assert!(verify_bundle_directory(&first).is_err());
            fs::remove_dir_all(first).unwrap();
            fs::remove_dir_all(second).unwrap();
        }
    }
}

use std::{ffi::OsString, fs, io::Write, path::PathBuf};

const USAGE: &str = "usage: maja inspect SOURCE | maja check SOURCE [--target web] --profile PROFILE | maja build SOURCE --profile PROFILE [--release] --out NEW_DIR";

/// Runs the CLI with injectable arguments and output streams.
/// Exit codes: 0 success, 2 usage, 3 input error, 4 rejection, 5 output error.
pub fn run<I>(args: I, out: &mut dyn Write, err: &mut dyn Write) -> i32
where
    I: IntoIterator<Item = OsString>,
{
    let args: Vec<OsString> = args.into_iter().skip(1).collect();
    let Some(command) = args.first().and_then(|s| s.to_str()) else {
        let _ = writeln!(err, "MAJA-CLI-E0001: {USAGE}");
        return 2;
    };
    if command == "--help" || command == "-h" {
        let _ = writeln!(out, "{USAGE}");
        return 0;
    }
    if !matches!(command, "inspect" | "check" | "build") || args.len() < 2 {
        let _ = writeln!(err, "MAJA-CLI-E0001: {USAGE}");
        return 2;
    }
    let source_path = PathBuf::from(&args[1]);
    let mut profile: Option<&str> = None;
    let mut target: Option<&str> = None;
    let mut output: Option<&std::ffi::OsStr> = None;
    let mut release = false;
    let mut i = 2;
    while i < args.len() {
        match args[i].to_str() {
            Some("--profile") if profile.is_none() && i + 1 < args.len() => {
                profile = args[i + 1].to_str();
                if profile.is_none() {
                    return usage(err);
                }
                i += 2;
            }
            Some("--target") if target.is_none() && i + 1 < args.len() => {
                target = args[i + 1].to_str();
                if target.is_none() {
                    return usage(err);
                }
                i += 2;
            }
            Some("--out") if output.is_none() && i + 1 < args.len() => {
                output = Some(&args[i + 1]);
                i += 2;
            }
            Some("--release") if !release => {
                release = true;
                i += 1;
            }
            _ => return usage(err),
        }
    }
    if (command == "inspect"
        && (profile.is_some() || target.is_some() || output.is_some() || release))
        || (command == "check" && (output.is_some() || release))
        || (command == "build" && output.is_none())
        || (command != "inspect" && profile.is_none())
    {
        return usage(err);
    }
    if target.is_some_and(|t| t != "web") {
        let _ = writeln!(err, "MAJA-CLI-E0002: unsupported target");
        return 4;
    }
    if profile.is_some_and(|p| p != maja_semantic::WEB_PROFILE && p != maja_semantic::LOCAL_PROFILE)
    {
        let _ = writeln!(err, "MAJA-CLI-E0003: unknown profile");
        return 4;
    }
    if release && profile == Some(maja_semantic::LOCAL_PROFILE) {
        let _ = writeln!(
            err,
            "MAJA-CLI-E0006: release builds are not qualified for the experimental local profile"
        );
        return 4;
    }
    let source = match fs::read_to_string(&source_path) {
        Ok(source) => source,
        Err(error) => {
            let _ = writeln!(err, "MAJA-CLI-E0004: cannot read source: {error}");
            return 3;
        }
    };
    inspect_source(&source, command, profile, output, out, err)
}

fn usage(err: &mut dyn Write) -> i32 {
    let _ = writeln!(err, "MAJA-CLI-E0001: {USAGE}");
    2
}

fn inspect_source(
    source: &str,
    command: &str,
    profile: Option<&str>,
    output: Option<&std::ffi::OsStr>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let inspected = match maja_semantic::inspect(source) {
        Ok(scene) => scene,
        Err(diagnostic) => return reject(err, &diagnostic),
    };
    if command == "inspect" {
        let view = inspected.handoff();
        let _ = writeln!(out, "source_contract: {}", view.contract);
        let _ = writeln!(out, "source_entries: {}", view.parsed.entries.len());
        let _ = writeln!(
            out,
            "web_disposition: profile-specific; experimental static subset only"
        );
        return 0;
    }
    if profile == Some(maja_semantic::LOCAL_PROFILE) {
        let ir = match maja_semantic::admit_local_static(&inspected, maja_semantic::LOCAL_PROFILE) {
            Ok(ir) => ir,
            Err(diagnostic) => return reject(err, &diagnostic),
        };
        if command == "check" {
            let _ = writeln!(out, "admitted: experimental static subset");
            return 0;
        }
        return match pack::write_static(
            &ir,
            std::path::Path::new(output.expect("build requires output")),
        ) {
            Ok(()) => {
                let _ = writeln!(out, "built: experimental static subset");
                0
            }
            Err(error) => {
                let _ = writeln!(err, "MAJA-CLI-E0005: output failed: {error}");
                5
            }
        };
    }
    match maja_semantic::admit_web(&inspected, maja_semantic::WEB_PROFILE) {
        Err(diagnostic) => reject(err, &diagnostic),
        Ok(_) => {
            // Emission is not implemented. Even if admission changes, this
            // CLI must not report a successful build or create artifacts.
            let _ = writeln!(err, "MAJA-CLI-E0005: browser emission is unavailable");
            4
        }
    }
}

fn reject(err: &mut dyn Write, diagnostic: &maja_semantic::Diagnostic) -> i32 {
    if let Some(range) = diagnostic.range {
        let _ = writeln!(
            err,
            "{}: {:?} [bytes {}..{}]",
            diagnostic.code, diagnostic.message, range.start, range.end
        );
    } else {
        let _ = writeln!(err, "{}: {:?}", diagnostic.code, diagnostic.message);
    }
    4
}
