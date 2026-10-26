//! B5: NamedValueSet and DynamicObject.

use syrp_var::{DynamicObject, Identifier, NamedValueSet, Var};

fn ids() -> [Identifier; 3] {
    ["b5-a", "b5-b", "b5-c"].map(Identifier::new)
}

#[test]
fn b5_set_get_contains_len() {
    let [a, b, c] = ids();
    let mut set = NamedValueSet::new();
    assert!(set.is_empty());
    assert!(set.set(a, 1));
    assert!(set.set(b, "two"));
    assert_eq!(set.len(), 2);
    assert!(set.contains(a) && set.contains(b) && !set.contains(c));
    assert_eq!(set.get(a).unwrap().as_i32(), 1);
    assert_eq!(set.get(b).unwrap().as_str(), Some("two"));
    assert!(set.get(c).is_none());
}

#[test]
fn b5_set_returns_whether_the_value_changed() {
    let [a, ..] = ids();
    let mut set = NamedValueSet::new();
    assert!(set.set(a, 1), "added");
    assert!(!set.set(a, 1), "same value");
    assert!(set.set(a, 1.0), "same number, different variant");
    assert!(set.get(a).unwrap().is_f64());
    assert!(set.set(a, "1"));
    assert!(!set.set(a, String::from("1")));
    assert_eq!(set.len(), 1);
}

#[test]
fn b5_iteration_is_in_insertion_order_and_survives_replace_and_remove() {
    let [a, b, c] = ids();
    let mut set = NamedValueSet::new();
    set.set(c, 3);
    set.set(a, 1);
    set.set(b, 2);
    set.set(a, 10);
    let order: Vec<_> = set.iter().map(|(n, v)| (n, v.as_i32())).collect();
    assert_eq!(order, [(c, 3), (a, 10), (b, 2)]);
    assert_eq!(set.iter().len(), 3);
    assert_eq!(set.remove(a).unwrap().as_i32(), 10);
    assert!(set.remove(a).is_none());
    let names: Vec<_> = (&set).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, [c, b]);
}

#[test]
fn b5_dynamic_object_properties() {
    let [a, b, c] = ids();
    let object = DynamicObject::new();
    assert!(!object.has_property(a));
    assert!(object.get_property(a).is_void());
    assert!(object.set_property(a, true));
    assert!(!object.set_property(a, true));
    assert!(object.set_property(b, Var::from(vec![Var::from(1)])));
    assert!(object.has_property(a));
    assert_eq!(object.property_count(), 2);
    assert!(object.get_property(a).as_bool());
    assert!(object.remove_property(a).unwrap().as_bool());
    assert!(object.remove_property(c).is_none());
    let names: Vec<_> = object.properties().iter().map(|(n, _)| n).collect();
    assert_eq!(names, [b]);
}

#[test]
fn b5_object_values_can_hold_the_object_itself() {
    let me = Identifier::new("b5-self");
    let value = Var::new_object();
    let object = value.as_object().unwrap();
    object.set_property(me, value.clone());
    assert_eq!(object.get_property(me), value);
    assert!(format!("{value:?}").contains("DynamicObject"));
    // Replacing the property while it refers to the object is fine.
    assert!(object.set_property(me, Var::void()));
}
