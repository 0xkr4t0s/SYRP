# syrp-var

## Purpose
The L2 dynamic value layer (master plan §5.2, Wave 1.1). Covers JUCE's `Identifier` (as
`Identifier`), `var` (as `Var`), `NamedValueSet`, `DynamicObject` and, in a later increment,
`JSON`. `syrp-state` stores node types and property names as `Identifier`s and property values
as `Var`s. Behaviour comes from the public JUCE documentation only. XML is a later WP here.

## Ledger rows
juce_core/var, juce_core/Identifier, juce_core/NamedValueSet, juce_core/DynamicObject,
juce_core/JSON

## Invariants
- `size_of::<Identifier>() == 4`; `size_of::<Var>() == 16` on 64-bit targets. Heap variants
  hold one thin `Rc` (`Rc<String>`, `Rc<Vec<_>>`, `Rc<DynamicObject>`), never `Arc`.
- Identifier index 0 is the empty name. Interned text is leaked and never freed; the name table
  is append-only chunks (`OnceLock`), so `as_str` is lock-free. `Ord` is interning order.
- No public fields (ADR 0003 D3). `Var::as_view()` lends a `Copy` `VarView` for matching.
- `PartialEq` is strict except `i32`/`i64`/`f64` compare by exact value; objects by `Rc`
  identity. `loose_eq` rules and the conversion table in the crate rustdoc are the source of
  truth. `NamedValueSet::set` reports a change unless `eq_same_type` (same variant and `==`).
- Arrays are copy-on-write (`Rc::make_mut`); objects are shared. `deep_clone` copies each
  source object once, so sharing and cycles are kept and it never recurses forever.
- `Rc` reference cycles through `DynamicObject` leak (documented in the crate rustdoc).
- `Debug` for `DynamicObject` never prints properties (objects may contain themselves).

## Thread affinity
`Identifier` is `Send + Sync`; `Identifier::new` locks and may allocate (not for the audio
thread), every other method is RT-safe. `Var`, `NamedValueSet` and `DynamicObject` are
`!Send` and `!Sync` (phase-1 board decision, spec B2a): message-thread or single-thread use.
Move values between threads by serialising them.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-var
```
The `compile_fail` doctests in `src/lib.rs` ("Threads") prove the `!Send`/`!Sync` half of B2a.

## Open questions (for the differential harness)
The JUCE `var` documentation does not decide these; the choices here are documented in rustdoc:
- `to_text` of a bool is `"1"`/`"0"`; of an `f64` is Rust's shortest round-trip text (`2.0`
  gives `"2"`, infinities `"inf"`/`"-inf"`, NaN `"NaN"`); of arrays, objects and binary is `""`.
- `as_bool` of arrays, objects and binary is false; of a string is true for trimmed `"true"`
  (any case) or a non-zero leading number; of NaN is true.
- Narrowing saturates: `i64` and string to `i32` clamp, `f64` to integers truncates and
  saturates with NaN as 0. String to integer reads digits only (`"1.9"` gives 1, `"1e3"` 1).
- `loose_eq`: a string against a scalar compares `to_text` (so `"42.0"` is not loosely `42`,
  `true` is loosely `"1"` but not `"true"`); void and undefined loosely equal only themselves;
  arrays, objects and binary use strict `==` (arrays are not compared loosely element-wise).
- Strict `==` of `i64` against `f64` is exact (no rounding through `f64`).
- `NamedValueSet::set` replacing `2` with `2.0` counts as a change (variant differs).
- `deep_clone` keeps aliasing: an object reached twice becomes one shared copy.

## Known gaps
B1-B5 are done (increments 1 and 2). Still to come: B6 (JSON `parse`/`to_string`, the new
`errors/var.toml` domain with line and column) and B7 (default-on `serde` feature). Review
notes for B6: keep `serde_json`'s recursion limit enabled, and add a test that deeply nested
input is refused with a coded error (unbounded nesting would overflow the stack on parse and
again on drop of the nested `Rc<Vec<Var>>`).
