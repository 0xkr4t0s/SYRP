# Work packages

A work package (WP) is one crate or sub-module, 1.5k to 6k lines with tests, delivered in 1 to 4
PRs of at most 1,200 changed lines. Definitions live in `work/packages/<id>.toml` (schema:
`work/wp.schema.json`); behaviour specs in `work/specs/<id>.md`; waves in `work/waves/`.

```toml
id = "WP-0.1-04"
title = "syrp-rt: RT tokens, RtSafe, SPSC queues, deferred drop, atomics, allocation guard"
crate = "crates/foundation/rt"          # owned directory
depends_on = ["WP-0.1-03"]              # must be merged first
spec = "work/specs/WP-0.1-04.md"
risk = ["public-api", "unsafe"]         # any entry => adversarial review
acceptance = "cargo syrp wp check WP-0.1-04 --json"
role = "implementer"                    # implementer | spec-room | pr-manager | oracle | human
status = "planned"                      # done = finished outside the pipeline
budget = { agent_hours = 24, max_review_rounds = 3, max_restarts = 2 }
```

## Conventions

- Branch: `wp/<id>-<slug>`, for example `wp/WP-0.1-04-rt`.
- PR title: `[<id>] <crate>: <what>`.
- PR body starts with the front matter marker:
  `<!-- syrp-pr: {"wp":"WP-0.1-04","final":false,"risk":["unsafe"],"deps_added":["rtrb"],"unsafe_count":3} -->`
- The PR that completes the WP sets `"final": true` and says `Closes #<issue>`.
- Every commit carries trailers:

```text
WP: WP-0.1-04
Spec: work/specs/WP-0.1-04.md
Agent-Model: <model id>
```

- Tests are named after spec behaviours: `b3_try_push_fails_when_full`.
