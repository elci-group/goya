//! GOYA-GFX-001 workload B03 (dynamic dashboard with independent
//! modulation) on this machine, for the GRU gate: at least 5× faster local
//! modulation updates than full graphical reconstruction.
//!
//! The dashboard has 64 gauges (track, bar, lamp, label each; 256
//! instances) and 128 slots. Each update changes one random gauge's level
//! (a transform) or status (a colour). Three strategies produce what a
//! backend needs to present the change:
//!
//! - **full reconstruction:** re-load the compiled unit (decode every
//!   geometry), re-evaluate everything at the current values, and rebuild
//!   the whole scene. This is what a runtime does that re-reads its
//!   graphic on change;
//! - **full re-evaluation:** the unit is already loaded, but every binding
//!   is re-evaluated and the whole scene rebuilt;
//! - **incremental GRU:** `set` the one slot, take the invalidation, and
//!   rebuild only the dirty instances.
//!
//!     cargo run -p goya-gru --example gru_dashboard --release

use goya_graphics_ir::{Affine, Drawing, Fill, FillRule, Item, Path, Point, Rgba8, Segment, Style};
use goya_gru::{
    encode, Binding, GruLimits, Map, Slot, SlotType, SlotValue, Target, Unit, UnitSource, Value,
};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Res<T> = Result<T, Box<dyn Error>>;
const GAUGES: u32 = 64;

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

