//! B1: interned identifiers.

use std::collections::HashSet;
use std::hash::{BuildHasher, RandomState};

use syrp_var::Identifier;

#[test]
fn b1_identifier_is_four_bytes_and_copy() {
    assert_eq!(size_of::<Identifier>(), 4);
    let a = Identifier::new("b1-copy");
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn b1_equal_strings_give_equal_identifiers() {
    let owned = String::from("b1-equal");
    assert_eq!(Identifier::new("b1-equal"), Identifier::new(&owned));
    assert_ne!(Identifier::new("b1-equal"), Identifier::new("b1-Equal"));
    assert_eq!(Identifier::new("b1-equal").as_str(), "b1-equal");
}

#[test]
fn b1_equal_strings_give_equal_identifiers_across_threads() {
    let names: Vec<String> = (0..200).map(|i| format!("b1-thread-{i}")).collect();
    let results: Vec<Vec<Identifier>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|t| {
                let names = &names;
                scope.spawn(move || {
                    // Each thread interns in a different order to race on first creation.
                    let mut ids = vec![Identifier::EMPTY; names.len()];
                    for k in 0..names.len() {
                        let i = (k * 7 + t * 31) % names.len();
                        ids[i] = Identifier::new(&names[i]);
                    }
                    ids
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for ids in &results[1..] {
        assert_eq!(ids, &results[0]);
    }
    let distinct: HashSet<_> = results[0].iter().collect();
    assert_eq!(distinct.len(), names.len());
    for (id, name) in results[0].iter().zip(&names) {
        assert_eq!(id.as_str(), name);
    }
}

#[test]
fn b1_as_str_is_static() {
    let text: &'static str = {
        let temporary = String::from("b1-static");
        Identifier::new(&temporary).as_str()
    };
    assert_eq!(text, "b1-static");
}

#[test]
fn b1_hash_and_equality_follow_the_handle() {
    let a = Identifier::new("b1-hash");
    let state = RandomState::new();
    assert_eq!(state.hash_one(a), state.hash_one(a.as_u32()));
}

#[test]
fn b1_ord_is_interning_order_and_cmp_text_is_lexical() {
    let first = Identifier::new("b1-ord-zzz");
    let second = Identifier::new("b1-ord-aaa");
    assert!(first < second, "earlier interned sorts first");
    assert!(first.cmp_text(second).is_gt(), "lexical order differs");
    assert!(Identifier::EMPTY < first);
}

#[test]
fn b1_empty_and_non_ascii_names_are_allowed() {
    assert_eq!(Identifier::new(""), Identifier::EMPTY);
    assert_eq!(Identifier::new("").as_str(), "");
    assert_eq!(Identifier::default(), Identifier::EMPTY);
    let name = "b1-ünïcödé-音";
    assert_eq!(Identifier::new(name).as_str(), name);
    assert!(!Identifier::new(name).is_empty());
}

#[test]
fn b1_many_identifiers_span_table_chunks() {
    let ids: Vec<_> = (0..5_000)
        .map(|i| Identifier::new(&format!("b1-many-{i}")))
        .collect();
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(id.as_str(), format!("b1-many-{i}"));
    }
}
