// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-01.md, increment 1 (B1 and B2).

use proptest::prelude::*;
use syrp_audio_basics::{AudioBuffer, BufferError, BufferSample};
use syrp_error::Diagnostic;
use syrp_rt::no_alloc;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// Fills channel `c`, frame `i` with `c * 1000 + i + 1`, so every sample is distinct and
/// non-zero.
fn stamp<T: BufferSample>(buf: &mut AudioBuffer<T>, to_t: fn(f64) -> T) {
    for c in 0..buf.channels() {
        for (i, s) in buf.channel_mut(c).iter_mut().enumerate() {
            *s = to_t((c * 1000 + i + 1) as f64);
        }
    }
}

fn stamped(c: usize, i: usize) -> f32 {
    (c * 1000 + i + 1) as f32
}

/// Checks ADR 0003 D2: every channel on a 64-byte boundary, stride a multiple of 64 bytes.
fn assert_layout<T: BufferSample>(buf: &AudioBuffer<T>) {
    let block = buf.as_block();
    assert!(block.is_simd_aligned());
    assert_eq!((block.stride() * size_of::<T>()) % 64, 0);
    assert!(block.stride() >= buf.frames());
    for c in 0..buf.channels() {
        assert_eq!(buf.channel(c).as_ptr() as usize % 64, 0, "channel {c}");
        assert_eq!(buf.channel(c).len(), buf.frames());
    }
}

