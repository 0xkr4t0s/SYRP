// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests for B3, B4 and B8 of work/specs/WP-0.2-06.md (parameter increment).

use std::sync::Arc;
use std::time::Duration;

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_plugin_api::{
    ParamIndex, ParamLayout, ParamRange, ParamSpec, Params, PluginError, Smoothing,
};
use syrp_rt::{RtSafe, no_alloc};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

fn unit(id: &str) -> ParamSpec {
    ParamSpec::new(id, id.to_uppercase(), ParamRange::new(0.0, 1.0))
}

#[track_caller]
fn close(got: f64, want: f64) {
    assert!((got - want).abs() < 1e-12, "{got} != {want}");
}

#[test]
fn b3_builder_records_every_field() {
    let range = ParamRange::new(-24.0, 24.0).with_step(0.5);
    let smoothing = Smoothing::Multiplicative(Duration::from_millis(50));
    let s = ParamSpec::new("gain", "Gain", range)
        .with_default(-6.0)
        .with_unit("dB")
        .with_smoothing(smoothing)
        .with_automatable(false)
        .with_group("out/level");
    assert_eq!((s.id(), s.name(), s.unit()), ("gain", "Gain", "dB"));
    assert_eq!((s.group(), s.smoothing()), (Some("out/level"), smoothing));
    assert_eq!((*s.range(), s.default_value()), (range, -6.0));
    assert_eq!(s.range().normalise(s.default_value()), 0.375);
    assert!(!s.is_automatable());
}

#[test]
fn b3_skew_gives_the_low_or_high_end_more_travel() {
    let low = ParamRange::new(0.0, 100.0).with_skew(0.5);
    close(low.normalise(25.0), 0.5); // skew < 1 favours the low end
    close(
        ParamRange::new(0.0, 100.0).with_skew(2.0).normalise(25.0),
        0.0625,
    );
    close(
        ParamRange::new(20.0, 20e3)
            .with_centre(632.0)
            .normalise(632.0),
        0.5,
    );
    let sym = ParamRange::new(-1.0, 1.0).with_skew(0.5);
    let sym = sym.with_symmetric_skew(true);
    close(sym.normalise(0.0), 0.5);
    close(sym.normalise(0.25), 0.75); // mirrored about the middle
    close(sym.normalise(-0.25), 0.25);
    close(sym.denormalise(0.75), 0.25);
    let clamped = [
        low.normalise(-1.0),
        low.normalise(1e9),
        low.normalise(f64::NAN),
    ];
    assert_eq!(clamped, [0.0, 1.0, 0.0]);
    assert_eq!([low.denormalise(-1.0), low.denormalise(2.0)], [0.0, 100.0]);
}

#[test]
fn b3_snap_rounds_to_the_step_and_clamps() {
    let r = ParamRange::new(-1.0, 1.0).with_step(0.3);
    assert_eq!(r.snap(-0.8), -1.0 + 0.3); // nearest step from the start
    close(r.snap(0.85), 0.8);
    assert_eq!(r.snap(0.99), 1.0); // the step past the end is clamped back
    assert_eq!([r.snap(-5.0), r.snap(5.0)], [-1.0, 1.0]);
    assert_eq!(ParamRange::new(0.0, 1.0).snap(0.123), 0.123);
    close(r.denormalise(r.normalise(0.1)), 0.1); // denormalise does not snap
}

proptest! {
    /// Each direction is checked where it is well conditioned: plain -> normalised -> plain
    /// everywhere (relative to the span), normalised -> plain -> normalised wherever the
    /// curve's slope is bounded (linear, or skew >= 1). Below 1 the slope at the skew pole is
    /// unbounded, so no float format keeps normalised values near it apart.
    #[test]
    fn b3_normalise_denormalise_round_trip_within_1e_6(
        start in -1.0e4f64..1.0e4,
        span in 1.0e-3f64..1.0e5,
        skew in 0.05f64..8.0,
        symmetric: bool,
        n in prop_oneof![0.0f64..=1.0, Just(0.0), Just(0.5), Just(1.0)],
    ) {
        let linear = ParamRange::new(start, start + span);
        let r = linear.with_skew(skew).with_symmetric_skew(symmetric);
        prop_assert!(r.is_valid());
        let v = r.denormalise(n);
        prop_assert!((r.denormalise(r.normalise(v)) - v).abs() <= 1e-6 * span);
        if skew >= 1.0 {
            prop_assert!((r.normalise(v) - n).abs() <= 1e-6);
        }
        prop_assert!((linear.normalise(linear.denormalise(n)) - n).abs() <= 1e-6);
    }
}

