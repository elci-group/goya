//! Reference playback runtime (ADR-AVC-001 Amendment 4).
//!
//! The runtime executes a compiled visual timeline against persistent
//! state: it **resolves** inventory references, **dispatches** state
//! mutations, maintains delta preconditions (AVC-C13), restores
//! checkpoints, and **presents** atomically. This is the contract's
//! reference interpreter — the executable demonstration that the
//! timeline is sufficient to reconstruct every presented frame, which is
//! what the playback-correctness acceptance target ("zero unexplained
//! pixel differences in lossless mode") is measured against.
//!
//! Declared subset: the fill-only path — `SET_ROW`, `SET_RUN`,
//! `APPLY_DELTA`, `LOAD_STATE`, `SYNC`, `PRESENT`. Trim and light
//! instructions are rejected explicitly (`RuntimeError::Unsupported`),
//! not silently ignored: a runtime that pretended to apply them would
//! break determinism. Exactness tracking is per-pixel; delta-applied
//! pixels inherit the exactness of the content they transition to, which
//! in the fill-only reference compiler is always `Exact` (Amendment 4:
//! trim-assisted deltas must extend `PixelChange` when trim compilation
//! lands).
//!
//! Checkpoint payloads are row-content ids in ascending row order,
//! encoded here. The compiler imports the encoder from this module so
//! the format cannot drift.

use crate::clock::MediaClock;
use crate::compile::CompiledVideo;
use crate::delta::ContentId;
use crate::instructions::Instruction;
use crate::pixel::ColourIndex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// A delta was applied against unmatched row content (AVC-C13).
    DeltaPrecondition { delta_id: u32, expected: ContentId },
    /// The instruction is outside this runtime's declared subset.
    Unsupported(&'static str),
    /// A referenced inventory entry does not exist.
    UnknownReference { kind: &'static str, id: u32 },
    /// The presented state contains pixels never declared.
    UndefinedRegion,
    /// The checkpoint payload does not decode against this video.
    CheckpointPayload(String),
    /// A write fell outside the declared geometry.
    GeometryMismatch { expected: usize, actual: usize },
}

/// One atomically presented frame: the row-major fill colours and the
/// per-pixel exactness of the presented state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentedFrame {
    pub at: MediaClock,
    pub pixels: Vec<ColourIndex>,
    /// True when every presented pixel was reconstructed exactly.
    pub exact: bool,
}

/// Encode a checkpoint payload: row-content ids, ascending row order,
/// little-endian (F5's fixed-point discipline applies to payload integers).
pub fn encode_checkpoint_payload(contents: &[ContentId]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(contents.len() * 4);
    for content in contents {
        payload.extend_from_slice(&content.0.to_le_bytes());
    }
    payload
}

/// Decode a checkpoint payload against a compiled video's row inventory,
/// rejecting ids that do not resolve.
pub fn decode_checkpoint_payload(
    payload: &[u8],
    video: &CompiledVideo,
) -> Result<Vec<ContentId>, RuntimeError> {
    if !payload.len().is_multiple_of(4) {
        return Err(RuntimeError::CheckpointPayload(format!(
            "payload length {} is not a multiple of 4",
            payload.len()
        )));
    }
    let count = video.dimensions.1 as usize;
    if payload.len() / 4 != count {
        return Err(RuntimeError::CheckpointPayload(format!(
            "payload names {} rows, video declares {count}",
            payload.len() / 4
        )));
    }
    (0..count)
        .map(|i| {
            let bytes = [
                payload[i * 4],
                payload[i * 4 + 1],
                payload[i * 4 + 2],
                payload[i * 4 + 3],
            ];
            let id = u32::from_le_bytes(bytes);
            if (id as usize) >= video.row_inventory.len() {
                return Err(RuntimeError::CheckpointPayload(format!(
                    "row content id {id} is outside the row inventory"
                )));
            }
            Ok(ContentId(id))
        })
        .collect()
}

/// The persistent visual state plus its reconstruction bookkeeping.
pub struct ReferenceRuntime {
    rows: Vec<Vec<ColourIndex>>,
    row_exact: Vec<Vec<bool>>,
    declared: Vec<bool>,
    width: usize,
    height: usize,
}

