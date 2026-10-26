// SPDX-License-Identifier: MIT OR Apache-2.0
//! Channel sets, buses and bus-layout negotiation (spec B2).

use syrp_rt::RtSafe;

/// A loudspeaker position, or an unnamed discrete channel.
///
/// Syrp's canonical channel order is the order [`ChannelSet`] lists its speakers in. Format
/// wrappers map host orders to it.
/// ```
/// use syrp_audio_api::{ChannelSet, Speaker};
/// assert_eq!(ChannelSet::surround_5_1().index_of(Speaker::Lfe), Some(3));
/// ```
#[doc(alias = "AudioChannelSet::ChannelType")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Speaker {
    /// Front left.
    Left,
    /// Front right.
    Right,
    /// Front centre.
    Centre,
    /// Low-frequency effects.
    Lfe,
    /// Side (or, in 5.x, surround) left.
    LeftSurround,
    /// Side (or, in 5.x, surround) right.
    RightSurround,
    /// Rear centre.
    CentreSurround,
    /// Rear left.
    LeftRearSurround,
    /// Rear right.
    RightRearSurround,
    /// Height layer, front left.
    TopFrontLeft,
    /// Height layer, front right.
    TopFrontRight,
    /// Height layer, rear left.
    TopRearLeft,
    /// Height layer, rear right.
    TopRearRight,
    /// Channel `n` (from 0) of a discrete layout, with no speaker position.
    Discrete(u32),
}

use Speaker::{
    Centre as C, CentreSurround as Cs, Left as L, LeftRearSurround as Lrs, LeftSurround as Ls, Lfe,
    Right as R, RightRearSurround as Rrs, RightSurround as Rs, TopFrontLeft as Tfl,
    TopFrontRight as Tfr, TopRearLeft as Trl, TopRearRight as Trr,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Disabled,
    Mono,
    Stereo,
    Lcr,
    Quad,
    S50,
    S51,
    S60,
    S61,
    S70,
    S71,
    S714,
    Discrete(u32),
}

impl Kind {
    fn named(self) -> (&'static [Speaker], &'static str) {
        match self {
            Kind::Disabled => (&[], "Disabled"),
            Kind::Mono => (&[C], "Mono"),
            Kind::Stereo => (&[L, R], "Stereo"),
            Kind::Lcr => (&[L, R, C], "LCR"),
            Kind::Quad => (&[L, R, Ls, Rs], "Quadraphonic"),
            Kind::S50 => (&[L, R, C, Ls, Rs], "5.0"),
            Kind::S51 => (&[L, R, C, Lfe, Ls, Rs], "5.1"),
            Kind::S60 => (&[L, R, C, Ls, Rs, Cs], "6.0"),
            Kind::S61 => (&[L, R, C, Lfe, Ls, Rs, Cs], "6.1"),
            Kind::S70 => (&[L, R, C, Ls, Rs, Lrs, Rrs], "7.0"),
            Kind::S71 => (&[L, R, C, Lfe, Ls, Rs, Lrs, Rrs], "7.1"),
            Kind::S714 => (
                &[L, R, C, Lfe, Ls, Rs, Lrs, Rrs, Tfl, Tfr, Trl, Trr],
                "7.1.4",
            ),
            Kind::Discrete(_) => (&[], "Discrete"),
        }
    }
}

/// An ordered set of channels: a named speaker layout or `n` discrete channels.
///
/// Named layouts list their speakers in Syrp's canonical order (see [`speakers`](Self::speakers)).
/// ```
/// use syrp_audio_api::{ChannelSet, Speaker};
/// let set = ChannelSet::surround_7_1_4();
/// assert_eq!(set.channel_count(), 12);
/// assert_eq!(set.speaker(8), Some(Speaker::TopFrontLeft));
/// assert_eq!(ChannelSet::discrete(3).speaker(2), Some(Speaker::Discrete(2)));
/// assert_eq!(set.name(), "7.1.4");
/// ```
#[doc(alias = "AudioChannelSet")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelSet {
    kind: Kind,
}

impl ChannelSet {
    const fn of(kind: Kind) -> Self {
        ChannelSet { kind }
    }

    /// No channels: a bus that is switched off.
    #[must_use]
    pub const fn disabled() -> Self {
        Self::of(Kind::Disabled)
    }

    /// One centre channel.
    #[must_use]
    pub const fn mono() -> Self {
        Self::of(Kind::Mono)
    }

    /// Left, right.
    #[must_use]
    pub const fn stereo() -> Self {
        Self::of(Kind::Stereo)
    }

    /// Left, right, centre.
    #[must_use]
    pub const fn lcr() -> Self {
        Self::of(Kind::Lcr)
    }

