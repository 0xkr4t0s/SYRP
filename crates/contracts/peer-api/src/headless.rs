// SPDX-License-Identifier: MIT OR Apache-2.0
//! A platform-free [`Desktop`] for tests: peers apply every request at once and record it,
//! and tests inject events as if the platform had sent them.

use std::cell::RefCell;
use std::rc::Rc;

use syrp_geom::Rect;
use syrp_loop_api::MessageThread;

use crate::{Desktop, Event, EventSink, Peer, PeerError, PeerId, PeerOptions, WindowState};

/// A request a headless peer received, in arrival order.
/// ```
/// use syrp_peer_api::{headless::Request, PeerId};
/// let r = Request::GrabFocus(PeerId::from_raw(1));
/// assert_eq!(r, Request::GrabFocus(PeerId::from_raw(1)));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Request {
    /// A peer was created with these options.
    Create(PeerId, PeerOptions),
    /// [`Peer::set_bounds`].
    SetBounds(PeerId, Rect<f64>),
    /// [`Peer::set_visible`].
    SetVisible(PeerId, bool),
    /// [`Peer::set_state`] (recorded only when accepted).
    SetState(PeerId, WindowState),
    /// [`Peer::set_title`].
    SetTitle(PeerId, String),
    /// [`Peer::repaint`].
    Repaint(PeerId, Rect<f64>),
    /// [`Peer::grab_focus`].
    GrabFocus(PeerId),
    /// [`Peer::set_ime_caret`].
    SetImeCaret(PeerId, Option<Rect<f64>>),
    /// The peer was dropped.
    Destroy(PeerId),
}

struct Shared {
    next_id: u64,
    scale: f64,
    requests: Vec<Request>,
    sinks: Vec<(PeerId, Option<EventSink>)>,
}

/// A headless desktop. Clones share the same peers and request log, so a test can keep one
/// clone while code under test owns another.
/// ```
/// # use syrp_geom::Rect; use syrp_loop_api::ManualLoop;
/// use syrp_peer_api::{headless::{HeadlessDesktop, Request}, Desktop, Event, PeerOptions};
/// let mut desktop = HeadlessDesktop::new(1.0);
/// ManualLoop::new().with_token(|mt| {
///     let seen = std::rc::Rc::new(std::cell::Cell::new(0));
///     let s = seen.clone();
///     let mut p = desktop.create_peer(mt, PeerOptions::top_level(Rect::default()), Box::new(move |_, _| s.set(s.get() + 1))).unwrap();
///     p.set_title(mt, "Hi");
///     assert!(desktop.inject(mt, p.id(), &Event::CloseRequested));
///     assert_eq!(seen.get(), 1);
///     assert_eq!(desktop.take_requests().last(), Some(&Request::SetTitle(p.id(), "Hi".into())));
/// });
/// ```
#[derive(Clone)]
pub struct HeadlessDesktop {
    shared: Rc<RefCell<Shared>>,
}

impl HeadlessDesktop {
    /// A desktop whose single display has `scale` physical pixels per logical unit.
    /// ```
    /// let _ = syrp_peer_api::headless::HeadlessDesktop::new(1.5);
    /// ```
    pub fn new(scale: f64) -> Self {
        HeadlessDesktop {
            shared: Rc::new(RefCell::new(Shared {
                next_id: 1,
                scale,
                requests: Vec::new(),
                sinks: Vec::new(),
            })),
        }
    }

    /// The requests recorded so far.
    /// ```
    /// assert!(syrp_peer_api::headless::HeadlessDesktop::new(1.0).requests().is_empty());
    /// ```
    pub fn requests(&self) -> Vec<Request> {
        self.shared.borrow().requests.clone()
    }

    /// Returns the requests recorded so far and clears the log.
    /// ```
    /// assert!(syrp_peer_api::headless::HeadlessDesktop::new(1.0).take_requests().is_empty());
    /// ```
    pub fn take_requests(&self) -> Vec<Request> {
        std::mem::take(&mut self.shared.borrow_mut().requests)
    }

    /// Delivers `event` to `peer`'s sink as the platform would. Returns `false` if the peer
    /// does not exist (dropped) or is already handling an event (re-entrant injection).
    /// The sink may call back into the desktop or the peer.
    /// ```
    /// # use syrp_loop_api::ManualLoop;
    /// use syrp_peer_api::{headless::HeadlessDesktop, Event, PeerId};
    /// let d = HeadlessDesktop::new(1.0);
    /// ManualLoop::new().with_token(|mt| assert!(!d.inject(mt, PeerId::from_raw(9), &Event::FocusLost)));
    /// ```
    pub fn inject(&self, mt: &MessageThread<'_>, peer: PeerId, event: &Event) -> bool {
        let taken = {
            let mut sh = self.shared.borrow_mut();
            sh.sinks
                .iter_mut()
                .find(|(id, _)| *id == peer)
                .and_then(|(_, s)| s.take())
        };
        let Some(mut sink) = taken else {
            return false;
        };
        sink(mt, event);
        let mut sh = self.shared.borrow_mut();
        if let Some((_, slot)) = sh.sinks.iter_mut().find(|(id, _)| *id == peer) {
            *slot = Some(sink);
        }
        true
    }

