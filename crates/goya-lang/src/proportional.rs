//! Source-to-source migration from absolute to proportional lengths.
//!
//! GSSH/3 admits no absolute lengths (decision D7 in `docs/gssh/decisions.md`).
//! [`to_proportional`] rewrites `position`, `extent`, `text-size` and
//! `stroke-width` as `position_p`, `extent_p`, `text-size-p` and
//! `stroke-width-p`, choosing each fraction so that the reference layout at
//! one reference viewport is unchanged. Only the rewritten property spans are
//! edited; comments and formatting elsewhere are preserved.
//!
//! The result is exact at the reference viewport and scales with the parent
//! at every other size, which is the point of the migration. Where a value
//! cannot be reproduced at the reference viewport (for example an absolute
//! element inside a zero-sized parent), the nearest fraction is written and
//! the property is reported in [`Conversion::inexact`].

use crate::scene::{layout, Frame, SceneCell, MP};
use crate::{parse_with_source_map, ParseError, SourceRange};

/// The converted source and every property it could not reproduce exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub source: String,
    pub inexact: Vec<Inexact>,
}

/// The element (for text size and stroke width) or its parent (for
/// position and extent) has zero size at the reference viewport, so no
/// fraction of it can reproduce a non-zero length.
pub const ZERO_BASE: &str = "zero-sized base at the reference viewport";
/// Any other length that no fraction reproduces.
pub const NO_FRACTION: &str = "no fraction reproduces this length at the reference viewport";

/// A property whose proportional replacement does not reproduce the
/// reference layout at the reference viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inexact {
    /// Range of the original property in the input source.
    pub range: SourceRange,
    pub property: String,
    pub reason: &'static str,
}

struct Plan<'a> {
    cell: &'a SceneCell,
    path: Vec<usize>,
    parent: Option<usize>,
    overflow: bool,
}

fn collect<'a>(
    cell: &'a SceneCell,
    path: Vec<usize>,
    parent: Option<usize>,
    overflow: bool,
    out: &mut Vec<Plan<'a>>,
) {
    let index = out.len();
    out.push(Plan {
        cell,
        path: path.clone(),
        parent,
        overflow,
    });
    for (n, child) in cell.children.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(n);
        collect(child, child_path, Some(index), cell.scroll, out);
    }
}

fn scale(value: f64) -> i64 {
    value as i64
}

/// Fraction `f` with `trunc(f * parent) == value`, or `None` if impossible.
fn exact_fraction(value: i64, parent: i64) -> Option<f64> {
    if value == 0 {
        return Some(0.0);
    }
    if parent <= 0 || value < 0 {
        return None;
    }
    let f = (value as f64 + 0.5) / parent as f64;
    (scale(f * parent as f64) == value).then_some(f)
}

/// A fraction for an extent or position offset `value` within `parent`.
/// Outside a scroll viewport the reference clamps both to the parent, so
/// anything at or beyond the parent edge becomes 1 and anything at or
/// before its origin becomes 0, with the same result.
fn length_fraction(value: i64, parent: i64, overflow: bool) -> (f64, bool) {
    if value <= 0 {
        return (0.0, true);
    }
    if !overflow && value >= parent {
        return (1.0, true);
    }
    match exact_fraction(value, parent) {
        Some(f) => (f, true),
        None => (0.0, false),
    }
}

