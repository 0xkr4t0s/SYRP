# The pipeline

Hands-off by default: work packages are scheduled, implemented, reviewed and merged by agents
and deterministic code. People are an escalation path. The design is master plan §12; this page
describes what is actually wired up.

## Actors

| Actor | Runs as | Can |
|---|---|---|
| **Conductor** | `conductor.yml`, every 30 minutes and after each merge. Deterministic: `cargo syrp conductor tick`. | Open WP issues, take and release leases, start agents, escalate. Cannot push code or merge. |
| **Implementer** | Claude Code in a GitHub Actions job, one per WP, up to `SYRP_SLOTS` (default 12) at once. Prompt: `agents/roles/implementer.md`. | Push to its own `wp/<id>-<slug>` branch and open or update one PR. Edits only its WP's paths. |
| **Spec room** | Same job with `agents/roles/spec-room.md`, for WPs with `role = "spec-room"`. | Write specs, scenarios, inventory. Never reads JUCE source. |
| **Reviewer** | `reviewer.yml` on every ready PR update. Prompt: `agents/roles/reviewer.md`. | Rerun everything in a fresh checkout and write a review JSON. The workflow, not the agent, validates it and sets the `syrp/review` status. |
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
   trailers, CI green, review approved, adversarial approved when risky. All pass: squash-merge.
7. The final PR of a WP says `Closes #<issue>`. The closed issue marks the WP done, and its
   dependants become ready on the next tick.

## Escalations

The conductor labels an issue `needs-human` and comments with a catalogued code:
`SYRP-COND-0001` (restarts exhausted), `SYRP-COND-0002` (review rounds exhausted),
`SYRP-COND-0003` (a WP only a person can do). Everything else keeps running.

## Set-up the pipeline needs (one time, by a person)

See [README.md](https://github.com/0xkr4t0s/SYRP#turning-on-the-pipeline) in the repository.
