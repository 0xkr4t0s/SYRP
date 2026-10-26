# Start a work package

**Goal:** go from a WP id to a first PR that passes acceptance.

**Preconditions:** the WP's dependencies are merged (`cargo auricle wp ready` lists it).

**Steps**
1. `cargo auricle wp show <id> --json`: read `spec`, `owned_paths`, `package.acceptance`.
2. `git switch -c wp/<id>-<slug> origin/main`.
3. If the WP owns a crate that does not exist, follow `create-a-crate.md`.
4. Write one failing test per spec behaviour, named `b<n>_<what>`.
5. Implement until `cargo auricle check` and `cargo auricle test --crate <package>` pass.
6. `cargo auricle wp check <id> --json` must exit 0.
7. Commit with trailers, push, open the PR from the template with the front matter filled in.

**Verification:** `cargo auricle wp check <id>` exits 0; `cargo auricle wp check-paths <id>` reports no diagnostics.

**Common errors:** `AUR-WP-0005` (you touched a shared file: revert it and file a change request),
`AUR-WP-0008` (crate missing), `AUR-CLI-0206` (crate metadata or AGENTS.md missing).
