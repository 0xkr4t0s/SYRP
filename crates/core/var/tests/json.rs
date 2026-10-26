//! B6: JSON.

use proptest::prelude::*;
use syrp_var::json::{self, Format, MAX_DEPTH};
use syrp_var::{Diagnostic, Identifier, Var, VarView};

/// Equality with objects compared by content (property names, order and values).
fn same(a: &Var, b: &Var) -> bool {
    match (a.as_view(), b.as_view()) {
        (VarView::Object(x), VarView::Object(y)) => {
            let (x, y) = (x.properties(), y.properties());
            x.len() == y.len()
                && x.iter()
                    .zip(&y)
                    .all(|((n, v), (m, w))| n == m && same(v, w))
        }
        (VarView::Array(x), VarView::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(v, w)| same(v, w))
        }
        _ => a == b,
    }
}

fn get(object: &Var, name: &str) -> Var {
    object
        .as_object()
        .unwrap()
        .get_property(Identifier::new(name))
}

#[test]
fn b6_parse_maps_json_types() {
    let v = json::parse(
        r#"{"n": null, "t": true, "i": -7, "big": 3000000000, "u": 18446744073709551615,
            "x": 1.5, "e": 2e3, "s": "hé", "a": [1, "2"], "o": {"k": false}}"#,
    )
    .unwrap();
    assert!(get(&v, "n").is_void());
    assert!(get(&v, "t").as_bool());
    assert!(get(&v, "i").is_i32() && get(&v, "i").as_i32() == -7);
    assert!(get(&v, "big").is_i64() && get(&v, "big").as_i64() == 3_000_000_000);
    assert!(get(&v, "u").is_f64());
    assert!(get(&v, "x").is_f64() && get(&v, "e").as_f64() == 2000.0);
    assert_eq!(get(&v, "s").as_str(), Some("hé"));
    assert_eq!(get(&v, "a").as_array().unwrap()[1].as_str(), Some("2"));
    assert!(get(&get(&v, "o"), "k").is_bool());
    let names: Vec<_> = v
        .as_object()
        .unwrap()
        .properties()
        .iter()
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(names, ["n", "t", "i", "big", "u", "x", "e", "s", "a", "o"]);
}

