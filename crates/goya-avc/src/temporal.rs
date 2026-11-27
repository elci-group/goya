//! Temporal model (directive §3; ADR-AVC-001 F5, F7).
//!
//! Pixels retain their previous state until explicitly instructed to
//! change: the timeline is a program of scheduled mutations against a
//! persistent visual state, punctuated by checkpoints (`LOAD_STATE`)
//! that bound reconstruction depth. All positions are fixed-point media
//! clock values; events at equal ticks preserve insertion order (frame
//! granularity), and the program rejects out-of-order schedules.

use crate::clock::MediaClock;
use crate::instructions::{Instruction, ProgramError};
use sha2::{Digest, Sha256};

/// Identifier of a temporal visual state (F7: never shares a namespace
/// with GVI variation states or AVR observation states).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TemporalStateId(pub u64);

/// Identifier of a scheduled event within a compiled timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventId(pub u32);

/// A `LOAD_STATE` checkpoint (directive §4): a complete visual state a
/// runtime can restore, bounding seek and reconstruction cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub state: TemporalStateId,
    pub at: MediaClock,
    /// SHA-256 of the canonical checkpoint payload (digest fixed at
    /// construction so certification cannot be inferred after the fact).
    pub digest: [u8; 32],
}

impl Checkpoint {
    pub fn new(state: TemporalStateId, at: MediaClock, payload: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(state.0.to_le_bytes());
        hasher.update(at.ticks().to_le_bytes());
        hasher.update(payload);
        Self {
            state,
            at,
            digest: hasher.finalize().into(),
        }
    }
}

/// A mutation scheduled on the media clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub id: EventId,
    pub at: MediaClock,
    pub instruction: Instruction,
}

