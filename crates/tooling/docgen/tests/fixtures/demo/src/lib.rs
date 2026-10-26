//! Fixture crate for syrp-docgen tests (not rustfmt-formatted, to stay short).

/// Adds two numbers.
///
/// Second paragraph, not part of the summary.
#[doc(alias = "addNumbers")]
pub fn add(a: u32, b: u32) -> u32 { a + b }

/// A gain stage
/// over two lines.
#[doc(alias("AudioGain", "juce::dsp::Gain"))]
pub struct Gain<T: Copy> { /** Linear gain. */ pub value: T, hidden: u8 }

impl<T: Copy> Gain<T> {
    /// Makes a new gain.
    pub fn new(value: T) -> Self { Self { value, hidden: 0 } }
    /// Returns the gain.
    pub const fn get(&self) -> T { self.value }
    /// Applies the gain to anything iterable.
    pub fn apply<'s, I>(&mut self, items: impl IntoIterator<Item = &'s mut f32>, _extra: I)
    where I: Clone + Send { let _ = (items, self.hidden); }
    fn private_helper(&self) {}
}

impl<T: Copy> Clone for Gain<T> {
    fn clone(&self) -> Self { Self { value: self.value, hidden: self.hidden } }
}

/// Things that process audio.
pub trait Process: Send {
    /// The sample type.
    type Sample: Copy;
    /// Number of channels.
    const CHANNELS: usize = 2;
    /// Processes a buffer.
    fn process(&mut self, buf: &mut [Self::Sample]);
}

/// Processing mode.
pub enum Mode { /** Fast. */ Fast, /** Fixed rate. */ Rate(f64), /** Custom. */ Custom { /** Name. */ name: &'static str } }

/// A sample index.
pub struct Index(pub usize);
/// Max channels.
pub const MAX: usize = 4;
/// Global counter.
pub static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Old name.
#[deprecated(since = "0.2.0", note = "use `add`")]
pub fn old_add(a: u32) -> u32 { a }
/// Makes a gain.
#[macro_export]
macro_rules! gain { ($v:expr) => { $crate::Gain::new($v) }; }

/// Sub module.
pub mod sub {
    /// Alias type.
    pub type Sample = f32;
    /// Callback type.
    pub type Callback = fn(&[f32]) -> Option<Box<dyn Fn(f32) -> f32 + Send>>;
}

mod hidden { /** Re-exported under the crate root as `Renamed`. */ pub struct Inner; }
pub use hidden::Inner as Renamed;
#[allow(dead_code)]
fn private() {}
