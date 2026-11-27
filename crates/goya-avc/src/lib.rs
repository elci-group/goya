//! goya-avc — Ahead-of-Time Video Compilation (AOT-VC) contracts,
//! directive GOYA-GFX-AVC-001.
//!
//! Phase 0 scope (ADR-AVC-001): the **logical contract** — the fixed-point
//! media clock, the temporal model (checkpoints and scheduled events), the
//! pixel-state record (fill, trims, light, per-record exactness), the
//! colour-inventory closure rule, the trim topology, the instruction
//! classes, the compilation profiles and the verification classes. No
//! binary encoding and no runtime live here yet: container sections
//! (`TIML`, `EVNT`, …) are proposals gated on Phase 2 and `.gvi` format
//! version negotiation, and the schedule/resolve/dispatch/present runtime
//! is out of Phase-0 scope (ADR-AVC-001, Consequences).
//!
//! Architectural position: AOT-VC is a **temporal derivation family over
//! GVI-R**, in the same relation to the GVI inventory as AVR (GOYA-GFX-AVR-001)
//! is a spatial one. The finite, addressable state model, budgets,
//! certification and hashing machinery are GVI's; this crate adds the media
//! clock and the temporal program, and inherits the bounded-computation
//! rule: continuous video colour either closes under declared bounds or is
//! rejected with an actionable diagnostic (F2, decision 2).

#![forbid(unsafe_code)]

pub mod certification;
pub mod clock;
pub mod colour;
pub mod compile;
pub mod delta;
pub mod ingestion;
pub mod instructions;
pub mod pixel;
pub mod profile;
pub mod runtime;
pub mod tags;
pub mod temporal;
pub mod topology;

/// Directive that defined this contract family.
pub const DIRECTIVE_ID: &str = "GOYA-GFX-AVC-001";

/// Logical contract version. Not a binary format version.
pub const CONTRACT_VERSION: &str = "avc-contract/0.0.1";

pub use certification::VerificationClass;
pub use clock::MediaClock;
pub use colour::{
    scale_entry, within_tolerance, ClosureVerdict, ColourEntry, ColourInventory, PaletteBound,
    RejectionDiagnostic, Tolerance, WorkingSpace,
};
pub use compile::{compile, CompileError, CompileStats, CompiledVideo, RowId};
pub use delta::{ContentId, PixelChange, RowDelta};
pub use ingestion::{
    srgb8_to_linear_q24_8, DecodedFrame, DecodedFrameFormat, IngestionError, IngestionParameters,
    RgbPixels, SourceError, VideoSource, LINEAR_UNITY,
};
pub use instructions::{
    Instruction, InstructionBudget, ProgramError, RegionRect, Target, MAX_DELTA_DEPTH,
};
pub use pixel::{ApproximationCause, ColourIndex, Exactness, Light, PixelState, MAX_TRIMS};
pub use profile::{CompilationBudgets, CompilationProfile, ProfileKind};
pub use runtime::{
    decode_checkpoint_payload, encode_checkpoint_payload, play, play_from, CheckpointRow,
    PresentedFrame, ReferenceRuntime, RuntimeError,
};
pub use tags::{ProposedTag, EXISTING_CONTAINER_TAGS};
pub use temporal::{Checkpoint, EventId, ScheduledEvent, TemporalStateId, Timeline, TimelineError};
pub use topology::{TopologyError, TrimDefinitionId, TrimDirection};
