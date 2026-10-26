//! `NormalisableRange<T>` (spec B2).

use crate::{Float, MathsError, Range};

/// A custom mapping function. It receives the range's start, end and the value to map, in
/// that order, and returns the mapped value.
///
/// It is a plain function pointer, so the range stays `Copy`. Closures that capture nothing
/// coerce to it; closures that capture variables cannot be used. A mapping that needs extra
/// state must encode it in constants or in the start and end it is passed. The range is [`RtSafe`](syrp_rt::RtSafe) on the understanding that these
/// functions are too: they must not allocate, lock or block if the range is used on an audio
/// thread.
///
/// ```
/// use syrp_maths::MapFn;
/// let halve: MapFn<f32> = |_start, _end, v| v / 2.0;
/// assert_eq!(halve(0.0, 1.0, 0.5), 0.25);
/// ```
pub type MapFn<T> = fn(T, T, T) -> T;

/// A value range that maps to and from the normalised range `0..=1`.
///
/// It has a start, an end, an interval (0 means continuous) and a skew. A skew of 1 maps
/// linearly; a skew below 1 spends more of `0..1` on the low end of the range, and above 1 on
/// the high end. With a symmetric skew, the skew applies outwards from the middle of the range
/// in both directions instead. Normalised position `p` maps through `p^(1/skew)` (or its
/// mirror image about 0.5 when symmetric), so the skew mapping is this crate's own and has not
/// yet been compared with JUCE's in the parity harness.
///
/// Custom [`MapFn`]s, when set, replace the built-in conversions entirely.
///
/// ```
/// use syrp_maths::NormalisableRange;
/// let r = NormalisableRange::with_centre(20.0_f64, 20_000.0, 1_000.0)?;
/// assert!((r.convert_from_0to1(0.5) - 1_000.0).abs() < 1e-6);
/// assert!((r.convert_to_0to1(r.convert_from_0to1(0.3)) - 0.3).abs() < 1e-12);
/// # Ok::<(), syrp_maths::MathsError>(())
/// ```
#[derive(Debug, Clone, Copy)]
pub struct NormalisableRange<T> {
    start: T,
    end: T,
    interval: T,
    skew: T,
    symmetric_skew: bool,
    from_0to1: Option<MapFn<T>>,
    to_0to1: Option<MapFn<T>>,
    snap: Option<MapFn<T>>,
}

fn finite(x: f64) -> bool {
    x.is_finite()
}

impl<T: Float> NormalisableRange<T> {
    /// A continuous, linear range from `start` to `end`.
    ///
    /// # Errors
    /// [`MathsError::InvalidRange`] unless both ends are finite and `start < end`.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(0.0_f32, 10.0)?;
    /// assert_eq!(r.convert_from_0to1(0.25), 2.5);
    /// assert!(NormalisableRange::new(1.0_f32, 1.0).is_err());
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    pub fn new(start: T, end: T) -> Result<Self, MathsError> {
        if !(finite(start.to_f64()) && finite(end.to_f64()) && start < end) {
            return Err(MathsError::InvalidRange);
        }
        Ok(Self {
            start,
            end,
            interval: T::default(),
            skew: T::from_f64(1.0),
            symmetric_skew: false,
            from_0to1: None,
            to_0to1: None,
            snap: None,
        })
    }

    /// A range whose skew makes normalised 0.5 map to `centre`.
    ///
    /// # Errors
    /// [`MathsError::InvalidRange`] unless `start < centre < end`, all finite.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::with_centre(0.0_f32, 100.0, 10.0)?;
    /// assert!((r.convert_from_0to1(0.5) - 10.0).abs() < 1e-4);
    /// assert!(r.skew() < 1.0);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[doc(alias = "setSkewForCentre")]
    pub fn with_centre(start: T, end: T, centre: T) -> Result<Self, MathsError> {
        Self::new(start, end)?.with_skew_for_centre(centre)
    }

