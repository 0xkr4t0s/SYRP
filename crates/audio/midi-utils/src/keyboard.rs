// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`MidiKeyboardState`]: which keys are down on each of the 16 MIDI channels, shared
//! lock-free between a UI thread and the audio thread.
//!
//! The audio thread feeds incoming MIDI through
//! [`process_next_midi_buffer`](MidiKeyboardState::process_next_midi_buffer); a UI (an
//! on-screen keyboard) presses keys with [`note_on`](MidiKeyboardState::note_on) and
//! [`note_off`](MidiKeyboardState::note_off), whose events wait in a `syrp_rt` SPSC queue
//! until the next `process_next_midi_buffer` injects them into the buffer. Key state lives in
//! one atomic word per note, so any thread can read it while another changes it.
//!
//! ```
//! use syrp_midi::{MidiBuffer, MidiData};
//! use syrp_midi_utils::keyboard::MidiKeyboardState;
//! let state = MidiKeyboardState::new();
//! state.note_on(1, 60, 0.8)?; // UI thread
//! assert!(state.is_note_on(1, 60));
//! let mut buffer = MidiBuffer::with_capacity(16, 64); // audio thread
//! state.process_next_midi_buffer(&mut buffer, 0, 512, true)?;
//! assert_eq!(buffer.iter().next().map(|e| e.bytes().to_vec()), Some(vec![0x90, 60, 102]));
//! # Ok::<(), syrp_midi_utils::keyboard::KeyboardError>(())
//! ```

use std::cell::UnsafeCell;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use syrp_error::{Diagnostic, ErrorCode};
use syrp_midi::{MidiBuffer, MidiData, MidiError};
use syrp_rt::spsc::{self, Consumer, Producer};

use crate::message::velocity_byte;

/// How many UI note events can wait for the audio thread: enough for an all-notes-off of
/// every key on every channel.
pub const QUEUE_CAPACITY: usize = 16 * 128;

/// Errors from [`MidiKeyboardState`].
/// ```
/// use syrp_error::Diagnostic;
/// let state = syrp_midi_utils::keyboard::MidiKeyboardState::new();
/// assert_eq!(state.note_on(0, 60, 1.0).unwrap_err().code().as_str(), "SYRP-MIDI-0002");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyboardError {
    /// A channel outside 1 to 16, a note above 127 or a NaN velocity (`SYRP-MIDI-0002`).
    ValueOutOfRange,
    /// The queue of UI events waiting for the audio thread is full (`SYRP-MIDI-0007`). The
    /// state is unchanged.
    QueueFull,
    /// The buffer had no room for every waiting UI event (`SYRP-MIDI-0003`). The rest stay
    /// queued for the next call.
    BufferFull,
}

impl fmt::Display for KeyboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            KeyboardError::ValueOutOfRange => "MIDI value out of range",
            KeyboardError::QueueFull => "keyboard state event queue is full",
            KeyboardError::BufferFull => "MIDI buffer capacity is full",
        })
    }
}

impl std::error::Error for KeyboardError {}

impl Diagnostic for KeyboardError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            KeyboardError::ValueOutOfRange => "SYRP-MIDI-0002",
            KeyboardError::QueueFull => "SYRP-MIDI-0007",
            KeyboardError::BufferFull => "SYRP-MIDI-0003",
        })
    }
}

