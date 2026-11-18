//! Bounded process-local priority queue over byte-vector items.
//!
//! Larger signed priorities are selected first; equal priorities are FIFO.
//! Only eligible items can be removed. Priority and eligibility changes are
//! serialized with enqueue/dequeue by one mutex. This describes selection
//! inside this queue only; it makes no OS scheduling or start-time promise.

use std::fmt;
use std::sync::Mutex;

pub const CONTRACT: &str = "goya.compute.priority_queue.process_local.v1";
pub const RECIPE: &str = "rust-std-bounded-priority-fifo-eligible-queue";
pub const RECIPE_VERSION: &str = "0.1.0";
pub const MAX_ITEMS: u32 = 1_000_000;
pub const MAX_TOTAL_BYTES: u32 = 64 * 1024 * 1024;
pub const MAX_ITEM_BYTES: u32 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub max_items: u32,
    pub max_total_bytes: u32,
    pub max_item_bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    pub pointer_width_bits: u8,
    pub standard_library: bool,
    pub standard_mutex: bool,
    pub fallible_reservation: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocalRust,
    Browser,
    Node,
    CrossProcess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroLimit,
    LimitTooLarge,
    ItemExceedsTotal,
    UnsupportedTarget,
    UnsupportedPointerWidth,
    AllocationLimitExceeded,
    MissingStandardLibrary,
    MissingStandardMutex,
    MissingFallibleReservation,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroLimit => "queue limits must be positive",
            Self::LimitTooLarge => "queue limit exceeds the supported bound",
            Self::ItemExceedsTotal => "per-item byte limit exceeds total byte limit",
            Self::UnsupportedTarget => "priority queue supports only process-local Rust",
            Self::UnsupportedPointerWidth => "target pointer width must be 32 or 64 bits",
            Self::AllocationLimitExceeded => "queue bound exceeds target allocation limits",
            Self::MissingStandardLibrary => "target requires the Rust standard library",
            Self::MissingStandardMutex => "target requires a standard mutex",
            Self::MissingFallibleReservation => "target requires fallible collection reservation",
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub max_items: usize,
    pub max_total_bytes: usize,
    pub max_item_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ir(Limits);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub target_pointer_width_bits: u8,
    pub source: String,
}

impl Declaration {
    pub fn normalize(self) -> Result<Ir, Diagnostic> {
        if self.max_items == 0 || self.max_total_bytes == 0 || self.max_item_bytes == 0 {
            return Err(Diagnostic::ZeroLimit);
        }
        if self.max_items > MAX_ITEMS
            || self.max_total_bytes > MAX_TOTAL_BYTES
            || self.max_item_bytes > MAX_ITEM_BYTES
        {
            return Err(Diagnostic::LimitTooLarge);
        }
        if self.max_item_bytes > self.max_total_bytes {
            return Err(Diagnostic::ItemExceedsTotal);
        }
        Ok(Ir(Limits {
            max_items: self.max_items as usize,
            max_total_bytes: self.max_total_bytes as usize,
            max_item_bytes: self.max_item_bytes as usize,
        }))
    }
}

