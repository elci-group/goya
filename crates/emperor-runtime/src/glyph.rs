//! Fixed 8x8 cell stamp for the Kumimanu software frame.
//!
//! Glyph positions are the admitted DIP rectangles divided by the same
//! millipoint scale the Wayland host uses. This module does not shape text
//! or choose a font. It is not the Wayland raster and it is not visual parity.

use goya_layout::Rect;
use goya_runtime::Primitive;

use crate::host::SceneFrame;

const SCALE: i64 = goya_wayland::MP_PER_PX;

#[path = "glyph_table.rs"]
mod glyph_table;

pub(crate) fn paint(frame: &SceneFrame, width: u16, height: u16) -> Result<Vec<u8>, String> {
    let width = usize::from(width);
    let height = usize::from(height);
    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| "frame size overflow".to_owned())?;
    if pixels > 16 * 1024 * 1024 {
        return Err("frame is above 16 megapixels".into());
    }
    let mut rgba = vec![0u8; pixels.saturating_mul(4)];
    let mut surface = Surface {
        buf: &mut rgba,
        width,
        height,
    };
    surface.fill(0, 0, width as i32, height as i32, [235, 235, 240, 255]);
    for (primitive, fill_color) in frame.primitives.iter().zip(frame.fills.iter()) {
        let rect = match primitive {
            Primitive::Panel(rect) | Primitive::Text(rect, _) | Primitive::Button(rect, _) => *rect,
        };
        let (x, y, w, h) = px_rect(rect);
        let background = match (primitive, fill_color) {
            (_, Some(packed)) => unpack(*packed),
            (Primitive::Panel(_), None) => [255, 255, 255, 255],
            (Primitive::Button(_, _), None) => [210, 210, 220, 255],
            (Primitive::Text(_, _), None) => [0, 0, 0, 0],
        };
        if background[3] != 0 {
            surface.fill(x, y, w, h, background);
        }
        let text = match primitive {
            Primitive::Text(_, text) | Primitive::Button(_, text) => text.as_str(),
            Primitive::Panel(_) => "",
        };
        surface.stamp(x + 4, y + 4, text, [20, 20, 28, 255]);
    }
    Ok(rgba)
}

/// The pixel buffer being painted, with the bounds its coordinates are
/// clipped to. Grouping them keeps the drawing helpers below the clippy
/// argument count without threading three related parameters by hand.
struct Surface<'a> {
    buf: &'a mut [u8],
    width: usize,
    height: usize,
}

impl Surface<'_> {
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
        if w <= 0 || h <= 0 || color[3] == 0 {
            return;
        }
        let x1 = x.max(0) as usize;
        let y1 = y.max(0) as usize;
        let x2 = (x.saturating_add(w)).max(0) as usize;
        let y2 = (y.saturating_add(h)).max(0) as usize;
        for row in y1..y2.min(self.height) {
            for col in x1..x2.min(self.width) {
                self.blend(col, row, color);
            }
        }
    }

    fn stamp(&mut self, x: i32, y: i32, text: &str, color: [u8; 4]) {
        let mut cursor = x;
        for ch in text.chars() {
            let code = ch as u32;
            if (32..128).contains(&code) {
                let glyph = glyph_table::FONT[code as usize];
                for (row, bits) in glyph.iter().enumerate() {
                    for col in 0..8 {
                        if bits & (1 << (7 - col)) == 0 {
                            continue;
                        }
                        let px = cursor + col;
                        let py = y + row as i32;
                        if px < 0
                            || py < 0
                            || px as usize >= self.width
                            || py as usize >= self.height
                        {
                            continue;
                        }
                        self.blend(px as usize, py as usize, color);
                    }
                }
            }
            cursor += 8;
        }
    }

    fn blend(&mut self, x: usize, y: usize, color: [u8; 4]) {
        if x >= self.width {
            return;
        }
        let index = y
            .checked_mul(self.width)
            .and_then(|offset| offset.checked_add(x))
            .and_then(|pixel| pixel.checked_mul(4));
        let Some(index) = index else {
            return;
        };
        if index + 3 >= self.buf.len() || color[3] == 0 {
            return;
        }
        let alpha = u16::from(color[3]);
        for (dst, src) in self.buf[index..index + 3].iter_mut().zip(color[..3].iter()) {
            let src = u16::from(*src);
            let old = u16::from(*dst);
            *dst = ((src * alpha + old * (255 - alpha)) / 255) as u8;
        }
        self.buf[index + 3] = 255;
    }
}

fn px_rect(rect: Rect) -> (i32, i32, i32, i32) {
    (
        (rect.x / SCALE) as i32,
        (rect.y / SCALE) as i32,
        (rect.width / SCALE) as i32,
        (rect.height / SCALE) as i32,
    )
}

fn unpack(packed: u32) -> [u8; 4] {
    [
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
        ((packed >> 24) & 0xff) as u8,
    ]
}
