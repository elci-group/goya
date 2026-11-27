//! End-to-end integration: container → adapter → AOT-VC compile.
//!
//! Skips gracefully when ffmpeg is unavailable (the tools are external,
//! never vendored); otherwise generates a deterministic lossless FFV1
//! MKV fixture, probes it, decodes every frame through the adapter and
//! compiles it through the goya-avc reference pipeline.

use std::path::PathBuf;
use std::process::Command;

use goya_avc::colour::{PaletteBound, Tolerance, WorkingSpace};
use goya_avc::compile::{compile, CompileError};
use goya_avc::ingestion::{IngestionParameters, VideoSource};
use goya_avc::instructions::Instruction;
use goya_avc::profile::{CompilationBudgets, CompilationProfile, ProfileKind};

const WIDTH: u32 = 8;
const HEIGHT: u32 = 4;
const FRAMES: u32 = 3;
const FPS: (u32, u32) = (30, 1);

const BG: [u8; 3] = [240, 240, 240];
const WINDOW: [u8; 3] = [16, 24, 200];
const CURSOR: [u8; 3] = [200, 40, 40];

fn ffmpeg_available() -> Option<(PathBuf, PathBuf)> {
    let dir = std::env::temp_dir();
    for (tool, out) in [("ffmpeg", "ffmpeg"), ("ffprobe", "ffprobe")] {
        let path = dir.join(format!("goya-avc-decode-avail-{out}"));
        let _ = std::fs::remove_file(&path);
        if Command::new(tool).arg("-version").output().is_err() {
            return None;
        }
    }
    Some((PathBuf::from("ffmpeg"), PathBuf::from("ffprobe")))
}

/// Three synthetic screen-recording frames: desktop, window, moving cursor.
fn frame_pixels(frame_index: u32) -> Vec<[u8; 3]> {
    let mut px = vec![BG; (WIDTH * HEIGHT) as usize];
    for y in 1..3 {
        for x in 0..4 {
            px[(y * WIDTH + x) as usize] = WINDOW;
        }
    }
    let cursor_row = 1 + frame_index.clamp(0, 1);
    px[(cursor_row * WIDTH + 6) as usize] = CURSOR;
    px
}

fn make_fixture(path: &std::path::Path) {
    let mut raw: Vec<u8> = Vec::new();
    for f in 0..FRAMES {
        for px in frame_pixels(f) {
            raw.extend_from_slice(&px);
        }
    }
    let raw_path = path.with_extension("raw");
    std::fs::write(&raw_path, &raw).expect("write raw fixture");
    let status = Command::new("ffmpeg")
        .args([
            "-v".to_string(),
            "error".to_string(),
            "-f".to_string(),
            "rawvideo".to_string(),
            "-pix_fmt".to_string(),
            "rgb24".to_string(),
            "-s".to_string(),
            format!("{WIDTH}x{HEIGHT}"),
            "-r".to_string(),
            format!("{}/{}", FPS.0, FPS.1),
            "-i".to_string(),
            raw_path.to_string_lossy().into_owned(),
            "-c:v".to_string(),
            "ffv1".to_string(),
            "-level".to_string(),
            "3".to_string(),
            "-y".to_string(),
            path.to_string_lossy().into_owned(),
        ])
        .status()
        .expect("launch ffmpeg");
    assert!(status.success(), "ffmpeg fixture encoding failed");
    let _ = std::fs::remove_file(&raw_path);
}

#[test]
fn container_round_trip_through_aot_vc_compile() {
    let Some((ffmpeg, ffprobe)) = ffmpeg_available() else {
        eprintln!("skipping: ffmpeg/ffprobe unavailable (external tools, never vendored)");
        return;
    };

    let dir = std::env::temp_dir().join(format!("goya-avc-decode-it-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    let mkv = dir.join("screen-recording.mkv");
    make_fixture(&mkv);

    // Probe: the adapter reports metadata and the opaque sidecar is absent.
    let adapter = goya_avc_decode::FfmpegAdapter::with_executables(ffmpeg, ffprobe);
    let source = adapter.probe(&mkv).expect("probe fixture");
    assert_eq!(source.dimensions(), (WIDTH, HEIGHT));
    assert_eq!(source.frame_rate(), FPS);
    assert_eq!(source.frame_count(), FRAMES);
    assert!(
        source.audio_sidecar().is_none(),
        "silent fixture has no sidecar"
    );
    assert_eq!(source.source_digest().len(), 64, "SHA-256 hex digest");

    // Compile through the full AOT-VC pipeline.
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
            max_checkpoint_interval: goya_avc::MediaClock::new(90, 30).unwrap(),
            max_delta_depth: 8,
            residency_budget_bytes: 1 << 20,
        },
    };
    let (video, stats) = match compile(&source, &params, &profile) {
        Ok(result) => result,
        Err(CompileError::Source(e)) => panic!("decode failed: {e:?}"),
        Err(e) => panic!("compile failed: {e:?}"),
    };

    // FFV1 is lossless: closure sees exactly the three source colours.
    assert_eq!(video.inventory.entries().len(), 3);
    assert_eq!(stats.frames, FRAMES as usize);
    // Temporal persistence: frame 0 declares 32 pixels via SET_ROW;
    // frame 1 re-declares the cursor's row by one SET_RUN; frame 2
    // materialises the leave-transition (SET_RUN) and applies the
    // precomputed enter-transition delta (position-independent reuse).
    assert_eq!(stats.naive_pixel_writes, 96);
    assert_eq!(stats.persistent_pixel_writes, 32 + 1 + 1 + 1);
    assert_eq!(stats.row_events, 4);
    assert_eq!(stats.run_events, 2);
    assert_eq!(stats.delta_events, 1);

    // The timeline's first events are the initial row declarations; the
    // last event is PRESENT.
    let events = video.timeline.events();
    assert!(matches!(
        events.first().unwrap().instruction,
        Instruction::SetRow { .. }
    ));
    assert!(matches!(
        events.last().unwrap().instruction,
        Instruction::Present
    ));

    // Determinism: a second decode+compile is identical.
    let source2 = adapter.probe(&mkv).expect("reprobe fixture");
    let (video2, _) = compile(&source2, &params, &profile).expect("second compile");
    assert_eq!(video.timeline, video2.timeline);
    assert_eq!(source.source_digest(), source2.source_digest());

    let _ = std::fs::remove_file(&mkv);
    let _ = std::fs::remove_dir_all(&dir);
}
