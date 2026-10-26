// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pointer (mouse, touch, pen) and wheel event payloads. Positions are logical units
//! relative to the peer's top-left.

use keyboard_types::Modifiers;
use syrp_geom::Point;

/// The kind of device behind a pointer.
/// ```
/// assert_ne!(syrp_peer_api::PointerKind::Pen, syrp_peer_api::PointerKind::Touch);
/// ```
#[doc(alias = "MouseInputSource::InputSourceType")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointerKind {
    /// A mouse or trackpad.
    Mouse,
    /// A finger on a touch screen.
    Touch,
    /// A stylus.
    Pen,
}

/// Where a pointer event is in its sequence.
/// ```
/// assert_ne!(syrp_peer_api::PointerPhase::Down, syrp_peer_api::PointerPhase::Up);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointerPhase {
    /// The pointer entered the peer (hover).
    Enter,
    /// A button was pressed or contact began.
    Down,
    /// The pointer moved (dragging if any button is held).
    Move,
    /// A button was released or contact ended.
    Up,
    /// The pointer left the peer.
    Exit,
    /// The platform cancelled the sequence (a touch taken over by a gesture, say).
    Cancel,
}

/// A pointer button; touch contact and pen tip count as [`Primary`](Self::Primary).
/// ```
/// let b = syrp_peer_api::PointerButtons::NONE.with(syrp_peer_api::PointerButton::Middle);
/// assert!(b.contains(syrp_peer_api::PointerButton::Middle));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointerButton {
    /// Left mouse button, touch contact or pen tip.
    Primary,
    /// Right mouse button or pen barrel button.
    Secondary,
    /// Middle mouse button.
    Middle,
    /// The "back" side button.
    Back,
    /// The "forward" side button.
    Forward,
}

impl PointerButton {
    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The set of buttons held down.
/// ```
/// use syrp_peer_api::{PointerButton, PointerButtons};
/// let held = PointerButtons::NONE.with(PointerButton::Primary);
/// assert!(held.contains(PointerButton::Primary) && !held.is_empty());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PointerButtons(u8);

impl PointerButtons {
    /// No buttons held.
    /// ```
    /// assert!(syrp_peer_api::PointerButtons::NONE.is_empty());
    /// ```
    pub const NONE: Self = PointerButtons(0);

    /// This set plus `button`.
    /// ```
    /// use syrp_peer_api::{PointerButton as B, PointerButtons};
    /// assert!(PointerButtons::NONE.with(B::Back).contains(B::Back));
    /// ```
    #[must_use]
    pub fn with(self, button: PointerButton) -> Self {
        PointerButtons(self.0 | button.bit())
    }

    /// Whether `button` is held.
    /// ```
    /// use syrp_peer_api::{PointerButton as B, PointerButtons};
    /// assert!(!PointerButtons::NONE.contains(B::Primary));
    /// ```
    pub fn contains(self, button: PointerButton) -> bool {
        self.0 & button.bit() != 0
    }

    /// Whether no button is held.
    /// ```
    /// assert!(syrp_peer_api::PointerButtons::default().is_empty());
    /// ```
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Identifies one pointer: its device kind plus an id that stays the same from `Down` to `Up`
/// (each finger of a multi-touch gesture has its own id; the mouse is usually 0).
/// ```
/// use syrp_peer_api::{PointerKind, PointerSource};
/// let finger = PointerSource::new(PointerKind::Touch, 2);
/// assert_eq!((finger.kind(), finger.id()), (PointerKind::Touch, 2));
/// ```
#[doc(alias = "MouseInputSource")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerSource(PointerKind, u32);

impl PointerSource {
    /// The mouse, id 0.
    /// ```
    /// assert_eq!(syrp_peer_api::PointerSource::MOUSE.id(), 0);
    /// ```
    pub const MOUSE: Self = PointerSource(PointerKind::Mouse, 0);

