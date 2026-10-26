# WP-0.1-11: juce-oracle differential harness (separate AGPL repository)

## Purpose
A separate repository, `juce-oracle`, licensed AGPL-3.0, that builds real JUCE 9.0.4 and runs parity scenarios to produce reference outputs (master plan §9.2). It is never distributed with Syrp, and no Syrp agent reads it.

## Why this is a human WP
Someone must create the repository, decide who may access it, and approve using JUCE under the AGPL. After that, an oracle agent can be granted access to that repository only.

## Behaviour (for the oracle agent, once the repo exists)
- B1. CMake project pinned to JUCE 9.0.4 that builds a scenario runner.
- B2. Reads scenario files in the format of `parity/scenarios/` and writes outputs (audio WAV, JSON dumps, PNG) plus a manifest with hashes.
- B3. Publishes reference outputs as release artefacts that Syrp CI downloads by hash.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.1-11` first and `cargo syrp wp check WP-0.1-11` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
