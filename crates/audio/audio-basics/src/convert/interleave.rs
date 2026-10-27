// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Interleaving between planar blocks and frame-by-frame slices (B3).

use syrp_audio_api::{AudioBlock, AudioBlockMut, AudioSample};

use super::ConvertError;

/// Checks that an interleaved slice of `len` samples matches `channels * frames`.
fn check_samples(len: usize, channels: usize, frames: usize) -> Result<(), ConvertError> {
    let expected = channels.saturating_mul(frames);
    if len == expected {
        Ok(())
    } else {
        Err(ConvertError::SampleCountMismatch {
            expected,
            actual: len,
        })
    }
}

/// Copies the planar `src` into `dest` frame by frame: `dest[i * channels + c]` is sample `i`
/// of channel `c`.
///
/// # Errors
/// [`ConvertError::SampleCountMismatch`] when `dest.len()` is not
/// `src.channels() * src.frames()`. `dest` is untouched on error.
///
/// ```
/// use syrp_audio_api::AudioBlock;
/// let planar = [1.0_f32, 2.0, 3.0, 10.0, 20.0, 30.0];
/// let block = AudioBlock::new(&planar, 2, 3, 3)?;
/// let mut out = [0.0; 6];
/// syrp_audio_basics::convert::interleave(&block, &mut out)?;
/// assert_eq!(out, [1.0, 10.0, 2.0, 20.0, 3.0, 30.0]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[doc(alias = "AudioData::interleaveSamples")]
pub fn interleave<T: AudioSample>(
    src: &AudioBlock<'_, T>,
    dest: &mut [T],
) -> Result<(), ConvertError> {
    let channels = src.channels();
    check_samples(dest.len(), channels, src.frames())?;
    if dest.is_empty() {
        return Ok(());
    }
    for c in 0..channels {
        let slots = dest[c..].iter_mut().step_by(channels);
        for (d, &s) in slots.zip(src.channel(c)) {
            *d = s;
        }
    }
    Ok(())
}

/// Copies the frame-by-frame `src` into the planar `dest`: sample `i` of channel `c` is
/// `src[i * channels + c]`.
///
/// # Errors
/// [`ConvertError::SampleCountMismatch`] when `src.len()` is not
/// `dest.channels() * dest.frames()`. `dest` is untouched on error.
///
/// ```
/// use syrp_audio_api::AudioBlockMut;
/// let mut planar = [0.0_f64; 4];
/// let mut block = AudioBlockMut::new(&mut planar, 2, 2, 2)?;
/// syrp_audio_basics::convert::deinterleave(&[1.0, -1.0, 2.0, -2.0], &mut block)?;
/// assert_eq!(block.channel(1), &[-1.0, -2.0]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[doc(alias = "AudioData::deinterleaveSamples")]
pub fn deinterleave<T: AudioSample>(
    src: &[T],
    dest: &mut AudioBlockMut<'_, T>,
) -> Result<(), ConvertError> {
    let channels = dest.channels();
    check_samples(src.len(), channels, dest.frames())?;
    if src.is_empty() {
        return Ok(());
    }
    for c in 0..channels {
        let samples = src[c..].iter().step_by(channels);
        for (d, &s) in dest.channel_mut(c).iter_mut().zip(samples) {
            *d = s;
        }
    }
    Ok(())
}