    /// A source of `kind` with the platform's pointer id.
    /// ```
    /// let _ = syrp_peer_api::PointerSource::new(syrp_peer_api::PointerKind::Pen, 1);
    /// ```
    pub const fn new(kind: PointerKind, id: u32) -> Self {
        PointerSource(kind, id)
    }

    /// The device kind.
    /// ```
    /// use syrp_peer_api::{PointerKind, PointerSource};
    /// assert_eq!(PointerSource::MOUSE.kind(), PointerKind::Mouse);
    /// ```
    pub const fn kind(self) -> PointerKind {
        self.0
    }

    /// The pointer id.
    /// ```
    /// assert_eq!(syrp_peer_api::PointerSource::MOUSE.id(), 0);
    /// ```
    pub const fn id(self) -> u32 {
        self.1
    }
}

/// Clamps to `[lo, hi]`; NaN becomes 0.
fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(lo, hi) }
}

/// A mouse, touch or pen event. Pressure and tilt are `None` when the device does not
/// report them (a mouse); a platform that reports them sets them with the `with_` builders.
/// ```
/// use syrp_geom::Point;
/// use syrp_peer_api::{PointerButton, PointerEvent, PointerKind, PointerPhase, PointerSource};
/// let ev = PointerEvent::new(PointerPhase::Down, PointerSource::new(PointerKind::Pen, 1), Point::new(4.0, 5.0))
///     .with_button(PointerButton::Primary)
///     .with_pressure(0.5)
///     .with_tilt(0.25, -0.5);
/// assert_eq!((ev.pressure(), ev.tilt()), (Some(0.5), Some((0.25, -0.5))));
/// assert!(ev.buttons().contains(PointerButton::Primary));
/// ```
#[doc(alias = "MouseEvent")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    phase: PointerPhase,
    source: PointerSource,
    position: Point<f64>,
    button: Option<PointerButton>,
    buttons: PointerButtons,
    modifiers: Modifiers,
    pressure: Option<f32>,
    tilt: Option<(f32, f32)>,
}

impl PointerEvent {
    /// An event with no button, no buttons held, no modifiers, pressure or tilt.
    /// ```
    /// use syrp_geom::Point;
    /// use syrp_peer_api::{PointerEvent, PointerPhase, PointerSource};
    /// let ev = PointerEvent::new(PointerPhase::Move, PointerSource::MOUSE, Point::new(1.0, 2.0));
    /// assert_eq!(ev.position(), Point::new(1.0, 2.0));
    /// ```
    pub fn new(phase: PointerPhase, source: PointerSource, position: Point<f64>) -> Self {
        PointerEvent {
            phase,
            source,
            position,
            button: None,
            buttons: PointerButtons::NONE,
            modifiers: Modifiers::empty(),
            pressure: None,
            tilt: None,
        }
    }

    /// Sets the button that changed (for `Down` and `Up`) and adds it to the held set on
    /// `Down`. On `Up` the held set is left to the caller ([`with_buttons`](Self::with_buttons)).
    /// ```
    /// use syrp_geom::Point;
    /// use syrp_peer_api::{PointerButton as B, PointerEvent, PointerPhase as P, PointerSource};
    /// let up = PointerEvent::new(P::Up, PointerSource::MOUSE, Point::new(0.0, 0.0)).with_button(B::Secondary);
    /// assert_eq!(up.button(), Some(B::Secondary));
    /// assert!(up.buttons().is_empty());
    /// ```
    #[must_use]
    pub fn with_button(mut self, button: PointerButton) -> Self {
        self.button = Some(button);
        if self.phase == PointerPhase::Down {
            self.buttons = self.buttons.with(button);
        }
        self
    }

