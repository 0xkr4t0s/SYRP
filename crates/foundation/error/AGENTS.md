# syrp-error

## Purpose
The single error vocabulary for every Syrp crate: `ErrorCode` (a const-checked
`SYRP-<DOMAIN>-<NNNN>`), `Severity`, the `Diagnostic` trait (code, severity, hint, docs URL) and
`Report`, a ready-made diagnostic built from parts. The `catalogue` module parses
`errors/*.toml` and renders the docs-site pages. No JUCE class: JUCE reports errors ad hoc
(`Result`, bool returns); this replaces that with catalogued codes.

## Ledger rows
None (infrastructure).

## Invariants
- Default features have zero dependencies. `serde` (optional) and `toml` (feature `catalogue`)
  are the only dependencies allowed (L0).
- Every code matches `^SYRP-[A-Z]+-[0-9]{4}$`; build them with `const C: ErrorCode =
  ErrorCode::new(..)` so a typo fails compilation.
- `Report` serialises to exactly the envelope's `diagnostics` item
  (`docs/agents/envelope.schema.json`): optional `location`/`hint` are omitted, never null,
  and `docs` is `https://docs.syrp.dev/errors/<code>`.
- Every error type here is `Send + Sync + 'static`.
- Codes raised by this crate (`SYRP-ERROR-*`) live in `errors/error.toml`.

## Thread affinity
Any thread. Plain data, no global state. Building a `Report` or calling `docs_url` allocates, so
do not do it on the audio thread; `ErrorCode`, `Severity` and `Diagnostic::code` do not allocate.

## Stability
experimental (0.x). Many crates depend on it; keep the public API small and additive.

## How to test
```sh
cargo syrp test --crate syrp-error   # a self dev-dependency enables serde + catalogue in tests
cargo syrp check
```

## Known gaps
- No deserialisation of `Report` (an `ErrorCode` holds a `&'static str`).
- `syrp-cli` still has its own `Diagnostic` and catalogue loader, and does not yet embed
  `errors/error.toml`; switching it over is a follow-up for the CLI's owner.