/// A compiled visual timeline: the deterministic program of mutations,
/// checkpoints and media-clock positions (directive §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    events: Vec<ScheduledEvent>,
    checkpoints: Vec<Checkpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineError {
    /// Events must be scheduled in non-decreasing tick order.
    NonMonotonicSchedule { at_tick: u64, after_tick: u64 },
    /// A checkpoint references a state id already bound to another
    /// checkpoint: one checkpoint per temporal state.
    DuplicateCheckpointState { state: u64 },
    /// Checkpoints must also follow the media clock.
    NonMonotonicCheckpoint { at_tick: u64, after_tick: u64 },
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            checkpoints: Vec::new(),
        }
    }

    /// Schedule one mutation. Ticks must be non-decreasing; equal ticks
    /// preserve insertion order (AVC-C04).
    pub fn push(
        &mut self,
        id: EventId,
        at: MediaClock,
        instruction: Instruction,
    ) -> Result<(), TimelineError> {
        if let Some(last) = self.events.last() {
            if at.ticks() < last.at.ticks() {
                return Err(TimelineError::NonMonotonicSchedule {
                    at_tick: at.ticks(),
                    after_tick: last.at.ticks(),
                });
            }
        }
        self.events.push(ScheduledEvent {
            id,
            at,
            instruction,
        });
        Ok(())
    }

    /// Record a checkpoint. One checkpoint per temporal state, and the
    /// checkpoint schedule follows the media clock too.
    pub fn push_checkpoint(&mut self, checkpoint: Checkpoint) -> Result<(), TimelineError> {
        if let Some(last) = self.checkpoints.last() {
            if checkpoint.at.ticks() < last.at.ticks() {
                return Err(TimelineError::NonMonotonicCheckpoint {
                    at_tick: checkpoint.at.ticks(),
                    after_tick: last.at.ticks(),
                });
            }
        }
        if self.checkpoints.iter().any(|c| c.state == checkpoint.state) {
            return Err(TimelineError::DuplicateCheckpointState {
                state: checkpoint.state.0,
            });
        }
        self.checkpoints.push(checkpoint);
        Ok(())
    }

    pub fn events(&self) -> &[ScheduledEvent] {
        &self.events
    }

    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// The last checkpoint at or before `at` — the seek entry point
    /// (directive §8: random access via state checkpoints). Reconstruction
    /// depth is bounded by the distance to it, not to time zero.
    pub fn seek_checkpoint(&self, at: MediaClock) -> Option<&Checkpoint> {
        self.checkpoints
            .iter()
            .rev()
            .find(|c| c.at.ticks() <= at.ticks())
    }

    /// Validate against an instruction program budget (AVC-C10).
    pub fn validate(
        &self,
        budget: &crate::instructions::InstructionBudget,
    ) -> Result<(), ProgramError> {
        if self.events.len() > budget.max_events {
            return Err(ProgramError::EventBudgetExceeded {
                events: self.events.len(),
                budget: budget.max_events,
            });
        }
        let mut deltas = 0usize;
        for event in &self.events {
            if matches!(event.instruction, Instruction::ApplyDelta { .. }) {
                deltas += 1;
            }
        }
        if deltas > budget.max_delta_applications {
            return Err(ProgramError::DeltaBudgetExceeded {
                applications: deltas,
                budget: budget.max_delta_applications,
            });
        }
        Ok(())
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instructions::{Instruction, InstructionBudget, ProgramError, Target};
    use crate::pixel::{ColourIndex, PixelState};

    #[test]
    fn events_preserve_monotonic_order_and_insertion_order() {
        let t0 = MediaClock::new(0, 30_000).unwrap();
        let t1 = MediaClock::new(1_001, 30_000).unwrap();
        let mut tl = Timeline::new();
        let ev = Instruction::SetFill {
            target: Target::Pixel { row: 1, pixel: 3 },
            colour: ColourIndex(0),
        };
        tl.push(EventId(0), t0, ev.clone()).unwrap();
        tl.push(EventId(1), t0, ev.clone()).unwrap();
        tl.push(EventId(2), t1, ev).unwrap();
        assert_eq!(tl.events().len(), 3);
        assert_eq!(tl.events()[0].id, EventId(0));
        assert_eq!(tl.events()[1].id, EventId(1));
    }

    #[test]
    fn out_of_order_schedule_is_rejected() {
        let t1 = MediaClock::new(2, 30).unwrap();
        let t0 = MediaClock::new(1, 30).unwrap();
        let mut tl = Timeline::new();
        let ev = Instruction::Sync;
        tl.push(EventId(0), t1, ev.clone()).unwrap();
        assert!(matches!(
            tl.push(EventId(1), t0, ev),
            Err(TimelineError::NonMonotonicSchedule { .. })
        ));
    }

    #[test]
    fn checkpoints_bound_seek_reconstruction_and_reject_duplicates() {
        let t0 = MediaClock::new(0, 30).unwrap();
        let t1 = MediaClock::new(90, 30).unwrap();
        let t2 = MediaClock::new(180, 30).unwrap();
        let mut tl = Timeline::new();
        tl.push_checkpoint(Checkpoint::new(TemporalStateId(1), t0, b"genesis"))
            .unwrap();
        tl.push_checkpoint(Checkpoint::new(TemporalStateId(2), t1, b"mid"))
            .unwrap();
        assert!(matches!(
            tl.push_checkpoint(Checkpoint::new(TemporalStateId(1), t2, b"dup")),
            Err(TimelineError::DuplicateCheckpointState { state: 1 })
        ));
        // Seek just before t2 lands on the t1 checkpoint.
        let seek = tl
            .seek_checkpoint(MediaClock::new(179, 30).unwrap())
            .unwrap();
        assert_eq!(seek.state, TemporalStateId(2));
        // Checkpoint digests are payload-sensitive.
        assert_ne!(
            Checkpoint::new(TemporalStateId(1), t0, b"a").digest,
            Checkpoint::new(TemporalStateId(1), t0, b"b").digest
        );
    }

    #[test]
    fn program_budgets_are_enforced() {
        let t0 = MediaClock::new(0, 30).unwrap();
        let mut tl = Timeline::new();
        let ev = Instruction::SetRow {
            target: Target::Row { row: 0 },
            states: vec![PixelState::exact_fill(ColourIndex(1))],
        };
        tl.push(EventId(0), t0, ev).unwrap();
        let budget = InstructionBudget {
            max_events: 1,
            max_delta_applications: 0,
        };
        assert!(tl.validate(&budget).is_ok());
        let tight = InstructionBudget {
            max_events: 0,
            max_delta_applications: 0,
        };
        assert_eq!(
            tl.validate(&tight),
            Err(ProgramError::EventBudgetExceeded {
                events: 1,
                budget: 0
            })
        );
    }
}
