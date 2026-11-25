//! Viuda (`GY-VIUDA/0.1`) — webOS-native interpretation of admitted
//! `.gscene` apps.
//!
//! Viuda is a host over the canonical resolver (ADR-RESOLVER-001). It
//! does not fork admission or the D1 machine:
//! * **Semantics.** `goya-resolver` admits the source, executes each D1
//!   action and publishes the revision (`GY-D1-APP/1`).
//! * **Presentation.** `goya-scene-runtime` (the Emperor/Nebula scene
//!   renderer) lays out, focuses, animates and paints. It runs in
//!   *published* mode: it shows the state the resolver published and
//!   executes nothing itself.
//! * **Graphics.** Each `@graphic` is compiled to GY-GVI/0 with
//!   `goya_scene_graphics::compile` when the app loads. Arachnid and Emperor
//!   call the same function at build time, so the inventory bytes are
//!   identical. This is an internal lowering of one loaded source, not an
//!   app-specific executable (directive §5.2, VIU-G02).
//!
//! The runtime binary is the same for every app. A packaged app is that
//! binary, its `.gscene` and its `appinfo.json`.
//!
//! See `docs/viuda/VIUDA-0.md` (Phase 0) and `docs/viuda/VIUDA-1.md`.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use goya_lang::InteractionValue;
use goya_resolver::{Instance, Resolver};
use goya_scene_runtime::{GraphicSpec, Scene, SceneSpec};

pub use goya_scene_runtime::tiny_skia::Pixmap;

/// Contract implemented by this crate.
pub const CONTRACT: &str = "GY-VIUDA/0.1";

/// Fonts Viuda looks for, in order. The first is the webOS TV system UI
/// font, so an app ships no font (directive OBJ-05).
pub const FONTS: [&str; 3] = [
    "/usr/share/fonts/LG_Smart_UI-Regular.ttf",
    "/usr/share/fonts/DroidSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];

/// A remote-control input after platform mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    Ok,
    Digit(u8),
}

/// What one key did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyOutcome {
    /// The trigger of the action the resolver ran, if any.
    pub action: Option<String>,
    /// The revision after the key.
    pub revision: u64,
    /// The D9 announcement the resolver produced for this action.
    pub announcement: Option<String>,
    /// Time spent in the resolver dispatch plus the presentation update.
    pub dispatch: Duration,
}

/// Load-time measurements. They are wall-clock durations on the device
/// that ran the load.
#[derive(Debug, Clone, Default)]
pub struct LoadReport {
    pub contract: String,
    pub digest: [u8; 32],
    pub source_bytes: usize,
    pub actions: usize,
    /// Resolver admission, resolution and start.
    pub resolve: Duration,
    /// Presentation admission, graphics and scene construction.
    pub presentation: Duration,
    /// The part of `presentation` spent compiling `@graphic`s.
    pub graphics_compile: Duration,
    pub graphics: Vec<GraphicReport>,
}

#[derive(Debug, Clone)]
pub struct GraphicReport {
    pub name: String,
    pub bytes: usize,
    pub sha256: String,
    pub states: usize,
    pub projections: usize,
}

/// One running app.
pub struct Viuda {
    instance: Instance,
    scene: Scene,
    triggers: BTreeSet<String>,
    report: LoadReport,
    /// OK pressed on a present button that starts no interaction.
    pub inert_activations: u64,
    /// Resolver refusals, with the trigger that caused them.
    pub errors: Vec<String>,
}

fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn binding(hex: &str) -> Result<[u8; 32], String> {
    let mut out = [0u8; 32];
    if hex.len() != 64 || !hex.is_ascii() {
        return Err(format!("graphic binding {hex:?} is not 32 bytes of hex"));
    }
    for (k, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * k..2 * k + 2], 16)
            .map_err(|_| format!("graphic binding {hex:?} is not hex"))?;
    }
    Ok(out)
}

