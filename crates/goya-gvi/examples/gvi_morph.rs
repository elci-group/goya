//! Baked projections and path morphing on this machine. The workload is a
//! 64-cubic star that morphs into a circle over 32 discrete frames, in 2
//! themes and 3 relative sizes (192 states). It reports:
//!
//! - **dynamic:** interpolate the path, compose the transforms and build
//!   the drawing on every state change, as a runtime without GVI must;
//! - **GVI:** state validation, lookup and building the drawing from
//!   precomputed (morphed and, optionally, baked) geometry;
//! - inventory size and precompute time for each bake policy;
//! - the largest per-channel difference between baked and unbaked renders.
//!
//!     cargo run -p goya-gvi --example gvi_morph --release

use goya_graphics_ir::reference::{render, RenderLimits};
use goya_graphics_ir::{
    Affine, Drawing, Fill, FillRule, Item, Path, Point, Rgba8, Segment, Style, FIXED_ONE,
};
use goya_gva::GvaLimits;
use goya_gvi::{
    compile_with, morph_path, Bake, Budget, Dimension, Effect, GviLimits, Inventory, Radix,
    Reachability, Selector, StateModel, Value,
};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Res<T> = Result<T, Box<dyn Error>>;
const N: usize = 64;
const FRAMES: u32 = 32;
const C: f64 = 50_000.0;

/// A closed path of `N` cubics whose end points lie on `radius(i)`, with
/// tangent-ish controls. The same structure is used for star and circle.
fn ring(radius: impl Fn(usize) -> f64) -> Res<Path> {
    let p = |i: usize, r: f64| {
        let a = (i % N) as f64 / N as f64 * std::f64::consts::TAU;
        Point::new(
            (C + r * a.cos()).round() as i32,
            (C + r * a.sin()).round() as i32,
        )
    };
    let mut segs = vec![Segment::MoveTo(p(0, radius(0)))];
    for i in 0..N {
        let (r0, r1) = (radius(i), radius(i + 1));
        let a = p(i, r0);
        let b = p(i + 1, r1);
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

fn scale(q16: i32) -> Affine {
    let e = (C - f64::from(q16) * C / f64::from(FIXED_ONE)).round() as i32;
    Affine {
        a: q16,
        b: 0,
        c: 0,
        d: q16,
        e,
        f: e,
    }
}

fn pct(v: &[Duration], p: f64) -> Duration {
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn main() -> Res<()> {
    let star = ring(|i| if i % 2 == 0 { 40_000.0 } else { 22_000.0 })?;
    let circle = Arc::new(ring(|_| 32_000.0)?);
    let src = Drawing {
        width: 100_000,
        height: 100_000,
        items: vec![Item {
            path: Arc::new(star.clone()),
            style: fill(42, 130, 255),
            transform: Affine::IDENTITY,
            opacity: 255,
        }],
    };
    let frames = (0..FRAMES)
        .map(|k| {
            Value::new(&format!("f{k}")).with(Effect::Morph(
                Selector::All,
                circle.clone(),
                k * 65_536 / (FRAMES - 1),
            ))
        })
        .collect();
    let model = StateModel {
        parent: "scene://loader/spinner".into(),
        dimensions: vec![
            Dimension::discrete("frame", frames),
            Dimension::discrete(
                "theme",
                vec![
                    Value::new("blue"),
                    Value::new("amber").with(Effect::Restyle(Selector::All, fill(255, 170, 0))),
                ],
            ),
            Dimension::discrete(
                "size",
                vec![
                    Value::new("small")
                        .with(Effect::Transform(Selector::All, scale(FIXED_ONE / 2))),
                    Value::new("medium")
                        .with(Effect::Transform(Selector::All, scale(FIXED_ONE * 3 / 4))),
                    Value::new("full"),
                ],
            ),
        ],
        reachability: Reachability::Product { exclusions: vec![] },
    };
    let (budget, gl) = (Budget::default(), GvaLimits::default());
    let binding = model.binding(1, &budget)?;
    let radix = Radix::new(vec![u64::from(FRAMES), 2, 3]).ok_or("radix")?;
    let states: Vec<Vec<u16>> = (0..radix.domain).map(|k| radix.unpack(k)).collect();

    let mut inventories = Vec::new();
    for bake in [Bake::None, Bake::Transforms] {
        let mut times = Vec::new();
        let mut bytes = Vec::new();
        for _ in 0..31 {
            let t = Instant::now();
            bytes = compile_with(&src, &model, &budget, &gl, bake)?;
            times.push(t.elapsed());
        }
        times.sort_unstable();
        println!(
            "{bake:?}: inventory {} B, precompute median {:.3?} (31 runs)",
            bytes.len(),
            pct(&times, 0.5)
        );
        inventories.push(bytes);
    }

    // Dynamic baseline: the canonical star is origin-folded by GVA, so morph
    // the source path and apply the size transform per state change.
    let dynamic = |s: &[u16]| -> Option<Drawing> {
        let t = s[0] as u32 * 65_536 / (FRAMES - 1);
        let path = morph_path(&star, &circle, t)?;
        let style = if s[1] == 1 {
            fill(255, 170, 0)
        } else {
            fill(42, 130, 255)
        };
        let transform = match s[2] {
            0 => scale(FIXED_ONE / 2),
            1 => scale(FIXED_ONE * 3 / 4),
            _ => Affine::IDENTITY,
        };
        Some(Drawing {
            width: 100_000,
            height: 100_000,
            items: vec![Item {
                path: Arc::new(path),
                style,
                transform,
                opacity: 255,
            }],
        })
    };

    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let order: Vec<usize> = (0..100_000)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % radix.domain) as usize
        })
        .collect();
    let mut dyn_t = Vec::new();
    for &i in &order {
        let t = Instant::now();
        std::hint::black_box(dynamic(&states[i]).ok_or("morph")?);
        dyn_t.push(t.elapsed());
    }
    dyn_t.sort_unstable();
    println!(
        "dynamic morph + drawing      median {:>9.3?} p99 {:>9.3?}",
        pct(&dyn_t, 0.5),
        pct(&dyn_t, 0.99)
    );

    let mut opened = Vec::new();
    for (bytes, name) in inventories
        .iter()
        .zip(["GVI lookup + drawing", "GVI baked lookup + drawing"])
    {
        let inv = Inventory::open(bytes, &binding, &GviLimits::default())?;
        let paths = inv.decode_geometry()?;
        let mut t_all = Vec::new();
        for &i in &order {
            let t = Instant::now();
            std::hint::black_box(inv.resolve(&states[i])?.drawing(&paths));
            t_all.push(t.elapsed());
        }
        t_all.sort_unstable();
        println!(
            "{name:<28} median {:>9.3?} p99 {:>9.3?}",
            pct(&t_all, 0.5),
            pct(&t_all, 0.99)
        );
        opened.push((inv, paths));
    }

    let rl = RenderLimits::default();
    let mut worst = 0u8;
    let mut differing = 0usize;
    for s in &states {
        let a = render(&opened[0].0.resolve(s)?.drawing(&opened[0].1), 1.0, &rl)?;
        let b = render(&opened[1].0.resolve(s)?.drawing(&opened[1].1), 1.0, &rl)?;
        let d = a.max_premultiplied_diff(&b).ok_or("size mismatch")?;
        worst = worst.max(d);
        differing += a
            .rgba
            .chunks(4)
            .zip(b.rgba.chunks(4))
            .filter(|(x, y)| x != y)
            .count();
    }
    println!("baked vs unbaked at 1 px/pt over {} states: max premultiplied channel diff {worst} levels, {differing} differing pixels", states.len());
    Ok(())
}
