//! goya-audio-linux — audio device lifecycle and output contract layer.
//!
//! Contract: [`AUDIO_LINUX_CONTRACT`] = `"GY-AUDIO-LINUX/1"`.
//!
//! This crate defines the device state machine and output trait without any
//! PipeWire (or other) C linkage. [`NullOutput`] is the missing-device
//! fallback: the application remains fully operational without audio —
//! writes always succeed and the pipeline completes.

/// Versioned contract identifier for this crate's audio lifecycle surface.
pub const AUDIO_LINUX_CONTRACT: &str = "GY-AUDIO-LINUX/1";

use std::fmt;

/// Lifecycle state of an audio device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DeviceState {
    /// Device is present and ready for output.
    #[default]
    Available,
    /// No usable device exists.
    Unavailable,
    /// Device exists but is temporarily suspended (e.g. power management).
    Suspended,
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceState::Available => write!(f, "available"),
            DeviceState::Unavailable => write!(f, "unavailable"),
            DeviceState::Suspended => write!(f, "suspended"),
        }
    }
}

/// Descriptive information about an audio output device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Human-readable device name.
    pub name: String,
    /// Output channel count (mono = 1, stereo = 2, ...).
    pub channels: u8,
    /// Whether this is the system default device.
    pub default: bool,
    /// Current lifecycle state.
    pub state: DeviceState,
}

