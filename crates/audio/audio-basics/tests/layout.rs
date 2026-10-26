// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6: ADR 0003 D7 for `AudioBuffer` (random set_size, write and clear sequences keep the
//! layout invariants and the kept content), and `RtSafe` plus `no_alloc` for every public
//! operation.

use proptest::prelude::*;
use syrp_audio_basics::{AudioBuffer, vector_ops as v};
use syrp_rt::{RtSafe, no_alloc};
use syrp_simd::Level;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

#[derive(Debug, Clone)]
enum Op {
    SetSize(usize, usize, bool, bool, bool),
    Write(usize, usize, f32),
    Clear,
    ClearChannel(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (
            0usize..5,
            0usize..80,
            any::<bool>(),
            any::<bool>(),
            any::<bool>()
        )
            .prop_map(|(c, f, k, x, a)| Op::SetSize(c, f, k, x, a)),
        (0usize..5, 0usize..80, -1.0_f32..1.0).prop_map(|(c, i, x)| Op::Write(c, i, x)),
        Just(Op::Clear),
        (0usize..5).prop_map(Op::ClearChannel),
    ]
}

/// The expected samples: `None` where the content is unspecified.
type Model = Vec<Vec<Option<f32>>>;

fn apply(buf: &mut AudioBuffer<f32>, model: &mut Model, op: Op) {
    let (channels, frames) = (buf.channels(), buf.frames());
    match op {
        Op::SetSize(c, f, keep, clear_extra, avoid) => {
            buf.set_size(c, f, keep, clear_extra, avoid).unwrap();
            let extra = if clear_extra { Some(0.0) } else { None };
            *model = (0..c)
                .map(|ch| {
                    (0..f)
                        .map(|i| match keep && ch < channels && i < frames {
                            true => model[ch][i],
                            false => extra,
                        })
                        .collect()
                })
                .collect();
        }
        Op::Write(c, i, x) if c < channels && i < frames => {
            buf.channel_mut(c)[i] = x;
            model[c][i] = Some(x);
        }
        Op::Clear => {
            buf.clear();
            model.iter_mut().flatten().for_each(|s| *s = Some(0.0));
        }
        Op::ClearChannel(c) if c < channels => {
            buf.clear_channel(c, 0..frames);
            if !buf.is_clear() {
                model[c].iter_mut().for_each(|s| *s = Some(0.0));
            }
        }
        _ => {}
    }
}

fn check(buf: &AudioBuffer<f32>, model: &Model) {
    let block = buf.as_block();
    assert_eq!(
        (block.channels(), block.frames()),
        (model.len(), buf.frames())
    );
    assert!(block.is_simd_aligned());
    assert_eq!(block.stride() * 4 % 64, 0);
    assert!(block.stride() >= buf.frames().max(1));
    for (ch, want) in model.iter().enumerate() {
        let got = buf.channel(ch);
        assert_eq!(got.as_ptr() as usize % 64, 0);
        assert_eq!(got.len(), want.len());
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            if buf.is_clear() {
                assert_eq!(*g, 0.0, "clear buffer, channel {ch} frame {i}");
            } else if let Some(w) = w {
                assert_eq!(g, w, "channel {ch} frame {i}");
            }
        }
    }
}

proptest! {
    #[test]
    fn b6_random_sequences_keep_layout_and_content(ops in prop::collection::vec(op(), 1..40)) {
        let mut buf = AudioBuffer::<f32>::new(2, 16).unwrap();
        let mut model: Model = vec![vec![Some(0.0); 16]; 2];
        for op in ops {
            apply(&mut buf, &mut model, op);
            check(&buf, &model);
        }
    }
}

#[test]
fn b6_every_public_operation_is_rt_safe() {
    fn rt_safe<T: RtSafe>() {}
    rt_safe::<AudioBuffer<f32>>();
    rt_safe::<AudioBuffer<f64>>();
    let _ = Level::detect();
    let src = AudioBuffer::<f64>::new(2, 64).unwrap();
    let mut buf = AudioBuffer::<f32>::new(2, 64).unwrap();
    let (mut a, b, fixed) = ([0.5_f32; 37], [0.25_f32; 37], [7_i32; 37]);
    no_alloc(|| {
        let _ = (buf.channels(), buf.frames(), buf.as_block(), buf.channel(1));
        buf.channel_mut(0)[3] = 1.0;
        let _ = buf.as_block_mut();
        buf.set_size(1, 100, true, true, true).unwrap();
        buf.make_copy_of(&src, true).unwrap();
        for level in Level::supported() {
            v::fill(&mut a, 0.5);
            v::copy(&mut a, &b);
            v::copy_with_multiply(level, &mut a, &b, 0.5);
            v::add_with_multiply(level, &mut a, &b, 0.5);
            v::add(level, &mut a, &b);
            v::add_scalar(level, &mut a, 0.5);
            v::subtract(level, &mut a, &b);
            v::multiply(level, &mut a, &b);
            v::multiply_scalar(level, &mut a, 0.5);
            v::negate(level, &mut a);
            v::abs(level, &mut a);
            v::min(level, &mut a, &b);
            v::min_scalar(level, &mut a, 0.5);
            v::max(level, &mut a, &b);
            v::max_scalar(level, &mut a, 0.5);
            v::clip(level, &mut a, -0.5, 0.5);
            let _ = (v::find_minimum(level, &a), v::find_maximum(level, &a));
            let _ = v::find_min_max(level, &a);
            v::convert_fixed_to_float(level, &mut a, &fixed, 0.5);
            v::clear(&mut a);
        }
    });
    assert!(buf.is_clear());
}
