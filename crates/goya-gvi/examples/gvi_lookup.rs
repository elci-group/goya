//! GOYA-GFX-001 workload B04 (multi-state interactive interface) on the
//! machine it runs on. It measures:
//!
//! - **precompute:** compiling the inventory (31 runs, median);
//! - **dynamic:** what a runtime without GVI does per state change, which
//!   is to apply the state's effects to the source items and produce a
//!   drawing;
//! - **lookup:** GVI state validation plus binary search;
//! - **lookup + drawing:** lookup, then building the same drawing from the
//!   projection.
//!
//! It reports median/p95/p99 over 200,000 warm resolutions and the §13.5
//! break-even. Rasterisation is excluded, because it costs the same on
//! both paths. Timings come from `Instant` around each call (tens of ns of
//! overhead), so treat the sub-µs figures as upper bounds.
//!
//!     cargo run -p goya-gvi --example gvi_lookup --release

use goya_graphics_ir::{
    Affine, Drawing, Fill, FillRule, Item, Path, Point, Rgba8, Segment, Style, FIXED_ONE,
};
use goya_gva::{encode as gva_encode, Gva, GvaLimits};
use goya_gvi::{
    compile, Budget, Dimension, Domain, Effect, GviLimits, Inventory, Radix, Reachability,
    Selector, StateModel, Value,
};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Res<T> = Result<T, Box<dyn Error>>;

const TILES: u32 = 12;

fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Res<Path> {
    use Segment::*;
    let p = Point::new;
    Ok(Path::new(vec![
        MoveTo(p(x0, y0)),
        LineTo(p(x1, y0)),
        LineTo(p(x1, y1)),
        LineTo(p(x0, y1)),
        Close,
    ])?)
}

fn fill(r: u8, g: u8, b: u8) -> Style {
    Style {
        fill: Some(Fill {
            colour: Rgba8::new(r, g, b, 255),
            rule: FillRule::NonZero,
        }),
        stroke: None,
    }
}

/// A 4×3 TV menu: per tile a background (item 3t), a label bar (3t+1) and a
/// focus ring (3t+2).
fn source() -> Res<Drawing> {
    let mut items = Vec::new();
    for t in 0..TILES as i32 {
        let (x, y) = ((t % 4) * 40_000 + 5_000, (t / 4) * 40_000 + 5_000);
        for (path, style) in [
            (rect(x, y, x + 30_000, y + 30_000)?, fill(40, 44, 52)),
            (
                rect(x + 2_000, y + 24_000, x + 28_000, y + 28_000)?,
                fill(200, 200, 210),
            ),
            (
                rect(x - 2_000, y - 2_000, x + 32_000, y + 32_000)?,
                fill(42, 130, 255),
            ),
        ] {
            items.push(Item {
                path: Arc::new(path),
                style,
                transform: Affine::IDENTITY,
                opacity: 255,
            });
        }
    }
    Ok(Drawing {
        width: 165_000,
        height: 125_000,
        items,
    })
}

fn model() -> StateModel {
    let rings: Vec<u32> = (0..TILES).map(|t| 3 * t + 2).collect();
    let focus = (0..TILES)
        .map(|t| {
            let others: Vec<u32> = rings.iter().copied().filter(|&r| r != 3 * t + 2).collect();
            // Focused tile grows 10% about its centre; other rings hidden.
            let (cx, cy) = (
                (t % 4) as i64 * 40_000 + 20_000,
                (t / 4) as i64 * 40_000 + 20_000,
            );
            let s = FIXED_ONE as i64 * 11 / 10;
            let grow = Affine {
                a: s as i32,
                b: 0,
                c: 0,
                d: s as i32,
                e: (cx - s * cx / FIXED_ONE as i64) as i32,
                f: (cy - s * cy / FIXED_ONE as i64) as i32,
            };
            Value::new(&format!("tile{t}"))
                .with(Effect::Hide(Selector::Items(others)))
                .with(Effect::Transform(
                    Selector::Items(vec![3 * t, 3 * t + 1, 3 * t + 2]),
                    grow,
                ))
        })
        .collect();
    let bgs: Vec<u32> = (0..TILES).map(|t| 3 * t).collect();
    StateModel {
        parent: "scene://tv/home_menu".into(),
        dimensions: vec![
            Dimension {
                name: "focus".into(),
                domain: Domain::Discrete(focus),
            },
            Dimension::discrete(
                "theme",
                vec![
                    Value::new("dark"),
                    Value::new("light")
                        .with(Effect::Restyle(Selector::Items(bgs), fill(235, 235, 240))),
                ],
            ),
            Dimension::discrete(
                "overlay",
                vec![
                    Value::new("closed"),
                    Value::new("open").with(Effect::Opacity(Selector::All, 96)),
                ],
            ),
        ],
        reachability: Reachability::Product { exclusions: vec![] },
    }
}

/// The non-GVI path: evaluate the state's effects on the source items.
fn dynamic(base: &[Item], m: &StateModel, state: &[u16]) -> Drawing {
    let mut items: Vec<(u32, Item, bool)> = base
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, it)| (i as u32, it, false))
        .collect();
    for (d, &v) in state.iter().enumerate() {
        let Domain::Discrete(values) = &m.dimensions[d].domain else {
            continue;
        };
        for e in &values[v as usize].effects {
            for (id, it, hidden) in items.iter_mut() {
                match e {
                    Effect::Transform(s, t) if s.matches(*id) => {
                        it.transform = it.transform.then(t).unwrap_or(it.transform)
                    }
                    Effect::Opacity(s, f) if s.matches(*id) => {
                        it.opacity = ((u32::from(it.opacity) * u32::from(*f) + 127) / 255) as u8
                    }
                    Effect::Restyle(s, st) if s.matches(*id) => it.style = *st,
                    Effect::Hide(s) if s.matches(*id) => *hidden = true,
                    _ => {}
                }
            }
        }
    }
    Drawing {
        width: 165_000,
        height: 125_000,
        items: items.into_iter().filter(|x| !x.2).map(|x| x.1).collect(),
    }
}

