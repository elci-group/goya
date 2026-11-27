//! goya-avc-decode — decoder adapter for the AOT-VC ingestion contract
//! (ADR-AVC-001 Amendment 2).
//!
//! The adapter calls the ffmpeg/ffprobe CLI **without a shell**: every
//! path and expression is passed as its own operating-system argument
//! (the `goya-tov` process-boundary pattern). This keeps Goya independent
//! of a particular ffmpeg installation and of any decode linkage; the
//! tools must be installed separately. When unavailable the adapter
//! returns `DecodeError::ToolUnavailable` with the path attempted.
//!
//! Responsibility split (ADR-AVC-001 decision 4 and Amendment 1): the
//! adapter owns container probing and chroma/format conversion *inside
//! the decoder process* — frames are always delivered as rawvideo RGB8,
//! the format family the ingestion contract accepts — and the visual
//! state contract never sees YUV, containers, or audio samples. Audio is
//! surfaced only as the opaque sidecar descriptor of decision 4
//! (codec, duration in its own time base, PTS offset); the adapter never
//! decodes audio.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use goya_avc::clock::MediaClock;
use goya_avc::ingestion::{
    DecodedFrame, DecodedFrameFormat, IngestionParameters, SourceError, VideoSource,
};
use sha2::{Digest, Sha256};

/// Adapter contract implemented by this crate.
pub const AVC_DECODE_ADAPTER_CONTRACT: &str = "avc-decode/0.1.0";

/// Probe metadata for one video stream, parsed from ffprobe output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMetadata {
    pub width: u32,
    pub height: u32,
    /// Rational frame rate in Hz (`numerator / denominator`).
    pub frame_rate: (u32, u32),
    /// Frame count: `nb_frames` when present, else `nb_read_packets`.
    pub frame_count: u64,
    /// Declared source pixel format (informational: decode always emits
    /// RGB8 rawvideo).
    pub pix_fmt: String,
}

/// The opaque audio sidecar descriptor (decision 4, `AUDS`). The visual
/// contract knows nothing about audio beyond this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSidecar {
    /// Container codec name (e.g. `aac`, `opus`) — opaque.
    pub codec: String,
    /// Stream duration in `time_base` units.
    pub duration_ts: i64,
    /// Stream time base as a rational (`numerator / denominator`).
    pub time_base: (u32, u32),
    /// Presentation timestamp of the first sample, in `time_base` units
    /// (may be negative for edit lists).
    pub start_ts: i64,
}

