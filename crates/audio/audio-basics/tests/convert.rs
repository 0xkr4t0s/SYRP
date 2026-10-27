// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-02.md, increments 1 and 2 (B1–B3): packed formats, the
//! conversion rule and interleaving.

use proptest::prelude::*;
use syrp_audio_api::{AudioBlock, AudioBlockMut};
use syrp_audio_basics::BufferSample;
use syrp_audio_basics::convert::{
    ConvertError, Endianness, SampleFormat, convert_bytes, deinterleave, interleave, read_into,
    write_from,
};
use syrp_error::Diagnostic;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

use Endianness::{Big, Little, Native};
use SampleFormat::{Float32, Float64, Int8, Int16, Int24, Int24In32, Int32, UInt8};

const ENDIANS: [Endianness; 3] = [Little, Big, Native];

/// Integer formats with their significant bits.
const INTS: [(SampleFormat, u32); 6] = [
    (Int8, 8),
    (UInt8, 8),
    (Int16, 16),
    (Int24, 24),
    (Int24In32, 24),
    (Int32, 32),
];

/// Decodes one sample to `f64`.
fn read1(bytes: &[u8], format: SampleFormat, e: Endianness) -> f64 {
    let mut out = [0.0_f64];
    let mut block = AudioBlockMut::new(&mut out, 1, 1, 1).unwrap();
    read_into(bytes, format, e, false, 1, &mut block).unwrap();
    out[0]
}

/// Encodes one sample of type `T`.
fn write1<T: BufferSample>(x: T, format: SampleFormat, e: Endianness) -> Vec<u8> {
    let data = [x];
    let block = AudioBlock::new(&data, 1, 1, 1).unwrap();
    let mut out = vec![0; format.bytes_per_sample()];
    write_from(&block, &mut out, format, e, false).unwrap();
    out
}

/// Encodes one `f64` to an integer format and reads the integer back (little-endian bytes).
fn quantised(x: f64, format: SampleFormat) -> i64 {
    let b = write1(x, format, Little);
    match format {
        Int8 => i64::from(b[0] as i8),
        UInt8 => i64::from(b[0]) - 128,
        Int16 => i64::from(i16::from_le_bytes([b[0], b[1]])),
        Int24 => i64::from(i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8),
        Int24In32 | Int32 => i64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        _ => unreachable!(),
    }
}

/// An independent statement of the B2 rule: scale, round half away from zero, clip, NaN to 0.
fn reference(x: f64, bits: u32) -> i64 {
    if x.is_nan() {
        return 0;
    }
    let scale = (1_i64 << (bits - 1)) as f64;
    let y = x * scale;
    let max = (1_i64 << (bits - 1)) - 1;
    if y >= max as f64 {
        return max;
    }
    if y <= -scale {
        return -(1_i64 << (bits - 1));
    }
    // |y| < 2^31: the whole and fractional parts are exact, so compare the fraction with 0.5
    // directly (adding 0.5 and flooring would round 0.49999999999999994 up).
    let whole = y.trunc();
    let step = if (y - whole).abs() >= 0.5 {
        y.signum()
    } else {
        0.0
    };
    (whole + step) as i64
}

#[test]
fn b1_bytes_per_sample_for_every_format() {
    let expected = [1, 1, 2, 3, 4, 4, 4, 8];
    for (f, n) in SampleFormat::ALL.iter().zip(expected) {
        assert_eq!(f.bytes_per_sample(), n, "{f:?}");
        assert_eq!(f.is_float(), matches!(f, Float32 | Float64), "{f:?}");
    }
    assert_eq!(SampleFormat::ALL.len(), 8);
}

#[test]
fn b1_native_endianness_is_the_target_order() {
    let native = if cfg!(target_endian = "little") {
        Little
    } else {
        Big
    };
    assert_eq!(Native.resolve(), native);
    assert_eq!(Little.resolve(), Little);
    assert_eq!(Big.resolve(), Big);
    assert_eq!(read1(&0x4000_i16.to_ne_bytes(), Int16, Native), 0.5);
}