    /// Returns a copy whose skew makes normalised 0.5 map to `centre` (symmetric skew off).
    ///
    /// # Errors
    /// [`MathsError::InvalidRange`] unless `centre` is strictly inside the range.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(0.0_f64, 1.0)?.with_skew_for_centre(0.25)?;
    /// assert!((r.skew() - 0.5).abs() < 1e-12);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[doc(alias = "setSkewForCentre")]
    pub fn with_skew_for_centre(self, centre: T) -> Result<Self, MathsError> {
        if !(self.start < centre && centre < self.end) {
            return Err(MathsError::InvalidRange);
        }
        let p = ((centre - self.start) / (self.end - self.start)).to_f64();
        // Solve 0.5^(1/skew) = p for skew.
        let skew = 0.5_f64.ln() / p.ln();
        Ok(Self {
            skew: T::from_f64(skew),
            symmetric_skew: false,
            ..self
        })
    }

    /// Returns a copy with `interval` as the snapping step (0 means continuous).
    ///
    /// # Errors
    /// [`MathsError::InvalidRange`] if `interval` is negative or not finite.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(0.0_f32, 10.0)?.with_interval(0.5)?;
    /// assert_eq!(r.snap_to_legal_value(3.3), 3.5);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    pub fn with_interval(self, interval: T) -> Result<Self, MathsError> {
        if !(finite(interval.to_f64()) && interval >= T::default()) {
            return Err(MathsError::InvalidRange);
        }
        Ok(Self { interval, ..self })
    }

    /// Returns a copy with the given skew (1 is linear).
    ///
    /// # Errors
    /// [`MathsError::InvalidRange`] unless `skew` is finite and greater than 0.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(0.0_f64, 1.0)?.with_skew(0.5)?;
    /// assert!((r.convert_from_0to1(0.5) - 0.25).abs() < 1e-12);
    /// assert!(NormalisableRange::new(0.0_f64, 1.0)?.with_skew(0.0).is_err());
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    pub fn with_skew(self, skew: T) -> Result<Self, MathsError> {
        if !(finite(skew.to_f64()) && skew > T::default()) {
            return Err(MathsError::InvalidRange);
        }
        Ok(Self { skew, ..self })
    }

    /// Returns a copy with the symmetric-skew flag set to `symmetric`.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(-1.0_f64, 1.0)?.with_skew(0.5)?.with_symmetric_skew(true);
    /// assert!(r.is_symmetric_skew());
    /// assert!(r.convert_from_0to1(0.5).abs() < 1e-12);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[must_use]
    pub fn with_symmetric_skew(self, symmetric: bool) -> Self {
        Self {
            symmetric_skew: symmetric,
            ..self
        }
    }

    /// Returns a copy that maps through `from_0to1` and `to_0to1` instead of the built-in
    /// skewed mapping. Each receives `(start, end, value)`. They are [`MapFn`] pointers:
    /// closures that capture variables do not coerce to them, so put any state a mapping needs
    /// into constants or derive it from `start` and `end`.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(1.0_f64, 100.0)?.with_custom_mapping(
    ///     |s, e, p| s * (e / s).powf(p),
    ///     |s, e, v| (v / s).ln() / (e / s).ln(),
    /// );
    /// assert!((r.convert_from_0to1(0.5) - 10.0).abs() < 1e-9);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[must_use]
    pub fn with_custom_mapping(self, from_0to1: MapFn<T>, to_0to1: MapFn<T>) -> Self {
        Self {
            from_0to1: Some(from_0to1),
            to_0to1: Some(to_0to1),
            ..self
        }
    }

    /// Returns a copy whose [`snap_to_legal_value`](Self::snap_to_legal_value) calls `snap`
    /// with `(start, end, value)` instead of rounding to the interval.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(0.0_f32, 10.0)?.with_custom_snap(|_, _, v| v.floor());
    /// assert_eq!(r.snap_to_legal_value(7.9), 7.0);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[must_use]
    pub fn with_custom_snap(self, snap: MapFn<T>) -> Self {
        Self {
            snap: Some(snap),
            ..self
        }
    }

    /// The lowest value.
    ///
    /// ```
    /// assert_eq!(syrp_maths::NormalisableRange::new(2.0_f32, 4.0)?.start(), 2.0);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    pub fn start(&self) -> T {
        self.start
    }

    /// The highest value.
    ///
    /// ```
    /// assert_eq!(syrp_maths::NormalisableRange::new(2.0_f32, 4.0)?.end(), 4.0);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    pub fn end(&self) -> T {
        self.end
    }

    /// The snapping step; 0 means continuous.
    ///
    /// ```
    /// assert_eq!(syrp_maths::NormalisableRange::new(2.0_f32, 4.0)?.interval(), 0.0);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    pub fn interval(&self) -> T {
        self.interval
    }

    /// The skew factor; 1 is linear.
    ///
    /// ```
    /// assert_eq!(syrp_maths::NormalisableRange::new(2.0_f32, 4.0)?.skew(), 1.0);
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    pub fn skew(&self) -> T {
        self.skew
    }

    /// True when the skew applies outwards from the middle of the range.
    ///
    /// ```
    /// assert!(!syrp_maths::NormalisableRange::new(2.0_f32, 4.0)?.is_symmetric_skew());
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    pub fn is_symmetric_skew(&self) -> bool {
        self.symmetric_skew
    }

    /// The range as a plain [`Range`].
    ///
    /// ```
    /// use syrp_maths::{NormalisableRange, Range};
    /// assert_eq!(NormalisableRange::new(2.0_f32, 4.0)?.range(), Range::new(2.0, 4.0));
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[doc(alias = "getRange")]
    pub fn range(&self) -> Range<T> {
        Range::new(self.start, self.end)
    }

    /// Maps `value` to `0..=1`, clamping values outside the range. A NaN `value` gives NaN
    /// (clamping does not remove it); custom mappings decide for themselves.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(10.0_f32, 20.0)?;
    /// assert_eq!((r.convert_to_0to1(15.0), r.convert_to_0to1(30.0)), (0.5, 1.0));
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[doc(alias = "convertTo0to1")]
    pub fn convert_to_0to1(&self, value: T) -> T {
        if let Some(f) = self.to_0to1 {
            return f(self.start, self.end, value);
        }
        let (s, e) = (self.start.to_f64(), self.end.to_f64());
        let p = ((value.to_f64() - s) / (e - s)).clamp(0.0, 1.0);
        T::from_f64(self.skewed(p, self.skew.to_f64()))
    }

    /// Maps a normalised `proportion` (clamped to `0..=1`) back into the range. It does not
    /// snap to the interval; call [`snap_to_legal_value`](Self::snap_to_legal_value) for that.
    /// A NaN `proportion` gives NaN.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(10.0_f32, 20.0)?;
    /// assert_eq!((r.convert_from_0to1(0.5), r.convert_from_0to1(-1.0)), (15.0, 10.0));
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[doc(alias = "convertFrom0to1")]
    pub fn convert_from_0to1(&self, proportion: T) -> T {
        if let Some(f) = self.from_0to1 {
            return f(self.start, self.end, proportion);
        }
        let (s, e) = (self.start.to_f64(), self.end.to_f64());
        let p = self.skewed(
            proportion.to_f64().clamp(0.0, 1.0),
            1.0 / self.skew.to_f64(),
        );
        T::from_f64(s + (e - s) * p)
    }

    /// Rounds `value` to the nearest multiple of the interval counted from `start` (when the
    /// interval is positive), then clamps it to the range. A NaN `value` gives NaN.
    ///
    /// ```
    /// use syrp_maths::NormalisableRange;
    /// let r = NormalisableRange::new(1.0_f64, 2.0)?.with_interval(0.25)?;
    /// assert_eq!((r.snap_to_legal_value(1.3), r.snap_to_legal_value(9.0)), (1.25, 2.0));
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[doc(alias = "snapToLegalValue")]
    pub fn snap_to_legal_value(&self, value: T) -> T {
        if let Some(f) = self.snap {
            return f(self.start, self.end, value);
        }
        let (s, e, i) = (
            self.start.to_f64(),
            self.end.to_f64(),
            self.interval.to_f64(),
        );
        let mut v = value.to_f64();
        if i > 0.0 {
            v = s + i * ((v - s) / i).round();
        }
        T::from_f64(v.clamp(s, e))
    }

    /// Applies the skew curve with `exponent` to a proportion in `0..=1`. The forward mapping
    /// uses `skew` and the inverse `1 / skew`.
    fn skewed(&self, p: f64, exponent: f64) -> f64 {
        if exponent == 1.0 {
            p
        } else if self.symmetric_skew {
            let d = 2.0 * p - 1.0;
            0.5 + 0.5 * d.signum() * d.abs().powf(exponent)
        } else {
            p.powf(exponent)
        }
    }
}
