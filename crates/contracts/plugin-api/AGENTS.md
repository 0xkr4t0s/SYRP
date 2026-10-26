# syrp-plugin-api

## Purpose
The L1 processor and plugin contract (master plan §5.4 and §5.8, ADR 0003 D2/D3). This
increment holds the parameter model: `ParamRange` (JUCE's NormalisableRange: step, skew,
centre, symmetric skew), the `ParamSpec` builder (JUCE's AudioParameterFloat and the
ranged-parameter family), `ParamLayout` (registration, dense `ParamIndex`) and `Params`
(runtime values). `PluginError` carries the `SYRP-PLUGIN-*` codes in `errors/plugin.toml`.

## Ledger rows
juce_audio_processors/NormalisableRange, juce_audio_processors/AudioParameterFloat,
juce_audio_processors/AudioProcessorParameterGroup (rows arrive when the spec room adds them)

## Invariants
- `ParamLayout::register` hands out `ParamIndex(0)`, `(1)`, ... in registration order, rejects
  duplicate ids (`SYRP-PLUGIN-0001`) and invalid specs (`SYRP-PLUGIN-0002`), and is unchanged
  on failure. String ids resolve through `ParamLayout::index_of` only; `Params` has no id lookup.
- `Params` is one contiguous `Box<[AtomicF32]>` of plain values, initialised to the defaults
  and reserved with `try_reserve_exact` (`SYRP-PLUGIN-0003` on failure).
- `ParamRange` maths is `f64`. `normalise`/`denormalise` clamp, never snap, and never panic,
  even on an invalid range. Round trips hold to 1e-6 in the well-conditioned direction (see
  the `b3_` proptest).
- Contract types have no public fields (ADR 0003 D3); the `b8_` test scans the sources.

## Thread affinity
RT-safe: `Params::{get, try_get, set, len}`, `ParamIndex` (`RtSafe`), every `ParamRange`
method. Message thread / prepare time: `ParamSpec`, `ParamLayout`, `Params::new`, and
dropping `Params`. Any thread: reading and writing `Params` values.

## Stability
contract (frozen in `contracts/syrp-plugin-api.public-api.txt` after review).

## How to test
```sh
cargo syrp test --crate syrp-plugin-api
```
`tests/params.rs` installs `syrp_rt::GuardedAllocator`, so an allocation inside `no_alloc`
aborts the test binary. B3 round trips use proptest.

## Known gaps
WP-0.2-06 ships in increments. Next: the rest of B4 (the sorted, non-allocating per-block
`ParamChanges` stream) with B1/B2/B7: the `Processor` trait with the `PrepareCtx` and
`RtCtx` tokens (trybuild compile-fail tests), `ProcessIo` with MIDI/UMP event I/O, bus layout
negotiation, latency, tail, bypass and precision. Then B5/B6: `Editor`, `NodeHandle`,
`EditorHost`, `ParamHandles`, `PluginDescriptor` and the versioned state traits. Not planned:
bool/choice/int shorthands, TOML and derive declaration (master plan §5.8).
