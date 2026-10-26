// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Seeded random numbers for reproducible noise inputs (spec B5).

use crate::{TestkitError, zeroed};

/// A small seeded generator (the SplitMix64 sequence). The same seed yields the same values
/// on every platform and in every release of this crate; it is for test inputs, not for
/// cryptography or statistics-grade simulation.
/// ```
/// use syrp_testkit::NoiseRng;
/// let (mut a, mut b) = (NoiseRng::new(42), NoiseRng::new(42));
/// assert_eq!(a.next_u64(), b.next_u64());
/// let x = a.next_f32();
/// assert!((0.0..1.0).contains(&x));
/// ```
#[doc(alias = "Random")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoiseRng {
    state: u64,
}

impl NoiseRng {
    /// A generator seeded with `seed`.
    /// ```
    /// let _rng = syrp_testkit::NoiseRng::new(0);
    /// ```
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 random bits.
    /// ```
    /// assert_eq!(syrp_testkit::NoiseRng::new(0).next_u64(), 0xe220_a839_7b1d_cdaf);
    /// ```
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uniform `f32` in `[0, 1)`, from the top 24 bits of [`NoiseRng::next_u64`].
    /// ```
    /// let x = syrp_testkit::NoiseRng::new(9).next_f32();
    /// assert!((0.0..1.0).contains(&x));
    /// ```
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / (1_u32 << 24) as f32)
    }

    /// A uniform `f32` in `[-amplitude, amplitude)`: one white-noise sample.
    /// ```
    /// let x = syrp_testkit::NoiseRng::new(3).next_bipolar(0.25);
    /// assert!((-0.25..0.25).contains(&x));
    /// ```
    pub fn next_bipolar(&mut self, amplitude: f32) -> f32 {
        (self.next_f32() * 2.0 - 1.0) * amplitude
    }

    /// Fills `out` with white noise of the given peak `amplitude`.
    /// ```
    /// let mut buf = [0.0_f32; 16];
    /// syrp_testkit::NoiseRng::new(1).fill_noise(&mut buf, 0.5);
    /// assert!(buf.iter().all(|s| s.abs() <= 0.5));
    /// ```
    pub fn fill_noise(&mut self, out: &mut [f32], amplitude: f32) {
        out.iter_mut()
            .for_each(|s| *s = self.next_bipolar(amplitude));
    }
}

/// `frames` samples of white noise in `[-amplitude, amplitude)` from a [`NoiseRng`] seeded
/// with `seed`. Fails with `SYRP-TESTKIT-0002` if the buffer cannot be allocated.
/// ```
/// let a = syrp_testkit::white_noise(5, 256, 1.0)?;
/// assert_eq!(a, syrp_testkit::white_noise(5, 256, 1.0)?);
/// assert_ne!(a, syrp_testkit::white_noise(6, 256, 1.0)?);
/// # Ok::<(), syrp_testkit::TestkitError>(())
/// ```
pub fn white_noise(seed: u64, frames: usize, amplitude: f32) -> Result<Vec<f32>, TestkitError> {
    let mut v = zeroed(frames)?;
    NoiseRng::new(seed).fill_noise(&mut v, amplitude);
    Ok(v)
}
