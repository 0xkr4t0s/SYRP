// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B3 (cleared flag) and B4 (buffer operations against plain scalar loops, `f32-arith`).

use proptest::prelude::*;
use syrp_audio_basics::AudioBuffer;
use syrp_rt::no_alloc;
use syrp_simd::Level;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// The `f32-arith` profile: 4 ulp, or 1e-7 absolute near zero.
fn close(got: &[f32], want: &[f32]) -> bool {
    let tol = |w: f32| 1e-7_f32.max(w.abs() * 4.0 * f32::EPSILON);
    got.len() == want.len() && got.iter().zip(want).all(|(g, w)| (g - w).abs() <= tol(*w))
}

fn filled(channels: usize, data: &[f32]) -> AudioBuffer<f32> {
    let mut buf = AudioBuffer::new(channels, data.len()).unwrap();
    (0..channels).for_each(|c| buf.channel_mut(c).copy_from_slice(data));
    buf
}

#[test]
fn b3_cleared_flag_follows_writes_and_clears() {
    let mut buf = AudioBuffer::<f32>::new(2, 8).unwrap();
    assert!(buf.is_clear() && AudioBuffer::<f64>::default().is_clear());
    let _ = buf.as_block_mut();
    assert!(!buf.is_clear());
    buf.clear();
    assert!(buf.is_clear());
    let silent = AudioBuffer::<f32>::new(1, 8).unwrap();
    // Adding or copying a clear source, or with zero gain, keeps the destination clear.
    buf.add_from(0, 0, &silent, 0, 0..8, 1.0);
    buf.copy_from(1, 0, &silent, 0, 0..8, 1.0);
    buf.add_from_with_ramp(0, 0, &[1.0; 8], 0.0, 0.0);
    buf.apply_gain(3.0);
    buf.reverse(0, 0..8);
    assert!(buf.is_clear());
    assert_eq!((buf.magnitude(0, 0..8), buf.rms_level(0, 0..8)), (0.0, 0.0));
    assert_eq!(buf.find_min_max(1, 2..4), Some((0.0, 0.0)));
    let one = filled(1, &[1.0; 8]);
    buf.add_from(1, 4, &one, 0, 0..4, 0.5);
    assert!(!buf.is_clear());
    assert_eq!(buf.channel(1), &[0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5]);
    assert_eq!(buf.channel(0), &[0.0; 8]);
    // Copying a clear source zeroes a dirty destination range.
    buf.copy_from(1, 4, &silent, 0, 0..2, 1.0);
    assert_eq!(buf.channel(1)[4..], [0.0, 0.0, 0.5, 0.5]);
    buf.apply_gain(0.0);
    assert!(buf.is_clear() && buf.channel(1).iter().all(|&s| s == 0.0));
    buf.set_not_clear();
    assert!(!buf.is_clear());
    // make_copy_of and resizing carry the flag.
    let mut copy = filled(1, &[2.0; 4]);
    copy.make_copy_of(&silent, true).unwrap();
    assert!(copy.is_clear() && copy.channel(0).iter().all(|&s| s == 0.0));
    let mut grown = filled(1, &[2.0; 4]);
    grown.set_size(2, 4, true, true, false).unwrap();
    assert!(!grown.is_clear());
    grown.set_size(2, 8, false, true, true).unwrap();
    assert!(grown.is_clear());
}

#[test]
fn b4_operations_never_allocate() {
    let _ = Level::detect();
    let src = filled(2, &[0.5; 64]);
    let mut buf = filled(2, &[0.25; 64]);
    no_alloc(|| {
        buf.apply_gain(0.5);
        buf.apply_gain_channel(0, 3..40, 2.0);
        buf.apply_gain_ramp(1, 0..64, 0.0, 1.0);
        buf.add_from(0, 1, &src, 1, 0..63, 0.7);
        buf.copy_from(1, 0, &src, 0, 5..64, 1.0);
        buf.add_from_with_ramp(0, 0, src.channel(0), 1.0, 0.0);
        buf.copy_from_with_ramp(1, 2, &src.channel(1)[..10], 0.2, 0.4);
        let _ = (buf.magnitude(0, 0..64), buf.rms_level(1, 0..64));
        let _ = buf.find_min_max(0, 1..9);
        buf.reverse(1, 0..33);
        buf.clear_channel(0, 0..3);
        buf.clear_frames(60..64);
        buf.clear();
    });
    assert!(buf.is_clear());
}

