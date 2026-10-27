// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Metadata chunks (`MARK`, `INST`, text chunks and the rest) into the contract's model.

use syrp_format_api::{CuePoint, FormatError, LoopKind, Metadata, MetadataItem, SampleLoop, Tag};

/// Size of an `INST` chunk body in the AIFF 1.3 layout.
pub(crate) const INST_LEN: usize = 20;

/// Text from a file: UTF-8 when it is valid, otherwise one character per byte (ISO 8859-1).
/// Syrp's choice: AIFF only promises ASCII.
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// A chunk identifier as text, decoded like [`decode_text`].
pub(crate) fn fourcc_name(id: [u8; 4]) -> String {
    decode_text(&id)
}

fn bad(reason: &str) -> FormatError {
    FormatError::invalid_data(format!("AIFF: {reason}"))
}

/// One entry in file order; `INST` loops wait until every marker is known.
enum Slot {
    Item(MetadataItem),
    Inst(Vec<u8>),
}

/// Collects metadata chunks in file order.
#[derive(Default)]
pub(crate) struct MetaBuilder {
    slots: Vec<Slot>,
    /// (marker id, position) for every marker seen, for `INST` loop lookups.
    markers: Vec<(i16, u32)>,
}

impl MetaBuilder {
    /// Takes one chunk that is neither `COMM`, `SSND` nor `FVER`.
    pub(crate) fn chunk(&mut self, id: [u8; 4], body: Vec<u8>) -> Result<(), FormatError> {
        let tag = match &id {
            b"MARK" => return self.mark(&body),
            b"INST" => {
                if body.len() < INST_LEN {
                    return Err(bad("INST chunk is too short"));
                }
                self.slots.push(Slot::Inst(body));
                return Ok(());
            }
            b"NAME" => Tag::Title,
            b"AUTH" => Tag::Artist,
            b"(c) " => Tag::Copyright,
            b"ANNO" => Tag::Comment,
            _ => {
                let item = MetadataItem::Other(fourcc_name(id), body);
                self.slots.push(Slot::Item(item));
                return Ok(());
            }
        };
        // Syrp's choice: trailing NUL bytes are padding, not text.
        let text = body
            .iter()
            .rposition(|&b| b != 0)
            .map_or(&[][..], |i| &body[..=i]);
        let item = MetadataItem::Tag(tag, decode_text(text));
        self.slots.push(Slot::Item(item));
        Ok(())
    }

    fn mark(&mut self, body: &[u8]) -> Result<(), FormatError> {
        if body.len() < 2 {
            return Err(bad("MARK chunk is too short"));
        }
        let count = usize::from(u16::from_be_bytes([body[0], body[1]]));
        // Each marker takes at least 8 bytes, so the count is bounded by the chunk.
        if count > (body.len() - 2) / 8 {
            return Err(bad("MARK count exceeds the chunk"));
        }
        self.markers
            .try_reserve_exact(count)
            .map_err(|_| bad("MARK count exceeds memory"))?;
        let mut at = 2;
        for _ in 0..count {
            let head = body
                .get(at..at + 7)
                .ok_or_else(|| bad("MARK marker runs past the chunk"))?;
            let id = i16::from_be_bytes([head[0], head[1]]);
            let position = u32::from_be_bytes([head[2], head[3], head[4], head[5]]);
            let len = usize::from(head[6]);
            let name = body
                .get(at + 7..at + 7 + len)
                .ok_or_else(|| bad("MARK marker name runs past the chunk"))?;
            if id <= 0 {
                return Err(bad("MARK marker id is not positive"));
            }
            // The count byte and the text are padded to an even length.
            at += 6 + (1 + len).next_multiple_of(2);
            let mut cue = CuePoint::new(id.unsigned_abs().into(), position.into());
            // Syrp's choice: an empty name means no label.
            if len > 0 {
                cue = cue.with_label(decode_text(name));
            }
            self.markers.push((id, position));
            self.slots.push(Slot::Item(MetadataItem::Cue(cue)));
        }
        Ok(())
    }

    /// Resolves `INST` loops against the markers and returns the items in file order.
    pub(crate) fn finish(self) -> Metadata {
        let Self { slots, markers } = self;
        let mut meta = Metadata::new();
        for slot in slots {
            match slot {
                Slot::Item(item) => meta.push(item),
                Slot::Inst(body) => {
                    for at in [8, 14] {
                        if let Some(lp) = resolve_loop(&markers, &body[at..at + 6]) {
                            meta.push(MetadataItem::Loop(lp));
                        }
                    }
                    meta.push(MetadataItem::Other("INST".into(), body));
                }
            }
        }
        meta
    }
}

/// One `INST` loop (play mode, begin marker, end marker). Syrp's choice: a loop with an
/// unknown play mode, a missing marker, or an end before its start adds nothing, and the
/// raw `INST` bytes still carry it.
fn resolve_loop(markers: &[(i16, u32)], raw: &[u8]) -> Option<SampleLoop> {
    let field = |i: usize| i16::from_be_bytes([raw[i], raw[i + 1]]);
    let kind = match field(0) {
        1 => LoopKind::Forward,
        2 => LoopKind::PingPong,
        _ => return None,
    };
    let find = |id: i16| markers.iter().find(|m| m.0 == id).map(|m| m.1);
    let (start, end) = (find(field(2))?, find(field(4))?);
    SampleLoop::new(kind, start.into(), end.into())
}
