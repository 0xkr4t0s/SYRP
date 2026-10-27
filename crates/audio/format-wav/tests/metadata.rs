// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B3: metadata chunks read into the contract's `Metadata`, in file order.

mod common;

use std::io::Cursor;

use common::{chunk, fmt, open, riff};
use syrp_error::Diagnostic;
use syrp_format_api::{Bext, CuePoint, LoopKind, MetadataItem, SampleLoop, Tag};
use syrp_format_wav::WavReader;

fn with(meta: Vec<Vec<u8>>) -> Vec<u8> {
    let mut chunks = vec![chunk(b"fmt ", &fmt(1, 1, 8000, 16))];
    chunks.extend(meta);
    chunks.push(chunk(b"data", &[0; 2]));
    riff(&chunks)
}

fn items(meta: Vec<Vec<u8>>) -> Vec<MetadataItem> {
    open(with(meta)).unwrap().metadata().items().to_vec()
}

fn code(meta: Vec<Vec<u8>>) -> String {
    let err = WavReader::open(Box::new(Cursor::new(with(meta)))).unwrap_err();
    err.code().as_str().to_owned()
}

fn padded(s: &[u8], width: usize) -> Vec<u8> {
    let mut v = s.to_vec();
    v.resize(width, 0);
    v
}

fn bext_body(extra: u8) -> Vec<u8> {
    let mut b = padded(b"desc", 256);
    b.extend(padded(b"orig", 32));
    b.extend(padded(b"ref", 32));
    b.extend_from_slice(b"2026-10-27");
    b.extend_from_slice(b"12:34:56");
    b.extend_from_slice(&0x0000_0001_0000_0002u64.to_le_bytes());
    b.extend(vec![extra; 256]);
    b.extend_from_slice(b"A=PCM,F=48000\r\n\0\0");
    b
}

fn list(kind: &[u8; 4], subs: &[Vec<u8>]) -> Vec<u8> {
    let mut b = kind.to_vec();
    b.extend(subs.concat());
    chunk(b"LIST", &b)
}

fn labelled(id: &[u8; 4], cue: u32, text: &[u8]) -> Vec<u8> {
    let mut b = cue.to_le_bytes().to_vec();
    b.extend_from_slice(text);
    chunk(id, &b)
}

