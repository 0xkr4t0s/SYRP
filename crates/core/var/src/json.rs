//! B6: reading and writing [`Var`]s as JSON.
//!
//! The mapping, which the `serde` impls (feature `serde`, on by default) share:
//!
//! | JSON | `Var` read | `Var` written |
//! |---|---|---|
//! | `null` | void | void and undefined |
//! | `true`, `false` | bool | bool |
//! | integer in `i32` range | `i32` | `i32`, `i64` |
//! | other integer in `i64` range | `i64` | |
//! | any other number | `f64` | `f64`, always with a `.` or exponent (`2.0`); NaN and infinities as `null` |
//! | string | string | string |
//! | array | array | array; binary as an array of byte values |
//! | object | [`DynamicObject`](crate::DynamicObject), keys in input order; a repeated key keeps its first position and its last value | object, properties in insertion order |
//!
//! Object keys become [`Identifier`](crate::Identifier)s, which are interned for the life of
//! the process: parsing untrusted JSON with unbounded key sets grows the identifier table.
//!
//! Nesting is limited to [`MAX_DEPTH`] levels both ways, so neither parsing nor writing (nor
//! dropping a parsed value) can overflow the stack. Deeper input fails with `SYRP-VAR-0002`;
//! writing a deeper or cyclic value fails with `SYRP-VAR-0003`. `serde_json`'s own recursion
//! limit stays enabled behind this one.
//!
//! `parse(to_string(v))` gives back an equal value for every `Var` without binary, undefined
//! or non-finite numbers, with objects compared by content (parsing makes new objects).
//!
//! ```
//! use syrp_var::{Identifier, json};
//!
//! let value = json::parse(r#"{"name": "osc", "gain": 0.5, "voices": [1, 2]}"#).unwrap();
//! let object = value.as_object().unwrap();
//! assert_eq!(object.get_property(Identifier::new("gain")).as_f64(), 0.5);
//! assert_eq!(
//!     json::to_string(&value, json::Format::Compact).unwrap(),
//!     r#"{"name":"osc","gain":0.5,"voices":[1,2]}"#
//! );
//! ```

use std::cell::Cell;

use serde_core::de::DeserializeSeed;

use crate::serde_impl::{Depth, Seed};
use crate::{Var, VarError};

/// The deepest nesting of arrays and objects that is read or written.
///
/// ```
/// assert_eq!(syrp_var::json::MAX_DEPTH, 100);
/// ```
pub const MAX_DEPTH: usize = 100;

/// How [`to_string`] lays out its output.
///
/// ```
/// use syrp_var::{Var, json::{self, Format}};
/// let v = Var::from(vec![Var::from(1)]);
/// assert_eq!(json::to_string(&v, Format::Compact).unwrap(), "[1]");
/// assert_eq!(json::to_string(&v, Format::Pretty).unwrap(), "[\n  1\n]");
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// No whitespace at all.
    #[default]
    Compact,
    /// One item per line, indented by two spaces, with `": "` after keys.
    Pretty,
}

/// Parses JSON text into a [`Var`].
///
/// Any JSON value is accepted at the top level. Errors carry the line and column and the code
/// `SYRP-VAR-0001` (invalid JSON) or `SYRP-VAR-0002` (nested deeper than [`MAX_DEPTH`]).
///
/// ```
/// use syrp_var::{Diagnostic, json};
/// assert_eq!(json::parse(" 42 ").unwrap().as_i32(), 42);
/// assert!(json::parse("4294967296").unwrap().is_i64());
/// assert!(json::parse("1.0").unwrap().is_f64());
/// let deep = "[".repeat(1000);
/// assert_eq!(json::parse(&deep).unwrap_err().code().as_str(), "SYRP-VAR-0002");
/// ```
pub fn parse(text: &str) -> Result<Var, VarError> {
    let too_deep = Cell::new(false);
    let mut reader = serde_json::Deserializer::from_str(text);
    Seed::root(&too_deep)
        .deserialize(&mut reader)
        .and_then(|value| reader.end().map(|()| value))
        .map_err(|error| VarError::parse(&error, too_deep.get()))
}

/// Writes a [`Var`] as JSON text.
///
/// Fails with `SYRP-VAR-0003` when arrays and objects nest deeper than [`MAX_DEPTH`], which
/// includes any object that contains itself.
///
/// ```
/// use syrp_var::{Diagnostic, Identifier, Var, json};
/// let looped = Var::new_object();
/// let object = looped.as_object().unwrap();
/// object.set_property(Identifier::new("me"), looped.clone());
/// let error = json::to_string(&looped, json::Format::Compact).unwrap_err();
/// assert_eq!(error.code().as_str(), "SYRP-VAR-0003");
/// object.remove_property(Identifier::new("me")); // break the cycle so it can be freed
/// ```
pub fn to_string(value: &Var, format: Format) -> Result<String, VarError> {
    let value = Depth::root(value);
    match format {
        Format::Compact => serde_json::to_string(&value),
        Format::Pretty => serde_json::to_string_pretty(&value),
    }
    .map_err(|error| VarError::write(&error))
}
