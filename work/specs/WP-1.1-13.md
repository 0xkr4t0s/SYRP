# WP-1.1-13: syrp-strings: Base64, StringPool, TextDiff and LocalisedStrings

## Purpose
The string utilities from master plan §5.2 (L2 Core) and Wave 1.1 item 13. Rust's `String` and `&str` replace JUCE's `String`, `StringArray` and `StringRef` (the ledger records those as "mapped"); this crate holds the utility classes that have no `std` equivalent. Delivery: 2 PRs (B1–B3, then B4–B6).

## Behaviour
- B1. `base64::encode(&[u8]) -> String` and `base64::decode(&str) -> Result<Vec<u8>, StringsError>` with the standard alphabet and padding (RFC 4648 §4), plus `encode_to(&[u8], &mut impl fmt::Write)`. Decoding rejects characters outside the alphabet and bad padding with a coded error. Proptest: `decode(encode(x)) == x` for any bytes; RFC 4648 §10 vectors pass.
- B2. `StringPool`: `get(&self, &str) -> Arc<str>` returns the same allocation for equal text (pointer equality test), `len`, and `garbage_collect()` that frees entries nobody else holds. Thread-safe (`Send + Sync`).
- B3. `TextDiff`: `TextDiff::new(original, target)` produces a list of changes (each an insertion of text at a character position or a removal of a character range); `apply(original)` reproduces `target` exactly for any pair (proptest over Unicode text, positions in `char`s). Minimality is not required; document the algorithm (a Myers-style diff is fine).
- B4. `LocalisedStrings`: parse the translation-file format the class documentation describes (a `language:` line, a `countries:` line, then `"original" = "translated"` lines; comments and blank lines ignored; escaped quotes inside strings), `translate(&str) -> &str` returning the input unchanged when no entry exists, `language_name`, `country_codes`, `ignore_case` option, `with_fallback(LocalisedStrings)`, and `merge`. Malformed lines produce a coded error with a line number.
- B5. A process-wide current mapping: `set_current_mappings(Option<LocalisedStrings>)` and `translate(&str)` free function (JUCE's `TRANS`), safe to call from any thread; documented as not for the audio thread.
- B6. Errors use the new `errors/strings.toml` domain (`SYRP-STRINGS-0001`...). No third-party dependencies are needed; if you add one, list it in `deps_added`.

## Crate setup
Create `crates/core/strings/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-13` first and `cargo syrp wp check WP-1.1-13` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
