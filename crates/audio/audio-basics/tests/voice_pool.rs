// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-05.md (B1–B7; B8 is `benches/voice_pool.rs`).

use proptest::prelude::*;
use syrp_audio_basics::AudioBuffer;
use syrp_audio_basics::voice::{
    MAX_FRAMES, MAX_VOICES, NoteStart, Voice, VoiceEvent, VoiceEventKind, VoiceHandle, VoicePool,
    VoicePoolError, VoiceSample,
};
use syrp_error::Diagnostic;
use syrp_rt::no_alloc;
use syrp_simd::{F32x4, F32x8, Mask, Sample};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// B1's test voice: a naive saw (-1 to 1, 220 Hz plus one hertz per note above 60) times a
/// linear gate that ramps to the velocity in 8 samples on start and to zero on a tail-off.
/// Logs every call except `render`, which it counts, unless `quiet` (logging allocates).
struct SawVoice<S> {
    phase: S,
    increment: S,
    gate: S,
    target: S,
    step: S,
    renders: usize,
    quiet: bool,
    log: Vec<String>,
}

impl<S> SawVoice<S> {
    fn say(&mut self, line: impl FnOnce() -> String) {
        if !self.quiet {
            self.log.push(line());
        }
    }
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
            renders: 0,
            quiet: false,
            log,
        }
    }
}

