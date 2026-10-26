//! B6: no allocation on the audio-thread paths, and `RtSafe` impls.

use syrp_maths::{
    NormalisableRange, Random, Range, StatisticsAccumulator, approximately_equal, limit,
    map_from_log10, map_range, map_to_log10, next_power_of_two, round_to_int,
};
use syrp_rt::{GuardedAllocator, RtSafe, no_alloc};

#[global_allocator]
static ALLOC: GuardedAllocator = GuardedAllocator;

fn assert_rt_safe<T: RtSafe>() {}

#[test]
fn b6_audio_thread_types_are_rt_safe() {
    assert_rt_safe::<Range<i32>>();
    assert_rt_safe::<Range<f64>>();
    assert_rt_safe::<NormalisableRange<f32>>();
    assert_rt_safe::<Random>();
    assert_rt_safe::<StatisticsAccumulator<f32>>();
}

#[test]
fn b6_no_allocation_in_range_helpers_random_and_statistics() {
    let nr = NormalisableRange::with_centre(20.0_f32, 20_000.0, 1_000.0)
        .unwrap()
        .with_interval(1.0)
        .unwrap();
    let mut random = Random::new(1);
    let mut stats = StatisticsAccumulator::<f32>::new();
    let mut bytes = [0u8; 32];
    let total = no_alloc(|| {
        let r = Range::new(10, 0)
            .get_intersection_with(Range::new(5, 20))
            .expanded(2);
        let mut acc = r.length() + Range::new(0, 100).constrain_range(r).start();
        for i in 0..64 {
            let p = random.next_f32();
            let v = nr.snap_to_legal_value(nr.convert_from_0to1(p));
            stats.add(nr.convert_to_0to1(v));
            acc += random.next_i32_below(10) + i32::from(random.next_bool()) + round_to_int(v);
            acc += i32::from(approximately_equal(
                map_range(p, 0.0, 1.0, 0.0, 2.0),
                2.0 * p,
            ));
            acc += round_to_int(map_to_log10(map_from_log10(p, 1.0, 10.0), 1.0, 10.0));
            acc += limit(0, 1, random.next_i32()) + next_power_of_two(i).unwrap_or(0);
        }
        random.fill_bytes(&mut bytes);
        acc.wrapping_add(round_to_int(stats.standard_deviation()) + random.next_i64() as i32)
    });
    assert_ne!(total, 0);
    assert_eq!(stats.count(), 64);
}
