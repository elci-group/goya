use crate::error::Error;

/// Maximum RGBA pixels accepted by one present. Matches the Wayland host cap.
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;

/// Truecolor layout discovered from the server setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFormat {
    pub depth: u8,
    pub bits_per_pixel: u8,
    pub scanline_pad: u8,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub byte_order_lsb: bool,
}

/// Native image bytes ready for `PutImage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub width: u16,
    pub height: u16,
    pub stride: usize,
    pub depth: u8,
    pub bytes: Vec<u8>,
}

/// Pack tightly packed top-down straight-alpha RGBA8 into the server visual.
///
/// Alpha is composited over `background`. The source is not scaled.
pub fn encode(
    rgba: &[u8],
    width: u16,
    height: u16,
    format: PixelFormat,
    background: [u8; 3],
) -> Result<EncodedFrame, Error> {
    if width == 0 || height == 0 {
        return Err(Error::PixelBuffer("width and height must be non-zero"));
    }
    let pixels = usize::from(width)
        .checked_mul(usize::from(height))
        .ok_or(Error::PixelBuffer("dimensions overflow"))?;
    if pixels > MAX_PIXELS {
        return Err(Error::PixelBuffer("frame is above 16 megapixels"));
    }
    let expected = pixels
        .checked_mul(4)
        .ok_or(Error::PixelBuffer("byte length overflow"))?;
    if rgba.len() != expected {
        return Err(Error::PixelBuffer("rgba length does not match the frame"));
    }
    let nbytes = match format.bits_per_pixel {
        16 | 32 => usize::from(format.bits_per_pixel / 8),
        _ => {
            return Err(Error::UnsupportedVisual {
                depth: format.depth,
                bits_per_pixel: format.bits_per_pixel,
            });
        }
    };
    let stride = stride_bytes(width, format.bits_per_pixel, format.scanline_pad)?;
    let len = stride
        .checked_mul(usize::from(height))
        .ok_or(Error::PixelBuffer("image length overflow"))?;
    let mut bytes = vec![0u8; len];
    for y in 0..usize::from(height) {
        for x in 0..usize::from(width) {
            let src = (y * usize::from(width) + x) * 4;
            let red = composite(rgba[src], background[0], rgba[src + 3]);
            let green = composite(rgba[src + 1], background[1], rgba[src + 3]);
            let blue = composite(rgba[src + 2], background[2], rgba[src + 3]);
            let pixel = pack_channel(red, format.red_mask)
                | pack_channel(green, format.green_mask)
                | pack_channel(blue, format.blue_mask);
            let raw = if format.byte_order_lsb {
                pixel.to_le_bytes()
            } else {
                pixel.to_be_bytes()
            };
            let start = if format.byte_order_lsb { 0 } else { 4 - nbytes };
            let dest = y * stride + x * nbytes;
            bytes[dest..dest + nbytes].copy_from_slice(&raw[start..start + nbytes]);
        }
    }
    Ok(EncodedFrame {
        width,
        height,
        stride,
        depth: format.depth,
        bytes,
    })
}

fn stride_bytes(width: u16, bits_per_pixel: u8, scanline_pad: u8) -> Result<usize, Error> {
    let pad = u32::from(scanline_pad.max(8));
    if !pad.is_power_of_two() {
        return Err(Error::PixelBuffer("scanline pad is not a power of two"));
    }
    let bits = u32::from(width)
        .checked_mul(u32::from(bits_per_pixel))
        .ok_or(Error::PixelBuffer("stride overflow"))?;
    let padded = bits.div_ceil(pad).saturating_mul(pad);
    usize::try_from(padded / 8).map_err(|_| Error::PixelBuffer("stride overflow"))
}

fn composite(src: u8, dst: u8, alpha: u8) -> u8 {
    let alpha = u16::from(alpha);
    ((u16::from(src) * alpha + u16::from(dst) * (255 - alpha)) / 255) as u8
}

fn pack_channel(value: u8, mask: u32) -> u32 {
    let bits = mask.count_ones();
    if bits == 0 {
        return 0;
    }
    let scaled = if bits >= 8 {
        u32::from(value) << (bits - 8)
    } else {
        (u32::from(value) * ((1u32 << bits) - 1)) / 255
    };
    (scaled << mask.trailing_zeros()) & mask
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xorg_24() -> PixelFormat {
        PixelFormat {
            depth: 24,
            bits_per_pixel: 32,
            scanline_pad: 32,
            red_mask: 0x00ff_0000,
            green_mask: 0x0000_ff00,
            blue_mask: 0x0000_00ff,
            byte_order_lsb: true,
        }
    }

    #[test]
    fn opaque_red_is_little_endian_00ff0000() {
        let frame = encode(&[255, 0, 0, 255], 1, 1, xorg_24(), [0, 0, 0]).unwrap();
        assert_eq!(frame.bytes, vec![0x00, 0x00, 0xff, 0x00]);
    }

    #[test]
    fn half_red_composites_over_white() {
        let frame = encode(&[255, 0, 0, 128], 1, 1, xorg_24(), [255, 255, 255]).unwrap();
        assert_eq!(frame.bytes[0], 127);
        assert_eq!(frame.bytes[1], 127);
        assert_eq!(frame.bytes[2], 255);
    }

    #[test]
    fn a_short_buffer_is_rejected() {
        let err = encode(&[255, 0, 0, 255], 2, 1, xorg_24(), [0, 0, 0]).unwrap_err();
        assert_eq!(
            err,
            Error::PixelBuffer("rgba length does not match the frame")
        );
    }

    #[test]
    fn zero_size_is_rejected() {
        assert!(encode(&[], 0, 1, xorg_24(), [0, 0, 0]).is_err());
    }
}
