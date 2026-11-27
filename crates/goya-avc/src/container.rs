//! The AOT-VC binary container (ADR-AVC-001 Phase 2, decision 7).
//!
//! Deterministic, little-endian, section-tagged. Every section is the
//! union of tags fixed by the Phase-0 collision review (`tags.rs`);
//! adoption runs through the tag review test and the header version.
//! The encoder's output depends only on the `CompiledVideo` — byte-
//! identical repeat builds of the same source under the same profile —
//! and `decode(encode(v)) == v` with `encode(decode(b)) == b`
//! (idempotence), which is the byte-identical-repeat-builds acceptance
//! target at the container level.
//!
//! Safety posture ("malformed binary handling: safe rejection"): every
//! read is bounds-checked against the section payload, unknown tags and
//! variant bytes are rejected explicitly, declared counts never drive an
//! allocation (they drive a bounded read loop), and checkpoint digests
//! are re-derived and compared so a tampered payload is rejected at
//! decode, not at playback.

use crate::audio::AudioSidecar;
use crate::clock::MediaClock;
use crate::colour::{ColourEntry, PaletteBound, Tolerance, WorkingSpace};
use crate::compile::CompiledVideo;
use crate::delta::{ContentId, PixelChange, RowDelta};
use crate::instructions::{Instruction, Target};
use crate::pixel::{ApproximationCause, ColourIndex, Exactness, Light, PixelState, MAX_TRIMS};
use crate::tags;
use crate::temporal::{Checkpoint, EventId, TemporalStateId, Timeline};

/// Container magic: `GYAV` — Goya AOT-VC.
const MAGIC: [u8; 4] = *b"GYAV";
/// Current container version. Decoders reject a higher major version
/// explicitly (version negotiation, decision 7).
const VERSION: u16 = 1;

const TAG_PALT: [u8; 4] = tags::PALT.0;
const TAG_ROWS: [u8; 4] = tags::ROWS.0;
const TAG_DELT: [u8; 4] = tags::DELT.0;
const TAG_EVNT: [u8; 4] = tags::EVNT.0;
const TAG_CHKP: [u8; 4] = tags::CHKP.0;
const TAG_AUDS: [u8; 4] = tags::AUDS.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerError {
    /// The leading bytes are not the `GYAV` magic.
    BadMagic,
    /// The container's version is newer than this decoder (version
    /// negotiation failed) — the message names both versions.
    UnsupportedVersion { found: u16, supported: u16 },
    /// A section tag is not in the negotiated tag set.
    UnknownSectionTag { tag: [u8; 4] },
    /// A required section is missing.
    MissingSection { tag: [u8; 4] },
    /// A section payload is shorter than its declared structure requires.
    SectionTruncated {
        tag: [u8; 4],
        needed: usize,
        available: usize,
    },
    /// An instruction, target, exactness or cause byte is not defined.
    UnknownVariant { what: &'static str, byte: u8 },
    /// A structural invariant of the container is violated (e.g. a
    /// delta's id does not equal its position, a row's width is wrong,
    /// a checkpoint digest does not match its payload).
    Malformed(&'static str),
}

impl core::fmt::Display for ContainerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ContainerError::BadMagic => write!(f, "not a GYAV container (bad magic)"),
            ContainerError::UnsupportedVersion { found, supported } => {
                write!(
                    f,
                    "container version {found} is newer than supported version {supported}"
                )
            }
            ContainerError::UnknownSectionTag { tag } => {
                write!(
                    f,
                    "unknown section tag {:?}",
                    std::str::from_utf8(tag).unwrap_or("????")
                )
            }
            ContainerError::MissingSection { tag } => {
                write!(
                    f,
                    "missing required section {:?}",
                    std::str::from_utf8(tag).unwrap_or("????")
                )
            }
            ContainerError::SectionTruncated {
                tag,
                needed,
                available,
            } => write!(
                f,
                "section {:?} truncated: needs {needed} bytes, has {available}",
                std::str::from_utf8(tag).unwrap_or("????")
            ),
            ContainerError::UnknownVariant { what, byte } => {
                write!(f, "unknown {what} variant byte {byte}")
            }
            ContainerError::Malformed(why) => write!(f, "malformed container: {why}"),
        }
    }
}

// ---------------------------------------------------------------- encode