proptest! {
    #[test]
    fn b4_operations_match_plain_scalar_loops(
        data in prop::collection::vec(-1.0_f32..1.0, 1..200),
        other in prop::collection::vec(-1.0_f32..1.0, 200),
        (cut, gain, end) in (0.0_f64..1.0, -2.0_f32..2.0, -2.0_f32..2.0),
    ) {
        let n = data.len();
        let (lo, hi) = ((cut * n as f64) as usize / 2, n - (cut * n as f64) as usize / 2);
        let m = hi - lo;
        let ramp = |i: usize| gain + (end - gain) * (i as f64 / m as f64) as f32;
        let src = filled(1, &other[..n]);

        let mut b = filled(2, &data);
        b.apply_gain_channel(1, lo..hi, gain);
        let want: Vec<f32> = data[lo..hi].iter().map(|x| x * gain).collect();
        prop_assert!(close(&b.channel(1)[lo..hi], &want));
        prop_assert!(close(b.channel(0), &data));

        let mut b = filled(1, &data);
        b.apply_gain_ramp(0, lo..hi, gain, end);
        let want: Vec<f32> = (0..m).map(|i| data[lo + i] * ramp(i)).collect();
        prop_assert!(close(&b.channel(0)[lo..hi], &want));

        let mut b = filled(1, &data);
        b.add_from(0, lo, &src, 0, 0..m, gain);
        let want: Vec<f32> = (0..m).map(|i| data[lo + i] + other[i] * gain).collect();
        prop_assert!(close(&b.channel(0)[lo..hi], &want));

        let mut b = filled(1, &data);
        b.copy_from(0, lo, &src, 0, n - m..n, gain);
        let want: Vec<f32> = (n - m..n).map(|i| other[i] * gain).collect();
        prop_assert!(close(&b.channel(0)[lo..hi], &want));

        let mut b = filled(1, &data);
        b.add_from_with_ramp(0, lo, &other[..m], gain, end);
        let want: Vec<f32> = (0..m).map(|i| data[lo + i] + other[i] * ramp(i)).collect();
        prop_assert!(close(&b.channel(0)[lo..hi], &want));

        let mut b = filled(1, &data);
        b.copy_from_with_ramp(0, lo, &other[..m], gain, end);
        let want: Vec<f32> = (0..m).map(|i| other[i] * ramp(i)).collect();
        prop_assert!(close(&b.channel(0)[lo..hi], &want));

        let b = filled(1, &data);
        let s = &data[lo..hi];
        let peak = s.iter().fold(0.0_f32, |a, x| a.max(x.abs()));
        let rms = if m == 0 { 0.0 } else { (s.iter().map(|x| x * x).sum::<f32>() / m as f32).sqrt() };
        prop_assert!(close(&[b.magnitude(0, lo..hi)], &[peak]));
        prop_assert!(close(&[b.rms_level(0, lo..hi)], &[rms]));
        let mm = s.iter().fold(None, |a: Option<(f32, f32)>, &x| {
            Some(a.map_or((x, x), |(l, h)| (l.min(x), h.max(x))))
        });
        prop_assert_eq!(b.find_min_max(0, lo..hi), mm);

        let mut b = filled(1, &data);
        b.reverse(0, lo..hi);
        let want: Vec<f32> = s.iter().rev().copied().collect();
        prop_assert_eq!(&b.channel(0)[lo..hi], &want[..]);
        b.clear_channel(0, lo..hi);
        prop_assert!(b.channel(0)[lo..hi].iter().all(|&x| x == 0.0));
        prop_assert!(close(&b.channel(0)[..lo], &data[..lo]));
    }
}

#[test]
#[should_panic(expected = "out of bounds")]
fn b4_out_of_range_frames_panic() {
    AudioBuffer::<f32>::new(1, 4).unwrap().reverse(0, 2..5);
}