#[test]
fn b6_repeated_keys_keep_first_position_and_last_value() {
    let v = json::parse(r#"{"a": 1, "b": 2, "a": 3}"#).unwrap();
    assert_eq!(
        json::to_string(&v, Format::Compact).unwrap(),
        r#"{"a":3,"b":2}"#
    );
}

#[test]
fn b6_to_string_compact_and_pretty() {
    let object = Var::new_object();
    let o = object.as_object().unwrap();
    o.set_property(Identifier::new("b6-name"), "x\"y");
    o.set_property(
        Identifier::new("b6-list"),
        vec![Var::from(2.0), Var::void(), Var::undefined()],
    );
    o.set_property(Identifier::new("b6-bin"), Var::from_bytes([0u8, 255]));
    o.set_property(Identifier::new("b6-nan"), f64::NAN);
    assert_eq!(
        json::to_string(&object, Format::Compact).unwrap(),
        r#"{"b6-name":"x\"y","b6-list":[2.0,null,null],"b6-bin":[0,255],"b6-nan":null}"#
    );
    assert_eq!(
        json::to_string(
            &Var::from(vec![Var::from(1), Var::new_object()]),
            Format::Pretty
        )
        .unwrap(),
        "[\n  1,\n  {}\n]"
    );
    assert_eq!(Format::default(), Format::Compact);
}

#[test]
fn b6_errors_carry_code_line_and_column() {
    let error = json::parse("{\n  \"a\": 1,\n  \"b\": ]\n}").unwrap_err();
    assert_eq!(error.code().as_str(), "SYRP-VAR-0001");
    assert_eq!((error.line(), error.column()), (3, 8));
    for bad in ["", "[1,]", "{'a': 1}", "1 2", "nul", "\"open", "1e999"] {
        let error = json::parse(bad).unwrap_err();
        assert_eq!(error.code().as_str(), "SYRP-VAR-0001", "{bad:?}");
        assert!(error.to_string().starts_with("SYRP-VAR-0001"), "{error}");
        assert!(error.hint().is_some());
    }
}

#[test]
fn b6_deeply_nested_input_is_refused_with_a_coded_error() {
    let at_limit = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
    assert!(json::parse(&at_limit).is_ok());
    // Far past both this crate's limit and serde_json's: no stack overflow on parse or drop.
    for open in ["[", "{\"k\":"] {
        for depth in [MAX_DEPTH + 1, 200, 100_000] {
            let error = json::parse(&open.repeat(depth)).unwrap_err();
            assert_eq!(error.code().as_str(), "SYRP-VAR-0002", "{open} x {depth}");
            assert_eq!(error.line(), 1);
            assert!(error.column() > MAX_DEPTH);
        }
    }
}

#[test]
fn b6_writing_deep_or_cyclic_values_fails_with_a_coded_error() {
    let mut deep = Var::from(1);
    for _ in 0..=MAX_DEPTH {
        deep = Var::from(vec![deep]);
    }
    let error = json::to_string(&deep, Format::Compact).unwrap_err();
    assert_eq!(error.code().as_str(), "SYRP-VAR-0003");
    assert_eq!((error.line(), error.column()), (0, 0));
    let cyclic = Var::new_object();
    let name = Identifier::new("b6-self");
    cyclic
        .as_object()
        .unwrap()
        .set_property(name, vec![cyclic.clone()]);
    let error = json::to_string(&cyclic, Format::Pretty).unwrap_err();
    assert_eq!(error.code().as_str(), "SYRP-VAR-0003");
    cyclic.as_object().unwrap().remove_property(name);
}

/// `levels` empty arrays nested inside each other.
fn nested_arrays(levels: usize) -> Var {
    let mut value = Var::from(Vec::new());
    for _ in 1..levels {
        value = Var::from(vec![value]);
    }
    value
}

#[test]
fn b6_write_and_parse_agree_at_the_depth_limit() {
    let at_limit = nested_arrays(MAX_DEPTH);
    for format in [Format::Compact, Format::Pretty] {
        let text = json::to_string(&at_limit, format).unwrap();
        assert!(same(&json::parse(&text).unwrap(), &at_limit));
        // Same rule for objects: MAX_DEPTH nested objects round-trip.
        let objects = "{\"k\":".repeat(MAX_DEPTH - 1) + "{}" + &"}".repeat(MAX_DEPTH - 1);
        let parsed = json::parse(&objects).unwrap();
        assert_eq!(json::to_string(&parsed, Format::Compact).unwrap(), objects);
    }
    let past_limit = nested_arrays(MAX_DEPTH + 1);
    let error = json::to_string(&past_limit, Format::Compact).unwrap_err();
    assert_eq!(error.code().as_str(), "SYRP-VAR-0003");
    let text = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
    assert_eq!(
        json::parse(&text).unwrap_err().code().as_str(),
        "SYRP-VAR-0002"
    );
    #[cfg(feature = "serde")]
    {
        assert!(serde_json::to_string(&at_limit).is_ok());
        assert!(serde_json::to_string(&past_limit).is_err());
    }
}

/// Any `Var` without binary, undefined or non-finite numbers, nested a few levels.
fn json_var() -> impl Strategy<Value = Var> {
    let leaf = prop_oneof![
        Just(Var::void()),
        any::<bool>().prop_map(Var::from),
        any::<i32>().prop_map(Var::from),
        any::<i64>().prop_map(Var::from),
        any::<f64>()
            .prop_filter("finite", |x| x.is_finite())
            .prop_map(Var::from),
        ".{0,8}".prop_map(Var::from),
    ];
    leaf.prop_recursive(4, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Var::from),
            prop::collection::vec(("[a-e]{1,2}", inner), 0..6).prop_map(|props| {
                let object = Var::new_object();
                for (name, value) in props {
                    object
                        .as_object()
                        .unwrap()
                        .set_property(Identifier::new(&name), value);
                }
                object
            }),
        ]
    })
}

proptest! {
    #[test]
    fn b6_round_trip_preserves_value(value in json_var()) {
        for format in [Format::Compact, Format::Pretty] {
            let text = json::to_string(&value, format).unwrap();
            let back = json::parse(&text).unwrap();
            prop_assert!(same(&value, &back), "{text}");
        }
    }
}
