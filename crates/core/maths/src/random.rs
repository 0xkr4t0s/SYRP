//! `Random` (spec B4).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// A seedable pseudo-random number generator.
///
/// The algorithm is SplitMix64: a 64-bit counter advanced by the odd constant
/// `0x9E37_79B9_7F4A_7C15` on each step, then scrambled by two xor-shift-multiply rounds. It is
/// fast, has a period of 2^64, and is defined entirely by integer arithmetic, so the same seed
/// gives the same sequence on every platform and in every release of this crate (a test pins
/// the first outputs). It is not cryptographically secure, and its sequences do not match
/// JUCE's.
///
/// ```
/// use syrp_maths::Random;
/// let mut a = Random::new(42);
/// let mut b = Random::new(42);
/// assert_eq!(a.next_i64(), b.next_i64());
/// let x = a.next_f64();
/// assert!((0.0..1.0).contains(&x));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Random {
    state: u64,
}

const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

impl Random {
    /// A generator that starts from `seed`.
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(7);
    /// let first = r.next_i32();
    /// assert_eq!(syrp_maths::Random::new(7).next_i32(), first);
    /// ```
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// A generator seeded from the operating system's randomness (through the standard
    /// library's hash-map keys) mixed with the current time. Allocation-free in practice, but
    /// not meant for the audio thread.
    ///
    /// ```
    /// let mut a = syrp_maths::Random::with_entropy();
    /// let mut b = syrp_maths::Random::with_entropy();
    /// assert_ne!((a.next_i64(), a.next_i64()), (b.next_i64(), b.next_i64()));
    /// ```
    pub fn with_entropy() -> Self {
        let mut hasher = RandomState::new().build_hasher();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        hasher.write_u128(nanos);
        Self::new(hasher.finish())
    }

    /// Restarts the sequence from `seed`.
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(1);
    /// let first = r.next_i64();
    /// r.set_seed(1);
    /// assert_eq!(r.next_i64(), first);
    /// ```
    #[doc(alias = "setSeed")]
    pub fn set_seed(&mut self, seed: u64) {
        self.state = seed;
    }

    /// The next 64 random bits.
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(0);
    /// assert_ne!(r.next_u64(), r.next_u64());
    /// ```
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A random `i32` over its whole range (the upper 32 bits of [`next_u64`](Self::next_u64)).
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(3);
    /// let _any: i32 = r.next_i32();
    /// ```
    #[doc(alias = "nextInt")]
    #[allow(clippy::cast_possible_truncation)]
    pub fn next_i32(&mut self) -> i32 {
        ((self.next_u64() >> 32) as u32).cast_signed()
    }

    /// A uniformly distributed value in `0..n`, without modulo bias. Returns 0 when `n <= 0`.
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(5);
    /// assert!((0..6).contains(&r.next_i32_below(6)));
    /// assert_eq!(r.next_i32_below(0), 0);
    /// ```
    #[doc(alias = "nextInt")]
    #[allow(clippy::cast_possible_truncation)]
    pub fn next_i32_below(&mut self, n: i32) -> i32 {
        if n <= 0 {
            return 0;
        }
        let n = u64::from(n.cast_unsigned());
        // Multiply-shift maps 32 random bits onto 0..n; rejecting the few low products that
        // would over-represent some results removes the bias.
        let threshold = (1u64 << 32) % n;
        loop {
            let product = (self.next_u64() >> 32) * n;
            if product & 0xFFFF_FFFF >= threshold {
                return (product >> 32) as i32;
            }
        }
    }

    /// A random `i64` over its whole range.
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(9);
    /// let _any: i64 = r.next_i64();
    /// ```
    #[doc(alias = "nextInt64")]
    pub fn next_i64(&mut self) -> i64 {
        self.next_u64().cast_signed()
    }

    /// A random `bool`, each with probability one half.
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(11);
    /// let heads = (0..1000).filter(|_| r.next_bool()).count();
    /// assert!((400..600).contains(&heads));
    /// ```
    #[doc(alias = "nextBool")]
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() >> 63 == 1
    }

    /// A uniformly distributed `f32` in `[0, 1)`, with 24 random bits.
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(13);
    /// assert!((0.0..1.0).contains(&r.next_f32()));
    /// ```
    #[doc(alias = "nextFloat")]
    #[allow(clippy::cast_precision_loss)]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / (1u32 << 24) as f32)
    }

    /// A uniformly distributed `f64` in `[0, 1)`, with 53 random bits.
    ///
    /// ```
    /// let mut r = syrp_maths::Random::new(17);
    /// assert!((0.0..1.0).contains(&r.next_f64()));
    /// ```
    #[doc(alias = "nextDouble")]
    #[allow(clippy::cast_precision_loss)]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Fills `bytes` with random bytes (little-endian chunks of [`next_u64`](Self::next_u64)).
    ///
    /// ```
    /// let mut buf = [0u8; 13];
    /// syrp_maths::Random::new(19).fill_bytes(&mut buf);
    /// assert!(buf.iter().any(|&b| b != 0));
    /// ```
    #[doc(alias = "fillBitsRandomly")]
    pub fn fill_bytes(&mut self, bytes: &mut [u8]) {
        for chunk in bytes.chunks_mut(8) {
            let word = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
    }
}

/// The process-wide shared generator, seeded with [`Random::with_entropy`] on first use.
///
/// It sits behind a lock, so hold the guard briefly and never call this on the audio thread
/// (keep a [`Random`] of your own there). A poisoned lock is recovered: the generator has no
/// invariant a panic could break.
///
/// ```
/// let x = syrp_maths::system_random().next_f64();
/// assert!((0.0..1.0).contains(&x));
/// ```
#[doc(alias = "getSystemRandom")]
pub fn system_random() -> MutexGuard<'static, Random> {
    static SYSTEM: OnceLock<Mutex<Random>> = OnceLock::new();
    SYSTEM
        .get_or_init(|| Mutex::new(Random::with_entropy()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}
