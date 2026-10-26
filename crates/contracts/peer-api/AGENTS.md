# syrp-peer-api

## Purpose
The L1 native window contract (master plan §5.6) between `syrp-ui` and the
`syrp-peer-{win32,appkit,x11,wayland,...}` platform crates. Covers JUCE's ComponentPeer and
Desktop roles, MouseCursor, and the event side of MouseInputSource, KeyPress,
TextInputTarget and external drag-and-drop: `Desktop::create_peer` (top-level, or child
embedded in a host `RawWindowHandle`; the desktop lends a `DisplayHandle`), `Peer` (bounds,
visibility, `WindowState`, title, scale factor, `render_target`, repaint, focus, IME caret,
`CursorIcon`, drop acceptance; lends a `WindowHandle`), `Event` (pointer, wheel, magnify,
`keyboard-types` keys, focus, `ImeEvent`, `DragEvent`, window changes) delivered to an
`EventSink`, `PeerError` (`errors/peer.toml`) and the `headless` desktop for tests. The
desktop also owns clipboard text, `Display`s (bounds, work area, scale) and async
`FileDialog`/`AlertDialog` requests (JUCE's SystemClipboard, Displays, FileChooser,
NativeMessageBox); peers take `accesskit` `TreeUpdate`s and report activation and
`ActionRequest`s as events. No platform code lives here.

## Ledger rows
juce_gui_basics/ComponentPeer, juce_gui_basics/Desktop, juce_gui_basics/MouseInputSource,
juce_gui_basics/TextInputTarget (event side; shared with `syrp-ui`),
juce_gui_basics/MouseCursor, juce_gui_basics/SystemClipboard, juce_gui_basics/Displays,
juce_gui_basics/FileChooser (native dialog), juce_gui_basics/NativeMessageBox

## Invariants
- Contract types have no public fields (ADR 0003 D3); the `b7_` test scans the sources.
  Event payload structs have constructors and `with_` builders so platform crates can
  build them; `Event` is `#[non_exhaustive]` so variants can be added.
- Units: `f64` logical units, y down. Top-level bounds are desktop coordinates; child bounds
  and event positions are relative to the parent / the peer. Scale factor = physical pixels
  per logical unit, as `syrp_gfx_api::RenderTarget`; `Peer::render_target` uses
  `RenderTarget::from_logical`, so rounding matches the renderers.
- Setters record at once and request; the platform reports only what differs (or what the
  user changed) as `BoundsChanged`/`StateChanged`, never echoing an unchanged request. A getter
  returns whichever came last; the headless desktop applies injected events the same way.
- `PeerOptions::child` is `unsafe`: platforms hand the raw parent handle to the OS.
  A child peer accepts only `WindowState::Normal` (`PeerError::Unsupported` otherwise).
- Dropping a peer destroys its window; a dropped peer's sink never runs again.
- Pressure is clamped to `[0, 1]`, tilt per axis to `[-1, 1]`; NaN becomes 0; `None` means
  the device does not report it. `PointerSource` ids stay fixed from `Down` to `Up`.
- Pointer and wheel `time()` is the platform's input time on the loop's monotonic clock;
  `None` means the GUI layer stamps on receipt.
- Drag data is present from `Enter`: XDND and Wayland backends fetch it eagerly.
- Drags: each starts unaccepted; `Drop` is delivered only if the last `accept_drop` was true.
- `Desktop: HasDisplayHandle`, `Peer: HasWindowHandle`; headless returns `NotSupported`.
- `Desktop::displays` is never empty, primary first; a display's scale is finite and
  positive (bad values become 1.0).
- Dialog callbacks run exactly once, later, on the message thread, never inside the
  request; on `Err` they are dropped unrun. An empty path list means cancelled.
- Accessibility: send a full tree after `AccessibilityActivated`, then incremental updates.
  `accesskit` types are re-exported as-is (third-party, so their public fields are theirs).
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
None for the contract: WP-0.2-08 is complete after increment 3. Custom image cursors and
rich clipboard formats (images, files) are left to later WPs if the ledger asks for them.
