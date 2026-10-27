// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B3: Vorbis comments, cue sheets, application and picture blocks, in file order.

mod common;

use common::*;
use syrp_format_api::{CuePoint, MetadataItem, Tag};

/// A VORBIS_COMMENT body.
fn vorbis(vendor: &str, fields: &[&str]) -> Vec<u8> {
    let mut out = (vendor.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(vendor.as_bytes());
    out.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    for f in fields {
        out.extend_from_slice(&(f.len() as u32).to_le_bytes());
        out.extend_from_slice(f.as_bytes());
    }
    out
}

/// A CUESHEET track: offset, number, and (index offset, index number) pairs.
type Track<'a> = (u64, u8, &'a [(u64, u8)]);

/// A CUESHEET body.
fn cuesheet(tracks: &[Track<'_>]) -> Vec<u8> {
    let mut out = vec![0u8; 128 + 8 + 259];
    out.push(tracks.len() as u8);
    for (offset, number, indices) in tracks {
        out.extend_from_slice(&offset.to_be_bytes());
        out.push(*number);
        out.extend_from_slice(&[0u8; 12 + 14]);
        out.push(indices.len() as u8);
        for (at, n) in *indices {
            out.extend_from_slice(&at.to_be_bytes());
            out.push(*n);
            out.extend_from_slice(&[0u8; 3]);
        }
    }
    out
}

#[test]
fn b3_vorbis_comments_become_tags_ignoring_name_case() {
    let (plain, _) = encode(&signal(1000, 1, 16, 8), 1, 16, 44_100, 1024);
    let fields = [
        "title=Take 3",
        "ARTIST=Someone",
        "Album=A",
        "GENRE=Jazz",
        "date=2026",
        "COMMENT=one",
        "Description=two",
        "TRACKNUMBER=4",
        "copyright=(c)",
        "ENCODER=x",
        "Mood=calm",
        "EMPTY=",
    ];
    let bytes = with_blocks(&plain, &[(4, vorbis("vendor", &fields))]);
    let r = open(bytes).unwrap();
    let tags: Vec<(Tag, &str)> = r
        .metadata()
        .items()
        .iter()
        .map(|i| match i {
            MetadataItem::Tag(t, v) => (t.clone(), v.as_str()),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    let want = [
        (Tag::Title, "Take 3"),
        (Tag::Artist, "Someone"),
        (Tag::Album, "A"),
        (Tag::Genre, "Jazz"),
        (Tag::Date, "2026"),
        (Tag::Comment, "one"),
        (Tag::Comment, "two"),
        (Tag::TrackNumber, "4"),
        (Tag::Copyright, "(c)"),
        (Tag::Encoder, "x"),
        (Tag::Custom("Mood".into()), "calm"),
        (Tag::Custom("EMPTY".into()), ""),
    ];
    assert_eq!(tags, want);
}

#[test]
fn b3_malformed_vorbis_comments_are_coded_errors() {
    let (plain, _) = encode(&signal(1000, 1, 16, 9), 1, 16, 44_100, 1024);
    let bad = |body: Vec<u8>| code(open(with_blocks(&plain, &[(4, body)])));
    assert_eq!(bad(vorbis("v", &["NOEQUALS"])), "SYRP-FORMAT-0002");
    assert_eq!(bad(vorbis("v", &["=empty name"])), "SYRP-FORMAT-0002");
    let mut overlong = vorbis("v", &["A=b"]);
    overlong[9..13].copy_from_slice(&1000u32.to_le_bytes());
    assert_eq!(bad(overlong), "SYRP-FORMAT-0002");
    let mut trailing = vorbis("v", &["A=b"]);
    trailing.push(0);
    assert_eq!(bad(trailing), "SYRP-FORMAT-0002");
}

#[test]
fn b3_cuesheet_indices_become_cue_points() {
    let (plain, _) = encode(&signal(1000, 1, 16, 10), 1, 16, 44_100, 1024);
    let sheet = cuesheet(&[
        (0, 1, &[(0, 1)]),
        (588, 2, &[(0, 0), (588, 1)]),
        (1000, 170, &[]),
    ]);
    let r = open(with_blocks(&plain, &[(5, sheet)])).unwrap();
    let want = [
        CuePoint::new(0x101, 0),
        CuePoint::new(0x200, 588),
        CuePoint::new(0x201, 1176),
    ];
    let got: Vec<&CuePoint> = r
        .metadata()
        .items()
        .iter()
        .filter_map(|i| {
            if let MetadataItem::Cue(c) = i {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(got, want.iter().collect::<Vec<_>>());
}

#[test]
fn b3_application_and_picture_blocks_are_other_items_in_file_order() {
    let (plain, _) = encode(&signal(1000, 1, 16, 11), 1, 16, 44_100, 1024);
    let app = b"Syrpdata".to_vec();
    let picture = vec![0, 0, 0, 3, 1, 2, 3];
    let blocks = [
        (2, app.clone()),
        (4, vorbis("v", &["TITLE=t"])),
        (1, vec![0; 16]),
        (6, picture.clone()),
        (5, cuesheet(&[(0, 1, &[(0, 1)])])),
        (100, vec![9; 3]),
    ];
    let r = open(with_blocks(&plain, &blocks)).unwrap();
    let want = [
        MetadataItem::Other("APPLICATION".into(), app),
        MetadataItem::Tag(Tag::Title, "t".into()),
        MetadataItem::Other("PICTURE".into(), picture),
        MetadataItem::Cue(CuePoint::new(0x101, 0)),
    ];
    assert_eq!(r.metadata().items(), &want[..]);
    assert_eq!(
        code(open(with_blocks(&plain, &[(2, b"Syr".to_vec())]))),
        "SYRP-FORMAT-0002"
    );
    assert_eq!(
        code(open(with_blocks(&plain, &[(127, vec![])]))),
        "SYRP-FORMAT-0002"
    );
}
