//! goya-music-player — MUSIC PLAYER exemplar Goya application.
//!
//! The UI is 100% declared in `apps/music_player.gscene` (GY-LANG/1
//! RUGID-style syntax, extent (100, 140) pt → 400 x 560 px window at
//! 4 px/pt). This host is the behavioural half: a deterministic playlist
//! state machine (5 tracks, fixed durations), transport and volume
//! buttons wired through scene hit-testing, a display overridden every
//! frame (track title, 20-char ASCII progress bar, volume readout), and
//! an optional short tone per press via goya-audio (ARSEM-free simple tap).
//!
//!   cargo run --bin music_player -- --live [--sound]
//!
//! Without `--live`, a scripted acceptance sequence runs headless and
//! prints PASS/FAIL per check:
//!   - play → tick(10) advances position by 10
//!   - pause → tick does not advance
//!   - next from last track wraps to first
//!   - prev from first track wraps to last
//!   - vol+ past 100 clamps to 100, vol- below 0 clamps to 0
//!   - track end auto-advances (position duration-1 + tick → next, pos 0)
//!   - progress bar string exact for a known position (50% → 10 filled)

use goya_audio_dsp::{limit, Limiter, Tap};
use goya_audio_linux::{AudioOutput, PwCatOutput};
use goya_lang::{hit_test, layout, DrawNode, SceneStore};
use goya_runtime::Primitive;
use goya_wayland::GoyaWindow;

const SCENE_PATH: &str = "apps/music_player.gscene";
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 560;
const SCENE_W_PT: f64 = 100.0;
const SCENE_H_PT: f64 = 140.0;

/// Deterministic playlist state machine. Pure logic — no rendering — so
/// the acceptance sequence can drive it headless.
struct Player {
    /// Index into [`TRACK_TITLES`] / [`TRACK_DURATIONS`].
    track: usize,
    /// Playback position within the current track, in seconds.
    pos: u64,
    playing: bool,
    /// 0..=100
    volume: i64,
}

const TRACK_TITLES: [&str; 5] = [
    "nocturne one",
    "ember drift",
    "hidden valley",
    "golden hour",
    "velvet rain",
];
const TRACK_DURATIONS: [u64; 5] = [180, 210, 195, 240, 165];
const BAR_WIDTH: usize = 20;

impl Default for Player {
    fn default() -> Self {
        Player {
            track: 0,
            pos: 0,
            playing: false,
            volume: 50,
        }
    }
}

impl Player {
    fn duration(&self) -> u64 {
        TRACK_DURATIONS[self.track]
    }

    fn title(&self) -> &'static str {
        TRACK_TITLES[self.track]
    }

    /// Advance one second of playback. Only moves while playing; at the
    /// end of a track, auto-advances (wrapping) to the next at position 0.
    fn tick(&mut self) {
        if !self.playing {
            return;
        }
        self.pos += 1;
        if self.pos >= self.duration() {
            self.pos = 0;
            self.track = (self.track + 1) % TRACK_TITLES.len();
        }
    }

    fn toggle_play(&mut self) {
        self.playing = !self.playing;
    }

    fn next(&mut self) {
        self.track = (self.track + 1) % TRACK_TITLES.len();
        self.pos = 0;
    }

    fn prev(&mut self) {
        self.track = (self.track + TRACK_TITLES.len() - 1) % TRACK_TITLES.len();
        self.pos = 0;
    }

    fn vol_up(&mut self) {
        self.volume = (self.volume + 5).min(100);
    }

    fn vol_down(&mut self) {
        self.volume = (self.volume - 5).max(0);
    }

    /// Map a scene button name to its behaviour.
    fn press(&mut self, name: &str) {
        match name {
            "btn_play" => self.toggle_play(),
            "btn_next" => self.next(),
            "btn_prev" => self.prev(),
            "btn_vol_up" => self.vol_up(),
            "btn_vol_down" => self.vol_down(),
            _ => {}
        }
    }

    /// 20-char ASCII bar: 'x' * filled + '.' padding. Halfway through a
    /// 180 s track (pos = 90) is exactly 10 filled.
    fn progress_bar(&self) -> String {
        let filled = ((self.pos * BAR_WIDTH as u64) / self.duration()) as usize;
        let filled = filled.min(BAR_WIDTH);
        let mut bar = String::with_capacity(BAR_WIDTH);
        for _ in 0..filled {
            bar.push('x');
        }
        for _ in filled..BAR_WIDTH {
            bar.push('.');
        }
        bar
    }

    fn volume_label(&self) -> String {
        format!("vol {}", self.volume)
    }

    fn play_label(&self) -> &'static str {
        if self.playing {
            "pause"
        } else {
            "play"
        }
    }
}

