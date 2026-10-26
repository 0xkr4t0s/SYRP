// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Offline block-wise rendering (spec B1) and the no-allocation helpers (spec B6).

use std::io::{BufWriter, Write as _};
use std::path::Path;

use syrp_audio_api::{AudioBlock, AudioBlockMut, ProcessConfig, ProcessMode};

use crate::{TestkitError, WavFormat, write_wav, zeroed};

/// Scratch channel stride granularity in samples: 64 bytes of `f32`, as the audio contract
/// recommends for SIMD kernels.
const STRIDE_STEP: usize = 16;

/// The output of [`render_offline`]: planar `f32` channels plus the configuration they were
/// rendered with. Storage is one allocation; borrow it with [`Rendered::as_view`].
/// ```
/// let out = syrp_testkit::render_offline(|_, _| {}, &[&[0.25; 10]], 4, 44_100.0)?;
/// assert_eq!((out.channels(), out.frames()), (1, 10));
/// assert_eq!(out.channel(0), &[0.25; 10]);
/// assert_eq!(out.config().max_block_size(), 4);
/// # Ok::<(), syrp_testkit::TestkitError>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    samples: Vec<f32>,
    channels: usize,
    frames: usize,
    config: ProcessConfig,
}

impl Rendered {
    /// Number of channels (always the input's channel count).
    /// ```
    /// let out = syrp_testkit::render_offline(|_, _| {}, &[&[0.0; 3], &[0.0; 3]], 2, 48e3)?;
    /// assert_eq!(out.channels(), 2);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Number of samples per channel (always the input's length).
    /// ```
    /// let out = syrp_testkit::render_offline(|_, _| {}, &[&[0.0; 3]], 2, 48e3)?;
    /// assert_eq!(out.frames(), 3);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The offline configuration the closure was given: the render's sample rate, the block
    /// size as `max_block_size`, and [`ProcessMode::Offline`].
    /// ```
    /// let out = syrp_testkit::render_offline(|_, _| {}, &[&[0.0; 3]], 2, 48e3)?;
    /// assert!(out.config().is_offline());
    /// assert_eq!(out.config().sample_rate(), 48e3);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn config(&self) -> ProcessConfig {
        self.config
    }

    /// Channel `ch`'s samples.
    ///
    /// # Panics
    /// If `ch >= self.channels()`.
    /// ```
    /// let out = syrp_testkit::render_offline(|_, _| {}, &[&[1.0, 2.0]], 1, 48e3)?;
    /// assert_eq!(out.channel(0), &[1.0, 2.0]);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn channel(&self, ch: usize) -> &[f32] {
        assert!(ch < self.channels, "channel {ch} out of range");
        &self.samples[ch * self.frames..(ch + 1) * self.frames]
    }

    /// A borrowed view of every channel; the channel stride equals [`Rendered::frames`].
    /// ```
    /// let out = syrp_testkit::render_offline(|_, _| {}, &[&[1.0], &[2.0]], 1, 48e3)?;
    /// assert_eq!(out.as_view().channel(1), &[2.0]);
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    #[must_use]
    pub fn as_view(&self) -> AudioBlock<'_, f32> {
        AudioBlock::new(&self.samples, self.channels, self.frames, self.frames)
            .expect("Rendered holds exactly channels * frames samples")
    }

    /// Writes the render to `path` as a WAV file in `format` (spec B2), at the render's
    /// sample rate rounded to whole Hz.
    /// ```no_run
    /// let out = syrp_testkit::render_offline(|_, _| {}, &[&[0.0; 480]], 64, 48e3)?;
    /// out.write_wav_file("target/render.wav", syrp_testkit::WavFormat::Float32)?;
    /// # Ok::<(), syrp_testkit::TestkitError>(())
    /// ```
    pub fn write_wav_file(
        &self,
        path: impl AsRef<Path>,
        format: WavFormat,
    ) -> Result<(), TestkitError> {
        let mut file = BufWriter::new(std::fs::File::create(path)?);
        write_wav(&mut file, self.as_view(), self.config.sample_rate(), format)?;
        file.flush()?;
        Ok(())
    }
}