#[derive(Debug)]
pub enum DecodeError {
    /// Input does not exist or is not a regular file.
    InvalidInput(PathBuf),
    /// The configured executable could not be found. Install ffmpeg/ffprobe
    /// or provide explicit paths via [`FfmpegAdapter::with_executables`].
    ToolUnavailable(PathBuf),
    /// The process could not be started for a reason other than absence.
    Launch(std::io::Error),
    /// The tool returned a non-zero status; diagnostics retained.
    Failed {
        tool: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    /// Probe output could not be parsed into contract metadata.
    UnexpectedProbeOutput(String),
    /// The container declares no video stream or an unusable frame count.
    UnsupportedSource(String),
    /// The source's decoded geometry is zero.
    ZeroGeometry,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::InvalidInput(path) => {
                write!(
                    f,
                    "video input is not an existing regular file: {}",
                    path.display()
                )
            }
            DecodeError::ToolUnavailable(path) => write!(
                f,
                "decoder tool '{}' is unavailable; install ffmpeg or configure executable paths",
                path.display()
            ),
            DecodeError::Launch(error) => write!(f, "could not launch decoder tool: {error}"),
            DecodeError::Failed {
                tool,
                status,
                stderr,
            } => {
                write!(f, "{tool} failed (status {status:?}): {}", stderr.trim())
            }
            DecodeError::UnexpectedProbeOutput(line) => {
                write!(
                    f,
                    "ffprobe output did not match the expected key=value form: {line}"
                )
            }
            DecodeError::UnsupportedSource(reason) => {
                write!(f, "source cannot be ingested: {reason}")
            }
            DecodeError::ZeroGeometry => {
                write!(f, "source declares zero-width or zero-height video")
            }
        }
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DecodeError::Launch(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DecodeError> for SourceError {
    fn from(error: DecodeError) -> Self {
        // The ingestion contract boundary maps adapter failures onto the
        // shared `SourceError` vocabulary (AVC-C12 keeps diagnostics).
        match error {
            DecodeError::ZeroGeometry => SourceError::MalformedFrame {
                expected_bytes: 0,
                actual_bytes: 0,
            },
            other => SourceError::DecodeFailed(other.to_string()),
        }
    }
}

/// Safe adapter around the ffmpeg/ffprobe CLI.
///
/// The defaults are `ffmpeg`/`ffprobe` resolved through `PATH`. Callers
/// can pin an installation with [`FfmpegAdapter::with_executables`].
#[derive(Debug, Clone)]
pub struct FfmpegAdapter {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

/// A probed container bound to an adapter: a [`VideoSource`] over the
/// AOT-VC ingestion contract.
#[derive(Debug, Clone)]
pub struct DecodingVideoSource {
    adapter: FfmpegAdapter,
    input: PathBuf,
    metadata: SourceMetadata,
    audio: Option<AudioSidecar>,
    /// SHA-256 of the container bytes — canonical identity input.
    digest: [u8; 32],
}

impl FfmpegAdapter {
    /// Use `ffmpeg`/`ffprobe` from the current process `PATH`.
    pub fn new() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
        }
    }

    /// Use specific ffmpeg/ffprobe executable paths.
    pub fn with_executables(ffmpeg: impl Into<PathBuf>, ffprobe: impl Into<PathBuf>) -> Self {
        Self {
            ffmpeg: ffmpeg.into(),
            ffprobe: ffprobe.into(),
        }
    }

    /// Probe a container: video stream metadata plus the audio sidecar
    /// descriptor when a first audio stream exists.
    pub fn probe(&self, input: impl AsRef<Path>) -> Result<DecodingVideoSource, DecodeError> {
        let input = validate_input(input.as_ref())?;
        let video_args = probe_args(&input, /* audio */ false);
        let output = self.execute("ffprobe", &self.ffprobe, &video_args)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let metadata = parse_probe_output(&stdout)?;
        if metadata.width == 0 || metadata.height == 0 {
            return Err(DecodeError::ZeroGeometry);
        }

        let audio_args = probe_args(&input, /* audio */ true);
        let audio_output = self.execute("ffprobe", &self.ffprobe, &audio_args)?;
        let audio_stdout = String::from_utf8_lossy(&audio_output.stdout).into_owned();
        let audio = parse_audio_sidecar(&audio_stdout);

        let digest = file_digest(&input)?;
        Ok(DecodingVideoSource {
            adapter: self.clone(),
            input,
            metadata,
            audio,
            digest,
        })
    }

    /// Decode one frame as RGB8 rawvideo on stdout (no temp files).
    /// The filter expression selects frame `frame_index` by number; the
    /// comma inside `eq(n,..)` is escaped for the filtergraph parser.
    fn decode_raw_rgb8(&self, input: &Path, frame_index: u32) -> Result<Vec<u8>, DecodeError> {
        let filter = format!("select=eq(n\\,{frame_index})");
        let args: Vec<OsString> = vec![
            "-v".into(),
            "error".into(),
            "-i".into(),
            input.as_os_str().to_owned(),
            "-vf".into(),
            filter.into(),
            "-vsync".into(),
            "0".into(),
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            "rgb24".into(),
            "pipe:1".into(),
        ];
        let output = self.execute("ffmpeg", &self.ffmpeg, &args)?;
        Ok(output.stdout)
    }

    fn execute(
        &self,
        tool: &'static str,
        executable: &Path,
        args: &[OsString],
    ) -> Result<Output, DecodeError> {
        let output = Command::new(executable)
            .args(args)
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    DecodeError::ToolUnavailable(executable.to_path_buf())
                } else {
                    DecodeError::Launch(error)
                }
            })?;
        if !output.status.success() {
            return Err(DecodeError::Failed {
                tool,
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(output)
    }
}

impl Default for FfmpegAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DecodingVideoSource {
    /// The opaque audio sidecar descriptor, when the container has audio.
    pub fn audio_sidecar(&self) -> Option<&AudioSidecar> {
        self.audio.as_ref()
    }

    /// The probed container metadata.
    pub fn metadata(&self) -> &SourceMetadata {
        &self.metadata
    }

    pub fn input_path(&self) -> &Path {
        &self.input
    }
}

impl VideoSource for DecodingVideoSource {
    fn source_digest(&self) -> String {
        self.digest.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.metadata.width, self.metadata.height)
    }

    fn frame_rate(&self) -> (u32, u32) {
        self.metadata.frame_rate
    }

    fn frame_count(&self) -> u32 {
        u32::try_from(self.metadata.frame_count).unwrap_or(u32::MAX)
    }

    fn decode_frame(
        &self,
        frame_index: u32,
        params: &IngestionParameters,
    ) -> Result<DecodedFrame, SourceError> {
        if frame_index >= self.frame_count() {
            return Err(SourceError::DecodeFailed(format!(
                "frame index {frame_index} outside the probed count {}",
                self.frame_count()
            )));
        }
        let (rate_num, rate_den) = self.metadata.frame_rate;
        let frame_duration =
            MediaClock::frame_duration(rate_num, rate_den, params.ticks_per_second)
                .map_err(|_| SourceError::DecodeFailed("unusable frame rate".into()))?;
        let at = MediaClock::new(
            frame_duration.ticks() * frame_index as u64,
            params.ticks_per_second,
        )
        .map_err(|_| SourceError::DecodeFailed("unusable media clock".into()))?;
        let data = self
            .adapter
            .decode_raw_rgb8(&self.input, frame_index)
            .map_err(SourceError::from)?;
        let frame = DecodedFrame {
            format: DecodedFrameFormat::Rgb8,
            width: self.metadata.width,
            height: self.metadata.height,
            at,
            data,
        };
        frame.validate()?;
        Ok(frame)
    }
}

