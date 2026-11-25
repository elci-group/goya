//! GVI-O against the strongest existing encoding (GOYA-GFX-GVIO-001 §41),
//! on this machine.
//!
//! For each workload, the baseline is the smaller of GVI-D depth 32 and
//! GVI-A provenance depth 8. GVI-O is run on top of the same
//! configuration. Reported: total bytes, `STAT` and `DIFF` bytes (so each
//! mechanism's saving shows separately), median build time, median load
//! (`open` + `decode_geometry`), and median warm `resolve`. Every GVI-O
//! inventory is checked against the baseline for every state before it is
//! reported.
//!
//!     cargo run -p goya-gvi --example gvi_operative --release

use goya_graphics_ir::{
    Affine, Drawing, Fill, FillRule, Item, Path, Point, Rgba8, Segment, Style, FIXED_ONE,
};
use goya_gva::GvaLimits;
use goya_gvi::{
    compile_with, AlbitProfile, Bake, Budget, CompileOptions, Dimension, Domain, Effect, GviLimits,
    Inventory, OperativeProfile, Radix, Reachability, Selector, StateModel, Value, DIFF, STAT,
};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Res<T> = Result<T, Box<dyn Error>>;
const C: f64 = 50_000.0;

fn ring(n: usize, radius: impl Fn(usize) -> f64) -> Res<Path> {
    let p = |i: usize| {
        let a = (i % n) as f64 / n as f64 * std::f64::consts::TAU;
        Point::new(
            (C + radius(i % n) * a.cos()).round() as i32,
            (C + radius(i % n) * a.sin()).round() as i32,
        )
    };
    let mut segs = vec![Segment::MoveTo(p(0))];
    for i in 0..n {
        let (a, b) = (p(i), p(i + 1));
        let c1 = Point::new(a.x + (b.x - a.x) / 3, a.y + (b.y - a.y) / 3);
        let c2 = Point::new(a.x + 2 * (b.x - a.x) / 3, a.y + 2 * (b.y - a.y) / 3);
        segs.push(Segment::CubicTo(c1, c2, b));
    }
    segs.push(Segment::Close);
    Ok(Path::new(segs)?)
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

fn drawing(path: Path) -> Drawing {
    Drawing {
        width: 100_000,
        height: 100_000,
        items: vec![Item {
            path: Arc::new(path),
            style: fill(42, 130, 255),
            transform: Affine::IDENTITY,
            opacity: 255,
        }],
    }
}

fn model(dims: Vec<Dimension>) -> StateModel {
    StateModel {
        parent: "scene://operative".into(),
        dimensions: dims,
        reachability: Reachability::Product { exclusions: vec![] },
    }
}

fn frames(target: &Arc<Path>, fractions: &[u32]) -> Dimension {
    Dimension::discrete(
        "frame",
        fractions
            .iter()
            .enumerate()
            .map(|(k, &t)| {
                Value::new(&format!("f{k}")).with(Effect::Morph(Selector::All, target.clone(), t))
            })
            .collect(),
    )
}

/// A section's length, read from the container directory.
fn section(bytes: &[u8], tag: goya_graphics_container::Tag) -> usize {
    let n = u32::from_le_bytes(bytes[32..36].try_into().unwrap_or([0; 4])) as usize;
    (0..n)
        .map(|i| 80 + 72 * i)
        .find(|&e| bytes[e..e + 4] == tag.bytes())
        .map(|e| u64::from_le_bytes(bytes[e + 16..e + 24].try_into().unwrap_or([0; 8])) as usize)
        .unwrap_or(0)
}

fn med(v: &mut [Duration]) -> Duration {
    v.sort_unstable();
    v[v.len() / 2]
}

fn states(m: &StateModel) -> Res<Vec<Vec<u16>>> {
    let radix = Radix::new(
        m.dimensions
            .iter()
            .map(|d| match &d.domain {
                Domain::Discrete(v) => v.len() as u64,
                _ => 1,
            })
            .collect(),
    )
    .ok_or("radix")?;
    Ok((0..radix.domain).map(|k| radix.unpack(k)).collect())
}

fn measure(bytes: &[u8], m: &StateModel, runs: usize) -> Res<(Duration, Duration)> {
    let budget = Budget {
        max_states: 1 << 20,
        ..Budget::default()
    };
    let binding = m.binding(1, &budget)?;
    let mut load = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        let inv = Inventory::open(bytes, &binding, &GviLimits::default())?;
        std::hint::black_box(inv.decode_geometry()?);
        load.push(t.elapsed());
    }
    let inv = Inventory::open(bytes, &binding, &GviLimits::default())?;
    let all = states(m)?;
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut look = Vec::new();
    for _ in 0..100_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let s = &all[(seed % all.len() as u64) as usize];
        let t = Instant::now();
        std::hint::black_box(inv.resolve(s)?);
        look.push(t.elapsed());
    }
    Ok((med(&mut load), med(&mut look)))
}

