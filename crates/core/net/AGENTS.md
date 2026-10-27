# syrp-net

## Purpose
The L2 networking transport (master plan §5.2): IP and MAC addresses with interface
enumeration, TCP (`StreamingSocket`) and UDP (`DatagramSocket`) sockets, and `NamedPipe`.
Covers JUCE's IPAddress, MACAddress, StreamingSocket, DatagramSocket, SocketOptions and
NamedPipe. URL and HTTP come from WP-1.1-09. Behaviour comes from the WP spec and the public
JUCE class documentation only.

## Ledger rows
juce_core/IPAddress, juce_core/MACAddress, juce_core/StreamingSocket, juce_core/DatagramSocket,
juce_core/SocketOptions, juce_core/NamedPipe

## Invariants
- `IpAddress` wraps `std::net::IpAddr` (private field, ADR 0003) and converts losslessly.
  Display is std's RFC 5952 form; ordering is IPv4 before IPv6, then bytes. The IPv6
  "broadcast" is `ff02::1` (IPv6 has no broadcast). The default value is `0.0.0.0`.
- All OS interface code is in `src/ifaces.rs`: `getifaddrs` on Linux, Android and Apple,
  `GetAdaptersAddresses` on Windows (IPv4 broadcast computed from the on-link prefix).
  Other targets return `NetError::Unsupported`.
- Socket addresses from the OS are read with `read_unaligned`; every `unsafe` block has a
  `// SAFETY:` comment (clippy `undocumented_unsafe_blocks`).
- `StreamingSocket` (`src/socket.rs`, socket2) takes `&self` everywhere; blocking waits poll
  in 50 ms slices so `close` from another thread ends them with `NetError::Closed`. Writes
  never raise `SIGPIPE` (`MSG_NOSIGNAL` on Linux, `SO_NOSIGPIPE` on Apple).
- `SocketOptions` is a `Copy` value applied once, at creation, to TCP and UDP sockets.
- `DatagramSocket` (`src/datagram.rs`) is IPv4 only (Syrp's choice) and shares the 50 ms
  wait loop with `StreamingSocket`. Reads use socket2's vectored receive, so an oversized
  datagram is truncated on every OS. On Unix `bind_to_port` sets `SO_REUSEADDR`, so several
  multicast receivers can share a port.
- Errors are `NetError` (`SYRP-NET-*`, `errors/net.toml`); OS failures carry the OS code.

## Thread affinity
Any thread. Nothing here is RT-safe: enumeration allocates and makes system calls.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-net
cargo check -p syrp-net --target x86_64-apple-darwin
cargo check -p syrp-net --target x86_64-pc-windows-msvc
```
Tests use loopback only and never a real network. Enumeration tests assert that loopback is
listed, never a count of other interfaces.

## Known gaps
WP-1.1-10 increment 3 (B1-B4). Still to come: `NamedPipe` (B5) and the remaining B6 codes. Socket types are
absent on targets other than Unix and Windows (socket2 does not build there).
`errors/net.toml` is not yet in the CLI's embedded catalogue.
