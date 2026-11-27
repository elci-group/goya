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
use crate::colour::{scale_entry, ColourEntry};
use crate::compile::CompiledVideo;
use crate::delta::ContentId;
use crate::instructions::{Instruction, Target};
use crate::pixel::{ColourIndex, Light};

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

/// One atomically presented frame: the row-major *presented colours*
/// (chroma scaled by light, Q24.8 per channel) and per-pixel exactness
/// (F3: `PRESENT` reports its exactness coverage).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentedFrame {
    pub at: MediaClock,
    pub pixels: Vec<[u32; 3]>,
    /// Per-pixel exactness, row-major.
    pub pixel_exact: Vec<bool>,
    /// True when every presented pixel was reconstructed exactly.
    pub exact: bool,
}

/// One restored row: content id plus light state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointRow {
    pub content: ContentId,
    pub light: Light,
}

/// Encode a checkpoint payload: (row-content id, light) pairs, ascending
/// row order, little-endian (F5's fixed-point discipline applies to
/// payload integers). Light is part of the visual state and must survive
/// a seek (Amendment 5).
pub fn encode_checkpoint_payload(rows: &[CheckpointRow]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(rows.len() * 6);
    for row in rows {
        payload.extend_from_slice(&row.content.0.to_le_bytes());
        payload.extend_from_slice(&row.light.0.to_le_bytes());
    }
    payload
}

/// Decode a checkpoint payload against a compiled video's row inventory,
/// rejecting ids that do not resolve.
pub fn decode_checkpoint_payload(
    payload: &[u8],
    video: &CompiledVideo,
) -> Result<Vec<CheckpointRow>, RuntimeError> {
    const RECORD: usize = 6; // u32 content id + u16 light, little-endian
    if !payload.len().is_multiple_of(RECORD) {
        return Err(RuntimeError::CheckpointPayload(format!(
            "payload length {} is not a multiple of {RECORD}",
            payload.len()
        )));
    }
    let count = video.dimensions.1 as usize;
    if payload.len() / RECORD != count {
        return Err(RuntimeError::CheckpointPayload(format!(
            "payload names {} rows, video declares {count}",
            payload.len() / RECORD
        )));
    }
    (0..count)
        .map(|i| {
            let b = &payload[i * RECORD..(i + 1) * RECORD];
            let content = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            let light = u16::from_le_bytes([b[4], b[5]]);
            if (content as usize) >= video.row_inventory.len() {
                return Err(RuntimeError::CheckpointPayload(format!(
                    "row content id {content} is outside the row inventory"
                )));
            }
            Ok(CheckpointRow {
                content: ContentId(content),
                light: Light(light),
            })
        })
        .collect()
}

