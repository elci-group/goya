//! Arachnid: Goya's webOS TV compiler target.
//!
//! Contract `arachnid.webos/0.0.1`. The pipeline is:
//! strict GSSH/3 admission → layout at the 1920×1080 ontology limit →
//! compile-time focus graph (reachability and traps) → web-engine caprichio
//! gate → HTML/CSS/JS emission → `appinfo.json` → deterministic IPK.
//!
//! Supported targets: webOS TV 4.0 (2018, Chromium 53) to webOS TV 26
//! (2026, Chromium 132). Model, platform and engine facts live in
//! `goya_caprichios::webos_engine_caprichio` and
//! `goya_caprichios::webos_tv_models`.

pub mod appinfo;
pub mod apps;
pub mod archive;
pub mod build;
pub mod deploy;
pub mod diagnostics;
pub mod emit;
pub mod focus;
pub mod graphics;
pub mod gvi_js;
pub mod ipk;
pub mod lanlink;
pub mod live;
pub mod motion;
pub mod ontology;
pub mod remote;
pub mod sky;
pub mod splash;
pub mod target;
pub mod traps;

pub const CONTRACT: &str = "arachnid.webos/0.0.1";
