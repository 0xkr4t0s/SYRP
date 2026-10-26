// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-05.md, increment 1 (B1–B2).

use syrp_audio_basics::voice::{
    MAX_VOICES, NoteStart, Voice, VoiceHandle, VoicePool, VoicePoolError, VoiceSample,
};
use syrp_error::Diagnostic;
use syrp_simd::{F32x4, F32x8, Mask, Sample};

/// B1's test voice: a naive saw (-1 to 1, 220 Hz plus one hertz per note above 60) times a
/// linear gate that ramps to the velocity in 8 samples on start and to zero on a tail-off.
/// Logs every call except `render`.
struct SawVoice<S> {
    phase: S,
    increment: S,
    gate: S,
    target: S,
    step: S,
    log: Vec<String>,
}

impl<S: VoiceSample> SawVoice<S> {
    fn new() -> Self {
        let z = S::zero();
        let log = Vec::with_capacity(64);
        Self {
            phase: z,
            increment: z,
            gate: z,
            target: z,
            step: z,
            log,
        }
    }
}

impl<S: VoiceSample> Voice<S> for SawVoice<S> {
    fn prepare(&mut self, sample_rate: f64, max_frames: usize, out_channels: usize) {
        self.log
            .push(format!("prepare {sample_rate} {max_frames} {out_channels}"));
    }
    fn start(&mut self, lane: usize, n: &NoteStart<'_>) {
        let (ch, note, vel, wheel, cc7) = (
            n.channel(),
            n.note(),
            n.velocity(),
            n.pitch_wheel(),
            n.controller(7),
        );
        self.log
            .push(format!("start {lane} {ch} {note} {vel} {wheel} {cc7}"));
        self.phase.set_lane(lane, -1.0);
        self.increment
            .set_lane(lane, (160.0 + f32::from(note)) / 24_000.0);
        self.gate.set_lane(lane, 0.0);
        self.target.set_lane(lane, vel);
        self.step.set_lane(lane, vel / 8.0);
    }
    fn stop(&mut self, lane: usize, _: f32, allow_tail_off: bool) {
        self.log.push(format!("stop {lane} {allow_tail_off}"));
        self.target.set_lane(lane, 0.0);
        if !allow_tail_off {
            self.gate.set_lane(lane, 0.0);
        }
    }
    fn pitch_wheel(&mut self, lane: usize, value: u16) {
        self.log.push(format!("wheel {lane} {value}"));
    }
    fn controller(&mut self, lane: usize, number: u8, value: u8) {
        self.log.push(format!("cc {lane} {number} {value}"));
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
            self.phase = S::select(
                self.phase.simd_ge(S::one()),
                self.phase - S::splat(2.0),
                self.phase,
            );
        }
    }
    fn active(&self) -> S::Mask {
        self.gate.simd_gt(S::zero()) | self.target.simd_gt(S::zero())
    }
}

type Pool<S> = VoicePool<S, SawVoice<S>>;

fn pool<S: VoiceSample>(voices: usize, max_frames: usize, channels: usize) -> Pool<S> {
    let mut pool = Pool::<S>::new(voices, |_| SawVoice::new()).unwrap();
    pool.prepare(48_000.0, max_frames, channels).unwrap();
    pool
}

fn log<S: VoiceSample>(p: &mut Pool<S>, pack: usize) -> &[String] {
    &p.voice_mut(VoiceHandle::new(pack, 0)).unwrap().log
}

#[test]
fn b1_saw_voice_writes_zeros_in_inactive_lanes() {
    let mut v = SawVoice::<F32x8>::new();
    v.start(2, &NoteStart::new(1, 60, 1.0));
    let mut ch = [F32x8::splat(9.0); 16];
    v.render(&mut [&mut ch[..]], 16);
    for x in ch {
        let lanes = x.to_array();
        assert!(lanes[2] != 0.0);
        assert!(lanes.iter().enumerate().all(|(l, &y)| l == 2 || y == 0.0));
    }
    assert_eq!(v.active().to_array(), core::array::from_fn(|l| l == 2));
    v.stop(2, 0.0, false);
    assert!(v.active().none());
}