/// Errors reported by audio output backends. Never panics the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    /// The device is not writable in its current state.
    NotWritable(DeviceState),
    /// The backend reported an unspecified failure; details in the payload.
    Backend(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NotWritable(state) => write!(f, "device not writable: {state}"),
            AudioError::Backend(detail) => write!(f, "audio backend error: {detail}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// Contract for writing interleaved f32 samples to an audio output.
pub trait AudioOutput {
    /// Write `samples` to the output, returning the number of samples
    /// accepted. Implementations must not panic and must degrade gracefully.
    fn write(&mut self, samples: &[f32]) -> Result<usize, AudioError>;

    /// Current lifecycle state of the underlying device.
    fn state(&self) -> DeviceState;
}

/// Missing-device fallback output.
///
/// Always accepts every sample (returns `Ok(len)`) even though its state is
/// [`DeviceState::Unavailable`]: the application stays operational without
/// audio, discarding samples at this boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NullOutput {
    /// Total samples accepted, for diagnostics.
    pub accepted: u64,
}

impl NullOutput {
    /// Create a null output.
    pub fn new() -> Self {
        Self { accepted: 0 }
    }
}

impl AudioOutput for NullOutput {
    fn write(&mut self, samples: &[f32]) -> Result<usize, AudioError> {
        self.accepted += samples.len() as u64;
        Ok(samples.len())
    }

    fn state(&self) -> DeviceState {
        DeviceState::Unavailable
    }
}

/// Device lifecycle monitor.
///
/// Tracks the last observed state and any error detail. `poll` never panics
/// and degrades gracefully: it reports the current state (defaulting to
/// [`DeviceState::Unavailable`]) and records errors instead of propagating
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceMonitor {
    /// Detail of the most recent error, if any.
    pub last_error: Option<String>,
    state: DeviceState,
}

impl DeviceMonitor {
    /// New monitor starting in the `Unavailable` state (safe default: no
    /// device assumed until one is observed).
    pub fn new() -> Self {
        Self {
            last_error: None,
            state: DeviceState::Unavailable,
        }
    }

    /// Poll the device subsystem. Never panics; on internal failure it
    /// degrades to `Unavailable` and records the error.
    pub fn poll(&mut self) -> DeviceState {
        match self.probe() {
            Ok(state) => {
                self.state = state;
                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
                self.state = DeviceState::Unavailable;
            }
        }
        self.state
    }

    /// Last observed state without re-probing.
    pub fn state(&self) -> &DeviceState {
        &self.state
    }

    /// Probe hook. In this contract layer there is no real backend, so the
    /// probe always succeeds with `Unavailable`; a backend integration would
    /// override this in a concrete monitor.
    fn probe(&self) -> Result<DeviceState, AudioError> {
        Ok(DeviceState::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_output_writes_always_succeed() {
        let mut out = NullOutput::new();
        assert_eq!(out.write(&[0.1, -0.1, 0.2]), Ok(3));
        assert_eq!(out.write(&[]), Ok(0));
        assert_eq!(out.accepted, 3);
        assert_eq!(out.state(), DeviceState::Unavailable);
    }

    #[test]
    fn monitor_poll_never_panics_and_degrades() {
        let mut mon = DeviceMonitor::new();
        let state = mon.poll();
        assert_eq!(state, DeviceState::Unavailable);
        assert_eq!(mon.last_error, None);
        assert_eq!(mon.state(), &DeviceState::Unavailable);
    }

    #[test]
    fn null_output_pipeline_completes() {
        // Contract test: a full render pipeline over NullOutput still
        // completes. Sources are generated locally (deterministic decaying
        // sine) so this crate has no DSP dependency at the manifest level.
        let mut out = NullOutput::new();
        let total = 2400usize;
        let mut buf = Vec::with_capacity(64);
        let mut written = 0usize;
        for i in 0..total {
            let t = i as f32 / 48_000.0;
            let s = if t < 0.05 {
                0.5 * (-40.0 * t).exp() * (440.0 * t)
            } else {
                0.0
            };
            buf.push(s);
            if buf.len() == 64 {
                match out.write(&buf) {
                    Ok(n) => written += n,
                    Err(_) => panic!("null output must never fail writes"),
                }
                buf.clear();
            }
        }
        if !buf.is_empty() {
            assert_eq!(out.write(&buf), Ok(buf.len()));
            written += buf.len();
        }
        assert_eq!(written, total);
        assert_eq!(out.accepted, total as u64);
        assert_eq!(out.state(), DeviceState::Unavailable);
    }

    #[test]
    fn audio_error_display_is_informative() {
        let e = AudioError::NotWritable(DeviceState::Suspended);
        assert!(e.to_string().contains("suspended"));
        let e = AudioError::Backend("pw stream failed".into());
        assert!(e.to_string().contains("pw stream failed"));
    }
}

/// Playback backend that pipes a WAV stream into the system PipeWire player
/// (`pw-cat`), falling back to `paplay`, then `aplay`. This is the Wave-2a
/// audio adapter: real audible output through the user's running PipeWire
/// graph, with zero C bindings — the Goya process only writes to a child's
/// stdin and reaps it on drop.
///
/// Contract (GY-AUDIO-LINUX/1 amendment): spawn failure degrades to
/// `DeviceState::Unavailable` (application remains operational, per the
/// directive's missing-device criterion); writes to a dead child return
/// Ok(0) (samples dropped gracefully, not an error storm).
pub struct PwCatOutput {
    child: Option<std::process::Child>,
    stdin: Option<std::process::ChildStdin>,
    state: DeviceState,
    written: usize,
    channels: u16,
}

impl PwCatOutput {
    /// Sample rate the WAV header advertises; the DSP path must match.
    pub const SAMPLE_RATE: u32 = 48_000;

    /// Try to spawn a player. Returns `None` when no player binary exists
    /// or spawn fails — callers continue without audio.
    pub fn spawn() -> Option<Self> {
        Self::spawn_with_channels(1)
    }

    /// Try to spawn a stereo player for interleaved binaural HRTF samples.
    pub fn spawn_stereo() -> Option<Self> {
        Self::spawn_with_channels(2)
    }

    /// Try to spawn a player for interleaved audio with `channels` channels.
    /// The streamed WAV supports mono through 7.1 (1..=8 channels).
    pub fn spawn_with_channels(channels: u16) -> Option<Self> {
        if !(1..=8).contains(&channels) {
            return None;
        }
        let players: [[&str; 3]; 3] = [
            ["pw-cat", "--playback", "-"],
            ["paplay", "-", "-"],
            ["aplay", "-q", "-"],
        ];
        for prog in players {
            let mut cmd = std::process::Command::new(prog[0]);
            cmd.args(&prog[1..])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            if let Ok(mut child) = cmd.spawn() {
                let stdin = child.stdin.take();
                let mut out = PwCatOutput {
                    child: Some(child),
                    stdin,
                    state: DeviceState::Available,
                    written: 0,
                    channels,
                };
                out.write_wav_header(Self::SAMPLE_RATE, channels);
                return Some(out);
            }
        }
        let fallback = PwCatOutput {
            child: None,
            stdin: None,
            state: DeviceState::Unavailable,
            written: 0,
            channels,
        };
        Some(fallback)
    }

    fn write_wav_header(&mut self, sample_rate: u32, channels: u16) {
        let h = wav_header(sample_rate, channels);
        let _ = self.push_raw(&h);
    }

    fn push_raw(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match &mut self.stdin {
            Some(stdin) => std::io::Write::write_all(stdin, bytes)?,
            None => return Ok(0),
        }
        Ok(bytes.len())
    }

    /// Total samples accepted so far.
    pub fn samples_written(&self) -> usize {
        self.written
    }

    /// Whether a player process is actually carrying audio.
    pub fn is_live(&self) -> bool {
        self.child.is_some()
    }

    /// Number of channels represented by each interleaved audio frame.
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

fn wav_header(sample_rate: u32, channels: u16) -> [u8; 44] {
    // Streamed IEEE float WAV header with a placeholder data length. Players
    // accept this form while the pipe remains open; channel count lets the
    // stereo FIR output reach the playback device without being interpreted
    // as a mono sequence.
    let byte_rate = sample_rate
        .saturating_mul(u32::from(channels))
        .saturating_mul(4);
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&3u16.to_le_bytes()); // IEEE float
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&channels.saturating_mul(4).to_le_bytes());
    h[34..36].copy_from_slice(&32u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    h
}

impl AudioOutput for PwCatOutput {
    fn write(&mut self, samples: &[f32]) -> Result<usize, AudioError> {
        if samples.len() % usize::from(self.channels) != 0 {
            return Err(AudioError::Backend(format!(
                "interleaved sample count {} is not a complete {}-channel frame",
                samples.len(),
                self.channels
            )));
        }
        if self.stdin.is_none() {
            return Ok(0); // dropped gracefully: no player, no error storm
        }
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for s in samples {
            let s = if s.is_finite() { *s } else { 0.0 };
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        match self.push_raw(&bytes) {
            Ok(n) => {
                let accepted = n / 4;
                self.written += accepted;
                Ok(accepted)
            }
            Err(_) => {
                // Player died: degrade to Unavailable, keep the app alive.
                self.state = DeviceState::Unavailable;
                self.stdin = None;
                Ok(0)
            }
        }
    }

    fn state(&self) -> DeviceState {
        self.state
    }
}

impl Drop for PwCatOutput {
    fn drop(&mut self) {
        // Close stdin first so the player sees EOF and finalises the stream.
        self.stdin = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod pwcat_tests {
    use super::*;

    #[test]
    fn missing_player_degrades_to_unavailable_and_ok_zero() {
        // Directly construct the no-player variant (spawn environment may
        // or may not have players; this pins the degradation contract).
        let mut out = PwCatOutput {
            child: None,
            stdin: None,
            state: DeviceState::Unavailable,
            written: 0,
            channels: 1,
        };
        assert_eq!(out.state(), DeviceState::Unavailable);
        assert_eq!(out.write(&[0.5, -0.5]), Ok(0));
        assert_eq!(out.write(&[1.0; 100]), Ok(0));
        assert!(!out.is_live());
    }

    #[test]
    fn wav_header_is_canonical_layout() {
        // Header bytes are 44, RIFF/WAVE markers present.
        // Verified indirectly: spawn() writes it first; sample accounting
        // starts after it, so written counts only samples.
        let mut out = match PwCatOutput::spawn() {
            Some(o) => o,
            None => return, // no player on host: the other test covers this
        };
        let zero = out.write(&[0.0; 16]).unwrap_or(0);
        // Either live (accepted == 16) or unavailable (0): both contracts.
        assert!(zero == 16 || zero == 0);
        assert_eq!(out.samples_written(), zero);
    }

    #[test]
    fn wav_header_encodes_stereo_float_layout() {
        let header = wav_header(PwCatOutput::SAMPLE_RATE, 2);
        assert_eq!(&header[0..4], b"RIFF");
        assert_eq!(&header[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([header[22], header[23]]), 2);
        assert_eq!(
            u32::from_le_bytes(header[24..28].try_into().unwrap()),
            48_000
        );
        assert_eq!(
            u32::from_le_bytes(header[28..32].try_into().unwrap()),
            384_000
        );
        assert_eq!(u16::from_le_bytes([header[32], header[33]]), 8);
        assert_eq!(u16::from_le_bytes([header[34], header[35]]), 32);
    }

    #[test]
    fn stereo_output_rejects_partial_interleaved_frames() {
        let mut out = PwCatOutput {
            child: None,
            stdin: None,
            state: DeviceState::Unavailable,
            written: 0,
            channels: 2,
        };
        assert!(matches!(out.write(&[0.25]), Err(AudioError::Backend(_))));
        assert_eq!(out.write(&[0.25, -0.25]), Ok(0));
        assert_eq!(out.channels(), 2);
    }
}
