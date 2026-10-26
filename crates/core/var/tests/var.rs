//! B2 and B2a: the Var value type and thread auto traits.

use std::rc::Rc;

use syrp_var::{DynamicObject, Identifier, Var, VarView};

#[test]
fn b2_var_is_sixteen_bytes() {
    #[cfg(target_pointer_width = "64")]
    assert_eq!(size_of::<Var>(), 16);
}

#[test]
fn b2_every_variant_constructs_and_reports_its_type() {
    let object = Rc::new(DynamicObject::new());
    let cases: Vec<(Var, &str)> = vec![
        (Var::void(), "void"),
        (Var::default(), "void"),
        (Var::undefined(), "undefined"),
        (Var::from(true), "bool"),
        (Var::from(-5_i32), "i32"),
        (Var::from(1_i64 << 40), "i64"),
        (Var::from(0.25), "f64"),
        (Var::from("s"), "string"),
        (Var::from(String::from("s")), "string"),
        (Var::from(vec![Var::from(1)]), "array"),
        (Var::from(Rc::clone(&object)), "object"),
        (Var::new_object(), "object"),
        (Var::from_bytes(vec![1, 2]), "binary"),
    ];
    for (value, kind) in cases {
        let flags = [
            ("void", value.is_void()),
            ("undefined", value.is_undefined()),
            ("bool", value.is_bool()),
            ("i32", value.is_i32()),
            ("i64", value.is_i64()),
            ("f64", value.is_f64()),
            ("string", value.is_string()),
            ("array", value.is_array()),
            ("object", value.is_object()),
            ("binary", value.is_binary()),
        ];
        for (name, set) in flags {
            assert_eq!(set, name == kind, "{value:?}: is_{name}");
        }
    }
}

#[test]
fn b2_accessors_borrow_without_coercion() {
    assert_eq!(Var::from("abc").as_str(), Some("abc"));
    assert_eq!(Var::from(1).as_str(), None);
    let array = Var::from(vec![Var::from(1), Var::from("x")]);
    assert_eq!(array.as_array().unwrap()[1].as_str(), Some("x"));
    assert!(Var::from("x").as_array().is_none());
    assert_eq!(Var::from_bytes(*b"raw").as_bytes(), Some(&b"raw"[..]));
    assert!(Var::from("raw").as_bytes().is_none());
    let object = Rc::new(DynamicObject::new());
    let value = Var::from(Rc::clone(&object));
    assert!(Rc::ptr_eq(value.as_object().unwrap(), &object));
    assert!(Var::from(1).as_object().is_none());
}

#[test]
fn b2_view_matches_the_variant() {
    let value = Var::from(vec![Var::from(1_i64), Var::from(2.5)]);
    // The view is Copy and its borrows outlive the match.
    let items: &[Var] = match value.as_view() {
        VarView::Array(items) => items,
        other => panic!("unexpected {other:?}"),
    };
    assert!(matches!(items[0].as_view(), VarView::Int64(1)));
    assert!(matches!(items[1].as_view(), VarView::Double(x) if x == 2.5));
    assert!(matches!(Var::void().as_view(), VarView::Void));
    assert!(matches!(Var::undefined().as_view(), VarView::Undefined));
    assert!(matches!(Var::from("t").as_view(), VarView::Str("t")));
    assert!(matches!(
        Var::from_bytes([9]).as_view(),
        VarView::Binary([9])
    ));
    assert!(matches!(Var::new_object().as_view(), VarView::Object(_)));
}

#[test]
fn b2_heap_variants_share_through_rc_on_clone() {
    let text = Var::from("shared");
    let copy = text.clone();
    assert!(std::ptr::eq(text.as_str().unwrap(), copy.as_str().unwrap()));
    let array = Var::from(vec![Var::from(1)]);
    let copy = array.clone();
    assert!(std::ptr::eq(
        array.as_array().unwrap(),
        copy.as_array().unwrap()
    ));
}

#[test]
fn b2_debug_does_not_recurse_into_objects() {
    let text = format!("{:?}", Var::from(vec![Var::new_object(), Var::from(1)]));
    assert!(text.contains("DynamicObject"), "{text}");
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn b2a_identifier_is_send_and_sync() {
    assert_send_sync::<Identifier>();
    // Var, NamedValueSet and DynamicObject are proven !Send and !Sync by the compile_fail
    // doctests in the crate documentation ("Threads").
}