impl Ir {
    pub fn limits(&self) -> Limits {
        self.0
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe={RECIPE}@{RECIPE_VERSION}\ntarget=process_local_rust\nitems=owned_byte_vectors\nselection=highest_signed_priority_then_fifo\neligibility=explicit_boolean\npriority_update=by_item_id\nmax_items={}\nmax_total_bytes={}\nmax_item_bytes={}\noverflow=reject_without_enqueuing\nsharing=single_process_mutex_serialized\nos_scheduling_guarantee=none\n",
            self.0.max_items, self.0.max_total_bytes, self.0.max_item_bytes
        )
    }

    pub fn lower_checked(&self, target: Target, host: HostProfile) -> Result<Artifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        let isize_max = match host.pointer_width_bits {
            32 => i32::MAX as u64,
            64 => i64::MAX as u64,
            _ => return Err(Diagnostic::UnsupportedPointerWidth),
        };
        if u64::from(self.0.max_total_bytes as u32) > isize_max
            || self.0.max_items as u64 > isize_max
        {
            return Err(Diagnostic::AllocationLimitExceeded);
        }
        if !host.standard_library {
            return Err(Diagnostic::MissingStandardLibrary);
        }
        if !host.standard_mutex {
            return Err(Diagnostic::MissingStandardMutex);
        }
        if !host.fallible_reservation {
            return Err(Diagnostic::MissingFallibleReservation);
        }
        let canonical_ir = format!(
            "{}target_pointer_width_bits={}\n",
            self.canonical(),
            host.pointer_width_bits
        );
        let limits = self.0;
        let source = format!(
            "// {CONTRACT}; {RECIPE}@{RECIPE_VERSION}; queue selection only, no OS scheduling guarantee.\npub fn make_priority_queue() -> Result<crate::priority_caprichio::PriorityQueue, crate::priority_caprichio::QueueError> {{\n    crate::priority_caprichio::PriorityQueue::new(crate::priority_caprichio::Limits {{ max_items: {}usize, max_total_bytes: {}usize, max_item_bytes: {}usize }})\n}}\n",
            limits.max_items, limits.max_total_bytes, limits.max_item_bytes
        );
        Ok(Artifact {
            contract: CONTRACT,
            recipe: RECIPE,
            recipe_version: RECIPE_VERSION,
            canonical_ir,
            target_pointer_width_bits: host.pointer_width_bits,
            source,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueueError {
    ItemTooLarge { maximum: usize },
    QueueFull { maximum: usize },
    ByteLimitExceeded { maximum: usize },
    AllocationFailed { requested: usize },
    IdentifierExhausted,
    UnknownItem { id: u64 },
    MutexPoisoned,
}

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ItemTooLarge { maximum } => write!(f, "item exceeds {maximum} bytes"),
            Self::QueueFull { maximum } => write!(f, "queue item count reached {maximum}"),
            Self::ByteLimitExceeded { maximum } => write!(f, "queue bytes would exceed {maximum}"),
            Self::AllocationFailed { requested } => {
                write!(f, "unable to reserve for {requested} items")
            }
            Self::IdentifierExhausted => f.write_str("queue item identifiers are exhausted"),
            Self::UnknownItem { id } => write!(f, "queue item {id} does not exist"),
            Self::MutexPoisoned => f.write_str("priority queue mutex is poisoned"),
        }
    }
}

impl std::error::Error for QueueError {}

#[derive(Debug)]
struct Entry {
    id: u64,
    sequence: u64,
    priority: i32,
    eligible: bool,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct State {
    entries: Vec<Entry>,
    total_bytes: usize,
    next_id: u64,
    next_sequence: u64,
}

/// Thread-safe queue. Each public operation is atomic relative to other
/// operations; a dequeue returns an owned payload and removes it exactly once.
#[derive(Debug)]
pub struct PriorityQueue {
    limits: Limits,
    state: Mutex<State>,
}

impl PriorityQueue {
    pub fn new(limits: Limits) -> Result<Self, QueueError> {
        if limits.max_items == 0
            || limits.max_total_bytes == 0
            || limits.max_item_bytes == 0
            || limits.max_item_bytes > limits.max_total_bytes
        {
            return Err(QueueError::ByteLimitExceeded {
                maximum: limits.max_total_bytes,
            });
        }
        let mut entries = Vec::new();
        entries.try_reserve(limits.max_items.min(64)).map_err(|_| {
            QueueError::AllocationFailed {
                requested: limits.max_items.min(64),
            }
        })?;
        Ok(Self {
            limits,
            state: Mutex::new(State {
                entries,
                total_bytes: 0,
                next_id: 1,
                next_sequence: 0,
            }),
        })
    }

