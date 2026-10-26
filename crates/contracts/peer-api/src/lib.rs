// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp native window contract (master plan §5.6): JUCE's ComponentPeer and Desktop
//! roles, implemented by the `syrp-peer-*` platform crates. Holds no platform code.
//!
//! - [`Desktop`] creates [`Peer`]s from [`PeerOptions`]: top-level windows or child windows
//!   embedded in a host-provided [`RawWindowHandle`] parent (the plugin editor case).
//! - A [`Peer`] owns one native window: bounds, visibility, [`WindowState`], title, scale
//!   factor, repaint requests, focus and the IME caret rectangle.
//! - Each peer delivers [`Event`]s (keyboard, focus, IME, window changes; pointer, wheel and
//!   drag-and-drop arrive in a later increment) to the [`EventSink`] it was created with.
//! - [`headless`] is a platform-free implementation for tests.
//!
//! Units: bounds and positions are `f64` logical units, y down; a top-level peer's bounds are
//! in desktop coordinates, a child's relative to its parent. The scale factor is physical
//! pixels per logical unit, the same model as [`RenderTarget`].
//!
//! ```
//! use syrp_geom::Rect;
//! use syrp_loop_api::ManualLoop;
//! use syrp_peer_api::{headless::HeadlessDesktop, Desktop, PeerOptions};
//! let mut desktop = HeadlessDesktop::new(2.0);
//! ManualLoop::new().with_token(|mt| {
//!     let opts = PeerOptions::top_level(Rect::new(0.0, 0.0, 400.0, 300.0)).with_title("Synth");
//!     let peer = desktop.create_peer(mt, opts, Box::new(|_mt, _ev| {})).unwrap();
//!     assert_eq!(peer.render_target().unwrap().width(), 800);
//! });
//! ```

mod event;
pub mod headless;

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};
use syrp_geom::Rect;
use syrp_gfx_api::{RenderError, RenderTarget};
use syrp_loop_api::MessageThread;

pub use event::{Event, EventSink, ImeEvent};
pub use keyboard_types::{self, KeyboardEvent, Modifiers};
pub use raw_window_handle::{self, RawWindowHandle};
pub use syrp_loop_api::PeerId;

/// Whether a window is restored, minimised, maximised or fullscreen.
/// ```
/// assert_eq!(syrp_peer_api::WindowState::default(), syrp_peer_api::WindowState::Normal);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WindowState {
    /// Restored to its own bounds.
    #[default]
    Normal,
    /// Minimised (iconified) to the task bar or dock.
    Minimised,
    /// Filling the display's work area.
    Maximised,
    /// Filling the whole display, without decorations.
    Fullscreen,
}

/// How to create a peer: top-level or child, initial bounds, title, visibility, decorations.
/// ```
/// use syrp_geom::Rect;
/// use syrp_peer_api::PeerOptions;
/// let o = PeerOptions::top_level(Rect::new(10.0, 10.0, 300.0, 200.0)).with_title("Gain").with_visible(false);
/// assert_eq!((o.title(), o.is_visible(), o.is_decorated(), o.parent()), ("Gain", false, true, None));
/// ```
#[doc(alias = "ComponentPeer::StyleFlags")]
#[derive(Clone, Debug, PartialEq)]
pub struct PeerOptions {
    bounds: Rect<f64>,
    parent: Option<RawWindowHandle>,
    title: String,
    visible: bool,
    decorated: bool,
}

impl PeerOptions {
    /// A visible, decorated top-level window at `bounds` (desktop logical units).
    /// ```
    /// let o = syrp_peer_api::PeerOptions::top_level(syrp_geom::Rect::new(0.0, 0.0, 1.0, 1.0));
    /// assert!(o.parent().is_none());
    /// ```
    pub fn top_level(bounds: Rect<f64>) -> Self {
        PeerOptions {
            bounds,
            parent: None,
            title: String::new(),
            visible: true,
            decorated: true,
        }
    }

