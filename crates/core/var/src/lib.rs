//! Dynamic values for Syrp: interned [`Identifier`]s, the [`Var`] value type,
//! [`NamedValueSet`] and [`DynamicObject`].
//!
//! `syrp-state` stores node types and property names as identifiers and property values as
//! `Var`s. The behaviour follows JUCE's public documentation for `Identifier`, `var`,
//! `NamedValueSet` and `DynamicObject`. Coercing conversions and equality, copy-on-write
//! arrays, the property API, JSON and serde arrive in later increments.
//!
//! ```
//! use syrp_var::{Identifier, Var};
//!
//! let gain = Identifier::new("gain");
//! let value = Var::from("0.5");
//! assert_eq!(gain.as_str(), "gain");
//! assert_eq!(value.as_str(), Some("0.5"));
//! ```
//!
//! # Threads
//! [`Identifier`] is `Send + Sync`. [`Var`], [`NamedValueSet`] and [`DynamicObject`] are
//! neither `Send` nor `Sync`: their heap data is shared through `Rc`, and objects change
//! through a `RefCell`, like JUCE's `var` and `DynamicObject`, which are not thread-safe, and
//! like the `!Send` `syrp-state` store that holds them. To move a value to another thread,
//! serialise it (JSON, or serde, both in later increments of this crate) and rebuild it there.
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

mod identifier;
mod object;
mod var;

pub use identifier::Identifier;
pub use object::{DynamicObject, NamedValueSet};
pub use var::{Var, VarView};