fn cue_chunk(points: &[(u32, u32)]) -> Vec<u8> {
    let mut b = u32::try_from(points.len()).unwrap().to_le_bytes().to_vec();
    for &(id, at) in points {
        for v in [id, at] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(b"data");
        for v in [0, 0, at] {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    chunk(b"cue ", &b)
}

fn smpl_chunk(loops: &[(u32, u32, u32, u32)]) -> Vec<u8> {
    let mut b = vec![0; 28];
    b.extend_from_slice(&u32::try_from(loops.len()).unwrap().to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    for &(kind, start, end, count) in loops {
        for v in [7, kind, start, end, 0, count] {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    chunk(b"smpl", &b)
}

#[test]
fn b3_bext_fields_and_time_reference() {
    let got = items(vec![chunk(b"bext", &bext_body(0))]);
    let want = Bext::new()
        .with_description("desc")
        .with_originator("orig")
        .with_originator_reference("ref")
        .with_origination_date("2026-10-27")
        .with_origination_time("12:34:56")
        .with_time_reference(0x0000_0001_0000_0002)
        .with_coding_history("A=PCM,F=48000\r\n");
    assert_eq!(got, [MetadataItem::Bext(want)]);
}

#[test]
fn b3_bext_umid_and_loudness_travel_as_other_after_the_bext_item() {
    let body = bext_body(1);
    let got = items(vec![chunk(b"bext", &body)]);
    assert!(matches!(got[0], MetadataItem::Bext(_)));
    assert_eq!(got[1], MetadataItem::Other("bext".into(), body));
}

#[test]
fn b3_cue_points_take_labl_then_note_labels() {
    let adtl = list(
        b"adtl",
        &[
            labelled(b"note", 1, b"a note\0"),
            labelled(b"labl", 1, b"verse\0"),
            labelled(b"note", 2, b"only note\0"),
        ],
    );
    let got = items(vec![cue_chunk(&[(1, 100), (2, 200), (3, 300)]), adtl]);
    assert_eq!(
        got,
        [
            MetadataItem::Cue(CuePoint::new(1, 100).with_label("verse")),
            MetadataItem::Cue(CuePoint::new(2, 200).with_label("only note")),
            MetadataItem::Cue(CuePoint::new(3, 300)),
        ]
    );
}

#[test]
fn b3_smpl_loops_map_their_kinds_and_play_counts() {
    let got = items(vec![smpl_chunk(&[
        (0, 0, 9, 0),
        (1, 10, 19, 3),
        (2, 20, 29, 1),
    ])]);
    let lp = |k, s, e| SampleLoop::new(k, s, e).unwrap();
    assert_eq!(
        got,
        [
            MetadataItem::Loop(lp(LoopKind::Forward, 0, 9)),
            MetadataItem::Loop(lp(LoopKind::PingPong, 10, 19).with_play_count(3)),
            MetadataItem::Loop(lp(LoopKind::Backward, 20, 29).with_play_count(1)),
        ]
    );
}

#[test]
fn b3_smpl_with_another_loop_type_is_kept_whole_as_other_syrp_choice() {
    let c = smpl_chunk(&[(0, 0, 9, 0), (32, 1, 2, 0)]);
    let body = c[8..].to_vec();
    assert_eq!(items(vec![c]), [MetadataItem::Other("smpl".into(), body)]);
}

#[test]
fn b3_info_text_maps_to_tags_and_custom() {
    let ids: [(&[u8; 4], Tag); 10] = [
        (b"INAM", Tag::Title),
        (b"IART", Tag::Artist),
        (b"IPRD", Tag::Album),
        (b"IGNR", Tag::Genre),
        (b"ICRD", Tag::Date),
        (b"ICMT", Tag::Comment),
        (b"ITRK", Tag::TrackNumber),
        (b"ICOP", Tag::Copyright),
        (b"ISFT", Tag::Encoder),
        (b"IENG", Tag::Custom("IENG".into())),
    ];
    let subs: Vec<Vec<u8>> = ids.iter().map(|(id, _)| chunk(id, b"v\0")).collect();
    let got = items(vec![list(b"INFO", &subs)]);
    let want: Vec<MetadataItem> = ids
        .into_iter()
        .map(|(_, t)| MetadataItem::Tag(t, "v".into()))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn b3_text_drops_trailing_nuls_and_falls_back_to_latin1_syrp_choice() {
    let got = items(vec![list(
        b"INFO",
        &[
            chunk(b"INAM", "Caf\u{e9}\0\0".as_bytes()),
            chunk(b"IART", b"Caf\xe9\0"),
        ],
    )]);
    assert_eq!(
        got,
        [
            MetadataItem::Tag(Tag::Title, "Caf\u{e9}".into()),
            MetadataItem::Tag(Tag::Artist, "Caf\u{e9}".into()),
        ]
    );
}

#[test]
fn b3_named_chunks_are_kept_unchanged_in_file_order_and_others_skipped() {
    let ids: [&[u8; 4]; 6] = [b"acid", b"inst", b"iXML", b"axml", b"Trkn", b"cart"];
    let mut meta: Vec<Vec<u8>> = vec![chunk(b"JUNK", &[0; 6]), chunk(b"fact", &[1, 0, 0, 0])];
    meta.extend(ids.iter().map(|id| chunk(id, b"abc")));
    meta.push(list(b"INFO", &[chunk(b"INAM", b"t\0")]));
    let mut want: Vec<MetadataItem> = ids
        .iter()
        .map(|id| MetadataItem::Other(String::from_utf8(id.to_vec()).unwrap(), b"abc".to_vec()))
        .collect();
    want.push(MetadataItem::Tag(Tag::Title, "t".into()));
    assert_eq!(items(meta), want);
}

#[test]
fn b3_metadata_after_the_data_chunk_is_read() {
    let file = riff(&[
        chunk(b"fmt ", &fmt(1, 1, 8000, 16)),
        chunk(b"data", &[0; 2]),
        list(b"INFO", &[chunk(b"ICMT", b"late\0")]),
    ]);
    let r = open(file).unwrap();
    assert_eq!(r.metadata().tag(&Tag::Comment), Some("late"));
}

#[test]
fn b3_malformed_metadata_is_0013() {
    assert_eq!(code(vec![chunk(b"bext", &[0; 601])]), "SYRP-FORMAT-0013");
    assert_eq!(code(vec![chunk(b"smpl", &[0; 35])]), "SYRP-FORMAT-0013");
    let mut c = smpl_chunk(&[(0, 0, 9, 0)]);
    c[8 + 28] = 2; // two loops declared, one present
    assert_eq!(code(vec![c]), "SYRP-FORMAT-0013");
    assert_eq!(code(vec![smpl_chunk(&[(0, 9, 8, 0)])]), "SYRP-FORMAT-0013");
    let mut c = cue_chunk(&[(1, 0)]);
    c[8] = 9;
    assert_eq!(code(vec![c]), "SYRP-FORMAT-0013");
    assert_eq!(code(vec![chunk(b"cue ", &[0; 3])]), "SYRP-FORMAT-0013");
}

#[test]
fn b3_metadata_chunk_past_the_end_is_skipped_syrp_choice() {
    let mut file = with(vec![]);
    file.extend(chunk(b"iXML", b"<x/>"));
    let at = file.len() - 12;
    file[at + 4..at + 8].copy_from_slice(&100u32.to_le_bytes());
    let len = u32::try_from(file.len() - 8).unwrap();
    file[4..8].copy_from_slice(&len.to_le_bytes());
    assert!(open(file).unwrap().metadata().is_empty());
}
