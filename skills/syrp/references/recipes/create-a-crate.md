# Create a crate

**Goal:** a new workspace crate that passes `cargo syrp check`.

**Preconditions**
- Your WP owns the crate path: `cargo syrp wp show <id>` lists it under `owned_paths`, and
  `wp check` reports `SYRP-WP-0008` until the crate exists.

**Steps**
1. Directory `crates/<group>/<name>/`. Workspace members are globbed: never edit the root
   `Cargo.toml` (that needs a `[CR]` change request).
2. `Cargo.toml`:
   ```toml
   [package]
   name = "syrp-<name>"
   description = "<one line>"
   version.workspace = true
   edition.workspace = true
   rust-version.workspace = true
   repository.workspace = true
   authors.workspace = true
   publish.workspace = true              # unpublished until release
   license = "MIT OR Apache-2.0"        # L0/L1 crates; others use license.workspace = true

   [package.metadata.syrp]
   layer = "L0"                          # L0..L7, see docs/guide/architecture.md
   wp = "<WP id>"

   [lints]
   workspace = true
   ```
   Third-party dependencies get an explicit version here (not `workspace = true` unless the
   workspace already declares them), a licence that `deny.toml` allows, and a `deps_added`
   entry in the PR front matter. Run `cargo deny check` after adding one.
3. `AGENTS.md` with the headings from `docs/agents/crate-agents-template.md`.
4. `src/lib.rs` with a crate-level doc comment; every public item gets rustdoc with an example.
5. `cargo syrp check --lints-only` (no compilation, seconds), then `scripts/loop.sh syrp-<name>`.

**Verification**
- `cargo syrp check --lints-only` reports no `SYRP-CLI-0206` or `SYRP-CLI-0203`.
- `cargo syrp test --crate syrp-<name> --json` runs the crate's tests.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` passes.

**Common errors**
- `SYRP-CLI-0206`: `[package.metadata.syrp]` or `AGENTS.md` missing.
- `SYRP-CLI-0203`: a dependency on a higher layer or a sibling; depend on the `*-api` crate.
- `SYRP-CLI-0204`: a banned dependency; remove it and do not read its source.
- `SYRP-CLI-0205`: a source file over 1,200 lines; split the module.
