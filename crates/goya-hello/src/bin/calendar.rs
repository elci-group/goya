//! goya-calendar — the CALENDAR exemplar Goya application.
//!
//! The UI is 100% declared in `apps/calendar.gscene` (GY-LANG/1
//! RUGID-style syntax). This host is the behavioural half: it keeps a
//! deterministic Gregorian (year, month) state, overrides the
//! `month_label` cell and the 42 `day_r{row}_c{col}` grid cells with the
//! displayed month, and routes prev/next/today button presses via
//! hit-test.
//!
//!   cargo run --bin calendar            (headless self-test)
//!   cargo run --bin calendar -- --live  (Wayland window)
//!
//! Without `--live`, deterministic calendar math is verified headless:
//! leap-year rule, the 2000-01-01 Saturday anchor, month wrap-around,
//! the 2026-11-01 grid column, and the today reset.

use goya_lang::{SceneStore, hit_test, layout};
use goya_runtime::Primitive;
use goya_wayland::{GoyaWindow, MP_PER_PX, PlatformEvent};

const SCENE_PATH: &str = "apps/calendar.gscene";
const SCENE_W_PT: f64 = 100.0;
const SCENE_H_PT: f64 = 170.0;
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 680;

/// The project "today": November 2026.
const TODAY_YEAR: i32 = 2026;
const TODAY_MONTH: u32 = 11;

const MONTH_NAMES: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Days in a month, proleptic Gregorian: leap years divisible by 4,
/// except centuries not divisible by 400.
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days since 2000-01-01 (may be negative). Howard Hinnant's
/// days_from_civil algorithm, deterministic for the whole i32 range.
fn days_from_epoch(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(if month <= 2 { 1 } else { 0 });
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468 - 10_957
}

/// Weekday of a date, 0 = sunday .. 6 = saturday.
/// Anchor: 2000-01-01 was a Saturday, so weekday = (days + 6) mod 7.
fn weekday_sunday_first(year: i32, month: u32, day: u32) -> u32 {
    let d = days_from_epoch(year, month, day);
    let w = (d + 6).rem_euclid(7);
    // (d + 6) mod 7 with d=0 (2000-01-01, Saturday) must give 6.
    w as u32
}

/// Column in a monday-first grid, 0 = mo .. 6 = su.
fn grid_column_monday_first(year: i32, month: u32, day: u32) -> u32 {
    (weekday_sunday_first(year, month, day) + 6) % 7
}

/// Calendar state: a (year, month) cursor with wrap-around navigation.
struct Calendar {
    year: i32,
    month: u32, // 1..=12
}

impl Calendar {
    fn today() -> Self {
        Calendar {
            year: TODAY_YEAR,
            month: TODAY_MONTH,
        }
    }

    fn prev_month(&mut self) {
        if self.month == 1 {
            self.month = 12;
            self.year -= 1;
        } else {
            self.month -= 1;
        }
    }

    fn next_month(&mut self) {
        if self.month == 12 {
            self.month = 1;
            self.year += 1;
        } else {
            self.month += 1;
        }
    }

    fn label(&self) -> String {
        format!(
            "{} {}",
            MONTH_NAMES[(self.month as usize).min(11)],
            self.year
        )
    }

    /// The 42 grid texts, monday-first: day numbers inside the month,
    /// empty strings for padding cells.
    fn grid_texts(&self) -> [String; 42] {
        let mut texts = std::array::from_fn(|_| String::new());
        let first_col = grid_column_monday_first(self.year, self.month, 1) as usize;
        let n_days = days_in_month(self.year, self.month) as usize;
        for (i, slot) in texts.iter_mut().enumerate() {
            let day = (i + 1).saturating_sub(first_col);
            if day >= 1 && day <= n_days {
                *slot = day.to_string();
            }
        }
        texts
    }

    /// Map a scene button name to its behaviour.
    fn press(&mut self, name: &str) {
        match name {
            "btn_prev" => self.prev_month(),
            "btn_next" => self.next_month(),
            "btn_today" => *self = Calendar::today(),
            _ => {}
        }
    }
}

