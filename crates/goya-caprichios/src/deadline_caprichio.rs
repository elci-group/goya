//! A cooperative process-local monotonic deadline capability.
//!
//! A deadline exposes remaining time and expiration checks. It does not
//! preempt work, interrupt blocking operations, or guarantee when a scheduler
//! runs a task.

use std::fmt;
use std::time::{Duration, Instant};

pub const CONTRACT: &str = "goya.compute.deadline.cooperative_process_local.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_DEADLINE_MS: u64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeadlineDeclaration {
    /// Positive, bounded duration from construction until expiration.
    pub duration_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeadlineIr {
    duration_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocalRust,
    CrossProcess,
    Browser,
    Node,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub process_local_memory: bool,
    pub monotonic_clock: bool,
    pub checked_instant_add: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroDuration,
    DurationTooLarge,
    UnsupportedTarget,
    MissingProcessLocalMemory,
    MissingMonotonicClock,
    MissingCheckedInstantAdd,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroDuration => "deadline duration must be positive",
            Self::DurationTooLarge => "deadline duration exceeds the supported 30-day bound",
            Self::UnsupportedTarget => "deadline supports only process-local Rust",
            Self::MissingProcessLocalMemory => "target runtime must provide process-local memory",
            Self::MissingMonotonicClock => "target runtime must provide a monotonic clock",
            Self::MissingCheckedInstantAdd => {
                "target runtime must provide checked monotonic addition"
            }
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeadlineError {
    MonotonicOverflow,
}

impl fmt::Display for DeadlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("deadline instant is outside the representable monotonic clock range")
    }
}

impl std::error::Error for DeadlineError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeadlineArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Deadline {
    expires_at: Instant,
}

impl Deadline {
    pub fn after(duration: Duration) -> Result<Self, DeadlineError> {
        let expires_at = Instant::now()
            .checked_add(duration)
            .ok_or(DeadlineError::MonotonicOverflow)?;
        Ok(Self { expires_at })
    }

