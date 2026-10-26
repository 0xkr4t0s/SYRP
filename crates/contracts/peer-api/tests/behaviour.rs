// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for syrp-peer-api, named after the spec's B<n> items.

use std::cell::RefCell;
use std::rc::Rc;

use syrp_error::Diagnostic;
use syrp_geom::Rect;
use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_peer_api::headless::{HeadlessDesktop, Request};
use syrp_peer_api::keyboard_types::{Code, Key, KeyState, Location, NamedKey};
use syrp_peer_api::raw_window_handle::{RawWindowHandle, WebWindowHandle};
use syrp_peer_api::{
    Desktop, Event, EventSink, ImeEvent, KeyboardEvent, Modifiers, Peer, PeerError, PeerOptions,
    WindowState,
};

type Log = Rc<RefCell<Vec<Event>>>;

fn sink() -> (EventSink, Log) {
    let log: Log = Rc::default();
    let l = Rc::clone(&log);
    (
        Box::new(move |_, ev: &Event| l.borrow_mut().push(ev.clone())),
        log,
    )
}

fn open(d: &mut HeadlessDesktop, mt: &MessageThread<'_>, o: PeerOptions) -> (Box<dyn Peer>, Log) {
    let (s, log) = sink();
    (d.create_peer(mt, o, s).unwrap(), log)
}

#[test]
fn b1_top_level_peer_applies_window_requests() {
    let mut d = HeadlessDesktop::new(1.25);
    ManualLoop::new().with_token(|mt| {
        let o = PeerOptions::top_level(Rect::new(10.0, 20.0, 101.0, 10.0)).with_title("Synth");
        let (mut p, _) = open(&mut d, mt, o);
        assert!(!p.is_child() && p.is_visible());
        assert_eq!((p.title(), p.scale_factor()), ("Synth", 1.25));
        // The render target follows RenderTarget::from_logical: 126.25 and 12.5 round up.
        let t = p.render_target().unwrap();
        assert_eq!(
            (t.width(), t.height(), t.scale(), t.pixels()),
            (127, 13, 1.25, None)
        );

        let r = Rect::new(0.0, 0.0, 50.0, 40.0);
        p.set_bounds(mt, r);
        p.set_visible(mt, false);
        for s in [
            WindowState::Minimised,
            WindowState::Maximised,
            WindowState::Fullscreen,
        ] {
            p.set_state(mt, s).unwrap();
            assert_eq!(p.state(), s);
        }
        p.set_title(mt, "Synth 2");
        p.repaint(mt, Rect::new(1.0, 1.0, 2.0, 2.0));
        assert_eq!(
            (p.bounds(), p.is_visible(), p.title()),
            (r, false, "Synth 2")
        );
        let id = p.id();
        drop(p);
        let reqs = d.take_requests();
        assert!(matches!(reqs[0], Request::Create(i, _) if i == id));
        assert_eq!(
            reqs[1..3],
            [Request::SetBounds(id, r), Request::SetVisible(id, false)]
        );
        assert_eq!(reqs[5], Request::SetState(id, WindowState::Fullscreen));
        assert_eq!(reqs[6], Request::SetTitle(id, "Synth 2".into()));
        assert_eq!(reqs[7], Request::Repaint(id, Rect::new(1.0, 1.0, 2.0, 2.0)));
        assert_eq!(reqs[8], Request::Destroy(id));
    });
}

#[test]
fn b1_child_peer_embeds_in_host_parent_and_cannot_change_state() {
    let mut d = HeadlessDesktop::new(1.0);
    let parent = RawWindowHandle::Web(WebWindowHandle::new(7));
    ManualLoop::new().with_token(|mt| {
        let (mut p, _) = open(
            &mut d,
            mt,
            PeerOptions::child(parent, Rect::new(0.0, 0.0, 9.0, 9.0)),
        );
        assert!(p.is_child());
        assert_eq!(
            p.set_state(mt, WindowState::Maximised),
            Err(PeerError::Unsupported)
        );
        assert_eq!(p.state(), WindowState::Normal);
        assert_eq!(p.set_state(mt, WindowState::Normal), Ok(()));
        match &d.requests()[0] {
            Request::Create(_, o) => {
                assert_eq!((o.parent(), o.is_decorated()), (Some(parent), false))
            }
            other => panic!("unexpected {other:?}"),
        }
    });
    assert_eq!(PeerError::InvalidParent.code().as_str(), "SYRP-PEER-0002");
    assert_eq!(
        PeerError::Backend("x".into()).code().as_str(),
        "SYRP-PEER-0003"
    );
}