/// Host-side data binding: override the month label and every day-grid
/// cell with the live month (scene declares structure, host supplies state).
fn with_calendar_data(
    mut nodes: Vec<goya_lang::DrawNode>,
    cal: &Calendar,
) -> Vec<goya_lang::DrawNode> {
    let label = cal.label();
    let texts = cal.grid_texts();
    for n in &mut nodes {
        if n.name == "month_label" {
            n.text = Some(label.clone());
        } else if n.name.starts_with("day_r") {
            if let Some(idx) = grid_index(&n.name) {
                n.text = Some(texts[idx].clone());
            }
        }
    }
    nodes
}

/// `day_r{row}_c{col}` -> flat index row * 7 + col; None on a malformed name.
fn grid_index(name: &str) -> Option<usize> {
    let rest = name.strip_prefix("day_r")?;
    let usplit = rest.find("_c")?;
    let row: usize = rest[..usplit].parse().ok()?;
    let col: usize = rest[usplit + 2..].parse().ok()?;
    if row < 6 && col < 7 {
        Some(row * 7 + col)
    } else {
        None
    }
}

fn present_nodes(nodes: &[goya_lang::DrawNode]) -> Vec<Primitive> {
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "--live");

    let source = std::fs::read_to_string(SCENE_PATH).unwrap_or_else(|e| {
        eprintln!("goya-calendar: cannot read {SCENE_PATH}: {e}");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-calendar: {e}");
            std::process::exit(1);
        }
    };

    if !live {
        self_test(&mut store);
        return;
    }

    if let Err(code) = run_live(&mut store) {
        eprintln!("goya-calendar: live unavailable: {code}");
    }
}

fn run_live(store: &mut SceneStore) -> Result<(), String> {
    let mut win =
        GoyaWindow::create("goya-calendar", WIN_W_PX, WIN_H_PX).map_err(|e| e.to_string())?;
    let mut cal = Calendar::today();
    let mut dirty = true; // first frame always paints
    let mut presents = 0u32;
    while !win.close_requested() {
        for ev in win.poll() {
            if let PlatformEvent::PointerPress(x, y) = ev {
                let frame = layout(store.get(), SCENE_W_PT, SCENE_H_PT);
                if let Some(name) =
                    hit_test(&frame, x as i64 * MP_PER_PX, y as i64 * MP_PER_PX)
                {
                    if name.starts_with("btn_") {
                        cal.press(&name);
                        dirty = true;
                    }
                }
            }
        }
        // Redraw only when something changed — idle frames pump events
        // but commit nothing.
        if dirty {
            let nodes = layout(store.get(), SCENE_W_PT, SCENE_H_PT).nodes;
            let nodes = with_calendar_data(nodes, &cal);
            let prims = present_nodes(&nodes);
            let fills: Vec<Option<u32>> = nodes.iter().map(|node| node.fill).collect();
            let _ = win.present(&prims, &fills, [235, 235, 240], [20, 20, 28], 3);
            presents += 1;
            dirty = false;
        }
        win.pump(1);
    }
    println!(
        "goya-calendar live: {presents} presents, last month '{}', generation {}",
        cal.label(),
        store.generation()
    );
    Ok(())
}

