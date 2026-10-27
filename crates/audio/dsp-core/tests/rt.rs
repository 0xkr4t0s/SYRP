// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B7: every audio-thread call of the crate in one
//! allocation guard, and the error catalogue.

use syrp_audio_api::{AudioBlock, AudioBlockMut, ProcessConfig};
use syrp_dsp_core::special::{asne, bessel_i0, cde, elliptic_integral_k, sne};
use syrp_dsp_core::{
    Complex, DspError, DspProcessor, DspSample, DspSpec, FastMathApproximations as Fast, Fft,
    LaneProcessor, LogRampedValue, LookupTable, LookupTableTransform, Matrix, Phase, Polynomial,
    ProcessContext, ProcessorChain, ProcessorDuplicator, ProcessorWrapper, WindowingFunction,
    WindowingMethod,
};
use syrp_error::Diagnostic;
use syrp_rt::no_alloc;
use syrp_simd::{F32x8, Sample};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// A lane-generic one-pole smoother for the duplicator.
struct Smooth<S> {
    y: S,
}

impl<S: DspSample<Element = f32>> LaneProcessor<f32> for Smooth<S> {
    type Lanes = S;
    fn prepare(&mut self, _: &DspSpec, _: &f32) -> Result<(), DspError> {
        Ok(())
    }
    fn reset(&mut self) {
        self.y = S::zero();
    }
    fn process_lanes(&mut self, c: &f32, xs: &mut [S], _: bool) {
        for x in xs {
            self.y += (*x - self.y) * S::splat(*c);
            *x = self.y;
        }
    }
}

#[test]
fn b7_every_audio_thread_call_is_allocation_free() {
    let spec = DspSpec::new(ProcessConfig::new(48_000.0, 256).unwrap(), 10);
    let gain = |_: &mut (), ctx: &mut ProcessContext<'_, f32>| {
        ctx.copy_input_to_output();
        ctx.output_block().channel_mut(0)[0] *= 0.5;
    };
    let mut chain = ProcessorChain::new((
        ProcessorWrapper::new((), gain),
        ProcessorDuplicator::new(0.1_f32, || Smooth { y: F32x8::zero() }),
    ));
    chain.prepare(&spec).unwrap();
    let mut fft = Fft::new(9).unwrap();
    let window = WindowingFunction::<f32>::new(512, WindowingMethod::Kaiser, true, 6.0).unwrap();
    let table = LookupTable::<f32>::new(|i| i.sqrt(), 128).unwrap();
    let transform = LookupTableTransform::<f32>::new(f32::exp, -1.0, 1.0, 256).unwrap();
    let poly = Polynomial::new(&[0.1_f32, 0.2, 0.3]).unwrap();
    let a = Matrix::<f64>::identity(3).unwrap();
    let mut b = Matrix::from_row_major(3, 1, &[1.0, 2.0, 3.0]).unwrap();
    let mut ramp = LogRampedValue::<f32>::new(0.0);
    ramp.reset(64);
    let mut phase = Phase::<F32x8>::new();

    let input = vec![0.25_f32; 10 * 256];
    let mut output = vec![0.0_f32; 10 * 256];
    let mut data = vec![0.5_f32; 1024];
    let cin = vec![Complex::new(1.0_f32, 0.0); 512];
    let mut cout = vec![Complex::default(); 512];
    let solved_before = a.clone();

    let total = no_alloc(|| {
        let mut ctx = ProcessContext::non_replacing(
            AudioBlock::new(&input, 10, 256, 256).unwrap(),
            AudioBlockMut::new(&mut output, 10, 256, 256).unwrap(),
        );
        chain.process(&mut ctx);
        chain.set_bypassed::<1>(true);
        chain.process(&mut ctx);
        chain.reset();

        fft.perform(&cin, &mut cout, false);
        fft.perform(&cin, &mut cout, true);
        fft.perform_real_only_forward_transform(&mut data, false);
        fft.perform_real_only_inverse_transform(&mut data);
        window.multiply_with_windowing_table(&mut data[..512]);
        fft.perform_frequency_only_forward_transform(&mut data, true);

        let x = F32x8::splat(0.3);
        let maths = Fast::sin(x)
            + Fast::cos(x)
            + Fast::tan(x)
            + Fast::sinh(x)
            + Fast::cosh(x)
            + Fast::tanh(x)
            + Fast::exp(x)
            + Fast::log_nplusone(x);
        ramp.set_target_value(1.0);
        ramp.skip(10);
        let special = elliptic_integral_k(0.5).0
            + sne(Complex::new(0.2, 0.1), 0.5).re
            + cde(Complex::new(0.2, 0.0), 0.5).re
            + asne(Complex::new(0.3, 0.0), 0.5).re
            + bessel_i0(2.0);
        maths.lane(0)
            + phase.advance(F32x8::splat(0.1)).lane(0)
            + table.get(F32x8::splat(10.5)).lane(0)
            + transform.process_sample(0.5_f32)
            + poly.evaluate(x).lane(0)
            + ramp.next_value()
            + special as f32
    });
    assert!(total.is_finite());
    assert!(output.iter().all(|v| v.is_finite()));
    // `Matrix` is design-time maths: `solve` works on a copy, so it allocates and stays out
    // of the guard (crate AGENTS.md, Thread affinity).
    assert!(a.solve(&mut b));
    assert_eq!(a, solved_before, "solve leaves the matrix alone");
}

#[test]
fn b7_every_error_code_is_in_the_dsp_catalogue() {
    // Split with `.lines()`: Windows CI checks out with CRLF.
    let catalogue: Vec<&str> = include_str!("../../../../errors/dsp.toml")
        .lines()
        .collect();
    let has = |code: &str| {
        catalogue
            .iter()
            .any(|l| l.trim() == format!("code = \"{code}\""))
    };
    let spec = DspSpec::new(ProcessConfig::new(48_000.0, usize::MAX).unwrap(), 1);
    let mut dup = ProcessorDuplicator::new(0.0_f32, || Smooth { y: F32x8::zero() });
    let errors = [
        dup.prepare(&spec).unwrap_err(),
        Fft::new(Fft::MAX_ORDER + 1).unwrap_err(),
        LookupTable::<f32>::new(|i| i, 0).unwrap_err(),
        Matrix::from_row_major(1, 2, &[0.0_f32]).unwrap_err(),
    ];
    let codes: Vec<&str> = errors.iter().map(|e| e.code().as_str()).collect();
    assert_eq!(
        codes,
        [
            "SYRP-DSP-0001",
            "SYRP-DSP-0002",
            "SYRP-DSP-0003",
            "SYRP-DSP-0004"
        ]
    );
    for (err, code) in errors.iter().zip(codes) {
        assert!(has(code), "{code} is not in errors/dsp.toml");
        assert!(!err.to_string().is_empty());
    }
}