/// Host-side data binding: override the live labels over the declared
/// scene structure every frame.
fn with_state(mut nodes: Vec<DrawNode>, player: &Player) -> Vec<DrawNode> {
    for n in &mut nodes {
        match n.name.as_str() {
            "track_label" => n.text = Some(player.title().to_string()),
            "progress_label" => n.text = Some(player.progress_bar()),
            "vol_label" => n.text = Some(player.volume_label()),
            "btn_play" => n.text = Some(player.play_label().to_string()),
            _ => {}
        }
    }
    nodes
}

fn present_nodes(nodes: &[DrawNode]) -> Vec<Primitive> {
    nodes
        .iter()
        .map(|n| {
            let rect = goya_layout::Rect {
                x: n.rect.x,
                y: n.rect.y,
                width: n.rect.width,
                height: n.rect.height,
            };
            let mut label = n.text.clone().unwrap_or_default();
            if n.is_button && label.is_empty() {
                label = n.name.clone();
            }
            if n.is_button {
                Primitive::Button(rect, label)
            } else if n.text.is_some() {
                Primitive::Text(rect, label)
            } else {
                Primitive::Panel(rect)
            }
        })
        .collect()
}

/// Short confirmation tone per button press (optional --sound path).
fn play_tap(audio: &mut Option<PwCatOutput>) {
    if let Some(out) = audio.as_mut() {
        let t = Tap::new(520.0, 48_000.0, 0.08);
        let mut lim = Limiter::new();
        let buf: Vec<f32> = (0..3_840)
            .map(|i| lim.push(0.45 * t.sample(i as f32 / 48_000.0)))
            .collect();
        let _ = AudioOutput::write(out, &buf);
        let _ = limit(0.0, 0.9); // limiter path stays warm (allocation-free contract)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "--live");
    let sound = args.iter().any(|a| a == "--sound");

    let source = std::fs::read_to_string(SCENE_PATH).unwrap_or_else(|e| {
        eprintln!("goya-music-player: cannot read {SCENE_PATH}: {e}");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-music-player: {e}");
            std::process::exit(1);
        }
    };

    if !live {
        self_test(&mut store);
        return;
    }

    let mut win = match GoyaWindow::create("goya-music-player", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("goya-music-player: live unavailable: {e}");
            return;
        }
    };
    let mut player = Player::default();
    let mut audio = if sound { PwCatOutput::spawn() } else { None };
    let mut dirty = true; // first frame always paints
    while !win.close_requested() {
        // Advance playback once per frame while playing; every tick can
        // move the progress bar, so mark dirty (bar granularity is
        // coarse enough that this stays cheap for an exemplar).
        let was_pos = player.pos;
        let was_track = player.track;
        player.tick();
        if player.pos != was_pos || player.track != was_track {
            dirty = true;
        }
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let frame = layout(store.get(), SCENE_W_PT, SCENE_H_PT);
                    if let Some(name) = hit_test(
                        &frame,
                        (x as i64) * goya_wayland::MP_PER_PX,
                        (y as i64) * goya_wayland::MP_PER_PX,
                    ) {
                        if name.starts_with("btn_") {
                            player.press(&name);
                            dirty = true;
                            play_tap(&mut audio);
                        }
                    }
                }
                goya_wayland::PlatformEvent::Resized(_, _) => dirty = true,
                _ => {}
            }
        }
        // Redraw only when something changed — idle frames still pump
        // compositor events but commit nothing.
        if dirty {
            let nodes = layout(store.get(), SCENE_W_PT, SCENE_H_PT).nodes;
            let nodes = with_state(nodes, &player);
            let prims = present_nodes(&nodes);
            let fills: Vec<Option<u32>> = nodes.iter().map(|node| node.fill).collect();
            let _ = win.present(&prims, &fills, &[], [235, 235, 240], [16, 16, 24], 4);
            dirty = false;
        }
        win.pump(1);
    }
    println!(
        "goya-music-player live: closed on '{}' at {} s, volume {}, generation {}",
        player.title(),
        player.pos,
        player.volume,
        store.generation()
    );
}

