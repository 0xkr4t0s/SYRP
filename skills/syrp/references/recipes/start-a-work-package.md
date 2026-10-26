# Start a work package

**Goal:** go from a WP id to a pushed branch whose PR passes every merge gate.

**Preconditions**
- `cargo syrp doctor --json` reports no errors.
- The WP's dependencies are done: `cargo syrp wp ready` lists it, or `wp show` marks each
  dependency `done`.
- `git fetch origin main` is fresh.

**Steps**
1. `cargo syrp wp show <id> --json`: read `spec`, `owned_paths`, `package.risk` and
   `package.acceptance`. Search the master plan for the section the spec cites.
2. `git switch -c wp/<id>-<slug> origin/main` (or a worktree:
   `git worktree add <dir> -b wp/<id>-<slug> origin/main`). If a branch already exists
   (`git ls-remote origin 'wp/<id>-*'`), continue it instead.
3. If the WP owns a crate that does not exist, follow `create-a-crate.md`.
4. Write one failing test per spec behaviour, named `b<n>_<what>`, then implement.
5. Loop after every edit: `scripts/loop.sh <package>` (or `cargo syrp check --json` and
   `cargo syrp test --crate <package> --json`), and `cargo syrp explain <code>` on any diagnostic.
6. New failure modes get codes: `add-error-code.md`. Add `changes/<id>-<n>.json`:
   `{"wp": "<id>", "kind": "added", "summary": "..."}`.
7. Keep the size in budget: `git diff --shortstat origin/main...HEAD -- . ':!Cargo.lock'`
   must stay at or under 1,200 lines. If the spec is bigger, ship a coherent first increment.
8. Acceptance: `cargo syrp wp check <id> --base origin/main --json` exits 0, then
   `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` passes.
9. Commit; every commit ends with the trailers:
   ```text
   WP: <id>
   Spec: work/specs/<id>.md
   Agent-Model: <your model id>
   ```
10. `git push -u origin wp/<id>-<slug>` and open the PR from `.github/pull_request_template.md`.
    Title `[<id>] <crate>: <what>`; first body line
    `<!-- syrp-pr: {"wp":"<id>","final":<bool>,"risk":[...],"contracts_touched":[],"ledger_rows":[],"deps_added":[],"unsafe_count":0} -->`.
    Copy `risk` from the WP file; set `final` to true only on the PR that completes the WP.

**Verification**
- `cargo syrp wp check <id> --json` exits 0 (its `steps` show check, test, ownership all ok).
- `cargo syrp wp check-paths <id> --base origin/main` reports no diagnostics.
- The size and trailer checks from `fix-failing-check.md` pass.

**Common errors**
- `SYRP-WP-0005`: you touched a file you do not own. Revert it; open a `[CR]` change request.
- `SYRP-WP-0008`: the owned crate does not exist yet (`create-a-crate.md`).
- `SYRP-CLI-0206`: crate metadata or `AGENTS.md` missing.
- `SYRP-CLI-0004`: the ownership diff failed; run `git fetch origin main` or pass `--base`.
- `SYRP-PR-0001`, `SYRP-PR-0005`, `SYRP-PR-0006`: size, front matter, trailers (`fix-failing-check.md`).
