# Fix a failing check

**Goal:** turn a red `check`, `test`, `wp check`, CI job or merge gate green without
suppressing anything.

**Preconditions**
- You are on your WP branch with a clean or committed tree, and `git fetch origin main` is fresh.
- You have the failing output: a local envelope, or the CI job's log.

**Steps**
1. Reproduce locally with the command the job ran. CI's jobs map to:
   - check job: `cargo syrp check --json`
   - test job: `cargo syrp test --json` (on Linux, macOS and Windows)
   - docs job: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, then
     `cargo test --workspace --doc`, then `cargo syrp docs build --json`
   - deny job: `cargo deny check`
   - ownership job: `cargo syrp wp check-paths <id> --base origin/main`
2. For each diagnostic run `cargo syrp explain <code>` and apply its `fix`. The usual ones:
   - `SYRP-CLI-0201` fmt: `cargo fmt --all`.
   - `SYRP-CLI-0202` clippy: fix the lint; an `#[allow]` needs a reason comment.
   - `SYRP-CLI-0301` tests: fix the code or the test's expectation from the spec. Never
     delete, skip or `#[ignore]` a test.
   - `SYRP-WP-0005` ownership: revert the file
     (`git checkout origin/main -- <path>`) and open a `[CR]` change-request issue.
3. Rustdoc warnings (docs job): fix broken intra-doc links. A link to an item behind a
   feature flag breaks when that feature is off; link only to always-present items or use
   plain backticks.
4. Size (`SYRP-PR-0001`): measure with
   `git diff --shortstat origin/main...HEAD -- . ':!Cargo.lock'`. Every file but `Cargo.lock`
   counts, including fixtures and generated JSON. Shrink fixtures, generate data in tests, or
   split the work: this PR ships `"final": false` and the rest comes in the next PR.
5. Provenance (`SYRP-PR-0006`): list each commit's trailers; an empty column is a failure:
   ```sh
   git log --format='%h %s | WP=%(trailers:key=WP,valueonly) | Agent-Model=%(trailers:key=Agent-Model,valueonly)' origin/main..HEAD
   ```
   Add the missing trailers to every commit and drop merge commits in one pass:
   ```sh
   git rebase --force-rebase origin/main --exec 'git -c trailer.ifexists=replace commit -q --amend --no-edit --trailer "WP: <id>" --trailer "Spec: work/specs/<id>.md" --trailer "Agent-Model: <model>"'
   git push --force-with-lease
   ```
   If you must merge `origin/main` instead of rebasing, the merge commit needs the trailers
   too, in one final paragraph:
   ```sh
   git merge --no-edit origin/main -m "Merge origin/main into wp/<id>-<slug>" -m "WP: <id>
   Spec: work/specs/<id>.md
   Agent-Model: <model>"
   ```
6. Front matter (`SYRP-PR-0005`): the PR body's first line must be
   `<!-- syrp-pr: {...} -->` with a known WP id and only allowed risk categories.
7. To evaluate every merge gate the way the PR manager does:
   `gh pr view <n> --json number,body,labels,files,commits,statusCheckRollup,mergeable,isDraft > pr.json`
   (outside the checkout), then `cargo syrp pr gates --pr pr.json`.

**Verification**
- `scripts/loop.sh <package>` prints `"ok":true` and exits 0.
- `cargo syrp wp check <id> --base origin/main --json` exits 0.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` exits 0.
- The size and trailer commands above show at most 1,200 lines and no empty columns.

**Common errors**
- `SYRP-CLI-0004`: the diff base is missing; `git fetch origin main` or pass `--base <ref>`.
- `SYRP-CLI-0102`: wrong toolchain; run `rustup toolchain install` in the repo root.
- `SYRP-CLI-0101`: a tool is missing; `cargo syrp doctor` names it.
- `SYRP-CLI-0207`: a review file was committed; delete it.