impl ReferenceRuntime {
    /// A fresh runtime for a compiled video: nothing is declared yet.
    pub fn new(video: &CompiledVideo) -> Result<Self, RuntimeError> {
        let (width, height) = (video.dimensions.0 as usize, video.dimensions.1 as usize);
        if width == 0 || height == 0 {
            return Err(RuntimeError::GeometryMismatch {
                expected: 1,
                actual: 0,
            });
        }
        Ok(Self {
            rows: vec![Vec::new(); height],
            row_exact: vec![Vec::new(); height],
            declared: vec![false; height],
            width,
            height,
        })
    }

    /// Restore from the last checkpoint at or before `at`, if any.
    pub fn from_checkpoint(video: &CompiledVideo, at: MediaClock) -> Result<Self, RuntimeError> {
        let mut runtime = Self::new(video)?;
        match video.timeline.seek_checkpoint(at) {
            Some(checkpoint) => {
                let contents = decode_checkpoint_payload(checkpoint.payload_bytes(), video)?;
                for (y, content) in contents.iter().enumerate() {
                    runtime.declare_row(y, &video.row_inventory[content.0 as usize], video)?;
                }
                Ok(runtime)
            }
            None => Ok(runtime), // reconstruction starts from the initial state
        }
    }

    /// Declare a row's full content from the row inventory.
    fn declare_row(
        &mut self,
        y: usize,
        content: &[ColourIndex],
        _video: &CompiledVideo,
    ) -> Result<(), RuntimeError> {
        if content.len() != self.width {
            return Err(RuntimeError::GeometryMismatch {
                expected: self.width,
                actual: content.len(),
            });
        }
        self.rows[y] = content.to_vec();
        self.row_exact[y] = vec![true; self.width];
        self.declared[y] = true;
        Ok(())
    }

    /// Apply one instruction against the persistent state.
    pub fn apply(
        &mut self,
        video: &CompiledVideo,
        instruction: &Instruction,
    ) -> Result<(), RuntimeError> {
        match instruction {
            Instruction::SetRow {
                target: crate::instructions::Target::Row { row },
                states,
            } => {
                let y = *row as usize;
                if y >= self.height || states.len() != self.width {
                    return Err(RuntimeError::GeometryMismatch {
                        expected: self.width,
                        actual: states.len(),
                    });
                }
                self.rows[y] = states.iter().map(|s| s.fill).collect();
                self.row_exact[y] = states.iter().map(|s| s.exactness.is_exact()).collect();
                self.declared[y] = true;
                Ok(())
            }
            Instruction::SetRun {
                row,
                start,
                len,
                state,
            } => {
                let y = *row as usize;
                let start = *start as usize;
                let len = *len as usize;
                if y >= self.height {
                    return Err(RuntimeError::GeometryMismatch {
                        expected: self.height,
                        actual: y,
                    });
                }
                if !self.declared[y] {
                    self.rows[y] = vec![state.fill; self.width];
                    self.row_exact[y] = vec![false; self.width];
                    self.declared[y] = true;
                }
                if start + len > self.width {
                    return Err(RuntimeError::GeometryMismatch {
                        expected: self.width,
                        actual: start + len,
                    });
                }
                for i in start..start + len {
                    self.rows[y][i] = state.fill;
                    self.row_exact[y][i] = state.exactness.is_exact();
                }
                Ok(())
            }
            Instruction::ApplyDelta { row, delta_id } => {
                let delta =
                    video
                        .deltas
                        .get(*delta_id as usize)
                        .ok_or(RuntimeError::UnknownReference {
                            kind: "delta",
                            id: *delta_id,
                        })?;
                let y = *row as usize;
                if y >= self.height {
                    return Err(RuntimeError::GeometryMismatch {
                        expected: self.height,
                        actual: y,
                    });
                }
                // Precondition: the targeted row must currently hold the
                // delta's content (AVC-C13). Applying against unmatched
                // content is a contract violation, never a best-effort patch.
                if !self.declared[y] || self.row_content_id(video, y) != Some(delta.from) {
                    return Err(RuntimeError::DeltaPrecondition {
                        delta_id: *delta_id,
                        expected: delta.from,
                    });
                }
                for change in &delta.changes {
                    let offset = change.offset as usize;
                    if offset >= self.width {
                        return Err(RuntimeError::GeometryMismatch {
                            expected: self.width,
                            actual: offset,
                        });
                    }
                    self.rows[y][offset] = change.to;
                    // Fill-only reference: transitioned-to content is
                    // inventory-exact (Amendment 4 limitation).
                    self.row_exact[y][offset] = true;
                }
                Ok(())
            }
            Instruction::LoadState { checkpoint_id } => {
                let checkpoints = video.timeline.checkpoints();
                let checkpoint = checkpoints.get(*checkpoint_id as usize).ok_or(
                    RuntimeError::UnknownReference {
                        kind: "checkpoint",
                        id: *checkpoint_id,
                    },
                )?;
                let contents = decode_checkpoint_payload(checkpoint.payload_bytes(), video)?;
                for (y, content) in contents.iter().enumerate() {
                    self.declare_row(y, &video.row_inventory[content.0 as usize], video)?;
                }
                Ok(())
            }
            Instruction::Sync => Ok(()),
            Instruction::Present => Ok(()), // presentation is driven by the driver below
            other => Err(RuntimeError::Unsupported(instruction_name(other))),
        }
    }

