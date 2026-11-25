//! `INDX` forms on this machine (gvi.md §4.2): size and warm `resolve`
//! latency for each key form, against the earlier fixed-width layout
//! (`u64` keys plus `u32` ids, binary-searched).
//!
//! Three 40 × 40 × 40 models (64,000-state domain), each with trivial
//! graphics so that only the index varies:
//!
//! - **identity:** full product, every key `0..n`;
//! - **bitmap:** product excluding states where `a = b` (dense, with holes);
//! - **packed:** transitions reaching only the diagonal staircase (sparse).
//!
//!     cargo run -p goya-gvi --example gvi_index --release

use goya_graphics_ir::{Affine, Drawing, Fill, FillRule, Item, Path, Point, Rgba8, Segment, Style};
use goya_gva::GvaLimits;
use goya_gvi::{
    compile, Budget, Dimension, GviLimits, Inventory, Radix, Reachability, Rule, StateModel, Value,
    INDX,
};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Res<T> = Result<T, Box<dyn Error>>;
const R: u16 = 40;

fn source() -> Res<Drawing> {
    use Segment::*;
    let p = Point::new;
    let path = Path::new(vec![
        MoveTo(p(0, 0)),
        LineTo(p(1000, 0)),
        LineTo(p(1000, 1000)),
        Close,
    ])?;
    Ok(Drawing {
        width: 1000,
        height: 1000,
        items: vec![Item {
            path: Arc::new(path),
            style: Style {
                fill: Some(Fill {
                    colour: Rgba8::new(1, 2, 3, 255),
                    rule: FillRule::NonZero,
                }),
                stroke: None,
            },
            transform: Affine::IDENTITY,
            opacity: 255,
        }],
    })
}

fn model(reach: Reachability) -> StateModel {
    let dim =
        |n: &str| Dimension::discrete(n, (0..R).map(|v| Value::new(&format!("v{v}"))).collect());
    StateModel {
        parent: "scene://index".into(),
        dimensions: vec![dim("a"), dim("b"), dim("c")],
        reachability: reach,
    }
}

fn main() -> Res<()> {
    let src = source()?;
    let budget = Budget {
        max_states: 1 << 20,
        ..Budget::default()
    };
    let mut staircase = Vec::new();
    for v in 0..R - 1 {
        staircase.push(Rule {
            dim: 0,
            from: Some(v),
            to: v + 1,
            guard: vec![(1, v), (2, v)],
        });
        staircase.push(Rule {
            dim: 1,
            from: Some(v),
            to: v + 1,
            guard: vec![(0, v + 1), (2, v)],
        });
        staircase.push(Rule {
            dim: 2,
            from: Some(v),
            to: v + 1,
            guard: vec![(0, v + 1), (1, v + 1)],
        });
    }
    let cases = [
        ("identity", Reachability::Product { exclusions: vec![] }),
        (
            "bitmap",
            Reachability::Product {
                exclusions: (0..R).map(|v| vec![(0, v), (1, v)]).collect(),
            },
        ),
        (
            "packed",
            Reachability::Transitions {
                initial: vec![0, 0, 0],
                rules: staircase,
            },
        ),
    ];
    let radix = Radix::new(vec![u64::from(R); 3]).ok_or("radix")?;
    println!(
        "{:<9} {:>7} {:>11} {:>11} {:>16} {:>16}",
        "form", "states", "INDX bytes", "fixed bytes", "resolve median", "fixed search"
    );
    for (name, reach) in cases {
        let m = model(reach);
        let bytes = compile(&src, &m, &budget, &GvaLimits::default())?;
        let inv = Inventory::open(&bytes, &m.binding(1, &budget)?, &GviLimits::default())?;
        // INDX size, read from the container directory.
        let indx_len = {
            let n = u32::from_le_bytes(bytes[32..36].try_into()?) as usize;
            (0..n)
                .map(|i| 80 + 72 * i)
                .find(|&e| bytes[e..e + 4] == INDX.bytes())
                .map(|e| u64::from_le_bytes(bytes[e + 16..e + 24].try_into().unwrap_or([0; 8])))
                .unwrap_or(0)
        };
        // Reachable keys, recovered through the public API.
        let states: Vec<Vec<u16>> = (0..radix.domain)
            .map(|k| radix.unpack(k))
            .filter(|s| inv.resolve(s).is_ok())
            .collect();
        let keys: Vec<u64> = (0..radix.domain)
            .filter(|&k| inv.resolve(&radix.unpack(k)).is_ok())
            .collect();
        let n = keys.len();
        let fixed = 16 + (n * 8).div_ceil(16) * 16 + (n * 4).div_ceil(16) * 16;
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let order: Vec<usize> = (0..200_000)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed % n as u64) as usize
            })
            .collect();
        let mut a = Vec::with_capacity(order.len());
        for &i in &order {
            let t = Instant::now();
            std::hint::black_box(inv.resolve(&states[i])?);
            a.push(t.elapsed());
        }
        // The fixed layout's cost: pack the state, then binary-search u64 keys.
        let mut b = Vec::with_capacity(order.len());
        for &i in &order {
            let t = Instant::now();
            let k = radix.pack(&states[i]).ok_or("pack")?;
            std::hint::black_box(keys.binary_search(&k).ok());
            b.push(t.elapsed());
        }
        let med = |v: &mut Vec<Duration>| {
            v.sort_unstable();
            v[v.len() / 2]
        };
        println!(
            "{name:<9} {n:>7} {indx_len:>11} {fixed:>11} {:>16.3?} {:>16.3?}",
            med(&mut a),
            med(&mut b)
        );
    }
    println!("(resolve includes state validation and packing; fixed search is packing plus binary search only)");
    Ok(())
}
