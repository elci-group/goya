//! goya-zcode — GY-ZCODE/1 read-only ZCode session monitor.
//!
//! See docs/goya-zcode.md for the contract. The crate is split into:
//! - [`view`]: the UI fact source (projection) — pure logic, unit-tested.
//! - [`agent`]: the NDJSON protocol client for the ZCode CLI `app-server`.

pub mod agent;
pub mod view;
