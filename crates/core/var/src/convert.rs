//! Coercing conversions and the two equality relations of [`Var`]. The rules are tabled in
//! the crate documentation.

use std::rc::Rc;

use crate::Var;
use crate::var::Repr;

/// Parses the leading integer of `text` (whitespace, optional sign, decimal digits),
/// saturating at the `i64` range. Text with no leading digits gives 0.
fn leading_i64(text: &str) -> i64 {
    let text = text.trim_start();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let mut value: i64 = 0;
    for byte in digits.bytes().take_while(u8::is_ascii_digit) {
        let digit = i64::from(byte - b'0');
        value = if negative {
            value.saturating_mul(10).saturating_sub(digit)
        } else {
            value.saturating_mul(10).saturating_add(digit)
        };
    }
    value
}

/// Parses the leading decimal number of `text`: whitespace, optional sign, digits with an
/// optional fraction, then an optional exponent. Text with no leading digits gives 0.
fn leading_f64(text: &str) -> f64 {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let digits_from = |mut i: usize| {
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        i
    };
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let int_end = digits_from(end);
    let mut mantissa_digits = int_end - end;
    end = int_end;
    if bytes.get(end) == Some(&b'.') {
        let frac_end = digits_from(end + 1);
        mantissa_digits += frac_end - end - 1;
        end = frac_end;
    }
    if mantissa_digits == 0 {
        return 0.0;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exp_end = digits_from(end + 1 + sign);
        if exp_end > end + 1 + sign {
            end = exp_end;
        }
    }
    text[..end].parse().unwrap_or(0.0)
}

/// Converts `i64` to `i32`, clamping to the `i32` range.
fn saturate_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

/// Text to `bool` by the conversion table (shared with XML attributes).
pub(crate) fn text_as_bool(text: &str) -> bool {
    text.trim().eq_ignore_ascii_case("true") || leading_f64(text) != 0.0
}

/// Text to `i32` by the conversion table (shared with XML attributes).
pub(crate) fn text_as_i32(text: &str) -> i32 {
    saturate_i32(leading_i64(text))
}

/// Text to `f64` by the conversion table (shared with XML attributes).
pub(crate) fn text_as_f64(text: &str) -> f64 {
    leading_f64(text)
}

/// Returns whether integer `i` and float `x` hold exactly the same number.
fn int_eq_float(i: i64, x: f64) -> bool {
    // 2^63 as f64; every integral f64 in [-2^63, 2^63) converts to i64 exactly.
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    x.fract() == 0.0 && (-LIMIT..LIMIT).contains(&x) && x as i64 == i
}

impl Var {
    /// Converts to `bool`; see the conversion table in the crate documentation.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert!(Var::from(-3).as_bool());
    /// assert!(!Var::from(0.0).as_bool());
    /// assert!(Var::from(" TRUE ").as_bool());
    /// assert!(Var::from("2 apples").as_bool());
    /// assert!(!Var::void().as_bool());
    /// ```
    #[doc(alias = "operator bool")]
    #[must_use]
    pub fn as_bool(&self) -> bool {
        match &self.repr {
            Repr::Bool(b) => *b,
            Repr::Int(n) => *n != 0,
            Repr::Int64(n) => *n != 0,
            Repr::Double(x) => *x != 0.0,
            Repr::Str(s) => text_as_bool(s),
            Repr::Void | Repr::Undefined | Repr::Array(_) | Repr::Object(_) | Repr::Binary(_) => {
                false
            }
        }
    }

    /// Converts to `i32`; see the conversion table in the crate documentation.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert_eq!(Var::from(2.9).as_i32(), 2);
    /// assert_eq!(Var::from(" -12px").as_i32(), -12);
    /// assert_eq!(Var::from(i64::MAX).as_i32(), i32::MAX);
    /// assert_eq!(Var::from(true).as_i32(), 1);
    /// ```
    #[doc(alias = "operator int")]
    #[must_use]
    pub fn as_i32(&self) -> i32 {
        match &self.repr {
            Repr::Int(n) => *n,
            Repr::Double(x) => *x as i32,
            _ => saturate_i32(self.as_i64()),
        }
    }

    /// Converts to `i64`; see the conversion table in the crate documentation.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert_eq!(Var::from("123456789012 samples").as_i64(), 123_456_789_012);
    /// assert_eq!(Var::from(-1.5).as_i64(), -1);
    /// assert_eq!(Var::from(f64::NAN).as_i64(), 0);
    /// ```
    #[doc(alias = "operator int64")]
    #[must_use]
    pub fn as_i64(&self) -> i64 {
        match &self.repr {
            Repr::Bool(b) => i64::from(*b),
            Repr::Int(n) => i64::from(*n),
            Repr::Int64(n) => *n,
            Repr::Double(x) => *x as i64,
            Repr::Str(s) => leading_i64(s),
            Repr::Void | Repr::Undefined | Repr::Array(_) | Repr::Object(_) | Repr::Binary(_) => 0,
        }
    }

