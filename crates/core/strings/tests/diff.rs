//! B3: TextDiff.

use proptest::prelude::*;
use syrp_strings::TextDiff;

#[test]
fn b3_apply_reproduces_the_target() {
    let cases = [
        ("", ""),
        ("", "new"),
        ("old", ""),
        ("kitten", "sitting"),
        ("the quick brown fox", "the slow brown dog"),
        ("aaaa", "aa"),
        ("ab", "ba"),
        ("héllo 🌍 wörld", "hello 🌎 world!"),
    ];
    for (original, target) in cases {
        let diff = TextDiff::new(original, target);
        assert_eq!(diff.apply(original), target, "{original:?} -> {target:?}");
    }
}

#[test]
fn b3_changes_are_insertions_or_removals_in_char_positions() {
    let diff = TextDiff::new("aé b", "aé Xb");
    assert_eq!(diff.len(), 1);
    let change = &diff.changes()[0];
    assert_eq!(
        (change.start(), change.removed_len(), change.inserted_text()),
        (3, 0, "X")
    );
    assert!(!change.is_deletion());

    let diff = TextDiff::new("aé🌍b", "ab");
    assert_eq!(diff.len(), 1);
    let change = &diff.changes()[0];
    assert_eq!(
        (change.start(), change.removed_len(), change.inserted_text()),
        (1, 2, "")
    );
    assert!(change.is_deletion());
}

#[test]
fn b3_changes_apply_one_by_one() {
    let (original, target) = ("one two three", "one 2 three four");
    let diff = TextDiff::new(original, target);
    let stepped = diff
        .changes()
        .iter()
        .fold(original.to_string(), |text, c| c.applied_to(&text));
    assert_eq!(stepped, target);
}

#[test]
fn b3_equal_texts_have_no_changes() {
    assert!(TextDiff::new("same text", "same text").is_empty());
}

#[test]
fn b3_small_edit_in_long_text_is_small() {
    let original = "x".repeat(5000) + "middle" + &"y".repeat(5000);
    let target = "x".repeat(5000) + "centre" + &"y".repeat(5000);
    let diff = TextDiff::new(&original, &target);
    assert!(diff.len() <= 6, "{:?}", diff.changes());
    assert_eq!(diff.apply(&original), target);
}

#[test]
fn b3_large_unrelated_texts_fall_back_and_stay_correct() {
    let original: String = (0..3000)
        .map(|i| char::from(b'a' + (i % 7) as u8))
        .collect();
    let target: String = (0..3000)
        .map(|i| char::from(b'h' + (i % 11) as u8))
        .collect();
    let diff = TextDiff::new(&original, &target);
    assert_eq!(diff.apply(&original), target);
}

proptest! {
    #[test]
    fn b3_apply_reproduces_any_unicode_target(original in "\\PC{0,40}", target in "\\PC{0,40}") {
        let diff = TextDiff::new(&original, &target);
        prop_assert_eq!(diff.apply(&original), target);
    }

    #[test]
    fn b3_apply_reproduces_similar_texts(original in "[ab\u{e9}\u{1F30D}]{0,60}", target in "[ab\u{e9}\u{1F30D}]{0,60}") {
        let diff = TextDiff::new(&original, &target);
        let stepped = diff.changes().iter().fold(original.clone(), |text, c| c.applied_to(&text));
        prop_assert_eq!(&stepped, &target);
        prop_assert_eq!(diff.apply(&original), target);
    }
}
