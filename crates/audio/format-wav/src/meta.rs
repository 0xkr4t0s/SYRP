// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Metadata chunks: `bext` (EBU Tech 3285), `cue ` with `LIST`/`adtl` labels, `smpl` loops,
//! `LIST`/`INFO` text, and the chunks kept byte for byte.

use syrp_format_api::{
    Bext, CuePoint, LoopKind, Metadata, MetadataItem, ReadSeek, SampleLoop, Tag,
};

use crate::WavError;
use crate::riff::{Chunk, read_payload};

/// Chunks JUCE's `WavAudioFormat` documentation names that the contract has no typed item
/// for; they become [`MetadataItem::Other`] unchanged.
pub(crate) const KEPT: [&[u8; 4]; 6] = [b"acid", b"inst", b"iXML", b"axml", b"Trkn", b"cart"];

/// The `INFO` ids with a typed [`Tag`]; any other id becomes [`Tag::Custom`].
pub(crate) const INFO_TAGS: [(&[u8; 4], Tag); 9] = [
    (b"INAM", Tag::Title),
    (b"IART", Tag::Artist),
    (b"IPRD", Tag::Album),
    (b"IGNR", Tag::Genre),
    (b"ICRD", Tag::Date),
    (b"ICMT", Tag::Comment),
    (b"ITRK", Tag::TrackNumber),
    (b"ICOP", Tag::Copyright),
    (b"ISFT", Tag::Encoder),
];

/// `bext` field widths in order: description, originator, reference, date, time.
pub(crate) const BEXT_TEXT: [usize; 5] = [256, 32, 32, 10, 8];
/// Offset of the time reference (low then high 32 bits).
pub(crate) const BEXT_TIME: usize = 338;
/// Version, UMID, loudness and reserved bytes: no typed field holds them.
pub(crate) const BEXT_EXTRA: std::ops::Range<usize> = 346..602;
/// Offset of the coding history, the variable-length tail.
pub(crate) const BEXT_HISTORY: usize = 602;

const SMPL_HEADER: usize = 36;
const SMPL_LOOP: usize = 24;
const CUE_POINT: usize = 24;

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Text with trailing NULs dropped, read as UTF-8, or as Latin-1 when it is not UTF-8
/// (Syrp's choice).
pub(crate) fn text(b: &[u8]) -> String {
    let end = b.iter().rposition(|&c| c != 0).map_or(0, |i| i + 1);
    let b = &b[..end];
    match std::str::from_utf8(b) {
        Ok(s) => s.to_owned(),
        Err(_) => b.iter().map(|&c| char::from(c)).collect(),
    }
}

fn malformed(chunk: &[u8; 4]) -> WavError {
    WavError::BadMetadata { chunk: *chunk }
}

/// Splits a `LIST` body (after its 4-byte type) into `(id, payload)` sub-chunks.
fn sub_chunks(mut b: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    std::iter::from_fn(move || {
        if b.len() < 8 {
            return None;
        }
        let id = [b[0], b[1], b[2], b[3]];
        let size = u32_at(b, 4) as usize;
        let body = b.get(8..8 + size.min(b.len() - 8))?;
        b = b.get(8 + size + (size & 1)..).unwrap_or(&[]);
        Some((id, body))
    })
}

fn bext(b: &[u8]) -> Result<Bext, WavError> {
    if b.len() < BEXT_HISTORY {
        return Err(malformed(b"bext"));
    }
    let mut at = 0;
    let mut field = |width: usize| {
        at += width;
        text(&b[at - width..at])
    };
    let [d, o, r, date, tm] = BEXT_TEXT.map(&mut field);
    let time = u64::from(u32_at(b, BEXT_TIME)) | (u64::from(u32_at(b, BEXT_TIME + 4)) << 32);
    Ok(Bext::new()
        .with_description(d)
        .with_originator(o)
        .with_originator_reference(r)
        .with_origination_date(date)
        .with_origination_time(tm)
        .with_time_reference(time)
        .with_coding_history(text(&b[BEXT_HISTORY..])))
}

