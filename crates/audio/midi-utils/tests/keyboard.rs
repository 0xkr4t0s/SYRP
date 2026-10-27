// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-07.md, increment 3: B3 (`MidiKeyboardState`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use syrp_error::Diagnostic;
use syrp_midi::{MidiBuffer, MidiData, MidiMessage};
use syrp_midi_utils::keyboard::{
    KeyboardError, MidiKeyboardState, MidiKeyboardStateListener, QUEUE_CAPACITY,
};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// One callback: (note on, channel, note, velocity, thread).
type Call = (bool, u8, u8, f32, std::thread::ThreadId);

/// Records every callback, with the thread it ran on.
#[derive(Default)]
struct Log(Mutex<Vec<Call>>);

impl MidiKeyboardStateListener for Log {
    fn handle_note_on(&self, _: &MidiKeyboardState, channel: u8, note: u8, velocity: f32) {
        self.0
            .lock()
            .unwrap()
            .push((true, channel, note, velocity, std::thread::current().id()));
    }
    fn handle_note_off(&self, _: &MidiKeyboardState, channel: u8, note: u8, velocity: f32) {
        self.0
            .lock()
            .unwrap()
            .push((false, channel, note, velocity, std::thread::current().id()));
    }
}

/// Counts callbacks without allocating or locking.
#[derive(Default)]
struct Count(AtomicUsize, AtomicUsize);

impl MidiKeyboardStateListener for Count {
    fn handle_note_on(&self, _: &MidiKeyboardState, _: u8, _: u8, _: f32) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn handle_note_off(&self, _: &MidiKeyboardState, _: u8, _: u8, _: f32) {
        self.1.fetch_add(1, Ordering::Relaxed);
    }
}

fn with_log() -> (MidiKeyboardState, Arc<Log>) {
    let log = Arc::new(Log::default());
    let mut state = MidiKeyboardState::new();
    state.add_listener(log.clone());
    (state, log)
}

fn events(buffer: &MidiBuffer) -> Vec<(Vec<u8>, u32)> {
    buffer
        .iter()
        .map(|e| (e.bytes().to_vec(), e.sample_offset()))
        .collect()
}

#[test]
fn b3_state_is_send_and_sync() {
    fn shared<T: Send + Sync>() {}
    shared::<MidiKeyboardState>();
}

#[test]
fn b3_note_on_and_off_from_the_ui_update_state_and_call_listeners() {
    let (state, log) = with_log();
    state.note_on(1, 60, 1.0).unwrap();
    state.note_on(16, 127, 0.0).unwrap();
    assert!(state.is_note_on(1, 60) && state.is_note_on(16, 127));
    assert!(!state.is_note_on(2, 60) && !state.is_note_on(1, 61));
    assert!(
        state.is_note_on_for_channels(0x0001, 60) && !state.is_note_on_for_channels(0xFFFE, 60)
    );
    state.note_off(1, 60, 0.5).unwrap();
    assert!(!state.is_note_on(1, 60));
    let me = std::thread::current().id();
    let calls = log.0.lock().unwrap().clone();
    // Velocity 0 is sent (and reported) as 1/127 so the note on stays a note on.
    assert_eq!(
        calls,
        [
            (true, 1, 60, 1.0, me),
            (true, 16, 127, 1.0 / 127.0, me),
            (false, 1, 60, 64.0 / 127.0, me)
        ]
    );
}

#[test]
fn b3_note_off_for_a_key_that_is_up_does_nothing() {
    let (state, log) = with_log();
    state.note_off(1, 60, 0.0).unwrap();
    let mut buffer = MidiBuffer::with_capacity(4, 12);
    state
        .process_next_midi_buffer(&mut buffer, 0, 64, true)
        .unwrap();
    assert!(buffer.is_empty() && log.0.lock().unwrap().is_empty());
}

