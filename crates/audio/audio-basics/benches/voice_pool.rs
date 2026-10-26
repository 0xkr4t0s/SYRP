// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B8: Callgrind instruction counts for `VoicePool::render` (ADR 0003 D7), in the harness style
//! of `syrp-simd`'s `benches/instructions.rs`.
//!
//! `cargo bench -p syrp-audio-basics --bench voice_pool` re-runs this binary under
//! `valgrind --tool=callgrind` once per case, counting only the instructions inside
//! `measured_region`: one 512-frame stereo render of 16 sounding saw voices, as 16 `f32` packs
//! and as 2 `F32x8` packs. It prints both counts and their ratio, and skips (exit 0) when
//! Valgrind is missing. It fails if the `F32x8` pool costs more than half the scalar one.

use std::hint::black_box;
use std::process::{Command, ExitCode};

use syrp_audio_basics::AudioBuffer;
use syrp_audio_basics::voice::{NoteStart, Voice, VoiceEvent, VoicePool, VoiceSample};
use syrp_simd::F32x8;

const FRAMES: usize = 512;
const VOICES: usize = 16;

/// The test voice of B1 without its call log: a naive saw times a linear gate.
struct SawVoice<S> {
    phase: S,
    increment: S,
    gate: S,
    target: S,
    step: S,
}

impl<S: VoiceSample> Voice<S> for SawVoice<S> {
    fn prepare(&mut self, _: f64, _: usize, _: usize) {}
    fn start(&mut self, lane: usize, n: &NoteStart<'_>) {
        self.phase.set_lane(lane, -1.0);
        self.increment
            .set_lane(lane, (160.0 + f32::from(n.note())) / 24_000.0);
        self.target.set_lane(lane, n.velocity());
        self.step.set_lane(lane, n.velocity() / 8.0);
    }
    fn stop(&mut self, lane: usize, _: f32, allow_tail_off: bool) {
        self.target.set_lane(lane, 0.0);
        if !allow_tail_off {
            self.gate.set_lane(lane, 0.0);
        }
    }
    fn render(&mut self, out: &mut [&mut [S]], frames: usize) {
        for i in 0..frames {
            let up = self.gate.simd_lt(self.target);
            let rising = (self.gate + self.step).min(self.target);
            let falling = (self.gate - self.step).max(self.target);
            self.gate = S::select(up, rising, falling);
            let y = self.phase * self.gate;
            out.iter_mut().for_each(|ch| ch[i] = y);
            self.phase += self.increment;
            let wrapped = self.phase - S::splat(2.0);
            self.phase = S::select(self.phase.simd_ge(S::one()), wrapped, self.phase);
        }
    }
    fn active(&self) -> S::Mask {
        self.gate.simd_gt(S::zero()) | self.target.simd_gt(S::zero())
    }
}

/// A prepared pool with all 16 voices sounding and their gates fully open.
fn sounding<S: VoiceSample>() -> VoicePool<S, SawVoice<S>> {
    let z = S::zero();
    let voice = |_| SawVoice {
        phase: z,
        increment: z,
        gate: z,
        target: z,
        step: z,
    };
    let mut pool = VoicePool::new(VOICES, voice).expect("16 voices");
    pool.prepare(48_000.0, FRAMES, 2).expect("stereo scratch");
    let notes: Vec<_> = (0..VOICES as u8)
        .map(|n| VoiceEvent::note_on(0, 1, 48 + n, 0.5))
        .collect();
    let mut warm = AudioBuffer::new(2, 64).expect("buffer");
    pool.render(warm.as_block_mut(), &notes);
    pool
}

/// Callgrind collects only inside this function (its name is the toggle pattern).
#[inline(never)]
fn measured_region<S: VoiceSample>(
    pool: &mut VoicePool<S, SawVoice<S>>,
    out: &mut AudioBuffer<f32>,
) {
    pool.render(out.as_block_mut(), &[]);
}

/// Child mode: run one case.
fn child(case: &str) -> ExitCode {
    let mut out = AudioBuffer::new(2, FRAMES).expect("buffer");
    match case {
        "f32" => measured_region(black_box(&mut sounding::<f32>()), &mut out),
        "f32x8" => measured_region(black_box(&mut sounding::<F32x8>()), &mut out),
        _ => unreachable!("unknown case {case}"),
    }
    black_box(out);
    ExitCode::SUCCESS
}

/// Instructions Callgrind collected for one case.
fn measure(case: &str) -> u64 {
    let exe = std::env::current_exe().expect("bench executable path");
    let out = Command::new("valgrind")
        .args([
            "--tool=callgrind",
            "--callgrind-out-file=/dev/null",
            "--collect-atstart=no",
            "--toggle-collect=*measured_region*",
        ])
        .arg(exe)
        .args(["--child", case])
        .output()
        .expect("valgrind runs");
    assert!(out.status.success(), "{case} failed: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains("Collected :"));
    let n = line
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|n| n.parse().ok());
    n.unwrap_or_else(|| panic!("no Callgrind total for {case}:\n{stderr}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, case] = &args[..]
        && flag == "--child"
    {
        return child(case);
    }
    if Command::new("valgrind").arg("--version").output().is_err() {
        eprintln!("syrp-audio-basics voice_pool bench: valgrind not found, skipping");
        return ExitCode::SUCCESS;
    }
    let (scalar, packed) = (measure("f32"), measure("f32x8"));
    let ratio = packed as f64 / scalar as f64;
    println!("16 voices x {FRAMES} frames, stereo: f32 {scalar}, F32x8 {packed}, ratio {ratio:.3}");
    if packed * 2 > scalar {
        eprintln!("F32x8 pool costs more than half the scalar pool");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
