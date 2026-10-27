// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Channel masks: Microsoft's published speaker bits mapped to the contract's speakers.
//!
//! WAV stores channels in ascending speaker-bit order; [`ChannelSet`] uses Syrp's canonical
//! order. The mapping is a permutation from canonical index to file index.

use syrp_audio_api::{ChannelSet, Speaker};

use crate::WavError;

const FRONT_LEFT: u32 = 0x1;
const FRONT_RIGHT: u32 = 0x2;
const FRONT_CENTER: u32 = 0x4;
const LOW_FREQUENCY: u32 = 0x8;
const BACK_LEFT: u32 = 0x10;
const BACK_RIGHT: u32 = 0x20;
const BACK_CENTER: u32 = 0x100;
const SIDE_LEFT: u32 = 0x200;
const SIDE_RIGHT: u32 = 0x400;
const TOP_FRONT_LEFT: u32 = 0x1000;
const TOP_FRONT_RIGHT: u32 = 0x4000;
const TOP_BACK_LEFT: u32 = 0x8000;
const TOP_BACK_RIGHT: u32 = 0x20000;

/// The named layouts a mask can resolve to, tried in this order.
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

/// The contract speaker for one mask bit. Back left/right are the surround pair when the
/// mask has no side pair, and the rear pair when it has one (Microsoft's 5.1 and 7.1 masks).
fn speaker_of(bit: u32, has_side: bool) -> Option<Speaker> {
    Some(match bit {
        FRONT_LEFT => Speaker::Left,
        FRONT_RIGHT => Speaker::Right,
        FRONT_CENTER => Speaker::Centre,
        LOW_FREQUENCY => Speaker::Lfe,
        BACK_LEFT if has_side => Speaker::LeftRearSurround,
        BACK_RIGHT if has_side => Speaker::RightRearSurround,
        BACK_LEFT => Speaker::LeftSurround,
        BACK_RIGHT => Speaker::RightSurround,
        BACK_CENTER => Speaker::CentreSurround,
        SIDE_LEFT => Speaker::LeftSurround,
        SIDE_RIGHT => Speaker::RightSurround,
        TOP_FRONT_LEFT => Speaker::TopFrontLeft,
        TOP_FRONT_RIGHT => Speaker::TopFrontRight,
        TOP_BACK_LEFT => Speaker::TopRearLeft,
        TOP_BACK_RIGHT => Speaker::TopRearRight,
        _ => return None,
    })
}

/// The file's layout and, for each canonical channel, the file channel holding it.
pub(crate) struct Layout {
    pub(crate) channels: ChannelSet,
    pub(crate) file_index: Vec<usize>,
}

fn identity(channels: ChannelSet, n: usize) -> Result<Layout, WavError> {
    let mut file_index = Vec::new();
    file_index
        .try_reserve_exact(n)
        .map_err(|_| WavError::OutOfMemory)?;
    file_index.extend(0..n);
    Ok(Layout {
        channels,
        file_index,
    })
}

/// Resolves `mask` (`None` for a plain header) for a file of `count` channels.
///
/// A plain header is mono or stereo for one or two channels (Microsoft's default for
/// `WAVEFORMATEX`) and discrete otherwise. A mask whose bit count differs from `count`, that
/// uses a bit the contract has no speaker for, or whose speakers form no named layout, gives
/// a discrete layout in file order.
pub(crate) fn from_mask(mask: Option<u32>, count: u16) -> Result<Layout, WavError> {
    let n = usize::from(count);
    let discrete = ChannelSet::discrete(u32::from(count));
    let Some(mask) = mask else {
        let set = match count {
            1 => ChannelSet::mono(),
            2 => ChannelSet::stereo(),
            _ => discrete,
        };
        return identity(set, n);
    };
    if mask.count_ones() != u32::from(count) {
        return identity(discrete, n);
    }
    let has_side = mask & (SIDE_LEFT | SIDE_RIGHT) != 0;
    let mut in_file = Vec::with_capacity(32);
    for shift in 0..32 {
        let bit = 1u32 << shift;
        if mask & bit != 0 {
            match speaker_of(bit, has_side) {
                Some(s) => in_file.push(s),
                None => return identity(discrete, n),
            }
        }
    }
    for set in NAMED {
        if set.channel_count() != n {
            continue;
        }
        let file_index: Option<Vec<usize>> = set
            .speakers()
            .map(|s| in_file.iter().position(|&f| f == s))
            .collect();
        if let Some(file_index) = file_index {
            return Ok(Layout {
                channels: set,
                file_index,
            });
        }
    }
    identity(discrete, n)
}