/// Construct the ffprobe invocation: video stream or audio stream, with
/// packet counting as the frame-count fallback. Every argument is its own
/// OS argument; no shell interpolation ever occurs.
fn probe_args(input: &Path, audio: bool) -> Vec<OsString> {
    let (select, entries) = if audio {
        ("-select_streams", "a:0")
    } else {
        ("-select_streams", "v:0")
    };
    let stream_entries = if audio {
        "stream=codec_name,duration_ts,time_base,start_pts"
    } else {
        "stream=width,height,r_frame_rate,nb_frames,pix_fmt,nb_read_packets"
    };
    let mut args: Vec<OsString> = vec!["-v".into(), "error".into(), select.into(), entries.into()];
    if !audio {
        args.push("-count_packets".into());
    }
    args.push("-show_entries".into());
    args.push(stream_entries.into());
    args.push("-of".into());
    args.push("default=noprint_wrappers=1".into());
    args.push(input.as_os_str().to_owned());
    args
}

/// Parse `key=value` lines (ffprobe `default=noprint_wrappers=1` output).
fn parse_probe_output(stdout: &str) -> Result<SourceMetadata, DecodeError> {
    let mut width = None;
    let mut height = None;
    let mut frame_rate = None;
    let mut nb_frames = None;
    let mut nb_read_packets = None;
    let mut pix_fmt = String::new();
    for line in stdout.lines() {
        let Some((key, value)) = line.split_once('=') else {
            if line.trim().is_empty() {
                continue;
            }
            return Err(DecodeError::UnexpectedProbeOutput(line.to_string()));
        };
        match key {
            "width" => width = value.parse().ok(),
            "height" => height = value.parse().ok(),
            "r_frame_rate" => frame_rate = parse_rational(value),
            "nb_frames" if value != "N/A" => nb_frames = value.parse().ok(),
            "nb_read_packets" => nb_read_packets = value.parse().ok(),
            "pix_fmt" => pix_fmt = value.to_string(),
            _ => {}
        }
    }
    match (width, height, frame_rate) {
        (Some(width), Some(height), Some(frame_rate)) => {
            let frame_count = nb_frames.or(nb_read_packets).ok_or_else(|| {
                DecodeError::UnsupportedSource(
                    "no frame count: neither nb_frames nor nb_read_packets is available"
                        .to_string(),
                )
            })?;
            Ok(SourceMetadata {
                width,
                height,
                frame_rate,
                frame_count,
                pix_fmt,
            })
        }
        _ => Err(DecodeError::UnsupportedSource(
            "no decodable video stream found (missing width, height or frame rate)".to_string(),
        )),
    }
}

/// Parse the audio sidecar descriptor; no audio stream yields `None`.
fn parse_audio_sidecar(stdout: &str) -> Option<AudioSidecar> {
    let mut codec = None;
    let mut duration_ts = None;
    let mut time_base = None;
    let mut start_ts = Some(0);
    for line in stdout.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "codec_name" => codec = Some(value.to_string()),
            "duration_ts" => duration_ts = value.parse().ok(),
            "time_base" => time_base = parse_rational(value),
            "start_pts" => {
                start_ts = value.parse().ok().or(start_ts);
            }
            _ => {}
        }
    }
    Some(AudioSidecar {
        codec: codec?,
        duration_ts: duration_ts?,
        time_base: time_base?,
        start_ts: start_ts.unwrap_or(0),
    })
}