/// Receives key changes. Callbacks run synchronously on the thread that changed the state:
/// the UI thread for [`note_on`](MidiKeyboardState::note_on) and friends, the audio thread for
/// [`process_next_midi_buffer`](MidiKeyboardState::process_next_midi_buffer), so they must
/// not allocate, lock or block when the state is fed from audio.
/// ```
/// use syrp_midi_utils::keyboard::{MidiKeyboardState, MidiKeyboardStateListener};
/// struct Log;
/// impl MidiKeyboardStateListener for Log {
///     fn handle_note_on(&self, _: &MidiKeyboardState, channel: u8, note: u8, velocity: f32) {}
///     fn handle_note_off(&self, _: &MidiKeyboardState, channel: u8, note: u8, velocity: f32) {}
/// }
/// let mut state = MidiKeyboardState::new();
/// state.add_listener(std::sync::Arc::new(Log));
/// ```
#[doc(alias = "MidiKeyboardState::Listener")]
pub trait MidiKeyboardStateListener: Send + Sync {
    /// A key went down: channel 1 to 16, note 0 to 127, velocity 0.0 to 1.0.
    #[doc(alias = "handleNoteOn")]
    fn handle_note_on(&self, state: &MidiKeyboardState, channel: u8, note: u8, velocity: f32);
    /// A key that was down went up.
    #[doc(alias = "handleNoteOff")]
    fn handle_note_off(&self, state: &MidiKeyboardState, channel: u8, note: u8, velocity: f32);
}

/// A value that one thread at a time may use. Claiming it is one compare-exchange, so it
/// never blocks, allocates or calls the OS; a second thread finds it busy instead of waiting.
struct Claim<T> {
    busy: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: `value` is reached only inside `try_with`, between a successful Acquire
// compare-exchange of `busy` from false to true and the Release store of false, so at most
// one thread uses it at a time and each sees the previous user's writes. `T: Send` lets that
// exclusive use move between threads.
unsafe impl<T: Send> Sync for Claim<T> {}

impl<T> Claim<T> {
    fn new(value: T) -> Self {
        Claim {
            busy: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Runs `f` on the value, or returns `None` when another thread holds it.
    fn try_with<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        /// Releases the claim even if `f` panics.
        struct Release<'a>(&'a AtomicBool);
        impl Drop for Release<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        self.busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()?;
        let _release = Release(&self.busy);
        // SAFETY: the compare-exchange above made this thread the only holder until
        // `_release` drops, and `f` cannot keep the reference past this call.
        Some(f(unsafe { &mut *self.value.get() }))
    }

    /// Like `try_with`, but waits (spinning, then yielding) while another thread holds it.
    /// Never used on the audio thread.
    fn with<R>(&self, mut f: impl FnMut(&mut T) -> R) -> R {
        loop {
            if let Some(r) = self.try_with(&mut f) {
                return r;
            }
            std::thread::yield_now();
        }
    }
}

/// The audio side of the UI queue: the consumer plus one event popped but not yet injected.
struct Inbox {
    queue: Consumer<[u8; 3]>,
    carried: Option<[u8; 3]>,
}

/// The note on/off state of all 16 MIDI channels, in the role of JUCE's `MidiKeyboardState`.
///
/// `Send + Sync`: share it in an `Arc` between the UI and the audio thread. Each note's
/// channels are bits of one `AtomicU16` (bit 0 is channel 1), so reads never lock. Listeners
/// are added with `&mut self`, before the state is shared (Syrp's choice, so the audio thread
/// can call them without a lock).
/// ```
/// let state = syrp_midi_utils::keyboard::MidiKeyboardState::new();
/// state.note_on(2, 64, 1.0)?;
/// assert!(state.is_note_on_for_channels(0b10, 64));
/// # Ok::<(), syrp_midi_utils::keyboard::KeyboardError>(())
/// ```
pub struct MidiKeyboardState {
    notes: [AtomicU16; 128],
    listeners: Vec<Arc<dyn MidiKeyboardStateListener>>,
    outbox: Claim<Producer<[u8; 3]>>,
    inbox: Claim<Inbox>,
}

impl fmt::Debug for MidiKeyboardState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let down: Vec<u8> = (0..128)
            .filter(|n| self.notes[usize::from(*n)].load(Ordering::Relaxed) != 0)
            .collect();
        f.debug_struct("MidiKeyboardState")
            .field("notes_down", &down)
            .field("listeners", &self.listeners.len())
            .finish_non_exhaustive()
    }
}

impl Default for MidiKeyboardState {
    fn default() -> Self {
        Self::new()
    }
}

/// Checks a channel (1 to 16) and note (0 to 127) and returns the channel's bit.
fn channel_bit(channel: u8, note: u8) -> Result<u16, KeyboardError> {
    if (1..=16).contains(&channel) && note < 128 {
        Ok(1 << (channel - 1))
    } else {
        Err(KeyboardError::ValueOutOfRange)
    }
}

