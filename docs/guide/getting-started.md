# Getting started

Auricle has no usable framework crates yet (Phase 0). What you can run today is the tooling.

## Requirements

- Rust (the repo pins the version in `rust-toolchain.toml`; rustup installs it automatically).
- Git. Optional: `gh` (GitHub automation), `mdbook` (this site), `cargo-nextest`, `cargo-deny`.

## First commands

```sh
git clone https://github.com/0xkr4t0s/auricle && cd auricle
cargo auricle doctor          # what is installed, what is missing
cargo auricle wp ready        # work packages that can start now
cargo auricle explain AUR-WP-0005
cargo auricle check           # the fast gate every change must pass
```

Inside the repository `cargo auricle` runs through a cargo alias, so nothing needs installing.
Output is human-readable in a terminal and JSON when piped (or with `--json`).

## Building these docs

```sh
cargo auricle docs build      # human site and agent side into target/site
```
