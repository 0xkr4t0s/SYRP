// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for syrp-peer-api, named after the spec's B<n> items.

use std::cell::RefCell;
use std::rc::Rc;

use syrp_error::Diagnostic;
use syrp_geom::{Point, Rect};
use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_peer_api::accesskit::{Action, Node, NodeId, Role, TreeId, TreeInfo};
use syrp_peer_api::headless::{HeadlessDesktop, Request};
use syrp_peer_api::keyboard_types::{Code, Key, KeyState, Location, NamedKey};
use syrp_peer_api::raw_window_handle::{HandleError, RawWindowHandle, WebWindowHandle};
use syrp_peer_api::{
    ActionRequest, AlertDialog, AlertKind, CursorIcon, Desktop, Display, DragData, DragEvent,
    DragPhase, Event, EventSink, FileDialog, FileDialogKind, HasDisplayHandle, ImeEvent,
    KeyboardEvent, Modifiers, Peer, PeerError, PeerOptions, PointerButton, PointerEvent,
    PointerKind, PointerPhase, PointerSource, TreeUpdate, WheelEvent, WheelUnit, WindowState,
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
        // SAFETY: the headless desktop never passes the handle to an OS.
        let o = unsafe { PeerOptions::child(parent, Rect::new(0.0, 0.0, 9.0, 9.0)) };
        let (mut p, _) = open(&mut d, mt, o);
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
fn b6_injected_bounds_and_state_events_update_the_peer_getters() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (mut p, log) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        p.set_bounds(mt, Rect::new(0.0, 0.0, 300.0, 200.0));
        // The platform clamps the request to a display edge and reports it.
        let applied = Rect::new(0.0, 0.0, 280.0, 200.0);
        assert!(d.inject(mt, p.id(), &Event::BoundsChanged(applied)));
        assert_eq!(p.bounds(), applied);
        p.set_state(mt, WindowState::Fullscreen).unwrap();
        // The user restores the window; the event wins until the next setter call.
        assert!(d.inject(mt, p.id(), &Event::StateChanged(WindowState::Normal)));
        assert_eq!(p.state(), WindowState::Normal);
        p.set_state(mt, WindowState::Maximised).unwrap();
        assert_eq!(p.state(), WindowState::Maximised);
        assert_eq!(log.borrow().len(), 2);
    });
}

#[test]
fn b2_pointer_events_carry_pressure_tilt_and_source_ids() {
    let pen = PointerSource::new(PointerKind::Pen, 3);
    let down = PointerEvent::new(PointerPhase::Down, pen, Point::new(1.0, 2.0))
        .with_button(PointerButton::Primary)
        .with_pressure(f32::NAN)
        .with_tilt(0.5, 9.0)
        .with_modifiers(Modifiers::ALT);
    assert_eq!(
        (down.source().kind(), down.source().id()),
        (PointerKind::Pen, 3)
    );
    assert_eq!(
        (down.pressure(), down.tilt()),
        (Some(0.0), Some((0.5, 1.0)))
    );
    assert!(down.buttons().contains(PointerButton::Primary) && down.modifiers().alt());
    let finger = PointerSource::new(PointerKind::Touch, 1);
    let up = PointerEvent::new(PointerPhase::Up, finger, Point::new(0.0, 0.0))
        .with_button(PointerButton::Primary)
        .with_pressure(-1.0);
    assert!(up.buttons().is_empty() && up.pressure() == Some(0.0));
    assert_ne!(finger, PointerSource::new(PointerKind::Touch, 2));
    let hover = PointerEvent::new(
        PointerPhase::Move,
        PointerSource::MOUSE,
        Point::new(0.0, 0.0),
    );
    assert_eq!(
        (hover.pressure(), hover.tilt(), hover.button()),
        (None, None, None)
    );
}