fn smpl(b: &[u8]) -> Result<Option<Vec<SampleLoop>>, WavError> {
    if b.len() < SMPL_HEADER {
        return Err(malformed(b"smpl"));
    }
    let count = u32_at(b, 28) as usize;
    if count > (b.len() - SMPL_HEADER) / SMPL_LOOP {
        return Err(malformed(b"smpl"));
    }
    let mut loops = Vec::new();
    loops
        .try_reserve_exact(count)
        .map_err(|_| WavError::OutOfMemory)?;
    for i in 0..count {
        let at = SMPL_HEADER + i * SMPL_LOOP;
        let kind = match u32_at(b, at + 4) {
            0 => LoopKind::Forward,
            1 => LoopKind::PingPong,
            2 => LoopKind::Backward,
            // The contract has no other kind: keep the whole chunk instead.
            _ => return Ok(None),
        };
        let (start, end) = (u32_at(b, at + 8), u32_at(b, at + 12));
        let lp = SampleLoop::new(kind, start.into(), end.into()).ok_or(malformed(b"smpl"))?;
        loops.push(lp.with_play_count(u32_at(b, at + 20)));
    }
    Ok(Some(loops))
}

fn cues(b: &[u8], labels: &[(u32, String)]) -> Result<Vec<CuePoint>, WavError> {
    if b.len() < 4 {
        return Err(malformed(b"cue "));
    }
    let count = u32_at(b, 0) as usize;
    if count > (b.len() - 4) / CUE_POINT {
        return Err(malformed(b"cue "));
    }
    let mut out = Vec::new();
    out.try_reserve_exact(count)
        .map_err(|_| WavError::OutOfMemory)?;
    for i in 0..count {
        let at = 4 + i * CUE_POINT;
        let id = u32_at(b, at);
        // The frame position is the sample offset (the last field) into the `data` chunk.
        let cue = CuePoint::new(id, u64::from(u32_at(b, at + 20)));
        out.push(match labels.iter().find(|(l, _)| *l == id) {
            Some((_, label)) => cue.with_label(label.clone()),
            None => cue,
        });
    }
    Ok(out)
}

/// Reads every metadata chunk in `chunks` (from the walk) into a [`Metadata`] in file order.
///
/// A metadata chunk that runs past the end of the input is skipped (Syrp's choice); one whose
/// own counts do not fit its size is `SYRP-FORMAT-0013`.
pub(crate) fn read(
    input: &mut dyn ReadSeek,
    chunks: &[Chunk],
    len: u64,
) -> Result<Metadata, WavError> {
    let wanted = |c: &&Chunk| {
        c.size <= len - c.offset
            && (matches!(&c.id, b"bext" | b"cue " | b"smpl" | b"LIST") || KEPT.contains(&&c.id))
    };
    let mut bodies = Vec::new();
    for c in chunks.iter().filter(wanted) {
        bodies.try_reserve(1).map_err(|_| WavError::OutOfMemory)?;
        bodies.push((c.id, read_payload(input, c.offset, c.size)?));
    }
    // Labels first: an `adtl` list may follow the `cue ` chunk it labels. `labl` wins over
    // `note` for the same cue (Syrp's choice).
    let mut labels: Vec<(u32, String)> = Vec::new();
    for (_, b) in bodies
        .iter()
        .filter(|(id, b)| id == b"LIST" && b.starts_with(b"adtl"))
    {
        for (id, body) in sub_chunks(&b[4..]) {
            if body.len() < 4 || !(id == *b"labl" || id == *b"note") {
                continue;
            }
            let cue = u32_at(body, 0);
            let have = labels.iter().position(|(c, _)| *c == cue);
            match have {
                Some(i) if id == *b"labl" => labels[i].1 = text(&body[4..]),
                Some(_) => {}
                None => labels.push((cue, text(&body[4..]))),
            }
        }
    }
    let mut meta = Metadata::new();
    for (id, b) in bodies {
        match &id {
            b"bext" => {
                meta.push(MetadataItem::Bext(bext(&b)?));
                if b[BEXT_EXTRA].iter().any(|&x| x != 0) {
                    meta.push(MetadataItem::Other("bext".into(), b));
                }
            }
            b"cue " => cues(&b, &labels)?
                .into_iter()
                .for_each(|c| meta.push(MetadataItem::Cue(c))),
            b"smpl" => match smpl(&b)? {
                Some(loops) => loops
                    .into_iter()
                    .for_each(|l| meta.push(MetadataItem::Loop(l))),
                None => meta.push(MetadataItem::Other("smpl".into(), b)),
            },
            b"LIST" if b.starts_with(b"INFO") => {
                for (tag_id, body) in sub_chunks(&b[4..]) {
                    let tag = INFO_TAGS
                        .iter()
                        .find(|(t, _)| **t == tag_id)
                        .map_or_else(|| Tag::Custom(text(&tag_id)), |(_, t)| t.clone());
                    meta.push(MetadataItem::Tag(tag, text(body)));
                }
            }
            b"LIST" => {}
            other => meta.push(MetadataItem::Other(text(other), b)),
        }
    }
    Ok(meta)
}
