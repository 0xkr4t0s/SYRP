// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Numeric comparators and tolerance profiles (spec B3) and their JSON form (spec B4).

use std::fmt;

use serde::Serialize;
use serde::ser::{SerializeStruct, Serializer};
use syrp_audio_api::AudioBlock;

use crate::{Rendered, TestkitError};

/// Anything the comparators accept: a single channel (`[f32]`, `Vec<f32>`, `[f32; N]`), a
/// multichannel [`AudioBlock`], or a [`Rendered`] output.
/// ```
/// use syrp_testkit::AsAudio;
/// assert_eq!([0.0_f32, 1.0].as_audio().frames(), 2);
/// assert_eq!(vec![0.0_f32; 3].as_audio().channels(), 1);
/// ```
pub trait AsAudio {
    /// A borrowed planar view of the signal.
    /// ```
    /// use syrp_testkit::AsAudio;
    /// assert_eq!([0.5_f32][..].as_audio().channel(0), &[0.5]);
    /// ```
    fn as_audio(&self) -> AudioBlock<'_, f32>;
}

impl AsAudio for [f32] {
    fn as_audio(&self) -> AudioBlock<'_, f32> {
        AudioBlock::new(self, 1, self.len(), self.len()).expect("one channel always fits")
    }
}

impl AsAudio for Vec<f32> {
    fn as_audio(&self) -> AudioBlock<'_, f32> {
        self.as_slice().as_audio()
    }
}

impl<const N: usize> AsAudio for [f32; N] {
    fn as_audio(&self) -> AudioBlock<'_, f32> {
        self.as_slice().as_audio()
    }
}

impl AsAudio for Rendered {
    fn as_audio(&self) -> AudioBlock<'_, f32> {
        self.as_view()
    }
}

impl AsAudio for AudioBlock<'_, f32> {
    fn as_audio(&self) -> AudioBlock<'_, f32> {
        *self
    }
}

/// The peak level of `signal` across all channels in dBFS (full scale = 1.0), or negative
/// infinity for silence. NaN samples are ignored.
/// ```
/// let p = syrp_testkit::peak_dbfs(&[0.0_f32, -0.5, 0.25]);
/// assert!((p - -6.0206).abs() < 1e-3);
/// assert_eq!(syrp_testkit::peak_dbfs(&[0.0_f32; 4]), f64::NEG_INFINITY);
/// ```
#[doc(alias = "Decibels::gainToDecibels")]
pub fn peak_dbfs<A: AsAudio + ?Sized>(signal: &A) -> f64 {
    let block = signal.as_audio();
    let peak = block.iter().flatten().fold(0.0_f32, |m, s| m.max(s.abs()));
    to_db(f64::from(peak))
}

fn to_db(level: f64) -> f64 {
    if level > 0.0 {
        20.0 * level.log10()
    } else {
        f64::NEG_INFINITY
    }
}

/// Error metrics between an actual and an expected signal of the same shape, from
/// [`compare`]. Serialises to the `comparison` object documented on [`Verdict`].
/// ```
/// let c = syrp_testkit::compare(&[0.5_f32, 0.0], &[0.5_f32, 0.1])?;
/// assert!((c.max_abs_error() - 0.1).abs() < 1e-7);
/// assert_eq!(c.max_abs_error_at(), Some((0, 1)));
/// assert!(!c.is_bit_exact());
/// # Ok::<(), syrp_testkit::TestkitError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    channels: usize,
    frames: usize,
    bit_exact: bool,
    max_abs_error: f64,
    max_abs_error_at: Option<(usize, usize)>,
    rms_error: f64,
    peak_dbfs_actual: f64,
    peak_dbfs_expected: f64,
}

