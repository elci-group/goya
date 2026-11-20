//! Small local-only, bounded executor for registered Iberia graphs.
//!
//! The runtime deliberately owns scheduling and deadlines locally. It adds no
//! wire fields or changes to the frozen contract. The defaults cap the model
//! registry at 16, pending jobs and retained results at 8 each, estimated
//! per-job tensor memory at 8 MiB, and scalar work at 16 Mi operations.

use crate::graph::Graph;
use ie_contract::errors::TypedError;
use ie_contract::tensors::TensorData;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Resource bounds for the in-process executor.
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_models: usize,
    pub max_pending: usize,
    pub max_retained: usize,
    pub max_payload_bytes_per_job: u64,
    pub max_work_per_job: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_models: 16,
            max_pending: 8,
            max_retained: 8,
            max_payload_bytes_per_job: 8 * 1024 * 1024,
            max_work_per_job: 16 * 1024 * 1024,
        }
    }
}

/// Result and local timings for one terminal request.
#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    pub result: Result<TensorData, TypedError>,
    pub queue_time: Duration,
    pub execution_time: Duration,
}

struct Job {
    id: u64,
    graph_index: usize,
    inputs: Vec<TensorData>,
    submitted_at: Instant,
    deadline: Instant,
}

/// Bounded FIFO runtime. Calls to `run_one` execute synchronously on the
/// caller's thread; cancellation therefore removes queued work and records a
/// terminal outcome, while an executing graph checks its deadline checkpoints.
pub struct Runtime {
    limits: Limits,
    models: Vec<([u8; 32], Graph)>,
    pending: VecDeque<Job>,
    completed: VecDeque<(u64, Completion)>,
    next_id: Option<u64>,
}

impl Runtime {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            models: Vec::new(),
            pending: VecDeque::new(),
            completed: VecDeque::new(),
            next_id: Some(1),
        }
    }

    pub fn register(&mut self, graph: Graph) -> Result<[u8; 32], TypedError> {
        let digest = graph.digest();
        if self.models.iter().any(|(known, _)| *known == digest) {
            return Ok(digest);
        }
        if self.models.len() >= self.limits.max_models {
            return Err(exhausted("model registry full"));
        }
        self.models.push((digest, graph));
        Ok(digest)
    }

    pub fn submit(
        &mut self,
        digest: [u8; 32],
        inputs: Vec<TensorData>,
        timeout: Duration,
    ) -> Result<u64, TypedError> {
        let Some(graph_index) = self.models.iter().position(|(known, _)| *known == digest) else {
            return Err(TypedError::ModelMissing {
                digest: hex(&digest),
            });
        };
        let graph = &self.models[graph_index].1;
        if self.pending.len() >= self.limits.max_pending {
            return Err(exhausted("pending queue full"));
        }
        graph.validate_inputs(&inputs)?;
        let payload = graph.payload_bytes();
        if payload > self.limits.max_payload_bytes_per_job {
            return Err(exhausted("job payload limit exceeded"));
        }
        let work = graph.work();
        if work > self.limits.max_work_per_job {
            return Err(exhausted("job work limit exceeded"));
        }
        if timeout.is_zero() {
            return Err(TypedError::DeadlineExceeded);
        }
        let submitted_at = Instant::now();
        let deadline = submitted_at
            .checked_add(timeout)
            .ok_or(TypedError::DeadlineExceeded)?;
        let id = self
            .next_id
            .ok_or_else(|| exhausted("request id space exhausted"))?;
        self.next_id = id.checked_add(1);
        self.pending.push_back(Job {
            id,
            graph_index,
            inputs,
            submitted_at,
            deadline,
        });
        Ok(id)
    }

    /// Execute the oldest pending job, if any, and return its request id.
    pub fn run_one(&mut self) -> Option<u64> {
        let job = self.pending.pop_front()?;
        let id = job.id;
        let queue_time = job.submitted_at.elapsed();
        let execution_started = Instant::now();
        let result = if Instant::now() >= job.deadline {
            Err(TypedError::DeadlineExceeded)
        } else {
            match self.models.get(job.graph_index) {
                Some((_, graph)) => graph.execute(job.inputs, || {
                    if Instant::now() >= job.deadline {
                        Err(TypedError::DeadlineExceeded)
                    } else {
                        Ok(())
                    }
                }),
                None => Err(TypedError::Internal {
                    reason: "registered model index became invalid".into(),
                }),
            }
        };
        self.retain(
            id,
            Completion {
                result,
                queue_time,
                execution_time: execution_started.elapsed(),
            },
        );
        Some(id)
    }

    /// Return `None` while pending; terminal completions are consumed once.
    pub fn poll(&mut self, id: u64) -> Result<Option<Completion>, TypedError> {
        if self.pending.iter().any(|job| job.id == id) {
            return Ok(None);
        }
        if let Some(index) = self.completed.iter().position(|(known, _)| *known == id) {
            let (_, completion) = self.completed.remove(index).expect("index was found");
            return Ok(Some(completion));
        }
        Err(TypedError::PeerLost)
    }

    /// Cancel queued work and preserve a pollable terminal cancellation.
    pub fn cancel(&mut self, id: u64) -> Result<(), TypedError> {
        if let Some(index) = self.pending.iter().position(|job| job.id == id) {
            let job = self.pending.remove(index).expect("index was found");
            self.retain(
                id,
                Completion {
                    result: Err(TypedError::Cancelled),
                    queue_time: job.submitted_at.elapsed(),
                    execution_time: Duration::ZERO,
                },
            );
            return Ok(());
        }
        if self.completed.iter().any(|(known, _)| *known == id) {
            return Ok(());
        }
        Err(TypedError::PeerLost)
    }

    fn retain(&mut self, id: u64, completion: Completion) {
        if self.limits.max_retained == 0 {
            return;
        }
        self.completed.push_back((id, completion));
        while self.completed.len() > self.limits.max_retained {
            self.completed.pop_front();
        }
    }
}

