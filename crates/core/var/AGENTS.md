# syrp-var

## Purpose
The L2 dynamic value layer (master plan §5.2, Wave 1.1). Covers JUCE's `Identifier` (as
`Identifier`), `var` (as `Var`), `NamedValueSet`, `DynamicObject`, `JSON` (module `json`) and, from WP-1.1-15, `XmlElement`
(module `xml`). `syrp-state` stores node types and property names as `Identifier`s and property values
as `Var`s. Behaviour comes from the public JUCE documentation only.

## Ledger rows
juce_core/var, juce_core/Identifier, juce_core/NamedValueSet, juce_core/DynamicObject,
juce_core/JSON, juce_core/XmlElement, juce_core/XmlAttribute (WP-1.1-15; juce_core/XmlDocument
follows with the parser)

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
- JSON and serde share one mapping (`src/serde_impl.rs`, tabled in `json` rustdoc). Both
  directions stop at `json::MAX_DEPTH` (100) nesting levels, so parsing, dropping and writing
  cannot overflow the stack; `serde_json`'s 128-level recursion limit stays on behind it
  (never enable its `unbounded_depth` feature). Codes: `SYRP-VAR-0001` invalid JSON,
  `-0002` input too deep, `-0003` value too deep or cyclic to write.
- JSON object keys are interned as `Identifier`s and never freed.
- `Debug` for `DynamicObject` never prints properties (objects may contain themselves).
- XML (`src/xml/`): `XmlElement` owns `tag: String`, `attributes: Vec<XmlAttribute>`,
  `children: Vec<XmlNode>`; `XmlNode` is an element or a text `String`. No `Rc`, so both are
  `Send + Sync`. Tag and attribute names always pass `is_valid_xml_name` (`SYRP-VAR-0004`).
  `Clone`, `Drop`, `is_equivalent_to` (also `==`) and `all_sub_text` use explicit stacks,
  never recursion (100,000-level tests); `Debug` is shallow. Attribute text conversions call
  the same `convert::text_as_*` helpers as `Var`.

## Features
`serde` (default): public `Serialize`/`Deserialize` for `Var`. `json` works without it
(`serde_core` and `serde_json` are plain dependencies).

## Thread affinity
`Identifier` is `Send + Sync`; `Identifier::new` locks and may allocate (not for the audio
thread), every other method is RT-safe. `Var`, `NamedValueSet` and `DynamicObject` are
`!Send` and `!Sync` (phase-1 board decision, spec B2a): message-thread or single-thread use.
Move values between threads by serialising them. `XmlElement`, `XmlNode` and `XmlAttribute`
are `Send + Sync`.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-var
cargo test -p syrp-var --no-default-features    # without the serde feature
PROPTEST_CASES=5000 cargo test -p syrp-var --test json   # deeper round-trip run
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
- JSON writes undefined as `null`, NaN and infinities as `null`, binary as an array of byte
  values, `f64` always with a fraction or exponent (`2.0`); unsigned integers above `i64::MAX`
  parse as `f64`; a repeated key keeps its first position and last value.
- XML model (WP-1.1-15): text is a separate `XmlNode` variant, not an element; the namespace
  prefix is the text before the *first* `:`; `has_tag_name_ignoring_namespace(n)` matches the
  full tag or the local part; `set_attribute` with an invalid name is an error (not ignored);
  `f64` attributes use Rust's shortest text, so non-finite values read back as 0.

## Known gaps
WP-1.1-01 is complete (B1-B7). The CLI catalogue entry for the new `errors/var.toml` domain is
tracked on CR #5. WP-1.1-15 increment 1 delivers the XML element model (B1-B2, the core of B3, and B5 for
clone, drop and comparison); the rest of B3, the parser (B4-B5) and the writer (B6-B8) follow.