    /// Sets the buttons held after this event.
    /// ```
    /// use syrp_geom::Point;
    /// use syrp_peer_api::{PointerButton as B, PointerButtons, PointerEvent, PointerPhase as P, PointerSource};
    /// let held = PointerButtons::NONE.with(B::Primary);
    /// let drag = PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0)).with_buttons(held);
    /// assert_eq!(drag.buttons(), held);
    /// ```
    #[must_use]
    pub fn with_buttons(mut self, buttons: PointerButtons) -> Self {
        self.buttons = buttons;
        self
    }

    /// Sets the keyboard modifiers held.
    /// ```
    /// use syrp_geom::Point;
    /// use syrp_peer_api::{Modifiers, PointerEvent, PointerPhase as P, PointerSource};
    /// let ev = PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0)).with_modifiers(Modifiers::SHIFT);
    /// assert!(ev.modifiers().shift());
    /// ```
    #[must_use]
    pub fn with_modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    /// Sets the pressure, clamped to `[0, 1]` (NaN becomes 0).
    /// ```
    /// use syrp_geom::Point;
    /// use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// let ev = PointerEvent::new(P::Down, PointerSource::MOUSE, Point::new(0.0, 0.0)).with_pressure(3.0);
    /// assert_eq!(ev.pressure(), Some(1.0));
    /// ```
    #[must_use]
    pub fn with_pressure(mut self, pressure: f32) -> Self {
        self.pressure = Some(clamp(pressure, 0.0, 1.0));
        self
    }

    /// Sets the pen tilt along x and y, each a fraction of the maximum tilt in `[-1, 1]`
    /// (positive leans towards +x / +y), clamped (NaN becomes 0).
    /// ```
    /// use syrp_geom::Point;
    /// use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// let ev = PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0)).with_tilt(-2.0, f32::NAN);
    /// assert_eq!(ev.tilt(), Some((-1.0, 0.0)));
    /// ```
    #[must_use]
    pub fn with_tilt(mut self, x: f32, y: f32) -> Self {
        self.tilt = Some((clamp(x, -1.0, 1.0), clamp(y, -1.0, 1.0)));
        self
    }

    /// Where in the sequence this event is.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// assert_eq!(PointerEvent::new(P::Exit, PointerSource::MOUSE, Point::new(0.0, 0.0)).phase(), P::Exit);
    /// ```
    pub fn phase(&self) -> PointerPhase {
        self.phase
    }

    /// The pointer that produced the event.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// let ev = PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0));
    /// assert_eq!(ev.source(), PointerSource::MOUSE);
    /// ```
    pub fn source(&self) -> PointerSource {
        self.source
    }

    /// Position in logical units relative to the peer's top-left.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// let ev = PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(3.0, 4.0));
    /// assert_eq!(ev.position().x, 3.0);
    /// ```
    pub fn position(&self) -> Point<f64> {
        self.position
    }

    /// The button that changed, for `Down` and `Up`.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// assert_eq!(PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0)).button(), None);
    /// ```
    pub fn button(&self) -> Option<PointerButton> {
        self.button
    }

    /// The buttons held after this event.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// assert!(PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0)).buttons().is_empty());
    /// ```
    pub fn buttons(&self) -> PointerButtons {
        self.buttons
    }

    /// The keyboard modifiers held.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// assert!(PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0)).modifiers().is_empty());
    /// ```
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Pressure in `[0, 1]`, if the device reports it.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// assert_eq!(PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0)).pressure(), None);
    /// ```
    pub fn pressure(&self) -> Option<f32> {
        self.pressure
    }

    /// Tilt along x and y in `[-1, 1]`, if the device reports it.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{PointerEvent, PointerPhase as P, PointerSource};
    /// assert_eq!(PointerEvent::new(P::Move, PointerSource::MOUSE, Point::new(0.0, 0.0)).tilt(), None);
    /// ```
    pub fn tilt(&self) -> Option<(f32, f32)> {
        self.tilt
    }
}

/// What a wheel delta counts.
/// ```
/// assert_ne!(syrp_peer_api::WheelUnit::Lines, syrp_peer_api::WheelUnit::Pixels);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WheelUnit {
    /// Notches of a stepped wheel (lines of text).
    Lines,
    /// Logical units, from a smooth trackpad or high-resolution wheel.
    Pixels,
}

