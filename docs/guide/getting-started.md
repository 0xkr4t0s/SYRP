# Getting started

Syrp has no usable framework crates yet (Phase 0). What you can run today is the tooling.

## Requirements

- Rust (the repo pins the version in `rust-toolchain.toml`; rustup installs it automatically).
- Git. Optional: `gh` (GitHub automation), `mdbook` (this site), `cargo-nextest`, `cargo-deny`.

## First commands

```sh
git clone https://github.com/SYRP-Foundation/SYRP && cd SYRP
cargo syrp doctor          # what is installed, what is missing
cargo syrp wp ready        # work packages that can start now
cargo syrp explain SYRP-WP-0005
cargo syrp check           # the fast gate every change must pass
```

Inside the repository `cargo syrp` runs through a cargo alias, so nothing needs installing.
Output is human-readable in a terminal and JSON when piped (or with `--json`).

## Building these docs

```sh
cargo syrp docs build      # human site and agent side into target/site
```