/// Encode a compiled video into the canonical container bytes.
pub fn encode_container(video: &CompiledVideo) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved flags

    out.extend_from_slice(&(video.source_digest.len() as u32).to_le_bytes());
    out.extend_from_slice(video.source_digest.as_bytes());
    out.extend_from_slice(&video.dimensions.0.to_le_bytes());
    out.extend_from_slice(&video.dimensions.1.to_le_bytes());

    let (palt, rows, delt, evnt, chkp, auds) = encode_sections(video);
    // AUDS is present only when the video carries an audio sidecar
    // descriptor; a silent container has no AUDS section at all.
    let auds = auds.map(|payload| (TAG_AUDS, payload));
    let sections = [
        Some((TAG_PALT, &palt)),
        Some((TAG_ROWS, &rows)),
        Some((TAG_DELT, &delt)),
        Some((TAG_EVNT, &evnt)),
        Some((TAG_CHKP, &chkp)),
        auds.as_ref().map(|(tag, payload)| (*tag, payload)),
    ];
    for (tag, payload) in sections.into_iter().flatten() {
        out.extend_from_slice(&tag);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
    }
    out
}

type SectionPayloads = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>);

fn encode_sections(video: &CompiledVideo) -> SectionPayloads {
    // PALT — the colour inventory, with the bound and tolerance the
    // closure ran under (round-trip equality: decode must rebuild the
    // exact inventory, not a re-derived one).
    let mut palt = Vec::new();
    palt.extend_from_slice(&video.inventory.bound().0.to_le_bytes());
    palt.extend_from_slice(&video.inventory.tolerance().0.to_le_bytes());
    palt.extend_from_slice(&(video.inventory.entries().len() as u32).to_le_bytes());
    for entry in video.inventory.entries() {
        for channel in &entry.linear_rgb {
            palt.extend_from_slice(&channel.to_le_bytes());
        }
    }

    // ROWS — the deduplicated row inventory (2 B per chroma index).
    let mut rows = Vec::new();
    rows.extend_from_slice(&(video.row_inventory.len() as u32).to_le_bytes());
    for row in &video.row_inventory {
        rows.extend_from_slice(&(row.len() as u32).to_le_bytes());
        for colour in row {
            rows.extend_from_slice(&colour.0.to_le_bytes());
        }
    }

    // DELT — the delta registry.
    let mut delt = Vec::new();
    delt.extend_from_slice(&(video.deltas.len() as u32).to_le_bytes());
    for delta in &video.deltas {
        delt.extend_from_slice(&delta.id.to_le_bytes());
        delt.extend_from_slice(&delta.from.0.to_le_bytes());
        delt.extend_from_slice(&delta.to.0.to_le_bytes());
        delt.extend_from_slice(&(delta.changes.len() as u32).to_le_bytes());
        for change in &delta.changes {
            delt.extend_from_slice(&change.offset.to_le_bytes());
            delt.extend_from_slice(&change.to.0.to_le_bytes());
        }
    }

    // EVNT — the scheduled instruction stream.
    let mut evnt = Vec::new();
    let events = video.timeline.events();
    evnt.extend_from_slice(&(events.len() as u32).to_le_bytes());
    for event in events {
        evnt.extend_from_slice(&event.id.0.to_le_bytes());
        evnt.extend_from_slice(&event.at.ticks().to_le_bytes());
        evnt.extend_from_slice(&event.at.ticks_per_second().to_le_bytes());
        encode_instruction(&mut evnt, &event.instruction);
    }

    // CHKP — the seek checkpoints (payload retained so the digest can be
    // re-derived on decode; a tampered payload is a decode error).
    let mut chkp = Vec::new();
    let checkpoints = video.timeline.checkpoints();
    chkp.extend_from_slice(&(checkpoints.len() as u32).to_le_bytes());
    for checkpoint in checkpoints {
        chkp.extend_from_slice(&checkpoint.state.0.to_le_bytes());
        chkp.extend_from_slice(&checkpoint.at.ticks().to_le_bytes());
        chkp.extend_from_slice(&checkpoint.at.ticks_per_second().to_le_bytes());
        chkp.extend_from_slice(&(checkpoint.payload_bytes().len() as u32).to_le_bytes());
        chkp.extend_from_slice(checkpoint.payload_bytes());
        chkp.extend_from_slice(&checkpoint.digest);
    }

    // AUDS — the audio sidecar descriptor, verbatim (AVC-C07: never
    // samples).
    let auds = video.audio.as_ref().map(|audio| {
        let mut section = Vec::new();
        section.extend_from_slice(&(audio.codec.len() as u32).to_le_bytes());
        section.extend_from_slice(audio.codec.as_bytes());
        section.extend_from_slice(&audio.duration_ts.to_le_bytes());
        section.extend_from_slice(&audio.time_base.0.to_le_bytes());
        section.extend_from_slice(&audio.time_base.1.to_le_bytes());
        section.extend_from_slice(&audio.start_ts.to_le_bytes());
        section
    });

    (palt, rows, delt, evnt, chkp, auds)
}