#[test]
fn b1_peer_ids_are_distinct_and_scale_changes_are_reported() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (a, la) = open(
            &mut d,
            mt,
            PeerOptions::top_level(Rect::new(0.0, 0.0, 10.0, 10.0)),
        );
        let (b, _) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        assert_ne!(a.id(), b.id());
        d.set_scale(mt, 2.0);
        assert_eq!(*la.borrow(), [Event::ScaleChanged(2.0)]);
        assert_eq!(a.render_target().unwrap().width(), 20);
    });
}

#[test]
fn b2_headless_delivers_key_and_focus_events_in_order() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (mut p, log) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        let id = p.id();
        let key = KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(NamedKey::Enter),
            code: Code::Enter,
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            repeat: false,
            is_composing: false,
        };
        let up = KeyboardEvent {
            state: KeyState::Up,
            ..key.clone()
        };
        let events = [
            Event::FocusGained,
            Event::Key(key),
            Event::Key(up),
            Event::FocusLost,
        ];
        for ev in &events {
            assert!(d.inject(mt, id, ev));
        }
        assert_eq!(*log.borrow(), events);
        p.grab_focus(mt);
        assert_eq!(d.requests().last(), Some(&Request::GrabFocus(id)));
        drop(p);
        assert!(
            !d.inject(mt, id, &Event::FocusGained),
            "dropped peers receive nothing"
        );
    });
}

#[test]
fn b3_ime_composition_events_and_caret_reporting() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (mut p, log) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        let caret = Rect::new(30.0, 4.0, 1.0, 14.0);
        p.set_ime_caret(mt, Some(caret));
        let seq = [
            Event::Ime(ImeEvent::Start),
            Event::Ime(ImeEvent::Preedit("に".into(), Some(3..3))),
            Event::Ime(ImeEvent::Preedit("日本".into(), Some(0..6))),
            Event::Ime(ImeEvent::Commit("日本".into())),
            Event::Ime(ImeEvent::End),
        ];
        for ev in &seq {
            d.inject(mt, p.id(), ev);
        }
        p.set_ime_caret(mt, None);
        assert_eq!(*log.borrow(), seq);
        let reqs = d.take_requests();
        assert_eq!(
            reqs[1..],
            [
                Request::SetImeCaret(p.id(), Some(caret)),
                Request::SetImeCaret(p.id(), None)
            ]
        );
    });
}

#[test]
fn b6_sinks_may_call_back_into_the_desktop_and_reentry_is_refused() {
    let d = HeadlessDesktop::new(1.0);
    let mut creator = d.clone();
    ManualLoop::new().with_token(|mt| {
        let inner = d.clone();
        let reentered = Rc::new(RefCell::new(None));
        let r = Rc::clone(&reentered);
        let me = Rc::new(RefCell::new(None));
        let m = Rc::clone(&me);
        let s: EventSink = Box::new(move |mt, _| {
            let id = m.borrow().unwrap();
            *r.borrow_mut() = Some(inner.inject(mt, id, &Event::CloseRequested));
        });
        let p = creator
            .create_peer(mt, PeerOptions::top_level(Rect::default()), s)
            .unwrap();
        *me.borrow_mut() = Some(p.id());
        assert!(d.inject(mt, p.id(), &Event::CloseRequested));
        assert_eq!(*reentered.borrow(), Some(false));
        // The sink was put back after the call.
        assert!(d.inject(mt, p.id(), &Event::FocusLost));
    });
}

#[test]
fn b7_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/event.rs"),
        include_str!("../src/headless.rs"),
    ];
    let mut in_struct = false;
    for line in src.iter().flat_map(|s| s.lines()) {
        if line.starts_with("pub struct") {
            assert!(!line.contains("(pub"), "public tuple field: {line}");
            in_struct = line.ends_with('{');
        } else if line.starts_with('}') {
            in_struct = false;
        } else if in_struct {
            assert!(
                !line.trim_start().starts_with("pub"),
                "public field: {line}"
            );
        }
    }
}
