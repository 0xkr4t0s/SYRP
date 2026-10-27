// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Helpers over the frozen [`ChannelSet`] and [`Speaker`] contract types, in the role of the
//! parts of JUCE's `AudioChannelSet` the contract does not cover (WP-2.1-02 B5): speaker names
//! and abbreviations, arrangement strings, `WAVEFORMATEXTENSIBLE` channel masks, and the
//! layouts for a channel count.
//!
//! Only the layouts the contract has are covered: disabled, mono to 7.1.4, and discrete.
//! A string or mask that describes any other layout gives `None`.
//!
//! ```
//! use syrp_audio_api::ChannelSet;
//! use syrp_audio_basics::channel_set;
//! let set = ChannelSet::surround_5_1();
//! assert_eq!(channel_set::speaker_arrangement_string(&set), "L R C Lfe Ls Rs");
//! assert_eq!(channel_set::from_abbreviated_string("L R C Lfe Ls Rs"), Some(set));
//! assert_eq!(channel_set::wave_channel_mask(&set), Some(0x60F));
//! ```

use std::borrow::Cow;

use syrp_audio_api::{ChannelSet, Speaker};

/// Every named layout the contract defines, in order of channel count.
const NAMED: [ChannelSet; 11] = [
    ChannelSet::mono(),
    ChannelSet::stereo(),
    ChannelSet::lcr(),
    ChannelSet::quadraphonic(),
    ChannelSet::surround_5_0(),
    ChannelSet::surround_5_1(),
    ChannelSet::surround_6_0(),
    ChannelSet::surround_6_1(),
    ChannelSet::surround_7_0(),
    ChannelSet::surround_7_1(),
    ChannelSet::surround_7_1_4(),
];

/// The positioned speakers with their name, abbreviation and `WAVEFORMATEXTENSIBLE` bit.
///
/// The bits are Microsoft's published `SPEAKER_*` values. `LeftSurround`/`RightSurround` are
/// the side speakers and `LeftRearSurround`/`RightRearSurround` the back ones (Syrp's choice,
/// matching the contract's description of them).
const SPEAKERS: [(Speaker, &str, &str, u32); 13] = [
    (Speaker::Left, "Left", "L", 0x1),
    (Speaker::Right, "Right", "R", 0x2),
    (Speaker::Centre, "Centre", "C", 0x4),
    (Speaker::Lfe, "LFE", "Lfe", 0x8),
    (Speaker::LeftSurround, "Left Surround", "Ls", 0x200),
    (Speaker::RightSurround, "Right Surround", "Rs", 0x400),
    (Speaker::CentreSurround, "Centre Surround", "Cs", 0x100),
    (Speaker::LeftRearSurround, "Left Surround Rear", "Lrs", 0x10),
    (
        Speaker::RightRearSurround,
        "Right Surround Rear",
        "Rrs",
        0x20,
    ),
    (Speaker::TopFrontLeft, "Top Front Left", "Tfl", 0x1000),
    (Speaker::TopFrontRight, "Top Front Right", "Tfr", 0x4000),
    (Speaker::TopRearLeft, "Top Rear Left", "Trl", 0x8000),
    (Speaker::TopRearRight, "Top Rear Right", "Trr", 0x2_0000),
];

/// `SPEAKER_BACK_LEFT | SPEAKER_BACK_RIGHT`.
const BACK_PAIR: u32 = 0x30;
/// `SPEAKER_SIDE_LEFT | SPEAKER_SIDE_RIGHT`.
const SIDE_PAIR: u32 = 0x600;

fn entry(speaker: Speaker) -> Option<&'static (Speaker, &'static str, &'static str, u32)> {
    SPEAKERS.iter().find(|e| e.0 == speaker)
}

/// The full name of a speaker, in the role of `AudioChannelSet::getChannelTypeName`: "Left",
/// "Left Surround", "LFE" and so on. The wording is Syrp's, built from the speaker names, and
/// not yet confirmed against JUCE's output. Discrete channels are "Discrete 1", "Discrete 2"
/// and so on, counting from one (Syrp's choice).
///
/// ```
/// use syrp_audio_api::Speaker;
/// use syrp_audio_basics::channel_set::speaker_name;
/// assert_eq!(speaker_name(Speaker::TopFrontLeft), "Top Front Left");
/// assert_eq!(speaker_name(Speaker::Discrete(0)), "Discrete 1");
/// ```
#[doc(alias = "AudioChannelSet::getChannelTypeName")]
#[must_use]
pub fn speaker_name(speaker: Speaker) -> Cow<'static, str> {
    match (speaker, entry(speaker)) {
        (_, Some(e)) => Cow::Borrowed(e.1),
        (Speaker::Discrete(n), None) => Cow::Owned(format!("Discrete {}", u64::from(n) + 1)),
        _ => Cow::Borrowed("Unknown"),
    }
}

