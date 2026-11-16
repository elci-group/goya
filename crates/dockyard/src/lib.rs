//! Dockyard compiles one admitted HTTP service into a static executable and an OCI layout.
//!
//! Profile `dockyard.linux.http/0.0.1` is not a scene compiler and not a container runtime.

mod admit;
mod compile;
mod oci;
mod start;

pub use admit::{admit, Diagnostic, Service};
pub use compile::{build, verify, BuildReport, Failure, VerifyReport};
pub use start::{exchange_netns, start, NetnsResponse, StartReport};

pub const PROFILE: &str = "dockyard.linux.http/0.0.1";
pub const TARGET: &str = "x86_64-unknown-linux-gnu";
pub const LINK_MODE: &str = "crt-static";
pub const IMAGE_USER: &str = "65532:65532";
pub const CREATED: &str = "1970-01-01T00:00:00Z";
pub const METHOD: &str = "GET";
pub const STATUS: u16 = 200;
pub const CONTENT_TYPE: &str = "text/plain";
pub const SOURCE_LIMIT: usize = 4096;
pub const BODY_LIMIT: usize = 512;