fn encode_target(out: &mut Vec<u8>, target: &Target) {
    match target {
        Target::Pixel { row, pixel } => {
            out.push(0);
            out.extend_from_slice(&row.to_le_bytes());
            out.extend_from_slice(&pixel.to_le_bytes());
        }
        Target::Row { row } => {
            out.push(1);
            out.extend_from_slice(&row.to_le_bytes());
        }
        Target::Region {
            row,
            pixel,
            height,
            width,
        } => {
            out.push(2);
            out.extend_from_slice(&row.to_le_bytes());
            out.extend_from_slice(&pixel.to_le_bytes());
            out.extend_from_slice(&height.to_le_bytes());
            out.extend_from_slice(&width.to_le_bytes());
        }
    }
}

fn encode_exactness(out: &mut Vec<u8>, exactness: &Exactness) {
    match exactness {
        Exactness::Exact => out.push(0),
        Exactness::Approximate(cause) => {
            out.push(1);
            out.push(match cause {
                ApproximationCause::TrimAssisted => 0,
                ApproximationCause::ToleranceBudget => 1,
                ApproximationCause::PaletteClosure => 2,
            });
        }
    }
}

fn encode_pixel_state(out: &mut Vec<u8>, state: &PixelState) {
    out.extend_from_slice(&state.fill.0.to_le_bytes());
    out.extend_from_slice(&state.light.0.to_le_bytes());
    encode_exactness(out, &state.exactness);
    for trim in &state.trims {
        match trim {
            Some(colour) => {
                out.push(1);
                out.extend_from_slice(&colour.0.to_le_bytes());
            }
            None => out.push(0),
        }
    }
}

