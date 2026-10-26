//! WP-1.2-04 B4 (UndoableAction) and B5 (queries and limits).

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{Store, UndoManager, UndoableAction};
use syrp_var::Identifier;

fn id(name: &str) -> Identifier {
    Identifier::new(name)
}

fn with_mt(f: impl FnOnce(&MessageThread<'_>)) {
    ManualLoop::new().with_token(f);
}

type Journal = Rc<RefCell<Vec<String>>>;

/// Logs `perform:<n>` and `undo:<n>`; coalesces with a following `Push` when `merge` is set.
struct Push {
    log: Journal,
    name: String,
    merge: bool,
    ok: bool,
    units: usize,
}

fn push(log: &Journal, name: &str) -> Push {
    let (log, name) = (Rc::clone(log), name.to_string());
    Push {
        log,
        name,
        merge: false,
        ok: true,
        units: 10,
    }
}

impl UndoableAction for Push {
    fn perform(&mut self, _: &MessageThread<'_>) -> bool {
        self.log.borrow_mut().push(format!("perform:{}", self.name));
        self.ok
    }
    fn undo(&mut self, _: &MessageThread<'_>) -> bool {
        self.log.borrow_mut().push(format!("undo:{}", self.name));
        self.ok
    }
    fn size_in_units(&self) -> usize {
        self.units
    }
    fn coalesce_with(&mut self, next: &dyn UndoableAction) -> bool {
        let Some(next) = (next as &dyn Any).downcast_ref::<Push>() else {
            return false;
        };
        if self.merge {
            self.name = format!("{}+{}", self.name, next.name);
        }
        self.merge
    }
}

#[test]
fn b4_actions_and_store_records_interleave_in_order() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        let log: Journal = Rc::default();
        let l = Rc::clone(&log);
        let _sub = store
            .subscribe(mt, &n, move |s, _, _| {
                l.borrow_mut()
                    .push(format!("store:{}", s.is_performing_undo_redo()));
            })
            .unwrap();
        undo.begin_new_transaction("mixed");
        assert!(undo.perform(mt, push(&log, "a")));
        store
            .set_property_with_undo(mt, &mut undo, &n, id("x"), 1)
            .unwrap();
        assert!(undo.perform(mt, push(&log, "b")));
        log.borrow_mut().clear();
        undo.undo(mt).unwrap();
        assert_eq!(*log.borrow(), ["undo:b", "store:true", "undo:a"]);
        log.borrow_mut().clear();
        undo.redo(mt).unwrap();
        assert_eq!(*log.borrow(), ["perform:a", "store:true", "perform:b"]);
        assert!(!undo.is_performing_undo_redo() && !store.is_performing_undo_redo());
    });
}

#[test]
fn b4_failed_actions_are_dropped_and_coalescing_is_offered() {
    with_mt(|mt| {
        let mut undo = UndoManager::new();
        let log: Journal = Rc::default();
        assert!(!undo.perform(
            mt,
            Push {
                ok: false,
                ..push(&log, "bad")
            }
        ));
        assert!(!undo.can_undo());
        undo.perform(
            mt,
            Push {
                merge: true,
                ..push(&log, "a")
            },
        );
        undo.perform(mt, push(&log, "b")); // absorbed by "a"
        undo.begin_new_transaction("next");
        undo.perform(mt, push(&log, "c")); // a new transaction: not offered
        log.borrow_mut().clear();
        undo.undo(mt).unwrap();
        undo.undo(mt).unwrap();
        assert_eq!(*log.borrow(), ["undo:c", "undo:a+b"]);
    });
}

#[test]
fn b4_a_failing_undo_stops_and_reports_false() {
    struct Fails;
    impl UndoableAction for Fails {
        fn perform(&mut self, _: &MessageThread<'_>) -> bool {
            true
        }
        fn undo(&mut self, _: &MessageThread<'_>) -> bool {
            false
        }
    }
    with_mt(|mt| {
        let mut undo = UndoManager::new();
        let log: Journal = Rc::default();
        undo.perform(mt, push(&log, "a"));
        undo.perform(mt, Fails);
        log.borrow_mut().clear();
        assert!(!undo.undo(mt).unwrap());
        assert!(log.borrow().is_empty()); // stopped before "a"
        assert!(undo.can_redo());
    });
}

#[test]
fn b5_queries_report_history() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        for name in ["one", "two", "three"] {
            undo.begin_new_transaction(name);
            store
                .set_property_with_undo(mt, &mut undo, &n, id(name), 1)
                .unwrap();
        }
        assert_eq!(undo.undo_descriptions(), ["three", "two", "one"]);
        assert_eq!(
            (undo.transaction_count(), undo.undo_description()),
            (3, Some("three"))
        );
        undo.undo(mt).unwrap();
        assert_eq!((undo.can_undo(), undo.can_redo()), (true, true));
        assert_eq!(undo.redo_description(), Some("three"));
        assert_eq!(undo.number_of_units_taken_up(), 3); // redo counts too
        undo.clear_undo_history();
        assert_eq!(
            (
                undo.can_undo(),
                undo.can_redo(),
                undo.number_of_units_taken_up()
            ),
            (false, false, 0)
        );
    });
}

