//! GVI-L style palette. Colour references share the style record's byte stream.
use goya_graphics_ir::{codes, GfxDiag, Style};
use goya_gva::{decode_style_record, encode_style_record, GvaLimits};

fn error(message: &str) -> GfxDiag {
    GfxDiag::new(codes::MALFORMED_SECTION, format!("LSTY: {message}"))
}

/// Header: style count u32, colour count u16, ten zero bytes. Then sorted
/// RGBA colours and twelve-byte style records, padded to sixteen bytes.
pub(crate) fn encode(styles: &[Style]) -> Option<Vec<u8>> {
    let records: Vec<_> = styles.iter().map(encode_style_record).collect();
    let mut colours: Vec<[u8; 4]> = records
        .iter()
        .flat_map(|r| {
            let mut v = Vec::new();
            if r[0] & 1 != 0 {
                v.push([r[4], r[5], r[6], r[7]]);
            }
            if r[0] & 2 != 0 {
                v.push([r[8], r[9], r[10], r[11]]);
            }
            v
        })
        .collect();
    colours.sort_unstable();
    colours.dedup();
    if colours.len() >= u16::MAX as usize {
        return None;
    }
    let mut out = Vec::with_capacity(16 + colours.len() * 4 + styles.len() * 12);
    out.extend_from_slice(&(styles.len() as u32).to_le_bytes());
    out.extend_from_slice(&(colours.len() as u16).to_le_bytes());
    out.extend_from_slice(&[0; 10]);
    for c in &colours {
        out.extend_from_slice(c);
    }
    for r in &records {
        out.extend_from_slice(&r[..4]);
        for (bit, start) in [(1, 4), (2, 8)] {
            let index = if r[0] & bit == 0 {
                u16::MAX
            } else {
                // Every colour was collected above, so the search succeeds.
                colours
                    .binary_search(&[r[start], r[start + 1], r[start + 2], r[start + 3]])
                    .ok()? as u16
            };
            out.extend_from_slice(&index.to_le_bytes());
        }
        out.extend_from_slice(&r[12..16]);
    }
    while !out.len().is_multiple_of(16) {
        out.push(0);
    }
    Some(out)
}

pub(crate) fn decode(data: &[u8], limits: &GvaLimits) -> Result<Vec<Style>, GfxDiag> {
    if data.len() < 16 {
        return Err(error("short header"));
    }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let nc = u16::from_le_bytes([data[4], data[5]]) as usize;
    if count > limits.max_styles as usize || data[6..16] != [0; 10] {
        return Err(error("invalid count or reserved bytes"));
    }
    let end = 16usize
        .checked_add(nc.checked_mul(4).ok_or_else(|| error("size overflow"))?)
        .and_then(|v| v.checked_add(count.checked_mul(12)?))
        .ok_or_else(|| error("size overflow"))?;
    if end > data.len()
        || end.div_ceil(16) * 16 != data.len()
        || data[end..].iter().any(|&b| b != 0)
    {
        return Err(error("invalid length or padding"));
    }
    let palette: Vec<[u8; 4]> = data[16..16 + nc * 4]
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3]])
        .collect();
    if palette.windows(2).any(|w| w[0] >= w[1]) {
        return Err(error("palette is not sorted and unique"));
    }
    let mut styles = Vec::with_capacity(count);
    for compact in data[16 + nc * 4..end].chunks_exact(12) {
        let mut record = [0u8; 16];
        record[..4].copy_from_slice(&compact[..4]);
        for (bit, at, dst) in [(1, 4, 4), (2, 6, 8)] {
            let idx = u16::from_le_bytes([compact[at], compact[at + 1]]);
            if record[0] & bit != 0 {
                record[dst..dst + 4].copy_from_slice(
                    palette
                        .get(idx as usize)
                        .ok_or_else(|| error("colour index out of range"))?,
                );
            } else if idx != u16::MAX {
                return Err(error("absent colour has an index"));
            }
        }
        record[12..16].copy_from_slice(&compact[8..12]);
        styles.push(decode_style_record(&record)?);
    }
    if encode(&styles).as_deref() != Some(data) {
        return Err(GfxDiag::new(codes::NON_CANONICAL, "LSTY is not canonical"));
    }
    Ok(styles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_gva::encode_style_pool;

    #[test]
    fn shared_colours_round_trip_and_reject_noncanonical_palette() {
        let styles: Vec<_> = (1..=100)
            .map(|width| {
                let mut record = [0u8; 16];
                record[0] = 2;
                record[8..12].copy_from_slice(&[10, 20, 30, 255]);
                record[12..16].copy_from_slice(&(width as u32).to_le_bytes());
                decode_style_record(&record).unwrap()
            })
            .collect();
        let encoded = encode(&styles).unwrap();
        assert!(encoded.len() < encode_style_pool(&styles).len());
        assert_eq!(decode(&encoded, &GvaLimits::default()).unwrap(), styles);
        let mut invalid = encoded;
        invalid[24] = 0;
        assert!(decode(&invalid, &GvaLimits::default()).is_err());
    }
}