/// Headless acceptance: verify the deterministic Gregorian math, the
/// scene structure, and that the host overrides reach the grid cells.
fn self_test(store: &mut SceneStore) {
    let mut failures: Vec<String> = {
        let mut f = Vec::new();
        let frame = layout(store.get(), SCENE_W_PT, SCENE_H_PT);

        // Structural checks: title, month label, 7 weekday headers,
        // 42 day cells, 3 nav buttons.
        if frame.find("title").is_none() {
            f.push("missing title cell".into());
        }
        if frame.find("month_label").is_none() {
            f.push("missing month_label cell".into());
        }
        for i in 0..7 {
            if frame.find(&format!("wd_{i}")).is_none() {
                f.push(format!("missing weekday header wd_{i}"));
            }
        }
        for row in 0..6 {
            for col in 0..7 {
                let name = format!("day_r{row}_c{col}");
                if frame.find(&name).is_none() {
                    f.push(format!("missing day cell {name}"));
                }
            }
        }
        for b in ["btn_prev", "btn_next", "btn_today"] {
            if frame.find(b).is_none() {
                f.push(format!("missing button {b}"));
            }
        }
        if let Some(b) = frame.find("btn_today") {
            let hit = hit_test(&frame, b.rect.x + 1_000, b.rect.y + 1_000);
            if hit.as_deref() != Some("btn_today") {
                f.push("btn_today not hit-testable".into());
            }
        }
        f
    };

    // Leap-year rule.
    if days_in_month(2024, 2) != 29 {
        failures.push(format!(
            "feb 2024: expected 29 days, got {}",
            days_in_month(2024, 2)
        ));
    }
    if days_in_month(2023, 2) != 28 {
        failures.push(format!(
            "feb 2023: expected 28 days, got {}",
            days_in_month(2023, 2)
        ));
    }
    if days_in_month(2000, 2) != 29 {
        failures.push("feb 2000 (century, /400): expected 29 days".into());
    }
    if days_in_month(1900, 2) != 28 {
        failures.push("feb 1900 (century, not /400): expected 28 days".into());
    }

    // Weekday anchor: 2000-01-01 was a Saturday.
    let w = weekday_sunday_first(2000, 1, 1);
    if w != 6 {
        failures.push(format!("2000-01-01: expected saturday (6), got {w}"));
    }

    // Month navigation: prev from jan 2024 wraps to dec 2023.
    let mut cal = Calendar {
        year: 2024,
        month: 1,
    };
    cal.prev_month();
    if cal.year != 2023 || cal.month != 12 {
        failures.push(format!(
            "prev from jan 2024: expected dec 2023, got {} {}",
            MONTH_NAMES[(cal.month as usize).min(11)],
            cal.year
        ));
    }
    // And next wraps back forward across the year boundary.
    cal.next_month();
    if cal.year != 2024 || cal.month != 1 {
        failures.push("next from dec 2023: expected jan 2024".into());
    }

    // Grid for november 2026: 2026-11-01 is a Sunday -> monday-first
    // column 6, so day 1 sits in day_r0_c6.
    let cal = Calendar::today();
    let texts = cal.grid_texts();
    if texts[6] != "1" {
        failures.push(format!(
            "nov 2026 day 1: expected column 6, got '{}'",
            texts[6]
        ));
    }
    if texts[0] != "" {
        failures.push(format!(
            "nov 2026 padding cell 0: expected empty, got '{}'",
            texts[0]
        ));
    }
    // November has 30 days: last day lands at slot 6 + 29 = 35; slot 36
    // is padding.
    if texts[6 + 29] != "30" {
        failures.push(format!(
            "nov 2026 day 30: expected slot 35, got '{}'",
            texts[35]
        ));
    }
    if texts[6 + 30] != "" {
        failures.push("nov 2026 slot 36 should be padding".into());
    }
    if cal.label() != "november 2026" {
        failures.push(format!(
            "label: expected 'november 2026', got '{}'",
            cal.label()
        ));
    }

    // Today button resets to november 2026.
    let mut cal = Calendar {
        year: 1999,
        month: 2,
    };
    cal.press("btn_today");
    if cal.year != TODAY_YEAR || cal.month != TODAY_MONTH {
        failures.push("today button: expected reset to nov 2026".into());
    }

    // Host override reaches the scene nodes.
    let frame = layout(store.get(), SCENE_W_PT, SCENE_H_PT);
    let nodes = with_calendar_data(frame.nodes, &Calendar::today());
    let day1 = nodes.iter().find(|n| n.name == "day_r0_c6");
    match day1 {
        Some(n) if n.text.as_deref() == Some("1") => {}
        Some(n) => failures.push(format!(
            "host override day_r0_c6: expected '1', got {:?}",
            n.text
        )),
        None => failures.push("host override: day_r0_c6 missing".into()),
    }

    if failures.is_empty() {
        println!("calendar: ALL CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("calendar FAILURE: {f}");
        }
        std::process::exit(1);
    }
}
