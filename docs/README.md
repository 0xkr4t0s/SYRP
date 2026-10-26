# Syrp

Syrp is a Rust reimplementation of the JUCE framework: audio apps, plugins, DSP and GUIs, with
JUCE's capabilities and idiomatic Rust APIs. It is built mostly by AI agents, and it is designed to
be used by them: every tool emits JSON with stable exit codes, every error has a catalogued code
with a fix, and the documentation has a side written for agents.

**Status: Phase 0.** The workspace, the `cargo syrp` CLI and the agent pipeline exist. The
framework crates are being written now, one work package at a time. Nothing is usable for
building audio software yet.

This site has two halves:

- **Guide, cookbook and migration** for people.
- **For agents**: the closed-loop workflow, the pipeline and the machine-readable files under
  [`/agents/`](agents/README.md) (`llms.txt`, `errors.json`, `work-packages.json`).
