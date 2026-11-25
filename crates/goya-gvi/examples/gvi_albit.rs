//! GVI-A against GVI-D on the directive's trajectory workloads
//! (GOYA-GFX-GVIA-001 §37), on this machine.
//!
//! Each configuration reports:
//!
//! - **bytes:** inventory size;
//! - **build:** median compile time (run count shown);
//! - **load:** median `decode_geometry` (eager reconstruction);
//! - **lazy:** median single-geometry `geometry(g)` for random `g` (A11);
//! - **traj:** how many records are GVI-A trajectory samples;
//! - **anc:** the largest ancestor set any lazy rebuild touches.
//!
//! Every inventory is checked against the plain inventory for every
//! state before it is reported.
//!
//!     cargo run -p goya-gvi --example gvi_albit --release

use goya_graphics_ir::{
    Affine, Drawing, Fill, FillRule, Item, Path, Point, Rgba8, Segment, Style, FIXED_ONE,
};
use goya_gva::GvaLimits;
use goya_gvi::{
    compile, compile_with, AlbitProfile, Bake, Budget, CompileOptions, Dimension, Effect,
    GviLimits, Inventory, Radix, Reachability, Selector, StateModel, Value,
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

fn source(star: &Path) -> Drawing {
    Drawing {
        width: 100_000,
        height: 100_000,
        items: vec![Item {
            path: Arc::new(star.clone()),
            style: fill(42, 130, 255),
            transform: Affine::IDENTITY,
            opacity: 255,
        }],
    }
}

fn about_centre(a: i32, b: i32, c: i32, d: i32) -> Affine {
    // x' = a·x + c·y + e with the centre fixed.
    let k = |v: i32| i64::from(v) * 50_000 / i64::from(FIXED_ONE);
    Affine {
        a,
        b,
        c,
        d,
        e: (50_000 - k(a) - k(c)) as i32,
        f: (50_000 - k(b) - k(d)) as i32,
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

fn model(dims: Vec<Dimension>) -> StateModel {
    StateModel {
        parent: "scene://albit".into(),
        dimensions: dims,
        reachability: Reachability::Product { exclusions: vec![] },
    }
}

fn med(v: &mut [Duration]) -> Duration {
    v.sort_unstable();
    v[v.len() / 2]
}

fn every_state(inv: &Inventory<'_>, m: &StateModel) -> Res<Vec<Drawing>> {
    let paths = inv.decode_geometry()?;
    let radix = Radix::new(
        m.dimensions
            .iter()
            .map(|d| match &d.domain {
                goya_gvi::Domain::Discrete(v) => v.len() as u64,
                _ => 1,
            })
            .collect(),
    )
    .ok_or("radix")?;
    (0..radix.domain)
        .map(|k| Ok(inv.resolve(&radix.unpack(k))?.drawing(&paths)))
        .collect()
}

fn run(name: &str, src: &Drawing, m: &StateModel, bake: Bake, runs: usize) -> Res<()> {
    let (budget, gl) = (
        Budget {
            max_states: 1 << 20,
            ..Budget::default()
        },
        GvaLimits::default(),
    );
    let binding = m.binding(1, &budget)?;
    let plain = compile_with(src, m, &budget, &gl, bake)?;
    let reference = every_state(
        &Inventory::open(&plain, &binding, &GviLimits::default())?,
        m,
    )?;
    println!("{name} ({bake:?}): plain {} B", plain.len());
    println!(
        "  {:<22} {:>8} {:>12} {:>10} {:>10} {:>6} {:>4}",
        "config", "bytes", "build", "load", "lazy", "traj", "anc"
    );
    let configs = [
        ("GVI-D depth 8", 8, AlbitProfile::Off),
        ("GVI-D depth 32", 32, AlbitProfile::Off),
        ("GVI-A provenance d8", 8, AlbitProfile::Provenance),
        ("GVI-A exhaustive d8", 8, AlbitProfile::Exhaustive),
    ];
    for (label, depth, albit) in configs {
        let opts = CompileOptions {
            bake,
            max_diff_depth: depth,
            albit,
            operative: goya_gvi::OperativeProfile::Off,
        };
        let mut build = Vec::new();
        let mut bytes = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            bytes = compile_with(src, m, &budget, &gl, opts)?;
            build.push(t.elapsed());
        }
        if let Some(dir) = std::env::var_os("GVI_DUMP") {
            let file = format!(
                "{}-{bake:?}-{}.gvi",
                name.split_whitespace().next().unwrap_or("x"),
                label.replace(' ', "_")
            );
            std::fs::write(std::path::Path::new(&dir).join(file), &bytes)?;
        }
        let inv = Inventory::open(&bytes, &binding, &GviLimits::default())?;
        if every_state(&inv, m)? != reference {
            return Err(format!("{name} {label}: states differ from the plain inventory").into());
        }
        let mut load = Vec::new();
        for _ in 0..runs {
            let t = Instant::now();
            std::hint::black_box(inv.decode_geometry()?);
            load.push(t.elapsed());
        }
        let n = inv.geometry_count() as u64;
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut lazy = Vec::new();
        for _ in 0..2_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let g = (seed % n) as u32;
            let t = Instant::now();
            std::hint::black_box(inv.geometry(g)?);
            lazy.push(t.elapsed());
        }
        let l = inv.geometry_layout();
        println!(
            "  {label:<22} {:>8} {:>12.3?} {:>10.3?} {:>10.3?} {:>6} {:>4}",
            bytes.len(),
            med(&mut build),
            med(&mut load),
            med(&mut lazy),
            l.trajectory_records,
            l.max_ancestors
        );
    }
    let _ = compile;
    Ok(())
}

fn main() -> Res<()> {
    let star64 = ring(64, |i| if i % 2 == 0 { 40_000.0 } else { 22_000.0 })?;
    let circle64 = Arc::new(ring(64, |_| 32_000.0)?);
    let uniform = |n: u32| -> Vec<u32> { (0..n).map(|k| k * 65_536 / (n - 1)).collect() };
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
            Value::new("small").with(Effect::Transform(
                Selector::All,
                about_centre(FIXED_ONE / 2, 0, 0, FIXED_ONE / 2),
            )),
            Value::new("medium").with(Effect::Transform(
                Selector::All,
                about_centre(FIXED_ONE * 3 / 4, 0, 0, FIXED_ONE * 3 / 4),
            )),
            Value::new("full"),
        ],
    );
    let src = source(&star64);

    // A01: the existing 192-state workload.
    let a01 = model(vec![
        frames(&circle64, &uniform(32)),
        theme.clone(),
        sizes.clone(),
    ]);
    run("A01 192-state morph", &src, &a01, Bake::None, 31)?;
    run("A01 192-state morph", &src, &a01, Bake::Transforms, 31)?;

    // A02: 1,024 frames.
    let a02 = model(vec![frames(&circle64, &uniform(1024))]);
    run("A02 1,024-frame morph", &src, &a02, Bake::None, 5)?;

    // A03: eased (quadratic) fractions, 32 frames.
    let eased: Vec<u32> = (0..32u64)
        .map(|k| (65_536 * k * k / (31 * 31)) as u32)
        .collect();
    let a03 = model(vec![frames(&circle64, &eased), theme.clone()]);
    run("A03 eased fractions", &src, &a03, Bake::None, 31)?;

    // A07: baked rotation family, 16 frames × 8 rotations.
    let rotations = Dimension::discrete(
        "angle",
        (0..8)
            .map(|k| {
                let a = f64::from(k) * std::f64::consts::PI / 4.0;
                let (c, s) = (
                    (a.cos() * 65_536.0).round() as i32,
                    (a.sin() * 65_536.0).round() as i32,
                );
                let v = Value::new(&format!("r{k}"));
                if k == 0 {
                    v
                } else {
                    v.with(Effect::Transform(Selector::All, about_centre(c, s, -s, c)))
                }
            })
            .collect(),
    );
    let a07 = model(vec![frames(&circle64, &uniform(16)), rotations]);
    run("A07 baked rotations", &src, &a07, Bake::Transforms, 11)?;

    // A09: every frame morphs fully onto its own unrelated target.
    let targets: Vec<Arc<Path>> = (0..16u32)
        .map(|k| {
            ring(64, move |i| {
                20_000.0 + f64::from((i as u32 * 7 + k * 13) % 17) * 1_000.0
            })
            .map(Arc::new)
        })
        .collect::<Res<_>>()?;
    let a09 = model(vec![Dimension::discrete(
        "frame",
        targets
            .iter()
            .enumerate()
            .map(|(k, t)| {
                Value::new(&format!("f{k}")).with(Effect::Morph(Selector::All, t.clone(), 65_536))
            })
            .collect(),
    )]);
    run("A09 unpredictable", &src, &a09, Bake::None, 11)?;
    Ok(())
}
