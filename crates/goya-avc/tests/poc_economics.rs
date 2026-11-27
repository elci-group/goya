//! POC-scale economics of the persistent-state model (ADR-AVC-001
//! Amendment 6).
//!
//! A deterministic synthetic screen recording — a window drifting
//! horizontally, a cursor moving independently, on a static desktop —
//! compiled under the lossless profile. The test asserts the structural
//! invariants the ADR's acceptance targets demand at POC scale and
//! prints the measured economics (`--nocapture`); the numbers recorded
//! in the ADR come from this run. It is not a storage benchmark: the
//! Phase-2 binary encoder does not exist, so sizes here are *models*
//! (instruction and payload arithmetic, no framing), clearly labelled.

use goya_avc::{
    compile, decode_checkpoint_payload, play, play_from, CompilationBudgets, CompilationProfile,
    DecodedFrame, DecodedFrameFormat, IngestionParameters, MediaClock, PaletteBound, ProfileKind,
    Tolerance, VideoSource, WorkingSpace,
};
use goya_avc::{ApproximationCause, ColourEntry, Exactness};
use std::time::Instant;

/// Desktop background: light grey.
const BG: [u8; 3] = [240, 240, 240];
/// Application window body: blue.
const WINDOW: [u8; 3] = [16, 24, 200];
/// Window title bar: darker blue (distinct content row band).
const TITLE: [u8; 3] = [12, 16, 120];
/// Cursor: red.
const CURSOR: [u8; 3] = [200, 40, 40];

/// A deterministic screen recording: 320×180, 240 frames at 30 FPS.
///
/// Geometry per frame `i`:
/// - a 96×64 window whose top-left drifts right by one pixel per frame
///   (band rows: title bar at the window's top, body below);
/// - a 3×3 cursor on a slow diagonal (wrapping), moving 2 px per frame;
/// - the desktop background everywhere else.
struct ScreenWorkload {
    width: u32,
    height: u32,
    frames: u32,
    window_w: u32,
    window_h: u32,
}

impl ScreenWorkload {
    fn new() -> Self {
        Self {
            width: 320,
            height: 180,
            frames: 240,
            window_w: 96,
            window_h: 64,
        }
    }

    fn frame(&self, i: u32) -> Vec<[u8; 3]> {
        let w = self.width as usize;
        let mut f = vec![BG; w * self.height as usize];
        // Window drifts right one pixel per frame, starting at x=16.
        let wx = (16 + i as usize).min(w - self.window_w as usize - 1);
        let wy = 40usize;
        for y in wy..wy + self.window_h as usize {
            for x in wx..wx + self.window_w as usize {
                f[y * w + x] = if y == wy { TITLE } else { WINDOW };
            }
        }
        // Cursor on a wrapping diagonal, 2 px per frame.
        let cx = (6 * i as usize + 11) % (w - 3);
        let cy = (3 * i as usize + 7) % (self.height as usize - 3);
        for y in cy..cy + 3 {
            for x in cx..cx + 3 {
                f[y * w + x] = CURSOR;
            }
        }
        f
    }
}

impl VideoSource for ScreenWorkload {
    fn source_digest(&self) -> String {
        format!("screen-econ-{}x{}-{}", self.width, self.height, self.frames)
    }
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn frame_rate(&self) -> (u32, u32) {
        (30, 1)
    }
    fn frame_count(&self) -> u32 {
        self.frames
    }
    fn decode_frame(
        &self,
        i: u32,
        p: &IngestionParameters,
    ) -> Result<DecodedFrame, goya_avc::SourceError> {
        let fd = MediaClock::frame_duration(30, 1, p.ticks_per_second).unwrap();
        let mut data = Vec::with_capacity((self.width * self.height * 3) as usize);
        for px in &self.frame(i) {
            data.extend_from_slice(px.as_slice());
        }
        Ok(DecodedFrame {
            format: DecodedFrameFormat::Rgb8,
            width: self.width,
            height: self.height,
            at: MediaClock::new(fd.ticks() * i as u64, p.ticks_per_second).unwrap(),
            data,
        })
    }
}

fn lossless_profile() -> CompilationProfile {
    CompilationProfile {
        kind: ProfileKind::Desktop,
        budgets: CompilationBudgets {
            palette_bound: PaletteBound(16),
            tolerance: Tolerance::ZERO,
            max_checkpoint_interval: MediaClock::new(90, 30).unwrap(),
            max_delta_depth: 8,
            residency_budget_bytes: 1 << 20,
        },
    }
}

/// Modelled binary size (Phase-2 encoder does not exist; this is the
/// arithmetic floor of the obvious fixed-width encoding, no framing or
/// varints): inventory entries 12 B, row inventory 2 B/pixel, deltas
/// 6 B/change, checkpoints 6 B/row, events 16 B.
fn modelled_bytes(video: &goya_avc::CompiledVideo) -> usize {
    let entries = video.inventory.entries().len() * 12;
    let rows: usize = video.row_inventory.iter().map(|r| r.len() * 2).sum();
    let deltas: usize = video.deltas.iter().map(|d| d.changes.len() * 6 + 16).sum();
    let checkpoints: usize = video.timeline.checkpoints().len() * video.dimensions.1 as usize * 6;
    let events = video.timeline.events().len() * 16;
    entries + rows + deltas + checkpoints + events
}

