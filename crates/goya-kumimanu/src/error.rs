use std::fmt;

use crate::phase::Phase;

/// Failures a host can report without reading protocol payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Connect(String),
    Protocol {
        code: u8,
        sequence: u16,
    },
    ProtocolLimit,
    UnsupportedVisual {
        depth: u8,
        bits_per_pixel: u8,
    },
    InvalidConfig(&'static str),
    StaleWindow,
    WrongPhase {
        current: Phase,
        attempted: &'static str,
    },
    SizeMismatch {
        window_width: u16,
        window_height: u16,
        frame_width: u16,
        frame_height: u16,
    },
    PixelBuffer(&'static str),
    EventQueueSaturated {
        capacity: usize,
    },
    ServerDisconnect,
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(message) => write!(formatter, "connect: {message}"),
            Self::Protocol { code, sequence } => {
                write!(formatter, "protocol error {code} at sequence {sequence}")
            }
            Self::ProtocolLimit => write!(formatter, "too many protocol errors"),
            Self::UnsupportedVisual {
                depth,
                bits_per_pixel,
            } => write!(
                formatter,
                "visual depth {depth} at {bits_per_pixel} bits is not a supported truecolor format"
            ),
            Self::InvalidConfig(reason) => write!(formatter, "window config: {reason}"),
            Self::StaleWindow => write!(formatter, "window id is not the live window"),
            Self::WrongPhase {
                current,
                attempted,
            } => write!(formatter, "cannot {attempted} from {current:?}"),
            Self::SizeMismatch {
                window_width,
                window_height,
                frame_width,
                frame_height,
            } => write!(
                formatter,
                "frame {frame_width}x{frame_height} does not match window {window_width}x{window_height}"
            ),
            Self::PixelBuffer(reason) => write!(formatter, "pixel buffer: {reason}"),
            Self::EventQueueSaturated { capacity } => {
                write!(formatter, "event queue reached {capacity}")
            }
            Self::ServerDisconnect => write!(formatter, "X server disconnected"),
            Self::Io(message) => write!(formatter, "socket: {message}"),
        }
    }
}

impl std::error::Error for Error {}