#[test]
fn b1_skeleton_declares_modules_benches_and_deps() {
    // The four modules exist (control and voice are empty until their WPs land).
    #[allow(unused_imports)]
    use syrp_audio_basics::{buffer, control, vector_ops, voice};
    let manifest = include_str!("../Cargo.toml").replace("\r\n", "\n");
    for dep in [
        "syrp-audio-api",
        "syrp-rt",
        "syrp-simd",
        "syrp-error",
        "proptest",
    ] {
        assert!(manifest.contains(&format!("{dep} = ")), "missing {dep}");
    }
    for bench in ["buffer", "control", "voice_pool"] {
        assert!(manifest.contains(&format!("name = \"{bench}\"\nharness = false")));
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(dir.join("benches/control.rs").exists());
    assert!(dir.join("benches/voice_pool.rs").exists());
    let agents = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    for owner in ["WP-2.1-01", "WP-2.1-03", "WP-2.1-05"] {
        assert!(agents.contains(owner));
    }
}

#[test]
fn b2_new_is_zeroed_and_aligned() {
    for (channels, frames) in [(0, 0), (1, 0), (1, 1), (2, 17), (3, 512), (8, 1000)] {
        let f = AudioBuffer::<f32>::new(channels, frames).unwrap();
        let d = AudioBuffer::<f64>::new(channels, frames).unwrap();
        assert_eq!((f.channels(), f.frames()), (channels, frames));
        assert_eq!((d.channels(), d.frames()), (channels, frames));
        assert_layout(&f);
        assert_layout(&d);
        assert!((0..channels).all(|c| f.channel(c).iter().all(|&s| s == 0.0)));
        assert!((0..channels).all(|c| d.channel(c).iter().all(|&s| s == 0.0)));
    }
    assert_eq!(
        AudioBuffer::<f32>::new(2, 5).unwrap().as_block().stride(),
        16
    );
    assert_eq!(
        AudioBuffer::<f64>::new(2, 9).unwrap().as_block().stride(),
        16
    );
}

#[test]
fn b2_channels_and_views_share_storage() {
    let mut buf = AudioBuffer::<f32>::new(3, 7).unwrap();
    buf.channel_mut(1).fill(0.5);
    assert_eq!(buf.channel(0), &[0.0; 7]);
    assert_eq!(buf.channel(2), &[0.0; 7]);
    assert_eq!(buf.as_block().channel(1), &[0.5; 7]);
    buf.as_block_mut().channel_mut(2)[6] = -1.0;
    assert_eq!(buf.channel(2)[6], -1.0);
    assert!(buf.as_block_mut().is_simd_aligned());
}

#[test]
#[should_panic(expected = "out of range")]
fn b2_channel_out_of_range_panics() {
    let _ = AudioBuffer::<f32>::new(2, 4).unwrap().channel(2);
}

#[test]
fn b2_set_size_keep_existing_preserves_overlap() {
    for avoid in [false, true] {
        for (nc, nf) in [(4, 40), (1, 3), (2, 100), (3, 16), (0, 0), (5, 1)] {
            let mut buf = AudioBuffer::<f32>::new(3, 20).unwrap();
            stamp(&mut buf, |x| x as f32);
            buf.set_size(nc, nf, true, true, avoid).unwrap();
            assert_layout(&buf);
            for c in 0..nc {
                for (i, &s) in buf.channel(c).iter().enumerate() {
                    let want = if c < 3 && i < 20 { stamped(c, i) } else { 0.0 };
                    assert_eq!(s, want, "avoid {avoid}, shape {nc}x{nf}, ch {c} frame {i}");
                }
            }
        }
    }
}

#[test]
fn b2_set_size_clear_extra_zeroes_stale_space() {
    let mut buf = AudioBuffer::<f64>::new(2, 64).unwrap();
    stamp(&mut buf, |x| x);
    // Shrink in place, then grow back in place: the regained space held stale samples.
    buf.set_size(1, 8, true, false, true).unwrap();
    buf.set_size(2, 64, true, true, true).unwrap();
    assert_eq!(
        buf.channel(0)[..8],
        [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
    );
    assert!(buf.channel(0)[8..].iter().all(|&s| s == 0.0));
    assert!(buf.channel(1).iter().all(|&s| s == 0.0));
}

#[test]
fn b2_set_size_without_keep_existing_clear_extra_zeroes_everything() {
    let mut buf = AudioBuffer::<f32>::new(2, 32).unwrap();
    stamp(&mut buf, |x| x as f32);
    buf.set_size(2, 16, false, true, true).unwrap();
    assert!((0..2).all(|c| buf.channel(c).iter().all(|&s| s == 0.0)));
}

#[test]
fn b2_set_size_avoid_reallocating_never_allocates() {
    let mut buf = AudioBuffer::<f32>::new(2, 512).unwrap();
    stamp(&mut buf, |x| x as f32);
    let base = buf.as_block().channel(0).as_ptr();
    no_alloc(|| {
        // Shrink, change the stride both ways, and grow back within the capacity.
        buf.set_size(4, 100, true, true, true).unwrap();
        buf.set_size(1, 1000, true, false, true).unwrap();
        buf.set_size(2, 512, true, true, true).unwrap();
        buf.set_size(2, 512, false, false, true).unwrap();
    });
    assert_eq!(buf.as_block().channel(0).as_ptr(), base);
    assert_eq!(
        buf.channel(0)[..100],
        (0..100).map(|i| stamped(0, i)).collect::<Vec<_>>()
    );
    assert_layout(&buf);
}

#[test]
fn b2_set_size_without_avoid_reallocating_uses_minimum_memory() {
    let mut buf = AudioBuffer::<f32>::new(2, 512).unwrap();
    buf.set_size(1, 10, true, true, false).unwrap();
    assert!(format!("{buf:?}").contains("capacity: 16"), "{buf:?}");
    buf.set_size(1, 10, true, true, true).unwrap();
    buf.set_size(1, 4, true, true, true).unwrap();
    assert!(format!("{buf:?}").contains("capacity: 16"));
}

#[test]
fn b2_too_large_is_an_error_and_leaves_the_buffer_unchanged() {
    let mut buf = AudioBuffer::<f32>::new(1, 4).unwrap();
    buf.channel_mut(0)[0] = 1.0;
    for (c, f) in [(usize::MAX, 2), (2, usize::MAX), (1 << 40, 1 << 30)] {
        let err = buf.set_size(c, f, true, true, false).unwrap_err();
        assert!(matches!(err, BufferError::TooLarge { .. }));
        assert_eq!(err.code().as_str(), "SYRP-AUDIO-0005");
        assert!(!err.to_string().is_empty());
        assert!(AudioBuffer::<f64>::new(c, f).is_err());
    }
    assert_eq!(
        (buf.channels(), buf.frames(), buf.channel(0)[0]),
        (1, 4, 1.0)
    );
}

#[test]
fn b2_make_copy_of_converts_between_f32_and_f64() {
    let mut src = AudioBuffer::<f32>::new(2, 3).unwrap();
    src.channel_mut(1).copy_from_slice(&[0.1, -0.5, 1.0]);
    let mut wide = AudioBuffer::<f64>::new(5, 50).unwrap();
    wide.make_copy_of(&src, true).unwrap();
    assert_eq!((wide.channels(), wide.frames()), (2, 3));
    assert_eq!(wide.channel(0), &[0.0; 3]);
    assert_eq!(wide.channel(1), &[f64::from(0.1_f32), -0.5, 1.0]);
    let mut back = AudioBuffer::<f32>::default();
    back.make_copy_of(&wide, false).unwrap();
    assert_eq!(back.channel(1), src.channel(1));
    let mut same = AudioBuffer::<f32>::new(1, 1).unwrap();
    same.make_copy_of(&src, false).unwrap();
    assert_eq!(same.channel(1), src.channel(1));
    assert_eq!(src.clone().channel(1), src.channel(1));
    assert_layout(&wide);
    assert_layout(&back);
}

proptest! {
    #[test]
    fn b2_set_size_keeps_overlap_for_any_shapes(
        start in (0usize..5, 0usize..70),
        steps in prop::collection::vec((0usize..5, 0usize..70, any::<bool>()), 1..8),
    ) {
        let mut buf = AudioBuffer::<f32>::new(start.0, start.1).unwrap();
        stamp(&mut buf, |x| x as f32);
        let (mut kc, mut kf) = start;
        for (c, f, avoid) in steps {
            buf.set_size(c, f, true, true, avoid).unwrap();
            assert_layout(&buf);
            kc = kc.min(c);
            kf = kf.min(f);
            for ch in 0..c {
                for (i, &s) in buf.channel(ch).iter().enumerate() {
                    let want = if ch < kc && i < kf { stamped(ch, i) } else { 0.0 };
                    prop_assert_eq!(s, want);
                }
            }
        }
    }
}
