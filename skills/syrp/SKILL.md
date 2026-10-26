---
name: syrp
description: Build, test and verify code in the Syrp Rust framework (a JUCE reimplementation). Use when working in the syrp repository, on an Syrp work package, or when a `cargo syrp` command or SYRP-* error code appears.
---

# Syrp

## The closed loop
1. Run `cargo syrp doctor --json` once per session; fix anything it reports first.
2. Look things up instead of guessing:
   `cargo syrp docs search "<words>"`, `cargo syrp wp show <id>`, `cargo syrp explain <code>`.
3. After every edit: `cargo syrp check --json`, then `cargo syrp test --crate <package> --json`.
4. On any SYRP-* code, run `cargo syrp explain <code>`, apply the fix, and rerun. Never suppress it.
5. Before opening a PR: `cargo syrp wp check <id> --json` must exit 0.

Parse the JSON envelope (`schema: syrp.envelope/v1`): `ok`, `exit_code`, `results[]`,
`diagnostics[{code, severity, message, location, hint}]`. Exit codes: 0 ok, 1 failed, 2 usage,
3 environment, 4 parity, 5 validator, 6 golden, 7 contract, 8 RT safety, 9 clean room, 10 internal.

## Hard rules
- Clean room: never fetch or recall JUCE source. Work from specs, public docs and scenarios.
- Edit only your work package's paths (`wp show` lists them). Shared files go through a change request.
- Never allocate, lock or do I/O on the audio thread. Message-thread APIs need a `MessageThread` token.
- Never edit goldens, `contracts/` or ledger evidence. Never skip or delete a test to get green.
- Every commit carries `WP:`, `Spec:` and `Agent-Model:` trailers.

## Recipes
- `references/recipes/start-a-work-package.md`
- `references/recipes/create-a-crate.md`

## Scripts
- `scripts/loop.sh [package]`: check, then test, one JSON summary line.
