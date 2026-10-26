//! B4: LocalisedStrings.

use syrp_error::Diagnostic;
use syrp_strings::{LocalisedStrings, StringsErrorKind};

const FRENCH: &str = "\u{feff}// French translations
language: French
countries:  fr be mc ch lu

# a hash comment
\"Hello\" = \"Bonjour\"
  \"Goodbye\"=\"Au revoir\"   // trailing comment
\"Say \\\"hi\\\"\" = \"Dites \\\"salut\\\"\"
\"Back\\\\slash\\n\" = \"Barre\\toblique\"
\"Hello\" = \"Salut\"
";

#[test]
fn b4_parses_header_mappings_comments_and_escapes() {
    let fr = LocalisedStrings::parse(FRENCH, false).unwrap();
    assert_eq!(fr.language_name(), "French");
    assert_eq!(fr.country_codes(), ["fr", "be", "mc", "ch", "lu"]);
    assert_eq!(fr.len(), 4);
    assert_eq!(fr.translate("Hello"), "Salut", "the later duplicate wins");
    assert_eq!(fr.translate("Goodbye"), "Au revoir");
    assert_eq!(fr.translate("Say \"hi\""), "Dites \"salut\"");
    assert_eq!(fr.translate("Back\\slash\n"), "Barre\toblique");
}

#[test]
fn b4_translate_returns_input_when_unmapped() {
    let fr = LocalisedStrings::parse(FRENCH, false).unwrap();
    let text = String::from("Not in the file");
    assert!(std::ptr::eq(fr.translate(&text), text.as_str()));
    assert_eq!(fr.translate("hello"), "hello", "case matters by default");
    assert!(LocalisedStrings::default().translate("x") == "x");
}

#[test]
fn b4_ignore_case_matches_any_case() {
    let fr = LocalisedStrings::parse(FRENCH, true).unwrap();
    assert!(fr.ignores_case());
    assert_eq!(fr.translate("HELLO"), "Salut");
    assert_eq!(fr.translate("goodBYE"), "Au revoir");
    let el = LocalisedStrings::parse("\"ΣΟΦΙΑ\" = \"wisdom\"", true).unwrap();
    assert_eq!(el.translate("σοφια"), "wisdom");
}

#[test]
fn b4_fallback_is_consulted_for_unmapped_text() {
    let base =
        LocalisedStrings::parse("\"Open\" = \"Ouvrir\"\n\"Email\" = \"E-mail\"", false).unwrap();
    let root = LocalisedStrings::parse("\"Quit\" = \"Quitter\"", true).unwrap();
    let fr_ca = LocalisedStrings::parse("\"Email\" = \"Courriel\"", false)
        .unwrap()
        .with_fallback(base.with_fallback(root));
    assert_eq!(fr_ca.translate("Email"), "Courriel");
    assert_eq!(fr_ca.translate("Open"), "Ouvrir");
    assert_eq!(
        fr_ca.translate("QUIT"),
        "Quitter",
        "each level applies its own ignore_case"
    );
    assert_eq!(fr_ca.translate("Save"), "Save");
    assert_eq!(fr_ca.len(), 1);
    assert_eq!(fr_ca.fallback().unwrap().len(), 2);
}

#[test]
fn b4_merge_adds_and_overrides_mappings() {
    let mut a = LocalisedStrings::parse(
        "language: French\ncountries: fr\n\"a\" = \"1\"\n\"b\" = \"2\"",
        false,
    )
    .unwrap();
    let b = LocalisedStrings::parse(
        "language: FRENCH\ncountries: fr be\n\"b\" = \"two\"\n\"c\" = \"3\"",
        false,
    )
    .unwrap();
    a.merge(&b).unwrap();
    assert_eq!(
        (a.translate("a"), a.translate("b"), a.translate("c")),
        ("1", "two", "3")
    );
    assert_eq!(a.country_codes(), ["fr", "be"]);
    assert_eq!(a.language_name(), "French");

    let mut unnamed = LocalisedStrings::default();
    unnamed.merge(&b).unwrap();
    assert_eq!(unnamed.language_name(), "FRENCH");
}

#[test]
fn b4_merge_rejects_a_different_language() {
    let mut fr = LocalisedStrings::parse("language: French\n\"a\" = \"1\"", false).unwrap();
    let de = LocalisedStrings::parse("language: German\n\"a\" = \"eins\"", false).unwrap();
    let before = fr.clone();
    let err = fr.merge(&de).unwrap_err();
    assert_eq!(err.kind(), StringsErrorKind::LanguageMismatch);
    assert_eq!(err.code().as_str(), "SYRP-STRINGS-0005");
    assert_eq!(fr, before);
}

#[test]
fn b4_malformed_lines_report_their_line_number() {
    let cases = [
        ("language: X\nnot a mapping", 2, 0),
        ("\"a\" \"b\"", 1, 4),
        ("\"a\" = b", 1, 6),
        ("\"a\" = \"b", 1, 8),
        ("\n\n  \"unterminated = \"x\"", 3, 19),
        ("\"a\" = \"b\" extra", 1, 10),
        ("\"bad \\q escape\" = \"x\"", 1, 5),
        ("\"a\" =", 1, 5),
    ];
    for (text, line, at) in cases {
        let err = LocalisedStrings::parse(text, false).unwrap_err();
        assert_eq!(err.kind(), StringsErrorKind::TranslationSyntax, "{text:?}");
        assert_eq!(err.code().as_str(), "SYRP-STRINGS-0004");
        assert_eq!(
            (err.line(), err.position()),
            (Some(line), Some(at)),
            "{text:?}"
        );
    }
    let message = LocalisedStrings::parse("\n?", false)
        .unwrap_err()
        .to_string();
    assert_eq!(message, "malformed translation line at line 2, byte 0");
}

#[test]
fn b4_crlf_line_endings_are_accepted() {
    let s = LocalisedStrings::parse("language: Polish\r\n\"Yes\" = \"Tak\"\r\n", false).unwrap();
    assert_eq!((s.language_name(), s.translate("Yes")), ("Polish", "Tak"));
}