#[test]
fn b2_integers_divide_by_two_to_the_bits_minus_one() {
    assert_eq!(read1(&[0x40], Int8, Little), 0.5);
    assert_eq!(read1(&[0x80], Int8, Little), -1.0);
    assert_eq!(read1(&[128], UInt8, Little), 0.0);
    assert_eq!(read1(&[0], UInt8, Little), -1.0);
    assert_eq!(read1(&[255], UInt8, Little), 127.0 / 128.0);
    assert_eq!(read1(&[0x00, 0x40], Int16, Little), 0.5);
    assert_eq!(read1(&[0x40, 0x00], Int16, Big), 0.5);
    assert_eq!(read1(&[0x00, 0x00, 0xC0], Int24, Little), -0.5);
    assert_eq!(read1(&[0xC0, 0x00, 0x00], Int24, Big), -0.5);
    assert_eq!(
        read1(&[0x01, 0x00, 0x00, 0x80], Int32, Little),
        (1.0 - 2f64.powi(31)) / 2f64.powi(31)
    );
    assert_eq!(read1(&[0x00, 0x00, 0x40, 0x00], Int24In32, Little), 0.5);
}

#[test]
fn b2_int24_in_32_ignores_the_top_byte_and_writes_the_sign_syrp_choice() {
    // Only the low 24 bits count when reading.
    assert_eq!(read1(&[0x00, 0x00, 0x40, 0x7F], Int24In32, Little), 0.5);
    assert_eq!(read1(&[0xAB, 0x40, 0x00, 0x00], Int24In32, Big), 0.5);
    // Writing sign-extends, so the word is the same value as an Int32.
    assert_eq!(
        write1(-0.5_f32, Int24In32, Little),
        [0x00, 0x00, 0xC0, 0xFF]
    );
    assert_eq!(write1(0.5_f32, Int24In32, Big), [0x00, 0x40, 0x00, 0x00]);
}

#[test]
fn b2_float_to_int_rounds_ties_away_from_zero_syrp_choice() {
    for (x, n) in [
        (0.5, 1),
        (-0.5, -1),
        (1.5, 2),
        (-1.5, -2),
        (2.5, 3),
        (-2.5, -3),
        (0.49, 0),
    ] {
        assert_eq!(quantised(x / 32768.0, Int16), n, "{x}");
        assert_eq!(quantised(x / 128.0, UInt8), n, "{x}");
    }
    // Every tie point of Int16 and its neighbours, from f32 and from f64 input.
    for k in -32770_i32..=32770 {
        let tie = (f64::from(k) + 0.5) / 32768.0;
        for x in [tie.next_down(), tie, tie.next_up()] {
            assert_eq!(quantised(x, Int16), reference(x, 16), "{x}");
        }
        let tie32 = (k as f32 + 0.5) / 32768.0;
        for x in [tie32.next_down(), tie32, tie32.next_up()] {
            let b = write1(x, Int16, Little);
            let got = i64::from(i16::from_le_bytes([b[0], b[1]]));
            assert_eq!(got, reference(f64::from(x), 16), "{x}");
        }
    }
}

#[test]
fn b2_float_to_int_rounds_every_int24_tie_syrp_choice() {
    for k in -(1_i32 << 23) - 1..=1 << 23 {
        let tie = (f64::from(k) + 0.5) / 8_388_608.0;
        assert_eq!(quantised(tie, Int24), reference(tie, 24), "{tie}");
    }
}

#[test]
fn b2_float_to_int_clips_out_of_range_syrp_choice() {
    for (f, bits) in INTS {
        let max = (1_i64 << (bits - 1)) - 1;
        let min = -(1_i64 << (bits - 1));
        for (x, n) in [
            (1.0, max),
            (-1.0, min),
            (2.0, max),
            (-7.5, min),
            (f64::INFINITY, max),
            (f64::NEG_INFINITY, min),
            (f64::MAX, max),
            (f64::MIN, min),
        ] {
            assert_eq!(quantised(x, f), n, "{f:?} {x}");
        }
    }
}