    pub fn enqueue(
        &self,
        priority: i32,
        eligible: bool,
        bytes: Vec<u8>,
    ) -> Result<u64, QueueError> {
        if bytes.len() > self.limits.max_item_bytes {
            return Err(QueueError::ItemTooLarge {
                maximum: self.limits.max_item_bytes,
            });
        }
        let mut state = self.state.lock().map_err(|_| QueueError::MutexPoisoned)?;
        if state.entries.len() >= self.limits.max_items {
            return Err(QueueError::QueueFull {
                maximum: self.limits.max_items,
            });
        }
        let total =
            state
                .total_bytes
                .checked_add(bytes.len())
                .ok_or(QueueError::ByteLimitExceeded {
                    maximum: self.limits.max_total_bytes,
                })?;
        if total > self.limits.max_total_bytes {
            return Err(QueueError::ByteLimitExceeded {
                maximum: self.limits.max_total_bytes,
            });
        }
        let id = state.next_id;
        let following_id = id.checked_add(1).ok_or(QueueError::IdentifierExhausted)?;
        let following_sequence = state
            .next_sequence
            .checked_add(1)
            .ok_or(QueueError::IdentifierExhausted)?;
        let next_len = state.entries.len() + 1;
        state
            .entries
            .try_reserve(1)
            .map_err(|_| QueueError::AllocationFailed {
                requested: next_len,
            })?;
        let sequence = state.next_sequence;
        state.entries.push(Entry {
            id,
            sequence,
            priority,
            eligible,
            bytes,
        });
        state.total_bytes = total;
        state.next_id = following_id;
        state.next_sequence = following_sequence;
        Ok(id)
    }

    pub fn update_priority(&self, id: u64, priority: i32) -> Result<(), QueueError> {
        let mut state = self.state.lock().map_err(|_| QueueError::MutexPoisoned)?;
        let entry = state
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(QueueError::UnknownItem { id })?;
        entry.priority = priority;
        Ok(())
    }

    pub fn set_eligible(&self, id: u64, eligible: bool) -> Result<(), QueueError> {
        let mut state = self.state.lock().map_err(|_| QueueError::MutexPoisoned)?;
        let entry = state
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(QueueError::UnknownItem { id })?;
        entry.eligible = eligible;
        Ok(())
    }