#[test]
fn b3_out_of_range_arguments_are_coded_errors() {
    let state = MidiKeyboardState::new();
    for (c, n, v) in [
        (0, 60, 1.0),
        (17, 60, 1.0),
        (1, 128, 1.0),
        (1, 60, f32::NAN),
    ] {
        let err = state.note_on(c, n, v).unwrap_err();
        assert_eq!(
            (err, err.code().as_str()),
            (KeyboardError::ValueOutOfRange, "SYRP-MIDI-0002")
        );
        assert_eq!(state.note_off(c, n, v), Err(KeyboardError::ValueOutOfRange));
    }
    assert_eq!(
        state.all_notes_off(Some(0)),
        Err(KeyboardError::ValueOutOfRange)
    );
    assert!(!state.is_note_on(1, 200) && !state.is_note_on_for_channels(0xFFFF, 200));
}

#[test]
fn b3_all_notes_off_for_one_channel_or_all() {
    let (state, log) = with_log();
    for (c, n) in [(1, 10), (1, 11), (2, 10), (9, 99)] {
        state.note_on(c, n, 1.0).unwrap();
    }
    state.all_notes_off(Some(1)).unwrap();
    assert!(!state.is_note_on(1, 10) && !state.is_note_on(1, 11) && state.is_note_on(2, 10));
    state.all_notes_off(None).unwrap();
    assert!((0..128).all(|n| !state.is_note_on_for_channels(0xFFFF, n)));
    let offs = log.0.lock().unwrap().iter().filter(|c| !c.0).count();
    assert_eq!(offs, 4);
    let mut buffer = MidiBuffer::with_capacity(16, 64);
    state
        .process_next_midi_buffer(&mut buffer, 0, 1, true)
        .unwrap();
    assert_eq!(buffer.len(), 8);
}

#[test]
fn b3_process_next_midi_event_presses_releases_and_handles_all_notes_off() {
    let (state, log) = with_log();
    state.process_next_midi_event(&MidiMessage::note_on(3, 50, 127).unwrap());
    state.process_next_midi_event(&MidiMessage::note_on(3, 51, 127).unwrap());
    state.process_next_midi_event(&MidiMessage::note_on(3, 50, 0).unwrap());
    assert!(!state.is_note_on(3, 50) && state.is_note_on(3, 51));
    state.process_next_midi_event(&MidiMessage::controller(4, 123, 0).unwrap());
    assert!(state.is_note_on(3, 51));
    state.process_next_midi_event(&MidiMessage::controller(3, 123, 0).unwrap());
    assert!(!state.is_note_on(3, 51));
    state.process_next_midi_event(&MidiMessage::program_change(3, 1).unwrap());
    let kinds: Vec<_> = log.0.lock().unwrap().iter().map(|c| (c.0, c.2)).collect();
    assert_eq!(kinds, [(true, 50), (true, 51), (false, 50), (false, 51)]);
}

#[test]
fn b3_process_next_midi_buffer_reads_the_range_and_injects_ui_events_at_its_start() {
    let state = MidiKeyboardState::new();
    let mut buffer = MidiBuffer::with_capacity(8, 32);
    for (note, at) in [(30, 5), (31, 100), (32, 164)] {
        buffer.try_add_event(&[0x90, note, 9], at).unwrap();
    }
    state.note_on(2, 70, 1.0).unwrap();
    state.note_off(2, 70, 0.0).unwrap();
    state
        .process_next_midi_buffer(&mut buffer, 100, 64, true)
        .unwrap();
    assert!(!state.is_note_on(1, 30) && state.is_note_on(1, 31) && !state.is_note_on(1, 32));
    let got = events(&buffer);
    assert_eq!(
        got[1..4],
        [
            (vec![0x90, 31, 9], 100),
            (vec![0x91, 70, 127], 100),
            (vec![0x81, 70, 0], 100)
        ]
    );
    // Injected events are not fed back into the state.
    assert!(!state.is_note_on(2, 70));
}

#[test]
fn b3_without_injection_queued_ui_events_are_discarded() {
    let state = MidiKeyboardState::new();
    state.note_on(1, 60, 1.0).unwrap();
    let mut buffer = MidiBuffer::with_capacity(4, 12);
    state
        .process_next_midi_buffer(&mut buffer, 0, 64, false)
        .unwrap();
    state
        .process_next_midi_buffer(&mut buffer, 0, 64, true)
        .unwrap();
    assert!(buffer.is_empty() && state.is_note_on(1, 60));
}