/// Compares `actual` with `expected` sample by sample (spec B3). Fails with
/// `SYRP-TESTKIT-0005` when their channel counts or lengths differ.
///
/// A sample pair whose bits differ and whose difference is NaN counts as an infinite error;
/// identical NaNs count as zero.
/// ```
/// let c = syrp_testkit::compare(&[1.0_f32, 1.0], &[1.0_f32, 1.0])?;
/// assert!(c.is_bit_exact());
/// assert_eq!(c.rms_error(), 0.0);
/// assert!(syrp_testkit::compare(&[1.0_f32], &[1.0_f32, 2.0]).is_err());
/// # Ok::<(), syrp_testkit::TestkitError>(())
/// ```
pub fn compare<A, E>(actual: &A, expected: &E) -> Result<Comparison, TestkitError>
where
    A: AsAudio + ?Sized,
    E: AsAudio + ?Sized,
{
    let (a, e) = (actual.as_audio(), expected.as_audio());
    if (a.channels(), a.frames()) != (e.channels(), e.frames()) {
        return Err(TestkitError::ShapeMismatch {
            actual: (a.channels(), a.frames()),
            expected: (e.channels(), e.frames()),
        });
    }
    let mut bit_exact = true;
    let mut max_abs_error = 0.0_f64;
    let mut max_abs_error_at = None;
    let mut sum_sq = 0.0_f64;
    for (ch, (ca, ce)) in a.iter().zip(e.iter()).enumerate() {
        for (frame, (&x, &y)) in ca.iter().zip(ce).enumerate() {
            if x.to_bits() == y.to_bits() {
                continue;
            }
            bit_exact = false;
            let d = (f64::from(x) - f64::from(y)).abs();
            let d = if d.is_nan() { f64::INFINITY } else { d };
            sum_sq += d * d;
            if d > max_abs_error || max_abs_error_at.is_none() {
                max_abs_error = d;
                max_abs_error_at = Some((ch, frame));
            }
        }
    }
    let count = a.channels() * a.frames();
    let rms_error = if count == 0 {
        0.0
    } else {
        (sum_sq / count as f64).sqrt()
    };
    Ok(Comparison {
        channels: a.channels(),
        frames: a.frames(),
        bit_exact,
        max_abs_error,
        max_abs_error_at,
        rms_error,
        peak_dbfs_actual: peak_dbfs(actual),
        peak_dbfs_expected: peak_dbfs(expected),
    })
}

impl Comparison {
    /// Channels compared.
    /// ```
    /// assert_eq!(syrp_testkit::compare(&[0.0_f32], &[0.0_f32])?.channels(), 1);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Frames compared per channel.
    /// ```
    /// assert_eq!(syrp_testkit::compare(&[0.0_f32; 3], &[0.0_f32; 3])?.frames(), 3);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// True when every sample pair has identical bits.
    /// ```
    /// assert!(!syrp_testkit::compare(&[0.0_f32], &[-0.0_f32])?.is_bit_exact());
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn is_bit_exact(&self) -> bool {
        self.bit_exact
    }

    /// The largest absolute sample difference.
    /// ```
    /// assert_eq!(syrp_testkit::compare(&[0.0_f32, 2.0], &[0.0_f32, 1.0])?.max_abs_error(), 1.0);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn max_abs_error(&self) -> f64 {
        self.max_abs_error
    }

    /// `(channel, frame)` of the first sample with the largest error, or `None` when
    /// bit-exact.
    /// ```
    /// assert_eq!(syrp_testkit::compare(&[0.0_f32], &[0.0_f32])?.max_abs_error_at(), None);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn max_abs_error_at(&self) -> Option<(usize, usize)> {
        self.max_abs_error_at
    }

    /// Root mean square of the sample differences over all channels and frames.
    /// ```
    /// let c = syrp_testkit::compare(&[0.0_f32; 4], &[1.0_f32, 1.0, -1.0, -1.0])?;
    /// assert_eq!(c.rms_error(), 1.0);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn rms_error(&self) -> f64 {
        self.rms_error
    }

    /// [`Comparison::rms_error`] in dBFS; negative infinity when it is zero.
    /// ```
    /// let c = syrp_testkit::compare(&[0.0_f32], &[0.1_f32])?;
    /// assert!((c.rms_error_dbfs() - -20.0).abs() < 1e-5);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn rms_error_dbfs(&self) -> f64 {
        to_db(self.rms_error)
    }

    /// Peak level of the actual signal in dBFS (see [`peak_dbfs`]).
    /// ```
    /// assert_eq!(syrp_testkit::compare(&[1.0_f32], &[0.5_f32])?.peak_dbfs_actual(), 0.0);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn peak_dbfs_actual(&self) -> f64 {
        self.peak_dbfs_actual
    }

    /// Peak level of the expected signal in dBFS.
    /// ```
    /// assert_eq!(syrp_testkit::compare(&[0.5_f32], &[1.0_f32])?.peak_dbfs_expected(), 0.0);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn peak_dbfs_expected(&self) -> f64 {
        self.peak_dbfs_expected
    }

    /// Actual peak minus expected peak, in dB: 0 when both are silent, infinite when only one
    /// is.
    /// ```
    /// let c = syrp_testkit::compare(&[0.5_f32], &[1.0_f32])?;
    /// assert!((c.peak_dbfs_difference() - -6.0206).abs() < 1e-3);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn peak_dbfs_difference(&self) -> f64 {
        if self.peak_dbfs_actual == self.peak_dbfs_expected {
            0.0
        } else {
            self.peak_dbfs_actual - self.peak_dbfs_expected
        }
    }

    /// Judges these metrics against `tolerance`.
    /// ```
    /// use syrp_testkit::{Tolerance, compare};
    /// let c = compare(&[0.0_f32], &[1e-7_f32])?;
    /// assert!(c.judge(Tolerance::dsp_default()).passed());
    /// assert!(!c.judge(Tolerance::exact()).passed());
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn judge(self, tolerance: Tolerance) -> Verdict {
        Verdict {
            passed: tolerance.accepts(&self),
            comparison: self,
            tolerance,
        }
    }
}