/// Runs `process` over `input` in consecutive blocks of `block_size` frames (the last block
/// may be shorter) and returns the output channels (spec B1).
///
/// Each call receives the offline [`ProcessConfig`] and an [`AudioBlockMut`] holding the next
/// block of input, which the closure overwrites with its output in place. Blocks are handed
/// out in order with no overlap, so the result depends only on the closure, the input and the
/// block size. All buffers are allocated before the first block. Fails with
/// `SYRP-TESTKIT-0001` if channels differ in length, `SYRP-AUDIO-0003`/`-0004` for an invalid
/// sample rate or a zero block size, and `SYRP-TESTKIT-0002` if allocation fails.
/// ```
/// use syrp_testkit::render_offline;
/// let mut sizes = Vec::new();
/// let out = render_offline(
///     |_, mut block| {
///         sizes.push(block.frames());
///         block.channel_mut(0).fill(1.0);
///     },
///     &[&[0.0; 10]],
///     4,
///     48_000.0,
/// )?;
/// assert_eq!(sizes, [4, 4, 2]);
/// assert_eq!(out.channel(0), &[1.0; 10]);
/// # Ok::<(), syrp_testkit::TestkitError>(())
/// ```
#[doc(alias = "processBlock")]
pub fn render_offline<F>(
    process: F,
    input: &[&[f32]],
    block_size: usize,
    sample_rate: f64,
) -> Result<Rendered, TestkitError>
where
    F: FnMut(&ProcessConfig, AudioBlockMut<'_, f32>),
{
    render(process, input, block_size, sample_rate, false)
}

/// [`render_offline`] with every block, the copies in and out included, run inside
/// [`syrp_rt::no_alloc`] (spec B6). Use it for closures that must be real-time safe.
///
/// The guard only bites when the test binary installs [`syrp_rt::GuardedAllocator`] as its
/// `#[global_allocator]` and is a debug build; an allocation then aborts with `SYRP-RT-0001`.
/// ```
/// #[global_allocator]
/// static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;
///
/// let out = syrp_testkit::render_offline_no_alloc(
///     |_, mut block| block.fill(0.5),
///     &[&[0.0; 100]],
///     32,
///     48_000.0,
/// )?;
/// assert_eq!(out.channel(0)[99], 0.5);
/// # Ok::<(), syrp_testkit::TestkitError>(())
/// ```
pub fn render_offline_no_alloc<F>(
    process: F,
    input: &[&[f32]],
    block_size: usize,
    sample_rate: f64,
) -> Result<Rendered, TestkitError>
where
    F: FnMut(&ProcessConfig, AudioBlockMut<'_, f32>),
{
    render(process, input, block_size, sample_rate, true)
}

/// Runs `f` inside [`syrp_rt::no_alloc`] and returns its result (spec B6): the helper for
/// audio tests whose code under test must not allocate. Needs
/// [`syrp_rt::GuardedAllocator`] installed in the test binary to take effect.
/// ```
/// # #[global_allocator]
/// # static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;
/// let mut gain = [1.0_f32; 8];
/// let peak = syrp_testkit::assert_no_alloc(|| {
///     gain.iter_mut().for_each(|g| *g *= 0.5);
///     gain.iter().copied().fold(0.0_f32, f32::max)
/// });
/// assert_eq!(peak, 0.5);
/// ```
pub fn assert_no_alloc<R>(f: impl FnOnce() -> R) -> R {
    syrp_rt::no_alloc(f)
}

fn render<F>(
    mut process: F,
    input: &[&[f32]],
    block_size: usize,
    sample_rate: f64,
    guard: bool,
) -> Result<Rendered, TestkitError>
where
    F: FnMut(&ProcessConfig, AudioBlockMut<'_, f32>),
{
    let config = ProcessConfig::new(sample_rate, block_size)?.with_mode(ProcessMode::Offline);
    let channels = input.len();
    let frames = input.first().map_or(0, |c| c.len());
    if let Some((channel, c)) = input.iter().enumerate().find(|(_, c)| c.len() != frames) {
        return Err(TestkitError::RaggedInput {
            channel,
            len: c.len(),
            expected: frames,
        });
    }
    let overflow = || TestkitError::OutOfMemory {
        samples: usize::MAX,
    };
    let stride = block_size
        .checked_next_multiple_of(STRIDE_STEP)
        .ok_or_else(overflow)?;
    let mut scratch = zeroed(channels.checked_mul(stride).ok_or_else(overflow)?)?;
    let mut samples = zeroed(channels.checked_mul(frames).ok_or_else(overflow)?)?;

    let mut start = 0;
    while start < frames {
        let n = block_size.min(frames - start);
        let mut step = || {
            for (ch, src) in input.iter().enumerate() {
                scratch[ch * stride..ch * stride + n].copy_from_slice(&src[start..start + n]);
            }
            let block = AudioBlockMut::new(&mut scratch, channels, n, stride)
                .expect("scratch holds channels * stride samples and n <= stride");
            process(&config, block);
            for ch in 0..channels {
                samples[ch * frames + start..ch * frames + start + n]
                    .copy_from_slice(&scratch[ch * stride..ch * stride + n]);
            }
        };
        if guard {
            syrp_rt::no_alloc(step);
        } else {
            step();
        }
        start += n;
    }
    Ok(Rendered {
        samples,
        channels,
        frames,
        config,
    })
}