/// A scroll. Positive deltas move the content right and down, in the platform's natural
/// direction (the platform has already applied any "reversed" setting).
/// ```
/// use syrp_geom::Point;
/// use syrp_peer_api::{WheelEvent, WheelUnit};
/// let ev = WheelEvent::new(Point::new(5.0, 5.0), 0.0, -3.0, WheelUnit::Lines);
/// assert_eq!((ev.delta(), ev.unit(), ev.is_inertial()), ((0.0, -3.0), WheelUnit::Lines, false));
/// ```
#[doc(alias = "MouseWheelDetails")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelEvent {
    position: Point<f64>,
    delta: (f64, f64),
    unit: WheelUnit,
    modifiers: Modifiers,
    inertial: bool,
}

impl WheelEvent {
    /// A scroll of `(dx, dy)` at `position`, with no modifiers, not inertial.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{WheelEvent, WheelUnit};
    /// let _ = WheelEvent::new(Point::new(0.0, 0.0), 1.0, 0.0, WheelUnit::Pixels);
    /// ```
    pub fn new(position: Point<f64>, dx: f64, dy: f64, unit: WheelUnit) -> Self {
        WheelEvent {
            position,
            delta: (dx, dy),
            unit,
            modifiers: Modifiers::empty(),
            inertial: false,
        }
    }

    /// Sets the keyboard modifiers held.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{Modifiers, WheelEvent, WheelUnit};
    /// let ev = WheelEvent::new(Point::new(0.0, 0.0), 0.0, 1.0, WheelUnit::Lines).with_modifiers(Modifiers::CONTROL);
    /// assert!(ev.modifiers().ctrl());
    /// ```
    #[must_use]
    pub fn with_modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    /// Marks the event as momentum generated after the fingers lifted.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{WheelEvent, WheelUnit};
    /// assert!(WheelEvent::new(Point::new(0.0, 0.0), 0.0, 1.0, WheelUnit::Pixels).with_inertial(true).is_inertial());
    /// ```
    #[must_use]
    pub fn with_inertial(mut self, inertial: bool) -> Self {
        self.inertial = inertial;
        self
    }

    /// Position in logical units relative to the peer.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{WheelEvent, WheelUnit};
    /// assert_eq!(WheelEvent::new(Point::new(2.0, 0.0), 0.0, 1.0, WheelUnit::Lines).position().x, 2.0);
    /// ```
    pub fn position(&self) -> Point<f64> {
        self.position
    }

    /// The `(dx, dy)` scroll amount in [`unit`](Self::unit)s.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{WheelEvent, WheelUnit};
    /// assert_eq!(WheelEvent::new(Point::new(0.0, 0.0), 1.0, 2.0, WheelUnit::Lines).delta(), (1.0, 2.0));
    /// ```
    pub fn delta(&self) -> (f64, f64) {
        self.delta
    }

    /// What the delta counts.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{WheelEvent, WheelUnit};
    /// assert_eq!(WheelEvent::new(Point::new(0.0, 0.0), 0.0, 1.0, WheelUnit::Pixels).unit(), WheelUnit::Pixels);
    /// ```
    pub fn unit(&self) -> WheelUnit {
        self.unit
    }

    /// The keyboard modifiers held.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{WheelEvent, WheelUnit};
    /// assert!(WheelEvent::new(Point::new(0.0, 0.0), 0.0, 1.0, WheelUnit::Lines).modifiers().is_empty());
    /// ```
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Whether this is momentum scrolling.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{WheelEvent, WheelUnit};
    /// assert!(!WheelEvent::new(Point::new(0.0, 0.0), 0.0, 1.0, WheelUnit::Lines).is_inertial());
    /// ```
    pub fn is_inertial(&self) -> bool {
        self.inertial
    }
}