/// The rule a [`Tolerance`] applies.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Rule {
    BitExact,
    MaxAbs(f64),
    RmsDbfs(f64),
}

/// A named tolerance profile (spec B3). The built-in profiles are `exact` (bit-identical),
/// `dsp-default` (max absolute error at most 1e-6) and `dsp-loose` (RMS error at most
/// -96 dBFS). Limits are inclusive; NaN errors never pass.
/// ```
/// use syrp_testkit::Tolerance;
/// assert_eq!(Tolerance::by_name("dsp-loose"), Some(Tolerance::dsp_loose()));
/// assert_eq!(Tolerance::dsp_default().name(), "dsp-default");
/// assert!(Tolerance::by_name("nope").is_none());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    name: &'static str,
    rule: Rule,
}

impl Tolerance {
    /// `exact`: every sample bit-identical.
    /// ```
    /// assert_eq!(syrp_testkit::Tolerance::exact().name(), "exact");
    /// ```
    #[must_use]
    pub const fn exact() -> Self {
        Self {
            name: "exact",
            rule: Rule::BitExact,
        }
    }

    /// `dsp-default`: max absolute error at most 1e-6.
    /// ```
    /// assert_eq!(syrp_testkit::Tolerance::dsp_default().limit(), Some(1e-6));
    /// ```
    #[must_use]
    pub const fn dsp_default() -> Self {
        Self {
            name: "dsp-default",
            rule: Rule::MaxAbs(1e-6),
        }
    }

    /// `dsp-loose`: RMS error at most -96 dBFS.
    /// ```
    /// assert_eq!(syrp_testkit::Tolerance::dsp_loose().limit(), Some(-96.0));
    /// ```
    #[must_use]
    pub const fn dsp_loose() -> Self {
        Self {
            name: "dsp-loose",
            rule: Rule::RmsDbfs(-96.0),
        }
    }

    /// A custom profile, named `max-abs`, with max absolute error at most `limit`.
    /// ```
    /// assert_eq!(syrp_testkit::Tolerance::max_abs(1e-3).name(), "max-abs");
    /// ```
    #[must_use]
    pub const fn max_abs(limit: f64) -> Self {
        Self {
            name: "max-abs",
            rule: Rule::MaxAbs(limit),
        }
    }

    /// A custom profile, named `rms-dbfs`, with RMS error at most `limit_db` dBFS.
    /// ```
    /// assert_eq!(syrp_testkit::Tolerance::rms_dbfs(-120.0).limit(), Some(-120.0));
    /// ```
    #[must_use]
    pub const fn rms_dbfs(limit_db: f64) -> Self {
        Self {
            name: "rms-dbfs",
            rule: Rule::RmsDbfs(limit_db),
        }
    }