/// The mask for writing `set` and, for each file channel in ascending bit order, the
/// canonical channel it holds. Discrete layouts get mask 0 and keep their order. The inverse
/// of [`from_mask`]: the surround pair goes to the side bits when the layout has a rear pair.
pub(crate) fn to_mask(set: ChannelSet) -> Result<(u32, Vec<usize>), WavError> {
    let n = set.channel_count();
    let has_rear = set.index_of(Speaker::LeftRearSurround).is_some();
    let mut bits = Vec::new();
    bits.try_reserve_exact(n)
        .map_err(|_| WavError::OutOfMemory)?;
    for (i, s) in set.speakers().enumerate() {
        let bit = match s {
            Speaker::Left => FRONT_LEFT,
            Speaker::Right => FRONT_RIGHT,
            Speaker::Centre => FRONT_CENTER,
            Speaker::Lfe => LOW_FREQUENCY,
            Speaker::LeftSurround if has_rear => SIDE_LEFT,
            Speaker::RightSurround if has_rear => SIDE_RIGHT,
            Speaker::LeftSurround | Speaker::LeftRearSurround => BACK_LEFT,
            Speaker::RightSurround | Speaker::RightRearSurround => BACK_RIGHT,
            Speaker::CentreSurround => BACK_CENTER,
            Speaker::TopFrontLeft => TOP_FRONT_LEFT,
            Speaker::TopFrontRight => TOP_FRONT_RIGHT,
            Speaker::TopRearLeft => TOP_BACK_LEFT,
            Speaker::TopRearRight => TOP_BACK_RIGHT,
            _ => return Ok((0, identity(set, n)?.file_index)),
        };
        bits.push((bit, i));
    }
    bits.sort_unstable();
    let mask = bits.iter().fold(0, |m, &(b, _)| m | b);
    Ok((mask, bits.into_iter().map(|(_, i)| i).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b2_seven_one_mask_reorders_side_before_rear() {
        let l = from_mask(Some(0x63F), 8).expect("fits");
        assert_eq!(l.channels, ChannelSet::surround_7_1());
        // File order: L R C LFE BL BR SL SR. Canonical: L R C LFE Ls Rs Lrs Rrs.
        assert_eq!(l.file_index, [0, 1, 2, 3, 6, 7, 4, 5]);
    }

    #[test]
    fn b4_every_named_layout_round_trips_through_its_mask() {
        for set in NAMED {
            let (mask, order) = to_mask(set).expect("fits");
            let back =
                from_mask(Some(mask), u16::try_from(order.len()).expect("small")).expect("fits");
            assert_eq!(back.channels, set, "{mask:#x}");
            // Reading file channel order[j] into canonical slot order[j] undoes the writer.
            for (canonical, &file) in back.file_index.iter().enumerate() {
                assert_eq!(order[file], canonical);
            }
        }
        assert_eq!(
            to_mask(ChannelSet::discrete(3)).expect("fits"),
            (0, vec![0, 1, 2])
        );
    }

    #[test]
    fn b2_unmapped_bit_gives_discrete() {
        // Front-left-of-centre has no contract speaker.
        let l = from_mask(Some(0x43), 3).expect("fits");
        assert_eq!(l.channels, ChannelSet::discrete(3));
        assert_eq!(l.file_index, [0, 1, 2]);
    }
}
