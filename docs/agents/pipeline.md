# The pipeline

Hands-off by default: work packages are scheduled, implemented, reviewed and merged by agents
and deterministic code. People are an escalation path. The design is master plan §12; this page
describes what is actually wired up.

## Actors

| Actor | Runs as | Can |
|---|---|---|
| **Conductor** | `conductor.yml`, every 30 minutes and after each merge. Deterministic: `cargo syrp conductor tick`. | Open WP issues, take and release leases, start agents, escalate. Cannot push code or merge. |
| **Implementer** | Claude Code in a GitHub Actions job, one per WP, up to `SYRP_SLOTS` (default 12) at once. Prompt: `agents/roles/implementer.md`. | Push to its own `wp/<id>-<slug>` branch and open or update one PR, using `SYRP_IMPLEMENTER_TOKEN` (cannot set statuses or merge). Edits only its WP's paths. |
| **Spec room** | Same job with `agents/roles/spec-room.md`, for WPs with `role = "spec-room"`. | Write specs, scenarios, inventory. Never reads JUCE source. |
| **Reviewer** | `reviewer.yml` on every ready PR update. Prompt: `agents/roles/reviewer.md`. | Rerun everything in a fresh checkout and write a review file outside the checkout. The workflow, not the agent, validates it against the PR and review kind and sets the `syrp/review` status. |
| **Adversarial verifier** | Same workflow, second job, different model, for high-risk PRs. Prompt: `agents/roles/adversarial.md`. | Same, with attack tests; sets `syrp/adversarial`. |
| **PR manager** | `pr-manager.yml` on checks, statuses and reviews. Gates are deterministic (`cargo syrp pr gates`); an agent (`agents/roles/pr-manager.md`) is started only to rebase conflicts or triage red CI. | Sole merger. Sole editor of shared files. |
| **People** | Issues labelled `needs-human`. | Decide, then remove the label. |

## Life of a work package

1. The conductor opens an issue `[WP-x] title` labelled `wp:<id>`, `status:ready`.
2. When its dependencies' issues are closed, the conductor takes a lease (`status:in-progress`,
   lease marker in the issue body) and starts an implementer.
3. The implementer opens a PR with front matter `<!-- syrp-pr: {...} -->`. When the job ends,
   the issue moves to `status:in-review` (or back to `status:ready` with a restart counted).
4. CI runs. The reviewer runs; its verdict becomes the `syrp/review` status and a PR label
   (`review:approved` or `review:changes-requested`).
5. Changes requested: the conductor starts a revision run (round + 1). After 3 rounds it escalates.
6. The PR manager evaluates gates: front matter, size (1,200 lines), ownership, provenance
   trailers, CI green, review approved, adversarial approved when risky. All pass: squash-merge,
   pinned to the evaluated head commit (`--match-head-commit`).
7. The final PR of a WP says `Closes #<issue>`. The closed issue marks the WP done, and its
   dependants become ready on the next tick.

## Escalations

The conductor labels an issue `needs-human` and comments with a catalogued code:
`SYRP-COND-0001` (restarts exhausted), `SYRP-COND-0002` (review rounds exhausted),
`SYRP-COND-0003` (a WP only a person can do). Everything else keeps running.

## Before enabling the pipeline

These settings live in GitHub, outside the repository, and the pipeline's safety depends on
them. Complete every item before setting `SYRP_PIPELINE_ENABLED=true`.

1. **Secret `ANTHROPIC_API_KEY`**, used by every agent job.
2. **Secret `SYRP_IMPLEMENTER_TOKEN`**: a fine-grained token for this repository only, with
   Contents, Pull requests and Issues read and write. **No Commit statuses, no Actions, no
   Administration, and no bypass of branch rules.** Implementer, spec-room and PR-manager agents
   run with it, so it must not be able to approve or merge.
3. **Secret `SYRP_BOT_TOKEN`**: a second fine-grained token (ideally a separate bot account or
   GitHub App) with Contents, Pull requests, Issues, Commit statuses and Actions read and write.
   Only the deterministic jobs use it: `conductor.yml`, the `publish` job of `reviewer.yml`, and
   `pr-manager.yml`. Statuses and merges made with the default `GITHUB_TOKEN` do not trigger
   other workflows, which is why the pipeline needs this token to chain its steps.
4. **A ruleset or branch protection on `main`**:
   - no direct pushes, no force pushes, no deletions;
   - squash merging only;
   - required status checks: every `ci` job, `syrp/review` and `syrp/adversarial`. Where GitHub
     lets you, pin the required source of `syrp/*` statuses to the bot identity;
   - code-owner review for the `CODEOWNERS` paths (shared files, `contracts/`). This means
     shared-file change requests wait for a person, which is intended;
   - no required approvals otherwise, or the pipeline cannot merge WP PRs.
5. **Actions settings**: allow GitHub Actions to create pull requests; Pages source
   "GitHub Actions" if you want the docs site (`SYRP_PAGES_ENABLED=true`).
6. **Optional variables**: `SYRP_SLOTS` (default 12), `SYRP_IMPLEMENTER_MODEL`,
   `SYRP_REVIEWER_MODEL`, `SYRP_ADVERSARIAL_MODEL`, `SYRP_PR_MANAGER_MODEL` (all default to
   `claude-opus-5-5`).
7. Finally `SYRP_PIPELINE_ENABLED=true`, then run the `conductor` workflow by hand.

## Known gaps

- **Reviewers run PR code.** The review job runs `cargo syrp wp check` on the PR's head in the
  same job that writes the review file. The file is written outside the checkout at an
  unpredictable path, a committed `review.json` is refused, and `review validate` checks the PR
  number and review kind, but a malicious `build.rs` could still find and alter it. The planned
  fix is a separate job without secrets that runs PR code and uploads results, and a reviewer
  that reads those results and the diff without executing PR code.
- **Clean-room network policy** is a hosts-file block of JUCE domains on the runner, not a real
  egress firewall.
