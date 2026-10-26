# WP-0.2-04: syrp-format-api: AudioFormat, Reader and Writer traits, metadata

## Behaviour
- B1. `trait AudioFormat`: name, file extensions, supported sample rates and bit depths, `can_read(header bytes)`, `create_reader`, `create_writer`.
- B2. `trait AudioReader`: length in samples, channels, sample rate, bit depth, float/int, `read(dest: AudioBlockMut, start)` with zero-fill past the end, metadata map.
- B3. `trait AudioWriter`: `write(block)`, `flush`, `finish` returning an error rather than panicking on I/O failure.
- B4. A metadata model covering common chunk types (BWF/bext, cue points, loops, ID3-style tags) as typed enums with an `Other(String, Vec<u8>)` escape.
- B5. Errors use `syrp-error` codes in a new `errors/format.toml`.
- B6. Contract freeze as in WP-0.2-01.

## Crate setup
Create `crates/contracts/format-api/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-04` first and `cargo syrp wp check WP-0.2-04` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
