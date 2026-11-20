//! Small local-only, bounded executor for registered Iberia graphs.
//!
//! The runtime deliberately owns scheduling and deadlines locally. It adds no
//! wire fields or changes to the frozen contract.

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
            max_pending: 64,
            max_retained: 128,
            max_payload_bytes_per_job: 64 * 1024 * 1024,
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
    graph_digest: [u8; 32],
    inputs: Vec<TensorData>,
    submitted_at: Instant,
    deadline: Instant,
}

/// Bounded FIFO runtime. Calls to `run_one` execute synchronously on the
/// caller's thread; cancellation therefore removes queued work and records a
/// terminal outcome, while an executing graph checks its deadline checkpoints.
pub struct Runtime {
    limits: Limits,
    models: Vec<( [u8; 32], Graph)>,
    pending: VecDeque<Job>,
    completed: VecDeque<(u64, Completion)>,
    evicted: VecDeque<u64>,
    next_id: Option<u64>,
}

impl Runtime {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            models: Vec::new(),
            pending: VecDeque::new(),
            completed: VecDeque::new(),
            evicted: VecDeque::new(),
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
        let Some((_, graph)) = self.models.iter().find(|(known, _)| *known == digest) else {
            return Err(TypedError::ModelMissing { digest: hex(&digest) });
        };
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
        let id = self.next_id.ok_or_else(|| exhausted("request id space exhausted"))?;
        self.next_id = id.checked_add(1);
        let submitted_at = Instant::now();
        let deadline = submitted_at.checked_add(timeout).unwrap_or(submitted_at);
        self.pending.push_back(Job {
            id,
            graph_digest: digest,
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
            let graph = self
                .models
                .iter()
                .find(|(digest, _)| *digest == job.graph_digest)
                .map(|(_, graph)| graph);
            match graph {
                Some(graph) => graph.execute(job.inputs, || {
                    if Instant::now() >= job.deadline {
                        Err(TypedError::DeadlineExceeded)
                    } else {
                        Ok(())
                    }
                }),
                None => Err(TypedError::ModelMissing { digest: hex(&job.graph_digest) }),
            }
        };
        self.retain(id, Completion {
            result,
            queue_time,
            execution_time: execution_started.elapsed(),
        });
        Some(id)
    }

    /// Return `None` while pending; terminal completions are consumed once.
    pub fn poll(&mut self, id: u64) -> Result<Option<Completion>, TypedError> {
        if self.pending.iter().any(|job| job.id == id) {
            return Ok(None);
        }
        if let Some(index) = self.completed.iter().position(|(known, _)| *known == id) {
            let (_, completion) = self.completed.remove(index).expect("index was found");
            self.remember_evicted(id);
            return Ok(Some(completion));
        }
        Err(TypedError::PeerLost)
    }

    /// Cancel queued work and preserve a pollable terminal cancellation.
    pub fn cancel(&mut self, id: u64) -> Result<(), TypedError> {
        if let Some(index) = self.pending.iter().position(|job| job.id == id) {
            let job = self.pending.remove(index).expect("index was found");
            self.retain(id, Completion {
                result: Err(TypedError::Cancelled),
                queue_time: job.submitted_at.elapsed(),
                execution_time: Duration::ZERO,
            });
            return Ok(());
        }
        if self.completed.iter().any(|(known, _)| *known == id) {
            return Ok(());
        }
        Err(TypedError::PeerLost)
    }

    fn retain(&mut self, id: u64, completion: Completion) {
        if self.limits.max_retained == 0 {
            self.remember_evicted(id);
            return;
        }
        self.completed.push_back((id, completion));
        while self.completed.len() > self.limits.max_retained {
            if let Some((evicted, _)) = self.completed.pop_front() {
                self.remember_evicted(evicted);
            }
        }
    }

    fn remember_evicted(&mut self, id: u64) {
        // Keep the unknown-outcome marker bounded independently of results.
        let marker_limit = self.limits.max_retained.max(1).saturating_mul(2);
        self.evicted.push_back(id);
        while self.evicted.len() > marker_limit {
            self.evicted.pop_front();
        }
    }
}

fn exhausted(reason: &str) -> TypedError {
    TypedError::ResourceExhausted { reason: reason.into() }
}

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