/// Admit the source for presentation the way Emperor does: GSSH/4, then
/// GSSH/4-live when the source uses live constructs.
fn admit_presentation(source: &str) -> Result<goya_lang::ParsedScene, String> {
    let handoff = match goya_lang::handoff::admit_v4(source) {
        Err(e) if e.code == "GY-E1012" => goya_lang::handoff::admit_v4_live(source),
        other => other,
    }
    .map_err(|e| {
        format!(
            "GY-VIUDA-PRESENT: Viuda presents GSSH/4 and GSSH/4-live scenes; {}: {e}",
            e.code
        )
    })?;
    Ok(handoff.parsed().clone())
}

fn published(instance: &Instance) -> Vec<(String, InteractionValue)> {
    instance
        .state()
        .into_iter()
        .map(|o| (o.key, o.value))
        .collect()
}

impl Viuda {
    /// Admit, resolve and start `source` on a `width` × `height` surface
    /// (one point is one surface pixel).
    ///
    /// The compiled graphics are kept for the life of the process, like the
    /// font. A host loads one app per process.
    pub fn load(
        source: &str,
        font: &'static [u8],
        width: u32,
        height: u32,
    ) -> Result<Viuda, String> {
        let mut report = LoadReport {
            source_bytes: source.len(),
            ..LoadReport::default()
        };

        let t = Instant::now();
        let resolver = Resolver::builder()
            .viewport(f64::from(width), f64::from(height))
            .build()
            .map_err(|d| d.to_string())?;
        let app = resolver.load(source).map_err(|d| d.to_string())?;
        let mut instance = resolver.instantiate(&app).map_err(|d| d.to_string())?;
        instance.start().map_err(|d| d.to_string())?;
        report.contract = app.semantic_contract().to_owned();
        report.digest = app.source_digest();
        let triggers: BTreeSet<String> = app.actions().into_iter().map(|a| a.trigger).collect();
        report.actions = triggers.len();
        report.resolve = t.elapsed();

        let t = Instant::now();
        let parsed = admit_presentation(source)?;
        let g0 = Instant::now();
        let mut graphics = Vec::new();
        for g in &parsed.declarations.graphics {
            let c = goya_scene_graphics::compile(&parsed.scene.name, g)
                .map_err(|why| format!("GY-VIUDA-GRAPHIC: @graphic {}: {why}", g.name))?;
            report.graphics.push(GraphicReport {
                name: c.name.clone(),
                bytes: c.bytes.len(),
                sha256: c.sha256.clone(),
                states: c.states,
                projections: c.projections,
            });
            graphics.push(GraphicSpec {
                name: leak_str(c.name.clone()),
                binding: binding(&c.binding)?,
                radices: leak(c.dims.iter().map(|d| d.1.len()).collect()),
                steps_ms: leak(c.dims.iter().map(|d| d.2.unwrap_or(0)).collect()),
                bytes: leak(c.bytes),
            });
        }
        report.graphics_compile = g0.elapsed();
        let spec = SceneSpec {
            source: "",
            title: leak_str(parsed.scene.name.clone()),
            graphics: leak(graphics),
            // Durable state belongs to the resolver, which classifies durable
            // keys and does not write them yet (ADR-RESOLVER-001 Decision 5).
            durable: false,
            font,
        };
        let mut scene = Scene::from_parsed(parsed, &spec, width, height)?;
        scene.show_published(published(&instance));
        report.presentation = t.elapsed();

        Ok(Viuda {
            instance,
            scene,
            triggers,
            report,
            inert_activations: 0,
            errors: Vec::new(),
        })
    }

    pub fn report(&self) -> &LoadReport {
        &self.report
    }

    pub fn revision(&self) -> u64 {
        self.instance.revision()
    }