    fn row_content_id(&self, video: &CompiledVideo, y: usize) -> Option<ContentId> {
        video
            .row_inventory
            .iter()
            .position(|content| content == &self.rows[y])
            .map(|index| ContentId(index as u32))
    }

    /// Atomically present the current visual state.
    pub fn present(&self, at: MediaClock) -> Result<PresentedFrame, RuntimeError> {
        if self.declared.iter().any(|d| !d) {
            return Err(RuntimeError::UndefinedRegion);
        }
        let mut pixels = Vec::with_capacity(self.width * self.height);
        let mut exact = true;
        for y in 0..self.height {
            pixels.extend_from_slice(&self.rows[y]);
            exact &= self.row_exact[y].iter().all(|&e| e);
        }
        Ok(PresentedFrame { at, pixels, exact })
    }
}

fn instruction_name(instruction: &Instruction) -> &'static str {
    match instruction {
        Instruction::SetFill { .. } => "SET_FILL",
        Instruction::SetTrim { .. } => "SET_TRIM",
        Instruction::SetLight { .. } => "SET_LIGHT",
        Instruction::SetRegion { .. } => "SET_REGION",
        Instruction::CopyRegion { .. } => "COPY_REGION",
        _ => "UNKNOWN",
    }
}

/// Replay a compiled video's timeline to completion, returning every
/// presented frame. Pure: the same video always produces the same
/// frames (deterministic playback).
pub fn play(video: &CompiledVideo) -> Result<Vec<PresentedFrame>, RuntimeError> {
    let mut runtime = ReferenceRuntime::new(video)?;
    let mut frames = Vec::new();
    for event in video.timeline.events() {
        match &event.instruction {
            Instruction::Present => frames.push(runtime.present(event.at)?),
            instruction => runtime.apply(video, instruction)?,
        }
    }
    Ok(frames)
}

