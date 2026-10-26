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

## Known gaps
Increment 1 covers B1, B2 and B2a. Still to come in this WP: B3 (coercing conversions
`as_bool`/`as_i32`/`as_i64`/`as_f64`/`to_text`, strict `PartialEq`, `loose_eq`, the conversion
table in rustdoc and an "Open questions" section here for cases the JUCE docs leave open),
B4 (copy-on-write `array_mut`, `deep_clone`), B5 (the `NamedValueSet` and `DynamicObject`
property API; only constructors and counts exist now), B6 (JSON, `errors/var.toml`) and B7
(serde feature). `Var` has no `PartialEq` or `Display` until B3.
