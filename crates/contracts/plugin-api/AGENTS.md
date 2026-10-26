# syrp-plugin-api

## Purpose
The L1 processor and plugin contract (master plan §5.4 and §5.8, ADR 0003 D2/D3):
`Processor`, its `PrepareCtx`/`RtCtx` tokens, `ProcessIo` (in-place f32/f64 audio, the
`ParamChanges` stream, `syrp-midi` MIDI 1.0 and UMP buffers) and the parameter model
(`ParamRange`, `ParamSpec`, `ParamLayout`, `Params`), plus `Editor`, `PluginDescriptor`, `PluginState`.

## Ledger rows
juce_audio_processors/NormalisableRange, juce_audio_processors/AudioParameterFloat,
juce_audio_processors/AudioProcessorParameterGroup (rows arrive when the spec room adds them)

## Invariants
- `ParamLayout::register` hands out `ParamIndex(0)`, `(1)`, ... in registration order, rejects
  duplicate ids (`SYRP-PLUGIN-0001`) and invalid specs (`SYRP-PLUGIN-0002`), and is unchanged
  on failure. String ids resolve through `ParamLayout::index_of` only; `Params` has no id lookup.
- `Params` is one contiguous `Box<[AtomicF32]>` of plain values, initialised to the defaults
  and reserved with `try_reserve_exact` (`SYRP-PLUGIN-0003` on failure).
- Only `prepare_processor` creates a `PrepareCtx`; it is borrowed for one `prepare` call and
  is not `Send`. `process` gets `ProcessIo`, `&Params` and `RtCtx`, none of which resolve ids
  (`tests/ui/` proves both with trybuild).
- `ParamChanges` is sorted by offset, equal offsets in push order; `try_push` never allocates
  and returns `SYRP-PLUGIN-0004` when full, leaving the stream unchanged.
- `ParamRange` maths is `f64`. `normalise`/`denormalise` clamp, never snap, and never panic,
  even on an invalid range. Round trips hold to 1e-6 in the well-conditioned direction (see
  the `b3_` proptest).
- Contract types have no public fields (ADR 0003 D3); the `b8_` test scans the sources.

## Thread affinity
RT-safe: `process`, `process_bypassed`, `reset`, `ProcessIo`, `RtCtx`, `ParamChanges` (except
`with_capacity`), `Params` getters and setters, `ParamRange`. Everything else (registration,
negotiation, `prepare_processor`, editors, state, construction, drop) is message thread / prepare time.

## Stability
contract (frozen in `contracts/syrp-plugin-api.public-api.txt` after review).

## How to test
```sh
cargo syrp test --crate syrp-plugin-api
```
`tests/params.rs` installs `syrp_rt::GuardedAllocator`, so an allocation inside `no_alloc`
aborts the test binary. B3 round trips use proptest.

## Known gaps
All of B1-B7 is in place. `prepare_processor` is safe, so `process` could
call it on itself: documented misuse the types do not forbid. Not planned: bool/choice/int
shorthands, TOML and derive declaration (master plan §5.8).
