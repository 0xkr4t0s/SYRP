# WP-1.1-10: syrp-net: StreamingSocket, DatagramSocket, IPAddress, MACAddress and NamedPipe

## Purpose
The transport layer of `syrp-net` from master plan §5.2 (L2 Core) and Wave 1.1 item 10. `syrp-osc` uses `DatagramSocket`, `syrp-ipc` builds `InterprocessConnection` on `StreamingSocket` and `NamedPipe`, and network service discovery uses broadcast and the address lists. It creates the `syrp-net` crate; URL and HTTP follow in WP-1.1-09. These types implement `std::io::Read`/`Write` where that fits; they are not `syrp-io` streams (they cannot seek), so this WP needs neither `syrp-io` nor `syrp-thread`. Delivery: 3 PRs (B1–B2, then B3–B4, then B5–B6).

## Behaviour
- B1. `IpAddress`: IPv4 and IPv6 in one value type (convertible to and from `std::net::IpAddr`), `parse`, `Display` (IPv6 in the RFC 5952 compressed form), `any`, `broadcast` and `local` (loopback) for each family, `is_null`, the IPv4-mapped conversions in both directions, ordering and hashing. `all_addresses(include_ipv6)`, `local_address(include_ipv6)` and `interface_broadcast_address(addr)` enumerate interfaces through `getifaddrs` on Unix and `GetAdaptersAddresses` on Windows. Tests: RFC 5952 §4 vectors, parse-display round-trip proptest, and enumeration that includes loopback.
- B2. `MacAddress`: 6 bytes, `to_string_with(separator)` (default `-`, as documented), `parse` accepting `-`, `:` or no separator, `is_null`, and `find_all_addresses()` (excluding null and loopback). Per platform: `getifaddrs` with `AF_PACKET` on Linux and `AF_LINK` on macOS, `GetAdaptersAddresses` on Windows. Containers may have none, so the test asserts success and no null entries, not a count.
- B3. `StreamingSocket` (TCP): `connect(host, port, timeout)`, `create_listener(port, local_host)`, `wait_for_next_connection() -> Result<StreamingSocket>`, `read(buf, block_until_full) -> Result<usize>`, `write`, `wait_until_ready(for_reading, timeout) -> Readiness::{Ready, TimedOut}` (errors are `Err`), `close`, `is_connected`, `is_local`, `host_name`, `port`, `bound_port`, and `SocketOptions` (send and receive buffer sizes) on creation. Built on `std::net` plus `socket2` for options. Tests run over loopback with port 0, on IPv4 and on IPv6 when available (skipped with a message otherwise). `close` from another thread unblocks a blocked read.
- B4. `DatagramSocket` (UDP): `new(enable_broadcasting)`, `bind_to_port(port, local_addr)`, `bound_port`, `read(buf, block) -> Result<(usize, IpAddress, u16)>`, `write(host, port, buf)`, `join_multicast`, `leave_multicast`, `set_multicast_loopback_enabled`, `set_enable_broadcasting` and `wait_until_ready`. Tests use loopback; the multicast test is best effort and skips with a message when the runner has no multicast route.
- B5. `NamedPipe`: `create_new_pipe(name, must_not_exist)`, `open_existing(name)`, `read(buf, timeout: Option<Duration>) -> Result<usize>`, `write(buf, timeout)`, `close`, `is_open` and `name`. One end creates and the other opens; both can read and write. `close` from another thread unblocks a pending read or write. Per platform: on Unix, two FIFOs (`mkfifo`), one per direction, in a per-user directory, with the naming rule documented; on Windows, a byte-mode duplex pipe `\\.\pipe\<name>` with overlapped I/O for timeouts. Tests: two threads in one process, and a child process (the test binary in a helper mode) as the other end.
- B6. Errors use the new `errors/net.toml` domain (`SYRP-NET-0001`...) and carry the OS error code. Tests never use a network beyond loopback. Every `unsafe` block has a `// SAFETY:` comment.

## Platforms
Linux, macOS and Windows are required, and CI must pass B1–B6 on all three. Android and iOS share the Unix paths and are not tested here; on `wasm32` the crate compiles and every constructor returns a documented "unsupported" error.

## Not in this WP
`URL` and HTTP (WP-1.1-09), `InterprocessConnection` and service discovery (`syrp-ipc`), TLS sockets.

## Crate setup
Create `crates/core/net/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-10` first and `cargo syrp wp check WP-1.1-10` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