#[test]
fn b2_headless_delivers_pointer_wheel_and_magnify_in_order() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (p, log) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        let at = Point::new(1.0, 1.0);
        let events = [
            Event::Pointer(PointerEvent::new(
                PointerPhase::Enter,
                PointerSource::MOUSE,
                at,
            )),
            Event::Wheel(WheelEvent::new(at, 0.0, -2.5, WheelUnit::Pixels).with_inertial(true)),
            Event::Wheel(WheelEvent::new(at, 1.0, 0.0, WheelUnit::Lines)),
            Event::Magnify(at, 1.1),
        ];
        for ev in &events {
            assert!(d.inject(mt, p.id(), ev));
        }
        assert_eq!(*log.borrow(), events);
    });
}

#[test]
fn b2_drops_arrive_only_after_the_peer_accepts_the_drag() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (mut p, log) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        let files = DragData::Files(vec!["/a.wav".into(), "/b.wav".into()]);
        let step = |ph, data: &DragData| {
            Event::Drag(DragEvent::new(ph, Point::new(3.0, 4.0), data.clone()))
        };
        assert!(d.inject(mt, p.id(), &step(DragPhase::Enter, &files)));
        assert!(
            !d.inject(mt, p.id(), &step(DragPhase::Drop, &files)),
            "not accepted yet"
        );
        p.accept_drop(mt, true);
        assert!(d.inject(mt, p.id(), &step(DragPhase::Over, &files)));
        assert!(d.inject(mt, p.id(), &step(DragPhase::Drop, &files)));
        // A new drag starts unaccepted again.
        let text = DragData::Text("C4 E4 G4".into());
        assert!(d.inject(mt, p.id(), &step(DragPhase::Enter, &text)));
        assert!(!d.inject(mt, p.id(), &step(DragPhase::Drop, &text)));
        assert!(d.inject(mt, p.id(), &step(DragPhase::Leave, &text)));
        let phases: Vec<_> = log
            .borrow()
            .iter()
            .map(|e| match e {
                Event::Drag(d) => d.phase(),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        use DragPhase::*;
        assert_eq!(phases, [Enter, Over, Drop, Enter, Leave]);
        assert_eq!(
            d.requests().last(),
            Some(&Request::AcceptDrop(p.id(), true))
        );
    });
}

#[test]
fn b4_cursor_requests_are_recorded() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (mut p, _) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        p.set_cursor(mt, Some(CursorIcon::EwResize));
        p.set_cursor(mt, None);
        let reqs = d.take_requests();
        assert_eq!(
            reqs[1..],
            [
                Request::SetCursor(p.id(), Some(CursorIcon::EwResize)),
                Request::SetCursor(p.id(), None)
            ]
        );
    });
}

#[test]
fn b1_headless_has_no_native_window_or_display_handle() {
    let mut d = HeadlessDesktop::new(1.0);
    assert!(matches!(d.display_handle(), Err(HandleError::NotSupported)));
    ManualLoop::new().with_token(|mt| {
        let (p, _) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        assert!(matches!(p.window_handle(), Err(HandleError::NotSupported)));
    });
}

#[test]
fn b4_clipboard_round_trips_text() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        assert_eq!(d.clipboard_text(mt), Ok(None));
        d.set_clipboard_text(mt, "C4 E4 G4").unwrap();
        assert_eq!(d.clipboard_text(mt), Ok(Some("C4 E4 G4".into())));
        assert_eq!(
            d.take_requests(),
            [Request::SetClipboard("C4 E4 G4".into())]
        );
    });
}

#[test]
fn b4_displays_report_work_areas_and_scale_primary_first() {
    let d = HeadlessDesktop::new(1.5);
    let only = d.displays();
    assert_eq!(only.len(), 1);
    assert!(only[0].is_primary() && only[0].scale() == 1.5);
    assert!(only[0].work_area().height() < only[0].bounds().height());
    let main = Rect::new(0.0, 0.0, 1440.0, 900.0);
    let side = Rect::new(1440.0, 0.0, 1920.0, 1080.0);
    d.set_displays(vec![
        Display::new(main, Rect::new(0.0, 25.0, 1440.0, 875.0), 2.0).with_primary(true),
        Display::new(side, side, 0.0),
    ]);
    let both = d.displays();
    assert_eq!(
        (both[0].scale(), both[1].scale(), both[1].bounds()),
        (2.0, 1.0, side)
    );
    d.set_displays(vec![]);
    assert_eq!(d.displays().len(), 2, "an empty list is ignored");
}

