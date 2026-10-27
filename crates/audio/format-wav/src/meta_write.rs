// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Encodes the contract's `Metadata` as WAV chunks, the inverse of `meta::read`.

use syrp_format_api::{Bext, FormatError, LoopKind, Metadata, MetadataItem, Tag};

use crate::meta::{BEXT_EXTRA, BEXT_HISTORY, BEXT_TEXT, BEXT_TIME, INFO_TAGS};

/// Chunk ids the writer owns; `Other` items with these ids are skipped (Syrp's choice).
const RESERVED: [&str; 6] = ["RIFF", "RF64", "BW64", "ds64", "fmt ", "data"];

/// MIDI unity note written in `smpl` (middle C; Syrp's choice, the contract has no field).
const UNITY_NOTE: u32 = 60;

/// Appends chunk `id` with `body` and its pad byte.
pub(crate) fn push_chunk(out: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) -> Result<(), FormatError> {
    let size = u32::try_from(body.len())
        .map_err(|_| FormatError::unsupported("a metadata chunk larger than 4 GiB"))?;
    out.extend_from_slice(id);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
    Ok(())
}

fn four_cc(id: &str) -> Option<[u8; 4]> {
    <[u8; 4]>::try_from(id.as_bytes()).ok()
}

fn u32_of(v: u64, what: &str) -> Result<u32, FormatError> {
    u32::try_from(v).map_err(|_| FormatError::unsupported(format!("{what} beyond 2^32 frames")))
}

/// `s` cut to at most `width` bytes on a character boundary, then NUL-padded.
fn fixed(out: &mut Vec<u8>, s: &str, width: usize) {
    let mut end = s.len().min(width);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    out.extend_from_slice(&s.as_bytes()[..end]);
    out.resize(out.len() + width - end, 0);
}

fn bext(b: &Bext, extra: Option<&[u8]>) -> Vec<u8> {
    let mut v = Vec::with_capacity(BEXT_HISTORY + b.coding_history().len());
    let texts = [
        b.description(),
        b.originator(),
        b.originator_reference(),
        b.origination_date(),
        b.origination_time(),
    ];
    for (s, width) in texts.into_iter().zip(BEXT_TEXT) {
        fixed(&mut v, s, width);
    }
    debug_assert_eq!(v.len(), BEXT_TIME);
    let time = b.time_reference();
    v.extend_from_slice(&(time as u32).to_le_bytes());
    v.extend_from_slice(&((time >> 32) as u32).to_le_bytes());
    match extra {
        Some(raw) => v.extend_from_slice(&raw[BEXT_EXTRA]),
        None => v.resize(BEXT_HISTORY, 0),
    }
    v.extend_from_slice(b.coding_history().as_bytes());
    v
}

fn info_id(tag: &Tag) -> Option<[u8; 4]> {
    match tag {
        Tag::Custom(id) => four_cc(id),
        t => INFO_TAGS.iter().find(|(_, k)| k == t).map(|(id, _)| **id),
    }
}

/// All chunks for `meta`, in item order. Cue points, loops and tags each become one chunk
/// (plus a `LIST`/`adtl` for cue labels) at the place of their first item; a raw
/// `Other("bext")` from a reader supplies the version, UMID and loudness bytes of the `Bext`
/// item instead of being written twice.
pub(crate) fn encode(meta: &Metadata, sample_rate: u32) -> Result<Vec<u8>, FormatError> {
    let items = meta.items();
    let has_bext = meta.bext().is_some();
    let raw_bext = items.iter().find_map(|i| match i {
        MetadataItem::Other(id, raw) if id == "bext" && raw.len() >= BEXT_HISTORY => {
            Some(raw.as_slice())
        }
        _ => None,
    });
    let (mut cues_done, mut loops_done, mut tags_done) = (false, false, false);
    let mut out = Vec::new();
    for item in items {
        match item {
            MetadataItem::Bext(b) => push_chunk(&mut out, b"bext", &bext(b, raw_bext))?,
            MetadataItem::Cue(_) if !cues_done => {
                cues_done = true;
                cue_chunks(&mut out, items)?;
            }
            MetadataItem::Loop(_) if !loops_done => {
                loops_done = true;
                smpl_chunk(&mut out, items, sample_rate)?;
            }
            MetadataItem::Tag(..) if !tags_done => {
                tags_done = true;
                let mut list = b"INFO".to_vec();
                for i in items {
                    if let MetadataItem::Tag(tag, value) = i
                        && let Some(id) = info_id(tag)
                    {
                        let mut text = value.as_bytes().to_vec();
                        text.push(0);
                        push_chunk(&mut list, &id, &text)?;
                    }
                }
                push_chunk(&mut out, b"LIST", &list)?;
            }
            MetadataItem::Other(id, _) if id == "bext" && has_bext => {}
            MetadataItem::Other(id, bytes) => {
                if let Some(id) = four_cc(id).filter(|_| !RESERVED.contains(&id.as_str())) {
                    push_chunk(&mut out, &id, bytes)?;
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn cue_chunks(out: &mut Vec<u8>, items: &[MetadataItem]) -> Result<(), FormatError> {
    let cues: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            MetadataItem::Cue(c) => Some(c),
            _ => None,
        })
        .collect();
    let mut body = u32_of(cues.len() as u64, "cue count")?
        .to_le_bytes()
        .to_vec();
    let mut adtl = b"adtl".to_vec();
    for c in &cues {
        let at = u32_of(c.position(), "a cue position")?;
        // id, play position, data chunk id, chunk start, block start, sample offset.
        body.extend_from_slice(&c.id().to_le_bytes());
        body.extend_from_slice(&at.to_le_bytes());
        body.extend_from_slice(b"data");
        body.extend_from_slice(&[0; 8]);
        body.extend_from_slice(&at.to_le_bytes());
        if let Some(label) = c.label() {
            let mut text = c.id().to_le_bytes().to_vec();
            text.extend_from_slice(label.as_bytes());
            text.push(0);
            push_chunk(&mut adtl, b"labl", &text)?;
        }
    }
    push_chunk(out, b"cue ", &body)?;
    if adtl.len() > 4 {
        push_chunk(out, b"LIST", &adtl)?;
    }
    Ok(())
}

fn smpl_chunk(out: &mut Vec<u8>, items: &[MetadataItem], rate: u32) -> Result<(), FormatError> {
    let loops: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            MetadataItem::Loop(l) => Some(l),
            _ => None,
        })
        .collect();
    let period_ns = 1_000_000_000 / rate;
    let count = u32_of(loops.len() as u64, "loop count")?;
    let mut body = Vec::new();
    // manufacturer, product, sample period, unity note, pitch fraction, SMPTE format and
    // offset, loop count, sampler data size.
    for v in [0, 0, period_ns, UNITY_NOTE, 0, 0, 0, count, 0] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    for (i, l) in loops.iter().enumerate() {
        let kind = match l.kind() {
            LoopKind::Forward => 0,
            LoopKind::PingPong => 1,
            LoopKind::Backward => 2,
            _ => return Err(FormatError::unsupported("a loop kind WAV cannot store")),
        };
        let start = u32_of(l.start(), "a loop start")?;
        let end = u32_of(l.end(), "a loop end")?;
        for v in [i as u32, kind, start, end, 0, l.play_count()] {
            body.extend_from_slice(&v.to_le_bytes());
        }
    }
    push_chunk(out, b"smpl", &body)
}
