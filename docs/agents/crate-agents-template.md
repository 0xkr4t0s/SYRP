# Crate AGENTS.md template

Every crate has an `AGENTS.md` with these headings (checked by `cargo auricle check`, which
requires the file; heading checks arrive with WP-0.1-12). Keep it under about 1,500 tokens: it
becomes the crate's agent brief.

```markdown
# <crate name>

## Purpose
One paragraph. Which JUCE classes or concepts it covers.

## Ledger rows
juce_module/Class, ...

## Invariants
Rules that must always hold (ordering, ownership, units).

## Thread affinity
Which APIs need `MessageThread`, which are RT-safe, which are any-thread.

## Stability
experimental | contract (frozen) | stable

## How to test
The exact commands, plus anything special (features, nightly, loom).

## Known gaps
What is missing and which WP will add it.
```
