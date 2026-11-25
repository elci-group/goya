//! Storage measurement for GOYA-GFX-001 workloads B01 (one static icon) and
//! B02 (10,000 repeated instances). The baseline is plain SVG text that
//! writes every shape with absolute coordinates, as typical exporters do.
//! This is a synthetic, size-only measurement: it says nothing about
//! decode or render speed, and it is not compared against an optimised
//! renderer.
//!
//!     cargo run -p goya-gva --example gva_storage --release

use goya_graphics_ir::{Affine, Drawing, Fill, FillRule, Item, Path, Point, Rgba8, Segment, Style};
use goya_gva::{encode, Gva, GvaLimits};
use std::error::Error;
use std::sync::Arc;

type Res<T> = Result<T, Box<dyn Error>>;

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

fn num(mp: i32) -> String {
    let s = format!("{:.3}", f64::from(mp) / 1000.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn svg(d: &Drawing) -> String {
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\">",
        num(d.width as i32),
        num(d.height as i32)
    );
    for it in &d.items {
        let mut path = String::new();
        for s in it.path.segments() {
            let pt = |p: &Point| format!("{} {}", num(p.x), num(p.y));
            path += &match s {
                Segment::MoveTo(p) => format!("M{}", pt(p)),
                Segment::LineTo(p) => format!("L{}", pt(p)),
                Segment::QuadTo(c, p) => format!("Q{} {}", pt(c), pt(p)),
                Segment::CubicTo(a, b, p) => format!("C{} {} {}", pt(a), pt(b), pt(p)),
                Segment::Close => "Z".into(),
            };
        }
        let c = it
            .style
            .fill
            .map(|f| f.colour)
            .unwrap_or(Rgba8::new(0, 0, 0, 0));
        out += &format!(
            "<path d=\"{path}\" fill=\"#{:02x}{:02x}{:02x}\"/>",
            c.r, c.g, c.b
        );
    }
    out + "</svg>"
}

fn report(name: &str, d: &Drawing) -> Res<()> {
    let gva = encode(d, &GvaLimits::default())?;
    let opened = Gva::open(&gva, &GvaLimits::default())?;
    let svg = svg(d);
    let reduction = 100.0 * (1.0 - gva.len() as f64 / svg.len() as f64);
    println!(
        "{name}: {} instances, {} stored geometries | SVG text {} B | GVA {} B | {reduction:+.1}% vs SVG",
        d.items.len(),
        opened.geometry_count(),
        svg.len(),
        gva.len()
    );
    Ok(())
}

fn main() -> Res<()> {
    let blue = Style {
        fill: Some(Fill {
            colour: Rgba8::new(42, 130, 255, 255),
            rule: FillRule::NonZero,
        }),
        stroke: None,
    };
    let one = Drawing {
        width: 24_000,
        height: 24_000,
        items: vec![Item {
            path: Arc::new(circle(12_000, 12_000, 7_000)?),
            style: blue,
            transform: Affine::IDENTITY,
            opacity: 255,
        }],
    };
    report("B01 single icon", &one)?;

    let mut items = Vec::new();
    for j in 0..100 {
        for i in 0..100 {
            items.push(Item {
                path: Arc::new(circle(i * 10_000 + 5_000, j * 10_000 + 5_000, 4_000)?),
                style: blue,
                transform: Affine::IDENTITY,
                opacity: 255,
            });
        }
    }
    let many = Drawing {
        width: 1_000_000,
        height: 1_000_000,
        items,
    };
    report("B02 10,000 repeated instances", &many)
}