    pub fn instance(&self) -> &Instance {
        &self.instance
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Apply one remote key. Arrows move focus between present buttons. OK
    /// and a bound digit run the focused button's interaction through the
    /// resolver, then show what it published.
    pub fn key(&mut self, key: Key) -> KeyOutcome {
        let mut out = KeyOutcome {
            revision: self.instance.revision(),
            ..KeyOutcome::default()
        };
        let target = match key {
            Key::Left => {
                self.scene.move_focus(-1, 0);
                None
            }
            Key::Right => {
                self.scene.move_focus(1, 0);
                None
            }
            Key::Up => {
                self.scene.move_focus(0, -1);
                None
            }
            Key::Down => {
                self.scene.move_focus(0, 1);
                None
            }
            Key::Ok => self.scene.focused_button().map(str::to_owned),
            Key::Digit(d) => self.scene.focus_digit(d).map(str::to_owned),
        };
        if let Some(name) = target {
            self.activate(&name, &mut out);
        }
        out
    }

    fn activate(&mut self, name: &str, out: &mut KeyOutcome) {
        if !self.triggers.contains(name) {
            self.inert_activations += 1;
            return;
        }
        let t = Instant::now();
        match self.instance.activate(&format!("button.{name}.activate")) {
            Ok(outcome) => {
                self.scene.show_published(published(&self.instance));
                out.action = Some(name.to_owned());
                out.revision = outcome.revision;
                out.announcement = outcome.announcement;
            }
            Err(d) => self.errors.push(format!("{name}: {d}")),
        }
        out.dispatch = t.elapsed();
    }

    /// Advance stepped graphics. True while an animation is running.
    pub fn tick(&mut self) -> bool {
        self.scene.tick()
    }

    /// Whether the scene has ambient motion (`fill-to`, `drift-p`).
    pub fn ambient(&self) -> bool {
        self.scene.ambient()
    }

    /// How often ambient motion needs a frame so that no colour level is
    /// skipped. `None` without ambient motion.
    pub fn ambient_interval(&self) -> Option<Duration> {
        self.scene.ambient_interval()
    }

    pub fn render(&self) -> Pixmap {
        self.scene.render()
    }

    /// Lifecycle: the surface is hidden. Input waits until [`Viuda::resume`].
    pub fn suspend(&mut self) -> Result<(), String> {
        self.instance.suspend().map_err(|d| d.to_string())
    }

    pub fn resume(&mut self) -> Result<(), String> {
        self.instance.resume().map_err(|d| d.to_string())
    }

    pub fn terminate(&mut self) -> Result<(), String> {
        self.instance.terminate().map_err(|d| d.to_string())
    }
}

/// The first readable font: `VIUDA_FONT` when set, then [`FONTS`].
pub fn system_font() -> Result<(&'static [u8], String), String> {
    let mut tried = Vec::new();
    let env = std::env::var("VIUDA_FONT").ok();
    for path in env.iter().map(String::as_str).chain(FONTS) {
        match std::fs::read(path) {
            Ok(bytes) => return Ok((leak(bytes), path.to_owned())),
            Err(e) => tried.push(format!("{path}: {e}")),
        }
    }
    Err(format!("GY-VIUDA-FONT: no UI font ({})", tried.join("; ")))
}

#[cfg(target_os = "linux")]
pub mod webos;

#[cfg(test)]
mod tests {
    use super::*;

    const PAIRS: &str = include_str!("../../../apps/pairs.gscene");