fn run(name: &str, src: &Drawing, m: &StateModel, bake: Bake, runs: usize) -> Res<()> {
    let budget = Budget {
        max_states: 1 << 20,
        max_bytes: 64 << 20,
        ..Budget::default()
    };
    let gl = GvaLimits::default();
    let binding = m.binding(1, &budget)?;
    let opts = |depth: u32, albit, operative| CompileOptions {
        bake,
        max_diff_depth: depth,
        albit,
        operative,
    };
    let build = |o: CompileOptions| -> Res<(Vec<u8>, Duration)> {
        let mut t_all = Vec::new();
        let mut out = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            out = compile_with(src, m, &budget, &gl, o)?;
            t_all.push(t.elapsed());
        }
        Ok((out, med(&mut t_all)))
    };
    let (d32, _) = build(opts(32, AlbitProfile::Off, OperativeProfile::Off))?;
    let (ga, ga_t) = build(opts(8, AlbitProfile::Provenance, OperativeProfile::Off))?;
    let (base, base_t, base_name, base_opts) = if ga.len() <= d32.len() {
        (
            ga,
            ga_t,
            "GVI-A prov d8",
            opts(8, AlbitProfile::Provenance, OperativeProfile::Off),
        )
    } else {
        let (b, t) = build(opts(32, AlbitProfile::Off, OperativeProfile::Off))?;
        (
            b,
            t,
            "GVI-D d32",
            opts(32, AlbitProfile::Off, OperativeProfile::Off),
        )
    };
    let (op, op_t) = build(CompileOptions {
        operative: OperativeProfile::Fast,
        ..base_opts
    })?;
    // Exactness: every state equals the baseline.
    let a = Inventory::open(&base, &binding, &GviLimits::default())?;
    let b = Inventory::open(&op, &binding, &GviLimits::default())?;
    let (pa, pb) = (a.decode_geometry()?, b.decode_geometry()?);
    for s in states(m)? {
        if a.resolve(&s)?.drawing(&pa) != b.resolve(&s)?.drawing(&pb) {
            return Err(format!("{name}: state {s:?} differs").into());
        }
    }
    let (bl, bk) = measure(&base, m, runs.max(5))?;
    let (ol, ok) = measure(&op, m, runs.max(5))?;
    println!("{name} ({bake:?}), {} states:", states(m)?.len());
    println!(
        "  {:<16} {:>8} B  STAT {:>7} B  DIFF {:>7} B  build {:>10.3?}  load {:>10.3?}  resolve {:>8.3?}",
        base_name,
        base.len(),
        section(&base, STAT),
        section(&base, DIFF),
        base_t,
        bl,
        bk
    );
    println!(
        "  {:<16} {:>8} B  STAT {:>7} B  DIFF {:>7} B  build {:>10.3?}  load {:>10.3?}  resolve {:>8.3?}",
        "+ GVI-O fast",
        op.len(),
        section(&op, STAT),
        section(&op, DIFF),
        op_t,
        ol,
        ok
    );
    println!(
        "  change: total {:+.1}%, STAT {:+.1}%, DIFF {:+.1}%{}",
        100.0 * (op.len() as f64 / base.len() as f64 - 1.0),
        100.0 * (section(&op, STAT) as f64 / section(&base, STAT).max(1) as f64 - 1.0),
        100.0 * (section(&op, DIFF) as f64 / section(&base, DIFF).max(1) as f64 - 1.0),
        if op == base {
            "  (byte-identical to baseline)"
        } else {
            ""
        }
    );
    Ok(())
}