    /// A visible, undecorated child window inside the host's `parent`, at `bounds` relative
    /// to it. A platform [`Desktop`] passes the handle to the OS, which trusts it, so a forged
    /// handle is undefined behaviour; this constructor is therefore `unsafe`.
    ///
    /// # Safety
    /// Until every peer created from these options (and every clone of them) is dropped:
    /// - `parent` must identify a window that exists and is not destroyed, of the windowing
    ///   system that the [`Desktop`] receiving the options drives (an `AppKit` handle for the
    ///   macOS desktop, `Win32` for Windows, `Xlib`/`Xcb`/`Wayland` for that Linux backend);
    /// - every pointer in the handle (an `NSView`, a `wl_surface`, an `HINSTANCE`, ...) must be
    ///   non-null, properly aligned, point to a live object of the type the variant names, and
    ///   stay valid; integer ids (an `HWND`, an X11 window) must name that live window;
    /// - the handle must be usable from the thread that runs the message loop, as the platform
    ///   requires (`AppKit` views on the main thread).
    ///
    /// The headless desktop never dereferences the handle, but callers may not rely on that.
    /// ```
    /// use syrp_peer_api::raw_window_handle::{RawWindowHandle, WebWindowHandle};
    /// let parent = RawWindowHandle::Web(WebWindowHandle::new(1));
    /// // SAFETY: these options are only inspected here, never passed to a Desktop.
    /// let o = unsafe { syrp_peer_api::PeerOptions::child(parent, syrp_geom::Rect::new(0.0, 0.0, 1.0, 1.0)) };
    /// assert_eq!((o.parent(), o.is_decorated()), (Some(parent), false));
    /// ```
    #[doc(alias = "addToDesktop")]
    pub unsafe fn child(parent: RawWindowHandle, bounds: Rect<f64>) -> Self {
        PeerOptions {
            parent: Some(parent),
            decorated: false,
            ..Self::top_level(bounds)
        }
    }

    /// Sets the title (shown by top-level windows; kept for accessibility on children).
    /// ```
    /// # use syrp_geom::Rect;
    /// assert_eq!(syrp_peer_api::PeerOptions::top_level(Rect::default()).with_title("A").title(), "A");
    /// ```
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets whether the window is shown on creation.
    /// ```
    /// # use syrp_geom::Rect;
    /// assert!(!syrp_peer_api::PeerOptions::top_level(Rect::default()).with_visible(false).is_visible());
    /// ```
    #[must_use]
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Sets whether a top-level window gets a native title bar and border.
    /// ```
    /// # use syrp_geom::Rect;
    /// assert!(!syrp_peer_api::PeerOptions::top_level(Rect::default()).with_decorations(false).is_decorated());
    /// ```
    #[must_use]
    pub fn with_decorations(mut self, decorated: bool) -> Self {
        self.decorated = decorated;
        self
    }

    /// Initial bounds in logical units.
    /// ```
    /// # use syrp_geom::Rect;
    /// let r = Rect::new(1.0, 2.0, 3.0, 4.0);
    /// assert_eq!(syrp_peer_api::PeerOptions::top_level(r).bounds(), r);
    /// ```
    pub fn bounds(&self) -> Rect<f64> {
        self.bounds
    }

    /// The host parent for a child window, `None` for a top-level one.
    /// ```
    /// # use syrp_geom::Rect;
    /// assert!(syrp_peer_api::PeerOptions::top_level(Rect::default()).parent().is_none());
    /// ```
    pub fn parent(&self) -> Option<RawWindowHandle> {
        self.parent
    }

    /// The title.
    /// ```
    /// # use syrp_geom::Rect;
    /// assert_eq!(syrp_peer_api::PeerOptions::top_level(Rect::default()).title(), "");
    /// ```
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Whether the window is shown on creation.
    /// ```
    /// # use syrp_geom::Rect;
    /// assert!(syrp_peer_api::PeerOptions::top_level(Rect::default()).is_visible());
    /// ```
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Whether the window has native decorations.
    /// ```
    /// # use syrp_geom::Rect;
    /// assert!(syrp_peer_api::PeerOptions::top_level(Rect::default()).is_decorated());
    /// ```
    pub fn is_decorated(&self) -> bool {
        self.decorated
    }
}

/// The platform's window system (JUCE's Desktop role). Platform crates implement it; tests
/// use [`headless::HeadlessDesktop`].
/// ```
/// use syrp_peer_api::{Desktop, EventSink, Peer, PeerError, PeerOptions};
/// use syrp_loop_api::MessageThread;
/// fn open(d: &mut dyn Desktop, mt: &MessageThread<'_>, o: PeerOptions, s: EventSink) -> Result<Box<dyn Peer>, PeerError> {
///     d.create_peer(mt, o, s)
/// }
/// ```
pub trait Desktop {
    /// Creates a native window that delivers its events to `events`. Dropping the returned
    /// peer destroys the window.
    ///
    /// # Errors
    /// [`PeerError::InvalidParent`] if a child's parent handle is of another platform or no
    /// longer valid, [`PeerError::Unsupported`] if the platform cannot create this kind of
    /// window, [`PeerError::Backend`] if the window system refuses.
    /// ```
    /// # use syrp_geom::Rect; use syrp_loop_api::ManualLoop;
    /// use syrp_peer_api::{headless::HeadlessDesktop, Desktop, PeerOptions};
    /// let mut d = HeadlessDesktop::new(1.0);
    /// ManualLoop::new().with_token(|mt| {
    ///     let p = d.create_peer(mt, PeerOptions::top_level(Rect::default()), Box::new(|_, _| {}));
    ///     assert!(p.is_ok());
    /// });
    /// ```
    fn create_peer(
        &mut self,
        mt: &MessageThread<'_>,
        options: PeerOptions,
        events: EventSink,
    ) -> Result<Box<dyn Peer>, PeerError>;
}