    /// The built-in profile called `name`, if there is one.
    /// ```
    /// assert_eq!(syrp_testkit::Tolerance::by_name("exact"), Some(syrp_testkit::Tolerance::exact()));
    /// ```
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        [Self::exact(), Self::dsp_default(), Self::dsp_loose()]
            .into_iter()
            .find(|t| t.name == name)
    }

    /// The profile's name.
    /// ```
    /// assert_eq!(syrp_testkit::Tolerance::dsp_loose().name(), "dsp-loose");
    /// ```
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The numeric limit (absolute error, or dBFS for RMS rules); `None` for `exact`.
    /// ```
    /// assert_eq!(syrp_testkit::Tolerance::exact().limit(), None);
    /// ```
    #[must_use]
    pub fn limit(&self) -> Option<f64> {
        match self.rule {
            Rule::BitExact => None,
            Rule::MaxAbs(l) | Rule::RmsDbfs(l) => Some(l),
        }
    }

    /// The JSON `criterion` string: `bit_exact`, `max_abs` or `rms_dbfs`.
    fn criterion(&self) -> &'static str {
        match self.rule {
            Rule::BitExact => "bit_exact",
            Rule::MaxAbs(_) => "max_abs",
            Rule::RmsDbfs(_) => "rms_dbfs",
        }
    }

    /// Whether `c` is within this tolerance.
    /// ```
    /// let c = syrp_testkit::compare(&[0.0_f32], &[1e-5_f32])?;
    /// assert!(!syrp_testkit::Tolerance::dsp_default().accepts(&c));
    /// assert!(syrp_testkit::Tolerance::max_abs(1e-4).accepts(&c));
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn accepts(&self, c: &Comparison) -> bool {
        match self.rule {
            Rule::BitExact => c.bit_exact,
            Rule::MaxAbs(l) => c.max_abs_error <= l,
            Rule::RmsDbfs(l) => c.rms_error_dbfs() <= l,
        }
    }
}

/// A [`Comparison`] judged against a [`Tolerance`], from [`Comparison::judge`] or
/// [`assert_close`]. Its `Serialize` output (spec B4) has this shape, with every non-finite
/// number (silence in dBFS, a NaN error) written as `null`:
///
/// ```json
/// {
///   "schema": "syrp.testkit.verdict/v1",
///   "passed": true,
///   "tolerance": { "name": "dsp-default", "criterion": "max_abs", "limit": 1e-6 },
///   "comparison": {
///     "channels": 2, "frames": 480, "bit_exact": false,
///     "max_abs_error": 1.2e-7, "max_abs_error_at": { "channel": 0, "frame": 12 },
///     "rms_error": 3.1e-8, "rms_error_dbfs": -150.2,
///     "peak_dbfs_actual": -6.02, "peak_dbfs_expected": -6.02, "peak_dbfs_difference": 0.0
///   }
/// }
/// ```
/// `criterion` is `bit_exact` (with `limit` null), `max_abs` or `rms_dbfs`;
/// `max_abs_error_at` is null when bit-exact.
/// ```
/// let v = syrp_testkit::assert_close(&[0.5_f32], &[0.5_f32], syrp_testkit::Tolerance::exact());
/// let json = serde_json::to_value(&v).unwrap();
/// assert_eq!(json["schema"], "syrp.testkit.verdict/v1");
/// assert_eq!(json["comparison"]["rms_error_dbfs"], serde_json::Value::Null);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    comparison: Comparison,
    tolerance: Tolerance,
    passed: bool,
}

impl Verdict {
    /// Whether the comparison is within the tolerance.
    /// ```
    /// let c = syrp_testkit::compare(&[0.0_f32], &[0.0_f32])?;
    /// assert!(c.judge(syrp_testkit::Tolerance::exact()).passed());
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn passed(&self) -> bool {
        self.passed
    }

