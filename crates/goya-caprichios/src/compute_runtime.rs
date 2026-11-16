//! Small, synchronous compute primitives with explicit capacity and retry bounds.
//!
//! This module implements only a subset of the compute catalog: bounded queue
//! admission, caller-directed bounded retries, and monotonic progress tracking.
//! It does not provide a thread pool, preemptive cancellation, or hard deadlines.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueError {
    ZeroCapacity,
    Full,
    Poisoned,
}

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroCapacity => "queue capacity must be positive",
            Self::Full => "queue is full",
            Self::Poisoned => "queue lock was poisoned",
        })
    }
}

impl std::error::Error for QueueError {}

/// A FIFO queue whose accepted item count never exceeds its fixed capacity.
/// `try_push` rejects without retaining an item when full; this type does not
/// block producers or create worker threads.
pub struct BoundedQueue<T> {
    capacity: usize,
    items: Mutex<VecDeque<T>>,
}

impl<T> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Result<Self, QueueError> {
        if capacity == 0 {
            return Err(QueueError::ZeroCapacity);
        }
        Ok(Self {
            capacity,
            items: Mutex::new(VecDeque::with_capacity(capacity)),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, VecDeque<T>>, QueueError> {
        self.items.lock().map_err(|_| QueueError::Poisoned)
    }

    pub fn try_push(&self, item: T) -> Result<(), QueueError> {
        let mut items = self.lock()?;
        if items.len() == self.capacity {
            return Err(QueueError::Full);
        }
        items.push_back(item);
        Ok(())
    }

    pub fn try_pop(&self) -> Result<Option<T>, QueueError> {
        Ok(self.lock()?.pop_front())
    }

    pub fn len(&self) -> Result<usize, QueueError> {
        Ok(self.lock()?.len())
    }

    pub fn is_empty(&self) -> Result<bool, QueueError> {
        Ok(self.lock()?.is_empty())
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Retry count and delay bound for a synchronous operation. The first call is
/// always made; `max_attempts` includes it. Retryability is decided by the
/// caller, and delays use a capped exponential schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryPolicyError {
    ZeroAttempts,
    InitialDelayExceedsMaximum,
}

impl RetryPolicy {
    pub fn new(
        max_attempts: u32,
        initial_delay: Duration,
        max_delay: Duration,
    ) -> Result<Self, RetryPolicyError> {
        if max_attempts == 0 {
            return Err(RetryPolicyError::ZeroAttempts);
        }
        if initial_delay > max_delay {
            return Err(RetryPolicyError::InitialDelayExceedsMaximum);
        }
        Ok(Self {
            max_attempts,
            initial_delay,
            max_delay,
        })
    }

    pub fn max_attempts(self) -> u32 {
        self.max_attempts
    }

    fn delay_after(self, failed_attempt: u32) -> Duration {
        if self.initial_delay.is_zero() || self.max_delay.is_zero() {
            return Duration::ZERO;
        }
        let mut delay = self.initial_delay;
        let mut remaining_doublings = failed_attempt.saturating_sub(1);
        while remaining_doublings > 0 && delay < self.max_delay {
            delay = delay.saturating_mul(2).min(self.max_delay);
            remaining_doublings -= 1;
        }
        delay
    }

    /// Invoke `operation` until it succeeds, fails permanently, or reaches the
    /// attempt bound. The supplied sleeper can be `std::thread::sleep`; making
    /// it injectable keeps scheduling policy testable without real delays.
    pub fn run<T, E, O, R, S>(
        self,
        mut operation: O,
        mut retryable: R,
        mut sleep: S,
    ) -> Result<T, E>
    where
        O: FnMut(u32) -> Result<T, E>,
        R: FnMut(&E) -> bool,
        S: FnMut(Duration),
    {
        for attempt in 1..=self.max_attempts {
            match operation(attempt) {
                Ok(value) => return Ok(value),
                Err(error) if attempt < self.max_attempts && retryable(&error) => {
                    let delay = self.delay_after(attempt);
                    if !delay.is_zero() {
                        sleep(delay);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("positive max_attempts is enforced by constructor")
    }
}

/// Monotonic work counter for tasks with a fixed, nonzero total.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Progress {
    total: u64,
    completed: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressError {
    ZeroTotal,
    Decreased,
    ExceedsTotal,
}

impl Progress {
    pub fn new(total: u64) -> Result<Self, ProgressError> {
        if total == 0 {
            return Err(ProgressError::ZeroTotal);
        }
        Ok(Self {
            total,
            completed: 0,
        })
    }

    pub fn advance_to(&mut self, completed: u64) -> Result<(), ProgressError> {
        if completed < self.completed {
            return Err(ProgressError::Decreased);
        }
        if completed > self.total {
            return Err(ProgressError::ExceedsTotal);
        }
        self.completed = completed;
        Ok(())
    }

    pub fn completed(self) -> u64 {
        self.completed
    }

    pub fn total(self) -> u64 {
        self.total
    }

    pub fn is_complete(self) -> bool {
        self.completed == self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn queue_enforces_capacity_and_fifo() {
        let queue = BoundedQueue::new(2).unwrap();
        queue.try_push("a").unwrap();
        queue.try_push("b").unwrap();
        assert_eq!(queue.try_push("c"), Err(QueueError::Full));
        assert_eq!(queue.len().unwrap(), 2);
        assert_eq!(queue.try_pop().unwrap(), Some("a"));
        queue.try_push("c").unwrap();
        assert_eq!(queue.try_pop().unwrap(), Some("b"));
        assert_eq!(queue.try_pop().unwrap(), Some("c"));
        assert_eq!(queue.try_pop().unwrap(), None);
    }

    #[test]
    fn queue_is_safe_for_concurrent_admission_and_removal() {
        let queue = Arc::new(BoundedQueue::new(16).unwrap());
        let producer_queue = Arc::clone(&queue);
        let producer = thread::spawn(move || {
            for value in 0..2_000 {
                loop {
                    match producer_queue.try_push(value) {
                        Ok(()) => break,
                        Err(QueueError::Full) => thread::yield_now(),
                        other => panic!("unexpected queue result: {other:?}"),
                    }
                }
            }
        });
        let mut received = Vec::new();
        while received.len() < 2_000 {
            if let Some(value) = queue.try_pop().unwrap() {
                received.push(value);
            } else {
                thread::yield_now();
            }
        }
        producer.join().unwrap();
        assert_eq!(received, (0..2_000).collect::<Vec<_>>());
    }

    #[test]
    fn bounded_queue_delivers_unique_items_once_with_multiple_producers_and_consumers() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        const PRODUCERS: usize = 4;
        const CONSUMERS: usize = 3;
        const ITEMS_PER_PRODUCER: usize = 1_000;
        const TOTAL_ITEMS: usize = PRODUCERS * ITEMS_PER_PRODUCER;

        let queue = Arc::new(BoundedQueue::new(11).unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let finished_producers = Arc::new(AtomicUsize::new(0));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);

        let mut producers = Vec::new();
        for producer_id in 0..PRODUCERS {
            let queue = Arc::clone(&queue);
            let stop = Arc::clone(&stop);
            let finished = Arc::clone(&finished_producers);
            producers.push(thread::spawn(move || {
                for local_id in 0..ITEMS_PER_PRODUCER {
                    let item = producer_id * ITEMS_PER_PRODUCER + local_id;
                    loop {
                        if stop.load(Ordering::Relaxed) {
                            finished.fetch_add(1, Ordering::Release);
                            return false;
                        }
                        match queue.try_push(item) {
                            Ok(()) => {
                                // Observe queue depth under the same lock used
                                // by admission and assert the configured bound.
                                if queue.len().unwrap() > queue.capacity() {
                                    stop.store(true, Ordering::Relaxed);
                                    finished.fetch_add(1, Ordering::Release);
                                    return false;
                                }
                                break;
                            }
                            Err(QueueError::Full) => {
                                if std::time::Instant::now() >= deadline {
                                    stop.store(true, Ordering::Relaxed);
                                    finished.fetch_add(1, Ordering::Release);
                                    return false;
                                }
                                thread::yield_now();
                            }
                            Err(_) => {
                                stop.store(true, Ordering::Relaxed);
                                finished.fetch_add(1, Ordering::Release);
                                return false;
                            }
                        }
                    }
                }
                finished.fetch_add(1, Ordering::Release);
                true
            }));
        }

        let mut consumers = Vec::new();
        for _ in 0..CONSUMERS {
            let queue = Arc::clone(&queue);
            let stop = Arc::clone(&stop);
            let finished = Arc::clone(&finished_producers);
            consumers.push(thread::spawn(move || {
                let mut received = Vec::new();
                let mut max_observed_depth = 0;
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return (received, max_observed_depth, false);
                    }
                    match queue.try_pop() {
                        Ok(Some(item)) => {
                            received.push(item);
                            let depth = queue.len().unwrap();
                            max_observed_depth = max_observed_depth.max(depth);
                            if depth > queue.capacity() {
                                stop.store(true, Ordering::Relaxed);
                                return (received, max_observed_depth, false);
                            }
                        }
                        Ok(None) if finished.load(Ordering::Acquire) == PRODUCERS => {
                            return (received, max_observed_depth, true);
                        }
                        Ok(None) => {
                            if std::time::Instant::now() >= deadline {
                                stop.store(true, Ordering::Relaxed);
                                return (received, max_observed_depth, false);
                            }
                            thread::yield_now();
                        }
                        Err(_) => {
                            stop.store(true, Ordering::Relaxed);
                            return (received, max_observed_depth, false);
                        }
                    }
                }
            }));
        }

        let producer_results: Vec<_> = producers
            .into_iter()
            .map(|producer| producer.join().unwrap())
            .collect();
        let consumer_results: Vec<_> = consumers
            .into_iter()
            .map(|consumer| consumer.join().unwrap())
            .collect();

        assert!(
            producer_results.into_iter().all(|completed| completed),
            "producer failed or exceeded the shared deadline"
        );
        assert!(
            consumer_results.iter().all(|(_, _, completed)| *completed),
            "consumer failed or exceeded the shared deadline"
        );
        assert!(consumer_results
            .iter()
            .all(|(_, max_depth, _)| *max_depth <= queue.capacity()));
        let mut received = Vec::with_capacity(TOTAL_ITEMS);
        for (items, _, _) in consumer_results {
            received.extend(items);
        }
        assert_eq!(received.len(), TOTAL_ITEMS);
        let mut seen = vec![false; TOTAL_ITEMS];
        for item in received {
            assert!(item < TOTAL_ITEMS, "received unexpected item {item}");
            assert!(!seen[item], "received duplicate item {item}");
            seen[item] = true;
        }
        assert!(seen.into_iter().all(|was_seen| was_seen));
        assert!(queue.is_empty().unwrap());
    }

    #[test]
    fn retry_stops_on_success_and_caps_exponential_delay() {
        let policy =
            RetryPolicy::new(4, Duration::from_millis(2), Duration::from_millis(5)).unwrap();
        let mut delays = Vec::new();
        let mut calls = 0;
        let result = policy.run(
            |_| {
                calls += 1;
                if calls < 4 {
                    Err("transient")
                } else {
                    Ok(17)
                }
            },
            |_| true,
            |delay| delays.push(delay),
        );
        assert_eq!(result, Ok(17));
        assert_eq!(calls, 4);
        assert_eq!(
            delays,
            [
                Duration::from_millis(2),
                Duration::from_millis(4),
                Duration::from_millis(5)
            ]
        );
    }

    #[test]
    fn retry_delay_reaches_cap_without_iterating_to_attempt_number() {
        let policy =
            RetryPolicy::new(u32::MAX, Duration::from_nanos(1), Duration::from_millis(5)).unwrap();
        assert_eq!(policy.delay_after(u32::MAX), Duration::from_millis(5));
        assert_eq!(
            RetryPolicy::new(u32::MAX, Duration::ZERO, Duration::MAX)
                .unwrap()
                .delay_after(u32::MAX),
            Duration::ZERO
        );
    }

    #[test]
    fn retry_does_not_repeat_permanent_failure_or_exceed_limit() {
        let policy = RetryPolicy::new(3, Duration::ZERO, Duration::ZERO).unwrap();
        let mut calls = 0;
        let result = policy.run(
            |_| {
                calls += 1;
                Err::<(), _>("failed")
            },
            |_| false,
            |_| panic!("zero delay must not sleep"),
        );
        assert_eq!(result, Err("failed"));
        assert_eq!(calls, 1);
    }

    #[test]
    fn progress_is_monotonic_and_bounded() {
        let mut progress = Progress::new(10).unwrap();
        progress.advance_to(4).unwrap();
        assert_eq!(progress.advance_to(3), Err(ProgressError::Decreased));
        assert_eq!(progress.advance_to(11), Err(ProgressError::ExceedsTotal));
        progress.advance_to(10).unwrap();
        assert!(progress.is_complete());
    }

    /// Run with `rustc --test ... --ignored --nocapture` for a simple local
    /// throughput sample; this is a smoke benchmark, not a stable benchmark suite.
    #[test]
    #[ignore]
    fn benchmark_bounded_queue_round_trip() {
        let queue = BoundedQueue::new(1).unwrap();
        let start = std::time::Instant::now();
        let iterations = 1_000_000;
        for value in 0..iterations {
            queue.try_push(value).unwrap();
            assert_eq!(queue.try_pop().unwrap(), Some(value));
        }
        println!("{iterations} queue round trips in {:?}", start.elapsed());
    }
}
