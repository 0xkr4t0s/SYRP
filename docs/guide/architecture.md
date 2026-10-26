# Architecture

The full design is in the [master plan](../design/master-plan.md) (§5). In short:

- **Layers L0 to L7.** Foundation crates (L0) and frozen contract crates (L1, the `*-api`
  crates) sit at the bottom. Core runtime (L2), audio, graphics and platform peers (L3), UI (L4),
  plugins and extras (L5), tooling and the facade (L6), and language bindings (L7) build on them.
  A crate may only depend on lower layers, and leaf crates in the same group never depend on each
  other. `cargo auricle check` enforces both.
- **Contracts first.** Every boundary that several work packages touch is a small trait-and-type
  crate, frozen as a public-API snapshot in `contracts/` before implementations start. This is
  what lets 10 to 15 agents build leaves in parallel.
- **One crate per work package.** Crates are small (2k to 8k lines) and each has one owner at a
  time, recorded in `work/packages/` and `ownership.toml`.
- **Threading is in the types.** Message-thread APIs take a `MessageThread` token; audio code gets
  an `RtCtx` that exposes no allocating APIs.
- **Deterministic testing.** `ManualLoop` drives timers on a virtual clock, renders are offline and
  bit-reproducible, and parity is checked against a separate harness built on real JUCE.

## Repository map

| Path | What |
|---|---|
| `crates/<group>/<crate>/` | Workspace crates (members are globbed) |
| `work/` | Work packages, waves and behaviour specs |
| `errors/` | The error-code catalogue |
| `parity/` | Parity ledger and scenarios |
| `contracts/` | Frozen public-API snapshots |
| `docs/` | This site's sources |
| `skills/auricle/` | The Agent Skill |
| `agents/` | Role prompts for the pipeline's agents |
| `.github/workflows/` | CI and the pipeline (conductor, reviewer, PR manager) |
