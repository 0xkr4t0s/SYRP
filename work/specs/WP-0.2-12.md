# WP-0.2-12: Docs generator v2: crate briefs, migration-map scaffold, error catalogue pages

## Purpose
Second docs generator increment (master plan §13.1), extending the crate from WP-0.1-10.

## Behaviour
- B1. Per-crate agent briefs (at most about 1,500 tokens) built from each crate's AGENTS.md plus its top API items.
- B2. Error catalogue pages: one Markdown page per domain for the human site, plus `errors.json` for agents.
- B3. Migration map scaffold: `docs/migration/*.md` front matter (`juce: juce::Slider`, `syrp: syrp_widgets_slider::Slider`, `notes`) compiled into `migration-map.json`; empty entries allowed.
- B4. A `check-refs` function that fails when a doc or spec cites a Rust path that is not in the API index.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-12` first and `cargo syrp wp check WP-0.2-12` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
