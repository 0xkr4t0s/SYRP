//! Dynamic values for Syrp: interned [`Identifier`]s, the [`Var`] value type,
//! [`NamedValueSet`] and [`DynamicObject`].
//!
//! `syrp-state` stores node types and property names as identifiers and property values as
//! `Var`s. The behaviour follows JUCE's public documentation for `Identifier`, `var`,
//! `NamedValueSet` and `DynamicObject`; where that documentation leaves a case open, the choice
//! made here is stated below and listed in the crate's `AGENTS.md` for the differential harness.
//! [`json`] reads and writes JSON; with the default `serde` feature, `Var` also implements
//! `serde::Serialize` and `serde::Deserialize` with the same mapping. [`xml`] holds the XML
//! element tree ([`XmlElement`], [`XmlNode`]).
//!
//! # Sharing
//! Cloning a [`Var`] is O(1). Strings and binary data are immutable and shared. Arrays have
//! value semantics: [`Var::array_mut`] copies the items first when they are shared. Objects are
//! shared by reference: a clone refers to the same [`DynamicObject`], and [`Var::deep_clone`]
//! copies arrays and objects recursively.
//!
//! Reference cycles leak: an object that holds a `Var` referring to itself, directly or through
//! other objects, is never freed, because `Rc` does not collect cycles. Break the cycle (remove
//! the property) before dropping the last outside handle.
//!
//! ```
//! use syrp_var::{Identifier, Var};
//!
//! let gain = Identifier::new("gain");
//! let value = Var::from("0.5");
//! assert_eq!(gain.as_str(), "gain");
//! assert_eq!(value.as_str(), Some("0.5"));
//! assert_eq!(value.as_f64(), 0.5);
//! assert!(value.loose_eq(&Var::from(0.5)));
//! ```
//!
//! # Threads
//! [`Identifier`] is `Send + Sync`. [`Var`], [`NamedValueSet`] and [`DynamicObject`] are
//! neither `Send` nor `Sync`: their heap data is shared through `Rc`, and objects change
//! through a `RefCell`, like JUCE's `var` and `DynamicObject`, which are not thread-safe, and
//! like the `!Send` `syrp-state` store that holds them. To move a value to another thread,
//! serialise it ([`json::to_string`] or serde) and rebuild it there.
//!
//! ```compile_fail
//! fn need_send<T: Send>() {}
//! need_send::<syrp_var::Var>();
//! ```
//! ```compile_fail
//! fn need_sync<T: Sync>() {}
//! need_sync::<syrp_var::Var>();
//! ```
//! ```compile_fail
//! fn need_send<T: Send>() {}
//! need_send::<syrp_var::NamedValueSet>();
//! ```
//! ```compile_fail
//! fn need_sync<T: Sync>() {}
//! need_sync::<syrp_var::NamedValueSet>();
//! ```
//! ```compile_fail
//! fn need_send<T: Send>() {}
//! need_send::<syrp_var::DynamicObject>();
//! ```
//! ```compile_fail
//! fn need_sync<T: Sync>() {}
//! need_sync::<syrp_var::DynamicObject>();
//! ```
//!
//! # Conversions
//! The coercing accessors never fail. "Leading number" means: skip leading whitespace, then
//! read an optional sign and the longest decimal number that follows (`as_i32`/`as_i64`: digits
//! only, so `"1.9"` gives 1; `as_f64`: digits, an optional fraction and an optional exponent).
//! Text with no leading number converts to 0.
//!
//! | From | `as_bool` | `as_i32` | `as_i64` | `as_f64` | `to_text` |
//! |---|---|---|---|---|---|
//! | void, undefined | false | 0 | 0 | 0.0 | `""` |
//! | bool | itself | 1 or 0 | 1 or 0 | 1.0 or 0.0 | `"1"` or `"0"` |
//! | `i32` | non-zero | itself | widened | exact | decimal |
//! | `i64` | non-zero | clamped to `i32` | itself | nearest | decimal |
//! | `f64` | non-zero (NaN is true) | truncated, saturating, NaN is 0 | same | itself | shortest round-trip text (`inf`, `-inf`, `NaN`) |
//! | string | `"true"` (trimmed, any case) or a non-zero leading number | leading integer, clamped | leading integer, saturating | leading number | itself |
//! | array, object, binary | false | 0 | 0 | 0.0 | `""` |
//!
//! # Equality
//! `==` ([`PartialEq`]) is strict: both values must have the same variant and value, except
//! that `i32`, `i64` and `f64` compare by exact numeric value (`2 == 2.0`). Strings and binary
//! compare by content, arrays element by element with `==`, and objects by identity (the same
//! `Rc`). Void equals only void, undefined only undefined. `NaN != NaN`, so `Var` is not `Eq`.
//!
//! [`Var::loose_eq`] coerces scalars (bools, numbers and strings) first:
//! - if either side is a string, both sides are compared as [`Var::to_text`];
//! - otherwise, if either side is an `f64`, both are compared as [`Var::as_f64`];
//! - otherwise both are compared as [`Var::as_i64`] (so `true` loosely equals `1`).
//!
//! Void, undefined, arrays, objects and binary are compared with `==`.

mod convert;
mod error;
mod identifier;
pub mod json;
mod object;
mod serde_impl;
mod share;
mod var;
pub mod xml;

pub use error::VarError;
pub use identifier::Identifier;
pub use object::{DynamicObject, NamedValueIter, NamedValueSet};
pub use syrp_error::Diagnostic;
pub use var::{Var, VarView};
pub use xml::{XmlAttribute, XmlDocument, XmlElement, XmlNode, XmlNodeView};
