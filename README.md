# Syrp

A Rust reimplementation of the [JUCE](https://juce.com) framework, built and used agent-first.

> **Status: Phase 0 (foundations).** The workspace, the `cargo syrp` CLI, the documentation
> pipeline and the multi-agent pipeline exist. Framework crates are being built now, one work
> package at a time. Nothing here is usable for audio software yet.
>
> Syrp is a working name. Not affiliated with or endorsed by Raw Material Software / JUCE.

## What is here

| | |
|---|---|
| `cargo syrp` | The closed-loop CLI: JSON envelope, stable exit codes, error catalogue, work board, merge gates, scheduler. Runs in-repo via a cargo alias. |
| `work/` | 25 work packages for Phase 0 (waves 0.1 and 0.2) with behaviour specs |
| `docs/` | The documentation site: a human side and an agent side (`cargo syrp docs build`) |
| `.github/workflows/` | CI plus the pipeline: conductor, agents, reviewer, PR manager |
| `docs/design/master-plan.md` | The full plan: architecture, roadmap, testing, licensing, pipeline |
| `docs/design/0002-open-decisions.md` | Decisions taken by default, open for the owner to override |

## Quick start

```sh
cargo syrp doctor
cargo syrp wp ready
cargo syrp check
cargo syrp docs build --skip-book
```

## Turning on the pipeline

The pipeline is off until a person completes the checklist in
[docs/agents/pipeline.md](docs/agents/pipeline.md#before-enabling-the-pipeline): two separate
tokens, the API key, branch protection on `main`, and the `SYRP_PIPELINE_ENABLED` switch.
Then run the `conductor` workflow once by hand. It opens issues for every work package and
starts agents on the ones that are ready.

## Licence

Draft: interface crates are `MIT OR Apache-2.0`; everything else is `AGPL-3.0-only OR BUSL-1.1`,
free for organisations under USD 100k a year. See [LICENSE.md](LICENSE.md).