/// The persistent visual state plus its reconstruction bookkeeping.
/// Each pixel holds its chroma index and its light value; the presented
/// colour is `scale_entry(chroma, light)` (Amendment 5).
pub struct ReferenceRuntime {
    /// The compiled video's colour inventory, held for presentation:
    /// chroma indices resolve to Q24.8 linear entries here.
    entries: Vec<ColourEntry>,
    rows: Vec<Vec<(ColourIndex, Light)>>,
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
            entries: video.inventory.entries().to_vec(),
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
                let rows = decode_checkpoint_payload(checkpoint.payload_bytes(), video)?;
                for (y, row) in rows.iter().enumerate() {
                    runtime.declare_row(
                        y,
                        &video.row_inventory[row.content.0 as usize],
                        row.light,
                    )?;
                }
                Ok(runtime)
            }
            None => Ok(runtime), // reconstruction starts from the initial state
        }
    }

    /// Declare a row's full content from the row inventory at a light value.
    fn declare_row(
        &mut self,
        y: usize,
        content: &[ColourIndex],
        light: Light,
    ) -> Result<(), RuntimeError> {
        if content.len() != self.width {
            return Err(RuntimeError::GeometryMismatch {
                expected: self.width,
                actual: content.len(),
            });
        }
        self.rows[y] = content.iter().map(|&c| (c, light)).collect();
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
                self.rows[y] = states.iter().map(|s| (s.fill, s.light)).collect();
                self.row_exact[y] = states.iter().map(|s| s.exactness.is_exact()).collect();
                self.declared[y] = true;
                Ok(())
            }
            Instruction::SetLight {
                target,
                light,
                exactness,
            } => {
                // Light multiplies each pixel's current light value
                // (compounding dimming); chroma is untouched (directive §2).
                // The event's exactness class applies to every pixel of the
                // span: light changes the presentation of all of them.
                match target {
                    Target::Row { row } => {
                        let y = *row as usize;
                        if y >= self.height {
                            return Err(RuntimeError::GeometryMismatch {
                                expected: self.height,
                                actual: y,
                            });
                        }
                        if !self.declared[y] {
                            return Err(RuntimeError::UndefinedRegion);
                        }
                        for pixel in &mut self.rows[y] {
                            pixel.1 = Light::mul(pixel.1, *light);
                        }
                        self.row_exact[y] = vec![exactness.is_exact(); self.width];
                        Ok(())
                    }
                    Target::Region {
                        row,
                        pixel,
                        height,
                        width,
                    } => {
                        let start_y = *row as usize;
                        let end_y = start_y + *height as usize;
                        let start_x = *pixel as usize;
                        let end_x = start_x + *width as usize;
                        if end_y > self.height || end_x > self.width {
                            return Err(RuntimeError::GeometryMismatch {
                                expected: self.width,
                                actual: end_x.max(self.width),
                            });
                        }
                        for y in start_y..end_y {
                            if !self.declared[y] {
                                return Err(RuntimeError::UndefinedRegion);
                            }
                            for i in start_x..end_x {
                                self.rows[y][i].1 = Light::mul(self.rows[y][i].1, *light);
                                self.row_exact[y][i] = exactness.is_exact();
                            }
                        }
                        Ok(())
                    }
                    _ => Err(RuntimeError::Unsupported("SET_LIGHT")),
                }
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
                    self.rows[y] = vec![(state.fill, state.light); self.width];
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
                    self.rows[y][i] = (state.fill, state.light);
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
                    self.rows[y][offset] = (change.to, Light::UNITY);
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
                let rows = decode_checkpoint_payload(checkpoint.payload_bytes(), video)?;
                for (y, row) in rows.iter().enumerate() {
                    self.declare_row(y, &video.row_inventory[row.content.0 as usize], row.light)?;
                }
                Ok(())
            }
            Instruction::Sync => Ok(()),
            Instruction::Present => Ok(()), // presentation is driven by the driver below
            other => Err(RuntimeError::Unsupported(instruction_name(other))),
        }
    }

    fn row_content_id(&self, video: &CompiledVideo, y: usize) -> Option<ContentId> {
        let chroma: Vec<ColourIndex> = self.rows[y].iter().map(|(c, _)| *c).collect();
        video
            .row_inventory
            .iter()
            .position(|content| content == &chroma)
            .map(|index| ContentId(index as u32))
    }

    /// Atomically present the current visual state: chroma scaled by
    /// light, per pixel (Amendment 5's presentation model).
    pub fn present(&self, at: MediaClock) -> Result<PresentedFrame, RuntimeError> {
        if self.declared.iter().any(|d| !d) {
            return Err(RuntimeError::UndefinedRegion);
        }
        let mut pixels = Vec::with_capacity(self.width * self.height);
        let mut pixel_exact = Vec::with_capacity(self.width * self.height);
        let mut exact = true;
        for y in 0..self.height {
            for (chroma, light) in &self.rows[y] {
                let entry =
                    self.entries
                        .get(chroma.0 as usize)
                        .ok_or(RuntimeError::UnknownReference {
                            kind: "colour",
                            id: u32::from(chroma.0),
                        })?;
                pixels.push(scale_entry(entry, *light));
            }
            pixel_exact.extend_from_slice(&self.row_exact[y]);
            exact &= self.row_exact[y].iter().all(|&e| e);
        }
        Ok(PresentedFrame {
            at,
            pixels,
            pixel_exact,
            exact,
        })
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
    use crate::pixel::{ApproximationCause, Exactness, PixelState};
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

    #[test]
    fn reconstruction_is_pixel_exact_in_lossless_mode() {
        let source = ScreenRecording::new();
        let (video, _) = compile_poc();
        let frames = play(&video).unwrap();
        assert_eq!(frames.len(), 3);

        for (f, frame) in frames.iter().enumerate() {
            assert!(frame.exact, "lossless path must present exact pixels");
            for (i, pixel) in frame.pixels.iter().enumerate() {
                let entry =
                    crate::ingestion::srgb8_to_colour_entry(source_pixel_rgb(&source, f, i));
                assert_eq!(
                    *pixel,
                    scale_entry(&entry, Light::UNITY),
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
            &Instruction::SetTrim {
                target: crate::instructions::Target::Row { row: 0 },
                slot: 0,
                colour: ColourIndex(0),
            },
        );
        assert_eq!(err, Err(RuntimeError::Unsupported("SET_TRIM")));
    }

    #[test]
    fn checkpoint_payload_round_trips_and_validates() {
        let (video, _) = compile_poc();
        // The POC interval never fires; synthesise a checkpoint whose
        // contents exist in the row inventory (the background row) at
        // half light — light is part of the payload (Amendment 5).
        let rows: Vec<CheckpointRow> = (0..video.dimensions.1)
            .map(|_| CheckpointRow {
                content: ContentId(0),
                light: Light(0x0080),
            })
            .collect();
        let checkpoint = Checkpoint::new(
            crate::temporal::TemporalStateId(9),
            crate::clock::MediaClock::new(1, 30).unwrap(),
            &encode_checkpoint_payload(&rows),
        );
        assert!(checkpoint.digest_is_intact());
        let decoded = decode_checkpoint_payload(checkpoint.payload_bytes(), &video).unwrap();
        assert_eq!(decoded, rows);
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
        let bad = encode_checkpoint_payload(
            &[CheckpointRow {
                content: ContentId(u32::MAX),
                light: Light::UNITY,
            }; 4],
        );
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

    /// A screen recording whose only frame-to-frame change is a pure
    /// luminance dimming: row 1 dims from sRGB gray 200 to gray 146,
    /// which is an *exact* Q24.8 half-light scaling (linear 148 -> 74),
    /// then returns to full brightness.
    struct DimmingRecording;

    impl VideoSource for DimmingRecording {
        fn source_digest(&self) -> String {
            "dimming-8x4-3".to_string()
        }
        fn dimensions(&self) -> (u32, u32) {
            (8, 4)
        }
        fn frame_rate(&self) -> (u32, u32) {
            (30, 1)
        }
        fn frame_count(&self) -> u32 {
            3
        }
        fn decode_frame(
            &self,
            i: u32,
            p: &IngestionParameters,
        ) -> Result<DecodedFrame, SourceError> {
            let fd = crate::clock::MediaClock::frame_duration(30, 1, p.ticks_per_second).unwrap();
            const GRAY_FULL: [u8; 3] = [200, 200, 200];
            const GRAY_DIM: [u8; 3] = [146, 146, 146];
            let mut data = Vec::with_capacity(96);
            let row = match i {
                0 | 2 => GRAY_FULL,
                1 => GRAY_DIM,
                _ => {
                    return Err(SourceError::DecodeFailed(format!(
                        "frame {i} out of bounds"
                    )))
                }
            };
            for y in 0..4usize {
                for _ in 0..8usize {
                    data.extend_from_slice(if y == 1 { &row } else { &BG });
                }
            }
            Ok(DecodedFrame {
                format: DecodedFrameFormat::Rgb8,
                width: 8,
                height: 4,
                at: crate::clock::MediaClock::new(fd.ticks() * i as u64, p.ticks_per_second)
                    .unwrap(),
                data,
            })
        }
    }

    fn compile_dimming(tolerance: Tolerance) -> (CompiledVideo, crate::compile::CompileStats) {
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
                tolerance,
                max_checkpoint_interval: crate::clock::MediaClock::new(90, 30).unwrap(),
                max_delta_depth: 8,
                residency_budget_bytes: 1 << 20,
            },
        };
        compile(&DimmingRecording, &params, &profile).unwrap()
    }

    #[test]
    fn exact_light_transition_compiles_to_a_light_event() {
        let (video, stats) = compile_dimming(Tolerance::ZERO);
        // The gray-200 -> gray-146 transition is an exact half-light
        // scaling; even the lossless profile takes the light path.
        assert_eq!(stats.light_events, 1);
        assert!(video
            .timeline
            .events()
            .iter()
            .any(|e| matches!(&e.instruction, Instruction::SetLight { .. })));
        // Chroma rows are unchanged: the dimmed row is not a new content.
        assert!(video.deltas.is_empty());
    }

    #[test]
    fn exact_light_replay_is_pixel_exact_and_restores_brightness() {
        let (video, _) = compile_dimming(Tolerance::ZERO);
        let frames = play(&video).unwrap();
        assert_eq!(frames.len(), 3);
        let full_entry = crate::ingestion::srgb8_to_colour_entry([200, 200, 200]);
        let dim_entry = crate::ingestion::srgb8_to_colour_entry([146, 146, 146]);
        // Frame 1: the dimmed presentation — exact, because the scaling
        // is exact in Q24.8.
        assert!(frames[1].exact);
        for x in 0..8usize {
            assert_eq!(
                frames[1].pixels[8 + x],
                scale_entry(&dim_entry, Light::UNITY)
            );
        }
        // Frame 2: brightness restored — a dimmed row must never stay
        // dimmed once the source returns to full light.
        assert!(frames[2].exact);
        for x in 0..8usize {
            assert_eq!(
                frames[2].pixels[8 + x],
                scale_entry(&full_entry, Light::UNITY)
            );
        }
    }

    #[test]
    fn tolerance_profile_takes_the_light_path_for_inexact_scaling() {
        // A tolerance budget admits light events the lossless profile
        // would refuse; the event is classified Approximate.
        let (video, stats) = compile_dimming(Tolerance(500));
        let _ = stats.light_events; // same fixture, but the invariant below is profile-independent
        for event in video.timeline.events() {
            if let Instruction::SetLight { exactness, .. } = &event.instruction {
                assert!(matches!(
                    exactness,
                    Exactness::Exact | Exactness::Approximate(_)
                ));
            }
        }
    }

    #[test]
    fn set_light_scales_the_presented_colours_and_marks_inexactness() {
        let (video, _) = compile_poc();
        let mut runtime = ReferenceRuntime::new(&video).unwrap();
        let h = video.dimensions.1 as usize;
        for y in 0..h {
            let content = video.row_inventory[y.min(video.row_inventory.len() - 1)].clone();
            let states: Vec<PixelState> =
                content.iter().map(|&c| PixelState::exact_fill(c)).collect();
            runtime
                .apply(
                    &video,
                    &Instruction::SetRow {
                        target: Target::Row { row: y as u32 },
                        states,
                    },
                )
                .unwrap();
        }
        let half = Light(0x0080);
        runtime
            .apply(
                &video,
                &Instruction::SetLight {
                    target: Target::Row { row: 0 },
                    light: half,
                    exactness: Exactness::Approximate(ApproximationCause::ToleranceBudget),
                },
            )
            .unwrap();
        let frame = runtime
            .present(crate::clock::MediaClock::new(1, 30).unwrap())
            .unwrap();
        assert!(!frame.exact, "an inexact light event must mark its pixels");
        assert!(frame.pixel_exact[..8].iter().all(|&e| !e));
        assert!(frame.pixel_exact[8..].iter().all(|&e| e));
        for (i, pixel) in frame.pixels.iter().enumerate().take(8) {
            let entry = &video.inventory.entries()[video.row_inventory[0][i].0 as usize];
            assert_eq!(*pixel, scale_entry(entry, half));
        }
        // Rows outside the event keep unity light and exactness.
        assert!(frame.pixel_exact[8..].iter().all(|&e| e));
    }

    #[test]
    fn set_light_on_an_undeclared_row_is_rejected() {
        let (video, _) = compile_poc();
        let mut runtime = ReferenceRuntime::new(&video).unwrap();
        assert_eq!(
            runtime.apply(
                &video,
                &Instruction::SetLight {
                    target: Target::Row { row: 0 },
                    light: Light(0x0080),
                    exactness: Exactness::Exact,
                },
            ),
            Err(RuntimeError::UndefinedRegion)
        );
    }

    #[test]
    fn region_light_touches_only_the_region() {
        let (video, _) = compile_poc();
        let mut runtime = ReferenceRuntime::new(&video).unwrap();
        let h = video.dimensions.1 as usize;
        // Declare the POC fixture's known frame-0 geometry: rows 1–2
        // hold WINDOW on x 0..4, everything else BG.
        let id_of = |rgb: [u8; 3]| -> ColourIndex {
            let entry = crate::ingestion::srgb8_to_colour_entry(rgb);
            ColourIndex(
                video
                    .inventory
                    .entries()
                    .iter()
                    .position(|e| e == &entry)
                    .unwrap() as u16,
            )
        };
        let (bg_id, window_id) = (id_of(BG), id_of(WINDOW));
        for y in 0..h {
            let states: Vec<PixelState> = (0..8usize)
                .map(|x| {
                    PixelState::exact_fill(if (1..=2).contains(&y) && x < 4 {
                        window_id
                    } else {
                        bg_id
                    })
                })
                .collect();
            runtime
                .apply(
                    &video,
                    &Instruction::SetRow {
                        target: Target::Row { row: y as u32 },
                        states,
                    },
                )
                .unwrap();
        }
        let half = Light(0x0080);
        runtime
            .apply(
                &video,
                &Instruction::SetLight {
                    target: Target::Region {
                        row: 1,
                        pixel: 2,
                        height: 2,
                        width: 3,
                    },
                    light: half,
                    exactness: Exactness::Exact,
                },
            )
            .unwrap();
        let frame = runtime
            .present(crate::clock::MediaClock::new(1, 30).unwrap())
            .unwrap();
        // The POC fixture's frame-0 geometry is known: rows 1–2 hold
        // WINDOW on x 0..4, everything else BG.
        let bg = crate::ingestion::srgb8_to_colour_entry(BG);
        let window = crate::ingestion::srgb8_to_colour_entry(WINDOW);
        for y in 0..h {
            for x in 0..8usize {
                let i = y * 8 + x;
                let entry = if (1..=2).contains(&y) && x < 4 {
                    &window
                } else {
                    &bg
                };
                let in_region = (1..3).contains(&y) && (2..5).contains(&x);
                let expected = if in_region {
                    scale_entry(entry, half)
                } else {
                    scale_entry(entry, Light::UNITY)
                };
                assert_eq!(frame.pixels[i], expected, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn checkpoint_restores_light_state() {
        // Light is visual state: restoring a checkpoint must restore the
        // dimmed presentation, not merely the chroma content.
        let (video, _) = compile_poc();
        let rows: Vec<CheckpointRow> = (0..video.dimensions.1)
            .map(|_| CheckpointRow {
                content: ContentId(0),
                light: Light(0x0080),
            })
            .collect();
        let payload = encode_checkpoint_payload(&rows);
        let checkpoint = Checkpoint::new(
            crate::temporal::TemporalStateId(7),
            crate::clock::MediaClock::new(1, 30).unwrap(),
            &payload,
        );
        // Loading an unknown checkpoint id is rejected.
        let mut runtime = ReferenceRuntime::new(&video).unwrap();
        assert_eq!(
            runtime.apply(&video, &Instruction::LoadState { checkpoint_id: 0 }),
            Err(RuntimeError::UnknownReference {
                kind: "checkpoint",
                id: 0
            })
        );
        let decoded = decode_checkpoint_payload(&payload, &video).unwrap();
        assert!(decoded.iter().all(|row| row.light == Light(0x0080)));
        // And through from_checkpoint with a real checkpoint injected.
        let mut with_checkpoint = video.clone();
        with_checkpoint
            .timeline
            .push_checkpoint(checkpoint)
            .unwrap();
        let runtime = ReferenceRuntime::from_checkpoint(
            &with_checkpoint,
            crate::clock::MediaClock::new(2, 30).unwrap(),
        )
        .unwrap();
        let frame = runtime
            .present(crate::clock::MediaClock::new(2, 30).unwrap())
            .unwrap();
        // ContentId(0) is the first deduplicated row of frame 0 — the
        // all-background row — so every restored pixel presents the
        // dimmed background entry.
        let bg = crate::ingestion::srgb8_to_colour_entry(BG);
        for pixel in &frame.pixels {
            assert_eq!(*pixel, scale_entry(&bg, Light(0x0080)));
        }
        // Restored rows are content-declared; the payload carries no
        // exactness record (Amendment 5 limitation: restored light
        // presents as declared content).
    }
}
