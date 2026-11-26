//! Exact indexed graphics decoding from explicitly supplied ROM or memory bytes.
//!
//! This layer does not locate resources, apply display register state, assign
//! transparency, or convert colours to host pixels. Those require a trace.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Space {
    Rom,
    Vram,
    PaletteRam,
}

impl Space {
    fn base(self) -> u32 {
        match self {
            Self::Rom => 0x0800_0000,
            Self::Vram => 0x0600_0000,
            Self::PaletteRam => 0x0500_0000,
        }
    }

    fn maximum(self) -> usize {
        match self {
            Self::Rom => crate::MAX_ROM_BYTES,
            Self::Vram => 96 * 1024,
            Self::PaletteRam => 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TileDepth {
    FourBpp,
    EightBpp,
}

impl TileDepth {
    pub fn bytes(self) -> usize {
        match self {
            Self::FourBpp => 32,
            Self::EightBpp => 64,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Empty,
    TooLarge { maximum: usize, actual: usize },
    OutOfBounds { offset: usize, length: usize },
    WrongSpace,
    InvalidTile,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "graphics source is empty"),
            Self::TooLarge { maximum, actual } => {
                write!(f, "graphics source is {actual} bytes; maximum is {maximum}")
            }
            Self::OutOfBounds { offset, length } => write!(
                f,
                "graphics range {offset}..{} is outside the source",
                offset.saturating_add(*length)
            ),
            Self::WrongSpace => write!(f, "operation is not valid for this memory space"),
            Self::InvalidTile => {
                write!(f, "tile must contain 64 indices within its declared depth")
            }
        }
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Provenance {
    /// SHA-256 of the entire supplied ROM or memory snapshot.
    pub source_sha256: String,
    pub space: Space,
    pub byte_range: [usize; 2],
    pub address_range: [u32; 2],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IndexedTile {
    pub depth: TileDepth,
    /// Row-major 8×8 palette indices. No transparency is assumed.
    pub pixels: Vec<u8>,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PaletteEntry {
    /// Original 16-bit value; bit 15 is retained without interpretation.
    pub raw: u16,
    pub red5: u8,
    pub green5: u8,
    pub blue5: u8,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TextMapEntry {
    pub tile_number: u16,
    pub horizontal_flip: bool,
    pub vertical_flip: bool,
    pub palette_bank: u8,
    pub provenance: Provenance,
}

/// Expand a five-bit channel to eight bits using bit replication, as commonly
/// used for displaying GBA colour values. The original `PaletteEntry::raw`
/// remains available for exact hardware comparisons.
pub const fn expand5(channel: u8) -> u8 {
    ((channel & 0x1f) << 3) | ((channel & 0x1f) >> 2)
}

/// Resolve one indexed tile through a supplied palette. Index zero is kept
/// opaque because transparency depends on the consuming display mode/layer.
pub fn resolve_tile_rgba(
    tile: &IndexedTile,
    palette: &[PaletteEntry],
) -> Result<Vec<[u8; 4]>, Error> {
    if tile.pixels.len() != 64
        || (tile.depth == TileDepth::FourBpp && tile.pixels.iter().any(|p| *p > 15))
    {
        return Err(Error::InvalidTile);
    }
    let mut rgba = Vec::with_capacity(tile.pixels.len());
    for &index in &tile.pixels {
        let color = palette.get(index as usize).ok_or(Error::OutOfBounds {
            offset: index as usize,
            length: 1,
        })?;
        rgba.push([
            expand5(color.red5),
            expand5(color.green5),
            expand5(color.blue5),
            255,
        ]);
    }
    Ok(rgba)
}

/// Explicit presentation policy; index-zero handling must come from the layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Transparency {
    Opaque,
    IndexZero,
}

/// Candidate GVA bytes and the evidence needed to evaluate the conversion.
/// This candidate is never automatically selected over the indexed source.
#[derive(Debug, Serialize)]
pub struct TileGva {
    pub bytes: Vec<u8>,
    pub tile_source: Provenance,
    pub palette_sources: Vec<Provenance>,
    pub transparency: Transparency,
    pub indexed_source_bytes: usize,
    pub encoded_bytes: usize,
}

#[derive(Debug)]
pub enum TileGvaError {
    Graphics(Error),
    Encoding(goya_graphics_ir::GfxDiag),
}

impl std::fmt::Display for TileGvaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Graphics(e) => e.fmt(f),
            Self::Encoding(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for TileGvaError {}

/// Encode maximal horizontal runs of equal resolved colour as GVA rectangles.
/// The 8×8 logical canvas uses 1000 millipoints per source pixel. This preserves
/// pixel boundaries as geometry; destination rasterisation still needs fidelity
/// verification. Palette is the selected 16-colour bank or full 256-colour table.
pub fn tile_to_gva(
    tile: &IndexedTile,
    palette: &[PaletteEntry],
    transparency: Transparency,
) -> Result<TileGva, TileGvaError> {
    use goya_graphics_ir::{
        Affine, Drawing, Fill, FillRule, Item, Path, Point, Rgba8, Segment, Style,
    };
    use std::sync::Arc;
    let rgba = resolve_tile_rgba(tile, palette).map_err(TileGvaError::Graphics)?;
    let mut items = Vec::new();
    let mut palette_sources = Vec::new();
    for y in 0..8 {
        let mut x = 0;
        while x < 8 {
            let i = y * 8 + x;
            if transparency == Transparency::IndexZero && tile.pixels[i] == 0 {
                x += 1;
                continue;
            }
            let colour = rgba[i];
            let start = x;
            while x < 8
                && rgba[y * 8 + x] == colour
                && !(transparency == Transparency::IndexZero && tile.pixels[y * 8 + x] == 0)
            {
                let provenance = &palette[tile.pixels[y * 8 + x] as usize].provenance;
                if !palette_sources.contains(provenance) {
                    palette_sources.push(provenance.clone());
                }
                x += 1;
            }
            let (left, right, top) = (start as i32 * 1000, x as i32 * 1000, y as i32 * 1000);
            let path = Path::new(vec![
                Segment::MoveTo(Point::new(left, top)),
                Segment::LineTo(Point::new(right, top)),
                Segment::LineTo(Point::new(right, top + 1000)),
                Segment::LineTo(Point::new(left, top + 1000)),
                Segment::Close,
            ])
            .map_err(TileGvaError::Encoding)?;
            items.push(Item {
                path: Arc::new(path),
                style: Style {
                    fill: Some(Fill {
                        colour: Rgba8::new(colour[0], colour[1], colour[2], colour[3]),
                        rule: FillRule::NonZero,
                    }),
                    stroke: None,
                },
                transform: Affine::IDENTITY,
                opacity: 255,
            });
        }
    }
    let bytes = goya_gva::encode(
        &Drawing {
            width: 8000,
            height: 8000,
            items,
        },
        &goya_gva::GvaLimits::default(),
    )
    .map_err(TileGvaError::Encoding)?;
    Ok(TileGva {
        encoded_bytes: bytes.len(),
        indexed_source_bytes: tile.depth.bytes() + palette_sources.len() * 2,
        bytes,
        tile_source: tile.provenance.clone(),
        palette_sources,
        transparency,
    })
}

/// A bounded source whose digest and address space travel with every result.
/// A VRAM or palette view represents one immutable captured snapshot.
pub struct AssetView<'a> {
    bytes: &'a [u8],
    space: Space,
    source_sha256: String,
}

impl<'a> AssetView<'a> {
    pub fn new(bytes: &'a [u8], space: Space) -> Result<Self, Error> {
        if bytes.is_empty() {
            return Err(Error::Empty);
        }
        if bytes.len() > space.maximum() {
            return Err(Error::TooLarge {
                maximum: space.maximum(),
                actual: bytes.len(),
            });
        }
        let mut source_sha256 = String::with_capacity(64);
        for byte in Sha256::digest(bytes) {
            write!(&mut source_sha256, "{byte:02x}").expect("String write");
        }
        Ok(Self {
            bytes,
            space,
            source_sha256,
        })
    }

    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    fn read(&self, offset: usize, length: usize) -> Result<(&'a [u8], Provenance), Error> {
        let end = offset
            .checked_add(length)
            .ok_or(Error::OutOfBounds { offset, length })?;
        let slice = self
            .bytes
            .get(offset..end)
            .ok_or(Error::OutOfBounds { offset, length })?;
        let start_address = self
            .space
            .base()
            .checked_add(offset as u32)
            .ok_or(Error::OutOfBounds { offset, length })?;
        let end_address = self
            .space
            .base()
            .checked_add(end as u32)
            .ok_or(Error::OutOfBounds { offset, length })?;
        Ok((
            slice,
            Provenance {
                source_sha256: self.source_sha256.clone(),
                space: self.space,
                byte_range: [offset, end],
                address_range: [start_address, end_address],
            },
        ))
    }

    pub fn tile(&self, offset: usize, depth: TileDepth) -> Result<IndexedTile, Error> {
        if self.space == Space::PaletteRam {
            return Err(Error::WrongSpace);
        }
        let (data, provenance) = self.read(offset, depth.bytes())?;
        let pixels = match depth {
            TileDepth::FourBpp => data.iter().flat_map(|b| [b & 0xf, b >> 4]).collect(),
            TileDepth::EightBpp => data.to_vec(),
        };
        Ok(IndexedTile {
            depth,
            pixels,
            provenance,
        })
    }

    pub fn palette_entry(&self, offset: usize) -> Result<PaletteEntry, Error> {
        if self.space == Space::Vram {
            return Err(Error::WrongSpace);
        }
        let (data, provenance) = self.read(offset, 2)?;
        let raw = u16::from_le_bytes([data[0], data[1]]);
        Ok(PaletteEntry {
            raw,
            red5: (raw & 0x1f) as u8,
            green5: ((raw >> 5) & 0x1f) as u8,
            blue5: ((raw >> 10) & 0x1f) as u8,
            provenance,
        })
    }

    /// Decode one 16-bit text-background map entry. The palette bank field
    /// remains raw even for 8bpp backgrounds, where the display ignores it.
    pub fn text_map_entry(&self, offset: usize) -> Result<TextMapEntry, Error> {
        if self.space == Space::PaletteRam {
            return Err(Error::WrongSpace);
        }
        let (data, provenance) = self.read(offset, 2)?;
        let raw = u16::from_le_bytes([data[0], data[1]]);
        Ok(TextMapEntry {
            tile_number: raw & 0x03ff,
            horizontal_flip: raw & 0x0400 != 0,
            vertical_flip: raw & 0x0800 != 0,
            palette_bank: (raw >> 12) as u8,
            provenance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gva_roundtrip_preserves_resolved_pixels_and_transparency() {
        let mut source = [0u8; 32];
        source[0] = 0x10;
        source[31] = 0x11;
        let tile = AssetView::new(&source, Space::Rom)
            .unwrap()
            .tile(0, TileDepth::FourBpp)
            .unwrap();
        let palette_bytes = [0, 0, 0x1f, 0];
        let view = AssetView::new(&palette_bytes, Space::PaletteRam).unwrap();
        let palette = vec![
            view.palette_entry(0).unwrap(),
            view.palette_entry(2).unwrap(),
        ];
        for transparency in [Transparency::Opaque, Transparency::IndexZero] {
            let candidate = tile_to_gva(&tile, &palette, transparency).unwrap();
            assert_eq!(
                candidate.bytes,
                tile_to_gva(&tile, &palette, transparency).unwrap().bytes
            );
            let limits = goya_gva::GvaLimits::default();
            let opened = goya_gva::Gva::open(&candidate.bytes, &limits).unwrap();
            opened.verify().unwrap();
            let image = goya_graphics_ir::reference::render(
                &opened.to_drawing().unwrap(),
                1.0,
                &Default::default(),
            )
            .unwrap();
            let mut expected = resolve_tile_rgba(&tile, &palette).unwrap();
            if transparency == Transparency::IndexZero {
                for (i, pixel) in expected.iter_mut().enumerate() {
                    if tile.pixels[i] == 0 {
                        *pixel = [0; 4];
                    }
                }
            }
            assert_eq!(
                image.rgba,
                expected.into_iter().flatten().collect::<Vec<_>>()
            );
            assert_eq!(candidate.tile_source, tile.provenance);
            assert!(candidate.palette_sources.contains(&palette[1].provenance));
        }
        let mut malformed = tile.clone();
        malformed.pixels.push(0);
        assert!(matches!(
            tile_to_gva(&malformed, &palette, Transparency::Opaque),
            Err(TileGvaError::Graphics(Error::InvalidTile))
        ));
    }

    #[test]
    fn four_bpp_keeps_low_nibble_first_and_provenance() {
        let mut source = [0u8; 32];
        source[0] = 0xa3;
        source[1] = 0x21;
        source[31] = 0xf4;
        let view = AssetView::new(&source, Space::Rom).unwrap();
        let tile = view.tile(0, TileDepth::FourBpp).unwrap();
        assert_eq!(&tile.pixels[..4], &[3, 10, 1, 2]);
        assert_eq!(&tile.pixels[62..], &[4, 15]);
        assert_eq!(tile.pixels.len(), 64);
        assert_eq!(tile.provenance.byte_range, [0, 32]);
        assert_eq!(tile.provenance.address_range, [0x0800_0000, 0x0800_0020]);
        assert_eq!(tile.provenance.source_sha256, view.source_sha256());
    }

    #[test]
    fn eight_bpp_is_row_major_and_bounded() {
        let source: Vec<u8> = (0..64).collect();
        let view = AssetView::new(&source, Space::Vram).unwrap();
        let tile = view.tile(0, TileDepth::EightBpp).unwrap();
        assert_eq!(&tile.pixels[8..16], &[8, 9, 10, 11, 12, 13, 14, 15]);
        assert_eq!(tile.provenance.address_range, [0x0600_0000, 0x0600_0040]);
        assert!(matches!(
            view.tile(1, TileDepth::EightBpp),
            Err(Error::OutOfBounds { .. })
        ));
    }

    #[test]
    fn palette_and_map_bits_remain_exact() {
        let palette = [0x1f, 0xfc]; // R=31, G=0, B=31, bit15=1
        let colour = AssetView::new(&palette, Space::PaletteRam)
            .unwrap()
            .palette_entry(0)
            .unwrap();
        assert_eq!((colour.red5, colour.green5, colour.blue5), (31, 0, 31));
        assert_eq!(colour.raw, 0xfc1f);
        let map = [0x55, 0xec]; // tile 0x55, both flips, bank 14
        let entry = AssetView::new(&map, Space::Vram)
            .unwrap()
            .text_map_entry(0)
            .unwrap();
        assert_eq!(entry.tile_number, 0x55);
        assert!(entry.horizontal_flip && entry.vertical_flip);
        assert_eq!(entry.palette_bank, 14);
    }

    #[test]
    fn memory_spaces_and_limits_are_enforced() {
        assert_eq!(AssetView::new(&[], Space::Rom).err(), Some(Error::Empty));
        assert!(matches!(
            AssetView::new(&[0; 1025], Space::PaletteRam),
            Err(Error::TooLarge { .. })
        ));
        let palette = AssetView::new(&[0; 2], Space::PaletteRam).unwrap();
        assert_eq!(palette.tile(0, TileDepth::FourBpp), Err(Error::WrongSpace));
    }
}
