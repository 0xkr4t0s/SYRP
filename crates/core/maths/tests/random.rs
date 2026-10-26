//! B4: `Random`.

use syrp_maths::{Random, system_random};

#[test]
fn b4_first_values_are_pinned_for_a_fixed_seed() {
    // SplitMix64 from seed 0; the first value is the published reference output.
    let mut r = Random::new(0);
    assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    assert_eq!(r.next_u64(), 0x06C4_5D18_8009_454F);
    let mut r = Random::new(12345);
    let pinned = [
        r.next_i32(),
        r.next_i32_below(100),
        i32::from(r.next_bool()),
    ];
    assert_eq!(pinned, PINNED_12345);
}

const PINNED_12345: [i32; 3] = [571_572_824, 20, 0];

#[test]
fn b4_same_seed_same_sequence() {
    let mut a = Random::new(99);
    let mut b = Random::new(98);
    b.set_seed(99);
    for _ in 0..1000 {
        assert_eq!(a.next_i64(), b.next_i64());
    }
    assert_ne!(Random::new(1).next_i64(), Random::new(2).next_i64());
}

#[test]
fn b4_bounded_and_unit_interval_values() {
    let mut r = Random::new(7);
    let mut seen = [0u32; 10];
    for _ in 0..10_000 {
        let v = r.next_i32_below(10);
        seen[usize::try_from(v).unwrap()] += 1;
        let (f, d) = (r.next_f32(), r.next_f64());
        assert!((0.0..1.0).contains(&f) && (0.0..1.0).contains(&d));
    }
    assert!(seen.iter().all(|&n| (850..1150).contains(&n)), "{seen:?}");
    assert_eq!(r.next_i32_below(1), 0);
    assert_eq!(r.next_i32_below(-5), 0);
    let big = (0..100).map(|_| r.next_i32_below(i32::MAX)).max().unwrap();
    assert!(big > 1 << 28);
}

#[test]
fn b4_bools_and_bytes_are_balanced() {
    let mut r = Random::new(3);
    let trues = (0..10_000).filter(|_| r.next_bool()).count();
    assert!((4_800..5_200).contains(&trues));
    let mut buf = [0u8; 4096];
    r.fill_bytes(&mut buf);
    let ones: u32 = buf.iter().map(|b| b.count_ones()).sum();
    assert!((16_000..16_800).contains(&ones));
    let mut a = [0u8; 11];
    let mut b = [0u8; 16];
    Random::new(5).fill_bytes(&mut a);
    Random::new(5).fill_bytes(&mut b);
    assert_eq!(a[..8], b[..8]);
    assert_eq!(a[8..], b[8..11]);
}

#[test]
fn b4_entropy_and_system_generators() {
    let (mut a, mut b) = (Random::with_entropy(), Random::with_entropy());
    assert_ne!([a.next_i64(), a.next_i64()], [b.next_i64(), b.next_i64()]);
    let x = system_random().next_f64();
    let y = system_random().next_f64();
    assert!((0.0..1.0).contains(&x) && x != y);
}