fn number(value: f64) -> String {
    // `Display` never uses an exponent, which GY-LANG numbers do not allow.
    let text = format!("{value}");
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

/// Convert every absolute length in `source` to a proportional one that
/// reproduces the reference layout at `width_pt` × `height_pt`.
///
/// Accepts anything the compatibility parser accepts; profile admission is
/// the caller's next step (`parse_strict_gssh3`).
pub fn to_proportional(
    source: &str,
    width_pt: f64,
    height_pt: f64,
) -> Result<Conversion, ParseError> {
    let parsed = parse_with_source_map(source)?;
    let scene = &parsed.scene;
    let frame = layout(scene, width_pt, height_pt);
    let mut plans = Vec::new();
    collect(&scene.root, Vec::new(), None, false, &mut plans);
    let viewport = ((width_pt * MP) as i64, (height_pt * MP) as i64);
    let parent_size = |plan: &Plan<'_>| match plan.parent {
        Some(p) => (frame.nodes[p].rect.width, frame.nodes[p].rect.height),
        None => viewport,
    };

    let mut edits: Vec<(SourceRange, String)> = Vec::new();
    let mut inexact = Vec::new();
    for entry in &parsed.entries {
        if entry.kind != "property" {
            continue;
        }
        let canonical = entry.name.replace('-', "_");
        if !matches!(
            canonical.as_str(),
            "position" | "extent" | "text_size" | "stroke_width"
        ) {
            continue;
        }
        let Some(index) = entry
            .scene_path
            .as_ref()
            .and_then(|path| plans.iter().position(|plan| &plan.path == path))
        else {
            continue;
        };
        let plan = &plans[index];
        let cell = plan.cell;
        let original = &source[entry.range.start..entry.range.end];
        let terminator = if original.trim_end().ends_with(';') {
            ";"
        } else {
            ""
        };
        let mut failure: Option<&'static str> = None;
        let replacement = match canonical.as_str() {
            // A proportional form already present wins in the reference
            // layout, so the absolute one has no effect: drop it.
            "position" if cell.position_p.is_some() => String::new(),
            "extent" if cell.extent_p.is_some() => String::new(),
            "text_size" if cell.text_size_p.is_some() => String::new(),
            "stroke_width" if cell.stroke_width_p.is_some() => String::new(),
            "position" | "extent" => {
                let vector = if canonical == "position" {
                    cell.position
                } else {
                    cell.extent
                }
                .expect("parsed property has a value");
                let (pw, ph) = parent_size(plan);
                let (fx, ex) = length_fraction(scale(vector.x * MP), pw, plan.overflow);
                let (fy, ey) = length_fraction(scale(vector.y * MP), ph, plan.overflow);
                if !(ex && ey) {
                    failure = Some(if (!ex && pw <= 0) || (!ey && ph <= 0) {
                        ZERO_BASE
                    } else {
                        NO_FRACTION
                    });
                }
                format!(
                    "{canonical}_p: ({}, {}, {}){terminator}",
                    number(fx),
                    number(fy),
                    number(vector.z)
                )
            }
            "text_size" => {
                let value = scale(cell.text_size.expect("parsed") * MP);
                let height = frame.nodes[index].rect.height;
                let fraction = exact_fraction(value, height).unwrap_or_else(|| {
                    failure = Some(if height <= 0 { ZERO_BASE } else { NO_FRACTION });
                    0.0
                });
                format!("text-size-p: {}{terminator}", number(fraction))
            }
            _ => {
                let value = scale(cell.stroke_width.expect("parsed") * MP);
                let rect = frame.nodes[index].rect;
                let base = rect.width.min(rect.height);
                let fraction = exact_fraction(value, base)
                    .map(|f| f.min(0.5))
                    .filter(|f| scale(f * base as f64) == value)
                    .unwrap_or_else(|| {
                        failure = Some(if base <= 0 { ZERO_BASE } else { NO_FRACTION });
                        0.5
                    });
                format!("stroke-width-p: {}{terminator}", number(fraction))
            }
        };
        if let Some(reason) = failure {
            inexact.push(Inexact {
                range: entry.range,
                property: entry.name.clone(),
                reason,
            });
        }
        edits.push((entry.range, replacement));
    }

    let mut output = source.to_owned();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in edits {
        output.replace_range(range.start..range.end, &replacement);
    }

    // Independent check: the converted scene must lay out identically at
    // the reference viewport, element for element.
    let converted = parse_with_source_map(&output)?;
    let after = layout(&converted.scene, width_pt, height_pt);
    if !same_layout(&frame, &after) && inexact.is_empty() {
        inexact.push(Inexact {
            range: SourceRange {
                start: 0,
                end: source.len(),
            },
            property: String::new(),
            reason: "the converted scene lays out differently at the reference viewport",
        });
    }
    Ok(Conversion {
        source: output,
        inexact,
    })
}

fn same_layout(a: &Frame, b: &Frame) -> bool {
    a.nodes.len() == b.nodes.len()
        && a.nodes.iter().zip(&b.nodes).all(|(x, y)| {
            x.rect == y.rect
                && x.text_size_mp == y.text_size_mp
                && x.stroke_width_mp == y.stroke_width_mp
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_lengths_and_keeps_the_reference_layout() {
        let source = "@ontology a { @cell root { id: \"r\"; // keep me\n @cell c { id: \"c\"; position: (10, 20, 0); extent: (100.5, 33.3, 0); text: \"x\"; text-size: 12; stroke: #000; stroke-width: 2; } @cell s { id: \"s\"; extent: (200, 100, 0); scroll: true; @cell big { id: \"big\"; position: (0, 50, 0); extent: (200, 900, 0); } } } }";
        let conversion = to_proportional(source, 400.0, 300.0).unwrap();
        assert!(conversion.inexact.is_empty(), "{:?}", conversion.inexact);
        let out = &conversion.source;
        assert!(out.contains("// keep me"));
        assert!(!out.contains("position:") && !out.contains("extent:"));
        assert!(!out.contains("text-size:") && !out.contains("stroke-width:"));
        let admitted = crate::parse_strict_gssh3(out).unwrap();
        let before = layout(&crate::parse(source).unwrap(), 400.0, 300.0);
        assert!(same_layout(&before, &layout(&admitted.scene, 400.0, 300.0)));
        // Inside the scroll viewport the content keeps overflowing (> 1).
        assert!(
            out.contains("extent_p: (1.0000025, 9.000005, 0.0)"),
            "{out}"
        );
    }

    #[test]
    fn reports_lengths_it_cannot_reproduce() {
        // A zero-height element gives a text size nothing to be a fraction of.
        let source = "@ontology a { @cell root { id: \"r\"; extent: (100, 0, 0); text: \"x\"; text-size: 40; } }";
        let conversion = to_proportional(source, 400.0, 300.0).unwrap();
        assert_eq!(conversion.inexact.len(), 1);
        assert_eq!(conversion.inexact[0].property, "text-size");
        assert_eq!(conversion.inexact[0].reason, ZERO_BASE);
    }
}