fn exhausted(reason: &str) -> TypedError {
    TypedError::ResourceExhausted {
        reason: reason.into(),
    }
}

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Node;
    use ie_contract::fixture::{GoldenFixture, GoldenOp};

    fn graph() -> Graph {
        Graph::compile(
            &[&[2], &[2]],
            &[Node {
                op: GoldenOp::Add,
                left: 0,
                right: 1,
            }],
        )
        .unwrap()
    }

    fn inputs() -> Vec<TensorData> {
        vec![
            GoldenFixture::tensor(&[1.0, 2.0], &[2]),
            GoldenFixture::tensor(&[3.0, 4.0], &[2]),
        ]
    }

    fn limits() -> Limits {
        Limits {
            max_pending: 1,
            max_retained: 2,
            ..Limits::default()
        }
    }

    #[test]
    fn pending_cap_and_completion_release_capacity() {
        let mut runtime = Runtime::new(limits());
        let digest = runtime.register(graph()).unwrap();
        assert_eq!(
            runtime.submit(digest, inputs(), Duration::from_secs(1)),
            Ok(1)
        );
        assert!(matches!(
            runtime.submit(digest, inputs(), Duration::from_secs(1)),
            Err(TypedError::ResourceExhausted { .. })
        ));
        assert_eq!(runtime.run_one(), Some(1));
        assert_eq!(
            runtime.submit(digest, inputs(), Duration::from_secs(1)),
            Ok(2)
        );
    }

    #[test]
    fn cancellation_is_terminal_and_releases_pending_slot() {
        let mut runtime = Runtime::new(limits());
        let digest = runtime.register(graph()).unwrap();
        let id = runtime
            .submit(digest, inputs(), Duration::from_secs(1))
            .unwrap();
        runtime.cancel(id).unwrap();
        assert_eq!(
            runtime.poll(id).unwrap().unwrap().result,
            Err(TypedError::Cancelled)
        );
        assert_eq!(
            runtime.submit(digest, inputs(), Duration::from_secs(1)),
            Ok(2)
        );
    }

    #[test]
    fn zero_timeout_is_rejected_as_expired() {
        let mut runtime = Runtime::new(limits());
        let digest = runtime.register(graph()).unwrap();
        assert_eq!(
            runtime.submit(digest, inputs(), Duration::ZERO),
            Err(TypedError::DeadlineExceeded)
        );
    }

    #[test]
    fn payload_and_work_budgets_are_enforced_before_admission() {
        let graph = graph();
        let mut small_payload = Runtime::new(Limits {
            max_payload_bytes_per_job: 23,
            ..Limits::default()
        });
        let digest = small_payload.register(graph.clone()).unwrap();
        assert!(matches!(
            small_payload.submit(digest, inputs(), Duration::from_secs(1)),
            Err(TypedError::ResourceExhausted { .. })
        ));
        assert_eq!(small_payload.pending.len(), 0);

        let mut small_work = Runtime::new(Limits {
            max_work_per_job: 1,
            ..Limits::default()
        });
        let digest = small_work.register(graph).unwrap();
        assert!(matches!(
            small_work.submit(digest, inputs(), Duration::from_secs(1)),
            Err(TypedError::ResourceExhausted { .. })
        ));
        assert_eq!(small_work.pending.len(), 0);
    }

    #[test]
    fn retained_results_are_bounded_and_consumed_once() {
        let mut runtime = Runtime::new(limits());
        let digest = runtime.register(graph()).unwrap();
        for expected in 1..=3 {
            let id = runtime
                .submit(digest, inputs(), Duration::from_secs(1))
                .unwrap();
            assert_eq!(id, expected);
            runtime.run_one();
        }
        assert_eq!(runtime.poll(1), Err(TypedError::PeerLost));
        assert!(runtime.poll(2).unwrap().is_some());
        assert_eq!(runtime.poll(2), Err(TypedError::PeerLost));
        assert!(runtime.poll(3).unwrap().is_some());
    }
}