    /// Converts to `f64`; see the conversion table in the crate documentation.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert_eq!(Var::from("  -2.5e2dB").as_f64(), -250.0);
    /// assert_eq!(Var::from("abc").as_f64(), 0.0);
    /// assert_eq!(Var::from(7_i64).as_f64(), 7.0);
    /// ```
    #[doc(alias = "operator double")]
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        match &self.repr {
            Repr::Bool(b) => f64::from(u8::from(*b)),
            Repr::Int(n) => f64::from(*n),
            Repr::Int64(n) => *n as f64,
            Repr::Double(x) => *x,
            Repr::Str(s) => text_as_f64(s),
            Repr::Void | Repr::Undefined | Repr::Array(_) | Repr::Object(_) | Repr::Binary(_) => {
                0.0
            }
        }
    }

    /// Converts to text; see the conversion table in the crate documentation.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert_eq!(Var::from(true).to_text(), "1");
    /// assert_eq!(Var::from(0.1).to_text(), "0.1");
    /// assert_eq!(Var::from(3.0).to_text(), "3");
    /// assert_eq!(Var::void().to_text(), "");
    /// ```
    #[doc(alias = "toString")]
    #[must_use]
    pub fn to_text(&self) -> String {
        match &self.repr {
            Repr::Bool(b) => if *b { "1" } else { "0" }.to_owned(),
            Repr::Int(n) => n.to_string(),
            Repr::Int64(n) => n.to_string(),
            Repr::Double(x) => x.to_string(),
            Repr::Str(s) => String::clone(s),
            Repr::Void | Repr::Undefined | Repr::Array(_) | Repr::Object(_) | Repr::Binary(_) => {
                String::new()
            }
        }
    }

    /// Returns whether both values have the same variant and are equal (`==`), so `2` and
    /// `2.0` differ here. [`NamedValueSet::set`](crate::NamedValueSet::set) uses it to decide
    /// whether a value changed.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert!(Var::from(2).eq_same_type(&Var::from(2)));
    /// assert!(!Var::from(2).eq_same_type(&Var::from(2.0)));
    /// ```
    #[doc(alias = "equalsWithSameType")]
    #[must_use]
    pub fn eq_same_type(&self, other: &Var) -> bool {
        std::mem::discriminant(&self.repr) == std::mem::discriminant(&other.repr) && self == other
    }

    /// Compares after coercion; see "Equality" in the crate documentation.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert!(Var::from("42").loose_eq(&Var::from(42)));
    /// assert!(Var::from(true).loose_eq(&Var::from(1.0)));
    /// assert!(!Var::from("42.0").loose_eq(&Var::from(42)));
    /// assert!(!Var::void().loose_eq(&Var::from("")));
    /// ```
    #[doc(alias = "equals")]
    #[must_use]
    pub fn loose_eq(&self, other: &Var) -> bool {
        let scalar = |v: &Var| {
            matches!(
                v.repr,
                Repr::Bool(_) | Repr::Str(_) | Repr::Int(_) | Repr::Int64(_) | Repr::Double(_)
            )
        };
        if !scalar(self) || !scalar(other) {
            return self == other;
        }
        if self.is_string() || other.is_string() {
            return self.to_text() == other.to_text();
        }
        if self.is_f64() || other.is_f64() {
            return self.as_f64() == other.as_f64();
        }
        self.as_i64() == other.as_i64()
    }
}

impl PartialEq for Var {
    /// Strict equality: the same variant and value, except that `i32`, `i64` and `f64`
    /// compare by numeric value. Objects compare by identity. See "Equality" in the crate
    /// documentation.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert_eq!(Var::from(2), Var::from(2.0));
    /// assert_ne!(Var::from(1), Var::from(true));
    /// assert_ne!(Var::from("1"), Var::from(1));
    /// ```
    fn eq(&self, other: &Var) -> bool {
        match (&self.repr, &other.repr) {
            (Repr::Void, Repr::Void) | (Repr::Undefined, Repr::Undefined) => true,
            (Repr::Bool(a), Repr::Bool(b)) => a == b,
            (Repr::Str(a), Repr::Str(b)) => a == b,
            (Repr::Array(a), Repr::Array(b)) => a == b,
            (Repr::Object(a), Repr::Object(b)) => Rc::ptr_eq(a, b),
            (Repr::Binary(a), Repr::Binary(b)) => a == b,
            (Repr::Double(a), Repr::Double(b)) => a == b,
            (Repr::Double(x), Repr::Int(i)) | (Repr::Int(i), Repr::Double(x)) => {
                int_eq_float(i64::from(*i), *x)
            }
            (Repr::Double(x), Repr::Int64(i)) | (Repr::Int64(i), Repr::Double(x)) => {
                int_eq_float(*i, *x)
            }
            (Repr::Int(_) | Repr::Int64(_), Repr::Int(_) | Repr::Int64(_)) => {
                self.as_i64() == other.as_i64()
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_numbers() {
        assert_eq!(leading_i64("  +17abc"), 17);
        assert_eq!(leading_i64("-"), 0);
        assert_eq!(leading_i64("99999999999999999999"), i64::MAX);
        assert_eq!(leading_i64("-99999999999999999999"), i64::MIN);
        assert_eq!(leading_f64(".5"), 0.5);
        assert_eq!(leading_f64("5."), 5.0);
        assert_eq!(leading_f64("1e"), 1.0);
        assert_eq!(leading_f64("1e+"), 1.0);
        assert_eq!(leading_f64("-."), 0.0);
        assert_eq!(leading_f64("2E-1x"), 0.2);
        assert!(int_eq_float(i64::MIN, -9_223_372_036_854_775_808.0));
        assert!(!int_eq_float(i64::MAX, 9_223_372_036_854_775_808.0));
    }
}