/// The short name of a speaker, in the role of
/// `AudioChannelSet::getAbbreviatedChannelTypeName`: "L", "Ls", "Lfe", "Tfl" and so on, in the
/// style of the 5.1 example "L R C Lfe Ls Rs" in the `AudioChannelSet` documentation.
/// Discrete channels are "D1", "D2" and so on, counting from one (Syrp's choice).
///
/// ```
/// use syrp_audio_api::Speaker;
/// use syrp_audio_basics::channel_set::speaker_abbreviation;
/// assert_eq!(speaker_abbreviation(Speaker::RightRearSurround), "Rrs");
/// assert_eq!(speaker_abbreviation(Speaker::Discrete(2)), "D3");
/// ```
#[doc(alias = "AudioChannelSet::getAbbreviatedChannelTypeName")]
#[must_use]
pub fn speaker_abbreviation(speaker: Speaker) -> Cow<'static, str> {
    match (speaker, entry(speaker)) {
        (_, Some(e)) => Cow::Borrowed(e.2),
        (Speaker::Discrete(n), None) => Cow::Owned(format!("D{}", u64::from(n) + 1)),
        _ => Cow::Borrowed("?"),
    }
}

/// The abbreviations of the set's speakers in channel order, separated by single spaces:
/// `"L R C Lfe Ls Rs"` for 5.1. A disabled set gives `""`.
///
/// ```
/// use syrp_audio_api::ChannelSet;
/// use syrp_audio_basics::channel_set::speaker_arrangement_string;
/// assert_eq!(speaker_arrangement_string(&ChannelSet::stereo()), "L R");
/// assert_eq!(speaker_arrangement_string(&ChannelSet::discrete(2)), "D1 D2");
/// ```
#[doc(alias = "AudioChannelSet::getSpeakerArrangementAsString")]
#[must_use]
pub fn speaker_arrangement_string(set: &ChannelSet) -> String {
    let names: Vec<_> = set.speakers().map(speaker_abbreviation).collect();
    names.join(" ")
}

/// Parses abbreviations separated by whitespace into the layout with exactly those speakers.
///
/// As with JUCE's `AudioChannelSet`, which is a set of speakers, the order does not matter:
/// `"R L"` is stereo. `"D1 ... Dn"` (each once) is `discrete(n)`, and a blank string is
/// disabled. `None` for an unknown or repeated abbreviation, or when no layout of the
/// contract has exactly those speakers.
///
/// ```
/// use syrp_audio_api::ChannelSet;
/// use syrp_audio_basics::channel_set::from_abbreviated_string;
/// assert_eq!(from_abbreviated_string("Ls Rs L R"), Some(ChannelSet::quadraphonic()));
/// assert_eq!(from_abbreviated_string("D2 D1 D3"), Some(ChannelSet::discrete(3)));
/// assert_eq!(from_abbreviated_string("L R Lfe"), None);
/// ```
#[doc(alias = "AudioChannelSet::fromAbbreviatedString")]
#[must_use]
pub fn from_abbreviated_string(text: &str) -> Option<ChannelSet> {
    let mut mask = 0_u32;
    let mut discrete: Vec<u32> = Vec::new();
    for word in text.split_whitespace() {
        if let Some(e) = SPEAKERS.iter().find(|e| e.2 == word) {
            if mask & e.3 != 0 {
                return None;
            }
            mask |= e.3;
        } else {
            // "D<n>": n from 1, digits only (no sign or leading zero).
            let digits = word.strip_prefix('D')?;
            if digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            discrete.push(digits.parse().ok()?);
        }
    }
    match (mask, discrete.is_empty()) {
        (0, true) => Some(ChannelSet::disabled()),
        (0, false) => {
            discrete.sort_unstable();
            let count = u32::try_from(discrete.len()).ok()?;
            let complete = discrete.iter().zip(1..).all(|(&n, i)| n == i);
            complete.then(|| ChannelSet::discrete(count))
        }
        (m, true) => set_with_mask(m),
        _ => None,
    }
}

/// The named layout whose speakers have exactly these `SPEAKERS` bits.
fn set_with_mask(mask: u32) -> Option<ChannelSet> {
    NAMED.into_iter().find(|set| {
        set.speakers()
            .map(|s| entry(s).map_or(0, |e| e.3))
            .sum::<u32>()
            == mask
    })
}

