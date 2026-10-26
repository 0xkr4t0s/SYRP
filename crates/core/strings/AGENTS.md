# syrp-strings

## Purpose
The L2 string utilities that have no `std` equivalent (master plan §5.2, Wave 1.1 item 13).
`String`/`&str` replace JUCE's `String`, `StringArray` and `StringRef` (ledger: mapped). This
crate covers JUCE's `Base64` (module `base64`: `encode`, `encode_to`, `decode`), `StringPool`
(`get`, `len`, `garbage_collect`, `global`) and `TextDiff` (`TextDiff`, `Change`). Errors are
`StringsError` (opaque: `kind()` + `position()`) with codes from `errors/strings.toml`.
`LocalisedStrings` and the process-wide `translate` follow in the second increment.

## Ledger rows
juce_core/Base64, juce_core/StringPool, juce_core/TextDiff, juce_core/LocalisedStrings

## Invariants
- Base64 is RFC 4648 §4 only: standard alphabet, output always padded. `decode` accepts exactly
  what `encode` produces: whitespace, URL-safe characters, missing or misplaced `=` and
  non-zero unused bits are errors (`SYRP-STRINGS-0001`/`0002`); never panics. The output buffer
  is reserved with `try_reserve_exact` (`SYRP-STRINGS-0003`).
- `StringPool::get` returns clones of one `Arc<str>` per distinct text while the entry is
  pooled; `garbage_collect` drops only entries whose strong count is 1 (the pool's own).
- `TextDiff` positions are `char` counts relative to the text after earlier changes; each
  `Change` is either a removal (`removed_len > 0`, empty text) or an insertion. `apply(original)`
  equals the target for every pair. Myers' algorithm on the trimmed middle, capped at edit
  distance 1024 (then one removal plus one insertion), so memory stays bounded.
- Error types bind every field explicitly in `Display` (no `..`).

## Thread affinity
Any thread, not RT-safe: everything allocates, and `StringPool` locks a mutex.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-strings
```
B1 and B3 use proptest (round trip over random bytes; diff over random Unicode text).

## Known gaps
Increment 2 of WP-1.1-13 adds B4 (`LocalisedStrings`: translation-file parser, `translate`,
`ignore_case`, fallback, merge, line-numbered errors), B5 (process-wide current mappings and
the `translate` free function) and their error codes.