/// One native window (JUCE's ComponentPeer). Setters take the [`MessageThread`] token.
///
/// Request and confirm: a setter records its value at once and asks the platform for it.
/// If the platform applies something else (a size constraint, a display edge, a refused
/// state) it reports the outcome as [`Event::BoundsChanged`] or [`Event::StateChanged`],
/// and it reports changes the user makes (dragging, the maximise button) the same way. A
/// platform never echoes a request it applied unchanged. So a getter returns whichever came
/// last: the latest setter call or the latest platform event.
/// ```
/// # use syrp_geom::Rect; use syrp_loop_api::ManualLoop;
/// use syrp_peer_api::{headless::HeadlessDesktop, Desktop, PeerOptions, WindowState};
/// let mut d = HeadlessDesktop::new(1.0);
/// ManualLoop::new().with_token(|mt| {
///     let mut p = d.create_peer(mt, PeerOptions::top_level(Rect::default()), Box::new(|_, _| {})).unwrap();
///     p.set_state(mt, WindowState::Maximised).unwrap();
///     assert_eq!(p.state(), WindowState::Maximised);
/// });
/// ```
#[doc(alias = "ComponentPeer")]
pub trait Peer {
    /// This peer's id, used for VBlank callbacks (`MessageLoop::on_vblank`).
    /// ```
    /// fn f(p: &dyn syrp_peer_api::Peer) { let _ = p.id(); }
    /// ```
    fn id(&self) -> PeerId;
    /// Whether this is a child window embedded in a host parent.
    /// ```
    /// fn f(p: &dyn syrp_peer_api::Peer) { let _ = p.is_child(); }
    /// ```
    fn is_child(&self) -> bool;
    /// Bounds in logical units (desktop coordinates, or parent-relative for a child): the last
    /// [`set_bounds`](Self::set_bounds) or [`Event::BoundsChanged`], whichever came later.
    /// ```
    /// fn f(p: &dyn syrp_peer_api::Peer) { let _ = p.bounds().width(); }
    /// ```
    fn bounds(&self) -> Rect<f64>;
    /// Requests new bounds; [`bounds`](Self::bounds) returns them at once. The platform sends
    /// [`Event::BoundsChanged`] only if it applies different bounds.
    /// ```
    /// fn f(p: &mut dyn syrp_peer_api::Peer, mt: &syrp_loop_api::MessageThread<'_>) { let _ = p.set_bounds(mt, syrp_geom::Rect::new(0.0, 0.0, 640.0, 480.0)); }
    /// ```
    fn set_bounds(&mut self, mt: &MessageThread<'_>, bounds: Rect<f64>);
    /// Whether the window is shown.
    /// ```
    /// fn f(p: &dyn syrp_peer_api::Peer) { let _ = p.is_visible(); }
    /// ```
    fn is_visible(&self) -> bool;
    /// Shows or hides the window.
    /// ```
    /// fn f(p: &mut dyn syrp_peer_api::Peer, mt: &syrp_loop_api::MessageThread<'_>) { let _ = p.set_visible(mt, true); }
    /// ```
    fn set_visible(&mut self, mt: &MessageThread<'_>, visible: bool);
    /// The window state: the last accepted [`set_state`](Self::set_state) or
    /// [`Event::StateChanged`], whichever came later.
    /// ```
    /// fn f(p: &dyn syrp_peer_api::Peer) { let _ = p.state(); }
    /// ```
    fn state(&self) -> WindowState;
    /// Requests a window state; once accepted, [`state`](Self::state) returns it at once. The
    /// platform sends [`Event::StateChanged`] only if the window ends up in another state.
    ///
    /// # Errors
    /// [`PeerError::Unsupported`] for any state but [`WindowState::Normal`] on a child window,
    /// or a state the platform lacks (fullscreen on some embedded targets).
    /// ```
    /// fn f(p: &mut dyn syrp_peer_api::Peer, mt: &syrp_loop_api::MessageThread<'_>) { let _ = p.set_state(mt, syrp_peer_api::WindowState::Fullscreen); }
    /// ```
    fn set_state(&mut self, mt: &MessageThread<'_>, state: WindowState) -> Result<(), PeerError>;
    /// The window title.
    /// ```
    /// fn f(p: &dyn syrp_peer_api::Peer) { let _ = p.title(); }
    /// ```
    fn title(&self) -> &str;
    /// Sets the window title.
    /// ```
    /// fn f(p: &mut dyn syrp_peer_api::Peer, mt: &syrp_loop_api::MessageThread<'_>) { let _ = p.set_title(mt, "Synth"); }
    /// ```
    fn set_title(&mut self, mt: &MessageThread<'_>, title: &str);
    /// Physical pixels per logical unit on the display the peer is on.
    /// ```
    /// fn f(p: &dyn syrp_peer_api::Peer) { let _ = p.scale_factor(); }
    /// ```
    fn scale_factor(&self) -> f64;
    /// Asks for `area` (logical, peer-relative) to be repainted on the next frame.
    /// ```
    /// fn f(p: &mut dyn syrp_peer_api::Peer, mt: &syrp_loop_api::MessageThread<'_>) { let _ = p.repaint(mt, syrp_geom::Rect::new(0.0, 0.0, 8.0, 8.0)); }
    /// ```
    fn repaint(&mut self, mt: &MessageThread<'_>, area: Rect<f64>);
    /// Asks the platform to give this peer keyboard focus; [`Event::FocusGained`] confirms.
    #[doc(alias = "grabFocus")]
    /// ```
    /// fn f(p: &mut dyn syrp_peer_api::Peer, mt: &syrp_loop_api::MessageThread<'_>) { let _ = p.grab_focus(mt); }
    /// ```
    fn grab_focus(&mut self, mt: &MessageThread<'_>);
    /// Reports where the text caret is (logical, peer-relative) so the IME can place its
    /// candidate window, and turns IME input on; `None` turns it off (no text field focused).
    #[doc(alias = "textInputRequired")]
    /// ```
    /// fn f(p: &mut dyn syrp_peer_api::Peer, mt: &syrp_loop_api::MessageThread<'_>) { let _ = p.set_ime_caret(mt, Some(syrp_geom::Rect::new(4.0, 2.0, 1.0, 12.0))); }
    /// ```
    fn set_ime_caret(&mut self, mt: &MessageThread<'_>, caret: Option<Rect<f64>>);

    /// A surface [`RenderTarget`] (no CPU buffer) matching the peer's size and scale factor:
    /// each side is the logical size times the scale, rounded up as `RenderTarget::from_logical`.
    ///
    /// # Errors
    /// [`RenderError::InvalidTarget`] if the scale factor or size is unusable.
    /// ```
    /// fn f(p: &dyn syrp_peer_api::Peer) { let _ = p.render_target().map(|t| t.width()); }
    /// ```
    fn render_target(&self) -> Result<RenderTarget, RenderError> {
        let b = self.bounds();
        RenderTarget::from_logical(b.width(), b.height(), self.scale_factor(), false)
    }
}

/// Errors from creating or driving a peer, catalogued in `errors/peer.toml`.
/// ```
/// use syrp_error::Diagnostic;
/// assert_eq!(syrp_peer_api::PeerError::Unsupported.code().as_str(), "SYRP-PEER-0001");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PeerError {
    /// This peer or platform cannot do that (`SYRP-PEER-0001`).
    Unsupported,
    /// The parent handle is of another platform or no longer valid (`SYRP-PEER-0002`).
    InvalidParent,
    /// The window system failed; the message is its own error (`SYRP-PEER-0003`).
    Backend(String),
}

impl fmt::Display for PeerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerError::Unsupported => f.write_str("operation not supported by this peer"),
            PeerError::InvalidParent => f.write_str("invalid parent window handle"),
            PeerError::Backend(msg) => write!(f, "window system failed: {msg}"),
        }
    }
}

impl std::error::Error for PeerError {}

impl Diagnostic for PeerError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            PeerError::Unsupported => "SYRP-PEER-0001",
            PeerError::InvalidParent => "SYRP-PEER-0002",
            PeerError::Backend(_) => "SYRP-PEER-0003",
        })
    }
}