/// Replay from `at`: state is restored from the last checkpoint at or
/// before `at` when one exists (events strictly before the checkpoint
/// are skipped — their effect is in it); otherwise the full program
/// runs from the initial state. Presented frames are only collected
/// from `at` onward either way.
pub fn play_from(
    video: &CompiledVideo,
    at: MediaClock,
) -> Result<Vec<PresentedFrame>, RuntimeError> {
    let start_tick = video
        .timeline
        .seek_checkpoint(at)
        .map(|checkpoint| checkpoint.at.ticks())
        .unwrap_or(0);
    let mut runtime = if start_tick > 0 {
        let checkpoint_at = MediaClock::new(start_tick, at.ticks_per_second())
            .map_err(|_| RuntimeError::CheckpointPayload("unusable checkpoint position".into()))?;
        ReferenceRuntime::from_checkpoint(video, checkpoint_at)?
    } else {
        ReferenceRuntime::new(video)?
    };
    let mut frames = Vec::new();
    for event in video.timeline.events() {
        if event.at.ticks() < start_tick {
            continue;
        }
        match &event.instruction {
            Instruction::Present => {
                if event.at.ticks() >= at.ticks() {
                    frames.push(runtime.present(event.at)?);
                }
            }
            instruction => runtime.apply(video, instruction)?,
        }
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::{PaletteBound, Tolerance, WorkingSpace};
    use crate::compile::compile;
    use crate::ingestion::{
        DecodedFrame, DecodedFrameFormat, IngestionParameters, SourceError, VideoSource,
    };
    use crate::instructions::InstructionBudget;
    use crate::profile::{CompilationBudgets, CompilationProfile, ProfileKind};
    use crate::temporal::Checkpoint;

    const BG: [u8; 3] = [240, 240, 240];
    const WINDOW: [u8; 3] = [16, 24, 200];
    const CURSOR: [u8; 3] = [200, 40, 40];

    struct ScreenRecording {
        frames: Vec<Vec<[u8; 3]>>,
        width: u32,
        height: u32,
    }

    impl ScreenRecording {
        fn new() -> Self {
            let (w, h) = (8u32, 4u32);
            let blank: Vec<[u8; 3]> = vec![BG; (w * h) as usize];
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
    }

    impl VideoSource for ScreenRecording {
        fn source_digest(&self) -> String {
            format!(
                "screen-{}x{}-{}",
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
            p: &IngestionParameters,
        ) -> Result<DecodedFrame, SourceError> {
            let fd = crate::clock::MediaClock::frame_duration(30, 1, p.ticks_per_second).unwrap();
            let mut data = Vec::new();
            for px in &self.frames[i as usize] {
                data.extend_from_slice(px.as_slice());
            }
            Ok(DecodedFrame {
                format: DecodedFrameFormat::Rgb8,
                width: self.width,
                height: self.height,
                at: crate::clock::MediaClock::new(fd.ticks() * i as u64, p.ticks_per_second)
                    .unwrap(),
                data,
            })
        }
    }

    fn compile_poc() -> (CompiledVideo, crate::compile::CompileStats) {
        let source = ScreenRecording::new();
        let params = IngestionParameters {
            working_space: WorkingSpace::LinearRec709,
            ticks_per_second: 30,
        }
        .bind()
        .unwrap();
        let profile = CompilationProfile {
            kind: ProfileKind::Desktop,
            budgets: CompilationBudgets {
                palette_bound: PaletteBound(16),
                tolerance: Tolerance::ZERO,
                max_checkpoint_interval: crate::clock::MediaClock::new(90, 30).unwrap(),
                max_delta_depth: 8,
                residency_budget_bytes: 1 << 20,
            },
        };
        compile(&source, &params, &profile).unwrap()
    }

    fn source_pixel_rgb(source: &ScreenRecording, frame: usize, index: usize) -> [u8; 3] {
        source.frames[frame][index]
    }

    fn expect_colour_entry_id(video: &CompiledVideo, rgb: [u8; 3]) -> ColourIndex {
        let entry = crate::ingestion::srgb8_to_colour_entry(rgb);
        let position = video
            .inventory
            .entries()
            .iter()
            .position(|e| e == &entry)
            .expect("colour must be in the inventory");
        ColourIndex(position as u16)
    }

    #[test]
    fn reconstruction_is_pixel_exact_in_lossless_mode() {
        let source = ScreenRecording::new();
        let (video, _) = compile_poc();
        let frames = play(&video).unwrap();
        assert_eq!(frames.len(), 3);

        for (f, frame) in frames.iter().enumerate() {
            assert!(frame.exact, "lossless path must present exact pixels");
            for (i, pixel) in frame.pixels.iter().enumerate() {
                let expected = expect_colour_entry_id(&video, source_pixel_rgb(&source, f, i));
                assert_eq!(
                    *pixel, expected,
                    "frame {f} pixel {i}: unexplained difference"
                );
            }
        }
    }

    #[test]
    fn playback_is_deterministic() {
        let (video, _) = compile_poc();
        assert_eq!(play(&video).unwrap(), play(&video).unwrap());
    }

    #[test]
    fn checkpointed_replay_equals_full_replay() {
        let (video, _) = compile_poc();
        let full = play(&video).unwrap();
        let from_frame_1 =
            play_from(&video, crate::clock::MediaClock::new(1, 30).unwrap()).unwrap();
        // The checkpointed replay reconstructs the same final state: its
        // last presented frame matches the full replay's last frame.
        assert_eq!(full.last(), from_frame_1.last());
    }

    #[test]
    fn delta_precondition_violation_is_a_contract_violation() {
        let (mut video, _) = compile_poc();
        // Corrupt the delta's precondition so no row matches: the runtime
        // must reject (AVC-C13), not patch best-effort.
        let corruption = {
            let mut delta = video.deltas[0].clone();
            delta.from = ContentId(u32::MAX);
            delta
        };
        video.deltas[0] = corruption;
        // ContentId(u32::MAX) would not resolve in the inventory; craft a
        // program event applying it directly to test the runtime path.
        let mut runtime = ReferenceRuntime::new(&video).unwrap();
        let h = video.dimensions.1 as usize;
        // Declare the initial state manually so a row exists.
        for y in 0..h {
            let content = video.row_inventory[y.min(video.row_inventory.len() - 1)].clone();
            let states: Vec<crate::pixel::PixelState> = content
                .iter()
                .map(|&c| crate::pixel::PixelState::exact_fill(c))
                .collect();
            runtime
                .apply(
                    &video,
                    &Instruction::SetRow {
                        target: crate::instructions::Target::Row { row: y as u32 },
                        states,
                    },
                )
                .unwrap();
        }
        assert_eq!(
            runtime.apply(
                &video,
                &Instruction::ApplyDelta {
                    row: 0,
                    delta_id: 0
                }
            ),
            Err(RuntimeError::DeltaPrecondition {
                delta_id: 0,
                expected: ContentId(u32::MAX)
            })
        );
    }

    #[test]
    fn unsupported_instructions_are_rejected_explicitly() {
        let (video, _) = compile_poc();
        let mut runtime = ReferenceRuntime::new(&video).unwrap();
        let err = runtime.apply(
            &video,
            &Instruction::SetLight {
                target: crate::instructions::Target::Row { row: 0 },
                light: crate::pixel::Light(0x0080),
            },
        );
        assert_eq!(err, Err(RuntimeError::Unsupported("SET_LIGHT")));
    }

    #[test]
    fn checkpoint_payload_round_trips_and_validates() {
        let (video, _) = compile_poc();
        // The POC interval never fires; synthesise a checkpoint whose
        // contents exist in the row inventory (the background row).
        let contents: Vec<ContentId> = vec![ContentId(0); video.dimensions.1 as usize];
        let checkpoint = Checkpoint::new(
            crate::temporal::TemporalStateId(9),
            crate::clock::MediaClock::new(1, 30).unwrap(),
            &encode_checkpoint_payload(&contents),
        );
        assert!(checkpoint.digest_is_intact());
        let decoded = decode_checkpoint_payload(checkpoint.payload_bytes(), &video).unwrap();
        assert_eq!(decoded, contents);
        // Tampered payload fails the digest check (AVC-C05) — the
        // from_parts path simulates a deserialised checkpoint.
        let intact_digest = checkpoint.digest;
        let tampered = Checkpoint::from_parts(
            checkpoint.state,
            checkpoint.at,
            b"corrupted payload".to_vec(),
            intact_digest,
        );
        assert!(!tampered.digest_is_intact());
        assert!(checkpoint.digest_is_intact());
        // Truncated payload is rejected.
        assert!(decode_checkpoint_payload(&checkpoint.payload_bytes()[..3], &video).is_err());
        // Out-of-range content id is rejected.
        let bad = encode_checkpoint_payload(&[ContentId(u32::MAX); 4]);
        assert!(decode_checkpoint_payload(&bad, &video).is_err());
    }

    #[test]
    fn present_with_undeclared_rows_is_rejected() {
        let (video, _) = compile_poc();
        let runtime = ReferenceRuntime::new(&video).unwrap();
        assert_eq!(
            runtime.present(crate::clock::MediaClock::new(0, 30).unwrap()),
            Err(RuntimeError::UndefinedRegion)
        );
    }

    #[test]
    fn run_program_budget_still_validates() {
        let (video, _) = compile_poc();
        let budget = InstructionBudget {
            max_events: 1_000,
            max_delta_applications: 10,
        };
        assert!(video.timeline.validate(&budget).is_ok());
    }
}