#[test]
fn b2_nan_becomes_zero_syrp_choice() {
    for (f, _) in INTS {
        for nan in [f64::NAN, -f64::NAN, f64::from_bits(0x7FF0_0000_0000_0001)] {
            assert_eq!(quantised(nan, f), 0, "{f:?}");
        }
        assert_eq!(
            write1(f32::NAN, f, Little),
            write1(0.0_f32, f, Little),
            "{f:?}"
        );
    }
    assert_eq!(write1(0.0_f32, UInt8, Little), [128]);
}

#[test]
fn b2_floats_are_stored_unscaled_and_unclipped() {
    let x = 3.25_f32;
    assert_eq!(write1(x, Float32, Little), x.to_le_bytes());
    assert_eq!(write1(x, Float32, Big), x.to_be_bytes());
    assert_eq!(write1(-9.5_f64, Float64, Big), (-9.5_f64).to_be_bytes());
    assert_eq!(read1(&(-9.5_f64).to_le_bytes(), Float64, Little), -9.5);
    // f64 narrowed to Float32 rounds to nearest.
    let wide = 0.1_f64;
    assert_eq!(write1(wide, Float32, Little), (wide as f32).to_le_bytes());
    // Float32 into f32 keeps every bit, a signalling NaN payload included.
    let snan = f32::from_bits(0x7F80_0001);
    let mut out = [0.0_f32];
    let mut block = AudioBlockMut::new(&mut out, 1, 1, 1).unwrap();
    read_into(&snan.to_le_bytes(), Float32, Little, false, 1, &mut block).unwrap();
    assert_eq!(out[0].to_bits(), snan.to_bits());
    assert_eq!(write1(snan, Float32, Little), snan.to_le_bytes());
}

#[test]
fn b2_every_8_and_16_bit_pattern_round_trips() {
    for e in ENDIANS {
        for b in 0..=255_u8 {
            for f in [Int8, UInt8] {
                assert_eq!(write1(read1(&[b], f, e) as f32, f, e), [b]);
            }
        }
        for n in 0..=u16::MAX {
            let bytes = n.to_le_bytes();
            let x32 = read1(&bytes, Int16, e) as f32;
            assert_eq!(write1(x32, Int16, e), bytes);
            assert_eq!(write1(read1(&bytes, Int16, e), Int16, e), bytes);
        }
    }
}

#[test]
fn b2_every_24_bit_pattern_round_trips_through_f32() {
    let mut bytes = vec![0_u8; 3 << 24];
    for (n, b) in bytes.chunks_exact_mut(3).enumerate() {
        b.copy_from_slice(&(n as u32).to_le_bytes()[..3]);
    }
    let frames = 1 << 24;
    let mut planar = vec![0.0_f32; frames];
    let mut block = AudioBlockMut::new(&mut planar, 1, frames, frames).unwrap();
    read_into(&bytes, Int24, Little, false, 1, &mut block).unwrap();
    assert_eq!(planar[0x40_0000], 0.5);
    assert_eq!(planar[0x80_0000], -1.0);
    let block = AudioBlock::new(&planar, 1, frames, frames).unwrap();
    let mut back = vec![0_u8; 3 << 24];
    write_from(&block, &mut back, Int24, Little, false).unwrap();
    assert!(back == bytes, "Int24 -> f32 -> Int24 changed a sample");
}

#[test]
fn b2_interleaved_and_planar_bytes_place_samples_differently() {
    // 2 channels x 3 frames of Int8: channel c, frame i holds c * 10 + i.
    let planar_bytes: Vec<u8> = (0..2)
        .flat_map(|c| (0..3).map(move |i| c * 10 + i))
        .collect();
    let interleaved_bytes: Vec<u8> = (0..3)
        .flat_map(|i| (0..2).map(move |c| c * 10 + i))
        .collect();
    for (bytes, inter) in [(&planar_bytes, false), (&interleaved_bytes, true)] {
        let mut out = [0.0_f32; 6];
        let mut block = AudioBlockMut::new(&mut out, 2, 3, 3).unwrap();
        read_into(bytes, Int8, Little, inter, 2, &mut block).unwrap();
        let expected: Vec<f32> = planar_bytes.iter().map(|&b| f32::from(b) / 128.0).collect();
        assert_eq!(out.as_slice(), expected.as_slice());
        let block = AudioBlock::new(&out, 2, 3, 3).unwrap();
        let mut back = vec![0; 6];
        write_from(&block, &mut back, Int8, Little, inter).unwrap();
        assert_eq!(&back, bytes);
    }
}