fn pct(sorted: &[Duration], p: f64) -> Duration {
    sorted[((sorted.len() as f64 - 1.0) * p).round() as usize]
}

fn summarise(name: &str, mut v: Vec<Duration>) -> Duration {
    v.sort_unstable();
    let med = pct(&v, 0.5);
    println!(
        "{name:<18} median {:>9.3?}  p95 {:>9.3?}  p99 {:>9.3?}",
        med,
        pct(&v, 0.95),
        pct(&v, 0.99)
    );
    med
}

fn main() -> Res<()> {
    let src = source()?;
    let m = model();
    let (budget, gl) = (Budget::default(), GvaLimits::default());

    let mut builds = Vec::new();
    let mut bytes = Vec::new();
    for _ in 0..31 {
        let t = Instant::now();
        bytes = compile(&src, &m, &budget, &gl)?;
        builds.push(t.elapsed());
    }
    builds.sort_unstable();
    let precompute = pct(&builds, 0.5);

    let binding = m.binding(src.items.len() as u32, &budget)?;
    let inv = Inventory::open(&bytes, &binding, &GviLimits::default())?;
    let paths = inv.decode_geometry()?;
    let gva_bytes = gva_encode(&src, &gl)?;
    let base = Gva::open(&gva_bytes, &gl)?.to_drawing()?.items;
    let radix = Radix::new(vec![u64::from(TILES), 2, 2]).ok_or("radix")?;
    let states: Vec<Vec<u16>> = (0..radix.domain).map(|k| radix.unpack(k)).collect();
    println!(
        "B04 TV menu: {} items, {} states, {} distinct projections, inventory {} B; precompute median {:.3?} (31 runs)",
        src.items.len(),
        inv.state_count(),
        inv.projection_count(),
        bytes.len(),
        precompute
    );

    // Warm-up, then measure in a fixed pseudo-random order.
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut order = Vec::with_capacity(200_000);
    for _ in 0..200_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        order.push((seed % radix.domain) as usize);
    }
    for s in &states {
        std::hint::black_box(inv.resolve(s)?);
        std::hint::black_box(dynamic(&base, &m, s));
    }
    let (mut look, mut look_draw, mut dyn_t) = (Vec::new(), Vec::new(), Vec::new());
    for &i in &order {
        let s = &states[i];
        let t = Instant::now();
        let p = std::hint::black_box(inv.resolve(s)?);
        look.push(t.elapsed());
        let t = Instant::now();
        let p2 = inv.resolve(s)?;
        std::hint::black_box(p2.drawing(&paths));
        look_draw.push(t.elapsed());
        let t = Instant::now();
        std::hint::black_box(dynamic(&base, &m, s));
        dyn_t.push(t.elapsed());
        std::hint::black_box(p.id);
    }
    // Residency (gvi.md §9.1): identical isolated loops, packed → expanded
    // → packed again, so drift shows up as a packed/packed mismatch.
    let (packed_file, packed_heap) = inv.table_residency();
    let mut inv = inv;
    let isolated = |inv: &Inventory<'_>, name: &str| -> Res<Duration> {
        let mut v = Vec::with_capacity(order.len());
        for &i in &order {
            let t = Instant::now();
            std::hint::black_box(inv.resolve(&states[i])?.drawing(&paths));
            v.push(t.elapsed());
        }
        Ok(summarise(name, v))
    };
    let p1 = isolated(&inv, "isolated packed")?;
    inv.set_residency(goya_gvi::Residency::Expanded);
    let (_, expanded_heap) = inv.table_residency();
    let x = isolated(&inv, "isolated expanded")?;
    inv.set_residency(goya_gvi::Residency::Packed);
    let p2 = isolated(&inv, "isolated packed again")?;
    // O13 (GVI-O): the same menu with operative encoding, same lookups.
    let op = goya_gvi::compile_with(
        &src,
        &m,
        &budget,
        &gl,
        goya_gvi::CompileOptions {
            bake: goya_gvi::Bake::None,
            max_diff_depth: 0,
            albit: goya_gvi::AlbitProfile::Off,
            operative: goya_gvi::OperativeProfile::Fast,
        },
    )?;
    let op_inv = Inventory::open(&op, &binding, &GviLimits::default())?;
    println!(
        "GVI-O: inventory {} B (baseline {} B), residency {:?}",
        op.len(),
        bytes.len(),
        op_inv.residency()
    );
    let _ = isolated(&op_inv, "isolated GVI-O")?;
    println!(
        "tables: packed {packed_file} B borrowed + {packed_heap} B heap; expanded adds {} B heap; expanded vs packed lookup+drawing: {:+.1}%",
        expanded_heap - packed_heap,
        100.0 * (x.as_secs_f64() / ((p1.as_secs_f64() + p2.as_secs_f64()) / 2.0) - 1.0)
    );
    let l = summarise("lookup", look);
    let ld = summarise("lookup + drawing", look_draw);
    let d = summarise("dynamic", dyn_t);
    let saved = d.as_secs_f64() - ld.as_secs_f64();
    if saved > 0.0 {
        println!(
            "break-even (§13.5, lookup+drawing vs dynamic): {:.0} state resolutions per build; dynamic/lookup = {:.0}x",
            precompute.as_secs_f64() / saved,
            d.as_secs_f64() / l.as_secs_f64().max(1e-12)
        );
    } else {
        println!("no per-resolution saving on this workload: dynamic is not slower than lookup + drawing");
    }
    Ok(())
}
