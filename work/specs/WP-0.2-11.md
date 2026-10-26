# WP-0.2-11: Agent Skill v1: recipes, references and loop script

## Purpose
Grow `skills/auricle/` (seeded at bootstrap) into the Agent Skill described in master plan §13.3.

## Behaviour
- B1. Recipes in `skills/auricle/references/recipes/`: start-a-work-package, add-error-code, create-a-crate, fix-failing-check, respond-to-review. Each has goal, preconditions, steps (real commands), verification and common errors.
- B2. `scripts/loop.sh` runs check, then test for an optional crate, and prints one JSON summary; exit code follows the CLI's.
- B3. References: `references/errors.json` generated from the catalogue by a script you add (`scripts/refresh.sh` calling `cargo auricle docs build --skip-book`).
- B4. Every command quoted in the skill exists: add a test script that runs `cargo auricle <cmd> --help` for each one.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo auricle wp show WP-0.2-11` first and `cargo auricle wp check WP-0.2-11` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
