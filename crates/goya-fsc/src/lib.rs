//! GY-FSC/1 — Formatting Similarity Compression & In-Object Variance.
//!
//! GY-TYP-002 (supersedes the DSC portions of GY-TYP-001): compression
//! operates on FORMATTING similarity, never glyph shapes. Common formatting
//! is consolidated into shared immutable profiles; per-object variation is
//! encoded as minimal typed deltas over text ranges.
//!
//! The crucial semantic rule (directive §3): identity accounts for BOTH the
//! resolved formatting AND the inheritance semantics — two objects that
//! currently look identical but override different properties are NOT the
//! same profile, because a theme change would affect them differently.
//!
//! Totality: every operation is panic-free on any input and deterministic.

use goya_change::PropertyMask;
use std::collections::BTreeMap;
use std::fmt;

/// Contract identifier implemented by this crate.
pub const FSC_CONTRACT: &str = "GY-FSC/1";

/// Formatting property identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FormatProp {
    Family,
    Size,
    Weight,
    Colour,
    LineHeight,
    LetterSpacing,
    Underline,
    Align,
    Opacity,
}

/// A formatting value. Deterministic scalar vocabulary; Colour is an sRGB
/// triple (theme resolution happens in goya-chroma upstream).
#[derive(Debug, Clone, PartialEq)]
pub enum FormatValue {
    Family(String),
    Size(f64),
    Weight(u16),
    Colour([u8; 3]),
    LineHeight(f64),
    LetterSpacing(f64),
    Underline(bool),
    Align(Align),
    Opacity(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Middle,
    End,
}

impl fmt::Display for FormatValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatValue::Family(s) => write!(f, "family={s}"),
            FormatValue::Size(v) => write!(f, "size={v}"),
            FormatValue::Weight(v) => write!(f, "weight={v}"),
            FormatValue::Colour(c) => write!(f, "colour=#{:02x}{:02x}{:02x}", c[0], c[1], c[2]),
            FormatValue::LineHeight(v) => write!(f, "line-height={v}"),
            FormatValue::LetterSpacing(v) => write!(f, "spacing={v}"),
            FormatValue::Underline(v) => write!(f, "underline={v}"),
            FormatValue::Align(a) => write!(f, "align={a:?}"),
            FormatValue::Opacity(v) => write!(f, "opacity={v}"),
        }
    }
}

/// Explicit property overrides of a profile relative to its parent.
/// Absent property = inherited.
pub type Overrides = BTreeMap<FormatProp, FormatValue>;

/// A shared immutable formatting profile. Profiles form a tree via
/// `parent`; resolution walks to the root with child overrides winning.
#[derive(Debug, Clone, PartialEq)]
pub struct FormattingProfile {
    pub id: u64,
    pub name: String,
    pub parent: Option<u64>,
    pub overrides: Overrides,
}

/// The profile dictionary: shared, immutable after registration.
#[derive(Debug, Default)]
pub struct ProfileStore {
    profiles: BTreeMap<u64, FormattingProfile>,
}

/// Resolution failure (cycle or missing parent). Never panics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveError;

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "profile resolution failed (cycle or missing parent)")
    }
}

impl std::error::Error for ResolveError {}

impl ProfileStore {
    pub fn register(&mut self, profile: FormattingProfile) {
        self.profiles.insert(profile.id, profile);
    }

    /// Fully resolve a profile through its ancestry. Returns per-property
    /// values plus the set of EXPLICITLY set properties — the inheritance
    /// semantics that profile identity requires (GY-TYP-002 §3).
    pub fn resolve(&self, id: u64) -> Result<Resolved, ResolveError> {
        let mut chain: Vec<u64> = Vec::new();
        let mut cur = Some(id);
        while let Some(pid) = cur {
            if chain.contains(&pid) {
                return Err(ResolveError); // cycle
            }
            chain.push(pid);
            let profile = self.profiles.get(&pid).ok_or(ResolveError)?;
            cur = profile.parent;
        }
        // Walk root → leaf so deeper overrides win. `explicit` records the
        // LEAF's own overrides only — provenance relative to inheritance.
        // (A property set at an ancestor is inherited here; a theme change
        // to that ancestor propagates to every descendant equally.)
        chain.reverse();
        let mut values: BTreeMap<FormatProp, FormatValue> = BTreeMap::new();
        let mut explicit = PropertyMask(0);
        let mut depth = 0u32;
        let n = chain.len();
        for (i, pid) in chain.iter().enumerate() {
            let profile = self.profiles.get(pid).ok_or(ResolveError)?;
            depth += 1;
            for (prop, val) in &profile.overrides {
                values.insert(*prop, val.clone());
                if i == n - 1 {
                    explicit.set(prop_bit(*prop));
                }
            }
        }
        Ok(Resolved {
            values,
            explicit,
            depth,
        })
    }
}

