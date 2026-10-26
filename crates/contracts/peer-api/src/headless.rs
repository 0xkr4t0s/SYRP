// SPDX-License-Identifier: MIT OR Apache-2.0
//! A platform-free [`Desktop`] for tests: peers apply every request at once and record it,
//! and tests inject events as if the platform had sent them.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;

use syrp_geom::Rect;
use syrp_loop_api::MessageThread;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use crate::{
    AlertDialog, AlertDone, CursorIcon, Desktop, Display, DragPhase, Event, EventSink, FileDialog,
    FileDialogDone, Peer, PeerError, PeerId, PeerOptions, TreeUpdate, WindowState,
};

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
    /// [`Peer::set_cursor`].
    SetCursor(PeerId, Option<CursorIcon>),
    /// [`Peer::accept_drop`].
    AcceptDrop(PeerId, bool),
    /// [`Peer::update_accessibility`].
    UpdateAccessibility(PeerId, TreeUpdate),
    /// [`Desktop::set_clipboard_text`].
    SetClipboard(String),
    /// [`Desktop::show_file_dialog`], with its parent.
    FileDialog(Option<PeerId>, FileDialog),
    /// [`Desktop::show_alert`], with its parent.
    Alert(Option<PeerId>, AlertDialog),
    /// The peer was dropped.
    Destroy(PeerId),
}

struct Shared {
    next_id: u64,
    scale: f64,
    requests: Vec<Request>,
    peers: Vec<Slot>,
    clipboard: Option<String>,
    displays: Option<Vec<Display>>,
    file_dialogs: VecDeque<FileDialogDone>,
    alerts: VecDeque<AlertDone>,
}

/// A live peer's sink (taken while it runs) and the state platform events can change.
struct Slot {
    id: PeerId,
    sink: Option<EventSink>,
    bounds: Rect<f64>,
    state: WindowState,
    drop_accepted: bool,
}

