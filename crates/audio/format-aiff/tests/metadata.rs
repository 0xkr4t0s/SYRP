// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B3: metadata chunks into the contract's model, in file order.

mod common;

use common::{chunk, comm, form, open, open_code, pstring, ssnd};
use syrp_format_api::{CuePoint, LoopKind, MetadataItem, SampleLoop, Tag};

/// A `MARK` body from (id, position, name).
fn mark(markers: &[(i16, u32, &[u8])]) -> Vec<u8> {
    let mut b = u16::try_from(markers.len()).unwrap().to_be_bytes().to_vec();
    for (id, pos, name) in markers {
        b.extend_from_slice(&id.to_be_bytes());
        b.extend_from_slice(&pos.to_be_bytes());
        b.extend_from_slice(&pstring(name));
    }
    b
}

/// An `INST` body: note fields 60, -5, 0..127, 1..127, gain -3, then two loops.
fn inst(sustain: [i16; 3], release: [i16; 3]) -> Vec<u8> {
    let mut b = vec![60, 0xFB, 0, 127, 1, 127];
    b.extend_from_slice(&(-3i16).to_be_bytes());
    for v in sustain.iter().chain(&release) {
        b.extend_from_slice(&v.to_be_bytes());
    }
    b
}

fn with(chunks: &[Vec<u8>]) -> Vec<MetadataItem> {
    let mut all = vec![chunk(b"COMM", &comm(1, 0, 16, 44_100.0))];
    all.extend_from_slice(chunks);
    all.push(chunk(b"SSND", &ssnd(0, 0, &[])));
    open(form(b"AIFF", &all))
        .unwrap()
        .metadata()
        .items()
        .to_vec()
}

fn lp(kind: LoopKind, start: u64, end: u64) -> MetadataItem {
    MetadataItem::Loop(SampleLoop::new(kind, start, end).unwrap())
}

#[test]
fn b3_markers_become_cue_points() {
    let items = with(&[chunk(
        b"MARK",
        &mark(&[(1, 10, b"intro"), (7, 4_800, b"go"), (3, 0, b"")]),
    )]);
    assert_eq!(
        items,
        [
            MetadataItem::Cue(CuePoint::new(1, 10).with_label("intro")),
            MetadataItem::Cue(CuePoint::new(7, 4_800).with_label("go")),
            MetadataItem::Cue(CuePoint::new(3, 0)),
        ]
    );
}

#[test]
fn b3_inst_loops_resolve_markers_and_keep_the_raw_chunk() {
    let raw = inst([1, 1, 2], [2, 2, 3]);
    // INST before MARK: loops still resolve against markers found later.
    let items = with(&[
        chunk(b"INST", &raw),
        chunk(
            b"MARK",
            &mark(&[(1, 100, b"a"), (2, 200, b"b"), (3, 300, b"c")]),
        ),
    ]);
    assert_eq!(items[0], lp(LoopKind::Forward, 100, 200));
    assert_eq!(items[1], lp(LoopKind::PingPong, 200, 300));
    assert_eq!(items[2], MetadataItem::Other("INST".into(), raw));
    assert_eq!(items.len(), 6, "two loops, the raw INST, three cues");
}

#[test]
fn b3_no_looping_adds_nothing() {
    let raw = inst([0, 1, 2], [0, 0, 0]);
    let items = with(&[
        chunk(b"MARK", &mark(&[(1, 1, b""), (2, 2, b"")])),
        chunk(b"INST", &raw),
    ]);
    assert_eq!(items.len(), 3);
    assert_eq!(items[2], MetadataItem::Other("INST".into(), raw));
}

#[test]
fn b3_unresolvable_loops_are_dropped_syrp_choice() {
    // Unknown play mode, a missing marker, and an end before its start.
    for loops in [[3, 1, 2], [1, 1, 9], [1, 2, 1]] {
        let raw = inst(loops, [0; 3]);
        let items = with(&[
            chunk(b"MARK", &mark(&[(1, 10, b""), (2, 20, b"")])),
            chunk(b"INST", &raw),
        ]);
        assert!(
            !items.iter().any(|i| matches!(i, MetadataItem::Loop(_))),
            "{loops:?}"
        );
        assert_eq!(items.last(), Some(&MetadataItem::Other("INST".into(), raw)));
    }
}

#[test]
fn b3_text_chunks_become_tags_in_order() {
    let items = with(&[
        chunk(b"ANNO", b"first"),
        chunk(b"NAME", b"Take 3"),
        chunk(b"AUTH", b"Someone"),
        chunk(b"(c) ", b"2026 Someone\0"),
        chunk(b"ANNO", b"second"),
        chunk(b"NAME", b"caf\xe9"),
    ]);
    let tag = |t: Tag, s: &str| MetadataItem::Tag(t, s.into());
    assert_eq!(
        items,
        [
            tag(Tag::Comment, "first"),
            tag(Tag::Title, "Take 3"),
            tag(Tag::Artist, "Someone"),
            tag(Tag::Copyright, "2026 Someone"),
            tag(Tag::Comment, "second"),
            // Not UTF-8: one character per byte (Syrp's choice).
            tag(Tag::Title, "caf\u{e9}"),
        ]
    );
}

#[test]
fn b3_other_chunks_are_kept_raw() {
    let items = with(&[
        chunk(b"COMT", &[0, 0]),
        chunk(b"APPL", b"stoc\x01"),
        chunk(b"ID3 ", b"ID3\x04"),
    ]);
    let other = |id: &str, b: &[u8]| MetadataItem::Other(id.into(), b.to_vec());
    assert_eq!(
        items,
        [
            other("COMT", &[0, 0]),
            other("APPL", b"stoc\x01"),
            other("ID3 ", b"ID3\x04")
        ]
    );
}

#[test]
fn b3_malformed_metadata_is_a_coded_error() {
    let bad = "SYRP-FORMAT-0002";
    let file = |c: Vec<u8>| form(b"AIFF", &[chunk(b"COMM", &comm(1, 0, 16, 44_100.0)), c]);
    assert_eq!(open_code(file(chunk(b"INST", &[0; 19]))), bad);
    assert_eq!(open_code(file(chunk(b"MARK", &[0]))), bad);
    // A count far larger than the chunk can hold.
    assert_eq!(
        open_code(file(chunk(b"MARK", &[0xFF, 0xFF, 0, 1, 0, 0, 0, 0, 0, 0]))),
        bad
    );
    let mut long_name = mark(&[(1, 0, b"abc")]);
    long_name[8] = 9;
    assert_eq!(open_code(file(chunk(b"MARK", &long_name))), bad);
    assert_eq!(open_code(file(chunk(b"MARK", &mark(&[(0, 5, b"")])))), bad);
    assert_eq!(open_code(file(chunk(b"MARK", &mark(&[(-4, 5, b"")])))), bad);
}