fn main() -> Res<()> {
    let star = ring(64, |i| if i % 2 == 0 { 40_000.0 } else { 22_000.0 })?;
    let circle = Arc::new(ring(64, |_| 32_000.0)?);
    let uniform = |n: u32| -> Vec<u32> { (0..n).map(|k| k * 65_536 / (n - 1)).collect() };
    let about = |q: i32| {
        let e = (C - f64::from(q) * C / f64::from(FIXED_ONE)).round() as i32;
        Affine {
            a: q,
            b: 0,
            c: 0,
            d: q,
            e,
            f: e,
        }
    };
    let theme = Dimension::discrete(
        "theme",
        vec![
            Value::new("blue"),
            Value::new("amber").with(Effect::Restyle(Selector::All, fill(255, 170, 0))),
        ],
    );
    let sizes = Dimension::discrete(
        "size",
        vec![
            Value::new("small").with(Effect::Transform(Selector::All, about(FIXED_ONE / 2))),
            Value::new("medium").with(Effect::Transform(Selector::All, about(FIXED_ONE * 3 / 4))),
            Value::new("full"),
        ],
    );
    let src = drawing(star.clone());

    // O01: the existing 192-state morph.
    let o01 = model(vec![frames(&circle, &uniform(32)), theme.clone(), sizes]);
    run("O01 192-state morph", &src, &o01, Bake::None, 11)?;
    run("O01 192-state morph", &src, &o01, Bake::Transforms, 11)?;
    // O02: 1,024 frames.
    run(
        "O02 1,024-frame morph",
        &src,
        &model(vec![frames(&circle, &uniform(1024))]),
        Bake::None,
        3,
    )?;
    // O04: irregular (eased) fractions.
    let eased: Vec<u32> = (0..32u64)
        .map(|k| (65_536 * k * k / (31 * 31)) as u32)
        .collect();
    run(
        "O04 eased fractions",
        &src,
        &model(vec![frames(&circle, &eased), theme.clone()]),
        Bake::None,
        11,
    )?;
    // O03: a 65,536-state regular domain (256 × 256 translation steps; one
    // dimension holds at most 65,534 values), with no morph.
    let small = Path::new(vec![
        Segment::MoveTo(Point::new(0, 0)),
        Segment::LineTo(Point::new(1_000, 0)),
        Segment::LineTo(Point::new(1_000, 1_000)),
        Segment::Close,
    ])?;
    let axis = |name: &str, x: bool| {
        Dimension::discrete(
            name,
            (0..256i32)
                .map(|k| {
                    let t = if x {
                        Affine::translate(k * 100, 0)
                    } else {
                        Affine::translate(0, k * 100)
                    };
                    Value::new(&format!("{name}{k}")).with(Effect::Transform(Selector::All, t))
                })
                .collect(),
        )
    };
    run(
        "O03 65,536-state domain",
        &drawing(small),
        &model(vec![axis("x", true), axis("y", false)]),
        Bake::None,
        3,
    )?;
    // O11: random labels and fractions.
    let mut x = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let random = Dimension::discrete(
        "frame",
        (0..32)
            .map(|_| {
                let label = format!("{:x}", next());
                Value::new(&label).with(Effect::Morph(
                    Selector::All,
                    circle.clone(),
                    (next() % 65_537) as u32,
                ))
            })
            .collect(),
    );
    run("O11 random", &src, &model(vec![random]), Bake::None, 11)?;
    Ok(())
}