/// Headless acceptance: drive the state machine exactly as clicks would,
/// verify playback semantics, and sanity-check scene structure.
fn self_test(store: &mut SceneStore) {
    let mut failures = Vec::new();
    let mut p = Player::default();
    let frame = layout(store.get(), SCENE_W_PT, SCENE_H_PT);

    // Structural checks: all controls present and hit-testable.
    for n in [
        "title",
        "track_label",
        "progress_label",
        "vol_label",
        "btn_prev",
        "btn_play",
        "btn_next",
        "btn_vol_down",
        "btn_vol_up",
    ] {
        if frame.find(n).is_none() {
            failures.push(format!("missing scene node {n}"));
        }
    }
    if let Some(b) = frame.find("btn_play") {
        let hit = hit_test(&frame, b.rect.x + 1_000, b.rect.y + 1_000);
        if hit.as_deref() != Some("btn_play") {
            failures.push("btn_play not hit-testable".into());
        }
    }

    // play → tick(10) advances position by 10.
    p.press("btn_play");
    if !p.playing {
        failures.push("play: playing flag not set".into());
    }
    for _ in 0..10 {
        p.tick();
    }
    if p.pos != 10 {
        failures.push(format!("play+tick(10): expected pos 10, got {}", p.pos));
    }

    // pause → tick does not advance.
    p.press("btn_play");
    if p.playing {
        failures.push("pause: playing flag not cleared".into());
    }
    p.tick();
    if p.pos != 10 {
        failures.push(format!("pause: tick advanced pos to {}", p.pos));
    }

    // next from last track wraps to first.
    p.track = TRACK_TITLES.len() - 1;
    p.press("btn_next");
    if p.track != 0 || p.pos != 0 {
        failures.push(format!("next wrap: track {} pos {}", p.track, p.pos));
    }

    // prev from first track wraps to last.
    p.press("btn_prev");
    if p.track != TRACK_TITLES.len() - 1 || p.pos != 0 {
        failures.push(format!("prev wrap: track {} pos {}", p.track, p.pos));
    }

    // vol+ past 100 clamps to 100; vol- below 0 clamps to 0.
    p.volume = 98;
    p.press("btn_vol_up");
    p.press("btn_vol_up");
    if p.volume != 100 {
        failures.push(format!("vol+ clamp: expected 100, got {}", p.volume));
    }
    p.volume = 2;
    p.press("btn_vol_down");
    p.press("btn_vol_down");
    if p.volume != 0 {
        failures.push(format!("vol- clamp: expected 0, got {}", p.volume));
    }

    // Track end auto-advances: position duration-1, playing, tick →
    // next track, position 0.
    p.track = 1;
    p.pos = TRACK_DURATIONS[1] - 1;
    p.playing = true;
    p.tick();
    if p.track != 2 || p.pos != 0 {
        failures.push(format!("auto-advance: track {} pos {}", p.track, p.pos));
    }

    // Progress bar exact for a known position: 50% of 180 s → 10 filled.
    p.track = 0;
    p.pos = 90;
    let bar = p.progress_bar();
    let expected = format!("{}{}", "x".repeat(10), ".".repeat(10));
    if bar != expected {
        failures.push(format!("progress bar: expected {expected:?}, got {bar:?}"));
    }
    // Full track → fully filled.
    p.pos = TRACK_DURATIONS[0];
    let full = p.progress_bar();
    if full != "x".repeat(BAR_WIDTH) {
        failures.push(format!("progress bar full: got {full:?}"));
    }

    // Host binding: play button label swaps with state.
    p.playing = false;
    let nodes = with_state(layout(store.get(), SCENE_W_PT, SCENE_H_PT).nodes, &p);
    let play_text = nodes
        .iter()
        .find(|n| n.name == "btn_play")
        .and_then(|n| n.text.clone())
        .unwrap_or_default();
    if play_text != "play" {
        failures.push(format!("btn_play label: expected play, got {play_text}"));
    }

    if failures.is_empty() {
        println!("music: ALL CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("MUSIC FAILURE: {f}");
        }
        std::process::exit(1);
    }
}
