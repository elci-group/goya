//! Emojified source projection (§10).
//!
//! The renderer inserts deterministic emoji markers with annotation tags at
//! valid display positions, preserving all original bytes, whitespace and
//! line structure. The projection is a derived representation; the artefact's
//! `source` section remains authoritative.

use crate::model::{annotation_sort_key, Annotation};
use std::collections::BTreeMap;

pub const RENDERER_VERSION: &str = "escene-render/1.0.0";

/// Presentation density (§10.4). Filtering is view-level only and never
/// mutates the underlying artefact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderProfile {
    /// Emoji only.
    Minimal,
    /// Emoji and annotation identity (default).
    Standard,
    /// Exact unannotated source.
    Source,
}

/// Render the emojified projection for `annotations` over `source`.
///
/// Line-anchored strategy: annotations whose span starts on a line are
/// inserted at the head of that line, ordered by the §10.3 rule. Span-less
/// annotations are rendered in a header block, never falsely attached to a
/// source position (§10.3). Insertion never splits a UTF-8 sequence because
/// insertions happen at line starts (char boundaries).
pub fn render_projection(
    source: &crate::source::SourceSnapshot,
    annotations: &[Annotation],
    profile: RenderProfile,
) -> Result<String, String> {
    // Always render from the authoritative payload (§7.1): the in-memory
    // `bytes` mirror is optional, the base64 source is not.
    let bytes = source.recover().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    if profile == RenderProfile::Source {
        return Ok(text.into_owned());
    }

    // Group annotations by starting line of their span.
    let mut by_line: BTreeMap<usize, Vec<&Annotation>> = BTreeMap::new();
    let mut header: Vec<&Annotation> = Vec::new();
    for ann in annotations {
        match &ann.span {
            Some(span) => {
                let line = text.as_bytes()[..span.start_byte]
                    .iter()
                    .filter(|&&b| b == b'\n')
                    .count();
                by_line.entry(line).or_default().push(ann);
            }
            None => header.push(ann),
        }
    }
    for group in by_line.values_mut() {
        group.sort_by_key(|a| annotation_sort_key(a));
    }

    let mut out = String::new();
    for (line_no, line) in text.lines().enumerate() {
        if line_no == 0 && !header.is_empty() {
            for ann in &header {
                out.push_str(&marker(ann, profile));
                out.push(' ');
            }
        }
        if let Some(group) = by_line.get(&line_no) {
            for ann in group {
                out.push_str(&marker(ann, profile));
                out.push(' ');
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    // Preserve a trailing-newline-free source exactly (no invented final \n).
    if !bytes.ends_with(b"\n") && out.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

fn marker(ann: &Annotation, profile: RenderProfile) -> String {
    match profile {
        RenderProfile::Minimal => ann.emoji.clone(),
        RenderProfile::Standard => format!("{}[{}]", ann.emoji, ann.display_tag()),
        RenderProfile::Source => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Annotation, Scope, Severity};
    use crate::registry::AnnotationClass as AC;
    use crate::registry::{AnnotationClass, DiagnosticId};
    use crate::source::{Precision, SourceSnapshot, Span};

    fn src() -> SourceSnapshot {
        SourceSnapshot::capture(
            "src-1",
            "Doom.gscene",
            b"block render_world {\n    resolve_geometry()\n    resolve_textures()\n}\n".to_vec(),
        )
    }

    fn ann(id: u32, class: AnnotationClass, code: &str, emoji: &str, start: usize, end: usize) -> Annotation {
        let s = src();
        Annotation {
            id: format!("ann-{id:04}"),
            class,
            code: DiagnosticId::new(class, code),
            emoji: emoji.into(),
            severity: Severity::Info,
            scope: Scope::Block,
            span: Some(Span { source_id: s.id.clone(), start_byte: start, end_byte: end, precision: Precision::Exact }),
            rule_id: "RULE X.v1".into(),
            evidence_refs: vec!["ev-1".into()],
            span_precision: Some(Precision::Exact),
        }
    }

    #[test]
    fn projection_matches_directive_example() {
        // Directive §10.1: block annotation + three expression annotations.
        let s = src();
        // bytes: "block render_world {\n" = 0..21; line 1 starts at 21.
        let annotations = vec![
            ann(1, AC::C, "NATIVE_LOWERING", "⚙️", 0, 21),
            ann(3, AC::C, "PRECOMPUTED", "🧮", 21, 40),
            ann(4, AC::S, "COMPUTATIONAL_REUSE", "♻️", 21, 40),
        ];
        let out = render_projection(&s, &annotations, RenderProfile::Standard).unwrap();
        // Markers are inserted at line starts; original indentation follows them.
        let expected = "⚙️[C:0001] block render_world {\n🧮[C:0003] ♻️[S:0004]     resolve_geometry()\n    resolve_textures()\n}\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_order_follows_spec() {
        // §10.3: offset, longest enclosing span first, then C/I/S, code, id.
        let s = src();
        // Two annotations on the same line: one enclosing a longer span ranks
        // first despite identical offsets.
        let a = ann(1, AC::I, "LATENCY_EXCEEDED", "⏳", 21, 40);
        let mut b = ann(2, AC::I, "BOTTLENECK", "🚧", 21, 40);
        b.span = Some(Span { source_id: s.id.clone(), start_byte: 21, end_byte: 30, precision: Precision::Exact });
        let out = render_projection(&s, &[a.clone(), b.clone()], RenderProfile::Standard).unwrap();
        // a has the longer span (19 bytes) → first.
        assert!(out.contains("⏳[I:0001] 🚧[I:0002]"), "got {out:?}");
        let out_rev = render_projection(&s, &[b, a], RenderProfile::Standard).unwrap();
        assert_eq!(out, out_rev, "order is deterministic regardless of input order");
    }

    #[test]
    fn spanless_annotations_render_in_header() {
        let s = src();
        let mut a = ann(9, AC::I, "UNOBSERVED", "👻", 0, s.bytes.len());
        a.span = None;
        let out = render_projection(&s, &[a], RenderProfile::Standard).unwrap();
        assert!(out.starts_with("👻[I:0009] "));
        assert!(out.contains("block render_world"));
    }

    #[test]
    fn source_profile_is_exact() {
        let s = src();
        assert_eq!(render_projection(&s, &[], RenderProfile::Source).unwrap(), String::from_utf8(s.bytes.clone()).unwrap());
    }
}