impl MidiKeyboardState {
    /// A state with every key up, no listeners and an empty queue of [`QUEUE_CAPACITY`]
    /// events. Allocates: message thread.
    /// ```
    /// assert!(!syrp_midi_utils::keyboard::MidiKeyboardState::new().is_note_on(1, 60));
    /// ```
    #[must_use]
    pub fn new() -> Self {
        let (producer, consumer) = spsc::channel(QUEUE_CAPACITY);
        MidiKeyboardState {
            notes: std::array::from_fn(|_| AtomicU16::new(0)),
            listeners: Vec::new(),
            outbox: Claim::new(producer),
            inbox: Claim::new(Inbox {
                queue: consumer,
                carried: None,
            }),
        }
    }

    /// Registers a listener. Takes `&mut self`: add listeners before sharing the state.
    /// ```
    /// # use syrp_midi_utils::keyboard::*;
    /// # struct L; impl MidiKeyboardStateListener for L {
    /// #   fn handle_note_on(&self, _: &MidiKeyboardState, _: u8, _: u8, _: f32) {}
    /// #   fn handle_note_off(&self, _: &MidiKeyboardState, _: u8, _: u8, _: f32) {} }
    /// let mut state = MidiKeyboardState::new();
    /// state.add_listener(std::sync::Arc::new(L));
    /// ```
    #[doc(alias = "addListener")]
    pub fn add_listener(&mut self, listener: Arc<dyn MidiKeyboardStateListener>) {
        self.listeners.push(listener);
    }

    /// Removes a listener added earlier (compared by pointer). Returns whether it was found.
    /// ```
    /// # use syrp_midi_utils::keyboard::*;
    /// # struct L; impl MidiKeyboardStateListener for L {
    /// #   fn handle_note_on(&self, _: &MidiKeyboardState, _: u8, _: u8, _: f32) {}
    /// #   fn handle_note_off(&self, _: &MidiKeyboardState, _: u8, _: u8, _: f32) {} }
    /// let mut state = MidiKeyboardState::new();
    /// let l: std::sync::Arc<dyn MidiKeyboardStateListener> = std::sync::Arc::new(L);
    /// state.add_listener(l.clone());
    /// assert!(state.remove_listener(&l));
    /// ```
    #[doc(alias = "removeListener")]
    pub fn remove_listener(&mut self, listener: &Arc<dyn MidiKeyboardStateListener>) -> bool {
        let before = self.listeners.len();
        self.listeners.retain(|l| !Arc::ptr_eq(l, listener));
        self.listeners.len() != before
    }

    /// True when `note` is down on `channel` (1 to 16); false for out-of-range arguments.
    /// RT-safe.
    /// ```
    /// assert!(!syrp_midi_utils::keyboard::MidiKeyboardState::new().is_note_on(17, 60));
    /// ```
    #[doc(alias = "isNoteOn")]
    #[must_use]
    pub fn is_note_on(&self, channel: u8, note: u8) -> bool {
        channel_bit(channel, note)
            .is_ok_and(|bit| self.notes[usize::from(note)].load(Ordering::Acquire) & bit != 0)
    }

    /// True when `note` is down on any channel whose bit is set in `channel_mask` (bit 0 is
    /// channel 1). RT-safe.
    /// ```
    /// let state = syrp_midi_utils::keyboard::MidiKeyboardState::new();
    /// state.note_on(16, 0, 0.5)?;
    /// assert!(state.is_note_on_for_channels(0x8000, 0));
    /// assert!(!state.is_note_on_for_channels(0x7FFF, 0));
    /// # Ok::<(), syrp_midi_utils::keyboard::KeyboardError>(())
    /// ```
    #[doc(alias = "isNoteOnForChannels")]
    #[must_use]
    pub fn is_note_on_for_channels(&self, channel_mask: u16, note: u8) -> bool {
        self.notes
            .get(usize::from(note))
            .is_some_and(|n| n.load(Ordering::Acquire) & channel_mask != 0)
    }

