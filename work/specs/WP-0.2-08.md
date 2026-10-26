# WP-0.2-08: syrp-peer-api: window peer, input, IME, clipboard, cursors, displays, dialogs

## Purpose
The native window boundary (JUCE: ComponentPeer, Desktop, MouseInputSource; master plan §5.6). Platform crates implement it.

## Behaviour
- B1. `trait Peer`: create top-level or child (embedded in a host-provided `raw-window-handle` parent) windows; bounds, visibility, minimise/maximise/fullscreen, title, scale factor, repaint requests.
- B2. Input events: mouse/touch/pen with pressure, tilt and source ids; wheel and magnify; keyboard via `keyboard-types`; focus; drag-and-drop of files and text.
- B3. IME: composition start/update/commit events and caret rectangle reporting.
- B4. Clipboard, cursors (`cursor-icon`), display enumeration with work areas and scale, native file/alert dialogs as async requests.
- B5. Accessibility: peers accept `accesskit` tree updates.
- B6. A `headless` peer for tests that records requests and lets tests inject events.
- B7. Contract freeze as in WP-0.2-01.

## Crate setup
Create `crates/contracts/peer-api/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-08` first and `cargo syrp wp check WP-0.2-08` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