#[test]
fn b2_read_into_skips_data_channels_past_the_block_syrp_choice() {
    // Four interleaved Int16 channels; the block takes the first two.
    let words: [i16; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let mut out = [0.0_f64; 4];
    let mut block = AudioBlockMut::new(&mut out, 2, 2, 2).unwrap();
    read_into(&bytes, Int16, Little, true, 4, &mut block).unwrap();
    assert_eq!(out.map(|x| x * 32768.0), [1.0, 5.0, 2.0, 6.0]);
}

#[test]
fn b2_size_mismatches_are_coded_errors_and_write_nothing() {
    let mut out = [7.0_f32; 8];
    let mut block = AudioBlockMut::new(&mut out, 2, 3, 4).unwrap();
    let e = read_into(&[0; 11], Int16, Little, true, 2, &mut block).unwrap_err();
    assert!(
        matches!(
            e,
            ConvertError::ByteLengthMismatch {
                expected: 12,
                actual: 11,
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(e.code().as_str(), "SYRP-AUDIO-0007");
    let e = read_into(&[0; 13], Int16, Little, false, 2, &mut block).unwrap_err();
    assert_eq!(e.code().as_str(), "SYRP-AUDIO-0007");
    let e = read_into(&[0; 6], Int16, Little, true, 1, &mut block).unwrap_err();
    assert!(
        matches!(
            e,
            ConvertError::TooFewChannels {
                block: 2,
                data: 1,
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(e.code().as_str(), "SYRP-AUDIO-0008");
    let e = read_into(&[0; 6], Int16, Little, true, usize::MAX, &mut block).unwrap_err();
    assert!(
        matches!(
            e,
            ConvertError::ByteLengthMismatch {
                expected: usize::MAX,
                actual: 6,
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(out, [7.0; 8]);

    let block = AudioBlock::new(&out, 2, 3, 4).unwrap();
    let mut bytes = [9_u8; 25];
    let e = write_from(&block, &mut bytes, Float32, Big, true).unwrap_err();
    assert!(
        matches!(
            e,
            ConvertError::ByteLengthMismatch {
                expected: 24,
                actual: 25,
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(bytes, [9; 25]);

    let e = convert_bytes(&[0; 5], Int16, Little, &mut bytes, Int8, Little).unwrap_err();
    assert!(
        matches!(
            e,
            ConvertError::PartialSample {
                len: 5,
                bytes_per_sample: 2,
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(e.code().as_str(), "SYRP-AUDIO-0009");
    let e = convert_bytes(&[0; 6], Int16, Little, &mut bytes, Int8, Little).unwrap_err();
    assert!(
        matches!(
            e,
            ConvertError::ByteLengthMismatch {
                expected: 3,
                actual: 25,
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(bytes, [9; 25]);
    assert!(!e.to_string().is_empty());
}

#[test]
fn b2_empty_shapes_convert_nothing() {
    let mut none: [f32; 0] = [];
    let mut block = AudioBlockMut::new(&mut none, 0, 5, 5).unwrap();
    read_into(&[], Int24, Big, true, 0, &mut block).unwrap();
    let mut block = AudioBlockMut::new(&mut none, 3, 0, 0).unwrap();
    read_into(&[], Int24, Big, true, 3, &mut block).unwrap();
    write_from(&block.as_block(), &mut [], Float64, Little, false).unwrap();
    assert_eq!(
        convert_bytes(&[], Int16, Little, &mut [], Int32, Big),
        Ok(0)
    );
}

#[test]
fn b2_convert_bytes_same_format_copies_or_swaps() {
    let src = [1, 2, 3, 4, 5, 6];
    let mut out = [0; 6];
    convert_bytes(&src, Int24, Little, &mut out, Int24, Native.resolve()).unwrap();
    let expected = if cfg!(target_endian = "little") {
        src
    } else {
        [3, 2, 1, 6, 5, 4]
    };
    assert_eq!(out, expected);
    convert_bytes(&src, Int24, Little, &mut out, Int24, Big).unwrap();
    assert_eq!(out, [3, 2, 1, 6, 5, 4]);
    let nan = f32::from_bits(0x7F80_0001).to_le_bytes();
    let mut out = [0; 4];
    convert_bytes(&nan, Float32, Little, &mut out, Float32, Big).unwrap();
    assert_eq!(out, [0x7F, 0x80, 0x00, 0x01]);
}

#[test]
fn b2_convert_bytes_narrows_with_the_rounding_rule() {
    // Int24 to Int16 divides by 256 with ties away from zero, then clips.
    let cases: [(i32, i16); 5] = [
        (0x80, 1),
        (-0x80, -1),
        (0x7F, 0),
        (0x17F, 1),
        (0x7F_FFFF, 0x7FFF),
    ];
    for (n, m) in cases {
        let mut out = [0; 2];
        convert_bytes(
            &n.to_le_bytes()[..3],
            Int24,
            Little,
            &mut out,
            Int16,
            Little,
        )
        .unwrap();
        assert_eq!(i16::from_le_bytes(out), m, "{n:#x}");
    }
}

fn format() -> impl Strategy<Value = SampleFormat> {
    prop::sample::select(SampleFormat::ALL)
}

fn endian() -> impl Strategy<Value = Endianness> {
    prop::sample::select(ENDIANS.to_vec())
}

proptest! {
    #[test]
    fn b2_quantise_matches_the_reference_rule(x in prop::num::f64::ANY, i in 0..INTS.len()) {
        let (f, bits) = INTS[i];
        prop_assert_eq!(quantised(x, f), reference(x, bits));
    }

    #[test]
    fn b2_quantise_matches_the_reference_rule_near_full_scale(x in -1.1_f64..1.1, i in 0..INTS.len()) {
        let (f, bits) = INTS[i];
        prop_assert_eq!(quantised(x, f), reference(x, bits));
    }

    #[test]
    fn b2_f32_input_quantises_like_its_f64_value(x in prop::num::f32::ANY, i in 0..INTS.len()) {
        let f = INTS[i].0;
        prop_assert_eq!(write1(x, f, Little), write1(f64::from(x), f, Little));
    }

    #[test]
    fn b2_every_format_round_trips_bytes_for_values_on_its_grid(
        f in format(), e in endian(), seed in prop::collection::vec(any::<u8>(), 8),
    ) {
        // Any bytes are a grid value of the format, except that Int24In32 only keeps 24 bits
        // and a float NaN may change payload through f64.
        let n = f.bytes_per_sample();
        let mut bytes = seed[..n].to_vec();
        let x = read1(&bytes, f, e);
        prop_assume!(!x.is_nan());
        if f == Int24In32 {
            let top = if e.resolve() == Big { 0 } else { 3 };
            let low = if top == 0 { bytes[1] } else { bytes[2] };
            bytes[top] = if low & 0x80 != 0 { 0xFF } else { 0 };
        }
        prop_assert_eq!(&write1(x, f, e), &bytes);
        // Int32 and Float64 have more precision than f32.
        if !matches!(f, Int32 | Float64) {
            prop_assert_eq!(write1(x as f32, f, e), bytes, "through f32");
        }
    }

    #[test]
    fn b2_convert_bytes_equals_read_then_write(
        from in format(), to in format(), fe in endian(), te in endian(),
        raw in prop::collection::vec(any::<u8>(), 0..64),
    ) {
        let n = from.bytes_per_sample();
        let src = &raw[..raw.len() / n * n];
        let count = src.len() / n;
        let mut direct = vec![0; count * to.bytes_per_sample()];
        prop_assert_eq!(convert_bytes(src, from, fe, &mut direct, to, te), Ok(count));
        let mut mid = vec![0.0_f64; count];
        let mut block = AudioBlockMut::new(&mut mid, 1, count, count).unwrap();
        read_into(src, from, fe, false, 1, &mut block).unwrap();
        prop_assume!(from != to && mid.iter().all(|x| !x.is_nan()));
        let block = AudioBlock::new(&mid, 1, count, count).unwrap();
        let mut via = vec![0; direct.len()];
        write_from(&block, &mut via, to, te, true).unwrap();
        prop_assert_eq!(direct, via);
    }

    #[test]
    fn b3_interleave_round_trips_and_places_frames(
        channels in 0_usize..6, frames in 0_usize..40, pad in 0_usize..5,
    ) {
        let stride = frames + pad;
        let planar: Vec<f32> = (0..channels * stride).map(|i| i as f32).collect();
        let block = AudioBlock::new(&planar, channels, frames, stride).unwrap();
        let mut inter = vec![-1.0; channels * frames];
        interleave(&block, &mut inter).unwrap();
        for c in 0..channels {
            for i in 0..frames {
                prop_assert_eq!(inter[i * channels + c], planar[c * stride + i]);
            }
        }
        let mut back = vec![0.0; channels * stride];
        let mut out = AudioBlockMut::new(&mut back, channels, frames, stride).unwrap();
        deinterleave(&inter, &mut out).unwrap();
        for c in 0..channels {
            prop_assert_eq!(out.channel(c), block.channel(c));
        }
    }
}

#[test]
fn b3_interleave_matches_the_documented_layout() {
    let planar = [1.0_f64, 2.0, 3.0, -1.0, -2.0, -3.0];
    let block = AudioBlock::new(&planar, 2, 3, 3).unwrap();
    let mut out = [0.0; 6];
    interleave(&block, &mut out).unwrap();
    assert_eq!(out, [1.0, -1.0, 2.0, -2.0, 3.0, -3.0]);
    let mut back = [0.0; 6];
    let mut dest = AudioBlockMut::new(&mut back, 2, 3, 3).unwrap();
    deinterleave(&out, &mut dest).unwrap();
    assert_eq!(back, planar);
}

#[test]
fn b3_interleave_length_mismatch_is_a_coded_error() {
    let planar = [1.0_f32; 8];
    let block = AudioBlock::new(&planar, 2, 4, 4).unwrap();
    let mut short = [5.0; 7];
    let e = interleave(&block, &mut short).unwrap_err();
    assert!(
        matches!(
            e,
            ConvertError::SampleCountMismatch {
                expected: 8,
                actual: 7,
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(e.code().as_str(), "SYRP-AUDIO-0010");
    assert_eq!(short, [5.0; 7]);
    let mut back = [3.0_f32; 8];
    let mut dest = AudioBlockMut::new(&mut back, 2, 4, 4).unwrap();
    assert!(deinterleave(&[0.0; 9], &mut dest).is_err());
    assert_eq!(back, [3.0; 8]);
}

/// Decodes one sample to `f32`.
fn read1_f32(bytes: &[u8], format: SampleFormat) -> f32 {
    let mut out = [0.0_f32];
    let mut block = AudioBlockMut::new(&mut out, 1, 1, 1).unwrap();
    read_into(bytes, format, Little, false, 1, &mut block).unwrap();
    out[0]
}

/// An `f32` on the grid of `format`: `k / 2^(bits-1)` for an integer `k` in range that `f32`
/// holds exactly, or any non-NaN float for the float formats.
fn grid_value(format: SampleFormat, raw: u32, shift: u32) -> f32 {
    let (bits, signed_range) = match format {
        Float32 | Float64 => {
            let x = f32::from_bits(raw);
            return if x.is_nan() { 0.0 } else { x };
        }
        Int8 | UInt8 => (8, 8),
        Int16 => (16, 16),
        Int24 | Int24In32 => (24, 24),
        _ => (32, 24), // Int32: 24 significant bits, shifted into place
    };
    let k = (raw as i32) >> (32 - signed_range);
    let k = if bits == 32 { k << (shift % 9) } else { k };
    (f64::from(k) / f64::from(1_u32 << (bits - 1))) as f32
}

proptest! {
    #[test]
    fn b6_every_format_round_trips_f32_values_on_its_grid(
        f in format(), raw in any::<u32>(), shift in any::<u32>(),
    ) {
        let x = grid_value(f, raw, shift);
        let back = read1_f32(&write1(x, f, Little), f);
        prop_assert_eq!(back.to_bits(), x.to_bits(), "{:?} {}", f, x);
    }
}
