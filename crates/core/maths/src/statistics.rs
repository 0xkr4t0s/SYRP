//! `StatisticsAccumulator<T>` (spec B5).

use crate::Float;

/// Running count, mean, variance, minimum and maximum of a stream of values.
///
/// It uses Welford's update in `f64`: each new value moves the mean by its share of the
/// difference and adds to a sum of squared deviations from the running mean. Unlike summing
/// values and their squares, this does not lose the variance to cancellation when the values
/// are large and close together. Variance is the population variance (divided by the count).
///
/// ```
/// use syrp_maths::StatisticsAccumulator;
/// let mut s = StatisticsAccumulator::<f64>::new();
/// for v in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] { s.add(v); }
/// assert_eq!((s.count(), s.average(), s.variance(), s.standard_deviation()), (8, 5.0, 4.0, 2.0));
/// assert_eq!((s.min(), s.max()), (Some(2.0), Some(9.0)));
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StatisticsAccumulator<T> {
    count: u64,
    mean: f64,
    squared_deviations: f64,
    min: Option<T>,
    max: Option<T>,
}

impl<T: Float> StatisticsAccumulator<T> {
    /// An empty accumulator.
    ///
    /// ```
    /// let s = syrp_maths::StatisticsAccumulator::<f32>::new();
    /// assert_eq!(s.count(), 0);
    /// ```
    pub const fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            squared_deviations: 0.0,
            min: None,
            max: None,
        }
    }

    /// Adds one value. NaN values make the average and variance NaN.
    ///
    /// ```
    /// let mut s = syrp_maths::StatisticsAccumulator::new();
    /// s.add(1.5_f32);
    /// assert_eq!(s.average(), 1.5);
    /// ```
    #[doc(alias = "addValue")]
    #[allow(clippy::cast_precision_loss)]
    pub fn add(&mut self, value: T) {
        self.count += 1;
        let v = value.to_f64();
        let delta = v - self.mean;
        self.mean += delta / self.count as f64;
        self.squared_deviations += delta * (v - self.mean);
        self.min = Some(match self.min {
            Some(m) if m <= value => m,
            _ => value,
        });
        self.max = Some(match self.max {
            Some(m) if m >= value => m,
            _ => value,
        });
    }

    /// Forgets every value added so far.
    ///
    /// ```
    /// let mut s = syrp_maths::StatisticsAccumulator::new();
    /// s.add(3.0_f64);
    /// s.reset();
    /// assert_eq!((s.count(), s.max()), (0, None));
    /// ```
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// The mean of the values added, or 0 when none have been.
    ///
    /// ```
    /// let mut s = syrp_maths::StatisticsAccumulator::new();
    /// s.add(1.0_f64);
    /// s.add(2.0);
    /// assert_eq!(s.average(), 1.5);
    /// ```
    #[doc(alias = "getAverage")]
    pub fn average(&self) -> T {
        T::from_f64(self.mean)
    }

    /// The population variance, or 0 for fewer than two values.
    ///
    /// ```
    /// let mut s = syrp_maths::StatisticsAccumulator::new();
    /// s.add(1.0_f64);
    /// s.add(3.0);
    /// assert_eq!(s.variance(), 1.0);
    /// ```
    #[doc(alias = "getVariance")]
    #[allow(clippy::cast_precision_loss)]
    pub fn variance(&self) -> T {
        if self.count < 2 {
            return T::default();
        }
        T::from_f64(self.squared_deviations / self.count as f64)
    }

    /// The square root of [`variance`](Self::variance).
    ///
    /// ```
    /// let mut s = syrp_maths::StatisticsAccumulator::new();
    /// s.add(1.0_f64);
    /// s.add(3.0);
    /// assert_eq!(s.standard_deviation(), 1.0);
    /// ```
    #[doc(alias = "getStandardDeviation")]
    pub fn standard_deviation(&self) -> T {
        T::from_f64(self.variance().to_f64().sqrt())
    }

    /// The smallest value added, or `None` when none have been.
    ///
    /// ```
    /// let mut s = syrp_maths::StatisticsAccumulator::new();
    /// s.add(2.0_f32);
    /// s.add(-1.0);
    /// assert_eq!(s.min(), Some(-1.0));
    /// ```
    #[doc(alias = "getMinValue")]
    pub fn min(&self) -> Option<T> {
        self.min
    }

    /// The largest value added, or `None` when none have been.
    ///
    /// ```
    /// let mut s = syrp_maths::StatisticsAccumulator::new();
    /// s.add(2.0_f32);
    /// s.add(-1.0);
    /// assert_eq!(s.max(), Some(2.0));
    /// ```
    #[doc(alias = "getMaxValue")]
    pub fn max(&self) -> Option<T> {
        self.max
    }

    /// How many values have been added.
    ///
    /// ```
    /// let mut s = syrp_maths::StatisticsAccumulator::new();
    /// s.add(0.0_f64);
    /// assert_eq!(s.count(), 1);
    /// ```
    #[doc(alias = "getCount")]
    pub fn count(&self) -> u64 {
        self.count
    }
}
