//! Reference compilation contract (directive §1 pipeline; ADR-AVC-001 F1/F2).
//!
//! The canonical compilation sequence as executable contract: decode →
//! working-space conversion → colour closure → row decomposition → row
//! deduplication → temporal optimisation (differential events over
//! persistent state) → checkpointing → `PRESENT`. This module defines the
//! contract types and one deterministic reference implementation; the
//! production compiler may be smarter (region ops, delta chains, trim
//! compilation) but must obey the same classes.
//!
//! Phase-1 proof-of-concept scope (ADR-AVC-001 decision 8): fill-only,
//! trimless, exact pixel states — the screen-recording case, where
//! boundaries are hard and the colour inventory closes trivially. Trim
//! and light compilation paths are contract-present
//! ([`crate::instructions::Instruction::SetTrim`],
//! [`SetLight`](crate::instructions::Instruction::SetLight)) but are not
//! exercised by the reference compiler.

use std::collections::HashMap;

use crate::clock::MediaClock;
use crate::colour::{ClosureVerdict, ColourEntry, ColourInventory, RejectionDiagnostic};
use crate::ingestion::{srgb8_to_colour_entry, IngestionParameters, SourceError, VideoSource};
use crate::instructions::{Instruction, InstructionBudget, Target};
use crate::pixel::{ColourIndex, PixelState};
use crate::profile::{CompilationProfile, ProfileError};
use crate::temporal::{Checkpoint, EventId, TemporalStateId, Timeline, TimelineError};

/// Identifier of a deduplicated row in the row inventory (directive §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// The source failed colour-inventory closure: compilation fails
    /// closed and routes the source to a conventional codec (F2, AVC-C12).
    ClosureRejected(RejectionDiagnostic),
    /// The source delivered an unusable frame.
    Source(SourceError),
    /// The profile is internally inconsistent (F9).
    Profile(ProfileError),
    /// The compiled program violates an instruction budget (AVC-C10).
    Budget(crate::instructions::ProgramError),
    /// The timeline schedule is invalid.
    Timeline(TimelineError),
    /// The source contains no frames.
    EmptySource,
}

