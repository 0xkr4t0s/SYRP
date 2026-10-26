# Create a crate

**Goal:** a new workspace crate that passes `cargo auricle check`.

**Steps**
1. Directory `crates/<group>/<name>/` (members are globbed: no root edit).
2. `Cargo.toml`:
   ```toml
   [package]
   name = "auricle-<name>"
   description = "<one line>"
   version.workspace = true
   edition.workspace = true
   rust-version.workspace = true
   repository.workspace = true
   authors.workspace = true
   license = "MIT OR Apache-2.0"        # L0/L1 crates; others use license.workspace = true

   [package.metadata.auricle]
   layer = "L0"                          # L0..L7, see docs/guide/architecture.md
   wp = "<WP id>"

   [lints]
   workspace = true
   ```
3. `AGENTS.md` with the headings from `docs/agents/crate-agents-template.md`.
4. `src/lib.rs` with a crate-level doc comment.

**Verification:** `cargo auricle check --lints-only` reports no `AUR-CLI-0206` or `AUR-CLI-0203`.
