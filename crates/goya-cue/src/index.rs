//! Indexed colour held until a fill.
//!
//! GVI-L already stores the shared palette. This module does not rewrite it.
//! Expansion to RGBA8 happens in [`resolve_at_fill`], once per shape, not once
//! per stored style record.

/// Palette index meaning the colour is absent. A present colour never uses it.
pub const ABSENT: u16 = 0xFFFF;

/// A colour kept until paint: the bytes themselves, or a slot in a [`Palette`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromaticRef {
    Direct([u8; 4]),
    Index(u16),
}

/// Shared colours, strictly ascending and unique.
///
/// Order is unsigned byte order: red, then green, then blue, then alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    colours: Vec<[u8; 4]>,
}

impl Palette {
    /// Dedup and sort. Order of first appearance is not preserved.
    pub fn from_colours(colours: &[[u8; 4]]) -> Self {
        let mut colours = colours.to_vec();
        colours.sort_unstable();
        colours.dedup();
        Self { colours }
    }

    pub fn len(&self) -> usize {
        self.colours.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colours.is_empty()
    }

    pub fn as_slice(&self) -> &[[u8; 4]] {
        &self.colours
    }

    /// Same result as [`resolve_at_fill`]. Callers expand at fill through that function.
    pub fn resolve(&self, reference: ChromaticRef) -> Option<[u8; 4]> {
        resolve_at_fill(self, reference)
    }
}

/// The paint-time expansion. Direct returns the colour and ignores the palette.
/// Index([`ABSENT`]) and an out-of-range index return None.
/// A valid index returns that palette entry.
pub fn resolve_at_fill(palette: &Palette, reference: ChromaticRef) -> Option<[u8; 4]> {
    match reference {
        ChromaticRef::Direct(colour) => Some(colour),
        // Absence is not a slot, even when the table is long enough to hold one.
        ChromaticRef::Index(ABSENT) => None,
        ChromaticRef::Index(index) => palette.colours.get(usize::from(index)).copied(),
    }
}

/// Pack a resolved colour as the Wayland fill word `0x00RRGGBB`.
///
/// The fill slice does not carry alpha. Absence stays a separate `None`.
pub fn fill_word(colour: [u8; 4]) -> u32 {
    (u32::from(colour[0]) << 16) | (u32::from(colour[1]) << 8) | u32::from(colour[2])
}

/// Expand one reference per primitive, once, into the fill slice the rasteriser
/// already consumes. The index does not survive past this call.
pub fn fills_at_paint(palette: &Palette, refs: &[ChromaticRef]) -> Vec<Option<u32>> {
    refs.iter()
        .map(|reference| resolve_at_fill(palette, *reference).map(fill_word))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_agree(palette: &Palette, reference: ChromaticRef, expected: Option<[u8; 4]>) {
        assert_eq!(resolve_at_fill(palette, reference), expected);
        assert_eq!(palette.resolve(reference), expected);
    }

    #[test]
    fn from_colours_sorts_and_dedups_red_green_blue_alpha() {
        let input = [
            [2, 0, 0, 0],
            [1, 9, 0, 0],
            [1, 8, 9, 0],
            [1, 8, 8, 2],
            [1, 8, 8, 1],
            [1, 8, 8, 1],
            [2, 0, 0, 0],
        ];
        let palette = Palette::from_colours(&input);
        assert!(palette.len() < input.len());
        assert!(!palette.is_empty());
        assert_eq!(
            palette.as_slice(),
            &[
                [1, 8, 8, 1],
                [1, 8, 8, 2],
                [1, 8, 9, 0],
                [1, 9, 0, 0],
                [2, 0, 0, 0],
            ]
        );
        assert!(palette.as_slice().windows(2).all(|w| w[0] < w[1]));
        assert_agree(&palette, ChromaticRef::Index(0), Some([1, 8, 8, 1]));
        assert_agree(&palette, ChromaticRef::Index(4), Some([2, 0, 0, 0]));
    }

    #[test]
    fn index_refs_of_the_same_colour_share_one_slot() {
        let shared = [9, 1, 2, 255];
        let input = [shared, [1, 2, 3, 4], shared, [0, 0, 0, 0], [1, 2, 3, 4]];
        let palette = Palette::from_colours(&input);
        assert!(palette.len() < input.len());
        assert_eq!(palette.len(), 3);
        assert_eq!(
            palette.as_slice().iter().filter(|c| **c == shared).count(),
            1
        );
        let slot = palette
            .as_slice()
            .iter()
            .position(|c| *c == shared)
            .unwrap() as u16;
        let first = ChromaticRef::Index(slot);
        let second = ChromaticRef::Index(slot);
        assert_agree(&palette, first, Some(shared));
        assert_agree(&palette, second, Some(shared));
        assert_eq!(resolve_at_fill(&palette, first), palette.resolve(second));
    }

    #[test]
    fn absent_and_out_of_range_index_resolve_to_none() {
        let palette = Palette::from_colours(&[[4, 0, 0, 255], [1, 0, 0, 255]]);
        assert!(palette.len() < 99);
        assert_eq!(palette.as_slice(), &[[1, 0, 0, 255], [4, 0, 0, 255]]);
        assert_agree(&palette, ChromaticRef::Index(ABSENT), None);
        assert_agree(&palette, ChromaticRef::Index(99), None);
        assert_agree(&palette, ChromaticRef::Index(2), None);
        assert_agree(&Palette::from_colours(&[]), ChromaticRef::Index(0), None);
    }

    #[test]
    fn direct_resolves_when_absent_from_the_palette() {
        let palette = Palette::from_colours(&[[9, 9, 9, 9], [8, 8, 8, 8]]);
        let colour = [1, 2, 3, 4];
        assert!(palette.as_slice().iter().all(|c| *c != colour));
        assert_agree(&palette, ChromaticRef::Direct(colour), Some(colour));
        assert_agree(
            &Palette::from_colours(&[]),
            ChromaticRef::Direct(colour),
            Some(colour),
        );
    }

    #[test]
    fn fills_expand_once_per_primitive_and_match_direct_words() {
        let rgba = [0x1A, 0x2A, 0x3B, 0xFF];
        let palette = Palette::from_colours(&[rgba, [9, 9, 9, 255]]);
        let slot = palette.as_slice().iter().position(|c| *c == rgba).unwrap() as u16;
        let refs = [
            ChromaticRef::Index(slot),
            ChromaticRef::Direct(rgba),
            ChromaticRef::Index(ABSENT),
            ChromaticRef::Index(99),
        ];
        let fills = fills_at_paint(&palette, &refs);
        assert_eq!(fills, vec![Some(0x1A2A3B), Some(0x1A2A3B), None, None]);
        assert_eq!(fill_word(rgba), 0x1A2A3B);
    }
}
