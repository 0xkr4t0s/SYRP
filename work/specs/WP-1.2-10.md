# WP-1.2-10: syrp-osc: addresses and patterns, arguments, messages, bundles, OSCSender and OSCReceiver

## Purpose
Wave 1.2 item 10: `juce_osc` from master plan §4.12 and §5.2 (L2 Headless extras). Master plan §4.12 decides to wrap `rosc` 0.11.4 for the packet codec and build the sender, receiver and pattern matching to JUCE's documented semantics. Phase 1 exit criteria include a differential suite of the OSC wire format, which the OSC 1.0 specification (opensoundcontrol.org) fixes. Sockets come from `syrp-net`'s `DatagramSocket` (WP-1.1-10) and time tags convert to `syrp-sys`' `Time` (WP-1.1-12). Message-thread delivery needs only a `TaskSender` from the frozen `syrp-loop-api` (WP-0.2-01), so this crate does not wait for `syrp-events`. This WP creates `syrp-osc`. Delivery: 2 PRs, each at most 1,200 changed lines (B1–B3, then B4–B6).

## Behaviour
- B1. Addresses: `OscAddress::new(&str) -> Result<OscAddress, OscError>` accepts what the OSC 1.0 address rules and the `OSCAddress` documentation accept (a leading `/`, no space, `#`, `*`, `,`, `?`, `[`, `]`, `{` or `}`), and `OscAddressPattern::new` also accepts the wildcards. `pattern.matches(&address)` implements `?`, `*`, `[abc]`, `[a-z]`, `[!a-z]` and `{foo,bar}` per part between slashes, as the OSC 1.0 specification defines them; `contains_wildcards`; `Display`; equality. Tests: a vector table from the specification and the class documentation, and a proptest that a pattern without wildcards matches exactly the equal address.
- B2. Values: `OscType` (int32, float32, string, blob and RGBA colour, the types the `OSCTypes` documentation lists), `OscArgument` with typed constructors and `is_*`/`as_*` accessors (no public fields), `OscColour` (`r`, `g`, `b`, `a` bytes and its packed `u32` form), and `OscTimeTag` (64-bit NTP time: `immediately()`, `is_immediately`, raw bits, and conversion to and from `syrp_sys::Time`).
- B3. Packets: `OscMessage` (an address pattern and arguments, with `add_*` builders) and `OscBundle` (a time tag and elements that are messages or nested bundles). `encode` and `decode` follow the OSC 1.0 wire format (big-endian, 4-byte alignment, a type tag string). `rosc` 0.11.4 (licence `MIT/Apache-2.0`) may be used inside, but no `rosc` type appears in the public API. An unknown type tag, a bad alignment or a truncated packet is a coded format error (JUCE's `OSCFormatError`) and the packet is dropped. Tests: byte vectors from the specification's examples, an encode-decode proptest, and a proptest that decoding random bytes never panics.
- B4. `OscSender`: `connect(host, port) -> Result`, `connect_to_socket(DatagramSocket, host, port)`, `send(&OscMessage)`, `send_bundle(&OscBundle)`, `send_to(host, port, packet)` and `disconnect`. A packet larger than the documented maximum is a coded error, not a truncated datagram.
- B5. `OscReceiver`: `connect(port) -> Result`, `connect_to_socket(DatagramSocket)` and `disconnect`; a receiver thread reads datagrams. Listeners are added for every packet or for messages whose pattern matches a given `OscAddress`, in one of two modes from the `OSCReceiver` documentation: message-thread (posted through a `TaskSender`, in arrival order, with `&MessageThread`) or realtime (called on the receiver thread). Adding or removing a listener inside a callback is safe, and `disconnect` or drop joins the thread. A format-error callback reports dropped packets.
- B6. Tests send and receive over loopback with port 0, through `ManualLoop` for message-thread delivery; nested bundles, every argument type and pattern listeners are covered. Errors use the new `errors/osc.toml` domain (`SYRP-OSC-0001`...), and `OscError` implements `Diagnostic`.

## Platforms
Linux, macOS and Windows; PR CI runs B1–B6 on all three. No platform code: sockets are `syrp-net`'s.

## Not in this WP
OSC over TCP (SLIP framing), OSC 1.1 extra types beyond those JUCE documents, and the OSC and IPC demos (master plan Wave 5.2).

## Crate setup
Create `crates/core/osc/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.2-10` first and `cargo syrp wp check WP-1.2-10` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
