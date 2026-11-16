//! Fail-closed boundary for a future verified Frampton browser adapter.
//!
//! The observed Frampton implementation is server-oriented and does not
//! establish a browser presentation API. This crate performs no lowering and
//! emits no assets until an integration contract has been verified.

use std::fmt;

/// Stable classification of an adapter failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterErrorCode {
    /// No verified Frampton browser integration is available.
    BrowserIntegrationUnavailable,
}

/// Structured failure, suitable for conversion into a compiler diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdapterError {
    pub code: AdapterErrorCode,
    pub message: &'static str,
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for AdapterError {}

/// Input and output contracts are supplied by integration; this boundary
/// does not define a competing presentation IR.
pub trait PresentationAdapter<Input: ?Sized> {
    type Output;
    fn lower(&self, input: &Input) -> Result<Self::Output, AdapterError>;
}

/// Current implementation: browser Frampton support is unavailable.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableAdapter;

/// Uninhabited output: this implementation cannot produce an artifact.
#[derive(Debug)]
pub enum UnavailableOutput {}

impl<Input: ?Sized> PresentationAdapter<Input> for UnavailableAdapter {
    type Output = UnavailableOutput;

    fn lower(&self, _input: &Input) -> Result<Self::Output, AdapterError> {
        Err(AdapterError {
            code: AdapterErrorCode::BrowserIntegrationUnavailable,
            message: "Frampton browser integration is unavailable; no verified browser presentation interface is configured",
        })
    }
}