    /// The metrics judged.
    /// ```
    /// let c = syrp_testkit::compare(&[0.0_f32], &[0.0_f32])?;
    /// assert_eq!(c.judge(syrp_testkit::Tolerance::exact()).comparison(), &c);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn comparison(&self) -> &Comparison {
        &self.comparison
    }

    /// The tolerance applied.
    /// ```
    /// let c = syrp_testkit::compare(&[0.0_f32], &[0.0_f32])?;
    /// assert_eq!(c.judge(syrp_testkit::Tolerance::exact()).tolerance().name(), "exact");
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn tolerance(&self) -> Tolerance {
        self.tolerance
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            comparison: c,
            tolerance: t,
            passed,
        } = self;
        write!(
            f,
            "{} under `{}`: {} ch x {} frames, max |err| {:e}",
            if *passed { "PASS" } else { "FAIL" },
            t.name,
            c.channels,
            c.frames,
            c.max_abs_error,
        )?;
        if let Some((ch, frame)) = c.max_abs_error_at {
            write!(f, " at channel {ch} frame {frame}")?;
        }
        write!(
            f,
            ", RMS err {:.2} dBFS, peak diff {:.2} dB",
            c.rms_error_dbfs(),
            c.peak_dbfs_difference()
        )
    }
}

/// Compares `actual` with `expected` and panics with a summary unless they agree within
/// `tolerance` (spec B3). Returns the [`Verdict`] so a test can also record it.
///
/// # Panics
/// If the shapes differ or the comparison fails the tolerance.
/// ```
/// use syrp_testkit::{Tolerance, assert_close};
/// assert_close(&[0.5_f32, 0.25], &[0.5_f32, 0.2500001], Tolerance::dsp_default());
/// ```
/// ```should_panic
/// syrp_testkit::assert_close(&[0.5_f32], &[0.6_f32], syrp_testkit::Tolerance::dsp_loose());
/// ```
#[track_caller]
pub fn assert_close<A, E>(actual: &A, expected: &E, tolerance: Tolerance) -> Verdict
where
    A: AsAudio + ?Sized,
    E: AsAudio + ?Sized,
{
    let verdict = match compare(actual, expected) {
        Ok(c) => c.judge(tolerance),
        Err(e) => panic!("assert_close: {e}"),
    };
    assert!(verdict.passed(), "assert_close: {verdict}");
    verdict
}

/// A number written as JSON `null` when it is not finite.
struct Finite(f64);

impl Serialize for Finite {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.0.is_finite() {
            s.serialize_f64(self.0)
        } else {
            s.serialize_none()
        }
    }
}

/// A `(channel, frame)` position as `{"channel": n, "frame": n}`.
struct At(usize, usize);

impl Serialize for At {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("At", 2)?;
        st.serialize_field("channel", &self.0)?;
        st.serialize_field("frame", &self.1)?;
        st.end()
    }
}

impl Serialize for Comparison {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            channels,
            frames,
            bit_exact,
            max_abs_error,
            max_abs_error_at,
            rms_error,
            peak_dbfs_actual,
            peak_dbfs_expected,
        } = *self;
        let mut st = s.serialize_struct("Comparison", 10)?;
        st.serialize_field("channels", &channels)?;
        st.serialize_field("frames", &frames)?;
        st.serialize_field("bit_exact", &bit_exact)?;
        st.serialize_field("max_abs_error", &Finite(max_abs_error))?;
        st.serialize_field("max_abs_error_at", &max_abs_error_at.map(|(c, f)| At(c, f)))?;
        st.serialize_field("rms_error", &Finite(rms_error))?;
        st.serialize_field("rms_error_dbfs", &Finite(self.rms_error_dbfs()))?;
        st.serialize_field("peak_dbfs_actual", &Finite(peak_dbfs_actual))?;
        st.serialize_field("peak_dbfs_expected", &Finite(peak_dbfs_expected))?;
        st.serialize_field("peak_dbfs_difference", &Finite(self.peak_dbfs_difference()))?;
        st.end()
    }
}

impl Serialize for Tolerance {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self { name, rule: _ } = *self;
        let mut st = s.serialize_struct("Tolerance", 3)?;
        st.serialize_field("name", name)?;
        st.serialize_field("criterion", self.criterion())?;
        st.serialize_field("limit", &self.limit().map(Finite))?;
        st.end()
    }
}

impl Serialize for Verdict {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            comparison,
            tolerance,
            passed,
        } = self;
        let mut st = s.serialize_struct("Verdict", 4)?;
        st.serialize_field("schema", "syrp.testkit.verdict/v1")?;
        st.serialize_field("passed", passed)?;
        st.serialize_field("tolerance", tolerance)?;
        st.serialize_field("comparison", comparison)?;
        st.end()
    }
}
