# syrp-provenance

## Purpose
Clean-room enforcement tooling (master plan §10.9). Two halves:
- `provenance`: checks that every commit in a range carries `WP:` (or `Change-Request:`) and
  `Agent-Model:` trailers, and that every source file *added* in the range starts with an SPDX
  header matching its crate's `license` field.
- Scanner (next PR for this WP): tokenises source into normalised n-grams, winnows them into
  fingerprints and scores files against a fingerprint database produced elsewhere.
Covers no JUCE class.

## Ledger rows
None (tooling).

## Invariants
- This crate never reads JUCE source. Tests use synthetic data only.
- Commit trailers are read from the last paragraph of the message; merge commits are skipped.
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
- WP-0.1-12 scanner (B2-B5: tokeniser, winnowing, fingerprint database format, JSON report,
  `SYRP-CR-0001`/`0002`) arrives in the next PR. The quarantined job that builds the database
  is the PR manager's.
- AGENTS.md heading checks (mentioned in the crate template) are not implemented here.