/// The compiled video contract: canonical colour inventory, row
/// inventory, and the executable visual timeline (directive §6/§7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledVideo {
    pub source_digest: String,
    pub inventory: ColourInventory,
    /// Deduplicated rows in first-use order; `timeline` events reference
    /// rows by index into this vector's *deduplication*, addressed
    /// positionally by `SetRow` targets.
    pub row_inventory: Vec<Vec<ColourIndex>>,
    pub timeline: Timeline,
    pub dimensions: (u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileStats {
    pub frames: usize,
    pub distinct_rows: usize,
    pub row_events: usize,
    pub checkpoints: usize,
    /// Naïve cost of re-declaring every pixel of every frame (the §5
    /// baseline), for benchmark comparison.
    pub naive_pixel_writes: u64,
    /// Pixel writes the persistent-state model actually issues.
    pub persistent_pixel_writes: u64,
}

/// Compile one source under one explicit profile. Deterministic: the same
/// source, parameters and profile produce a byte-identical timeline.
pub fn compile(
    source: &dyn VideoSource,
    params: &IngestionParameters,
    profile: &CompilationProfile,
) -> Result<(CompiledVideo, CompileStats), CompileError> {
    profile.validate().map_err(CompileError::Profile)?;
    let (width, height) = source.dimensions();
    let frame_count = source.frame_count();
    if frame_count == 0 || width == 0 || height == 0 {
        return Err(CompileError::EmptySource);
    }

    // 1–2. Decode every frame and convert to canonical colour entries.
    let mut frames: Vec<Vec<ColourEntry>> = Vec::with_capacity(frame_count as usize);
    for i in 0..frame_count {
        let frame = source
            .decode_frame(i, params)
            .map_err(CompileError::Source)?;
        let entries: Vec<ColourEntry> = frame
            .rgb_pixels()
            .map_err(CompileError::Source)?
            .map(srgb8_to_colour_entry)
            .collect();
        frames.push(entries);
    }

    // 3. Colour closure (F2, AVC-C12): fail closed with a diagnostic.
    let mut candidates: Vec<ColourEntry> = Vec::new();
    for entries in &frames {
        for entry in entries {
            if !candidates.contains(entry) {
                candidates.push(*entry);
            }
        }
    }
    match ColourInventory::close(
        &candidates,
        profile.budgets.palette_bound,
        profile.budgets.tolerance,
    ) {
        ClosureVerdict::Closed { .. } => {}
        ClosureVerdict::Rejected(d) => return Err(CompileError::ClosureRejected(d)),
    }
    let inventory = ColourInventory::new(
        &candidates,
        profile.budgets.palette_bound,
        profile.budgets.tolerance,
        params.working_space,
    )
    .map_err(|_| {
        CompileError::ClosureRejected(RejectionDiagnostic {
            observed_distinct_colours: candidates.len() as u64,
            bound: profile.budgets.palette_bound,
            remediation: "inventory construction exceeded its own closure verdict (contract bug)",
        })
    })?;
    let mut colour_ids: HashMap<ColourEntry, ColourIndex> = HashMap::new();
    for (i, entry) in inventory.entries().iter().enumerate() {
        colour_ids.insert(*entry, ColourIndex(i as u16));
    }

    // 4–5. Row decomposition and deduplication, frame by frame.
    let mut row_ids: HashMap<Vec<ColourIndex>, u32> = HashMap::new();
    let mut row_inventory: Vec<Vec<ColourIndex>> = Vec::new();
    let mut frame_rows: Vec<Vec<u32>> = Vec::with_capacity(frames.len());
    for entries in &frames {
        let mut rows = Vec::with_capacity(height as usize);
        for y in 0..height as usize {
            let row: Vec<ColourIndex> = entries[y * width as usize..(y + 1) * width as usize]
                .iter()
                .map(|e| colour_ids[e])
                .collect();
            let next_id = row_inventory.len() as u32;
            let id = *row_ids.entry(row.clone()).or_insert(next_id);
            if id == next_id {
                row_inventory.push(row);
            }
            rows.push(id);
        }
        frame_rows.push(rows);
    }

    // 6. Temporal optimisation: persistent state, differential events.
    let (rate_num, rate_den) = source.frame_rate();
    let frame_duration = MediaClock::frame_duration(rate_num, rate_den, params.ticks_per_second)
        .map_err(|_| CompileError::EmptySource)?;
    let mut timeline = Timeline::new();
    let mut event_id = 0u32;
    let mut checkpoints = 0usize;
    let mut last_checkpoint_tick = 0u64;
    let mut persistent_writes = 0u64;

    let emit = |timeline: &mut Timeline,
                event_id: &mut u32,
                at: MediaClock,
                instruction: Instruction|
     -> Result<(), CompileError> {
        timeline
            .push(EventId(*event_id), at, instruction)
            .map_err(CompileError::Timeline)?;
        *event_id += 1;
        Ok(())
    };

    let mut previous: Option<&Vec<u32>> = None;
    for (frame_index, rows) in frame_rows.iter().enumerate() {
        // Frame positions start at tick 0 (F5): frame i sits at
        // i * frame_duration exactly.
        let at = MediaClock::new(
            frame_duration.ticks() * frame_index as u64,
            params.ticks_per_second,
        )
        .map_err(|_| CompileError::EmptySource)?;

        // Checkpoint at or beyond the interval: the seek entry point.
        let since_checkpoint = at.ticks() - last_checkpoint_tick;
        if frame_index > 0 && since_checkpoint >= profile.budgets.max_checkpoint_interval.ticks() {
            let payload = checkpoint_payload(rows);
            timeline
                .push_checkpoint(Checkpoint::new(
                    TemporalStateId(frame_index as u64),
                    at,
                    &payload,
                ))
                .map_err(CompileError::Timeline)?;
            checkpoints += 1;
            last_checkpoint_tick = at.ticks();
        }

        match previous {
            None => {
                // Initial state: every row declared once (the frame-1 write set).
                for (y, row_id) in rows.iter().enumerate() {
                    emit(
                        &mut timeline,
                        &mut event_id,
                        at,
                        Instruction::SetRow {
                            target: Target::Row { row: y as u32 },
                            states: fill_states(&row_inventory[*row_id as usize]),
                        },
                    )?;
                    persistent_writes += width as u64;
                }
            }
            Some(prev) => {
                // Differential: only rows whose content changed are re-declared.
                for (y, row_id) in rows.iter().enumerate() {
                    if prev[y] != *row_id {
                        emit(
                            &mut timeline,
                            &mut event_id,
                            at,
                            Instruction::SetRow {
                                target: Target::Row { row: y as u32 },
                                states: fill_states(&row_inventory[*row_id as usize]),
                            },
                        )?;
                        persistent_writes += width as u64;
                    }
                }
            }
        }
        previous = Some(rows);
    }

    // `PRESENT`: atomically present the completed visual state.
    let last_tick = frame_duration.ticks() * (frame_count as u64 - 1);
    let at = MediaClock::new(last_tick, params.ticks_per_second)
        .map_err(|_| CompileError::EmptySource)?;
    emit(&mut timeline, &mut event_id, at, Instruction::Present)?;

    // Budget validation before any emission (AVC-C10).
    let budget = InstructionBudget {
        max_events: frame_count as usize * (height as usize + 1) + 1,
        max_delta_applications: 0,
    };
    timeline.validate(&budget).map_err(CompileError::Budget)?;

    let stats = CompileStats {
        frames: frames.len(),
        distinct_rows: row_inventory.len(),
        row_events: event_id as usize,
        checkpoints,
        naive_pixel_writes: frames.len() as u64 * u64::from(width) * u64::from(height),
        persistent_pixel_writes: persistent_writes,
    };

    Ok((
        CompiledVideo {
            source_digest: source.source_digest(),
            inventory,
            row_inventory,
            timeline,
            dimensions: (width, height),
        },
        stats,
    ))
}

fn fill_states(row: &[ColourIndex]) -> Vec<PixelState> {
    row.iter().map(|&c| PixelState::exact_fill(c)).collect()
}

fn checkpoint_payload(rows: &[u32]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(rows.len() * 4);
    for row in rows {
        payload.extend_from_slice(&row.to_le_bytes());
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::{PaletteBound, Tolerance, WorkingSpace};
    use crate::ingestion::{DecodedFrame, DecodedFrameFormat};
    use crate::profile::{CompilationBudgets, ProfileKind};

    /// A deterministic synthetic screen recording: a small desktop with
    /// two windows and a moving cursor — sparse changes over persistent
    /// regions (directive §10, screen recordings).
    struct ScreenRecording {
        frames: Vec<Vec<[u8; 3]>>,
        width: u32,
        height: u32,
    }

    const BG: [u8; 3] = [240, 240, 240];
    const WINDOW: [u8; 3] = [16, 24, 200];
    const CURSOR: [u8; 3] = [200, 40, 40];

    impl ScreenRecording {
        fn new() -> Self {
            let (w, h) = (8u32, 4u32);
            let blank: Vec<[u8; 3]> = vec![BG; (w * h) as usize];
            // Window occupies the left half of the middle two rows.
            let with_window = |f: &mut Vec<[u8; 3]>| {
                for y in 1..3 {
                    for x in 0..4 {
                        f[y * w as usize + x] = WINDOW;
                    }
                }
            };
            let mut f0 = blank.clone();
            with_window(&mut f0);
            let mut f1 = f0.clone();
            f1[w as usize + 6] = CURSOR;
            let mut f2 = f0.clone();
            f2[2 * w as usize + 6] = CURSOR;
            Self {
                frames: vec![f0, f1, f2],
                width: w,
                height: h,
            }
        }

        fn with_excess_colours() -> Self {
            let mut frames = Vec::new();
            let mut f = Vec::new();
            // Widely separated greys so the Q24.8 quantisation does not
            // collapse near-black levels into one another.
            for i in 0..20u8 {
                f.push([i * 13 + 6, 0, 0]);
            }
            frames.push(f);
            Self {
                frames,
                width: 20,
                height: 1,
            }
        }
    }

    impl VideoSource for ScreenRecording {
        fn source_digest(&self) -> String {
            format!(
                "screen-recording-{}x{}-{}",
                self.width,
                self.height,
                self.frames.len()
            )
        }
        fn dimensions(&self) -> (u32, u32) {
            (self.width, self.height)
        }
        fn frame_rate(&self) -> (u32, u32) {
            (30, 1)
        }
        fn frame_count(&self) -> u32 {
            self.frames.len() as u32
        }
        fn decode_frame(
            &self,
            i: u32,
            params: &IngestionParameters,
        ) -> Result<DecodedFrame, SourceError> {
            let fd = MediaClock::frame_duration(30, 1, params.ticks_per_second).unwrap();
            let at = MediaClock::new(fd.ticks() * i as u64, params.ticks_per_second).unwrap();
            let mut data = Vec::new();
            for px in &self.frames[i as usize] {
                data.extend_from_slice(px.as_slice());
            }
            Ok(DecodedFrame {
                format: DecodedFrameFormat::Rgb8,
                width: self.width,
                height: self.height,
                at,
                data,
            })
        }
    }

    fn params() -> IngestionParameters {
        IngestionParameters {
            working_space: WorkingSpace::LinearRec709,
            ticks_per_second: 30,
        }
        .bind()
        .unwrap()
    }

    fn profile(bound: PaletteBound) -> CompilationProfile {
        CompilationProfile {
            kind: ProfileKind::Desktop,
            budgets: CompilationBudgets {
                palette_bound: bound,
                tolerance: Tolerance::ZERO,
                max_checkpoint_interval: MediaClock::new(90, 30).unwrap(), // 3 s at 30 tps
                max_delta_depth: 8,
                residency_budget_bytes: 1 << 20,
            },
        }
    }

    #[test]
    fn screen_recording_compiles_with_persistent_state_advantage() {
        let source = ScreenRecording::new();
        let (video, stats) = compile(&source, &params(), &profile(PaletteBound(16))).unwrap();

        // Closure: three distinct colours.
        assert_eq!(video.inventory.entries().len(), 3);
        // Temporal persistence: fewer writes than the naive baseline.
        assert_eq!(stats.naive_pixel_writes, 3 * 8 * 4);
        assert!(
            stats.persistent_pixel_writes < stats.naive_pixel_writes,
            "persistent {} vs naive {}",
            stats.persistent_pixel_writes,
            stats.naive_pixel_writes
        );
        // Frames 1 and 2 move the cursor: frame 1 changes one row, frame 2
        // changes two rows (the cursor leaves row 1 and enters row 2),
        // plus PRESENT.
        assert_eq!(
            stats.row_events,
            4 /* init rows */ + 3 /* differential rows */ + 1 /* PRESENT */
        );
        assert_eq!(stats.persistent_pixel_writes, 32 + 8 + 16);
        // Timeline is well-formed and validated.
        assert!(video
            .timeline
            .validate(&InstructionBudget {
                max_events: 100,
                max_delta_applications: 0,
            })
            .is_ok());
    }

    #[test]
    fn compilation_is_deterministic() {
        let source = ScreenRecording::new();
        let (a, sa) = compile(&source, &params(), &profile(PaletteBound(16))).unwrap();
        let (b, sb) = compile(&source, &params(), &profile(PaletteBound(16))).unwrap();
        assert_eq!(a.timeline, b.timeline);
        assert_eq!(a.row_inventory, b.row_inventory);
        assert_eq!(a.source_digest, b.source_digest);
        assert_eq!(sa, sb);
    }

    #[test]
    fn excess_colours_fail_closed_with_diagnostic() {
        let source = ScreenRecording::with_excess_colours();
        match compile(&source, &params(), &profile(PaletteBound(8))) {
            Err(CompileError::ClosureRejected(d)) => {
                assert_eq!(d.bound, PaletteBound(8));
                assert!(d.observed_distinct_colours > 8);
                assert!(d.remediation.contains("conventional codec"));
            }
            other => panic!("expected closure rejection, got {other:?}"),
        }
    }

    #[test]
    fn empty_sources_are_rejected() {
        struct Empty;
        impl VideoSource for Empty {
            fn source_digest(&self) -> String {
                "empty".into()
            }
            fn dimensions(&self) -> (u32, u32) {
                (8, 4)
            }
            fn frame_rate(&self) -> (u32, u32) {
                (30, 1)
            }
            fn frame_count(&self) -> u32 {
                0
            }
            fn decode_frame(
                &self,
                _i: u32,
                _p: &IngestionParameters,
            ) -> Result<DecodedFrame, SourceError> {
                unreachable!()
            }
        }
        assert_eq!(
            compile(&Empty, &params(), &profile(PaletteBound(16))),
            Err(CompileError::EmptySource)
        );
    }

    #[test]
    fn checkpoint_interval_bounds_seek_distance() {
        let source = ScreenRecording::new();
        let (video, stats) = compile(&source, &params(), &profile(PaletteBound(16))).unwrap();
        // 3 frames at 30 tps → ticks 0, 1, 2; interval 90 ticks never
        // fires, so reconstruction starts from the initial state.
        assert_eq!(stats.checkpoints, 0);
        assert!(video
            .timeline
            .seek_checkpoint(MediaClock::new(2, 30).unwrap())
            .is_none());

        // A one-tick interval checkpoints every differential frame.
        let tight = CompilationProfile {
            kind: ProfileKind::Interactive,
            budgets: CompilationBudgets {
                max_checkpoint_interval: MediaClock::new(1, 30).unwrap(),
                ..profile(PaletteBound(16)).budgets
            },
        };
        let (video, stats) = compile(&source, &params(), &tight).unwrap();
        assert_eq!(stats.checkpoints, 2); // frames 1 and 2
        let seek = video
            .timeline
            .seek_checkpoint(MediaClock::new(2, 30).unwrap())
            .unwrap();
        assert_eq!(seek.state, TemporalStateId(2));
    }
}