    /// Presses a key from the UI: queues a note on for the next
    /// [`process_next_midi_buffer`](Self::process_next_midi_buffer), marks the key down and
    /// calls the listeners on this thread. Velocity is 0.0 to 1.0, scaled as
    /// [`note_on_float`](crate::message::note_on_float) does, except that 0 is sent as 1
    /// because a note on with velocity 0 means note off (Syrp's choice). Not for the audio thread:
    /// it waits if another thread is pressing keys at the same moment.
    /// ```
    /// let state = syrp_midi_utils::keyboard::MidiKeyboardState::new();
    /// state.note_on(1, 60, 1.0)?;
    /// assert!(state.is_note_on(1, 60));
    /// # Ok::<(), syrp_midi_utils::keyboard::KeyboardError>(())
    /// ```
    #[doc(alias = "noteOn")]
    pub fn note_on(&self, channel: u8, note: u8, velocity: f32) -> Result<(), KeyboardError> {
        let bit = channel_bit(channel, note)?;
        // A note on with velocity 0 would mean note off, so 0 is sent as 1.
        let byte = velocity_byte(velocity)
            .map_err(|_| KeyboardError::ValueOutOfRange)?
            .max(1);
        self.queue([0x90 | (channel - 1), note, byte])?;
        self.press(bit, channel, note, f32::from(byte) / 127.0);
        Ok(())
    }

    /// Releases a key from the UI, as [`note_on`](Self::note_on) presses one. Does nothing
    /// when the key is not down on that channel.
    /// ```
    /// let state = syrp_midi_utils::keyboard::MidiKeyboardState::new();
    /// state.note_on(1, 60, 1.0)?;
    /// state.note_off(1, 60, 0.0)?;
    /// assert!(!state.is_note_on(1, 60));
    /// # Ok::<(), syrp_midi_utils::keyboard::KeyboardError>(())
    /// ```
    #[doc(alias = "noteOff")]
    pub fn note_off(&self, channel: u8, note: u8, velocity: f32) -> Result<(), KeyboardError> {
        let bit = channel_bit(channel, note)?;
        let byte = velocity_byte(velocity).map_err(|_| KeyboardError::ValueOutOfRange)?;
        if self.notes[usize::from(note)].load(Ordering::Acquire) & bit == 0 {
            return Ok(());
        }
        self.queue([0x80 | (channel - 1), note, byte])?;
        self.release(bit, channel, note, f32::from(byte) / 127.0);
        Ok(())
    }

    /// Releases, through [`note_off`](Self::note_off), every key down on `channel`, or on
    /// every channel when `channel` is `None`.
    /// ```
    /// let state = syrp_midi_utils::keyboard::MidiKeyboardState::new();
    /// state.note_on(3, 1, 1.0)?;
    /// state.note_on(4, 2, 1.0)?;
    /// state.all_notes_off(None)?;
    /// assert!(!state.is_note_on(3, 1) && !state.is_note_on(4, 2));
    /// # Ok::<(), syrp_midi_utils::keyboard::KeyboardError>(())
    /// ```
    #[doc(alias = "allNotesOff")]
    pub fn all_notes_off(&self, channel: Option<u8>) -> Result<(), KeyboardError> {
        let channels = match channel {
            Some(c) => {
                channel_bit(c, 0)?;
                c..=c
            }
            None => 1..=16,
        };
        for c in channels {
            for note in 0..128 {
                self.note_off(c, note, 0.0)?;
            }
        }
        Ok(())
    }