#[test]
fn b3_text_conversion_defaults_and_overrides() {
    let hz = ParamSpec::new("f", "F", ParamRange::new(20.0, 20e3)).with_unit("Hz");
    assert_eq!(hz.value_to_text(440.0), "440.00 Hz");
    let parse = |t: &str| hz.text_to_value(t);
    assert_eq!(
        [parse("440 Hz"), parse("5"), parse("loud"), parse("NaN")],
        [Some(440.0), Some(20.0), None, None]
    );
    let stepped = ParamSpec::new("s", "S", ParamRange::new(0.0, 1.0).with_step(0.01));
    assert_eq!(stepped.value_to_text(0.5), "0.50");
    close(stepped.text_to_value("0.333").unwrap(), 0.33);
    let shown = |start: f64, step: f64, v: f64| {
        let r = ParamRange::new(start, 10.0).with_step(step);
        ParamSpec::new("s", "S", r).value_to_text(v)
    };
    assert_eq!(shown(0.0, 0.25, 1.25), "1.25");
    assert_eq!(shown(0.0, 0.125, 0.375), "0.375");
    assert_eq!(shown(0.0, 0.025, 0.075), "0.075");
    assert_eq!(shown(0.5, 1.0, 2.5), "2.5"); // an offset start needs its own decimals
    assert_eq!([shown(0.0, 0.1, 0.3), shown(0.0, 1.0, 3.0)], ["0.3", "3"]);
    const MODES: [&str; 3] = ["Low", "Mid", "High"];
    let mode = ParamSpec::new("m", "M", ParamRange::new(0.0, 2.0).with_step(1.0))
        .with_value_to_text(|v| MODES[v as usize].to_owned())
        .with_text_to_value(|t| MODES.iter().position(|m| *m == t).map(|i| i as f64 + 0.4));
    assert_eq!(mode.value_to_text(1.0), "Mid");
    assert_eq!(mode.text_to_value("High"), Some(2.0)); // custom results are snapped too
}

#[test]
fn b3_invalid_specs_are_rejected_with_a_code() {
    let range = |r: ParamRange| ParamSpec::new("a", "A", r);
    let bad = [
        ParamSpec::new("", "Empty id", ParamRange::new(0.0, 1.0)),
        unit("a").with_default(2.0),
        unit("a").with_default(f64::NAN),
        range(ParamRange::new(1.0, 1.0)),
        range(ParamRange::new(0.0, f64::INFINITY)),
        range(ParamRange::new(0.0, 1.0).with_step(-1.0)),
        range(ParamRange::new(0.0, 1.0).with_skew(0.0)),
        range(ParamRange::new(0.0, 1.0).with_centre(2.0)),
    ];
    let mut layout = ParamLayout::new();
    for spec in bad {
        let err = layout.register(spec).unwrap_err();
        assert_eq!(err, PluginError::InvalidParamSpec);
        assert_eq!(err.code().as_str(), "SYRP-PLUGIN-0002");
    }
    assert!(layout.is_empty());
}

#[test]
fn b4_indices_are_dense_in_registration_order() {
    let mut layout = ParamLayout::new();
    let ids = ["zeta", "alpha", "mid", "beta"];
    for (i, id) in (0u32..).zip(ids) {
        assert_eq!(layout.register(unit(id)).unwrap(), ParamIndex::new(i));
    }
    for (i, id) in ids.iter().enumerate() {
        let index = layout.index_of(id).unwrap();
        assert_eq!((index.get() as usize, index.as_usize()), (i, i));
        assert_eq!(layout.spec(index).map(ParamSpec::id), Some(*id));
        assert_eq!(layout.specs()[i].id(), *id);
    }
    assert_eq!(layout.index_of("gamma"), None);
    assert!(layout.spec(ParamIndex::new(4)).is_none());
    let err = layout.register(unit("mid").with_default(0.5)).unwrap_err();
    assert_eq!(err, PluginError::DuplicateParamId);
    assert_eq!(err.code().as_str(), "SYRP-PLUGIN-0001");
    assert_eq!(layout.len(), 4); // a rejected registration leaves the layout unchanged
}

#[test]
fn b4_values_live_in_one_atomic_array_shared_across_threads() {
    let mut layout = ParamLayout::new();
    let a = layout.register(unit("a").with_default(0.25)).unwrap();
    let b = layout.register(unit("b")).unwrap();
    let params = Arc::new(Params::new(&layout).unwrap());
    assert_eq!((params.len(), params.get(a), params.get(b)), (2, 0.25, 0.0));
    let writer = Arc::clone(&params);
    std::thread::spawn(move || writer.set(b, 0.75))
        .join()
        .unwrap();
    let past_end = ParamIndex::new(2);
    let read = no_alloc(|| (params.get(b), params.try_get(a), params.try_get(past_end)));
    assert_eq!(read, (0.75, Some(0.25), None));
    fn rt_safe<T: RtSafe>() {}
    rt_safe::<ParamIndex>();
    let code = PluginError::AllocationFailed.code();
    assert_eq!(code.as_str(), "SYRP-PLUGIN-0003");
}

#[test]
fn b8_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/range.rs"),
        include_str!("../src/param.rs"),
        include_str!("../src/layout.rs"),
    ];
    let mut in_struct = false;
    for line in src.iter().flat_map(|s| s.lines()) {
        if line.starts_with("pub struct") {
            assert!(!line.contains("(pub"), "public tuple field: {line}");
            in_struct = line.ends_with('{');
        } else if line.starts_with('}') {
            in_struct = false;
        } else if in_struct {
            let field = line.trim_start();
            assert!(!field.starts_with("pub"), "public field: {line}");
        }
    }
}