#[test]
fn b3_a_full_buffer_keeps_the_rest_queued_in_order() {
    let state = MidiKeyboardState::new();
    for n in 0..3 {
        state.note_on(1, n, 1.0).unwrap();
    }
    let mut small = MidiBuffer::with_capacity(2, 6);
    let err = state
        .process_next_midi_buffer(&mut small, 0, 64, true)
        .unwrap_err();
    assert_eq!(
        (err, err.code().as_str()),
        (KeyboardError::BufferFull, "SYRP-MIDI-0003")
    );
    assert_eq!(small.len(), 2);
    let mut next = MidiBuffer::with_capacity(4, 12);
    state
        .process_next_midi_buffer(&mut next, 7, 64, true)
        .unwrap();
    assert_eq!(events(&next), [(vec![0x90, 2, 127], 7)]);
}

#[test]
fn b3_a_full_queue_refuses_new_ui_events_and_leaves_state_unchanged() {
    let state = MidiKeyboardState::new();
    for i in 0..QUEUE_CAPACITY {
        let (c, n) = (
            u8::try_from(i / 128 + 1).unwrap(),
            u8::try_from(i % 128).unwrap(),
        );
        state.note_on(c, n, 1.0).unwrap();
    }
    state.note_off(1, 0, 0.0).unwrap_err();
    let err = state.note_off(1, 0, 0.0).unwrap_err();
    assert_eq!(
        (err, err.code().as_str()),
        (KeyboardError::QueueFull, "SYRP-MIDI-0007")
    );
    assert!(state.is_note_on(1, 0));
}

#[test]
fn b3_audio_path_never_allocates() {
    let count = Arc::new(Count::default());
    let mut state = MidiKeyboardState::new();
    state.add_listener(count.clone());
    let mut buffer = MidiBuffer::with_capacity(64, 256);
    for n in 0..8 {
        buffer.try_add_event(&[0x93, n, 100], u32::from(n)).unwrap();
    }
    buffer.try_add_event(&[0xB3, 123, 0], 20).unwrap();
    state.note_on(1, 60, 0.5).unwrap();
    state.note_off(1, 60, 0.5).unwrap();
    let result = syrp_rt::no_alloc(|| {
        let r = state.process_next_midi_buffer(&mut buffer, 0, 512, true);
        state.process_next_midi_event(&buffer.iter().next().unwrap());
        (
            r,
            state.is_note_on(4, 0),
            state.is_note_on_for_channels(0xFFFF, 1),
        )
    });
    assert_eq!(result, (Ok(()), true, false));
    assert_eq!(
        (
            count.0.load(Ordering::Relaxed),
            count.1.load(Ordering::Relaxed)
        ),
        (10, 9)
    );
    assert_eq!(buffer.len(), 11);
}

#[test]
fn b3_ui_and_audio_threads_share_the_state_without_locks() {
    let count = Arc::new(Count::default());
    let mut state = MidiKeyboardState::new();
    state.add_listener(count.clone());
    let state = Arc::new(state);
    let ui = {
        let state = state.clone();
        std::thread::spawn(move || {
            for i in 0..2000_u32 {
                let note = u8::try_from(i % 128).unwrap();
                while state.note_on(1, note, 1.0) == Err(KeyboardError::QueueFull) {
                    std::thread::yield_now();
                }
                while state.note_off(1, note, 0.0) == Err(KeyboardError::QueueFull) {
                    std::thread::yield_now();
                }
            }
        })
    };
    let mut injected = 0;
    let mut buffer = MidiBuffer::with_capacity(256, 1024);
    while !ui.is_finished() || injected < 4000 {
        buffer.clear();
        // A full buffer keeps the rest queued for the next block.
        let r = state.process_next_midi_buffer(&mut buffer, 0, 64, true);
        assert!(matches!(r, Ok(()) | Err(KeyboardError::BufferFull)));
        injected += buffer.len();
    }
    ui.join().unwrap();
    assert_eq!(injected, 4000);
    assert_eq!(
        (
            count.0.load(Ordering::Relaxed),
            count.1.load(Ordering::Relaxed)
        ),
        (2000, 2000)
    );
    assert!((0..128).all(|n| !state.is_note_on(1, n)));
}
