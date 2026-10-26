// SPDX-License-Identifier: MIT OR Apache-2.0
//! Events a peer delivers to its [`EventSink`]: keyboard, focus, IME and window changes.
//! Areas are logical units relative to the peer's top-left.

use std::ops::Range;

use keyboard_types::KeyboardEvent;
use syrp_geom::Rect;
use syrp_loop_api::MessageThread;

use crate::WindowState;

/// Receives a peer's events on the message thread, in the order the platform produced them.
/// ```
/// use syrp_peer_api::{Event, EventSink};
/// let _sink: EventSink = Box::new(|_mt, ev: &Event| println!("{ev:?}"));
/// ```
pub type EventSink = Box<dyn FnMut(&MessageThread<'_>, &Event)>;

/// One event from a peer.
/// ```
/// use syrp_peer_api::{Event, ImeEvent};
/// let ev = Event::Ime(ImeEvent::Commit("é".into()));
/// assert!(matches!(ev, Event::Ime(ImeEvent::Commit(ref s)) if s == "é"));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// A key press or release (`keyboard-types` vocabulary).
    Key(KeyboardEvent),
    /// Input-method composition.
    Ime(ImeEvent),
    /// The peer gained keyboard focus.
    FocusGained,
    /// The peer lost keyboard focus.
    FocusLost,
    /// The platform moved or resized the peer; the new bounds in logical units.
    BoundsChanged(Rect<f64>),
    /// The peer's scale factor (physical pixels per logical unit) changed.
    ScaleChanged(f64),
    /// The peer was minimised, maximised, made fullscreen or restored.
    StateChanged(WindowState),
    /// The platform discarded this area's pixels; it must be repainted.
    Expose(Rect<f64>),
    /// The user asked to close the window (close button, Alt+F4, ...). Nothing closes by itself.
    CloseRequested,
}

/// Input-method composition (JUCE's TextInputTarget role). While composing, `Key` events
/// carry `is_composing` and should not edit text; the commit carries the final text.
/// ```
/// use syrp_peer_api::ImeEvent;
/// let ev = ImeEvent::Preedit("かな".into(), Some(3..6));
/// assert_eq!(ev, ImeEvent::Preedit("かな".into(), Some(3..6)));
/// ```
#[doc(alias = "TextInputTarget")]
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImeEvent {
    /// Composition began.
    Start,
    /// The uncommitted text so far, and the caret or selection inside it as a byte range
    /// (`None` hides the caret). An empty string clears the shown composition.
    Preedit(String, Option<Range<usize>>),
    /// Text to insert at the caret; ends the composition if one is open.
    Commit(String),
    /// Composition ended (after a commit, or cancelled with no text).
    End,
}
