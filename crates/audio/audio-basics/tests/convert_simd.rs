// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-02.md B4: the `syrp_simd` pack paths of the converters give the
//! scalar codec's bits for every input (`exact` profile), and no converter allocates.

use proptest::prelude::*;
use syrp_audio_api::{AudioBlock, AudioBlockMut};
use syrp_audio_basics::convert::{
    Endianness, SampleFormat, convert_bytes, deinterleave, interleave, read_into, read_into_at,
    write_from, write_from_at,
};
use syrp_rt::no_alloc;
use syrp_simd::Level;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

use SampleFormat::{Float32, Int16, Int24, Int32};

/// The formats with pack paths (little-endian, `f32`).
const PACKED: [SampleFormat; 4] = [Int16, Int24, Int32, Float32];

/// Reads `bytes` at `level` into a `channels` x `frames` block of `f32`.
fn read_at(
    level: Level,
    bytes: &[u8],
    f: SampleFormat,
    inter: bool,
    channels: usize,
    frames: usize,
) -> Vec<u32> {
    let mut out = vec![0.0_f32; channels * frames];
    let mut block = AudioBlockMut::new(&mut out, channels, frames, frames).unwrap();
    read_into_at(
        level,
        bytes,
        f,
        Endianness::Little,
        inter,
        channels,
        &mut block,
    )
    .unwrap();
    out.iter().map(|x| x.to_bits()).collect()
}

/// Writes a `channels` x `frames` block of `f32` at `level`.
fn write_at(level: Level, data: &[f32], f: SampleFormat, inter: bool, channels: usize) -> Vec<u8> {
    let frames = data.len() / channels.max(1);
    let block = AudioBlock::new(data, channels, frames, frames).unwrap();
    let mut out = vec![0_u8; data.len() * f.bytes_per_sample()];
    write_from_at(level, &block, &mut out, f, Endianness::Little, inter).unwrap();
    out
}

/// Floats that stress the encoder: ties at every scale, the range ends, NaN, infinities,
/// subnormals and arbitrary bit patterns.
fn interesting_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        any::<u32>().prop_map(f32::from_bits),
        (-40_000_i32..40_000, 0_u32..3)
            .prop_map(|(k, s)| { (k as f32 + 0.5) / [128.0, 32_768.0, 8_388_608.0][s as usize] }),
        (-9_000_000_i32..9_000_000).prop_map(|k| (k as f32 + 0.5) / 8_388_608.0),
        prop::sample::select(vec![
            1.0,
            -1.0,
            1.0 - f32::EPSILON / 2.0,
            -1.0 - f32::EPSILON,
            f32::NAN,
            -f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN_POSITIVE / 2.0,
            f32::MAX,
            f32::MIN,
            0.0,
            -0.0,
        ]),
        -1.5_f32..1.5,
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn b4_pack_decode_is_bit_identical_to_scalar(
        f in prop::sample::select(PACKED.to_vec()),
        channels in 1_usize..4,
        frames in 0_usize..41,
        inter in any::<bool>(),
        seed in prop::collection::vec(any::<u8>(), 4 * 3 * 41),
    ) {
        let bytes = &seed[..channels * frames * f.bytes_per_sample()];
        let reference = read_at(Level::Scalar, bytes, f, inter, channels, frames);
        for level in Level::supported() {
            prop_assert_eq!(&read_at(level, bytes, f, inter, channels, frames), &reference,
                "{:?} {:?}", f, level);
        }
    }

    #[test]
    fn b4_pack_encode_is_bit_identical_to_scalar(
        f in prop::sample::select(PACKED.to_vec()),
        channels in 1_usize..4,
        inter in any::<bool>(),
        data in prop::collection::vec(interesting_f32(), 0..120),
    ) {
        let data = &data[..data.len() / channels * channels];
        let reference = write_at(Level::Scalar, data, f, inter, channels);
        for level in Level::supported() {
            prop_assert_eq!(&write_at(level, data, f, inter, channels), &reference,
                "{:?} {:?}", f, level);
        }
    }
}

#[test]
fn b4_pack_encode_matches_scalar_on_int16_and_int24_ties() {
    let levels: Vec<Level> = Level::supported().collect();
    // Every Int16 tie; every 7th Int24 tie plus all within 2048 of the range ends (the
    // packs are slow in debug builds, and the property test above covers the rest).
    for (f, scale, every) in [(Int16, 32_768.0_f32, 1), (Int24, 8_388_608.0, 7)] {
        let half = scale as i32;
        let data: Vec<f32> = (-half - 2..=half + 1)
            .filter(|k| k % every == 0 || half - k.abs() < 2048)
            .flat_map(|k| {
                let tie = (k as f32 + 0.5) / scale;
                [tie.next_down(), tie, tie.next_up()]
            })
            .collect();
        let reference = write_at(Level::Scalar, &data, f, false, 1);
        for &level in &levels {
            assert!(
                write_at(level, &data, f, false, 1) == reference,
                "{f:?} {level:?}"
            );
        }
    }
}

#[test]
fn b4_converters_and_interleaving_never_allocate() {
    let _ = Level::detect(); // the first call probes the CPU and may allocate
    let bytes = vec![0x55_u8; 2 * 64 * 8];
    let mut planar = vec![0.0_f32; 2 * 64];
    let mut wide = vec![0.0_f64; 2 * 64];
    let mut out = vec![0_u8; 2 * 64 * 8];
    let mut inter = vec![0.0_f32; 2 * 64];
    no_alloc(|| {
        for &f in SampleFormat::ALL {
            let n = 2 * 64 * f.bytes_per_sample();
            for (e, inter_bytes) in [(Endianness::Big, false), (Endianness::Little, true)] {
                let mut b = AudioBlockMut::new(&mut planar, 2, 64, 64).unwrap();
                read_into(&bytes[..n], f, e, inter_bytes, 2, &mut b).unwrap();
                write_from(&b.as_block(), &mut out[..n], f, e, inter_bytes).unwrap();
                let mut w = AudioBlockMut::new(&mut wide, 2, 64, 64).unwrap();
                read_into(&bytes[..n], f, e, inter_bytes, 2, &mut w).unwrap();
                write_from(&w.as_block(), &mut out[..n], f, e, inter_bytes).unwrap();
            }
            let m = 64 * f.bytes_per_sample();
            convert_bytes(
                &bytes[..m],
                f,
                Endianness::Little,
                &mut out[..128],
                Int16,
                Endianness::Big,
            )
            .unwrap();
        }
        let b = AudioBlock::new(&planar, 2, 64, 64).unwrap();
        interleave(&b, &mut inter).unwrap();
        let mut b = AudioBlockMut::new(&mut planar, 2, 64, 64).unwrap();
        deinterleave(&inter, &mut b).unwrap();
        // Errors do not allocate either.
        assert!(interleave(&b.as_block(), &mut inter[1..]).is_err());
        assert!(read_into(&bytes[..3], Int32, Endianness::Little, false, 2, &mut b).is_err());
    });
}