    fn load(width: u32, height: u32) -> Viuda {
        let font = system_font().map(|(f, _)| f).unwrap_or_else(|e| panic!("{e}"));
        Viuda::load(PAIRS, font, width, height).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Every resolver key, as the resolver and the scene each hold it.
    fn both_sides(v: &Viuda) -> Vec<(String, String, Option<String>)> {
        v.instance()
            .state()
            .into_iter()
            .map(|o| {
                let shown = v.scene().state().get(&o.key).map(|d| format!("{d:?}"));
                (o.key, format!("{:?}", o.value), shown)
            })
            .collect()
    }

    fn assert_agree(v: &Viuda) {
        for (key, published, shown) in both_sides(v) {
            assert_eq!(shown.as_deref(), Some(published.as_str()), "{key}");
        }
    }

    #[test]
    fn pairs_loads_and_both_sides_agree_on_initial_state() {
        let v = load(1920, 1080);
        assert_eq!(v.report().contract, "GSSH/4-live");
        assert_eq!(v.report().actions, 314);
        assert_eq!(v.report().graphics.len(), 1);
        assert_eq!(v.revision(), 0);
        assert_agree(&v);
    }

    #[test]
    fn ok_dispatches_through_the_resolver_and_the_scene_shows_it() {
        let mut v = load(1920, 1080);
        let out = v.key(Key::Ok);
        assert!(v.errors.is_empty(), "{:?}", v.errors);
        assert_eq!(out.revision, 1, "{out:?}");
        assert!(out.action.is_some());
        assert_agree(&v);
    }

    #[test]
    fn arrows_move_focus_without_publishing() {
        let mut v = load(1920, 1080);
        let first = v.scene().focused_button().map(str::to_owned);
        let out = v.key(Key::Right);
        assert!(out.action.is_none());
        assert_eq!(v.revision(), 0);
        assert_ne!(v.scene().focused_button().map(str::to_owned), first);
    }

    #[test]
    fn a_sequence_publishes_once_per_action_and_stays_in_agreement() {
        let mut v = load(1920, 1080);
        let mut published = 0;
        for k in [
            Key::Ok,
            Key::Right,
            Key::Ok,
            Key::Ok,
            Key::Down,
            Key::Ok,
            Key::Right,
            Key::Ok,
            Key::Ok,
        ] {
            if v.key(k).action.is_some() {
                published += 1;
            }
            assert_agree(&v);
        }
        assert!(v.errors.is_empty(), "{:?}", v.errors);
        assert_eq!(v.revision(), published);
        assert!(published >= 3, "{published}");
    }

    #[test]
    fn suspended_instance_refuses_and_resumes() {
        let mut v = load(1920, 1080);
        v.suspend().unwrap_or_else(|e| panic!("{e}"));
        let out = v.key(Key::Ok);
        assert!(out.action.is_none());
        assert_eq!(v.errors.len(), 1, "{:?}", v.errors);
        v.resume().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(v.key(Key::Ok).revision, 1);
    }

    #[test]
    fn pairs_breathing_background_needs_a_frame_every_1_36_s() {
        // fill #f4f1ea -> #e6edf3 over 30 s: the red channel moves 14
        // levels, so one level takes at least 60 / (14 pi) s.
        let v = load(1920, 1080);
        let i = v.ambient_interval().map(|d| d.as_millis());
        assert_eq!(i, Some(1364));
    }

    #[test]
    fn render_fills_the_surface() {
        let v = load(960, 540);
        let pm = v.render();
        assert_eq!((pm.width(), pm.height()), (960, 540));
    }

    #[test]
    fn graphics_match_the_build_time_compilers() {
        // Arachnid packages the same bytes as graphics/card.gvi
        // (docs/arachnid/ARACHNID-2.md §6).
        let v = load(1920, 1080);
        let g = &v.report().graphics[0];
        assert_eq!(g.name, "card");
        assert!(g.sha256.starts_with("c74eba3b"), "{}", g.sha256);
    }

    #[test]
    fn a_source_that_does_not_admit_is_refused() {
        let font = system_font().map(|(f, _)| f).unwrap_or_else(|e| panic!("{e}"));
        assert!(Viuda::load("this is not a scene", font, 100, 100).is_err());
    }

    #[test]
    fn binding_parses_hex() {
        let hex = "00".repeat(31) + "ff";
        assert_eq!(binding(&hex).map(|b| b[31]), Ok(255));
        assert!(binding("zz").is_err());
    }
}
