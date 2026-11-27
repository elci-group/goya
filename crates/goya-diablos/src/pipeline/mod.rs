//! Diablos compilation pipeline.

pub mod orchestrate;
pub mod stage;

pub use orchestrate::{CommandKind, Invocation, Run, execute};
pub use stage::{StageResult, Status, DIA_STAGES, base_stages, update_stage, exit_code_from_stages};