    /// Remaining time, clamped to zero after expiration.
    pub fn remaining(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

impl DeadlineDeclaration {
    pub fn normalize(self) -> Result<DeadlineIr, Diagnostic> {
        if self.duration_ms == 0 {
            return Err(Diagnostic::ZeroDuration);
        }
        if self.duration_ms > MAX_DEADLINE_MS {
            return Err(Diagnostic::DurationTooLarge);
        }
        Ok(DeadlineIr {
            duration_ms: self.duration_ms,
        })
    }
}

impl DeadlineIr {
    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=process_local_rust\nduration_ms={}\nclock=monotonic\nbehavior=cooperative_check_only\n",
            self.duration_ms
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<DeadlineArtifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.process_local_memory {
            return Err(Diagnostic::MissingProcessLocalMemory);
        }
        if !capabilities.monotonic_clock {
            return Err(Diagnostic::MissingMonotonicClock);
        }
        if !capabilities.checked_instant_add {
            return Err(Diagnostic::MissingCheckedInstantAdd);
        }
        Ok(DeadlineArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; cooperative checks only; no preemption or scheduling guarantee.\npub fn make_deadline() -> Result<crate::deadline_caprichio::Deadline, crate::deadline_caprichio::DeadlineError> {{\n    crate::deadline_caprichio::Deadline::after(std::time::Duration::from_millis({}u64))\n}}\n",
                self.duration_ms
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: RuntimeCapabilities = RuntimeCapabilities {
        process_local_memory: true,
        monotonic_clock: true,
        checked_instant_add: true,
    };

    fn declaration(duration_ms: u64) -> DeadlineDeclaration {
        DeadlineDeclaration { duration_ms }
    }

    #[test]
    fn canonical_ir_and_factory_are_deterministic_and_narrow() {
        let ir = declaration(250).normalize().unwrap();
        assert_eq!(ir.duration_ms(), 250);
        let first = ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap();
        assert_eq!(
            first,
            ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap()
        );
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("clock=monotonic\n"));
        assert!(first
            .canonical_ir
            .contains("behavior=cooperative_check_only\n"));
        assert!(first.source.contains("Duration::from_millis(250u64)"));
        assert!(first
            .source
            .contains("no preemption or scheduling guarantee"));
    }

    #[test]
    fn rejects_zero_and_out_of_bound_duration() {
        assert_eq!(declaration(0).normalize(), Err(Diagnostic::ZeroDuration));
        assert_eq!(
            declaration(MAX_DEADLINE_MS + 1).normalize(),
            Err(Diagnostic::DurationTooLarge)
        );
        assert_eq!(
            declaration(MAX_DEADLINE_MS)
                .normalize()
                .unwrap()
                .duration_ms(),
            MAX_DEADLINE_MS
        );
    }

    #[test]
    fn negotiation_fails_closed_on_target_and_clock_requirements() {
        let ir = declaration(1).normalize().unwrap();
        for target in [Target::CrossProcess, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, LOCAL),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    process_local_memory: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingProcessLocalMemory)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    monotonic_clock: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingMonotonicClock)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    checked_instant_add: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingCheckedInstantAdd)
        );
    }

    #[test]
    fn runtime_checks_remaining_expiration_and_overflow() {
        assert_eq!(
            Deadline::after(Duration::MAX),
            Err(DeadlineError::MonotonicOverflow)
        );
        let deadline = Deadline::after(Duration::from_millis(10)).unwrap();
        assert!(!deadline.is_expired());
        assert!(deadline.remaining() <= Duration::from_millis(10));
        let zero = Deadline::after(Duration::ZERO).unwrap();
        assert!(zero.is_expired());
        assert_eq!(zero.remaining(), Duration::ZERO);
    }

    #[test]
    fn generated_factory_compiles_and_runs() {
        use std::process::Command;
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration(100)
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, LOCAL)
            .unwrap();
        let manifest_dir = option_env!("CARGO_MANIFEST_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .expect("current directory should be available")
                    .join("crates/goya-caprichios")
            });
        let source_path = manifest_dir.join("src/deadline_caprichio.rs");
        let harness = format!(
            "#[path = {source_path:?}]\nmod deadline_caprichio;\n{}\nfn main() {{ let deadline = make_deadline().unwrap(); assert!(!deadline.is_expired()); assert!(deadline.remaining() <= std::time::Duration::from_millis(100)); }}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let executable = std::env::temp_dir().join(format!(
            "goya-deadline-factory-{}-{nonce}",
            std::process::id()
        ));
        let _cleanup = BinaryCleanup(executable.clone());
        let output = Command::new("rustc")
            .arg("--edition=2021")
            .arg("-")
            .arg("-o")
            .arg(&executable)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(harness.as_bytes())?;
                child.wait_with_output()
            })
            .expect("rustc should compile generated factory");
        assert!(
            output.status.success(),
            "rustc failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let run = Command::new(&executable)
            .output()
            .expect("generated factory should run");
        assert!(
            run.status.success(),
            "generated program failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    struct BinaryCleanup(std::path::PathBuf);

    impl Drop for BinaryCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    #[ignore = "manual construction/runtime microbenchmark"]
    fn benchmark_deadline_construction_and_checks() {
        use std::time::Instant;
        let ir = declaration(1000).normalize().unwrap();
        let start = Instant::now();
        let mut checks = 0usize;
        for _ in 0..100_000 {
            let deadline = Deadline::after(Duration::from_millis(ir.duration_ms())).unwrap();
            checks += usize::from(!deadline.is_expired());
            checks += usize::from(deadline.remaining() > Duration::ZERO);
        }
        eprintln!(
            "100000 deadline constructions and 200000 checks: {:?} ({checks})",
            start.elapsed()
        );
        std::hint::black_box(checks);
    }
}