    /// Left, right, left surround, right surround.
    #[must_use]
    pub const fn quadraphonic() -> Self {
        Self::of(Kind::Quad)
    }

    /// 5.0: L, R, C, Ls, Rs.
    #[must_use]
    pub const fn surround_5_0() -> Self {
        Self::of(Kind::S50)
    }

    /// 5.1: L, R, C, LFE, Ls, Rs.
    #[must_use]
    pub const fn surround_5_1() -> Self {
        Self::of(Kind::S51)
    }

    /// 6.0: L, R, C, Ls, Rs, Cs.
    #[must_use]
    pub const fn surround_6_0() -> Self {
        Self::of(Kind::S60)
    }

    /// 6.1: L, R, C, LFE, Ls, Rs, Cs.
    #[must_use]
    pub const fn surround_6_1() -> Self {
        Self::of(Kind::S61)
    }

    /// 7.0: L, R, C, Ls, Rs, Lrs, Rrs.
    #[must_use]
    pub const fn surround_7_0() -> Self {
        Self::of(Kind::S70)
    }

    /// 7.1: L, R, C, LFE, Ls, Rs, Lrs, Rrs.
    #[must_use]
    pub const fn surround_7_1() -> Self {
        Self::of(Kind::S71)
    }

    /// 7.1.4: 7.1 plus top front left/right and top rear left/right.
    #[must_use]
    pub const fn surround_7_1_4() -> Self {
        Self::of(Kind::S714)
    }

    /// `n` discrete channels with no speaker positions (`n == 0` is [`disabled`](Self::disabled)).
    /// ```
    /// assert_eq!(syrp_audio_api::ChannelSet::discrete(16).channel_count(), 16);
    /// ```
    #[must_use]
    pub const fn discrete(n: u32) -> Self {
        if n == 0 {
            Self::disabled()
        } else {
            Self::of(Kind::Discrete(n))
        }
    }

    /// The default named layout for `n` channels: disabled, mono, stereo, LCR, quadraphonic,
    /// 5.0, 5.1, 7.0, 7.1 or 7.1.4 (for 0 to 8 and 12), and `discrete(n)` otherwise.
    /// ```
    /// use syrp_audio_api::ChannelSet;
    /// assert_eq!(ChannelSet::canonical(6), ChannelSet::surround_5_1());
    /// assert_eq!(ChannelSet::canonical(9), ChannelSet::discrete(9));
    /// ```
    #[must_use]
    pub const fn canonical(n: u32) -> Self {
        Self::of(match n {
            0 => Kind::Disabled,
            1 => Kind::Mono,
            2 => Kind::Stereo,
            3 => Kind::Lcr,
            4 => Kind::Quad,
            5 => Kind::S50,
            6 => Kind::S51,
            7 => Kind::S70,
            8 => Kind::S71,
            12 => Kind::S714,
            n => Kind::Discrete(n),
        })
    }

    /// Number of channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        match self.kind {
            Kind::Discrete(n) => n as usize,
            kind => kind.named().0.len(),
        }
    }

    /// True for a discrete layout.
    #[must_use]
    pub fn is_discrete(&self) -> bool {
        matches!(self.kind, Kind::Discrete(_))
    }

    /// True when the set has no channels.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.kind == Kind::Disabled
    }

    /// A short name: `"Stereo"`, `"5.1"`, `"Discrete"` and so on.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.kind.named().1
    }

    /// The speaker on channel `index`, or `None` past the last channel.
    #[must_use]
    pub fn speaker(&self, index: usize) -> Option<Speaker> {
        match self.kind {
            Kind::Discrete(n) => (index < n as usize).then_some(Speaker::Discrete(index as u32)),
            kind => kind.named().0.get(index).copied(),
        }
    }

    /// The speakers in channel order.
    pub fn speakers(&self) -> impl ExactSizeIterator<Item = Speaker> + use<> {
        let set = *self;
        (0..set.channel_count()).map(move |i| set.speaker(i).unwrap_or(Speaker::Discrete(i as u32)))
    }

    /// The channel index of `speaker`, if the set has it.
    #[must_use]
    pub fn index_of(&self, speaker: Speaker) -> Option<usize> {
        match (self.kind, speaker) {
            (Kind::Discrete(n), Speaker::Discrete(i)) => (i < n).then_some(i as usize),
            (kind, s) => kind.named().0.iter().position(|&x| x == s),
        }
    }
}

// SAFETY: plain `Copy` data with no destructor; no method allocates, locks or blocks.
unsafe impl RtSafe for Speaker {}
// SAFETY: as above.
unsafe impl RtSafe for ChannelSet {}