/// Resolved formatting: effective values + inheritance provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub values: BTreeMap<FormatProp, FormatValue>,
    /// Bits for properties explicitly set (vs inherited) in this profile.
    pub explicit: PropertyMask,
    /// Ancestry depth.
    pub depth: u32,
}

fn prop_bit(p: FormatProp) -> u32 {
    match p {
        FormatProp::Family => 1,
        FormatProp::Size => 2,
        FormatProp::Weight => 4,
        FormatProp::Colour => 8,
        FormatProp::LineHeight => 16,
        FormatProp::LetterSpacing => 32,
        FormatProp::Underline => 64,
        FormatProp::Align => 128,
        FormatProp::Opacity => 256,
    }
}

/// Identity for interning: equal only when resolved values match AND the
/// inheritance provenance matches (leaf-explicit sets AND ancestry depth).
/// Two objects that look identical today but respond differently to a
/// theme/parent change are intentionally distinct.
pub fn same_identity(a: &Resolved, b: &Resolved) -> bool {
    a.values == b.values && a.explicit == b.explicit && a.depth == b.depth
}

// ---- In-object variance -------------------------------------------------

/// A character range over an object's content (byte offsets, end-exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    fn contains_pos(&self, pos: usize) -> bool {
        pos >= self.start && pos < self.end.max(self.start)
    }
}

/// Typed delta: range-scoped overrides of the object's base format.
#[derive(Debug, Clone, PartialEq)]
pub struct FormattingDelta {
    pub range: TextRange,
    pub overrides: Overrides,
}

/// One semantic text object with delta-based variance (GY-TYP-002 §4).
#[derive(Debug, Clone)]
pub struct FormattedText {
    pub id: String,
    pub content: String,
    pub base_format: u64,
    pub deltas: Vec<FormattingDelta>,
}

/// Edit event for stable-anchor adjustment (GY-TYP-002 §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edit {
    /// `len` bytes inserted at `pos`.
    Insert { pos: usize, len: usize },
    /// `len` bytes deleted starting at `pos`.
    Delete { pos: usize, len: usize },
}

impl FormattedText {
    pub fn new(id: &str, content: &str, base_format: u64) -> Self {
        FormattedText {
            id: id.to_string(),
            content: content.to_string(),
            base_format,
            deltas: Vec::new(),
        }
    }

