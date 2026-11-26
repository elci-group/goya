//! Explicit ROM asset extraction. Offsets are supplied by a caller or trace.

use crate::graphics::{
    AssetView, IndexedTile, PaletteEntry, Space, TileDepth, TileGva, TileGvaError, Transparency,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ExtractedTile {
    pub tile: IndexedTile,
    /// Complete selected bank, preserving unused colours and original values.
    pub palette: Vec<PaletteEntry>,
    pub rgba: Vec<[u8; 4]>,
    pub gva_candidate: TileGva,
    /// Size of the tile plus the complete supplied palette bank.
    pub indexed_bytes: usize,
    /// Size comparison only, not a performance or fidelity certification.
    pub gva_smaller: bool,
    pub production_verified: bool,
}

pub fn extract_tile(
    rom: &[u8],
    tile_offset: usize,
    palette_offset: usize,
    depth: TileDepth,
    transparency: Transparency,
) -> Result<ExtractedTile, TileGvaError> {
    let view = AssetView::new(rom, Space::Rom).map_err(TileGvaError::Graphics)?;
    let tile = view
        .tile(tile_offset, depth)
        .map_err(TileGvaError::Graphics)?;
    let count = match depth {
        TileDepth::FourBpp => 16,
        TileDepth::EightBpp => 256,
    };
    let mut palette = Vec::with_capacity(count);
    for i in 0..count {
        let offset = palette_offset
            .checked_add(i * 2)
            .ok_or(TileGvaError::Graphics(
                crate::graphics::Error::OutOfBounds {
                    offset: palette_offset,
                    length: count * 2,
                },
            ))?;
        palette.push(view.palette_entry(offset).map_err(TileGvaError::Graphics)?);
    }
    let mut rgba =
        crate::graphics::resolve_tile_rgba(&tile, &palette).map_err(TileGvaError::Graphics)?;
    if transparency == Transparency::IndexZero {
        for (index, pixel) in rgba.iter_mut().enumerate() {
            if tile.pixels[index] == 0 {
                *pixel = [0; 4];
            }
        }
    }
    let gva_candidate = crate::graphics::tile_to_gva(&tile, &palette, transparency)?;
    let indexed_bytes = depth.bytes() + count * 2;
    let gva_smaller = gva_candidate.encoded_bytes < indexed_bytes;
    Ok(ExtractedTile {
        tile,
        palette,
        rgba,
        gva_candidate,
        indexed_bytes,
        gva_smaller,
        production_verified: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_retains_bank_and_rejects_incomplete_palette() {
        let mut rom = vec![0u8; 64];
        rom[0] = 0x10;
        rom[34] = 31;
        let asset = extract_tile(&rom, 0, 32, TileDepth::FourBpp, Transparency::IndexZero).unwrap();
        assert_eq!(asset.rgba[0], [0; 4]);
        assert_eq!(asset.rgba[1], [255, 0, 0, 255]);
        assert_eq!(asset.palette.len(), 16);
        assert_eq!(asset.indexed_bytes, 64);
        assert_eq!(
            asset.tile.provenance.source_sha256,
            asset.palette[15].provenance.source_sha256
        );
        assert!(!asset.production_verified);
        assert!(extract_tile(&rom[..63], 0, 32, TileDepth::FourBpp, Transparency::Opaque).is_err());
        assert!(extract_tile(
            &rom,
            0,
            usize::MAX,
            TileDepth::FourBpp,
            Transparency::Opaque
        )
        .is_err());
    }
}