    /// Moves the display to a new scale factor: every peer reports it from then on and
    /// receives [`Event::ScaleChanged`].
    /// ```
    /// # use syrp_geom::Rect; use syrp_loop_api::ManualLoop;
    /// use syrp_peer_api::{headless::HeadlessDesktop, Desktop, PeerOptions};
    /// let mut d = HeadlessDesktop::new(1.0);
    /// ManualLoop::new().with_token(|mt| {
    ///     let p = d.create_peer(mt, PeerOptions::top_level(Rect::default()), Box::new(|_, _| {})).unwrap();
    ///     d.set_scale(mt, 2.0);
    ///     assert_eq!(p.scale_factor(), 2.0);
    /// });
    /// ```
    pub fn set_scale(&self, mt: &MessageThread<'_>, scale: f64) {
        let ids: Vec<PeerId> = {
            let mut sh = self.shared.borrow_mut();
            sh.scale = scale;
            sh.sinks.iter().map(|(id, _)| *id).collect()
        };
        for id in ids {
            self.inject(mt, id, &Event::ScaleChanged(scale));
        }
    }
}

impl Desktop for HeadlessDesktop {
    fn create_peer(
        &mut self,
        _mt: &MessageThread<'_>,
        options: PeerOptions,
        events: EventSink,
    ) -> Result<Box<dyn Peer>, PeerError> {
        let mut sh = self.shared.borrow_mut();
        let id = PeerId::from_raw(sh.next_id);
        sh.next_id += 1;
        sh.sinks.push((id, Some(events)));
        sh.requests.push(Request::Create(id, options.clone()));
        Ok(Box::new(HeadlessPeer {
            id,
            shared: Rc::clone(&self.shared),
            child: options.parent().is_some(),
            bounds: options.bounds(),
            visible: options.is_visible(),
            state: WindowState::Normal,
            title: options.title().to_owned(),
        }))
    }
}

struct HeadlessPeer {
    id: PeerId,
    shared: Rc<RefCell<Shared>>,
    child: bool,
    bounds: Rect<f64>,
    visible: bool,
    state: WindowState,
    title: String,
}

impl HeadlessPeer {
    fn record(&self, r: Request) {
        self.shared.borrow_mut().requests.push(r);
    }
}

impl Peer for HeadlessPeer {
    fn id(&self) -> PeerId {
        self.id
    }
    fn is_child(&self) -> bool {
        self.child
    }
    fn bounds(&self) -> Rect<f64> {
        self.bounds
    }
    fn set_bounds(&mut self, _mt: &MessageThread<'_>, bounds: Rect<f64>) {
        self.bounds = bounds;
        self.record(Request::SetBounds(self.id, bounds));
    }
    fn is_visible(&self) -> bool {
        self.visible
    }
    fn set_visible(&mut self, _mt: &MessageThread<'_>, visible: bool) {
        self.visible = visible;
        self.record(Request::SetVisible(self.id, visible));
    }
    fn state(&self) -> WindowState {
        self.state
    }
    fn set_state(&mut self, _mt: &MessageThread<'_>, state: WindowState) -> Result<(), PeerError> {
        if self.child && state != WindowState::Normal {
            return Err(PeerError::Unsupported);
        }
        self.state = state;
        self.record(Request::SetState(self.id, state));
        Ok(())
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn set_title(&mut self, _mt: &MessageThread<'_>, title: &str) {
        title.clone_into(&mut self.title);
        self.record(Request::SetTitle(self.id, title.to_owned()));
    }
    fn scale_factor(&self) -> f64 {
        self.shared.borrow().scale
    }
    fn repaint(&mut self, _mt: &MessageThread<'_>, area: Rect<f64>) {
        self.record(Request::Repaint(self.id, area));
    }
    fn grab_focus(&mut self, _mt: &MessageThread<'_>) {
        self.record(Request::GrabFocus(self.id));
    }
    fn set_ime_caret(&mut self, _mt: &MessageThread<'_>, caret: Option<Rect<f64>>) {
        self.record(Request::SetImeCaret(self.id, caret));
    }
}

impl Drop for HeadlessPeer {
    fn drop(&mut self) {
        let mut sh = self.shared.borrow_mut();
        sh.sinks.retain(|(id, _)| *id != self.id);
        sh.requests.push(Request::Destroy(self.id));
    }
}
