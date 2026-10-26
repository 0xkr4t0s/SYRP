// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for WP-0.2-04 (first increment: stream description, metadata, errors).

use std::io;

use syrp_audio_api::ChannelSet;
use syrp_error::Diagnostic;
use syrp_format_api::{
    AudioSpec, Bext, CuePoint, FormatError, LoopKind, Metadata, MetadataItem, SampleKind,
    SampleLoop, Tag, WriterOptions,
};

#[test]
fn b2_spec_reports_stream_shape() {
    let spec = AudioSpec::new(96_000.0, ChannelSet::surround_5_1(), 24, SampleKind::Int).unwrap();
    assert_eq!(spec.sample_rate(), 96_000.0);
    assert_eq!(spec.channels(), ChannelSet::surround_5_1());
    assert_eq!(spec.channel_count(), 6);
    assert_eq!(spec.bits_per_sample(), 24);
    assert_eq!(spec.sample_kind(), SampleKind::Int);
    assert!(!spec.is_float());
    let opts = WriterOptions::new(spec).with_quality(3);
    assert_eq!((opts.spec(), opts.quality()), (&spec, Some(3)));
}

#[test]
fn b4_metadata_holds_typed_chunks_and_an_escape() {
    let mut meta = Metadata::new();
    let bext = Bext::new()
        .with_description("dawn chorus")
        .with_originator("rec")
        .with_originator_reference("ref-1")
        .with_origination_date("2026-10-26")
        .with_origination_time("05:30:00")
        .with_time_reference(1_000)
        .with_coding_history("A=PCM,F=48000");
    meta.push(MetadataItem::Bext(bext.clone()));
    meta.push(MetadataItem::Cue(CuePoint::new(1, 10).with_label("in")));
    let lp = SampleLoop::new(LoopKind::Forward, 10, 20).unwrap();
    meta.push(MetadataItem::Loop(lp.with_play_count(3)));
    meta.set_tag(Tag::Title, "first");
    meta.set_tag(Tag::Custom("ISRC".into()), "XX0000000000");
    meta.push(MetadataItem::Other("junk".into(), vec![1, 2, 3]));
    meta.push(MetadataItem::Tag(Tag::Title, "duplicate".into()));
    meta.set_tag(Tag::Title, "second");

    assert_eq!(meta.len(), 6, "set_tag collapses duplicate titles");
    assert_eq!(meta.tag(&Tag::Title), Some("second"));
    assert!(matches!(meta.items()[3], MetadataItem::Tag(Tag::Title, _)));
    assert_eq!(meta.tag(&Tag::Custom("ISRC".into())), Some("XX0000000000"));
    assert_eq!(meta.bext(), Some(&bext));
    assert_eq!(bext.description(), "dawn chorus");
    assert_eq!(bext.originator_reference(), "ref-1");
    assert_eq!(
        (bext.origination_date(), bext.origination_time()),
        ("2026-10-26", "05:30:00")
    );
    assert_eq!(bext.coding_history(), "A=PCM,F=48000");
    assert!(
        matches!(&meta.items()[5], MetadataItem::Other(id, b) if id == "junk" && b == &[1, 2, 3])
    );
    assert!(SampleLoop::new(LoopKind::Backward, 2, 1).is_none());
    let spec = AudioSpec::new(48_000.0, ChannelSet::stereo(), 32, SampleKind::Float).unwrap();
    let opts = WriterOptions::new(spec).with_metadata(meta.clone());
    assert_eq!(opts.metadata(), &meta);
}

#[test]
fn b5_error_codes_are_catalogued() {
    let catalogue = include_str!("../../../../errors/format.toml");
    let errors = [
        FormatError::from(io::Error::other("x")),
        FormatError::invalid_data("x"),
        FormatError::unsupported("x"),
        AudioSpec::new(f64::NAN, ChannelSet::mono(), 16, SampleKind::Int).unwrap_err(),
    ];
    for (i, err) in errors.iter().enumerate() {
        let code = err.code();
        assert_eq!(code.domain(), "FORMAT");
        assert_eq!(code.as_str(), format!("SYRP-FORMAT-{:04}", i + 1));
        assert!(catalogue.contains(&format!("code = \"{code}\"")), "{code}");
        assert!(!err.to_string().is_empty());
    }
    // SYRP-FORMAT-0005 (ChannelMismatch) is checked by the unit test in lib.rs.
    assert!(catalogue.contains("code = \"SYRP-FORMAT-0005\""));
    assert_eq!(catalogue.matches("[[error]]").count(), 5);
    assert!(std::error::Error::source(&errors[0]).is_some());
    for (bits, kind) in [
        (0, SampleKind::Int),
        (33, SampleKind::Int),
        (16, SampleKind::Float),
    ] {
        assert!(AudioSpec::new(48_000.0, ChannelSet::mono(), bits, kind).is_err());
    }
    for channels in [ChannelSet::disabled(), ChannelSet::discrete(0)] {
        assert!(AudioSpec::new(48_000.0, channels, 16, SampleKind::Int).is_err());
    }
}

#[test]
fn b6_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/metadata.rs"),
        include_str!("../src/spec.rs"),
    ];
    let mut in_struct = false;
    for line in src.iter().flat_map(|s| s.lines()) {
        if line.starts_with("pub struct") {
            assert!(!line.contains("(pub"), "public tuple field: {line}");
            in_struct = line.ends_with('{');
        } else if line.starts_with('}') {
            in_struct = false;
        } else if in_struct {
            assert!(
                !line.trim_start().starts_with("pub"),
                "public field: {line}"
            );
        }
    }
    fn send_sync<T: Send + Sync>() {}
    send_sync::<AudioSpec>();
    send_sync::<WriterOptions>();
    send_sync::<Metadata>();
}