/// The `WAVEFORMATEXTENSIBLE` `dwChannelMask` for a named layout: the OR of Microsoft's
/// `SPEAKER_*` bits for its speakers. `None` for disabled and discrete sets, which have no
/// speaker positions.
///
/// Surround speakers map to the side bits and rear surround speakers to the back bits, so
/// 5.1 is `0x60F` (`KSAUDIO_SPEAKER_5POINT1_SURROUND`) and 7.1 is `0x63F`. WAV stores channels
/// in the order of the mask bits, which for 7.1 and 7.1.4 differs from the set's channel
/// order; reordering is the WAV format's job (WP-2.1-14).
///
/// ```
/// use syrp_audio_api::ChannelSet;
/// use syrp_audio_basics::channel_set::wave_channel_mask;
/// assert_eq!(wave_channel_mask(&ChannelSet::stereo()), Some(0x3));
/// assert_eq!(wave_channel_mask(&ChannelSet::surround_7_1()), Some(0x63F));
/// assert_eq!(wave_channel_mask(&ChannelSet::discrete(2)), None);
/// ```
#[doc(alias = "AudioChannelSet::getWaveChannelMask")]
#[must_use]
pub fn wave_channel_mask(set: &ChannelSet) -> Option<u32> {
    if set.is_discrete() || set.is_disabled() {
        return None;
    }
    set.speakers().map(|s| entry(s).map(|e| e.3)).sum()
}

/// The named layout for a `WAVEFORMATEXTENSIBLE` channel mask, or `None` when no layout of the
/// contract has exactly those speakers (including mask 0, which names none).
///
/// Besides the masks [`wave_channel_mask`] produces, the back pair (`0x30`) is accepted in
/// place of the side pair when the mask has no side bits, as older files write it: `0x3F`
/// (`KSAUDIO_SPEAKER_5POINT1`) is 5.1 and `0x33` is quadraphonic (Syrp's choice).
///
/// ```
/// use syrp_audio_api::ChannelSet;
/// use syrp_audio_basics::channel_set::from_wave_channel_mask;
/// assert_eq!(from_wave_channel_mask(0x60F), Some(ChannelSet::surround_5_1()));
/// assert_eq!(from_wave_channel_mask(0x3F), Some(ChannelSet::surround_5_1()));
/// assert_eq!(from_wave_channel_mask(0x800), None); // top centre: no such layout
/// ```
#[doc(alias = "AudioChannelSet::channelSetForWaveChannelMask")]
#[must_use]
pub fn from_wave_channel_mask(mask: u32) -> Option<ChannelSet> {
    if mask == 0 {
        return None;
    }
    set_with_mask(mask).or_else(|| {
        (mask & SIDE_PAIR == 0 && mask & BACK_PAIR == BACK_PAIR)
            .then(|| set_with_mask(mask & !BACK_PAIR | SIDE_PAIR))
            .flatten()
    })
}

/// Every layout with `n` channels: the named ones first, in the order the contract lists
/// them, then `discrete(n)`. For `n == 0`, only the disabled set.
///
/// ```
/// use syrp_audio_api::ChannelSet;
/// use syrp_audio_basics::channel_set::channel_sets_with_count;
/// assert_eq!(
///     channel_sets_with_count(6),
///     [ChannelSet::surround_5_1(), ChannelSet::surround_6_0(), ChannelSet::discrete(6)]
/// );
/// assert_eq!(channel_sets_with_count(9), [ChannelSet::discrete(9)]);
/// ```
#[doc(alias = "AudioChannelSet::channelSetsWithNumberOfChannels")]
#[must_use]
pub fn channel_sets_with_count(n: u32) -> Vec<ChannelSet> {
    if n == 0 {
        return vec![ChannelSet::disabled()];
    }
    let mut sets: Vec<ChannelSet> = NAMED
        .into_iter()
        .filter(|s| s.channel_count() == n as usize)
        .collect();
    sets.push(ChannelSet::discrete(n));
    sets
}

/// The canonical named layout for `n` channels (mono, stereo, LCR, quadraphonic, 5.0, 5.1,
/// 7.0, 7.1, 7.1.4, and disabled for 0), or `None` when `n` has none. Unlike
/// [`ChannelSet::canonical`], it does not fall back to `discrete(n)`.
///
/// ```
/// use syrp_audio_api::ChannelSet;
/// use syrp_audio_basics::channel_set::canonical_for_count;
/// assert_eq!(canonical_for_count(8), Some(ChannelSet::surround_7_1()));
/// assert_eq!(canonical_for_count(10), None);
/// ```
#[doc(alias = "AudioChannelSet::canonicalChannelSet")]
#[must_use]
pub fn canonical_for_count(n: u32) -> Option<ChannelSet> {
    let set = ChannelSet::canonical(n);
    (!set.is_discrete()).then_some(set)
}