#[test]
fn b1_note_start_carries_the_channel_state() {
    let mut ccs = [0_u8; 128];
    ccs[7] = 100;
    let note = NoteStart::new(3, 60, 0.5).with_channel_state(1000, &ccs);
    let mut v = SawVoice::<F32x4>::new();
    v.start(1, &note);
    v.pitch_wheel(1, 2000);
    v.controller(1, 1, 5);
    v.stop(1, 0.0, true);
    let want = [
        "start 1 3 60 0.5 1000 100",
        "wheel 1 2000",
        "cc 1 1 5",
        "stop 1 true",
    ];
    assert_eq!(v.log, want);
    let plain = NoteStart::new(16, 127, 1.0);
    assert_eq!(
        (plain.pitch_wheel(), plain.controller(7), plain.channel()),
        (8192, 0, 16)
    );
}

/// A voice that keeps the default `pitch_wheel` and `controller`.
struct Silent(bool);

impl Voice<f32> for Silent {
    fn prepare(&mut self, _: f64, _: usize, _: usize) {}
    fn start(&mut self, _: usize, _: &NoteStart<'_>) {
        self.0 = true;
    }
    fn stop(&mut self, _: usize, _: f32, _: bool) {
        self.0 = false;
    }
    fn render(&mut self, out: &mut [&mut [f32]], frames: usize) {
        out.iter_mut().for_each(|ch| ch[..frames].fill(0.0));
    }
    fn active(&self) -> bool {
        self.0
    }
}

#[test]
fn b1_pitch_wheel_and_controller_default_to_no_ops() {
    let mut v = Silent(false);
    v.start(0, &NoteStart::new(1, 60, 1.0));
    v.pitch_wheel(0, 0);
    v.controller(0, 64, 127);
    assert!(v.active());
    let mut ch = [1.0_f32; 8];
    v.render(&mut [&mut ch[..]], 8);
    assert_eq!(ch, [0.0; 8]);
}

#[test]
fn b2_packs_are_allocated_up_front_and_prepared() {
    let mut seen = Vec::new();
    let p = Pool::<f32>::new(10, |i| {
        seen.push(i);
        SawVoice::new()
    });
    assert_eq!(p.unwrap().packs(), 10);
    assert_eq!(seen, (0..10).collect::<Vec<_>>());
    assert_eq!(
        Pool::<F32x4>::new(10, |_| SawVoice::new()).unwrap().packs(),
        3
    );
    let mut p = pool::<F32x8>(10, 256, 2);
    assert_eq!((p.voices(), p.packs()), (10, 2));
    for pack in 0..2 {
        assert_eq!(log(&mut p, pack), ["prepare 48000 256 2"]);
    }
}

#[test]
fn b2_handles_address_one_voice() {
    let mut p = pool::<F32x8>(10, 16, 1);
    assert_eq!(p.handle(9), Some(VoiceHandle::new(1, 1)));
    assert_eq!(p.handle(10), None);
    assert!(p.voice_mut(VoiceHandle::new(1, 1)).is_some());
    assert!(p.voice_mut(VoiceHandle::new(1, 2)).is_none());
    let p = pool::<f32>(4, 16, 1);
    assert_eq!(p.handle(3), Some(VoiceHandle::new(3, 0)));
}

#[test]
fn b2_oversized_pools_fail_without_aborting() {
    let err = Pool::<f32>::new(usize::MAX, |_| SawVoice::new()).unwrap_err();
    assert!(matches!(
        err,
        VoicePoolError::TooManyVoices {
            voices: usize::MAX,
            ..
        }
    ));
    assert_eq!(err.code().as_str(), "SYRP-AUDIO-0006");
    let zst = VoicePool::<f32, Silent>::new(MAX_VOICES + 1, |_| Silent(false));
    assert!(matches!(zst, Err(VoicePoolError::TooManyVoices { .. })));
    assert_eq!(
        Pool::<F32x8>::new(MAX_VOICES, |_| SawVoice::new())
            .unwrap()
            .packs(),
        8192
    );
    let mut p = pool::<F32x8>(8, 16, 1);
    assert!(p.prepare(48_000.0, usize::MAX, 2).is_err());
    let err = p.prepare(48_000.0, 16, 65).unwrap_err();
    assert!(matches!(
        err,
        VoicePoolError::CannotPrepare {
            out_channels: 65,
            ..
        }
    ));
}
