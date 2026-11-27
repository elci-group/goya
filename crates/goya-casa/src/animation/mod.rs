//! CSS keyframe animation transformation (directive GOYA-DIS-CASA-001).
//!
//! Purpose: turn `@keyframes` rules from CasaIR into either the GVI-A
//! precomputed trajectory runtime players consume or a typed runtime
//! deferral, reporting every lossy or deferred decision through the CASA
//! diagnostic registry.
//!
//! Progressive scope, by submodule:
//! - [`keyframes`] lifts `@keyframes` IR rules into the typed
//!   [`Animation`] model (normalized offsets, merged duplicates,
//!   ascending order) and classifies per-property value kinds.
//! - [`easing`] parses `animation-timing-function` values into a typed
//!   [`Easing`] and samples them with a deterministic bézier solver.
//! - [`interpolation`] produces the value between two keyframe values at
//!   a progress point; non-interpolable pairs step discretely at 50%.
//! - [`trajectory`] compiles an animation into the fixed-grid [`Sample`]
//!   list (GVI-A framing) with identical consecutive samples removed.
//! - [`classification`] decides [`Classification::Precomputed`] vs
//!   [`Classification::Runtime`] per animation.
//!
//! Determinism: every function here is pure — no time, randomness, or
//! iteration-order dependence; ordered data lives in `BTreeMap`/`BTreeSet`
//! and stable sorts. Identical inputs always produce byte-identical
//! outputs, and malformed values are reported, never guessed at.

pub mod classification;
pub mod easing;
pub mod interpolation;
pub mod keyframes;
pub mod trajectory;

pub use classification::{classify, classify_with_easing, Classification, RuntimeReason};
pub use easing::Easing;
pub use interpolation::{interpolate, InterpolatedValue};
pub use keyframes::{parse_keyframes, property_kind, Animation, AnimationStep, PropertyKind};
pub use trajectory::{sample_animation, Sample, Trajectory, SAMPLE_COUNT};
