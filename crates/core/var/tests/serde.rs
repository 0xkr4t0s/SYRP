//! B7: serde impls (default-on `serde` feature).
#![cfg(feature = "serde")]

use serde::{Deserialize, Serialize};
use syrp_var::{Identifier, Var, json};

#[derive(Serialize, Deserialize)]
struct Preset {
    name: String,
    state: Var,
}

#[test]
fn b7_serde_matches_the_json_mapping() {
    let text = r#"{"a":[1,3000000000,2.5,"s",null,true],"o":{"k":{}}}"#;
    let value = json::parse(text).unwrap();
    assert_eq!(serde_json::to_string(&value).unwrap(), text);
    let back: Var = serde_json::from_str(text).unwrap();
    assert_eq!(json::to_string(&back, json::Format::Compact).unwrap(), text);
    let items = back.as_object().unwrap().get_property(Identifier::new("a"));
    let items = items.as_array().unwrap();
    assert!(items[0].is_i32() && items[1].is_i64() && items[2].is_f64() && items[4].is_void());
}

#[test]
fn b7_var_works_inside_derived_types_and_other_deserializers() {
    let preset = Preset {
        name: "init".into(),
        state: json::parse(r#"{"gain":0.5}"#).unwrap(),
    };
    let tree = serde_json::to_value(&preset).unwrap();
    assert_eq!(tree["state"]["gain"], 0.5);
    let back: Preset = serde_json::from_value(tree).unwrap();
    assert_eq!(back.name, "init");
    let gain = back
        .state
        .as_object()
        .unwrap()
        .get_property(Identifier::new("gain"));
    assert_eq!(gain.as_f64(), 0.5);
}

#[test]
fn b7_deserialize_limits_depth_and_serialize_refuses_cycles() {
    let deep = "[".repeat(1000) + &"]".repeat(1000);
    let error = serde_json::from_str::<Var>(&deep).unwrap_err();
    assert!(error.to_string().contains("levels deep"), "{error}");
    let cyclic = Var::new_object();
    let name = Identifier::new("b7-self");
    cyclic
        .as_object()
        .unwrap()
        .set_property(name, cyclic.clone());
    assert!(serde_json::to_string(&cyclic).is_err());
    cyclic.as_object().unwrap().remove_property(name);
}
