//! Construction of a bounded retry policy.
//!
//! The generated factory binds to this crate's process-local Rust retry
//! runtime. A caller still chooses which failures are retryable and supplies
//! the actual sleep behavior when running the policy.

use std::fmt;

pub const CONTRACT: &str = "goya.compute.retry_policy.bounded_process_local.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryDeclaration {
    /// Includes the initial operation attempt; must be positive.
    pub max_attempts: u32,
    /// Delay after the first retryable failure, in milliseconds.
    pub initial_delay_ms: u64,
    /// Maximum exponential retry delay, in milliseconds.
    pub max_delay_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryIr {
    max_attempts: u32,
    initial_delay_ms: u64,
    max_delay_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocal,
    CrossProcess,
    Browser,
    Node,
}

/// Facilities used by the policy object. Actual waiting is supplied by its caller.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub process_local_memory: bool,
    pub standard_duration: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroAttempts,
    InitialDelayExceedsMaximum,
    UnsupportedTarget,
    MissingProcessLocalMemory,
    MissingStandardDuration,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroAttempts => "maximum attempts must be positive",
            Self::InitialDelayExceedsMaximum => {
                "initial retry delay must not exceed maximum retry delay"
            }
            Self::UnsupportedTarget => {
                "retry policy factory supports only a process-local Rust target"
            }
            Self::MissingProcessLocalMemory => "target runtime must provide process-local memory",
            Self::MissingStandardDuration => "target runtime must provide standard duration values",
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl RetryDeclaration {
    pub fn normalize(self) -> Result<RetryIr, Diagnostic> {
        if self.max_attempts == 0 {
            return Err(Diagnostic::ZeroAttempts);
        }
        if self.initial_delay_ms > self.max_delay_ms {
            return Err(Diagnostic::InitialDelayExceedsMaximum);
        }
        Ok(RetryIr {
            max_attempts: self.max_attempts,
            initial_delay_ms: self.initial_delay_ms,
            max_delay_ms: self.max_delay_ms,
        })
    }
}

impl RetryIr {
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=process_local_rust\nmax_attempts={}\ninitial_delay_ms={}\nmax_delay_ms={}\ndelay_schedule=exponential_capped\n",
            self.max_attempts, self.initial_delay_ms, self.max_delay_ms
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<RetryArtifact, Diagnostic> {
        if target != Target::ProcessLocal {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.process_local_memory {
            return Err(Diagnostic::MissingProcessLocalMemory);
        }
        if !capabilities.standard_duration {
            return Err(Diagnostic::MissingStandardDuration);
        }
        Ok(RetryArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; caller controls retryability and sleeping.\npub fn make_retry_policy() -> Result<crate::compute_runtime::RetryPolicy, crate::compute_runtime::RetryPolicyError> {{\n    crate::compute_runtime::RetryPolicy::new({}, std::time::Duration::from_millis({}u64), std::time::Duration::from_millis({}u64))\n}}\n",
                self.max_attempts, self.initial_delay_ms, self.max_delay_ms
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: RuntimeCapabilities = RuntimeCapabilities {
        process_local_memory: true,
        standard_duration: true,
    };

    #[test]
    fn declaration_has_deterministic_ir_and_factory_binding() {
        let ir = RetryDeclaration {
            max_attempts: 4,
            initial_delay_ms: 5,
            max_delay_ms: 40,
        }
        .normalize()
        .unwrap();
        assert_eq!(ir.max_attempts(), 4);
        let first = ir.lower_checked(Target::ProcessLocal, LOCAL).unwrap();
        let second = ir.lower_checked(Target::ProcessLocal, LOCAL).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("initial_delay_ms=5\n"));
        assert!(first.canonical_ir.contains("max_delay_ms=40\n"));
        assert!(first
            .canonical_ir
            .contains("delay_schedule=exponential_capped\n"));
        assert!(first.source.contains("RetryPolicy::new(4"));
        assert!(first.source.contains("Duration::from_millis(40u64)"));
    }

    #[test]
    fn rejects_invalid_attempts_and_delay_relation() {
        assert_eq!(
            RetryDeclaration {
                max_attempts: 0,
                initial_delay_ms: 0,
                max_delay_ms: 0,
            }
            .normalize(),
            Err(Diagnostic::ZeroAttempts)
        );
        assert_eq!(
            RetryDeclaration {
                max_attempts: 2,
                initial_delay_ms: 5,
                max_delay_ms: 4,
            }
            .normalize(),
            Err(Diagnostic::InitialDelayExceedsMaximum)
        );
    }

    #[test]
    fn negotiation_rejects_unsupported_targets_and_facilities() {
        let ir = RetryDeclaration {
            max_attempts: 1,
            initial_delay_ms: 0,
            max_delay_ms: 0,
        }
        .normalize()
        .unwrap();
        for target in [Target::CrossProcess, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, LOCAL),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocal,
                RuntimeCapabilities {
                    process_local_memory: false,
                    standard_duration: true,
                }
            ),
            Err(Diagnostic::MissingProcessLocalMemory)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocal,
                RuntimeCapabilities {
                    process_local_memory: true,
                    standard_duration: false,
                }
            ),
            Err(Diagnostic::MissingStandardDuration)
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_with_bounded_retry_behavior() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = RetryDeclaration {
            max_attempts: 3,
            initial_delay_ms: 10,
            max_delay_ms: 15,
        }
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
            "#[path = {runtime_path:?}]\nmod compute_runtime;\n{}\nfn main() {{\n    let policy = make_retry_policy().expect(\"factory should build policy\");\n    assert_eq!(policy.max_attempts(), 3);\n    let mut delays = Vec::new();\n    let mut calls = 0;\n    let result = policy.run(\n        |attempt| {{ calls += 1; if attempt < 3 {{ Err::<u32, _>(\"temporary\") }} else {{ Ok(42) }} }},\n        |_| true,\n        |delay| delays.push(delay),\n    );\n    assert_eq!(result, Ok(42));\n    assert_eq!(calls, 3);\n    assert_eq!(delays, [std::time::Duration::from_millis(10), std::time::Duration::from_millis(15)]);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-retry-factory-{}-{nonce}",
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
            .expect("run generated retry factory");
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }

    /// Local construction smoke benchmark, not a stable performance suite.
    #[test]
    #[ignore]
    fn benchmark_retry_factory_construction() {
        let ir = RetryDeclaration {
            max_attempts: 5,
            initial_delay_ms: 1,
            max_delay_ms: 16,
        }
        .normalize()
        .unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100_000 {
            std::hint::black_box(ir.lower_checked(Target::ProcessLocal, LOCAL).unwrap());
        }
        println!(
            "100,000 retry factory constructions in {:?}",
            start.elapsed()
        );
    }
}

#[cfg(test)]
struct BinaryCleanup(std::path::PathBuf);

#[cfg(test)]
impl Drop for BinaryCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
