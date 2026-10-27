// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! `VORBIS_COMMENT` and `CUESHEET` bodies to tags and cue points (RFC 9639 sections 8.6 and 8.7).

use syrp_format_api::{CuePoint, FormatError, Metadata, MetadataItem, Tag};

use crate::meta::{Bytes, invalid};

/// Maps a Vorbis comment field name to a tag, ignoring ASCII case.
fn tag_for(name: &str) -> Tag {
    const KNOWN: [(&str, Tag); 10] = [
        ("TITLE", Tag::Title),
        ("ARTIST", Tag::Artist),
        ("ALBUM", Tag::Album),
        ("GENRE", Tag::Genre),
        ("DATE", Tag::Date),
        ("COMMENT", Tag::Comment),
        ("DESCRIPTION", Tag::Comment),
        ("TRACKNUMBER", Tag::TrackNumber),
        ("COPYRIGHT", Tag::Copyright),
        ("ENCODER", Tag::Encoder),
    ];
    KNOWN
        .into_iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map_or_else(|| Tag::Custom(name.to_owned()), |(_, tag)| tag)
}

/// Parses a `VORBIS_COMMENT` body (little-endian lengths) into tags, in field order. The
/// vendor string is checked but not reported.
pub(crate) fn parse_vorbis(b: &[u8], meta: &mut Metadata) -> Result<(), FormatError> {
    let mut cur = Bytes(b);
    let vendor_len = cur.u32_le()? as usize;
    std::str::from_utf8(cur.take(vendor_len)?).map_err(|_| invalid("vendor is not UTF-8"))?;
    let count = cur.u32_le()?;
    for _ in 0..count {
        let len = cur.u32_le()? as usize;
        let field = std::str::from_utf8(cur.take(len)?)
            .map_err(|_| invalid("a Vorbis comment is not UTF-8"))?;
        let (name, value) = field
            .split_once('=')
            .ok_or_else(|| invalid("a Vorbis comment has no '='"))?;
        if name.is_empty() || !name.bytes().all(|c| (0x20..=0x7D).contains(&c)) {
            return Err(invalid(
                "a Vorbis comment field name is not printable ASCII",
            ));
        }
        meta.push(MetadataItem::Tag(tag_for(name), value.to_owned()));
    }
    cur.finish()
}

/// Parses a `CUESHEET` body: one cue point per track index, at the track offset plus the
/// index offset. The id is `track_number << 8 | index_number` (Syrp's choice).
pub(crate) fn parse_cuesheet(b: &[u8], meta: &mut Metadata) -> Result<(), FormatError> {
    let mut cur = Bytes(b);
    // Media catalog number (128), lead-in samples (8), CD flag and reserved bits (259).
    cur.take(128 + 8 + 259)?;
    let tracks = cur.u8()?;
    for _ in 0..tracks {
        let offset = cur.u64_be()?;
        let number = cur.u8()?;
        // ISRC (12), type and pre-emphasis flags with reserved bits (14).
        cur.take(12 + 14)?;
        let indices = cur.u8()?;
        for _ in 0..indices {
            let at = offset
                .checked_add(cur.u64_be()?)
                .ok_or_else(|| invalid("CUESHEET index offset overflows"))?;
            let index = cur.u8()?;
            cur.take(3)?;
            let id = u32::from(number) << 8 | u32::from(index);
            meta.push(MetadataItem::Cue(CuePoint::new(id, at)));
        }
    }
    cur.finish()
}

/// Reports an `APPLICATION` or `PICTURE` block as `Other` with the block's name and whole body.
pub(crate) fn push_other(
    application: bool,
    body: Vec<u8>,
    meta: &mut Metadata,
) -> Result<(), FormatError> {
    if application && body.len() < 4 {
        return Err(invalid("APPLICATION block without an identifier"));
    }
    let name = if application {
        "APPLICATION"
    } else {
        "PICTURE"
    };
    meta.push(MetadataItem::Other(name.into(), body));
    Ok(())
}
