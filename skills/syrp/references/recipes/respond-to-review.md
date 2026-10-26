# Respond to a review

**Goal:** address every requested change on your PR so the next review round approves it.

**Preconditions**
- The PR has the `review:changes-requested` label, or its `syrp/review` (or
  `syrp/adversarial`) status is failure (`SYRP-PR-0003`, `SYRP-PR-0004`).
- You have the PR number and its branch `wp/<id>-<slug>`. Rounds are limited (the WP's
  `budget.max_review_rounds`, see `cargo syrp wp show <id>`); after that it escalates
  (`SYRP-COND-0002`), so fix everything in one round.

**Steps**
1. Check out the PR branch (`git fetch origin` then `git switch wp/<id>-<slug>`) and rebase
   it on `origin/main` if main moved (see `fix-failing-check.md` for the trailers).
2. Read every review comment: `gh pr view <n> --comments`. The review is a list of checklist
   items; `cargo syrp review checklist` prints what each id (`spec-conformance`, `tests`,
   `docs`, `clean-room`, ...) means.
3. Make a list of every requested change and address each one. Reproduce any failure the
   reviewer quotes with the same command before fixing it. An adversarial finding usually
   comes with an attack test: add it (named `b<n>_...` for the behaviour it breaks) and make
   it pass.
4. If you disagree with a request, do not silently skip it: explain why in a PR comment,
   citing the spec line or the master plan section.
5. Never answer a finding by deleting or skipping a test, adding an unexplained `#[allow]`,
   or editing a file you do not own (that needs a `[CR]` change request).
6. Re-run the full loop: `scripts/loop.sh <package>`,
   `cargo syrp wp check <id> --base origin/main --json` and
   `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`.
7. Commit with the trailers, push (`git push --force-with-lease` after a rebase), and update
   the PR body if traceability, `final` or `deps_added` changed.

**Verification**
- Every review comment has either a commit that addresses it or a reply explaining why not.
- `cargo syrp wp check <id> --json` exits 0; the size stays at or under 1,200 lines
  (`git diff --shortstat origin/main...HEAD -- . ':!Cargo.lock'`).
- If you have the PR JSON (`gh pr view <n> --json ...`), `cargo syrp pr gates --pr <file>`
  shows only the review gates pending.

**Common errors**
- `SYRP-PR-0006`: a new commit or a merge from main without trailers.
- `SYRP-PR-0001`: the fixes pushed the PR over 1,200 lines; move unrelated work to a follow-up PR.
- `SYRP-PR-0002`: CI went red on the new push; reproduce it with `fix-failing-check.md`.
- `SYRP-COND-0002`: review rounds exhausted; a person takes over.