fn encode_instruction(out: &mut Vec<u8>, instruction: &Instruction) {
    match instruction {
        Instruction::SetFill { target, colour } => {
            out.push(0);
            encode_target(out, target);
            out.extend_from_slice(&colour.0.to_le_bytes());
        }
        Instruction::SetTrim {
            target,
            slot,
            colour,
        } => {
            out.push(1);
            encode_target(out, target);
            out.push(*slot);
            out.extend_from_slice(&colour.0.to_le_bytes());
        }
        Instruction::SetLight {
            target,
            light,
            exactness,
        } => {
            out.push(2);
            encode_target(out, target);
            out.extend_from_slice(&light.0.to_le_bytes());
            encode_exactness(out, exactness);
        }
        Instruction::SetRun {
            row,
            start,
            len,
            state,
        } => {
            out.push(3);
            out.extend_from_slice(&row.to_le_bytes());
            out.extend_from_slice(&start.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            encode_pixel_state(out, state);
        }
        Instruction::SetRow { target, states } => {
            out.push(4);
            encode_target(out, target);
            out.extend_from_slice(&(states.len() as u32).to_le_bytes());
            for state in states {
                encode_pixel_state(out, state);
            }
        }
        Instruction::SetRegion { region, state } => {
            out.push(5);
            encode_target(out, region);
            encode_pixel_state(out, state);
        }
        Instruction::CopyRegion { from, to } => {
            out.push(6);
            encode_target(out, from);
            encode_target(out, to);
        }
        Instruction::ApplyDelta { row, delta_id } => {
            out.push(7);
            out.extend_from_slice(&row.to_le_bytes());
            out.extend_from_slice(&delta_id.to_le_bytes());
        }
        Instruction::LoadState { checkpoint_id } => {
            out.push(8);
            out.extend_from_slice(&checkpoint_id.to_le_bytes());
        }
        Instruction::Sync => out.push(9),
        Instruction::Present => out.push(10),
    }
}

// ---------------------------------------------------------------- decode

/// Decode a container into a compiled video, rejecting malformed input
/// safely (no panics, no untrusted allocations).
pub fn decode_container(bytes: &[u8]) -> Result<CompiledVideo, ContainerError> {
    let mut r = Reader { bytes, pos: 0 };
    let magic = r.take(4, b"HDRS")?;
    if magic != MAGIC {
        return Err(ContainerError::BadMagic);
    }
    let version = r.read_u16()?;
    if version > VERSION {
        return Err(ContainerError::UnsupportedVersion {
            found: version,
            supported: VERSION,
        });
    }
    let _flags = r.read_u16()?;

    let digest_len = r.read_u32()? as usize;
    let digest_bytes = r.take(digest_len, b"HDRS")?;
    let source_digest = String::from_utf8(digest_bytes.to_vec())
        .map_err(|_| ContainerError::Malformed("source digest is not UTF-8"))?;
    let width = r.read_u32()?;
    let height = r.read_u32()?;
    let dimensions = (width, height);

    // Sections: fixed order, each present at most once.
    let mut palt: Option<Vec<u8>> = None;
    let mut rows: Option<Vec<u8>> = None;
    let mut delt: Option<Vec<u8>> = None;
    let mut evnt: Option<Vec<u8>> = None;
    let mut chkp: Option<Vec<u8>> = None;
    let mut auds: Option<Vec<u8>> = None;
    while r.pos + 8 <= r.bytes.len() {
        let mut tag = [0u8; 4];
        tag.copy_from_slice(r.take(4, b"SECT")?);
        let len = r.read_u32()? as usize;
        let payload = r.take(len, &tag)?;
        let slot = match &tag {
            t if *t == TAG_PALT => &mut palt,
            t if *t == TAG_ROWS => &mut rows,
            t if *t == TAG_DELT => &mut delt,
            t if *t == TAG_EVNT => &mut evnt,
            t if *t == TAG_CHKP => &mut chkp,
            t if *t == TAG_AUDS => &mut auds,
            _ => return Err(ContainerError::UnknownSectionTag { tag }),
        };
        if slot.is_some() {
            return Err(ContainerError::Malformed("duplicate section"));
        }
        *slot = Some(payload.to_vec());
    }
    if r.pos != r.bytes.len() {
        return Err(ContainerError::Malformed(
            "trailing bytes after final section",
        ));
    }

    let palt = palt.ok_or(ContainerError::MissingSection { tag: TAG_PALT })?;
    let rows = rows.ok_or(ContainerError::MissingSection { tag: TAG_ROWS })?;
    let delt = delt.ok_or(ContainerError::MissingSection { tag: TAG_DELT })?;
    let evnt = evnt.ok_or(ContainerError::MissingSection { tag: TAG_EVNT })?;
    let chkp = chkp.ok_or(ContainerError::MissingSection { tag: TAG_CHKP })?;
    let audio = match auds {
        Some(section) => {
            let mut ar = Reader {
                bytes: &section,
                pos: 0,
            };
            let codec_len = ar.read_u32()? as usize;
            let codec_bytes = ar.take(codec_len, &TAG_AUDS)?;
            let codec = String::from_utf8(codec_bytes.to_vec())
                .map_err(|_| ContainerError::Malformed("audio codec name is not UTF-8"))?;
            let duration_ts = i64::from_le_bytes(ar.read_u64()?.to_le_bytes());
            let num = ar.read_u32()?;
            let den = ar.read_u32()?;
            if den == 0 {
                return Err(ContainerError::Malformed(
                    "audio time base denominator is zero",
                ));
            }
            let start_ts = i64::from_le_bytes(ar.read_u64()?.to_le_bytes());
            if ar.pos != ar.bytes.len() {
                return Err(ContainerError::Malformed(
                    "audio sidecar section has trailing bytes",
                ));
            }
            Some(AudioSidecar {
                codec,
                duration_ts,
                time_base: (num, den),
                start_ts,
            })
        }
        None => None,
    };

    // PALT → ColourInventory (revalidated through the constructor).
    let mut pr = Reader {
        bytes: &palt,
        pos: 0,
    };
    let bound = PaletteBound(pr.read_u32()?);
    let tolerance = Tolerance(pr.read_u16()?);
    let entry_count = pr.read_u32()? as usize;
    if pr.bytes.len() != pr.pos + entry_count * 12 {
        return Err(ContainerError::SectionTruncated {
            tag: TAG_PALT,
            needed: entry_count * 12,
            available: pr.bytes.len() - pr.pos,
        });
    }
    let mut entries = Vec::new();
    for _ in 0..entry_count {
        entries.push(ColourEntry {
            linear_rgb: [pr.read_u32()?, pr.read_u32()?, pr.read_u32()?],
        });
    }
    // The inventory is rebuilt under its stored closure parameters; the
    // working space is the contractual one (AVC-C11).
    let inventory =
        crate::colour::ColourInventory::new(&entries, bound, tolerance, WorkingSpace::LinearRec709)
            .map_err(|_| {
                ContainerError::Malformed(
                    "colour inventory does not rebuild under its stored closure parameters",
                )
            })?;

    // ROWS → row inventory (every row must span the declared width).
    let mut rr = Reader {
        bytes: &rows,
        pos: 0,
    };
    let row_count = rr.read_u32()? as usize;
    let mut row_inventory = Vec::new();
    for _ in 0..row_count {
        let len = rr.read_u32()? as usize;
        if len != width as usize {
            return Err(ContainerError::Malformed(
                "row length differs from declared width",
            ));
        }
        let mut row = Vec::new();
        for _ in 0..len {
            row.push(ColourIndex(rr.read_u16()?));
        }
        row_inventory.push(row);
    }

    // DELT → delta registry (id must equal position: deltas are
    // addressed by index).
    let mut dr = Reader {
        bytes: &delt,
        pos: 0,
    };
    let delta_count = dr.read_u32()? as usize;
    let mut deltas = Vec::new();
    for id in 0..delta_count {
        let stored_id = dr.read_u32()?;
        if stored_id != id as u32 {
            return Err(ContainerError::Malformed(
                "delta id does not equal its position",
            ));
        }
        let from = ContentId(dr.read_u32()?);
        let to = ContentId(dr.read_u32()?);
        let change_count = dr.read_u32()? as usize;
        let mut changes = Vec::new();
        for _ in 0..change_count {
            changes.push(PixelChange {
                offset: dr.read_u32()?,
                to: ColourIndex(dr.read_u16()?),
            });
        }
        deltas.push(RowDelta {
            id: stored_id,
            from,
            to,
            changes,
        });
    }

    // EVNT → timeline (Timeline::push re-validates monotonicity and
    // checkpoint uniqueness).
    let mut er = Reader {
        bytes: &evnt,
        pos: 0,
    };
    let event_count = er.read_u32()? as usize;
    let mut timeline = Timeline::new();
    for id in 0..event_count {
        let stored_id = er.read_u32()?;
        if stored_id != id as u32 {
            return Err(ContainerError::Malformed(
                "event id does not equal its position",
            ));
        }
        let at = MediaClock::new(er.read_u64()?, er.read_u32()?)
            .map_err(|_| ContainerError::Malformed("event position has a zero timescale"))?;
        let instruction = decode_instruction(&mut er)?;
        timeline
            .push(EventId(stored_id), at, instruction)
            .map_err(|_| ContainerError::Malformed("timeline rejected the decoded events"))?;
    }

    // CHKP → checkpoints; digests are re-derived and compared.
    let mut cr = Reader {
        bytes: &chkp,
        pos: 0,
    };
    let checkpoint_count = cr.read_u32()? as usize;
    for _ in 0..checkpoint_count {
        let state = TemporalStateId(cr.read_u64()?);
        let at = MediaClock::new(cr.read_u64()?, cr.read_u32()?)
            .map_err(|_| ContainerError::Malformed("checkpoint position has a zero timescale"))?;
        let payload_len = cr.read_u32()? as usize;
        let payload = cr.take(payload_len, &TAG_CHKP)?.to_vec();
        let stored_digest: [u8; 32] = cr
            .take(32, &TAG_CHKP)?
            .try_into()
            .expect("take returned exactly 32 bytes");
        let rebuilt = Checkpoint::new(state, at, &payload);
        if rebuilt.digest != stored_digest {
            return Err(ContainerError::Malformed(
                "checkpoint digest does not match its payload",
            ));
        }
        timeline
            .push_checkpoint(rebuilt)
            .map_err(|_| ContainerError::Malformed("timeline rejected the decoded checkpoints"))?;
    }

    Ok(CompiledVideo {
        source_digest,
        inventory,
        row_inventory,
        deltas,
        timeline,
        dimensions,
        audio,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, len: usize, tag: &[u8; 4]) -> Result<&[u8], ContainerError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(ContainerError::Malformed("length overflows"))?;
        if end > self.bytes.len() {
            return Err(ContainerError::SectionTruncated {
                tag: *tag,
                needed: len,
                available: self.bytes.len() - self.pos,
            });
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, ContainerError> {
        Ok(self.take(1, b"READ")?[0])
    }

    fn read_u16(&mut self) -> Result<u16, ContainerError> {
        let b = self.take(2, b"READ")?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, ContainerError> {
        let b = self.take(4, b"READ")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, ContainerError> {
        let b = self.take(8, b"READ")?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
}

fn decode_target(r: &mut Reader<'_>) -> Result<Target, ContainerError> {
    match r.read_u8()? {
        0 => Ok(Target::Pixel {
            row: r.read_u32()?,
            pixel: r.read_u32()?,
        }),
        1 => Ok(Target::Row { row: r.read_u32()? }),
        2 => Ok(Target::Region {
            row: r.read_u32()?,
            pixel: r.read_u32()?,
            height: r.read_u32()?,
            width: r.read_u32()?,
        }),
        byte => Err(ContainerError::UnknownVariant {
            what: "target",
            byte,
        }),
    }
}

fn decode_exactness(r: &mut Reader<'_>) -> Result<Exactness, ContainerError> {
    match r.read_u8()? {
        0 => Ok(Exactness::Exact),
        1 => Ok(Exactness::Approximate(match r.read_u8()? {
            0 => ApproximationCause::TrimAssisted,
            1 => ApproximationCause::ToleranceBudget,
            2 => ApproximationCause::PaletteClosure,
            byte => {
                return Err(ContainerError::UnknownVariant {
                    what: "approximation cause",
                    byte,
                })
            }
        })),
        byte => Err(ContainerError::UnknownVariant {
            what: "exactness",
            byte,
        }),
    }
}

fn decode_pixel_state(r: &mut Reader<'_>) -> Result<PixelState, ContainerError> {
    let fill = ColourIndex(r.read_u16()?);
    let light = Light(r.read_u16()?);
    let exactness = decode_exactness(r)?;
    let mut trims = [None; MAX_TRIMS];
    for slot in &mut trims {
        if r.read_u8()? == 1 {
            *slot = Some(ColourIndex(r.read_u16()?));
        }
    }
    Ok(PixelState {
        fill,
        trims,
        light,
        exactness,
    })
}

fn decode_instruction(r: &mut Reader<'_>) -> Result<Instruction, ContainerError> {
    match r.read_u8()? {
        0 => Ok(Instruction::SetFill {
            target: decode_target(r)?,
            colour: ColourIndex(r.read_u16()?),
        }),
        1 => Ok(Instruction::SetTrim {
            target: decode_target(r)?,
            slot: r.read_u8()?,
            colour: ColourIndex(r.read_u16()?),
        }),
        2 => Ok(Instruction::SetLight {
            target: decode_target(r)?,
            light: Light(r.read_u16()?),
            exactness: decode_exactness(r)?,
        }),
        3 => Ok(Instruction::SetRun {
            row: r.read_u32()?,
            start: r.read_u32()?,
            len: r.read_u32()?,
            state: decode_pixel_state(r)?,
        }),
        4 => {
            let target = decode_target(r)?;
            let count = r.read_u32()? as usize;
            let mut states = Vec::new();
            for _ in 0..count {
                states.push(decode_pixel_state(r)?);
            }
            Ok(Instruction::SetRow { target, states })
        }
        5 => Ok(Instruction::SetRegion {
            region: decode_target(r)?,
            state: decode_pixel_state(r)?,
        }),
        6 => Ok(Instruction::CopyRegion {
            from: decode_target(r)?,
            to: decode_target(r)?,
        }),
        7 => Ok(Instruction::ApplyDelta {
            row: r.read_u32()?,
            delta_id: r.read_u32()?,
        }),
        8 => Ok(Instruction::LoadState {
            checkpoint_id: r.read_u32()?,
        }),
        9 => Ok(Instruction::Sync),
        10 => Ok(Instruction::Present),
        byte => Err(ContainerError::UnknownVariant {
            what: "instruction",
            byte,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::{PaletteBound, Tolerance};
    use crate::compile::compile;
    use crate::ingestion::{
        DecodedFrame, DecodedFrameFormat, IngestionParameters, SourceError, VideoSource,
    };
    use crate::profile::{CompilationBudgets, CompilationProfile, ProfileKind};

    const BG: [u8; 3] = [240, 240, 240];

    /// A 3-frame recording whose row 1 dims (exact half-light) and
    /// returns; with a 1-tick checkpoint interval this exercises
    /// checkpoints *and* light events in one round trip.
    struct DimmingRecording;

    impl VideoSource for DimmingRecording {
        fn source_digest(&self) -> String {
            "dimming-8x4-3".to_string()
        }
        fn dimensions(&self) -> (u32, u32) {
            (8, 4)
        }
        fn frame_rate(&self) -> (u32, u32) {
            (30, 1)
        }
        fn frame_count(&self) -> u32 {
            3
        }
        fn decode_frame(
            &self,
            i: u32,
            p: &IngestionParameters,
        ) -> Result<DecodedFrame, SourceError> {
            let fd = crate::clock::MediaClock::frame_duration(30, 1, p.ticks_per_second).unwrap();
            const GRAY_FULL: [u8; 3] = [200, 200, 200];
            const GRAY_DIM: [u8; 3] = [146, 146, 146];
            let mut data = Vec::with_capacity(96);
            let row = match i {
                0 | 2 => GRAY_FULL,
                1 => GRAY_DIM,
                _ => {
                    return Err(SourceError::DecodeFailed(format!(
                        "frame {i} out of bounds"
                    )))
                }
            };
            for y in 0..4usize {
                for _ in 0..8usize {
                    data.extend_from_slice(if y == 1 { &row } else { &BG });
                }
            }
            Ok(DecodedFrame {
                format: DecodedFrameFormat::Rgb8,
                width: 8,
                height: 4,
                at: crate::clock::MediaClock::new(fd.ticks() * i as u64, p.ticks_per_second)
                    .unwrap(),
                data,
            })
        }
    }

    fn compile_dimming() -> CompiledVideo {
        let params = IngestionParameters {
            working_space: WorkingSpace::LinearRec709,
            ticks_per_second: 30,
        }
        .bind()
        .unwrap();
        let profile = CompilationProfile {
            kind: ProfileKind::Desktop,
            budgets: CompilationBudgets {
                palette_bound: PaletteBound(16),
                tolerance: Tolerance::ZERO,
                max_checkpoint_interval: crate::clock::MediaClock::new(1, 30).unwrap(),
                max_delta_depth: 8,
                residency_budget_bytes: 1 << 20,
            },
        };
        compile(&DimmingRecording, &params, &profile).unwrap().0
    }

    #[test]
    fn round_trip_is_exact_and_idempotent() {
        let video = compile_dimming();
        let bytes = encode_container(&video);
        let decoded = decode_container(&bytes).unwrap();
        assert_eq!(decoded, video, "decode(encode(v)) must equal v");
        assert_eq!(
            encode_container(&decoded),
            bytes,
            "encode is idempotent: byte-identical repeat builds"
        );
    }

    #[test]
    fn round_trip_preserves_checkpoints_light_and_playback() {
        let video = compile_dimming();
        assert!(
            video.timeline.checkpoints().len() >= 2,
            "fixture must exercise checkpoints"
        );
        assert!(
            video
                .timeline
                .events()
                .iter()
                .any(|e| matches!(&e.instruction, Instruction::SetLight { .. })),
            "fixture must exercise a light event"
        );
        let decoded = decode_container(&encode_container(&video)).unwrap();
        let frames = crate::runtime::play(&decoded).unwrap();
        assert_eq!(frames.len(), 3);
        assert!(frames.iter().all(|f| f.exact));
        let dim_entry = crate::ingestion::srgb8_to_colour_entry([146, 146, 146]);
        for x in 0..8usize {
            assert_eq!(
                frames[1].pixels[8 + x],
                crate::colour::scale_entry(&dim_entry, Light::UNITY)
            );
        }
        // Checkpointed seek on the decoded video still works.
        let from_mid =
            crate::runtime::play_from(&decoded, crate::clock::MediaClock::new(2, 30).unwrap())
                .unwrap();
        assert_eq!(frames.last(), from_mid.last());
    }

    #[test]
    fn malformed_containers_are_rejected_safely() {
        let video = compile_dimming();
        let bytes = encode_container(&video);

        // Bad magic.
        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert_eq!(decode_container(&bad_magic), Err(ContainerError::BadMagic));

        // Newer version: rejected by negotiation, not misread.
        let mut bad_version = bytes.clone();
        bad_version[4] = 2;
        assert_eq!(
            decode_container(&bad_version),
            Err(ContainerError::UnsupportedVersion {
                found: 2,
                supported: VERSION
            })
        );

        // Every truncation is an Err, never a panic.
        for len in 0..bytes.len() {
            assert!(
                decode_container(&bytes[..len]).is_err(),
                "prefix of {len} bytes decoded successfully"
            );
        }

        // A flipped byte inside a section tag is rejected (unknown or
        // duplicate) — never misinterpreted.
        let tag_offset = 4 + 2 + 2 + 4 + video.source_digest.len() + 8;
        let mut bad_tag = bytes.clone();
        bad_tag[tag_offset] = b'1';
        assert!(decode_container(&bad_tag).is_err());

        // A tampered checkpoint payload fails the re-derived digest.
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xFF;
        assert!(decode_container(&tampered).is_err());

        // An impossible declared count cannot drive an allocation or a
        // panic: the exact-length check rejects it. The count field sits
        // after magic(4) version(2) flags(2) digest(4+len) dims(8),
        // section header(8), bound(4), tolerance(2).
        let mut huge_count = bytes.clone();
        let palt_count_offset = 4 + 2 + 2 + 4 + video.source_digest.len() + 8 + 8 + 4 + 2;
        huge_count[palt_count_offset..palt_count_offset + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_container(&huge_count).is_err());
    }

    #[test]
    fn zero_timescales_are_rejected_at_the_container_boundary() {
        // A timeline event whose tps is 0 cannot exist (F5): the decode
        // must reject, not build an invalid clock.
        let video = compile_dimming();
        let bytes = encode_container(&video);
        // Locate the first event's tps field: header (4+2+2), digest
        // (4+len), dims (8), then sections PALT (8 + 4+2+4+12n), ROWS
        // (8 + ...), DELT (8 + ...), EVNT header (8 + 4) + id (4) +
        // ticks (8).
        let n = video.inventory.entries().len();
        let rows_bytes: usize = 4 + video
            .row_inventory
            .iter()
            .map(|r| 4 + r.len() * 2)
            .sum::<usize>();
        let delt_bytes: usize = 4 + video
            .deltas
            .iter()
            .map(|d| 16 + d.changes.len() * 6)
            .sum::<usize>();
        let evnt_offset = 4
            + 2
            + 2
            + 4
            + video.source_digest.len()
            + 8
            + 8
            + (4 + 2 + 4 + n * 12)
            + 8
            + rows_bytes
            + 8
            + delt_bytes
            + 8
            + 4
            + 4
            + 8;
        let mut zero_tps = bytes.clone();
        zero_tps[evnt_offset..evnt_offset + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            decode_container(&zero_tps),
            Err(ContainerError::Malformed(_))
        ));
    }

    #[test]
    fn audio_sidecar_round_trips_verbatim() {
        let mut video = compile_dimming();
        video.audio = Some(AudioSidecar {
            codec: "aac".to_string(),
            duration_ts: 1_024,
            time_base: (1, 48_000),
            start_ts: -1_024, // edit-list shift: negative, still integer (F5)
        });
        let bytes = encode_container(&video);
        let decoded = decode_container(&bytes).unwrap();
        assert_eq!(decoded, video);
        let audio = decoded
            .audio
            .as_ref()
            .expect("descriptor survives the round trip");
        assert_eq!(audio.codec, "aac");
        assert_eq!(audio.duration_ts, 1_024);
        assert_eq!(audio.time_base, (1, 48_000));
        assert_eq!(audio.start_ts, -1_024);
        assert_eq!(encode_container(&decoded), bytes, "idempotent");
    }

    #[test]
    fn silent_container_has_no_auds_section() {
        let video = compile_dimming();
        assert!(video.audio.is_none());
        let bytes = encode_container(&video);
        // The tag must not appear even inside section bodies: AUDS is
        // absent, not empty (AVC-C07 — no descriptor, no section).
        assert!(!bytes.windows(4).any(|w| w == TAG_AUDS));
        let decoded = decode_container(&bytes).unwrap();
        assert!(decoded.audio.is_none());
    }

    #[test]
    fn malformed_audio_descriptors_are_rejected() {
        let mut video = compile_dimming();
        video.audio = Some(AudioSidecar {
            codec: "opus".to_string(),
            duration_ts: 960,
            time_base: (1, 48_000),
            start_ts: 0,
        });
        let bytes = encode_container(&video);
        // Locate the time-base denominator: AUDS is the final section
        // (codec len 4 + "opus" 4 + duration 8 + num 4) and den is
        // followed only by start_ts (8).
        let den_offset = bytes.len() - 8 - 4;
        let mut zero_den = bytes.clone();
        zero_den[den_offset..den_offset + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            decode_container(&zero_den),
            Err(ContainerError::Malformed(
                "audio time base denominator is zero"
            ))
        ));

        // A trailing byte inside the section is rejected: the descriptor
        // is fixed-shape, so appending anything is malformed.
        let mut trailing = bytes.clone();
        let digest_start = trailing.len() - 32;
        trailing.insert(digest_start, 0);
        // The AUDS section length no longer matches its payload layout;
        // the digest region now shifts — decode must reject, not panic.
        assert!(decode_container(&trailing).is_err());
    }
}