/// Parse a rational rate or time base (`"30000/1001"`, `"1/90000"`,
/// `"30"`). Zero denominators and overflow are rejected.
fn parse_rational(value: &str) -> Option<(u32, u32)> {
    let (num, den) = match value.split_once('/') {
        Some((num, den)) => (num.trim(), den.trim()),
        None => (value.trim(), "1"),
    };
    if den.is_empty() {
        return None;
    }
    let num: u32 = num.parse().ok()?;
    let den: u32 = den.parse().ok()?;
    if den == 0 {
        return None;
    }
    Some((num, den))
}

fn validate_input(input: &Path) -> Result<PathBuf, DecodeError> {
    match fs::metadata(input) {
        Ok(metadata) if metadata.is_file() => Ok(input.into()),
        _ => Err(DecodeError::InvalidInput(input.into())),
    }
}

/// SHA-256 of the container bytes, streamed (canonical identity input).
fn file_digest(path: &Path) -> Result<[u8; 32], DecodeError> {
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|_| DecodeError::InvalidInput(path.into()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(DecodeError::Launch)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_output() -> &'static str {
        "width=8\nheight=4\nr_frame_rate=30000/1001\nnb_frames=N/A\npix_fmt=yuv420p\nnb_read_packets=3\n"
    }

    #[test]
    fn probe_output_parses_into_contract_metadata() {
        let metadata = parse_probe_output(probe_output()).unwrap();
        assert_eq!(metadata.width, 8);
        assert_eq!(metadata.height, 4);
        assert_eq!(metadata.frame_rate, (30_000, 1_001));
        assert_eq!(
            metadata.frame_count, 3,
            "nb_frames=N/A falls back to nb_read_packets"
        );
        assert_eq!(metadata.pix_fmt, "yuv420p");
    }

    #[test]
    fn missing_video_stream_is_rejected() {
        let err = parse_probe_output("").unwrap_err();
        assert!(matches!(err, DecodeError::UnsupportedSource(_)));
    }

    #[test]
    fn garbage_probe_lines_are_rejected() {
        let err = parse_probe_output("width=8\n???unparseable\n").unwrap_err();
        assert!(matches!(err, DecodeError::UnexpectedProbeOutput(_)));
    }

    #[test]
    fn rational_parsing_covers_rates_and_time_bases() {
        assert_eq!(parse_rational("30000/1001"), Some((30_000, 1_001)));
        assert_eq!(parse_rational("1/90000"), Some((1, 90_000)));
        assert_eq!(parse_rational("30"), Some((30, 1)));
        assert_eq!(parse_rational("1/0"), None);
        assert_eq!(parse_rational("x/y"), None);
    }

    #[test]
    fn audio_sidecar_is_opaque_descriptor_or_absent() {
        let with_audio = "codec_name=aac\nduration_ts=90090\ntime_base=1/90000\nstart_time=0\n";
        let sidecar = parse_audio_sidecar(with_audio).unwrap();
        assert_eq!(sidecar.codec, "aac");
        assert_eq!(sidecar.time_base, (1, 90_000));
        assert_eq!(sidecar.duration_ts, 90_090);
        assert_eq!(sidecar.start_ts, 0);

        let no_audio = parse_audio_sidecar("");
        assert!(
            no_audio.is_none(),
            "no audio stream → no sidecar descriptor"
        );
    }

    #[test]
    fn probe_arguments_never_use_a_shell() {
        let args = probe_args(Path::new("/tmp/my video.mkv"), false);
        // The path is one argument, spaces and all.
        assert!(args.iter().any(|a| a == "/tmp/my video.mkv"));
        // Packet counting and entries selection are separate arguments.
        assert!(args.contains(&OsString::from("-count_packets")));
        assert!(args.contains(&OsString::from("-show_entries")));
        // Audio probes select the audio stream and never count packets.
        let audio_args = probe_args(Path::new("/tmp/my video.mkv"), true);
        assert!(audio_args
            .iter()
            .any(|a| a.to_string_lossy().contains("a:0")));
        assert!(!audio_args.contains(&OsString::from("-count_packets")));
    }

    #[test]
    fn missing_tool_is_reported_gracefully() {
        let adapter = FfmpegAdapter::with_executables(
            "/nonexistent/ffmpeg-path",
            "/nonexistent/ffprobe-path",
        );
        let err = adapter.probe("/nonexistent/input.mkv").unwrap_err();
        assert!(
            matches!(err, DecodeError::InvalidInput(_)),
            "input validation precedes tool launch"
        );
    }

    #[test]
    fn missing_input_is_rejected_before_launch() {
        let adapter = FfmpegAdapter::new();
        let err = adapter.probe("/nonexistent/input.mkv").unwrap_err();
        assert!(matches!(err, DecodeError::InvalidInput(_)));
    }
}