    /// Updates the state from one incoming message, calling the listeners on this thread:
    /// note ons press keys, note offs release keys that are down, and (Syrp's choice) All
    /// Notes Off and All Sound Off (controllers 123 and 120) release every key of their
    /// channel. Other messages are ignored. RT-safe when the listeners are.
    /// ```
    /// let state = syrp_midi_utils::keyboard::MidiKeyboardState::new();
    /// state.process_next_midi_event(&syrp_midi::MidiMessage::note_on(5, 70, 64).unwrap());
    /// assert!(state.is_note_on(5, 70));
    /// ```
    #[doc(alias = "processNextMidiEvent")]
    pub fn process_next_midi_event<M: MidiData + ?Sized>(&self, message: &M) {
        let Some(channel) = message.channel() else {
            return;
        };
        let bit = 1 << (channel - 1);
        if let (Some(note), Some(velocity)) = (message.note_number(), message.velocity()) {
            let velocity = f32::from(velocity) / 127.0;
            if message.is_note_on() {
                self.press(bit, channel, note, velocity);
            } else {
                self.release(bit, channel, note, velocity);
            }
        } else if matches!(message.controller_number(), Some(120 | 123)) {
            for note in 0..128 {
                self.release(bit, channel, note, 0.0);
            }
        }
    }

    /// Feeds the events of `buffer` at sample offsets `start_sample..start_sample +
    /// num_samples` through [`process_next_midi_event`](Self::process_next_midi_event). Then,
    /// when `inject_indirect_events` is true, adds the events queued by
    /// [`note_on`](Self::note_on) and [`note_off`](Self::note_off) at `start_sample`, in the
    /// order they were made (Syrp's choice of position); when it is false they are discarded.
    ///
    /// RT-safe when the listeners are: it uses `MidiBuffer::try_add_event` and never
    /// allocates. If the buffer fills, the events that did not fit stay queued for the next
    /// call and the result is [`KeyboardError::BufferFull`]. If another thread is inside this
    /// method on the same state, this call injects nothing.
    /// ```
    /// use syrp_midi::MidiBuffer;
    /// let state = syrp_midi_utils::keyboard::MidiKeyboardState::new();
    /// let mut buffer = MidiBuffer::with_capacity(4, 12);
    /// buffer.try_add_event(&[0x90, 40, 1], 10).unwrap();
    /// state.process_next_midi_buffer(&mut buffer, 0, 64, true)?;
    /// assert!(state.is_note_on(1, 40));
    /// # Ok::<(), syrp_midi_utils::keyboard::KeyboardError>(())
    /// ```
    #[doc(alias = "processNextMidiBuffer")]
    pub fn process_next_midi_buffer(
        &self,
        buffer: &mut MidiBuffer,
        start_sample: u32,
        num_samples: u32,
        inject_indirect_events: bool,
    ) -> Result<(), KeyboardError> {
        for event in buffer.range(start_sample..start_sample.saturating_add(num_samples)) {
            self.process_next_midi_event(&event);
        }
        let injected = self.inbox.try_with(|inbox| {
            while let Some(bytes) = inbox.carried.take().or_else(|| inbox.queue.try_pop()) {
                if !inject_indirect_events {
                    continue;
                }
                if let Err(e) = buffer.try_add_event(&bytes, start_sample) {
                    inbox.carried = Some(bytes);
                    return Err(match e {
                        MidiError::BufferFull => KeyboardError::BufferFull,
                        _ => KeyboardError::ValueOutOfRange,
                    });
                }
            }
            Ok(())
        });
        injected.unwrap_or(Ok(()))
    }

    /// Queues a UI event for the audio thread.
    fn queue(&self, bytes: [u8; 3]) -> Result<(), KeyboardError> {
        self.outbox
            .with(|producer| producer.try_push(bytes))
            .map_err(|_| KeyboardError::QueueFull)
    }

    /// Marks a key down and tells the listeners.
    fn press(&self, bit: u16, channel: u8, note: u8, velocity: f32) {
        self.notes[usize::from(note)].fetch_or(bit, Ordering::AcqRel);
        for l in &self.listeners {
            l.handle_note_on(self, channel, note, velocity);
        }
    }

    /// Marks a key up and tells the listeners, if it was down.
    fn release(&self, bit: u16, channel: u8, note: u8, velocity: f32) {
        if self.notes[usize::from(note)].fetch_and(!bit, Ordering::AcqRel) & bit != 0 {
            for l in &self.listeners {
                l.handle_note_off(self, channel, note, velocity);
            }
        }
    }
}
