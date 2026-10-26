# syrp-provenance

## Purpose
Clean-room enforcement tooling (master plan §10.9). Two halves:
- `provenance`: checks that every commit in a range carries `WP:` (or `Change-Request:`) and
  `Agent-Model:` trailers, and that every source file *added* in the range starts with an SPDX
  header matching its crate's `license` field.
- `scanner` + `database` + `report`: tokenises C-family source into normalised n-grams,
  winnows them into fingerprints, and scores files against a fingerprint database produced
  elsewhere. Covers no JUCE class.

## Ledger rows
None (tooling).

## Invariants
- This crate never reads JUCE source. It only consumes the fingerprint database, whose format
  (`syrp.cleanroom-fingerprints/v1`) is documented in `src/database.rs`. The database holds
  hashes only, never source text.
- Fingerprints are deterministic across platforms: FNV-1a 64 over normalised tokens, never
  `std`'s randomised hasher. Changing the normaliser means bumping `NORMALISER`.
- Tests use synthetic corpora only.
- SPDX checks apply to files added in the range only; pre-existing files are not re-checked.
- Every `SYRP-CR-*` code used in `src/` exists in `errors/cleanroom.toml`.

## Thread affinity
Any thread. Not used on audio threads. `provenance::git_*` shell out to `git`.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-provenance
cargo syrp check
```
The provenance integration tests need `git` on `PATH`; they build throwaway repositories in the
system temp directory.

## Known gaps
- Not yet wired into `cargo syrp` (needs a change to `crates/tooling/cli`, including embedding
  `errors/cleanroom.toml` in the explain catalogue).
- The quarantined CI job that builds the database is the PR manager's (B3).
- AGENTS.md heading checks (mentioned in the crate template) are not implemented here.
- Identifiers are lowercased but kept (B2 as written), so consistent renaming evades the
  scanner. Candidate for an identifier-erasing `syrp-cfamily-v3` normaliser.
- The verdict is a per-file fraction: a long verbatim passage in a large file can stay under the
  threshold. Candidate: also fail on the longest matched run or the raw `matched` count.
- `Dialect::Cpp` does not yet understand `R"d(...)d"` raw strings or `1'000` digit separators
  (they add noise tokens, not missed matches).
