//! Cooperative authority transfer.
//!
//! A prepared record keeps the original writer. A granted record names exactly
//! one successor. A snapshot that disagrees with the record admits nobody:
//! this crate will not elect a second writer to fill the gap.

use crate::canonical::Sink;
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffRecord {
    Prepared {
        from: [u8; 16],
        to: [u8; 16],
        epoch: u64,
        revision: u64,
    },
    Granted {
        from: [u8; 16],
        to: [u8; 16],
        epoch: u64,
        revision: u64,
        fingerprint: [u8; 32],
    },
}

impl HandoffRecord {
    pub fn revision(self) -> u64 {
        match self {
            Self::Prepared { revision, .. } | Self::Granted { revision, .. } => revision,
        }
    }
}

/// The only device allowed to commit, and the epoch it must use.
pub fn admitted_writer(record: &HandoffRecord) -> ([u8; 16], u64) {
    match *record {
        HandoffRecord::Prepared { from, epoch, .. } => (from, epoch),
        HandoffRecord::Granted { to, epoch, .. } => (to, epoch),
    }
}

pub fn encode_checkpoint(snapshot: &[u8], record: &HandoffRecord) -> Result<Vec<u8>, Error> {
    let (kind, from, to, epoch, revision, fingerprint) = match *record {
        HandoffRecord::Prepared {
            from,
            to,
            epoch,
            revision,
        } => (1u8, from, to, epoch, revision, [0u8; 32]),
        HandoffRecord::Granted {
            from,
            to,
            epoch,
            revision,
            fingerprint,
        } => (2, from, to, epoch, revision, fingerprint),
    };
    if epoch == 0 || from == to {
        return Err(Error::new(
            "RMT-E5008",
            "authority record does not name one successor epoch",
        ));
    }
    let len = u32::try_from(snapshot.len())
        .map_err(|_| Error::new("RMT-E8001", "checkpoint snapshot exceeds the byte budget"))?;
    let mut out = Vec::new();
    let mut sink = Sink::new(&mut out);
    sink.push(b"RCK1")?;
    sink.push(&[1, kind])?;
    sink.push(&from)?;
    sink.push(&to)?;
    sink.push(&epoch.to_le_bytes())?;
    sink.push(&revision.to_le_bytes())?;
    sink.push(&fingerprint)?;
    sink.push(&len.to_le_bytes())?;
    sink.push(snapshot)?;
    Ok(out)
}

pub fn decode_checkpoint(bytes: &[u8]) -> Result<(Vec<u8>, HandoffRecord), Error> {
    if bytes.len() < 4 + 2 + 16 + 16 + 8 + 8 + 32 + 4 || &bytes[..4] != b"RCK1" || bytes[4] != 1 {
        return Err(Error::new(
            "RMT-E6001",
            "checkpoint is truncated or corrupt",
        ));
    }
    let mut offset = 5;
    let kind = bytes[offset];
    offset += 1;
    let from = read_array::<16>(bytes, &mut offset)?;
    let to = read_array::<16>(bytes, &mut offset)?;
    let epoch = read_u64(bytes, &mut offset)?;
    let revision = read_u64(bytes, &mut offset)?;
    let fingerprint = read_array::<32>(bytes, &mut offset)?;
    let len = read_u32(bytes, &mut offset)? as usize;
    if offset.checked_add(len) != Some(bytes.len()) || epoch == 0 || from == to {
        return Err(Error::new(
            "RMT-E6001",
            "checkpoint is truncated or corrupt",
        ));
    }
    let snapshot = bytes[offset..].to_vec();
    let record = match kind {
        1 if fingerprint == [0u8; 32] => HandoffRecord::Prepared {
            from,
            to,
            epoch,
            revision,
        },
        2 => HandoffRecord::Granted {
            from,
            to,
            epoch,
            revision,
            fingerprint,
        },
        _ => {
            return Err(Error::new(
                "RMT-E6001",
                "checkpoint is truncated or corrupt",
            ))
        }
    };
    Ok((snapshot, record))
}

fn read_array<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], Error> {
    let end = offset.checked_add(N).ok_or(Error::new(
        "RMT-E6001",
        "checkpoint is truncated or corrupt",
    ))?;
    if end > bytes.len() {
        return Err(Error::new(
            "RMT-E6001",
            "checkpoint is truncated or corrupt",
        ));
    }
    let mut raw = [0u8; N];
    raw.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(raw)
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, Error> {
    let raw = read_array::<4>(bytes, offset)?;
    Ok(u32::from_le_bytes(raw))
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64, Error> {
    let raw = read_array::<8>(bytes, offset)?;
    Ok(u64::from_le_bytes(raw))
}
