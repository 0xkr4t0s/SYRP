//! B4: sharing, copy on write and deep copies.

use std::rc::Rc;

use syrp_var::{Identifier, Var};

#[test]
fn b4_clone_shares_strings_binary_and_arrays() {
    let text = Var::from("shared");
    assert!(std::ptr::eq(
        text.as_str().unwrap(),
        text.clone().as_str().unwrap()
    ));
    let bytes = Var::from_bytes(vec![1, 2, 3]);
    assert!(std::ptr::eq(
        bytes.as_bytes().unwrap(),
        bytes.clone().as_bytes().unwrap()
    ));
    let array = Var::from(vec![Var::from(1)]);
    assert!(std::ptr::eq(
        array.as_array().unwrap(),
        array.clone().as_array().unwrap()
    ));
}

#[test]
fn b4_array_mut_copies_on_write_when_shared() {
    let mut a = Var::from(vec![Var::from(1), Var::from(2)]);
    let b = a.clone();
    a.array_mut().unwrap()[0] = Var::from("changed");
    assert_eq!(a.as_array().unwrap()[0].as_str(), Some("changed"));
    assert_eq!(b.as_array().unwrap()[0].as_i32(), 1);
    assert_eq!(b, Var::from(vec![Var::from(1), Var::from(2)]));
}

#[test]
fn b4_array_mut_does_not_copy_when_unique() {
    let mut a = Var::from(vec![Var::from(1)]);
    let before: *const [Var] = a.as_array().unwrap();
    a.array_mut().unwrap()[0] = Var::from(5);
    assert!(std::ptr::eq(before, a.as_array().unwrap()));
    assert!(Var::from("not an array").array_mut().is_none());
}

#[test]
fn b4_objects_are_shared_by_reference() {
    let a = Var::new_object();
    let b = a.clone();
    assert!(Rc::ptr_eq(a.as_object().unwrap(), b.as_object().unwrap()));
    b.as_object()
        .unwrap()
        .set_property(Identifier::new("b4-x"), 1);
    assert!(a.as_object().unwrap().has_property(Identifier::new("b4-x")));
    // An object inside a copied-on-write array is still the same object.
    let mut list = Var::from(vec![a.clone()]);
    let other = list.clone();
    list.array_mut().unwrap().push(Var::from(0));
    assert_eq!(list.as_array().unwrap()[0], other.as_array().unwrap()[0]);
}

#[test]
fn b4_deep_clone_copies_objects_and_arrays_recursively() {
    let name = Identifier::new("b4-inner");
    let inner = Var::new_object();
    inner.as_object().unwrap().set_property(name, 1);
    let original = Var::from(vec![inner.clone(), Var::from("s")]);
    let copy = original.deep_clone();
    let copied_inner = &copy.as_array().unwrap()[0];
    assert_ne!(copied_inner, &inner, "a new object");
    assert_eq!(
        copied_inner
            .as_object()
            .unwrap()
            .get_property(name)
            .as_i32(),
        1
    );
    copied_inner.as_object().unwrap().set_property(name, 2);
    assert_eq!(inner.as_object().unwrap().get_property(name).as_i32(), 1);
    assert!(!std::ptr::eq(
        original.as_array().unwrap(),
        copy.as_array().unwrap()
    ));
    assert!(std::ptr::eq(
        original.as_array().unwrap()[1].as_str().unwrap(),
        copy.as_array().unwrap()[1].as_str().unwrap()
    ));
}

#[test]
fn b4_deep_clone_keeps_sharing_and_cycles() {
    let (me, a, b) = (
        Identifier::new("b4-self"),
        Identifier::new("b4-a"),
        Identifier::new("b4-b"),
    );
    let shared = Var::new_object();
    let root = Var::new_object();
    let object = root.as_object().unwrap();
    object.set_property(a, shared.clone());
    object.set_property(b, shared.clone());
    object.set_property(me, root.clone());
    let copy = root.deep_clone();
    let copied = copy.as_object().unwrap();
    assert_ne!(copy, root);
    assert_eq!(copied.get_property(me), copy, "cycle points at the copy");
    assert_eq!(copied.get_property(a), copied.get_property(b));
    assert_ne!(copied.get_property(a), shared);
    // Break the cycles so the test does not leak.
    object.remove_property(me);
    copied.remove_property(me);
}