#[test]
fn b4_dialogs_complete_later_exactly_once_in_order() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (p, _) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        let got: Rc<RefCell<Vec<String>>> = Rc::default();
        let g = Rc::clone(&got);
        let open_dlg = FileDialog::new(FileDialogKind::Open).with_filter("Audio", ["wav"]);
        d.show_file_dialog(
            mt,
            Some(p.id()),
            open_dlg.clone(),
            Box::new(move |_, paths| {
                g.borrow_mut().push(format!("open {paths:?}"));
            }),
        )
        .unwrap();
        let g = Rc::clone(&got);
        d.show_file_dialog(
            mt,
            None,
            FileDialog::new(FileDialogKind::Save),
            Box::new(move |_, paths| {
                g.borrow_mut().push(format!("save {}", paths.len()));
            }),
        )
        .unwrap();
        let g = Rc::clone(&got);
        let alert = AlertDialog::new(AlertKind::Question, "Quit?", "Unsaved changes")
            .with_buttons(["Save", "Discard"]);
        d.show_alert(
            mt,
            Some(p.id()),
            alert.clone(),
            Box::new(move |_, b| {
                g.borrow_mut().push(format!("alert {b:?}"));
            }),
        )
        .unwrap();
        assert!(
            got.borrow().is_empty(),
            "callbacks never run inside the request"
        );
        assert!(d.answer_alert(mt, Some(1)));
        assert!(d.answer_file_dialog(mt, vec!["/a.wav".into()]));
        assert!(d.answer_file_dialog(mt, vec![]));
        assert!(!d.answer_file_dialog(mt, vec![]) && !d.answer_alert(mt, None));
        assert_eq!(
            *got.borrow(),
            ["alert Some(1)", "open [\"/a.wav\"]", "save 0"]
        );
        let reqs = d.take_requests();
        assert_eq!(reqs[1], Request::FileDialog(Some(p.id()), open_dlg));
        assert_eq!(reqs[3], Request::Alert(Some(p.id()), alert));
    });
}

#[test]
fn b5_peers_take_accesskit_updates_and_report_actions() {
    let mut d = HeadlessDesktop::new(1.0);
    ManualLoop::new().with_token(|mt| {
        let (mut p, log) = open(&mut d, mt, PeerOptions::top_level(Rect::default()));
        let root = NodeId(1);
        let update = TreeUpdate {
            nodes: vec![(root, Node::new(Role::Window))],
            tree: Some(TreeInfo::new(root)),
            tree_id: TreeId::ROOT,
            focus: root,
        };
        assert!(d.inject(mt, p.id(), &Event::AccessibilityActivated));
        p.update_accessibility(mt, update.clone());
        let click = ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: root,
            data: None,
        };
        assert!(d.inject(mt, p.id(), &Event::AccessibilityAction(click.clone())));
        assert!(d.inject(mt, p.id(), &Event::AccessibilityDeactivated));
        assert_eq!(
            d.requests().last(),
            Some(&Request::UpdateAccessibility(p.id(), update))
        );
        assert_eq!(
            *log.borrow(),
            [
                Event::AccessibilityActivated,
                Event::AccessibilityAction(click),
                Event::AccessibilityDeactivated
            ]
        );
    });
}

#[test]
fn b7_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/event.rs"),
        include_str!("../src/headless.rs"),
        include_str!("../src/pointer.rs"),
        include_str!("../src/drag.rs"),
        include_str!("../src/dialog.rs"),
        include_str!("../src/display.rs"),
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
