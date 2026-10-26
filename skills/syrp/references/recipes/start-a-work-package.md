# Start a work package

**Goal:** go from a WP id to a first PR that passes acceptance.

**Preconditions:** the WP's dependencies are merged (`cargo syrp wp ready` lists it).

**Steps**
1. `cargo syrp wp show <id> --json`: read `spec`, `owned_paths`, `package.acceptance`.
2. `git switch -c wp/<id>-<slug> origin/main`.
3. If the WP owns a crate that does not exist, follow `create-a-crate.md`.
4. Write one failing test per spec behaviour, named `b<n>_<what>`.
5. Implement until `cargo syrp check` and `cargo syrp test --crate <package>` pass.
6. `cargo syrp wp check <id> --json` must exit 0.
7. Commit with trailers, push, open the PR from the template with the front matter filled in.

**Verification:** `cargo syrp wp check <id>` exits 0; `cargo syrp wp check-paths <id>` reports no diagnostics.

**Common errors:** `SYRP-WP-0005` (you touched a shared file: revert it and file a change request),
`SYRP-WP-0008` (crate missing), `SYRP-CLI-0206` (crate metadata or AGENTS.md missing).
