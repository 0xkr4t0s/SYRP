# WP-1.2-08: syrp-state: juce-compat ValueTree binary and XML layouts and JUCE properties files (black-box)

## Purpose
Wave 1.2 item 8: the `juce-compat` feature of master plan §5.9, "readers and writers for JUCE's ValueTree binary and XML layouts ... specified by black-box observation of harness-produced files (§10). This is how ported plugins keep their sessions." Phase 1 exit criteria require ValueTree binary and XML round trips against the harness. Nothing here may come from JUCE source: every layout fact comes from files that the juce-oracle harness (WP-0.1-11) writes from real JUCE 9.0.4, so this WP waits for it. Master plan §11 lists as an open question whether black-box observation is enough for these formats and whether counsel must confirm an interoperability exception; if the evidence runs out, stop and comment `BLOCKED:` on the WP issue rather than guess. It works in the crate WP-1.2-03 creates, after WP-1.2-06. Delivery: 2 PRs, each at most 1,200 changed lines (B1–B3, then B4–B6).

## Behaviour
- B1. Fixtures: add scenario files under `parity/scenarios/state/formats/` (the WP-0.1-11 format) that make JUCE write, for a set of trees covering every `var` type, empty and deeply nested trees, non-ASCII names and values and many properties: `ValueTree::writeToStream` bytes, `toXmlString` text, the gzip form, and a `PropertiesFile` in each storage format. CI fetches the harness outputs by hash (WP-0.1-11 B3). Record each layout fact you rely on in the module rustdoc next to the scenario that shows it.
- B2. Binary: behind the `juce-compat` cargo feature (off by default; CI tests with it on), `juce_compat::write_binary(store, node) -> Vec<u8>` is byte-identical to every fixture, and `read_binary(store, mt, &[u8], ReadLimits) -> Result<NodeRef, StateError>` reads every fixture to an equivalent tree, with `_gzip` variants. Corrupt or forged input is a coded error, never a panic or an unbounded allocation (byte-flip and truncation proptest), as in WP-1.2-13 B3.
- B3. XML: `juce_compat::to_xml` and `from_xml` produce and read JUCE's ValueTree XML, text-identical to the fixtures through WP-1.1-15's writer, including how each `var` type is written and read back. If the fixtures show that JUCE writes properties in insertion order, which the store does not keep (it sorts them, WP-1.2-03 B1), stop and report it as `BLOCKED:` with the evidence: the spec room then chooses between an order column in the store and a documented gap.
- B4. Round trips: JUCE bytes → Syrp → JUCE bytes is the identity for every fixture, in both formats, and Syrp trees made by the WP-1.2-03 B7 generator survive Syrp → JUCE layout → Syrp, apart from differences the rustdoc lists (for example `var` types JUCE's layout cannot carry).
- B5. Properties files: `PropertiesFile` (WP-1.2-06) gains `StorageFormat::JuceXml`, `JuceBinary` and `JuceCompressedBinary` behind the same feature, reading and writing files byte-identical to the fixtures, so a ported app keeps its users' settings.
- B6. Layering (ADR 0003 D1): this module depends on nothing in `syrp-compat-state` and names no compat type; `syrp-compat-state` may call it. A `docs/migration/` page explains how a ported plugin loads its old sessions. Errors continue `errors/state.toml`.

## Not in this WP
Plugin state envelopes (`AudioProcessor::copyXmlToBinary` and the per-format state chunks, which belong with `syrp-plugin`), JUCE's `var` binary stream format outside ValueTree properties, and `ValueTreeSynchroniser` messages.

## Crate setup
The whole crate `crates/core/state/`, which WP-1.2-03 creates; this WP starts after WP-1.2-06 merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate. This WP also owns `parity/scenarios/state/formats/` for B1.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.2-08` first and `cargo syrp wp check WP-1.2-08` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
