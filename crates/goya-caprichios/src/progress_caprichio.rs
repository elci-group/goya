//! Construction of fixed-total, monotonic progress tracking.
//!
//! This narrow factory binds to the crate's process-local `Progress` type. It
//! does not implement scheduling, persistence, or cross-process coordination.

use std::fmt;

pub const CONTRACT: &str = "goya.compute.progress.fixed_total_process_local.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressDeclaration {
    /// Fixed nonzero count of work units represented by this tracker.
    pub total: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressIr {
    total: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// A Rust process containing this crate's runtime module.
    ProcessLocal,
    CrossProcess,
    Browser,
    Node,
}

/// The host must explicitly confirm process-local memory for this target.
/// Progress values use fixed-width `u64` counters, so no pointer-width
/// capability is required.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub process_local_memory: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroTotal,
    UnsupportedTarget,
    MissingProcessLocalMemory,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroTotal => "progress total must be positive",
            Self::UnsupportedTarget => "progress factory supports only a process-local Rust target",
            Self::MissingProcessLocalMemory => "target runtime must provide process-local memory",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Constructed source is a factory definition, not proof of crate integration
/// until the emitted code is compiled in its consumer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl ProgressDeclaration {
    pub fn normalize(self) -> Result<ProgressIr, Diagnostic> {
        if self.total == 0 {
            return Err(Diagnostic::ZeroTotal);
        }
        Ok(ProgressIr { total: self.total })
    }
}

impl ProgressIr {
    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=process_local_rust\nprogress=fixed_total_monotonic\ntotal={}\n",
            self.total
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<ProgressArtifact, Diagnostic> {
        if target != Target::ProcessLocal {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.process_local_memory {
            return Err(Diagnostic::MissingProcessLocalMemory);
        }
        Ok(ProgressArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; process-local monotonic fixed-total progress.\npub fn make_progress() -> Result<crate::compute_runtime::Progress, crate::compute_runtime::ProgressError> {{\n    crate::compute_runtime::Progress::new({}u64)\n}}\n",
                self.total
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BinaryCleanup(std::path::PathBuf);

    impl Drop for BinaryCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    const LOCAL: RuntimeCapabilities = RuntimeCapabilities {
        process_local_memory: true,
    };

    #[test]
    fn fixed_total_normalizes_to_reproducible_factory_and_ir() {
        let ir = ProgressDeclaration { total: 99 }.normalize().unwrap();
        assert_eq!(ir.total(), 99);
        let artifact = ir.lower_checked(Target::ProcessLocal, LOCAL).unwrap();
        assert_eq!(
            artifact,
            ir.lower_checked(Target::ProcessLocal, LOCAL).unwrap()
        );
        assert_eq!(artifact.contract, CONTRACT);
        assert!(artifact.canonical_ir.contains("total=99\n"));
        assert!(artifact
            .source
            .contains("crate::compute_runtime::Progress::new(99u64)"));
    }

    #[test]
    fn rejects_zero_total_nonlocal_targets_and_missing_memory() {
        assert_eq!(
            ProgressDeclaration { total: 0 }.normalize(),
            Err(Diagnostic::ZeroTotal)
        );
        let ir = ProgressDeclaration { total: 1 }.normalize().unwrap();
        for target in [Target::CrossProcess, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, LOCAL),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(Target::ProcessLocal, RuntimeCapabilities::default()),
            Err(Diagnostic::MissingProcessLocalMemory)
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_compute_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = ProgressDeclaration { total: 5 }
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocal, LOCAL)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/compute_runtime.rs");
        assert!(
            runtime_path.is_file(),
            "compute_runtime.rs should be present next to this crate"
        );
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod compute_runtime;\n{}\nfn main() {{\n    let mut progress = make_progress().expect(\"factory should construct tracker\");\n    assert_eq!(progress.total(), 5);\n    assert_eq!(progress.completed(), 0);\n    progress.advance_to(3).unwrap();\n    assert_eq!(progress.completed(), 3);\n    progress.advance_to(5).unwrap();\n    assert!(progress.is_complete());\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-progress-factory-{}-{nonce}",
            std::process::id()
        ));
        let _cleanup = BinaryCleanup(output_path.clone());
        let mut rustc = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&output_path)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to compile generated source");
        rustc
            .stdin
            .take()
            .expect("rustc stdin")
            .write_all(harness.as_bytes())
            .expect("write generated factory harness");
        let compile = rustc.wait_with_output().expect("wait for rustc");
        assert!(
            compile.status.success(),
            "generated factory failed to compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let execution = Command::new(&output_path)
            .output()
            .expect("run generated progress factory");
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }

    /// Local construction smoke benchmark, not a stable performance suite.
    #[test]
    #[ignore]
    fn benchmark_progress_factory_construction() {
        let ir = ProgressDeclaration { total: 10_000 }.normalize().unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100_000 {
            std::hint::black_box(ir.lower_checked(Target::ProcessLocal, LOCAL).unwrap());
        }
        println!(
            "100,000 progress factory constructions in {:?}",
            start.elapsed()
        );
    }
}