impl Shared {
    fn slot(&mut self, id: PeerId) -> Option<&mut Slot> {
        self.peers.iter_mut().find(|s| s.id == id)
    }
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
    /// A desktop whose single display has `scale` physical pixels per logical unit. Until
    /// [`set_displays`](Self::set_displays), that display is 1920 x 1080 logical units at the
    /// origin, with a work area 40 units shorter (a task bar at the bottom).
    /// ```
    /// let _ = syrp_peer_api::headless::HeadlessDesktop::new(1.5);
    /// ```
    pub fn new(scale: f64) -> Self {
        HeadlessDesktop {
            shared: Rc::new(RefCell::new(Shared {
                next_id: 1,
                scale,
                requests: Vec::new(),
                peers: Vec::new(),
                clipboard: None,
                displays: None,
                file_dialogs: VecDeque::new(),
                alerts: VecDeque::new(),
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
    /// The sink may call back into the desktop or the peer. Like a platform, a delivered
    /// [`Event::BoundsChanged`] or [`Event::StateChanged`] first updates what the peer's
    /// `bounds()` or `state()` returns. Drags follow the platform rule: `Enter` resets the
    /// peer's [`Peer::accept_drop`] answer to `false`, and a `Drop` is refused (not delivered,
    /// `false`) unless the last answer was `true`.
    /// ```
    /// # use syrp_loop_api::ManualLoop;
    /// use syrp_peer_api::{headless::HeadlessDesktop, Event, PeerId};
    /// let d = HeadlessDesktop::new(1.0);
    /// ManualLoop::new().with_token(|mt| assert!(!d.inject(mt, PeerId::from_raw(9), &Event::FocusLost)));
    /// ```
    pub fn inject(&self, mt: &MessageThread<'_>, peer: PeerId, event: &Event) -> bool {
        let mut sink = {
            let mut sh = self.shared.borrow_mut();
            let Some(slot) = sh.slot(peer) else {
                return false;
            };
            let Some(sink) = slot.sink.take() else {
                return false;
            };
            match event {
                Event::BoundsChanged(b) => slot.bounds = *b,
                Event::StateChanged(st) => slot.state = *st,
                Event::Drag(d) if d.phase() == DragPhase::Enter => slot.drop_accepted = false,
                Event::Drag(d) if d.phase() == DragPhase::Drop && !slot.drop_accepted => {
                    slot.sink = Some(sink);
                    return false;
                }
                _ => {}
            }
            sink
        };
        sink(mt, event);
        if let Some(slot) = self.shared.borrow_mut().slot(peer) {
            slot.sink = Some(sink);
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
            sh.peers.iter().map(|s| s.id).collect()
        };
        for id in ids {
            self.inject(mt, id, &Event::ScaleChanged(scale));
        }
    }
}

impl HeadlessDesktop {
    /// Replaces the display list that [`Desktop::displays`] reports (ignored if empty).
    /// ```
    /// # use syrp_geom::Rect;
    /// use syrp_peer_api::{headless::HeadlessDesktop, Desktop, Display};
    /// let d = HeadlessDesktop::new(1.0);
    /// let r = Rect::new(0.0, 0.0, 800.0, 600.0);
    /// d.set_displays(vec![Display::new(r, r, 2.0).with_primary(true)]);
    /// assert_eq!(d.displays()[0].scale(), 2.0);
    /// ```
    pub fn set_displays(&self, displays: Vec<Display>) {
        if !displays.is_empty() {
            self.shared.borrow_mut().displays = Some(displays);
        }
    }

    /// Completes the oldest open file dialog with `paths` (empty: cancelled), running its
    /// callback. Returns `false` if no file dialog is open.
    /// ```
    /// # use syrp_loop_api::ManualLoop;
    /// let d = syrp_peer_api::headless::HeadlessDesktop::new(1.0);
    /// ManualLoop::new().with_token(|mt| assert!(!d.answer_file_dialog(mt, vec![])));
    /// ```
    pub fn answer_file_dialog(&self, mt: &MessageThread<'_>, paths: Vec<PathBuf>) -> bool {
        let done = self.shared.borrow_mut().file_dialogs.pop_front();
        done.map(|done| done(mt, paths)).is_some()
    }

    /// Completes the oldest open alert with `button` (`None`: dismissed), running its
    /// callback. Returns `false` if no alert is open.
    /// ```
    /// # use syrp_loop_api::ManualLoop;
    /// let d = syrp_peer_api::headless::HeadlessDesktop::new(1.0);
    /// ManualLoop::new().with_token(|mt| assert!(!d.answer_alert(mt, Some(0))));
    /// ```
    pub fn answer_alert(&self, mt: &MessageThread<'_>, button: Option<usize>) -> bool {
        let done = self.shared.borrow_mut().alerts.pop_front();
        done.map(|done| done(mt, button)).is_some()
    }
}

impl Desktop for HeadlessDesktop {
    fn clipboard_text(&mut self, _mt: &MessageThread<'_>) -> Result<Option<String>, PeerError> {
        Ok(self.shared.borrow().clipboard.clone())
    }

    fn set_clipboard_text(&mut self, _mt: &MessageThread<'_>, text: &str) -> Result<(), PeerError> {
        let mut sh = self.shared.borrow_mut();
        sh.clipboard = Some(text.to_owned());
        sh.requests.push(Request::SetClipboard(text.to_owned()));
        Ok(())
    }

    fn displays(&self) -> Vec<Display> {
        let sh = self.shared.borrow();
        sh.displays.clone().unwrap_or_else(|| {
            let bounds = Rect::new(0.0, 0.0, 1920.0, 1080.0);
            let work = Rect::new(0.0, 0.0, 1920.0, 1040.0);
            vec![Display::new(bounds, work, sh.scale).with_primary(true)]
        })
    }

    fn show_file_dialog(
        &mut self,
        _mt: &MessageThread<'_>,
        parent: Option<PeerId>,
        dialog: FileDialog,
        done: FileDialogDone,
    ) -> Result<(), PeerError> {
        let mut sh = self.shared.borrow_mut();
        sh.requests.push(Request::FileDialog(parent, dialog));
        sh.file_dialogs.push_back(done);
        Ok(())
    }

    fn show_alert(
        &mut self,
        _mt: &MessageThread<'_>,
        parent: Option<PeerId>,
        alert: AlertDialog,
        done: AlertDone,
    ) -> Result<(), PeerError> {
        let mut sh = self.shared.borrow_mut();
        sh.requests.push(Request::Alert(parent, alert));
        sh.alerts.push_back(done);
        Ok(())
    }

    fn create_peer(
        &mut self,
        _mt: &MessageThread<'_>,
        options: PeerOptions,
        events: EventSink,
    ) -> Result<Box<dyn Peer>, PeerError> {
        let mut sh = self.shared.borrow_mut();
        let id = PeerId::from_raw(sh.next_id);
        sh.next_id += 1;
        sh.peers.push(Slot {
            id,
            sink: Some(events),
            bounds: options.bounds(),
            state: WindowState::Normal,
            drop_accepted: false,
        });
        sh.requests.push(Request::Create(id, options.clone()));
        Ok(Box::new(HeadlessPeer {
            id,
            shared: Rc::clone(&self.shared),
            child: options.parent().is_some(),
            visible: options.is_visible(),
            title: options.title().to_owned(),
        }))
    }
}

struct HeadlessPeer {
    id: PeerId,
    shared: Rc<RefCell<Shared>>,
    child: bool,
    visible: bool,
    title: String,
}

impl HeadlessPeer {
    fn record(&self, r: Request) {
        self.shared.borrow_mut().requests.push(r);
    }

    fn with_slot<R>(&self, f: impl FnOnce(&mut Slot) -> R) -> R {
        let mut sh = self.shared.borrow_mut();
        f(sh.slot(self.id).expect("a live peer has a slot"))
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
        self.with_slot(|s| s.bounds)
    }
    fn set_bounds(&mut self, _mt: &MessageThread<'_>, bounds: Rect<f64>) {
        self.with_slot(|s| s.bounds = bounds);
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
        self.with_slot(|s| s.state)
    }
    fn set_state(&mut self, _mt: &MessageThread<'_>, state: WindowState) -> Result<(), PeerError> {
        if self.child && state != WindowState::Normal {
            return Err(PeerError::Unsupported);
        }
        self.with_slot(|s| s.state = state);
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
    fn set_cursor(&mut self, _mt: &MessageThread<'_>, cursor: Option<CursorIcon>) {
        self.record(Request::SetCursor(self.id, cursor));
    }
    fn accept_drop(&mut self, _mt: &MessageThread<'_>, accept: bool) {
        self.with_slot(|s| s.drop_accepted = accept);
        self.record(Request::AcceptDrop(self.id, accept));
    }
    fn update_accessibility(&mut self, _mt: &MessageThread<'_>, update: TreeUpdate) {
        self.record(Request::UpdateAccessibility(self.id, update));
    }
}

/// No native display: always [`HandleError::NotSupported`].
impl HasDisplayHandle for HeadlessDesktop {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Err(HandleError::NotSupported)
    }
}

/// No native window: always [`HandleError::NotSupported`].
impl HasWindowHandle for HeadlessPeer {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::NotSupported)
    }
}

impl Drop for HeadlessPeer {
    fn drop(&mut self) {
        let mut sh = self.shared.borrow_mut();
        sh.peers.retain(|s| s.id != self.id);
        sh.requests.push(Request::Destroy(self.id));
    }
}