#[test]
fn b5_units_are_one_per_record_plus_one_per_64_payload_bytes() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        store
            .set_property_with_undo(mt, &mut undo, &n, id("s"), "x".repeat(130))
            .unwrap();
        assert_eq!(undo.number_of_units_taken_up(), 1 + 130 / 64);
        undo.begin_new_transaction("replace");
        // old 130 + new 70 bytes = 200 bytes: 1 + 3 units.
        store
            .set_property_with_undo(mt, &mut undo, &n, id("s"), "y".repeat(70))
            .unwrap();
        assert_eq!(undo.number_of_units_taken_up(), 3 + 4);
        let log: Journal = Rc::default();
        undo.perform(
            mt,
            Push {
                units: 25,
                ..push(&log, "big")
            },
        );
        assert_eq!(undo.number_of_units_taken_up(), 3 + 4 + 25);
    });
}

#[test]
fn b5_the_oldest_transactions_go_first_but_min_transactions_stay() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        undo.set_max_number_of_stored_units(3, 2);
        for i in 0..6 {
            undo.begin_new_transaction(format!("t{i}"));
            store
                .set_property_with_undo(mt, &mut undo, &n, id("x"), i)
                .unwrap();
        }
        // Trimming runs as each new transaction starts, before it gets its record.
        assert_eq!(undo.undo_descriptions(), ["t5", "t4", "t3", "t2"]);
        undo.set_max_number_of_stored_units(0, 2);
        assert_eq!(undo.undo_descriptions(), ["t5", "t4"]);
        undo.set_max_number_of_stored_units(0, 0);
        assert_eq!(undo.undo_descriptions(), ["t5"]); // the current transaction is kept
    });
}

#[test]
fn b5_undo_current_transaction_only_discards_it() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        undo.begin_new_transaction("keep");
        store
            .set_property_with_undo(mt, &mut undo, &n, id("a"), 1)
            .unwrap();
        undo.begin_new_transaction("drop");
        store
            .set_property_with_undo(mt, &mut undo, &n, id("b"), 1)
            .unwrap();
        assert!(undo.undo_current_transaction_only(mt).unwrap());
        assert!(!store.has_property(&n, id("b")).unwrap() && !undo.can_redo());
        assert_eq!(undo.undo_description(), Some("keep"));
        undo.begin_new_transaction("empty");
        assert!(!undo.undo_current_transaction_only(mt).unwrap()); // nothing in it yet
        assert!(store.has_property(&n, id("a")).unwrap());
    });
}

#[test]
fn b4_an_action_after_undo_starts_a_new_transaction() {
    with_mt(|mt| {
        let mut undo = UndoManager::new();
        let log: Journal = Rc::default();
        undo.begin_new_transaction("a");
        undo.perform(mt, push(&log, "a"));
        undo.begin_new_transaction("b");
        undo.perform(mt, push(&log, "b"));
        undo.undo(mt).unwrap();
        undo.perform(
            mt,
            Push {
                merge: true,
                ..push(&log, "c")
            },
        ); // no begin
        log.borrow_mut().clear();
        undo.undo(mt).unwrap();
        assert_eq!(*log.borrow(), ["undo:c"]); // not joined to "a"
        assert_eq!(undo.undo_description(), Some("a"));
    });
}

#[test]
fn b5_undo_current_transaction_only_does_nothing_after_undo_or_redo() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        for name in ["a", "b"] {
            undo.begin_new_transaction(name);
            store
                .set_property_with_undo(mt, &mut undo, &n, id(name), 1)
                .unwrap();
        }
        undo.undo(mt).unwrap();
        assert!(!undo.undo_current_transaction_only(mt).unwrap()); // "a" is history now
        assert!(store.has_property(&n, id("a")).unwrap());
        assert_eq!(
            (undo.undo_description(), undo.redo_description()),
            (Some("a"), Some("b"))
        );
        undo.redo(mt).unwrap();
        assert!(!undo.undo_current_transaction_only(mt).unwrap()); // so is "b" after a redo
        assert!(store.has_property(&n, id("b")).unwrap());
        assert_eq!(undo.undo_descriptions(), ["b", "a"]);
        // New work opens a current transaction again.
        store
            .set_property_with_undo(mt, &mut undo, &n, id("c"), 1)
            .unwrap();
        assert!(undo.undo_current_transaction_only(mt).unwrap());
        assert!(!store.has_property(&n, id("c")).unwrap());
        assert_eq!(undo.undo_descriptions(), ["b", "a"]);
    });
}
