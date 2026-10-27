# WP-1.2-12: syrp-unlock: key-file generation and decoding (KeyGeneration)

## Purpose
Wave 1.2 item 12: key-file generation from `juce_product_unlocking` (master plan §5.2, L2 Headless extras; the module table puts key files in phase 1 and the online flow, `OnlineUnlockStatus`, `OnlineUnlockForm` and `TracktionMarketplaceStatus`, in Wave 5.1 item 13). A vendor's server or tool generates a key file with its private RSA key; the product decodes it with the public key and checks the machine. It uses `RsaKey` and the hashes from `syrp-crypto` (WP-1.2-09), `XmlElement` (WP-1.1-15) and `Time` (WP-1.1-12); the online parts that need `syrp-net` come later. This WP creates `syrp-unlock`. Delivery: 2 PRs, each at most 1,200 changed lines (B1–B2, then B3–B5).

## Behaviour
- B1. `key_generation::generate_key_file(app_name, user_email, user_name, machine_numbers, private_key: &RsaKey) -> String` and `generate_expiring_key_file(app_name, user_email, user_name, machine_numbers, expiry: Time, private_key) -> String`, as the `KeyGeneration` documentation describes them: the result is text holding an RSA-encoded block with the key's details, plus human-readable lines. Both have `_at(.., now: Time)` forms so output is deterministic in tests. The layout of the details and of the text is written into the rustdoc.
- B2. Decoding, which `OnlineUnlockStatus` (Wave 5.1) will reuse: `decode_key_file(text, public_key: &RsaKey) -> Result<KeyFileDetails, UnlockError>` returns the app id, user email, user name, machine numbers, creation time and optional expiry time. A wrong key, edited text or malformed block is a coded error. `KeyFileDetails::unlocks(machine_ids: &[String], now: Time) -> bool` checks the machine numbers and expiry as the class documentation describes (a list of machine numbers, any one matching).
- B3. Round trips: a proptest over random names, e-mail addresses (non-ASCII included), machine-number lists and times checks `decode(generate(..))` returns the inputs for keys of several sizes, and that changing any character of the encoded block makes decoding fail or return different details, never a panic.
- B4. JUCE interop: products already in the field decode keys with JUCE's `OnlineUnlockStatus`, so the layout must match JUCE's. The documentation does not give it; add scenario files under `parity/scenarios/unlock/` (the WP-0.1-11 format) in which JUCE generates key files from fixed inputs and a fixed key, and decodes key files Syrp generated. Until the harness runs them, the crate `AGENTS.md` lists the layout under "Open questions" and the ledger row stays partial; do not take the layout from anywhere but the documentation and those observations.
- B5. Errors use the new `errors/unlock.toml` domain (`SYRP-UNLOCK-0001`...); `UnlockError` implements `Diagnostic`. An example `examples/keygen.rs` generates a key pair and a key file from the command line.

## Not in this WP
`OnlineUnlockStatus`, `OnlineUnlockForm`, `TracktionMarketplaceStatus` and `InAppPurchases` (Wave 5.1 and later), machine-id discovery (`getLocalMachineIDs`), and any network access.

## Crate setup
Create `crates/core/unlock/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.
This WP also owns `parity/scenarios/unlock/` for B4.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.2-12` first and `cargo syrp wp check WP-1.2-12` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