impl<S: VoiceSample> Voice<S> for SawVoice<S> {
    fn prepare(&mut self, sample_rate: f64, max_frames: usize, out_channels: usize) {
        self.say(|| format!("prepare {sample_rate} {max_frames} {out_channels}"));
    }
    fn start(&mut self, lane: usize, n: &NoteStart<'_>) {
        let (ch, note, vel, wheel, cc7) = (
            n.channel(),
            n.note(),
            n.velocity(),
            n.pitch_wheel(),
            n.controller(7),
        );
        self.say(|| format!("start {lane} {ch} {note} {vel} {wheel} {cc7}"));
        self.phase.set_lane(lane, -1.0);
        self.increment
            .set_lane(lane, (160.0 + f32::from(note)) / 24_000.0);
        self.gate.set_lane(lane, 0.0);
        self.target.set_lane(lane, vel);
        self.step.set_lane(lane, vel / 8.0);
    }
    fn stop(&mut self, lane: usize, _: f32, allow_tail_off: bool) {
        self.say(|| format!("stop {lane} {allow_tail_off}"));
        self.target.set_lane(lane, 0.0);
        if !allow_tail_off {
            self.gate.set_lane(lane, 0.0);
        }
    }
    fn pitch_wheel(&mut self, lane: usize, value: u16) {
        self.say(|| format!("wheel {lane} {value}"));
    }
    fn controller(&mut self, lane: usize, number: u8, value: u8) {
        self.say(|| format!("cc {lane} {number} {value}"));
    }
    fn render(&mut self, out: &mut [&mut [S]], frames: usize) {
        self.renders += 1;
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

/// Renders `frames` frames of `events` into a buffer pre-filled with `fill`.
fn run<S: VoiceSample>(
    p: &mut Pool<S>,
    chans: usize,
    frames: usize,
    fill: f32,
    events: &[VoiceEvent],
) -> AudioBuffer<f32> {
    let mut buf = AudioBuffer::new(chans, frames).unwrap();
    (0..chans).for_each(|c| buf.channel_mut(c).fill(fill));
    p.render(buf.as_block_mut(), events);
    buf
}

fn log<S: VoiceSample>(p: &mut Pool<S>, pack: usize) -> &[String] {
    &p.pack_mut(VoiceHandle::new(pack, 0)).unwrap().log
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
    assert!(p.pack_mut(VoiceHandle::new(1, 1)).is_some());
    assert!(p.pack_mut(VoiceHandle::new(1, 2)).is_none());
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
    assert!(p.prepare(48_000.0, MAX_FRAMES + 1, 0).is_err());
    assert!(p.prepare(48_000.0, MAX_FRAMES, 1).is_ok());
    let err = p.prepare(48_000.0, 16, 65).unwrap_err();
    assert!(matches!(
        err,
        VoicePoolError::CannotPrepare {
            out_channels: 65,
            ..
        }
    ));
}

#[test]
fn b3_events_are_sample_accurate() {
    let mut p = pool::<f32>(4, 256, 1);
    let out = run(&mut p, 1, 256, 0.0, &[VoiceEvent::note_on(100, 1, 60, 1.0)]);
    let ch = out.channel(0);
    assert!(ch[..100].iter().all(|&x| x == 0.0));
    assert!(ch[100] != 0.0);
}

#[test]
fn b3_short_splits_merge_forward() {
    let mut p = pool::<f32>(4, 256, 1);
    p.set_minimum_subdivision(32);
    let out = run(&mut p, 1, 64, 0.0, &[VoiceEvent::note_on(10, 1, 60, 1.0)]);
    assert!(
        out.channel(0)[0] != 0.0,
        "an event 10 samples in is applied at 0"
    );
    let mut p = pool::<f32>(4, 256, 1);
    p.set_minimum_subdivision(32);
    let out = run(&mut p, 1, 64, 0.0, &[VoiceEvent::note_on(40, 1, 60, 1.0)]);
    assert!(out.channel(0)[39] == 0.0 && out.channel(0)[40] != 0.0);
    let mut p = pool::<f32>(4, 256, 1);
    p.set_minimum_subdivision(128);
    let out = run(&mut p, 1, 64, 0.0, &[VoiceEvent::note_on(64, 1, 60, 1.0)]);
    assert!(
        out.channel(0).iter().all(|&x| x == 0.0),
        "the block end is never crossed"
    );
    assert_eq!(p.active_voices(), 1, "the event is applied after the block");
}

#[test]
fn b3_lanes_are_summed_and_added_to_every_channel() {
    let notes = [
        VoiceEvent::note_on(0, 1, 60, 1.0),
        VoiceEvent::note_on(5, 1, 67, 0.5),
    ];
    let both = run(&mut pool::<F32x8>(8, 64, 2), 2, 64, 1.0, &notes);
    let a = run(&mut pool::<F32x8>(8, 64, 2), 2, 64, 0.0, &notes[..1]);
    let b = run(&mut pool::<F32x8>(8, 64, 2), 2, 64, 0.0, &notes[1..]);
    for c in 0..2 {
        for i in 0..64 {
            let want = 1.0 + a.channel(0)[i] + b.channel(0)[i];
            assert!(
                (both.channel(c)[i] - want).abs() < 1e-6,
                "channel {c} frame {i}"
            );
        }
    }
}

#[test]
fn b3_long_blocks_render_in_pieces() {
    let events = [
        VoiceEvent::note_on(3, 1, 60, 1.0),
        VoiceEvent::note_off(250, 1, 60, 0.0),
    ];
    let whole = run(&mut pool::<F32x4>(4, 512, 1), 1, 300, 0.0, &events);
    let pieces = run(&mut pool::<F32x4>(4, 64, 1), 1, 300, 0.0, &events);
    assert_eq!(whole.channel(0), pieces.channel(0));
}

#[test]
fn b3_note_off_tails_and_all_off_events() {
    let mut p = pool::<F32x4>(4, 64, 1);
    let ons = [(1, 61), (2, 62), (2, 63)].map(|(c, n)| VoiceEvent::note_on(0, c, n, 1.0));
    run(&mut p, 1, 16, 0.0, &ons);
    assert_eq!(p.active_voices(), 3);
    run(&mut p, 1, 1, 0.0, &[VoiceEvent::note_off(0, 1, 61, 0.0)]);
    assert_eq!(
        p.active_voices(),
        3,
        "the released lane is still in its tail"
    );
    run(&mut p, 1, 16, 0.0, &[]);
    assert_eq!(p.active_voices(), 2);
    let off = VoiceEventKind::AllNotesOff { channel: Some(2) };
    run(&mut p, 1, 16, 0.0, &[VoiceEvent::new(0, off)]);
    assert_eq!(p.active_voices(), 0);
    run(&mut p, 1, 16, 0.0, &ons);
    let out = run(
        &mut p,
        1,
        16,
        0.0,
        &[VoiceEvent::new(4, VoiceEventKind::AllSoundOff)],
    );
    assert_eq!(p.active_voices(), 0);
    assert!(out.channel(0)[..4].iter().all(|&x| x != 0.0));
    assert!(out.channel(0)[4..].iter().all(|&x| x == 0.0));
}

#[test]
fn b3_render_before_prepare_applies_events_silently() {
    let mut p = Pool::<F32x4>::new(4, |_| SawVoice::new()).unwrap();
    let out = run(&mut p, 2, 64, 0.5, &[VoiceEvent::note_on(3, 1, 60, 1.0)]);
    assert!(
        out.channel(0)
            .iter()
            .chain(out.channel(1))
            .all(|&x| x == 0.5)
    );
    assert_eq!(
        p.active_voices(),
        1,
        "the note is assigned but nothing is rendered"
    );
    p.prepare(48_000.0, 64, 2).unwrap();
    assert!(run(&mut p, 2, 64, 0.0, &[]).channel(1)[63] != 0.0);
}

#[test]
fn b3_render_never_allocates() {
    let mut p = pool::<F32x8>(16, 128, 2);
    let mut buf = AudioBuffer::<f32>::new(2, 512).unwrap();
    let events = [
        VoiceEvent::note_on(0, 1, 60, 1.0),
        VoiceEvent::new(
            7,
            VoiceEventKind::Sustain {
                channel: 1,
                down: true,
            },
        ),
        VoiceEvent::new(
            9,
            VoiceEventKind::Sostenuto {
                channel: 1,
                down: true,
            },
        ),
        VoiceEvent::new(
            100,
            VoiceEventKind::PitchWheel {
                channel: 1,
                value: 0,
            },
        ),
        VoiceEvent::new(
            120,
            VoiceEventKind::Controller {
                channel: 1,
                number: 1,
                value: 9,
            },
        ),
        VoiceEvent::note_off(300, 1, 60, 0.0),
        VoiceEvent::new(400, VoiceEventKind::AllNotesOff { channel: None }),
        VoiceEvent::new(450, VoiceEventKind::AllSoundOff),
    ];
    for pack in 0..2 {
        p.pack_mut(VoiceHandle::new(pack, 0)).unwrap().quiet = true;
    }
    no_alloc(|| p.render(buf.as_block_mut(), &events));
    assert!(buf.channel(1)[299] != 0.0 && buf.channel(1)[450..].iter().all(|&x| x == 0.0));
}

/// Renders `events` in a 16-frame block, enough for any tail to end.
fn step<S: VoiceSample>(p: &mut Pool<S>, events: &[VoiceEvent]) {
    run(p, 1, 16, 0.0, events);
}

fn kind(offset: u32, kind: VoiceEventKind) -> VoiceEvent {
    VoiceEvent::new(offset, kind)
}

fn notes<S: VoiceSample>(p: &Pool<S>) -> Trace {
    (0..p.voices())
        .map(|i| p.note(p.handle(i).unwrap()))
        .collect()
}

#[test]
fn b4_note_on_takes_the_lowest_free_lane() {
    let mut p = pool::<F32x4>(8, 64, 1);
    step(
        &mut p,
        &[60, 62, 64].map(|n| VoiceEvent::note_on(0, 1, n, 1.0)),
    );
    step(&mut p, &[VoiceEvent::note_off(0, 1, 62, 0.0)]);
    step(&mut p, &[]);
    step(&mut p, &[VoiceEvent::note_on(0, 1, 65, 1.0)]);
    let want = [Some((1, 60)), Some((1, 65)), Some((1, 64))];
    assert_eq!(notes(&p)[..4], [want[0], want[1], want[2], None]);
}

#[test]
fn b4_repeated_note_moves_to_a_free_lane_or_reuses_its_own() {
    let mut p = pool::<F32x4>(4, 64, 1);
    step(&mut p, &[VoiceEvent::note_on(0, 1, 60, 1.0)]);
    run(&mut p, 1, 1, 0.0, &[VoiceEvent::note_on(0, 1, 60, 0.5)]);
    assert_eq!(
        log(&mut p, 0)[2..],
        ["stop 0 true", "start 1 1 60 0.5 8192 0"]
    );
    assert_eq!(
        notes(&p)[..2],
        [Some((1, 60)), Some((1, 60))],
        "lane 0 is in its tail"
    );
    let mut p = pool::<f32>(1, 64, 1);
    step(&mut p, &[VoiceEvent::note_on(0, 1, 60, 1.0)]);
    step(&mut p, &[VoiceEvent::note_on(0, 1, 60, 0.5)]);
    assert_eq!(
        log(&mut p, 0)[2..],
        ["stop 0 true", "start 0 1 60 0.5 8192 0"]
    );
    assert_eq!(p.active_voices(), 1);
}

#[test]
fn b4_note_off_stops_only_its_channel_and_note() {
    let mut p = pool::<F32x4>(4, 64, 1);
    let ons = [(1, 60), (2, 60), (1, 61)].map(|(c, n)| VoiceEvent::note_on(0, c, n, 1.0));
    step(&mut p, &ons);
    step(&mut p, &[VoiceEvent::note_off(0, 1, 60, 0.0)]);
    step(&mut p, &[]);
    assert_eq!(notes(&p), [None, Some((2, 60)), Some((1, 61)), None]);
}

#[test]
fn b4_sustain_holds_released_keys_until_the_pedal_lifts() {
    let mut p = pool::<F32x4>(4, 64, 1);
    let down = |d| {
        kind(
            0,
            VoiceEventKind::Sustain {
                channel: 1,
                down: d,
            },
        )
    };
    step(&mut p, &[down(true), VoiceEvent::note_on(0, 1, 60, 1.0)]);
    step(&mut p, &[VoiceEvent::note_off(0, 1, 60, 0.0)]);
    step(&mut p, &[]);
    assert_eq!(p.active_voices(), 1);
    assert!(!log(&mut p, 0).iter().any(|l| l.starts_with("stop")));
    step(&mut p, &[down(false)]);
    step(&mut p, &[]);
    assert_eq!(
        (p.active_voices(), log(&mut p, 0).last().unwrap().as_str()),
        (0, "stop 0 true")
    );
}

#[test]
fn b4_sostenuto_holds_only_keys_down_when_pressed() {
    let mut p = pool::<F32x4>(4, 64, 1);
    let sos = |d| {
        kind(
            0,
            VoiceEventKind::Sostenuto {
                channel: 1,
                down: d,
            },
        )
    };
    let sus = |d| {
        kind(
            0,
            VoiceEventKind::Sustain {
                channel: 1,
                down: d,
            },
        )
    };
    step(&mut p, &[VoiceEvent::note_on(0, 1, 60, 1.0), sos(true)]);
    step(&mut p, &[VoiceEvent::note_on(0, 1, 62, 1.0)]);
    step(
        &mut p,
        &[60, 62].map(|n| VoiceEvent::note_off(0, 1, n, 0.0)),
    );
    step(&mut p, &[]);
    assert_eq!(
        notes(&p)[..2],
        [Some((1, 60)), None],
        "62 was pressed after the pedal"
    );
    step(&mut p, &[sus(true), sos(false)]);
    step(&mut p, &[]);
    assert_eq!(notes(&p)[0], Some((1, 60)), "sustain still holds it");
    step(&mut p, &[sus(false)]);
    step(&mut p, &[]);
    assert_eq!(p.active_voices(), 0);
    // A repeated "down" neither captures later keys nor drops what the first press holds.
    step(&mut p, &[VoiceEvent::note_on(0, 1, 60, 1.0), sos(true)]);
    step(&mut p, &[VoiceEvent::note_on(0, 1, 62, 1.0), sos(true)]);
    step(
        &mut p,
        &[60, 62].map(|n| VoiceEvent::note_off(0, 1, n, 0.0)),
    );
    step(&mut p, &[]);
    assert_eq!(notes(&p)[..2], [Some((1, 60)), None]);
}

/// A full scalar pool holding `held` (started in order), with every gate fully open.
fn full(held: &[u8]) -> Pool<f32> {
    let mut p = pool::<f32>(held.len(), 64, 1);
    let ons: Vec<_> = held
        .iter()
        .map(|&n| VoiceEvent::note_on(0, 1, n, 1.0))
        .collect();
    run(&mut p, 1, 16, 0.0, &ons);
    p
}

fn stolen(p: &mut Pool<f32>, voice: usize) -> &[String] {
    let log = log(p, voice);
    &log[log.len() - 2..]
}

#[test]
fn b5_steals_the_oldest_released_lane_first() {
    let mut p = full(&[60, 61, 62, 63]);
    let offs = [61, 60].map(|n| VoiceEvent::note_off(0, 1, n, 0.0));
    run(&mut p, 1, 1, 0.0, &offs);
    assert_eq!(
        p.active_voices(),
        4,
        "both released lanes are in their tails"
    );
    run(&mut p, 1, 1, 0.0, &[VoiceEvent::note_on(0, 1, 70, 1.0)]);
    assert_eq!(stolen(&mut p, 0), ["stop 0 false", "start 0 1 70 1 8192 0"]);
    assert_eq!(
        notes(&p),
        [Some((1, 70)), Some((1, 61)), Some((1, 62)), Some((1, 63))]
    );
}

#[test]
fn b5_then_the_oldest_held_lane_that_is_not_lowest_or_highest() {
    let mut p = full(&[60, 67, 64, 62]);
    run(&mut p, 1, 1, 0.0, &[VoiceEvent::note_on(0, 1, 70, 1.0)]);
    assert_eq!(stolen(&mut p, 2), ["stop 0 false", "start 0 1 70 1 8192 0"]);
    assert_eq!(notes(&p)[2], Some((1, 70)));
}

#[test]
fn b5_then_the_oldest_held_lane() {
    let mut p = full(&[62, 60]);
    run(&mut p, 1, 1, 0.0, &[VoiceEvent::note_on(0, 1, 70, 1.0)]);
    assert_eq!(notes(&p), [Some((1, 70)), Some((1, 60))]);
    run(&mut p, 1, 1, 0.0, &[VoiceEvent::note_on(0, 1, 72, 1.0)]);
    assert_eq!(notes(&p), [Some((1, 70)), Some((1, 72))]);
}

#[test]
fn b5_without_stealing_the_new_note_is_dropped() {
    let mut p = full(&[60, 62, 64]);
    p.set_note_stealing(false);
    run(&mut p, 1, 1, 0.0, &[VoiceEvent::note_on(0, 1, 70, 1.0)]);
    assert_eq!(notes(&p), [Some((1, 60)), Some((1, 62)), Some((1, 64))]);
    assert!((0..3).all(|v| !log(&mut p, v).iter().any(|l| l.contains(" 70 "))));
}

#[test]
fn b6_silent_packs_sleep_and_finished_lanes_free() {
    let mut p = pool::<F32x8>(64, 128, 2);
    let renders = |p: &mut Pool<F32x8>| -> Vec<usize> {
        (0..8)
            .map(|k| p.pack_mut(VoiceHandle::new(k, 0)).unwrap().renders)
            .collect()
    };
    let ons = [
        VoiceEvent::note_on(0, 1, 60, 1.0),
        VoiceEvent::note_on(0, 1, 64, 1.0),
    ];
    run(&mut p, 2, 128, 0.0, &ons);
    assert_eq!(renders(&mut p), [1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(p.active_voices(), 2);
    run(&mut p, 2, 128, 0.0, &[VoiceEvent::note_off(0, 1, 60, 0.0)]);
    assert_eq!(
        (p.active_voices(), notes(&p)[..2].to_vec()),
        (1, vec![None, Some((1, 64))])
    );
    run(&mut p, 2, 128, 0.0, &[VoiceEvent::note_off(0, 1, 64, 0.0)]);
    let out = run(&mut p, 2, 128, 0.0, &[]);
    assert_eq!(p.active_voices(), 0);
    assert_eq!(
        renders(&mut p),
        [3, 0, 0, 0, 0, 0, 0, 0],
        "the idle pack is skipped"
    );
    assert!(out.channel(0).iter().all(|&x| x == 0.0));
}

/// One block of a random event stream: up to six events, sorted by offset, in 64 frames.
fn block() -> impl Strategy<Value = Vec<VoiceEvent>> {
    let event = (0_u32..64, 0_u8..12, 1_u8..=2, 60_u8..68, 0.1_f32..1.0).prop_map(
        |(at, k, channel, note, velocity)| {
            let down = velocity > 0.5;
            kind(
                at,
                match k {
                    0..=5 => VoiceEventKind::NoteOn {
                        channel,
                        note,
                        velocity,
                    },
                    6..=8 => VoiceEventKind::NoteOff {
                        channel,
                        note,
                        velocity,
                    },
                    9 => VoiceEventKind::Sustain { channel, down },
                    10 => VoiceEventKind::Sostenuto { channel, down },
                    _ => VoiceEventKind::AllNotesOff {
                        channel: down.then_some(channel),
                    },
                },
            )
        },
    );
    prop::collection::vec(event, 0..6).prop_map(|mut b| {
        b.sort_by_key(VoiceEvent::offset);
        b
    })
}

/// The `(channel, note)` of every voice, by voice index.
type Trace = Vec<Option<(u8, u8)>>;

/// Renders `blocks` (64 frames each) and returns the output and the lane trace per block.
fn play<S: VoiceSample>(voices: usize, blocks: &[Vec<VoiceEvent>]) -> (Vec<f32>, Vec<Trace>) {
    let mut p = pool::<S>(voices, 64, 1);
    let (mut out, mut trace) = (Vec::new(), Vec::new());
    for b in blocks {
        out.extend_from_slice(run(&mut p, 1, 64, 0.0, b).channel(0));
        trace.push(notes(&p));
    }
    (out, trace)
}

/// `f32-arith` (4 ulp, or 1e-7 near zero) scaled to the summed magnitude: each of `voices`
/// voices is within ±1, and a pack sums its lanes before adding them to the output.
fn close(got: &[f32], want: &[f32], voices: usize) -> bool {
    let tol = 1e-7_f32.max(4.0 * f32::EPSILON * voices as f32);
    got.len() == want.len() && got.iter().zip(want).all(|(g, w)| (g - w).abs() <= tol)
}

proptest! {
    #[test]
    fn b7_scalar_and_pack_pools_agree(blocks in prop::collection::vec(block(), 1..12)) {
        let (scalar, trace) = play::<f32>(16, &blocks);
        for (out, t) in [play::<F32x4>(16, &blocks), play::<F32x8>(16, &blocks)] {
            prop_assert!(close(&out, &scalar, 16));
            prop_assert_eq!(&t, &trace);
        }
        let (odd, _) = play::<F32x8>(10, &blocks);
        prop_assert!(close(&odd, &play::<f32>(10, &blocks).0, 10));
    }

    #[test]
    fn b7_no_note_is_lost_or_doubled(blocks in prop::collection::vec(block(), 1..12)) {
        let mut p = pool::<f32>(5, 64, 1);
        let mut note_ons = 0;
        for b in &blocks {
            note_ons += b.iter().filter(|e| matches!(e.kind(), VoiceEventKind::NoteOn { .. })).count();
            run(&mut p, 1, 64, 0.0, b);
            let sounding: Vec<_> = (0..5)
                .filter(|&v| log(&mut p, v).last().is_some_and(|l| l.starts_with("start")))
                .collect();
            let held: Vec<_> = sounding.iter().map(|&v| p.note(VoiceHandle::new(v, 0))).collect();
            let mut unique = held.clone();
            unique.sort_unstable();
            unique.dedup();
            prop_assert_eq!(unique.len(), held.len(), "a note is held twice: {:?}", held);
        }
        step(&mut p, &[kind(0, VoiceEventKind::AllNotesOff { channel: None })]);
        step(&mut p, &[]);
        prop_assert_eq!(p.active_voices(), 0);
        let mut starts = 0;
        for v in 0..5 {
            let calls = &log(&mut p, v)[1..];
            starts += calls.iter().filter(|l| l.starts_with("start")).count();
            // Every start is ended by at least one stop before the next start or the end.
            let ended = calls.windows(2).all(|w| !(w[0].starts_with("start") && w[1].starts_with("start")));
            prop_assert!(ended && calls.last().is_none_or(|l| l.starts_with("stop")), "{:?}", calls);
        }
        prop_assert_eq!(starts, note_ons, "every note-on starts a voice");
    }
}