/// A named bus with a channel set, such as "Main" (stereo) or "Sidechain" (mono).
/// ```
/// use syrp_audio_api::{Bus, ChannelSet};
/// let bus = Bus::new("Sidechain", ChannelSet::mono());
/// assert_eq!((bus.name(), bus.channel_count()), ("Sidechain", 1));
/// ```
#[doc(alias = "AudioProcessor::Bus")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bus {
    name: String,
    channels: ChannelSet,
}

impl Bus {
    /// A bus called `name` carrying `channels`.
    pub fn new(name: impl Into<String>, channels: ChannelSet) -> Self {
        Bus {
            name: name.into(),
            channels,
        }
    }

    /// The bus name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The bus channel set.
    #[must_use]
    pub fn channel_set(&self) -> ChannelSet {
        self.channels
    }

    /// Number of channels on the bus.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.channel_count()
    }
}

/// The ordered input and output buses of a processor. Bus 0 in each direction is the main bus.
///
/// Built and negotiated on the message thread; the audio thread sees only channel counts.
/// ```
/// use syrp_audio_api::{BusLayout, ChannelSet};
/// let layout = BusLayout::new()
///     .with_input("Main", ChannelSet::stereo())
///     .with_input("Sidechain", ChannelSet::mono())
///     .with_output("Main", ChannelSet::stereo());
/// assert_eq!(layout.input_channels(), 3);
/// assert_eq!(layout.outputs()[0].name(), "Main");
/// ```
#[doc(alias = "AudioProcessor::BusesLayout")]
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct BusLayout {
    inputs: Vec<Bus>,
    outputs: Vec<Bus>,
}

impl BusLayout {
    /// A layout with no buses.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an input bus.
    #[must_use]
    pub fn with_input(mut self, name: impl Into<String>, channels: ChannelSet) -> Self {
        self.inputs.push(Bus::new(name, channels));
        self
    }

    /// Appends an output bus.
    #[must_use]
    pub fn with_output(mut self, name: impl Into<String>, channels: ChannelSet) -> Self {
        self.outputs.push(Bus::new(name, channels));
        self
    }

    /// The input buses in order.
    #[must_use]
    pub fn inputs(&self) -> &[Bus] {
        &self.inputs
    }

    /// The output buses in order.
    #[must_use]
    pub fn outputs(&self) -> &[Bus] {
        &self.outputs
    }

    /// Replaces the channel set of input bus `index`. Panics when `index` is out of range.
    pub fn set_input(&mut self, index: usize, channels: ChannelSet) {
        self.inputs[index].channels = channels;
    }

    /// Replaces the channel set of output bus `index`. Panics when `index` is out of range.
    pub fn set_output(&mut self, index: usize, channels: ChannelSet) {
        self.outputs[index].channels = channels;
    }

    /// Total input channels over all buses.
    #[must_use]
    pub fn input_channels(&self) -> usize {
        self.inputs.iter().map(Bus::channel_count).sum()
    }

    /// Total output channels over all buses.
    #[must_use]
    pub fn output_channels(&self) -> usize {
        self.outputs.iter().map(Bus::channel_count).sum()
    }

    /// True when both layouts have the same channel sets in the same order, whatever the names.
    /// ```
    /// use syrp_audio_api::{BusLayout, ChannelSet};
    /// let a = BusLayout::new().with_output("Out", ChannelSet::stereo());
    /// let b = BusLayout::new().with_output("Main", ChannelSet::stereo());
    /// assert!(a.same_channel_sets(&b));
    /// ```
    #[must_use]
    pub fn same_channel_sets(&self, other: &BusLayout) -> bool {
        let sets = |b: &[Bus]| b.iter().map(Bus::channel_set).collect::<Vec<_>>();
        sets(&self.inputs) == sets(&other.inputs) && sets(&self.outputs) == sets(&other.outputs)
    }
}

/// A processor's answer when a host or graph proposes a [`BusLayout`].
/// ```
/// use syrp_audio_api::{BusLayout, ChannelSet, LayoutResponse};
/// fn negotiate(proposed: &BusLayout) -> LayoutResponse {
///     if proposed.outputs().iter().all(|b| b.channel_count() <= 2) {
///         LayoutResponse::Accepted
///     } else {
///         LayoutResponse::Proposed(BusLayout::new().with_output("Main", ChannelSet::stereo()))
///     }
/// }
/// let big = BusLayout::new().with_output("Main", ChannelSet::surround_5_1());
/// assert!(matches!(negotiate(&big), LayoutResponse::Proposed(_)));
/// ```
#[doc(alias = "isBusesLayoutSupported")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LayoutResponse {
    /// The proposed layout is supported as it is.
    Accepted,
    /// Not supported; this nearby layout is, and the proposer may retry with it.
    Proposed(BusLayout),
    /// Not supported, with no alternative to offer.
    Rejected,
}
