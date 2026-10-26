//! B5: process-wide current mappings and `translate`.
//!
//! The mappings are process-wide, so every check lives in one test function.

use std::borrow::Cow;
use std::sync::Arc;
use std::thread;

use syrp_strings::{LocalisedStrings, current_mappings, set_current_mappings, translate};

#[test]
fn b5_current_mappings_are_shared_by_all_threads() {
    assert!(current_mappings().is_none());
    assert!(matches!(translate("Play"), Cow::Borrowed("Play")));

    let it = LocalisedStrings::parse("language: Italian\n\"Play\" = \"Riproduci\"", false).unwrap();
    set_current_mappings(Some(it));
    assert_eq!(translate("Play"), "Riproduci");
    assert!(matches!(translate("Stop"), Cow::Borrowed("Stop")));

    let held = current_mappings().unwrap();
    let results: Vec<String> = (0..8)
        .map(|_| thread::spawn(|| translate("Play").into_owned()))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();
    assert!(results.iter().all(|r| r == "Riproduci"));

    // Readers and a writer racing must never see a torn state.
    let readers: Vec<_> = (0..4)
        .map(|_| {
            thread::spawn(|| {
                for _ in 0..1000 {
                    let out = translate("Play");
                    assert!(
                        out == "Play" || out == "Riproduci" || out == "Abspielen",
                        "{out}"
                    );
                }
            })
        })
        .collect();
    for i in 0..100 {
        let next = (i % 2 == 0)
            .then(|| LocalisedStrings::parse("\"Play\" = \"Abspielen\"", false).unwrap());
        set_current_mappings(next);
    }
    readers.into_iter().for_each(|h| h.join().unwrap());

    set_current_mappings(None);
    assert_eq!(translate("Play"), "Play");
    assert_eq!(
        held.translate("Play"),
        "Riproduci",
        "a held set outlives replacement"
    );
    assert_eq!(Arc::strong_count(&held), 1);
}
