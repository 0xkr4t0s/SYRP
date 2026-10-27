//! B3 (`Value` and `CachedValue<T>` over node properties) and B4 (`.handle()` on both).

use std::cell::RefCell;
use std::rc::Rc;

use syrp_compat_state::{CachedValue, Value, ValueListener, ValueTree, VarConvert};
use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{Store, UndoManager};
use syrp_var::Var;

fn with<R>(f: impl FnOnce(&MessageThread<'_>, &Rc<Store>) -> R) -> R {
    ManualLoop::new().with_token(|mt| f(mt, &Rc::new(Store::new())))
}

#[derive(Default)]
struct Seen(Vec<String>);

impl ValueListener for Seen {
    fn value_changed(&mut self, _: &MessageThread<'_>, v: &Value) {
        self.0.push(format!(
            "{}={}",
            v.property_id().as_str(),
            v.get().to_text()
        ));
    }
}

#[test]
fn b3_value_reads_and_writes_the_property() {
    with(|mt, s| {
        let t = ValueTree::new_in(mt, s, "T");
        let v = t.property_as_value("x");
        assert!(v.get().is_void());
        v.set(mt, "a", None).unwrap();
        assert_eq!(t.property("x").to_text(), "a");
        t.set_property(mt, "x", 2, None).unwrap();
        assert_eq!(v.get(), Var::from(2));
        let other = t.property_as_value("x");
        assert!(other.refers_to_same_source_as(&v) && other.handle() == t.handle());
        assert!(!t.property_as_value("y").refers_to_same_source_as(&v));
    });
}

#[test]
fn b3_value_listener_hears_only_its_property_and_real_changes() {
    with(|mt, s| {
        let t = ValueTree::new_in(mt, s, "T");
        let child = ValueTree::new_in(mt, s, "C");
        t.append_child(mt, &child, None).unwrap();
        let mut v = t.property_as_value("x");
        let seen = Rc::new(RefCell::new(Seen::default()));
        v.add_listener(mt, &seen);
        v.add_listener(mt, &seen);
        t.set_property(mt, "x", 1, None).unwrap();
        t.set_property(mt, "x", 1, None).unwrap(); // equal: no change
        t.set_property(mt, "y", 1, None).unwrap();
        child.set_property(mt, "x", 5, None).unwrap(); // a descendant's "x" is another property
        t.remove_property(mt, "x", None).unwrap();
        assert_eq!(seen.borrow().0, ["x=1", "x="]);
        v.clone().set(mt, 3, None).unwrap();
        v.remove_listener(&seen);
        v.set(mt, 4, None).unwrap();
        assert_eq!(seen.borrow().0, ["x=1", "x=", "x=3"]);
    });
}

#[test]
fn b3_value_listener_follows_refer_to_and_dies_with_wrapper() {
    ManualLoop::new().with_token(|mt| {
        let (mut a, b) = (Value::new(mt, 1), Value::new(mt, 2));
        let seen = Rc::new(RefCell::new(Seen::default()));
        a.add_listener(mt, &seen);
        let old = a.clone();
        a.refer_to(mt, &b);
        assert!(
            seen.borrow().0.is_empty(),
            "the switch itself does not notify"
        );
        old.set(mt, 10, None).unwrap();
        b.set(mt, 20, None).unwrap();
        assert_eq!(a.get(), Var::from(20));
        assert_eq!(seen.borrow().0, ["value=20"]);
        drop(a);
        b.set(mt, 30, None).unwrap();
        assert_eq!(seen.borrow().0.len(), 1);
    });
}

#[test]
fn b3_value_set_records_undo() {
    with(|mt, s| {
        let v = ValueTree::new_in(mt, s, "T").property_as_value("x");
        let mut um = UndoManager::new();
        v.set(mt, 1, Some(&mut um)).unwrap();
        um.begin_new_transaction("second");
        v.set(mt, 2, Some(&mut um)).unwrap();
        um.undo(mt).unwrap();
        assert_eq!(v.get(), Var::from(1));
        um.undo(mt).unwrap();
        assert!(v.get().is_void());
    });
}

#[test]
fn b3_value_on_invalid_tree_is_void_and_inert() {
    ManualLoop::new().with_token(|mt| {
        let mut v = ValueTree::default().property_as_value("x");
        v.add_listener(mt, &Rc::new(RefCell::new(Seen::default())));
        v.set(mt, 1, None).unwrap();
        assert!(v.get().is_void() && v.handle().is_none());
    });
}

#[test]
fn b3_cached_value_uses_default_until_set() {
    with(|mt, s| {
        let t = ValueTree::new_in(mt, s, "T");
        let mut c = CachedValue::bound(mt, &t, "gain", 0.5);
        assert!(c.is_using_default());
        assert_eq!((c.get(), *c.default_value()), (0.5, 0.5));
        c.set(mt, 0.75, None).unwrap();
        assert!(!c.is_using_default());
        assert_eq!(c.get(), 0.75);
        c.set_default(0.1);
        assert_eq!(c.get(), 0.75);
        c.reset_to_default(mt, None).unwrap();
        assert_eq!(c.get(), 0.1);
        assert_eq!(
            (c.property_id().as_str(), c.value_tree()),
            ("gain", t.clone())
        );
    });
}

#[test]
fn b3_cached_value_follows_every_route_including_undo() {
    with(|mt, s| {
        let t = ValueTree::new_in(mt, s, "T");
        t.set_property(mt, "n", 3, None).unwrap();
        let c = CachedValue::bound(mt, &t, "n", 0_i32);
        assert_eq!(c.get(), 3, "filled at bind time");
        let mut um = UndoManager::new();
        t.property_as_value("n").set(mt, 4, Some(&mut um)).unwrap();
        assert_eq!(c.get(), 4);
        um.undo(mt).unwrap();
        assert_eq!(c.get(), 3);
        t.remove_all_properties(mt, None).unwrap();
        assert!(c.is_using_default());
        t.set_property(mt, "other", 9, None).unwrap();
        assert_eq!(c.get(), 0);
    });
}

#[test]
fn b3_cached_value_refer_to_rebinds() {
    with(|mt, s| {
        let (a, b) = (ValueTree::new_in(mt, s, "A"), ValueTree::new_in(mt, s, "B"));
        b.set_property(mt, "s", "bee", None).unwrap();
        let mut c: CachedValue<String> = CachedValue::default();
        assert_eq!(c.get(), "");
        c.set(mt, "ignored".into(), None).unwrap(); // unbound: no-op
        c.refer_to(mt, &a, "s", "dflt".into());
        assert_eq!(c.get(), "dflt");
        c.refer_to(mt, &b, "s", "dflt".into());
        assert_eq!(c.get(), "bee");
        a.set_property(mt, "s", "ay", None).unwrap();
        assert_eq!(c.get(), "bee", "no longer bound to a");
        assert_eq!(a.reference_count(), 1, "the old subscription was dropped");
    });
}

#[test]
fn b3_var_conversions_are_loose_on_read() {
    let v = Var::from("12.5");
    assert_eq!((i32::from_var(&v), i64::from_var(&v)), (12, 12));
    assert_eq!((f64::from_var(&v), f32::from_var(&v)), (12.5, 12.5));
    assert!(bool::from_var(&Var::from(1)) && !bool::from_var(&Var::void()));
    assert_eq!(String::from_var(&Var::from(3)), "3");
    assert_eq!(Var::from_var(&v), v);
    assert_eq!(true.to_var(), Var::from(true));
    assert_eq!(7_i64.to_var(), Var::from(7_i64));
    assert_eq!(1.5_f32.to_var(), Var::from(1.5));
    assert_eq!("s".to_owned().to_var(), Var::from("s"));
    with(|mt, s| {
        let t = ValueTree::new_in(mt, s, "T");
        t.set_property(mt, "flag", "true", None).unwrap();
        assert!(CachedValue::bound(mt, &t, "flag", false).get());
    });
}

#[test]
fn b4_value_and_cached_value_expose_the_native_key() {
    with(|mt, s| {
        let t = ValueTree::new_in(mt, s, "T");
        let key = t.handle().unwrap();
        assert_eq!(t.property_as_value("x").handle(), Some(key));
        assert_eq!(CachedValue::bound(mt, &t, "x", 0).handle(), Some(key));
        let native = s.upgrade(key).unwrap();
        s.set_property(mt, &native, "x".into(), 8).unwrap();
        assert_eq!(t.property_as_value("x").get(), Var::from(8));
        assert_eq!(CachedValue::<i32>::default().handle(), None);
    });
}