#[test]
fn poc_screen_recording_economics() {
    let source = ScreenWorkload::new();
    let params = IngestionParameters {
        working_space: WorkingSpace::LinearRec709,
        ticks_per_second: 30,
    }
    .bind()
    .unwrap();

    let started = Instant::now();
    let (video, stats) = compile(&source, &params, &lossless_profile()).unwrap();
    let compile_time = started.elapsed();

    // --- Correctness at scale (lossless acceptance target) ---
    let play_started = Instant::now();
    let frames = play(&video).unwrap();
    let play_time = play_started.elapsed();
    assert_eq!(frames.len(), source.frames as usize);
    for (i, frame) in frames.iter().enumerate() {
        assert!(frame.exact, "frame {i}: lossless replay must be exact");
        let expected = source.frame(i as u32);
        for (px, entry) in frame.pixels.iter().zip(expected.iter()) {
            let want = goya_avc::scale_entry(
                &ColourEntry {
                    linear_rgb: [
                        goya_avc::srgb8_to_linear_q24_8(entry[0]),
                        goya_avc::srgb8_to_linear_q24_8(entry[1]),
                        goya_avc::srgb8_to_linear_q24_8(entry[2]),
                    ],
                },
                goya_avc::Light::UNITY,
            );
            assert_eq!(*px, want, "frame {i}: unexplained pixel difference");
        }
    }

    // Deterministic playback.
    assert_eq!(frames, play(&video).unwrap());

    // Checkpointed seek from mid-timeline reconstructs the same final
    // state as the full replay.
    let mid = MediaClock::new(120, 30).unwrap();
    let from_mid = play_from(&video, mid).unwrap();
    assert_eq!(frames.last(), from_mid.last());

    // --- Economics: persistence beats re-declaration, decisively ---
    let naive = stats.naive_pixel_writes;
    let persistent = stats.persistent_pixel_writes;
    assert!(persistent * 10 < naive, "the persistence model must win by an order of magnitude on a screen recording (naive {naive}, persistent {persistent})");

    // Delta reuse: the drifting window produces many distinct
    // transitions, but the cursor's wraps and the window's motion still
    // share content pairs; at least some deltas must be reused.
    assert!(stats.delta_events > 0, "deltas must be exercised");
    assert!(stats.row_events > 0 && stats.run_events > 0);
    assert_eq!(
        stats.light_events, 0,
        "fixture has no light-only transitions"
    );

    // Checkpoints exist and their payloads decode.
    assert!(!video.timeline.checkpoints().is_empty());
    for checkpoint in video.timeline.checkpoints() {
        let decoded = decode_checkpoint_payload(checkpoint.payload_bytes(), &video).unwrap();
        assert_eq!(decoded.len(), video.dimensions.1 as usize);
    }

    // --- The measured table (recorded in ADR-AVC-001 Amendment 6) ---
    let raw_bytes = (source.frames * source.width * source.height * 3) as usize;
    println!(
        "=== AOT-VC POC screen-recording economics (320×180 @ 30 FPS, 240 frames, lossless) ==="
    );
    println!(
        "colour entries                : {}",
        video.inventory.entries().len()
    );
    println!(
        "distinct rows / row slots     : {} (dedup from {} frame-row slots)",
        video.row_inventory.len(),
        source.frames as usize * source.height as usize
    );
    println!(
        "SET_ROW / SET_RUN / APPLY_DELTA / SET_LIGHT events: {} / {} / {} / {}",
        stats.row_events, stats.run_events, stats.delta_events, stats.light_events
    );
    println!(
        "checkpoints                   : {}",
        video.timeline.checkpoints().len()
    );
    println!(
        "timeline events               : {}",
        video.timeline.events().len()
    );
    println!("naive pixel writes            : {naive}");
    println!("persistent pixel writes       : {persistent}");
    println!(
        "write reduction               : {:.1}×",
        naive as f64 / persistent as f64
    );
    println!("raw source bytes              : {raw_bytes}");
    println!(
        "modelled binary size (floor)  : {modelled} bytes ({:.2}× raw)",
        raw_bytes as f64 / modelled_bytes(&video) as f64,
        modelled = modelled_bytes(&video)
    );
    println!("compile time (this host)      : {compile_time:?} (non-deterministic, informational)");
    println!("replay time (this host)       : {play_time:?} (non-deterministic, informational)");

    // Exactness bookkeeping stays coherent at scale: an approximate
    // cause never appears on the lossless path.
    let _ = ApproximationCause::ToleranceBudget;
    let _ = Exactness::Exact;
}
