# WP-1.1-01: syrp-var: Var, Identifier, NamedValueSet, DynamicObject and JSON

## Purpose
The dynamic value layer from master plan §5.2 (L2 Core) and Wave 1.1 items 1–2. `syrp-state` stores node types and property names as `Identifier`s and property values as `Var`s, so this crate is on the critical path to WP-1.2-03 and the parked WP-1.9-01. Delivery: 3 PRs (B1–B3, then B4–B5, then B6–B7). XML (`XmlElement`, `XmlDocument`) is a later WP in this crate.

## Behaviour
- B1. `Identifier`: a process-wide interned name, `Copy`, 4 bytes (`size_of::<Identifier>() == 4`), compared and hashed by its `u32`. `Identifier::new(&str)` returns the same value for equal strings from any thread; `as_str()` returns `&'static str` (interned strings are never freed; say so in the docs). `Ord` is by interning order, not by text; `cmp_text` gives lexical order. Empty and non-ASCII names are allowed. Creating an identifier may allocate and lock and is documented as not for the audio thread; reading one never does.
- B2. `Var`: an enum-like value with variants void, undefined, bool, `i32`, `i64`, `f64`, string, array, object (`Arc<DynamicObject>`) and binary (bytes). `size_of::<Var>() == 16` on 64-bit targets (a test asserts it), so heap variants use thin pointers. No public fields (ADR 0003 D3): constructors, `From` impls, `is_*` predicates and accessors.
- B3. Conversions and comparison. `as_bool`, `as_i32`, `as_i64`, `as_f64` and `to_text` follow the conversion table you write into the rustdoc (numbers from strings parse a leading number, booleans from numbers are non-zero, void and undefined convert to zero or empty). `loose_eq` compares after numeric and string coercion; `PartialEq` is strict (same variant and value, except that `i32`, `i64` and `f64` compare numerically). Write these rules from the `var` documentation; where it does not decide a case, choose, document and list it under "Open questions" in the crate `AGENTS.md` for the differential harness.
- B4. Sharing. Cloning a `Var` is O(1). Strings and binary are immutable and shared. Arrays have value semantics: mutating through `array_mut()` copies on write when shared. Objects are shared by reference, as `DynamicObject` documents (cloning a `Var` that holds an object refers to the same object); `deep_clone()` copies objects and arrays recursively.
- B5. `NamedValueSet` (a small `Vec<(Identifier, Var)>`: `get`, `set` returning whether the value changed, `remove`, `contains`, `len`, iteration in insertion order) and `DynamicObject` (a `NamedValueSet` of properties behind interior mutability, `!Sync` documented, plus `has_property`, `get_property`, `set_property`, `remove_property`). Methods on objects (native functions) are out of scope.
- B6. JSON: `json::parse(&str) -> Result<Var, VarError>` and `json::to_string(&Var, Format)` with compact and pretty formats; objects map to `DynamicObject`, arrays to arrays, integers that fit `i32`/`i64` to those variants, other numbers to `f64`. Round trip `parse(to_string(v))` preserves value for every `Var` without binary (proptest). Errors carry a line and column and a code in the new `errors/var.toml` domain (`SYRP-VAR-0001`...). `serde_json` may be used for parsing.
- B7. `serde::Serialize`/`Deserialize` for `Var` behind a default-on `serde` feature, matching the B6 mapping.

## Not in this WP
XML, `var` method calls and `var::NativeFunction`, JUCE's binary `var` stream format (it needs `syrp-io`).

## Crate setup
Create `crates/core/var/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-01` first and `cargo syrp wp check WP-1.1-01` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