    pub fn dequeue(&self) -> Result<Option<(u64, i32, Vec<u8>)>, QueueError> {
        let mut state = self.state.lock().map_err(|_| QueueError::MutexPoisoned)?;
        let selected = state
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.eligible)
            .max_by(|(_, a), (_, b)| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| b.sequence.cmp(&a.sequence))
            })
            .map(|(index, _)| index);
        let Some(index) = selected else {
            return Ok(None);
        };
        let entry = state.entries.swap_remove(index);
        state.total_bytes -= entry.bytes.len();
        Ok(Some((entry.id, entry.priority, entry.bytes)))
    }

    pub fn len(&self) -> Result<usize, QueueError> {
        self.state
            .lock()
            .map(|state| state.entries.len())
            .map_err(|_| QueueError::MutexPoisoned)
    }

    pub fn is_empty(&self) -> Result<bool, QueueError> {
        Ok(self.len()? == 0)
    }

    pub fn total_bytes(&self) -> Result<usize, QueueError> {
        self.state
            .lock()
            .map(|state| state.total_bytes)
            .map_err(|_| QueueError::MutexPoisoned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Declaration {
        Declaration {
            max_items: 32,
            max_total_bytes: 4096,
            max_item_bytes: 1024,
        }
    }

    fn host() -> HostProfile {
        HostProfile {
            pointer_width_bits: usize::BITS as u8,
            standard_library: true,
            standard_mutex: true,
            fallible_reservation: true,
        }
    }

    fn limits() -> Limits {
        declaration().normalize().unwrap().limits()
    }

    #[test]
    fn declaration_and_lowering_are_bounded_deterministic_and_fail_closed() {
        assert_eq!(
            Declaration {
                max_items: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroLimit)
        );
        assert_eq!(
            Declaration {
                max_total_bytes: MAX_TOTAL_BYTES + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::LimitTooLarge)
        );
        assert_eq!(
            Declaration {
                max_item_bytes: 4097,
                max_total_bytes: 2048,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ItemExceedsTotal)
        );
        let ir = declaration().normalize().unwrap();
        let first = ir.lower_checked(Target::ProcessLocalRust, host()).unwrap();
        assert_eq!(
            first,
            ir.lower_checked(Target::ProcessLocalRust, host()).unwrap()
        );
        assert!(first
            .canonical_ir
            .contains("selection=highest_signed_priority_then_fifo\n"));
        assert!(first
            .canonical_ir
            .contains("os_scheduling_guarantee=none\n"));
        assert!(first.source.contains("PriorityQueue::new"));
        assert_eq!(
            ir.lower_checked(Target::CrossProcess, host()),
            Err(Diagnostic::UnsupportedTarget)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    standard_mutex: false,
                    ..host()
                }
            ),
            Err(Diagnostic::MissingStandardMutex)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    fallible_reservation: false,
                    ..host()
                }
            ),
            Err(Diagnostic::MissingFallibleReservation)
        );
    }

    #[test]
    fn selects_highest_eligible_then_fifo_and_updates_are_observed() {
        let queue = PriorityQueue::new(limits()).unwrap();
        let first = queue.enqueue(5, true, vec![1]).unwrap();
        let second = queue.enqueue(5, true, vec![2]).unwrap();
        let high_blocked = queue.enqueue(9, false, vec![3]).unwrap();
        let lower = queue.enqueue(-1, true, vec![4]).unwrap();
        assert_eq!(queue.dequeue().unwrap(), Some((first, 5, vec![1])));
        queue.set_eligible(high_blocked, true).unwrap();
        assert_eq!(queue.dequeue().unwrap(), Some((high_blocked, 9, vec![3])));
        queue.update_priority(lower, 5).unwrap();
        assert_eq!(queue.dequeue().unwrap(), Some((second, 5, vec![2])));
        assert_eq!(queue.dequeue().unwrap(), Some((lower, 5, vec![4])));
        assert_eq!(queue.dequeue().unwrap(), None);
    }

    #[test]
    fn rejected_operations_leave_queue_state_unchanged() {
        let tight = Limits {
            max_items: 2,
            max_total_bytes: 3,
            max_item_bytes: 2,
        };
        let queue = PriorityQueue::new(tight).unwrap();
        let id = queue.enqueue(1, false, vec![1, 2]).unwrap();
        assert_eq!(
            queue.enqueue(2, true, vec![3, 4]),
            Err(QueueError::ByteLimitExceeded { maximum: 3 })
        );
        assert_eq!(queue.len().unwrap(), 1);
        assert_eq!(queue.total_bytes().unwrap(), 2);
        assert_eq!(queue.dequeue().unwrap(), None);
        assert_eq!(queue.enqueue(2, true, vec![3]), Ok(id + 1));
        assert_eq!(
            queue.enqueue(3, true, vec![4]),
            Err(QueueError::QueueFull { maximum: 2 })
        );
        assert_eq!(
            queue.update_priority(u64::MAX, 10),
            Err(QueueError::UnknownItem { id: u64::MAX })
        );
        assert_eq!(queue.len().unwrap(), 2);
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_real_queue() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};
        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, host())
            .unwrap();
        let module =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/priority_caprichio.rs");
        let harness = format!("#[path = {module:?}] mod priority_caprichio;\n{}\nfn main() {{ let q = make_priority_queue().unwrap(); let low = q.enqueue(1, true, vec![1]).unwrap(); let hi = q.enqueue(2, true, vec![2]).unwrap(); assert_eq!(q.dequeue().unwrap(), Some((hi, 2, vec![2]))); assert_eq!(q.dequeue().unwrap(), Some((low, 1, vec![1]))); }}\n", artifact.source);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let binary = std::env::temp_dir().join(format!(
            "goya-priority-factory-{}-{nonce}",
            std::process::id()
        ));
        let mut compiler = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&binary)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(harness.as_bytes())
            .unwrap();
        let compile = compiler.wait_with_output().unwrap();
        assert!(
            compile.status.success(),
            "generated priority factory did not compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&binary).output().unwrap();
        let _ = std::fs::remove_file(binary);
        assert!(
            run.status.success(),
            "generated priority factory failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    #[test]
    #[ignore = "manual priority queue benchmark"]
    fn benchmark_priority_queue_construction_and_runtime() {
        use std::time::Instant;
        let start = Instant::now();
        let limits = limits();
        let operations = limits.max_items;
        for _ in 0..1000 {
            let queue = PriorityQueue::new(limits).unwrap();
            for n in 0..operations {
                queue
                    .enqueue((n % 8) as i32, true, vec![n as u8; 64])
                    .unwrap();
            }
            for _ in 0..operations {
                std::hint::black_box(queue.dequeue().unwrap());
            }
        }
        eprintln!(
            "1000 queue constructions with {operations} enqueues/dequeues each: {:?}",
            start.elapsed()
        );
    }
}
