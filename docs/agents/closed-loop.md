# The closed loop

The same loop serves implementers, reviewers and (later) downstream users' agents.

```text
wp show <id>  ->  write failing tests from the spec  ->  implement
     ^                                                      |
     |                                                      v
  explain <code>  <-  diagnostics  <-  check  ->  test --crate <pkg>
                                                      |
                                                      v
                                           wp check <id>  (acceptance, exit 0)
```

## Commands available now

| Command | Use |
|---|---|
| `cargo auricle doctor` | Toolchain and repo health |
| `cargo auricle check [--lints-only]` | Layering, file size, crate metadata, banned crates, WP specs, fmt, clippy |
| `cargo auricle test [--crate <pkg>]` | Tests with pass/fail counts |
| `cargo auricle wp list / ready / show <id> / validate` | The work board |
| `cargo auricle wp check <id>` | The acceptance command: check, the WP crate's tests, path ownership |
| `cargo auricle wp check-paths <id>` | Only the ownership part |
| `cargo auricle explain <code>` / `--list` | Error catalogue |
| `cargo auricle docs build / search` | Docs |
| `cargo auricle review checklist / validate <file>` | Reviewer harness |
| `cargo auricle pr gates --pr <file>` | Merge-gate evaluation (PR manager) |
| `cargo auricle conductor tick / finish` | Scheduler (conductor workflow) |
| `cargo auricle schema` | The envelope schema |

Commands from the master plan that do not exist yet (`render-audio`, `render-ui`, `parity`,
`validate`, `host`, `inspect`, `rt-check`, `migrate`) arrive with the work packages that need them.
