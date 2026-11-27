//! Colour Cueing execution policy.
//!
//! GVI-L already stores a shared palette and expands it when an inventory is
//! opened. This crate keeps a chromatic reference until a fill, prices a
//! dirty rectangle against a full frame (including a front-buffer copy), and
//! refuses a device-local GPU placement while Vulkan is blocked.
//!
//! It is not a new `.gvi` section.

pub mod baseline;
pub mod damage;
pub mod index;
pub mod placement;
pub mod residency;
pub mod retained;
pub mod select;

pub use baseline::{account, time_paths, PathAccount, PresentPath, TimedSample};
pub use damage::{millipoints_to_pixels, MillipointRect};
pub use index::{fill_word, fills_at_paint, resolve_at_fill, ChromaticRef, Palette, ABSENT};
pub use placement::{admit, strategy_for, HardwareTarget, Strategy};
pub use residency::{place, MemoryTier, PlaceError, Placement};
pub use retained::RetainedFrame;
pub use select::{paint_cost_direct, paint_cost_indexed, select_paint, PaintForm};
