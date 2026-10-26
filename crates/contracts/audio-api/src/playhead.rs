// SPDX-License-Identifier: MIT OR Apache-2.0
//! Transport position reported by a host (spec B4).

use syrp_rt::RtSafe;

/// A musical time signature, such as 7/8.
/// ```
/// let ts = syrp_audio_api::TimeSignature::new(7, 8).unwrap();
/// assert_eq!((ts.numerator(), ts.denominator()), (7, 8));
/// assert!(syrp_audio_api::TimeSignature::new(4, 0).is_none());
/// ```
#[doc(alias = "AudioPlayHead::TimeSignature")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeSignature {
    numerator: u32,
    denominator: u32,
}

impl TimeSignature {
    /// `numerator / denominator`, or `None` if either is zero.
    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            None
        } else {
            Some(TimeSignature {
                numerator,
                denominator,
            })
        }
    }

    /// Beats per bar.
    #[must_use]
    pub const fn numerator(&self) -> u32 {
        self.numerator
    }

    /// The note value of one beat (4 for quarter notes).
    #[must_use]
    pub const fn denominator(&self) -> u32 {
        self.denominator
    }
}

/// A loop range in quarter notes (PPQ).
/// ```
/// let lp = syrp_audio_api::LoopPoints::new(4.0, 12.0);
/// assert_eq!((lp.start_ppq(), lp.end_ppq()), (4.0, 12.0));
/// ```
#[doc(alias = "AudioPlayHead::LoopPoints")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopPoints {
    start_ppq: f64,
    end_ppq: f64,
}

impl LoopPoints {
    /// A loop from `start_ppq` to `end_ppq`.
    #[must_use]
    pub const fn new(start_ppq: f64, end_ppq: f64) -> Self {
        LoopPoints { start_ppq, end_ppq }
    }

    /// Loop start, in quarter notes.
    #[must_use]
    pub const fn start_ppq(&self) -> f64 {
        self.start_ppq
    }

    /// Loop end, in quarter notes.
    #[must_use]
    pub const fn end_ppq(&self) -> f64 {
        self.end_ppq
    }
}

/// Where the host transport is at the start of a block. Every value is optional because
/// hosts report different subsets; the flags default to `false`.
/// ```
/// use syrp_audio_api::{PlayHeadInfo, TimeSignature};
/// let info = PlayHeadInfo::new()
///     .with_bpm(120.0)
///     .with_time_signature(TimeSignature::new(3, 4).unwrap())
///     .with_sample_position(96_000)
///     .with_playing(true);
/// assert_eq!(info.bpm(), Some(120.0));
/// assert_eq!(info.time_in_seconds(48_000.0), Some(2.0));
/// assert!(info.is_playing() && !info.is_recording());
/// assert_eq!(info.ppq_position(), None);
/// ```
#[doc(alias = "AudioPlayHead::PositionInfo")]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayHeadInfo {
    bpm: Option<f64>,
    time_signature: Option<TimeSignature>,
    ppq_position: Option<f64>,
    bar_start_ppq: Option<f64>,
    loop_points: Option<LoopPoints>,
    sample_position: Option<i64>,
    host_time_ns: Option<u64>,
    playing: bool,
    recording: bool,
    looping: bool,
}

/// Generates a `with_` setter and a getter for an optional field.
///
/// A macro because seven optional fields each need the same documented setter/getter pair.
macro_rules! optional {
    ($($field:ident, $with:ident: $t:ty, $doc:literal;)*) => {$(
        #[doc = concat!("The same info with ", $doc, " set.")]
        #[must_use]
        pub const fn $with(mut self, value: $t) -> Self {
            self.$field = Some(value);
            self
        }

        #[doc = concat!("The ", $doc, ", if the host reports it.")]
        #[must_use]
        pub const fn $field(&self) -> Option<$t> {
            self.$field
        }
    )*};
}

impl PlayHeadInfo {
    /// Info with nothing reported and every flag `false`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    optional! {
        bpm, with_bpm: f64, "tempo in beats per minute";
        time_signature, with_time_signature: TimeSignature, "time signature";
        ppq_position, with_ppq_position: f64, "position in quarter notes from the timeline start";
        bar_start_ppq, with_bar_start_ppq: f64, "quarter-note position of the start of the current bar";
        loop_points, with_loop_points: LoopPoints, "loop range";
        sample_position, with_sample_position: i64, "position in samples from the timeline start (negative during pre-roll)";
        host_time_ns, with_host_time_ns: u64, "host system time of the block start, in nanoseconds";
    }

    /// The same info with the playing flag set to `on`.
    #[must_use]
    pub const fn with_playing(mut self, on: bool) -> Self {
        self.playing = on;
        self
    }

    /// The same info with the recording flag set to `on`.
    #[must_use]
    pub const fn with_recording(mut self, on: bool) -> Self {
        self.recording = on;
        self
    }

    /// The same info with the looping flag set to `on`.
    #[must_use]
    pub const fn with_looping(mut self, on: bool) -> Self {
        self.looping = on;
        self
    }

    /// True while the transport is playing.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    /// True while the transport is recording.
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        self.recording
    }

    /// True while looping is switched on.
    #[must_use]
    pub const fn is_looping(&self) -> bool {
        self.looping
    }

    /// The sample position converted to seconds at `sample_rate`.
    #[must_use]
    pub fn time_in_seconds(&self, sample_rate: f64) -> Option<f64> {
        self.sample_position.map(|s| s as f64 / sample_rate)
    }
}

/// A source of transport position, such as a host wrapper or a test transport.
/// ```
/// use syrp_audio_api::{PlayHead, PlayHeadInfo};
/// struct Stopped;
/// impl PlayHead for Stopped {
///     fn position(&self) -> Option<PlayHeadInfo> {
///         Some(PlayHeadInfo::new())
///     }
/// }
/// assert!(!Stopped.position().unwrap().is_playing());
/// ```
#[doc(alias = "AudioPlayHead")]
pub trait PlayHead {
    /// The position at the start of the current block, or `None` if the host gives none.
    /// Must be RT-safe: it is called on the audio thread.
    fn position(&self) -> Option<PlayHeadInfo>;
}

// SAFETY: plain `Copy` data with no destructor; no method allocates, locks or blocks.
unsafe impl RtSafe for TimeSignature {}
// SAFETY: as above.
unsafe impl RtSafe for LoopPoints {}
// SAFETY: as above.
unsafe impl RtSafe for PlayHeadInfo {}