fn circle(cx: i32, cy: i32, r: i32) -> Res<Path> {
    use Segment::*;
    let k = (f64::from(r) * 0.552_284_749_830_793_6).round() as i32;
    let p = Point::new;
    Ok(Path::new(vec![
        MoveTo(p(cx + r, cy)),
        CubicTo(p(cx + r, cy + k), p(cx + k, cy + r), p(cx, cy + r)),
        CubicTo(p(cx - k, cy + r), p(cx - r, cy + k), p(cx - r, cy)),
        CubicTo(p(cx - r, cy - k), p(cx - k, cy - r), p(cx, cy - r)),
        CubicTo(p(cx + k, cy - r), p(cx + r, cy - k), p(cx + r, cy)),
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

fn source() -> Res<UnitSource> {
    let mut items = Vec::new();
    let mut slots = Vec::new();
    let mut bindings = Vec::new();
    for g in 0..GAUGES {
        let (x0, y0) = ((g % 16) as i32 * 12_000, (g / 16) as i32 * 60_000);
        for (path, style) in [
            (
                rect(x0 + 2_000, y0 + 2_000, x0 + 8_000, y0 + 40_000)?,
                fill(50, 54, 62),
            ),
            (
                rect(x0 + 3_000, y0 + 3_000, x0 + 7_000, y0 + 39_000)?,
                fill(42, 130, 255),
            ),
            (circle(x0 + 5_000, y0 + 46_000, 2_500)?, fill(120, 120, 120)),
            (
                rect(x0 + 1_000, y0 + 50_000, x0 + 9_000, y0 + 52_000)?,
                fill(230, 230, 235),
            ),
        ] {
            items.push(Item {
                path: Arc::new(path),
                style,
                transform: Affine::IDENTITY,
                opacity: 255,
            });
        }
        slots.push(Slot {
            name: format!("level{g}"),
            ty: SlotType::Scalar {
                min: 0,
                max: 65_536,
            },
            default: SlotValue::Scalar(32_768),
        });
        bindings.push(Binding {
            slot: (2 * g) as u16,
            target: Target::Scale {
                instance: 4 * g + 1,
                pivot: Point::new(x0 + 5_000, y0 + 39_000),
            },
            map: Map::Linear {
                mul: 58_982,
                add: 6_554,
            },
        });
        slots.push(Slot {
            name: format!("status{g}"),
            ty: SlotType::Enum(vec!["ok".into(), "warn".into(), "fault".into()]),
            default: SlotValue::Enum(0),
        });
        bindings.push(Binding {
            slot: (2 * g + 1) as u16,
            target: Target::FillColour(4 * g + 2),
            map: Map::Table(vec![
                Value::Colour(Rgba8::new(40, 200, 90, 255)),
                Value::Colour(Rgba8::new(255, 170, 0, 255)),
                Value::Colour(Rgba8::new(220, 40, 40, 255)),
            ]),
        });
    }
    Ok(UnitSource {
        drawing: Drawing {
            width: 192_000,
            height: 240_000,
            items,
        },
        slots,
        bindings,
        regions: vec![],
        provenance: None,
    })
}

fn pct(v: &mut [Duration], p: f64) -> Duration {
    v.sort_unstable();
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn report(name: &str, mut v: Vec<Duration>) -> Duration {
    let med = pct(&mut v, 0.5);
    let p99 = pct(&mut v, 0.99);
    println!("{name:<22} median {med:>10.3?}  p99 {p99:>10.3?}");
    med
}

fn main() -> Res<()> {
    let limits = GruLimits::default();
    let bytes = encode(&source()?, &limits)?;
    let unit = Unit::open(&bytes, &limits)?;
    println!(
        "B03 dashboard: {} instances, {} slots, {} stored geometries, unit {} B",
        unit.instance_count(),
        unit.slots().len(),
        unit.geometry().len(),
        bytes.len()
    );

    // A fixed pseudo-random sequence of single-slot updates.
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let updates: Vec<(u16, SlotValue)> = (0..2_000)
        .map(|_| {
            let g = (next() % u64::from(GAUGES)) as u16;
            if next() % 2 == 0 {
                (2 * g, SlotValue::Scalar((next() % 65_537) as i32))
            } else {
                (2 * g + 1, SlotValue::Enum((next() % 3) as u16))
            }
        })
        .collect();

    let mut values: Vec<SlotValue> = unit.slots().iter().map(|s| s.default).collect();
    let (mut full, mut reeval, mut incr) = (Vec::new(), Vec::new(), Vec::new());
    let mut st = unit.instantiate()?;
    st.take_invalidation();
    let mut dirty_items = 0usize;
    for &(slot, v) in &updates {
        values[usize::from(slot)] = v;

        let t = Instant::now();
        let u2 = Unit::open(&bytes, &limits)?;
        let mut s2 = u2.instantiate()?;
        for (k, val) in values.iter().enumerate() {
            s2.set(k as u16, *val)?;
        }
        std::hint::black_box(s2.drawing());
        full.push(t.elapsed());

        let t = Instant::now();
        let mut s3 = unit.instantiate()?;
        for (k, val) in values.iter().enumerate() {
            s3.set(k as u16, *val)?;
        }
        std::hint::black_box(s3.drawing());
        reeval.push(t.elapsed());

        let t = Instant::now();
        st.set(slot, v)?;
        let inv = st.take_invalidation();
        let items: Vec<Option<Item>> = inv.instances.iter().map(|&i| st.item(i as usize)).collect();
        std::hint::black_box(&items);
        incr.push(t.elapsed());
        dirty_items += items.len();
    }
    // The incremental state must equal a from-scratch evaluation.
    let mut check = unit.instantiate()?;
    for (k, val) in values.iter().enumerate() {
        check.set(k as u16, *val)?;
    }
    assert_eq!(check.drawing(), st.drawing(), "incremental state diverged");

    let f = report("full reconstruction", full);
    let r = report("full re-evaluation", reeval);
    let i = report("incremental GRU", incr);
    println!(
        "speed-up (median): {:.0}x vs reconstruction, {:.0}x vs re-evaluation; mean dirty instances per update {:.2}",
        f.as_secs_f64() / i.as_secs_f64(),
        r.as_secs_f64() / i.as_secs_f64(),
        dirty_items as f64 / updates.len() as f64
    );
    Ok(())
}
