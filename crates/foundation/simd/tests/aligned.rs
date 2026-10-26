// SPDX-License-Identifier: MIT OR Apache-2.0
//! B3: `AlignedBuf`. Run under Miri with `cargo +nightly miri test -p syrp-simd --test aligned`.

use syrp_simd::{ALIGN, AlignedBuf, F32x8, F64x4, Zeroable};

fn check<T: Zeroable + PartialEq + Default + core::fmt::Debug>(len: usize) {
    let mut buf = AlignedBuf::<T>::zeroed(len);
    assert_eq!(buf.len(), len);
    assert_eq!(buf.is_empty(), len == 0);
    assert!(buf.as_slice().iter().all(|x| *x == T::default()));
    if len > 0 {
        assert_eq!(buf.as_slice().as_ptr() as usize % ALIGN, 0);
    }
    let before = buf.as_ptr();
    buf.as_mut_slice().fill(T::default());
    assert_eq!(buf.as_ptr(), before);
    assert_eq!(buf.as_mut_slice().len(), len);
}

#[test]
fn b3_buffers_are_zeroed_and_64_byte_aligned() {
    for len in [0, 1, 3, 15, 16, 17, 64, 1000] {
        check::<f32>(len);
        check::<f64>(len);
        check::<F32x8>(len);
        check::<F64x4>(len);
    }
}

#[test]
fn b3_writes_stick_and_the_buffer_never_moves() {
    let mut buf = AlignedBuf::<f32>::zeroed(33);
    let ptr = buf.as_ptr();
    for (i, x) in buf.as_mut_slice().iter_mut().enumerate() {
        *x = i as f32;
    }
    assert_eq!(buf[32], 32.0);
    assert_eq!(buf.iter().sum::<f32>(), (0..33).sum::<i32>() as f32);
    assert_eq!(buf.as_ptr(), ptr);
    assert_eq!(format!("{:?}", AlignedBuf::<f64>::zeroed(2)), "[0.0, 0.0]");
}

#[test]
fn b3_buffers_move_between_threads() {
    let mut buf = AlignedBuf::<f64>::zeroed(8);
    buf[7] = 2.0;
    let sum = std::thread::spawn(move || buf.iter().sum::<f64>())
        .join()
        .unwrap();
    assert_eq!(sum, 2.0);
}

#[test]
#[should_panic(expected = "overflows")]
fn b3_oversized_length_panics_instead_of_allocating() {
    let _ = AlignedBuf::<f64>::zeroed(usize::MAX / 4);
}
