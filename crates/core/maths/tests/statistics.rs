//! B5: `StatisticsAccumulator`.

use syrp_maths::StatisticsAccumulator;

#[test]
fn b5_basic_statistics() {
    let mut s = StatisticsAccumulator::<f32>::new();
    assert_eq!(
        (s.count(), s.average(), s.variance(), s.min(), s.max()),
        (0, 0.0, 0.0, None, None)
    );
    s.add(4.0);
    assert_eq!((s.count(), s.average(), s.variance()), (1, 4.0, 0.0));
    for v in [-2.0, 10.0, 0.0] {
        s.add(v);
    }
    assert_eq!((s.count(), s.average()), (4, 3.0));
    assert_eq!(s.variance(), 21.0);
    assert!((s.standard_deviation() - 21.0_f32.sqrt()).abs() < 1e-6);
    assert_eq!((s.min(), s.max()), (Some(-2.0), Some(10.0)));
    s.reset();
    assert_eq!(s, StatisticsAccumulator::new());
}

#[test]
fn b5_stable_where_naive_sums_lose_precision() {
    // Large values close together: sum-of-squares minus squared sum cancels catastrophically.
    let offset = 1e9;
    let values = [4.0, 7.0, 13.0, 16.0].map(|v| v + offset);
    let mut s = StatisticsAccumulator::<f64>::new();
    let (mut sum, mut sum_sq) = (0.0, 0.0);
    for _ in 0..250_000 {
        for v in values {
            s.add(v);
            sum += v;
            sum_sq += v * v;
        }
    }
    let n = 1e6;
    let naive = sum_sq / n - (sum / n) * (sum / n);
    assert!(
        (naive - 22.5).abs() > 1.0,
        "the naive formula should fail here: {naive}"
    );
    assert!((s.variance() - 22.5).abs() < 1e-6, "{}", s.variance());
    assert!((s.average() - (offset + 10.0)).abs() < 1e-6);
    // f32 input, f64 accumulation.
    let mut f = StatisticsAccumulator::<f32>::new();
    for i in 0..100_000 {
        f.add(if i % 2 == 0 { 10_000.0 } else { 10_002.0 });
    }
    assert!((f.variance() - 1.0).abs() < 1e-4 && (f.standard_deviation() - 1.0).abs() < 1e-4);
}
