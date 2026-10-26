# syrp-peer-api

## Purpose
The L1 native window contract (master plan §5.6) between `syrp-ui` and the
`syrp-peer-{win32,appkit,x11,wayland,...}` platform crates. Covers JUCE's ComponentPeer and
Desktop roles and the event side of KeyPress and TextInputTarget (MouseInputSource next):
`Desktop::create_peer` (top-level, or child embedded in a host `RawWindowHandle`), `Peer`
(bounds, visibility, `WindowState`, title, scale factor, `render_target`, repaint, focus, IME
caret), `Event` (`keyboard-types` keys, focus, `ImeEvent`, window changes) delivered to an
`EventSink`, `PeerError` (`errors/peer.toml`) and the `headless` desktop for tests. No platform code lives here.

## Ledger rows
juce_gui_basics/ComponentPeer, juce_gui_basics/Desktop, juce_gui_basics/MouseInputSource,
juce_gui_basics/TextInputTarget (event side; shared with `syrp-ui`)

## Invariants
- Contract types have no public fields (ADR 0003 D3); the `b7_` test scans the sources.
  Event payload structs (from increment 2) get constructors and `with_` builders so platform
  crates can build them; `Event` is `#[non_exhaustive]` so variants can be added.
- Units: `f64` logical units, y down. Top-level bounds are desktop coordinates; child bounds
  and event positions are relative to the parent / the peer. Scale factor = physical pixels
  per logical unit, as `syrp_gfx_api::RenderTarget`; `Peer::render_target` uses
  `RenderTarget::from_logical`, so rounding matches the renderers.
- Setters are requests; the platform reports adjustments as `BoundsChanged`/`StateChanged`.
  A child peer accepts only `WindowState::Normal` (`PeerError::Unsupported` otherwise).
- Dropping a peer destroys its window; a dropped peer's sink never runs again.
- IME: `Preedit` ranges are byte ranges into the preedit string. `set_ime_caret(None)`
  turns IME input off.

## Thread affinity
Message thread only: every setter and `Desktop::create_peer` takes `&MessageThread`; sinks
run on the message thread. Peers and the headless desktop are `!Send`. Nothing is RT-safe.

## Stability
contract (frozen in `contracts/syrp-peer-api.public-api.txt` once final, committed by the PR
manager).

## How to test
```sh
cargo syrp test --crate syrp-peer-api
```

## Known gaps
Increment 1 of WP-0.2-08. Still to come in this WP: pointer events (mouse, touch, pen with
pressure, tilt and source ids), wheel, magnify and drag-and-drop of files and text (B2),
clipboard, cursors (`cursor-icon`), display enumeration with work areas and scale, async
native file and alert dialogs (B4), and `accesskit` tree updates (B5), each with headless
support.
