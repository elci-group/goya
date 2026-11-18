//! Expected on-screen bounds of a compiled scene on Android, from the Rust
//! reference (`goya_lang::layout`, `Frame::scroll_limits`) and Aragon's pixel
//! rule. Used by `scripts/check-android-device.py` to check the accessibility
//! tree of a running app.
//!
//! ```text
//! aragon_device_expect <scene> <left> <top> <width> <height> <density>
//!     <screen-width> <screen-height> [--scroll-max <element id> <x|y|xy>]...
//! ```
//!
//! `left top width height` is the viewport in screen pixels (the area inside
//! the system bars). Output: one line per element, depth-first:
//! `<id> <parent id|-> <cell|button|input> <text|button|input|none>
//!  <scroll 0|1> <scrollX> <scrollY> <l> <t> <r> <b>`, where the rectangle is
//! the element's visible bounds on screen (empty when it is not visible).

use goya_lang::{CellKind, SceneCell};

/// `java.lang.Math.round(double)`: nearest, ties toward positive infinity.
fn java_round(value: f64) -> i64 {
    let floor = value.floor();
    let rounded = if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    rounded as i64
}

fn px(mp: i64, density: f64) -> i64 {
    java_round(mp as f64 * density / 1000.0).clamp(i32::MIN as i64, i32::MAX as i64)
}

#[derive(Clone, Copy)]
struct Rect {
    l: i64,
    t: i64,
    r: i64,
    b: i64,
}

impl Rect {
    fn intersect(self, other: Rect) -> Rect {
        let l = self.l.max(other.l);
        let t = self.t.max(other.t);
        Rect {
            l,
            t,
            r: self.r.min(other.r).max(l),
            b: self.b.min(other.b).max(t),
        }
    }
}

fn collect<'a>(
    cell: &'a SceneCell,
    parent: Option<usize>,
    out: &mut Vec<(&'a SceneCell, Option<usize>)>,
) {
    let index = out.len();
    out.push((cell, parent));
    for child in &cell.children {
        collect(child, Some(index), out);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 8 {
        eprintln!("usage: aragon_device_expect <scene> <left> <top> <width> <height> <density> <screen-width> <screen-height> [--scroll-max <id> <x|y|xy>]...");
        std::process::exit(2);
    }
    let number = |index: usize| -> i64 { args[index].parse().expect("integer argument") };
    let source = std::fs::read_to_string(&args[0]).expect("readable scene");
    let (left, top, width, height) = (number(1), number(2), number(3), number(4));
    // The app reads displayMetrics.density as a Float and widens it.
    let density = f64::from(args[5].parse::<f32>().expect("density"));
    let screen = Rect {
        l: 0,
        t: 0,
        r: number(6),
        b: number(7),
    };
    let mut scroll_max = Vec::new();
    let mut rest = args[8..].iter();
    while let Some(flag) = rest.next() {
        assert_eq!(flag, "--scroll-max", "unknown option {flag}");
        let id = rest.next().expect("element id").clone();
        let axes = rest.next().expect("axes x, y or xy").clone();
        scroll_max.push((id, axes.contains('x'), axes.contains('y')));
    }

    let handoff = goya_lang::handoff::admit_v3(&source).expect("GSSH/3 scene");
    let scene = &handoff.parsed().scene;
    let mut cells = Vec::new();
    collect(&scene.root, None, &mut cells);
    let frame = goya_lang::layout(scene, width as f64 / density, height as f64 / density);
    let mut screen_rects: Vec<Rect> = Vec::new();
    let mut visible: Vec<Rect> = Vec::new();
    let mut scroll: Vec<(i64, i64)> = Vec::new();
    for (index, ((cell, parent), node)) in cells.iter().zip(&frame.nodes).enumerate() {
        let r = node.rect;
        let edges = Rect {
            l: px(r.x, density),
            t: px(r.y, density),
            r: px(r.x.saturating_add(r.width), density),
            b: px(r.y.saturating_add(r.height), density),
        };
        // Children are placed relative to their container and shifted by
        // every scrolled ancestor layer.
        let (mut dx, mut dy) = (left, top);
        let mut ancestor = *parent;
        while let Some(a) = ancestor {
            dx -= scroll[a].0;
            dy -= scroll[a].1;
            ancestor = cells[a].1;
        }
        let on_screen = Rect {
            l: edges.l + dx,
            t: edges.t + dy,
            r: edges.r + dx,
            b: edges.b + dy,
        };
        let clip = parent.map_or(screen, |p| visible[p]);
        screen_rects.push(on_screen);
        visible.push(on_screen.intersect(clip));
        let id = cell.authored_id.clone().unwrap_or_default();
        let axes = scroll_max
            .iter()
            .find(|(element, _, _)| *element == id)
            .map_or((false, false), |(_, x, y)| (*x, *y));
        let offsets = if cell.scroll {
            let (lx, ly) = frame.scroll_limits(&cell.name);
            let right = r.x.saturating_add(r.width);
            let bottom = r.y.saturating_add(r.height);
            (
                if axes.0 {
                    (px(right.saturating_add(lx), density) - edges.r).max(0)
                } else {
                    0
                },
                if axes.1 {
                    (px(bottom.saturating_add(ly), density) - edges.b).max(0)
                } else {
                    0
                },
            )
        } else {
            (0, 0)
        };
        scroll.push(offsets);
        let kind = match cell.kind {
            CellKind::Cell => "cell",
            CellKind::Button => "button",
            CellKind::Input => "input",
        };
        let content = match cell.kind {
            CellKind::Cell if cell.text.is_some() => "text",
            CellKind::Cell => "none",
            CellKind::Button => "button",
            CellKind::Input => "input",
        };
        let parent_id = parent.map_or("-".to_owned(), |p| {
            cells[p].0.authored_id.clone().unwrap_or_default()
        });
        let v = visible[index];
        println!(
            "{id} {parent_id} {kind} {content} {} {} {} {} {} {} {}",
            u8::from(cell.scroll),
            offsets.0,
            offsets.1,
            v.l,
            v.t,
            v.r,
            v.b
        );
    }
}
