//! B3: conversions and comparison.

use syrp_var::Var;

#[test]
fn b3_void_and_undefined_convert_to_zero_or_empty() {
    for v in [Var::void(), Var::undefined()] {
        assert!(!v.as_bool());
        assert_eq!(v.as_i32(), 0);
        assert_eq!(v.as_i64(), 0);
        assert_eq!(v.as_f64(), 0.0);
        assert_eq!(v.to_text(), "");
    }
}

#[test]
fn b3_booleans_from_numbers_are_non_zero() {
    assert!(Var::from(1).as_bool());
    assert!(Var::from(-1_i64).as_bool());
    assert!(Var::from(0.001).as_bool());
    assert!(Var::from(f64::NAN).as_bool());
    assert!(!Var::from(0).as_bool());
    assert!(!Var::from(0_i64).as_bool());
    assert!(!Var::from(-0.0).as_bool());
}

#[test]
fn b3_bools_convert_to_one_or_zero() {
    let (t, f) = (Var::from(true), Var::from(false));
    assert_eq!((t.as_i32(), t.as_i64(), t.as_f64()), (1, 1, 1.0));
    assert_eq!((f.as_i32(), f.as_i64(), f.as_f64()), (0, 0, 0.0));
    assert_eq!((t.to_text(), f.to_text()), ("1".to_owned(), "0".to_owned()));
}

#[test]
fn b3_numbers_from_strings_parse_a_leading_number() {
    assert_eq!(Var::from("  42abc").as_i32(), 42);
    assert_eq!(Var::from("-7.9").as_i32(), -7);
    assert_eq!(Var::from("+3").as_i64(), 3);
    assert_eq!(Var::from("x42").as_i32(), 0);
    assert_eq!(Var::from("").as_f64(), 0.0);
    assert_eq!(Var::from("\t1.5e3 Hz").as_f64(), 1500.0);
    assert_eq!(Var::from("-.25").as_f64(), -0.25);
    assert_eq!(Var::from("99999999999").as_i32(), i32::MAX);
    assert_eq!(Var::from("99999999999").as_i64(), 99_999_999_999);
}

#[test]
fn b3_booleans_from_strings() {
    assert!(Var::from("true").as_bool());
    assert!(Var::from(" True\n").as_bool());
    assert!(Var::from("0.5").as_bool());
    assert!(!Var::from("false").as_bool());
    assert!(!Var::from("0").as_bool());
    assert!(!Var::from("yes").as_bool());
}

#[test]
fn b3_float_and_wide_integer_narrowing() {
    assert_eq!(Var::from(2.99).as_i32(), 2);
    assert_eq!(Var::from(-2.99).as_i64(), -2);
    assert_eq!(Var::from(1e300).as_i32(), i32::MAX);
    assert_eq!(Var::from(f64::NAN).as_i32(), 0);
    assert_eq!(Var::from(i64::MIN).as_i32(), i32::MIN);
    assert_eq!(Var::from(1_i64 << 53).as_f64(), 9_007_199_254_740_992.0);
}

#[test]
fn b3_to_text() {
    assert_eq!(Var::from(-12).to_text(), "-12");
    assert_eq!(Var::from(i64::MAX).to_text(), "9223372036854775807");
    assert_eq!(Var::from(0.1).to_text(), "0.1");
    assert_eq!(Var::from(2.0).to_text(), "2");
    assert_eq!(Var::from(f64::INFINITY).to_text(), "inf");
    assert_eq!(Var::from("as is").to_text(), "as is");
    assert_eq!(Var::from(0.1).to_string(), "0.1");
}

#[test]
fn b3_containers_convert_to_zero_or_empty() {
    for v in [
        Var::from(vec![Var::from(5)]),
        Var::new_object(),
        Var::from_bytes([1]),
    ] {
        assert!(!v.as_bool());
        assert_eq!((v.as_i32(), v.as_i64(), v.as_f64()), (0, 0, 0.0));
        assert_eq!(v.to_text(), "");
    }
}

#[test]
fn b3_strict_equality_compares_numbers_numerically() {
    assert_eq!(Var::from(3), Var::from(3_i64));
    assert_eq!(Var::from(3), Var::from(3.0));
    assert_eq!(Var::from(1_i64 << 62), Var::from((1_i64 << 62) as f64));
    assert_ne!(Var::from(i64::MAX), Var::from(i64::MAX as f64));
    assert_ne!(Var::from(3), Var::from(3.5));
    assert_ne!(Var::from(f64::NAN), Var::from(f64::NAN));
}

#[test]
fn b3_strict_equality_needs_the_same_variant_otherwise() {
    assert_ne!(Var::from(1), Var::from(true));
    assert_ne!(Var::from("1"), Var::from(1));
    assert_ne!(Var::void(), Var::undefined());
    assert_ne!(Var::void(), Var::from(""));
    assert_eq!(Var::void(), Var::void());
    assert_eq!(Var::from("a"), Var::from(String::from("a")));
    assert_eq!(Var::from_bytes([1, 2]), Var::from_bytes(vec![1, 2]));
    assert_eq!(
        Var::from(vec![Var::from(1), Var::from("x")]),
        Var::from(vec![Var::from(1.0), Var::from("x")])
    );
    assert_ne!(
        Var::from(vec![Var::from(1)]),
        Var::from(vec![Var::from(true)])
    );
}

#[test]
fn b3_objects_compare_by_identity() {
    let a = Var::new_object();
    assert_eq!(a, a.clone());
    assert_ne!(a, Var::new_object());
    assert!(a.loose_eq(&a.clone()));
    assert!(!a.loose_eq(&Var::new_object()));
}

#[test]
fn b3_loose_equality_coerces_scalars() {
    assert!(Var::from("42").loose_eq(&Var::from(42)));
    assert!(Var::from(42_i64).loose_eq(&Var::from("42")));
    assert!(Var::from("0.5").loose_eq(&Var::from(0.5)));
    assert!(!Var::from("42.0").loose_eq(&Var::from(42)));
    assert!(Var::from(true).loose_eq(&Var::from(1)));
    assert!(Var::from(true).loose_eq(&Var::from(1.0)));
    assert!(Var::from(false).loose_eq(&Var::from(0_i64)));
    assert!(Var::from(true).loose_eq(&Var::from("1")));
    assert!(!Var::from(true).loose_eq(&Var::from("true")));
    assert!(Var::from(2).loose_eq(&Var::from(2.0)));
}

#[test]
fn b3_loose_equality_keeps_void_undefined_and_containers_strict() {
    assert!(Var::void().loose_eq(&Var::void()));
    assert!(!Var::void().loose_eq(&Var::from("")));
    assert!(!Var::void().loose_eq(&Var::from(0)));
    assert!(!Var::undefined().loose_eq(&Var::void()));
    assert!(!Var::from(vec![Var::from(1)]).loose_eq(&Var::from(vec![Var::from("1")])));
    assert!(!Var::from_bytes([]).loose_eq(&Var::from("")));
}