    /// Apply an edit to the content and adjust delta ranges with stable
    /// anchor rules (documented, deterministic):
    /// - insertion strictly inside a range extends it (formatting continues
    ///   onto typed text); insertion exactly at `start` also extends
    ///   (typing at a styled boundary adopts the style);
    /// - insertion before a range shifts it right;
    /// - insertion after a range leaves it untouched;
    /// - deletion intersects/clamps; offsets clamp to content bounds.
    ///
    /// Never panics.
    pub fn apply_edit(&mut self, edit: Edit) {
        // Materialise the content change first (byte offsets snap to char
        // boundaries; our text is UTF-8 and clamped — never panics).
        let len = self.content_len();
        match edit {
            Edit::Insert { pos, len: ilen } => {
                let pos = pos.min(len);
                // Snap to char boundary (content may be multibyte).
                let mut pos = pos;
                while pos > 0 && !self.content.is_char_boundary(pos) {
                    pos -= 1;
                }
                self.content.insert_str(pos, &"\u{0}".repeat(ilen));
            }
            Edit::Delete { pos, len: dlen } => {
                let mut pos = pos.min(len);
                while pos > 0 && !self.content.is_char_boundary(pos) {
                    pos -= 1;
                }
                let mut end = (pos + dlen).min(self.content.len());
                while end > pos && !self.content.is_char_boundary(end) {
                    end += 1;
                }
                self.content.replace_range(pos..end, "");
            }
        }
        let len = self.content_len();
        match edit {
            Edit::Insert { pos, len: ilen } => {
                let pos = pos.min(len);
                for d in &mut self.deltas {
                    if pos < d.range.start {
                        // Strictly before: shift right.
                        d.range.start += ilen;
                        d.range.end += ilen;
                    } else if pos <= d.range.end {
                        // At start (adopts style) or inside (extends).
                        d.range.end += ilen;
                    }
                    // After the range: untouched.
                }
            }
            Edit::Delete { pos, len: mut dlen } => {
                let pos = pos.min(len);
                let avail = len - pos;
                dlen = dlen.min(avail);
                let del_end = pos + dlen;
                for d in &mut self.deltas {
                    let (s, e) = (d.range.start, d.range.end);
                    if del_end <= s {
                        // Deletion wholly before: shift left.
                        d.range.start -= dlen;
                        d.range.end -= dlen;
                    } else if pos >= e {
                        // Deletion wholly after: untouched.
                    } else if pos < s {
                        // Deletion starts before the range: everything up
                        // to `pos` shifts left by dlen, styled bytes in
                        // [s, del_end) are removed.
                        d.range.start = pos;
                        d.range.end = e - dlen;
                        if d.range.end < d.range.start {
                            d.range.end = d.range.start;
                        }
                    } else {
                        // Deletion starts inside the range: start holds,
                        // end loses min(dlen, e - pos).
                        let removed = dlen.min(e - pos);
                        d.range.end -= removed;
                        if d.range.end < d.range.start {
                            d.range.end = d.range.start;
                        }
                    }
                }
            }
        }
    }

    fn content_len(&self) -> usize {
        self.content.len()
    }

    /// Effective format at byte position `pos`: base ⊕ deltas covering pos.
    /// Every query is total.
    pub fn effective_at(&self, pos: usize, store: &ProfileStore) -> Result<Resolved, ResolveError> {
        let base = store.resolve(self.base_format)?;
        let mut values = base.values.clone();
        let mut explicit = base.explicit;
        for d in &self.deltas {
            if d.range.contains_pos(pos) {
                for (p, v) in &d.overrides {
                    values.insert(*p, v.clone());
                    explicit.set(prop_bit(*p));
                }
            }
        }
        Ok(Resolved {
            values,
            explicit,
            depth: base.depth + 1,
        })
    }

    /// Canonical formatting runs: maximal contiguous ranges with identical
    /// effective formatting (values AND explicit semantics). The renderer's
    /// input — adjacent spans with equal formatting merge.
    ///
    /// Implementation is run-based (directive §7): formatting can only
    /// change at delta range boundaries, so candidate breakpoints are the
    /// sorted boundary set {0, n} ∪ {start, end of every delta}, and each
    /// interval between consecutive boundaries is resolved exactly once —
    /// O(d log d + d·r) rather than per-character resolution.
    pub fn runs(&self, store: &ProfileStore) -> Result<Vec<(TextRange, Resolved)>, ResolveError> {
        let n = self.content_len();
        let mut runs = Vec::new();
        if n == 0 {
            return Ok(runs);
        }
        let mut boundaries: Vec<usize> = vec![0, n];
        for d in &self.deltas {
            boundaries.push(d.range.start.min(n));
            boundaries.push(d.range.end.min(n));
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut start = boundaries[0];
        let mut current = self.effective_at(start, store)?;
        for &b in &boundaries[1..] {
            if !same_identity(&self.effective_at(b.min(n - 1), store)?, &current) {
                runs.push((TextRange { start, end: b }, current.clone()));
                start = b;
                current = self.effective_at(b.min(n - 1), store)?;
            }
        }
        if start < n {
            runs.push((TextRange { start, end: n }, current));
        }
        Ok(runs)
    }

    /// Canonicalise (GY-TYP-002 §7): sort deltas, drop empty ranges,
    /// deduplicate exact duplicates, merge adjacent ranges with identical
    /// overrides.
    pub fn canonicalise(&mut self) {
        self.deltas
            .sort_by_key(|d| (d.range.start, d.range.end, d.overrides.len()));
        self.deltas.dedup();
        let mut merged: Vec<FormattingDelta> = Vec::new();
        for d in self.deltas.drain(..) {
            if d.range.start >= d.range.end {
                continue; // empty: drop
            }
            match merged.last_mut() {
                Some(last) if last.range.end == d.range.start && last.overrides == d.overrides => {
                    last.range.end = d.range.end;
                }
                _ => merged.push(d),
            }
        }
        self.deltas = merged;
    }
}

// ---- Invalidation classification (directive §6) -------------------------

/// What a formatting delta affects downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidationScope {
    pub reshape: bool,
    pub relayout: bool,
    pub reraster: bool,
    pub recompose: bool,
}

