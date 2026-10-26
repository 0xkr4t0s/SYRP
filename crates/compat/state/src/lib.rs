//! JUCE-shaped `ValueTree` over the `syrp-state` store, for porting JUCE code (ADR 0003 D1).
//!
//! This crate owns no storage. A [`ValueTree`] is a handle: a shared reference to a
//! [`syrp_state::Store`] plus a counted node handle. Every operation forwards to the store, so
//! native and compat code see the same document, and [`ValueTree::handle`] gives the native
//! [`syrp_state::NodeKey`] for moving code off the compat layer. Dependencies run compat to
//! core only: `syrp-state` never depends on this crate.
//!
//! [`Listener`] is JUCE's `ValueTree::Listener`; listeners belong to the wrapper they were added to.
//!
//! Method names follow Rust conventions; each carries a `#[doc(alias)]` with its JUCE name
//! (`getProperty`, `createCopy`, ...), so rustdoc search finds both.
//!
//! ```
//! use syrp_compat_state::ValueTree;
//!
//! syrp_loop_api::ManualLoop::new().with_token(|mt| {
//!     let synth = ValueTree::new(mt, "Synth");
//!     synth.set_property(mt, "cutoff", 1200.0, None).unwrap();
//!     assert_eq!(synth.property("cutoff").as_f64(), 1200.0);
//! });
//! ```

mod listener;
mod tree;

pub use listener::Listener;
pub use tree::{Children, ValueTree};
