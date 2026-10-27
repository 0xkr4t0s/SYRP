// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-02.md B5 and B6: `channel_set` helpers over the contract's
//! `ChannelSet`.

use syrp_audio_api::{ChannelSet, Speaker};
use syrp_audio_basics::channel_set::{
    canonical_for_count, channel_sets_with_count, from_abbreviated_string, from_wave_channel_mask,
    speaker_abbreviation, speaker_arrangement_string, speaker_name, wave_channel_mask,
};

/// Every named layout with its arrangement string and `WAVEFORMATEXTENSIBLE` mask, written
/// out from Microsoft's `SPEAKER_*` values (FL 0x1, FR 0x2, FC 0x4, LFE 0x8, BL 0x10,
/// BR 0x20, BC 0x100, SL 0x200, SR 0x400, TFL 0x1000, TFR 0x4000, TBL 0x8000, TBR 0x20000).
fn layouts() -> [(ChannelSet, &'static str, u32); 11] {
    [
        (ChannelSet::mono(), "C", 0x4),
        (ChannelSet::stereo(), "L R", 0x3),
        (ChannelSet::lcr(), "L R C", 0x7),
        (ChannelSet::quadraphonic(), "L R Ls Rs", 0x603),
        (ChannelSet::surround_5_0(), "L R C Ls Rs", 0x607),
        (ChannelSet::surround_5_1(), "L R C Lfe Ls Rs", 0x60F),
        (ChannelSet::surround_6_0(), "L R C Ls Rs Cs", 0x707),
        (ChannelSet::surround_6_1(), "L R C Lfe Ls Rs Cs", 0x70F),
        (ChannelSet::surround_7_0(), "L R C Ls Rs Lrs Rrs", 0x637),
        (ChannelSet::surround_7_1(), "L R C Lfe Ls Rs Lrs Rrs", 0x63F),
        (
            ChannelSet::surround_7_1_4(),
            "L R C Lfe Ls Rs Lrs Rrs Tfl Tfr Trl Trr",
            0x2D63F,
        ),
    ]
}

#[test]
fn b5_speaker_names_and_abbreviations() {
    use Speaker::*;
    let table = [
        (Left, "Left", "L"),
        (Right, "Right", "R"),
        (Centre, "Centre", "C"),
        (Lfe, "LFE", "Lfe"),
        (LeftSurround, "Left Surround", "Ls"),
        (RightSurround, "Right Surround", "Rs"),
        (CentreSurround, "Centre Surround", "Cs"),
        (LeftRearSurround, "Left Surround Rear", "Lrs"),
        (RightRearSurround, "Right Surround Rear", "Rrs"),
        (TopFrontLeft, "Top Front Left", "Tfl"),
        (TopFrontRight, "Top Front Right", "Tfr"),
        (TopRearLeft, "Top Rear Left", "Trl"),
        (TopRearRight, "Top Rear Right", "Trr"),
    ];
    for (s, name, abbr) in table {
        assert_eq!(speaker_name(s), name);
        assert_eq!(speaker_abbreviation(s), abbr);
    }
}

#[test]
fn b5_discrete_speakers_count_from_one_syrp_choice() {
    assert_eq!(speaker_name(Speaker::Discrete(0)), "Discrete 1");
    assert_eq!(speaker_abbreviation(Speaker::Discrete(15)), "D16");
    assert_eq!(
        speaker_abbreviation(Speaker::Discrete(u32::MAX)),
        "D4294967296"
    );
    assert_eq!(
        speaker_arrangement_string(&ChannelSet::discrete(3)),
        "D1 D2 D3"
    );
    assert_eq!(speaker_arrangement_string(&ChannelSet::disabled()), "");
}

#[test]
fn b5_arrangement_strings_list_channels_in_order() {
    for (set, text, _) in layouts() {
        assert_eq!(speaker_arrangement_string(&set), text, "{set:?}");
    }
}

#[test]
fn b5_from_abbreviated_string_ignores_order_and_spacing() {
    assert_eq!(from_abbreviated_string("R L"), Some(ChannelSet::stereo()));
    assert_eq!(
        from_abbreviated_string("  Lfe\tC  R\nL Rs Ls "),
        Some(ChannelSet::surround_5_1())
    );
    assert_eq!(from_abbreviated_string(""), Some(ChannelSet::disabled()));
    assert_eq!(from_abbreviated_string(" \t"), Some(ChannelSet::disabled()));
    assert_eq!(
        from_abbreviated_string("D3 D1 D2"),
        Some(ChannelSet::discrete(3))
    );
}

#[test]
fn b5_from_abbreviated_string_rejects_what_it_cannot_name() {
    for text in [
        "L L",          // repeated speaker
        "L R Lfe",      // no such layout in the contract
        "l r",          // abbreviations are case-sensitive
        "L R X",        // unknown
        "D1 D1",        // repeated discrete
        "D1 D3",        // gap
        "D0",           // discrete counts from one
        "D01",          // leading zero
        "D+1",          // sign
        "D",            // no number
        "L D1",         // mixed
        "D99999999999", // out of range
    ] {
        assert_eq!(from_abbreviated_string(text), None, "{text:?}");
    }
}

#[test]
fn b5_wave_channel_masks_follow_the_microsoft_bits() {
    for (set, _, mask) in layouts() {
        assert_eq!(wave_channel_mask(&set), Some(mask), "{set:?}");
    }
    assert_eq!(wave_channel_mask(&ChannelSet::discrete(2)), None);
    assert_eq!(wave_channel_mask(&ChannelSet::disabled()), None);
}

#[test]
fn b5_from_wave_channel_mask_accepts_back_pair_surrounds_syrp_choice() {
    assert_eq!(
        from_wave_channel_mask(0x3F),
        Some(ChannelSet::surround_5_1())
    );
    assert_eq!(
        from_wave_channel_mask(0x37),
        Some(ChannelSet::surround_5_0())
    );
    assert_eq!(
        from_wave_channel_mask(0x33),
        Some(ChannelSet::quadraphonic())
    );
    assert_eq!(
        from_wave_channel_mask(0x137),
        Some(ChannelSet::surround_6_0())
    );
    // A lone back speaker or a back pair beside a side pair is not reinterpreted.
    assert_eq!(from_wave_channel_mask(0x13), None);
    assert_eq!(from_wave_channel_mask(0x633), None);
}

#[test]
fn b5_from_wave_channel_mask_rejects_unnamed_layouts() {
    for mask in [0, 0x800, 0x40, 0x8000_0000, 0x0B, 0x6_3F | 0x800, u32::MAX] {
        assert_eq!(from_wave_channel_mask(mask), None, "{mask:#x}");
    }
}

#[test]
fn b5_channel_sets_with_count_and_canonical() {
    assert_eq!(channel_sets_with_count(0), [ChannelSet::disabled()]);
    assert_eq!(
        channel_sets_with_count(1),
        [ChannelSet::mono(), ChannelSet::discrete(1)]
    );
    assert_eq!(
        channel_sets_with_count(7),
        [
            ChannelSet::surround_6_1(),
            ChannelSet::surround_7_0(),
            ChannelSet::discrete(7)
        ]
    );
    assert_eq!(
        channel_sets_with_count(12),
        [ChannelSet::surround_7_1_4(), ChannelSet::discrete(12)]
    );
    for n in 0..=16 {
        let sets = channel_sets_with_count(n);
        assert!(sets.iter().all(|s| s.channel_count() == n as usize));
        let canonical = canonical_for_count(n);
        assert_eq!(canonical.is_some(), matches!(n, 0..=8 | 12), "{n}");
        if let Some(c) = canonical {
            assert_eq!(c, ChannelSet::canonical(n));
            assert!(sets.contains(&c));
        }
    }
    assert_eq!(canonical_for_count(0), Some(ChannelSet::disabled()));
    assert_eq!(canonical_for_count(6), Some(ChannelSet::surround_5_1()));
}

#[test]
fn b6_every_layout_round_trips_abbreviations_and_masks() {
    let mut sets: Vec<ChannelSet> = layouts().iter().map(|l| l.0).collect();
    sets.extend([1, 2, 7, 64, 65, 200].map(ChannelSet::discrete));
    sets.push(ChannelSet::disabled());
    for set in sets {
        let text = speaker_arrangement_string(&set);
        assert_eq!(from_abbreviated_string(&text), Some(set), "{text:?}");
        for (i, s) in set.speakers().enumerate() {
            assert_eq!(text.split(' ').nth(i), Some(&*speaker_abbreviation(s)));
        }
        if let Some(mask) = wave_channel_mask(&set) {
            assert_eq!(from_wave_channel_mask(mask), Some(set), "{mask:#x}");
            assert_eq!(mask.count_ones() as usize, set.channel_count());
        }
    }
}