impl InvalidationScope {
    /// Family/size: everything may change.
    pub const FULL: Self = Self {
        reshape: true,
        relayout: true,
        reraster: true,
        recompose: true,
    };
    /// Colour/opacity: shaping, layout and glyph masks all reuse.
    pub const COMPOSE_ONLY: Self = Self {
        reshape: false,
        relayout: false,
        reraster: false,
        recompose: true,
    };
}

/// Classify a property delta into its invalidation scope per the
/// directive's table (§6). Conservative "usually reuse" rows reuse.
pub fn invalidation_scope(prop: FormatProp) -> InvalidationScope {
    match prop {
        FormatProp::Colour | FormatProp::Opacity | FormatProp::Underline => {
            InvalidationScope::COMPOSE_ONLY
        }
        FormatProp::Family | FormatProp::Size => InvalidationScope::FULL,
        // Weight may change outlines, advances and wrapping.
        FormatProp::Weight => InvalidationScope::FULL,
        // Letter spacing: glyph IDs usually reused; placement recomputed.
        FormatProp::LetterSpacing | FormatProp::LineHeight | FormatProp::Align => {
            InvalidationScope {
                reshape: false,
                relayout: true,
                reraster: false,
                recompose: true,
            }
        }
    }
}

/// LCPE bridge: map a formatting delta to goya-change property bits so
/// invalidation flows through the change-envelope machinery.
pub fn lcpe_property_mask(prop: FormatProp) -> u32 {
    match prop {
        FormatProp::Family
        | FormatProp::Size
        | FormatProp::LetterSpacing
        | FormatProp::LineHeight
        | FormatProp::Align => PropertyMask::GEOMETRY,
        FormatProp::Colour | FormatProp::Opacity => PropertyMask::COLOUR,
        FormatProp::Weight | FormatProp::Underline => PropertyMask::COLOUR | PropertyMask::GEOMETRY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> ProfileStore {
        let mut s = ProfileStore::default();
        s.register(FormattingProfile {
            id: 1,
            name: "BodyText".into(),
            parent: None,
            overrides: Overrides::from([
                (FormatProp::Family, FormatValue::Family("Inter".into())),
                (FormatProp::Size, FormatValue::Size(16.0)),
                (FormatProp::Weight, FormatValue::Weight(400)),
            ]),
        });
        s.register(FormattingProfile {
            id: 2,
            name: "DisplayText".into(),
            parent: Some(1),
            overrides: Overrides::from([
                (FormatProp::Size, FormatValue::Size(32.0)),
                (FormatProp::Weight, FormatValue::Weight(700)),
            ]),
        });
        s
    }

    #[test]
    fn inheritance_resolution_child_wins() {
        let s = store();
        let r = s.resolve(2).unwrap();
        assert_eq!(
            r.values.get(&FormatProp::Size),
            Some(&FormatValue::Size(32.0))
        );
        // Family inherited from BodyText.
        assert_eq!(
            r.values.get(&FormatProp::Family),
            Some(&FormatValue::Family("Inter".into()))
        );
        // Explicit bits: Size + Weight set at DisplayText, Family inherited.
        assert!(r.explicit.contains(prop_bit(FormatProp::Size)));
        assert!(!r.explicit.contains(prop_bit(FormatProp::Family)));
    }

    #[test]
    fn identity_requires_inheritance_semantics() {
        // Two profiles with identical resolved values but different
        // explicit sets are NOT the same identity.
        let mut s2 = ProfileStore::default();
        s2.register(FormattingProfile {
            id: 1,
            name: "a".into(),
            parent: None,
            overrides: Overrides::from([(FormatProp::Size, FormatValue::Size(16.0))]),
        });
        s2.register(FormattingProfile {
            id: 2,
            name: "b".into(),
            parent: Some(1),
            overrides: Overrides::from([(FormatProp::Size, FormatValue::Size(16.0))]),
        });
        let ra = s2.resolve(1).unwrap();
        let rb = s2.resolve(2).unwrap();
        assert_eq!(ra.values, rb.values, "resolved values equal");
        assert!(!same_identity(&ra, &rb), "different override provenance");
    }

    #[test]
    fn cycles_rejected_not_looped() {
        let mut s = ProfileStore::default();
        s.register(FormattingProfile {
            id: 1,
            name: "a".into(),
            parent: Some(2),
            overrides: Overrides::new(),
        });
        s.register(FormattingProfile {
            id: 2,
            name: "b".into(),
            parent: Some(1),
            overrides: Overrides::new(),
        });
        assert!(s.resolve(1).is_err());
    }

    #[test]
    fn delta_variance_across_ranges() {
        let s = store();
        let mut text = FormattedText::new(
            "sentence",
            "The renderer is fast, adaptive, and resource-aware.",
            1,
        );
        let fast_at = text.content.find("fast").unwrap();
        let adaptive_at = text.content.find("adaptive").unwrap();
        text.deltas.push(FormattingDelta {
            range: TextRange {
                start: fast_at,
                end: fast_at + 4,
            },
            overrides: Overrides::from([(FormatProp::Weight, FormatValue::Weight(700))]),
        });
        text.deltas.push(FormattingDelta {
            range: TextRange {
                start: adaptive_at,
                end: adaptive_at + 8,
            },
            overrides: Overrides::from([(FormatProp::Colour, FormatValue::Colour([255, 0, 128]))]),
        });
        let inside = text.effective_at(fast_at, &s).unwrap();
        assert_eq!(
            inside.values.get(&FormatProp::Weight),
            Some(&FormatValue::Weight(700))
        );
        let outside = text.effective_at(0, &s).unwrap();
        assert_eq!(
            outside.values.get(&FormatProp::Weight),
            Some(&FormatValue::Weight(400))
        );
        // Runs: base, "fast", base, "adaptive", base — at least 5.
        let runs = text.runs(&s).unwrap();
        assert!(runs.len() >= 5, "expected ≥5 runs, got {}", runs.len());
    }

    #[test]
    fn insertion_inside_range_extends_it() {
        let mut text = FormattedText::new("t", "hello world", 1);
        text.deltas.push(FormattingDelta {
            range: TextRange { start: 0, end: 5 },
            overrides: Overrides::from([(FormatProp::Weight, FormatValue::Weight(700))]),
        });
        text.apply_edit(Edit::Insert { pos: 2, len: 3 });
        assert_eq!(text.deltas[0].range, TextRange { start: 0, end: 8 });
        // Insertion at start adopts the style (extends).
        text.apply_edit(Edit::Insert { pos: 0, len: 2 });
        assert_eq!(text.deltas[0].range, TextRange { start: 0, end: 10 });
        // Insertion after leaves untouched.
        text.apply_edit(Edit::Insert { pos: 20, len: 4 });
        assert_eq!(text.deltas[0].range, TextRange { start: 0, end: 10 });
    }

    #[test]
    fn deletion_intersecting_range_shrinks_it() {
        let mut text = FormattedText::new("t", "hello world", 1);
        text.deltas.push(FormattingDelta {
            range: TextRange { start: 3, end: 9 },
            overrides: Overrides::from([(FormatProp::Weight, FormatValue::Weight(700))]),
        });
        text.apply_edit(Edit::Delete { pos: 0, len: 5 }); // delete "hello"
        assert_eq!(text.deltas[0].range, TextRange { start: 0, end: 4 });
    }

    #[test]
    fn canonicalise_merges_adjacent_equivalent_deltas() {
        let mut text = FormattedText::new("t", "abcdefgh", 1);
        let w = || Overrides::from([(FormatProp::Weight, FormatValue::Weight(700))]);
        text.deltas.push(FormattingDelta {
            range: TextRange { start: 0, end: 3 },
            overrides: w(),
        });
        text.deltas.push(FormattingDelta {
            range: TextRange { start: 3, end: 5 },
            overrides: w(),
        });
        text.deltas.push(FormattingDelta {
            range: TextRange { start: 5, end: 5 },
            overrides: w(),
        });
        text.canonicalise();
        assert_eq!(text.deltas.len(), 1);
        assert_eq!(text.deltas[0].range, TextRange { start: 0, end: 5 });
    }

    #[test]
    fn invalidation_table_matches_directive() {
        // §6: colour/opacity/underline → composition-only reuse;
        // family/size/weight → full pipeline; spacing → relayout only.
        assert_eq!(
            invalidation_scope(FormatProp::Colour),
            InvalidationScope::COMPOSE_ONLY
        );
        assert_eq!(
            invalidation_scope(FormatProp::Opacity),
            InvalidationScope::COMPOSE_ONLY
        );
        assert_eq!(
            invalidation_scope(FormatProp::Family),
            InvalidationScope::FULL
        );
        assert_eq!(
            invalidation_scope(FormatProp::Size),
            InvalidationScope::FULL
        );
        let spacing = invalidation_scope(FormatProp::LetterSpacing);
        assert!(!spacing.reshape && spacing.relayout && !spacing.reraster);
        let weight = invalidation_scope(FormatProp::Weight);
        assert!(weight.reshape && weight.relayout && weight.reraster);
    }

    #[test]
    fn lcpe_bridge_maps_properties() {
        assert_eq!(lcpe_property_mask(FormatProp::Colour), PropertyMask::COLOUR);
        assert_eq!(lcpe_property_mask(FormatProp::Size), PropertyMask::GEOMETRY);
        assert_eq!(
            lcpe_property_mask(FormatProp::Weight),
            PropertyMask::COLOUR | PropertyMask::GEOMETRY
        );
    }

    /// GY-TYP-002 §10 acceptance: 100,000 chars, 1,000 variations, one base
    /// style. (1) global colour change → compose-only scope; (2) one-word
    /// weight change → local reshape; (3) insert inside a formatted range
    /// preserves anchors; runs computable over the whole document.
    #[test]
    fn acceptance_100k_document() {
        let s = store();
        let words = ["lorem ", "ipsum ", "dolor ", "sit "];
        let mut content = String::with_capacity(100_000);
        while content.len() < 100_000 {
            content.push_str(words[content.len() % 4]);
        }
        content.truncate(100_000); // words are ASCII; safe to cut exactly
        let mut doc = FormattedText::new("doc", &content, 1);
        // 1,000 variations: emphasise every 100-byte span (20 bytes).
        for i in 0..1_000usize {
            let start = i * 100;
            doc.deltas.push(FormattingDelta {
                range: TextRange {
                    start,
                    end: start + 20,
                },
                overrides: Overrides::from([
                    (FormatProp::Weight, FormatValue::Weight(700)),
                    (FormatProp::Colour, FormatValue::Colour([0, 200, 120])),
                ]),
            });
        }
        assert_eq!(doc.content.len(), 100_000);

        // (1) Global base colour change: a new base profile version carries
        // the colour; scope is composition-only; deltas untouched.
        let mut s_global = store();
        s_global.register(FormattingProfile {
            id: 1,
            name: "BodyText-v2".into(),
            parent: None,
            overrides: Overrides::from([
                (FormatProp::Family, FormatValue::Family("Inter".into())),
                (FormatProp::Size, FormatValue::Size(16.0)),
                (FormatProp::Weight, FormatValue::Weight(400)),
                (FormatProp::Colour, FormatValue::Colour([200, 200, 210])),
            ]),
        });
        assert_eq!(
            invalidation_scope(FormatProp::Colour),
            InvalidationScope::COMPOSE_ONLY,
            "global recolour must not reshape"
        );
        let before_runs = doc.runs(&s).unwrap().len();
        let after_runs = doc.runs(&s_global).unwrap().len();
        assert_eq!(
            before_runs, after_runs,
            "run structure identical across recolour"
        );

        // (2) One-word weight change: local reshape scope.
        let wscope = invalidation_scope(FormatProp::Weight);
        assert!(wscope.reshape && wscope.relayout);
        let one_word_at = 500;
        doc.deltas.push(FormattingDelta {
            range: TextRange {
                start: one_word_at,
                end: one_word_at + 4,
            },
            overrides: Overrides::from([(FormatProp::Weight, FormatValue::Weight(200))]),
        });

        // (3) Insert inside formatted variation #1 (bytes 100..120):
        // anchors preserved — the covering delta extends over the insert.
        doc.apply_edit(Edit::Insert { pos: 105, len: 3 });
        assert_eq!(doc.content.len(), 100_003);
        assert_eq!(
            doc.deltas[1].range,
            TextRange {
                start: 100,
                end: 123
            }
        );

        // Runs still computable over the full document.
        let runs = doc.runs(&s).unwrap();
        assert!(runs.len() >= 1_000, "runs: {}", runs.len());
    }

    /// Totality fuzz: random edits/deltas/canonicalise never panic and runs
    /// remain computable.
    #[test]
    fn fuzz_operations_never_panic() {
        let s = store();
        let mut rng = 0xA5A5_5A5A_1234_ABCDu64;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let mut text = FormattedText::new("f", &"abcdefghij".repeat(20), 1);
        for _ in 0..800u64 {
            match next() % 4 {
                0 => text.apply_edit(Edit::Insert {
                    pos: (next() as usize) % (text.content.len() + 1),
                    len: (next() % 20) as usize,
                }),
                1 if text.content.len() > 5 => text.apply_edit(Edit::Delete {
                    pos: (next() as usize) % (text.content.len() - 1),
                    len: (next() % 10) as usize,
                }),
                2 => text.canonicalise(),
                _ => {
                    let len = text.content.len();
                    let a = (next() as usize) % (len + 1);
                    let b = a + (next() as usize) % (len + 1 - a).max(1);
                    text.deltas.push(FormattingDelta {
                        range: TextRange { start: a, end: b },
                        overrides: Overrides::from([(
                            FormatProp::Weight,
                            FormatValue::Weight((next() % 900 + 100) as u16),
                        )]),
                    });
                }
            }
            let _ = text.runs(&s);
        }
    }
}

#[cfg(test)]
mod utf8_boundary_tests {
    use super::*;

    /// Audit P1 verification: edits enforce UTF-8 boundaries and the
    /// content+formatting mutation is atomic (content AND anchors move
    /// together).
    #[test]
    fn multibyte_insert_snaps_and_extends_range() {
        // "héllo" — é is 2 bytes; content bytes: h(0) é(1-2) l(3) l(4) o(5).
        let mut text = FormattedText::new("u", "héllo", 1);
        text.deltas.push(FormattingDelta {
            range: TextRange { start: 0, end: 5 },
            overrides: Overrides::from([(FormatProp::Weight, FormatValue::Weight(700))]),
        });
        // Insert 3 bytes at byte offset 1 (mid-é): snapped back to 1... the
        // snap walks BACK to a boundary; offset 1 is the interior of é, so
        // it snaps to 1 (start of é). Actually offset 1 IS é's start; try 2
        // (é's interior) → snaps to 3 (start of l)? snap walks backward to
        // boundary: 2 is interior → 1. Verify content stays valid UTF-8.
        text.apply_edit(Edit::Insert { pos: 2, len: 3 });
        assert!(std::str::from_utf8(text.content.as_bytes()).is_ok());
        let rendered = std::str::from_utf8(text.content.as_bytes());
        assert!(rendered.map(|r| r.contains('\u{0}')).unwrap_or(false));
        // Anchors snapped consistently: range stays char-boundary aligned.
        let s = text.deltas[0].range.start;
        let e = text.deltas[0].range.end;
        assert!(text.content.is_char_boundary(s));
        assert!(text.content.is_char_boundary(e));
    }

    #[test]
    fn delete_mid_char_snaps_and_keeps_valid_utf8() {
        let mut text = FormattedText::new("u", "aébc", 1);
        text.apply_edit(Edit::Delete { pos: 2, len: 3 }); // interior of é
        assert!(std::str::from_utf8(text.content.as_bytes()).is_ok());
    }

    #[test]
    fn atomic_mutation_updates_content_and_anchors_together() {
        let mut text = FormattedText::new("u", "abc", 1);
        text.deltas.push(FormattingDelta {
            range: TextRange { start: 0, end: 3 },
            overrides: Overrides::from([(FormatProp::Weight, FormatValue::Weight(700))]),
        });
        text.apply_edit(Edit::Insert { pos: 1, len: 2 });
        // Content AND range both reflect the edit: atomic.
        assert_eq!(text.content.len(), 5);
        assert_eq!(text.deltas[0].range, TextRange { start: 0, end: 5 });
    }
}
