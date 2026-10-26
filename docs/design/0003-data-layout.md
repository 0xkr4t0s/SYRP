# 0003: Data layout, API layering and the DSP execution model

- **Status:** accepted by the project owner on 2026-10-26.
- **Applies to:** every crate that runs on the audio thread or in per-frame UI passes, the four Wave 0.2 contract crates (`syrp-audio-api`, `syrp-midi`, `syrp-plugin-api`, `syrp-gfx-api`), and the planned `syrp-ui`, `syrp-state`, `syrp-audio-basics`, `syrp-dsp-*` and `syrp-processors` crates.
- **Amends:** master plan §1, A17, A18, §4.9, §5.2, §5.3, §5.6, §5.8 and §5.9.
- **Research page:** the background, precedents and estimates behind this record are on the owner's
  "SYRP Data Layout" page. This file is the normative version.

## Context
Syrp targets feature parity with JUCE 9.0.4 and wants two things that pull in different directions:

1. The best achievable performance for new plugins and apps, which means data-oriented layouts:
   contiguous arrays, integer handles instead of pointers, struct-of-arrays where passes touch a
   few fields of many items, and SIMD across voices and channels.
2. An easy port for existing JUCE code, whose object model is the opposite: Component subclasses
   with virtual hooks, ValueTree nodes shared by reference, one object per synth voice and one
   filter object per channel.

Parity was already defined as functional, not source-compatible (master plan §1). This record
decides how the object model and the data-oriented core coexist.

## Decision

### D1. One data core, two front-ends
Syrp has exactly one storage model, the data-oriented core. There are two ways to program against
it:

- **Native API** (core crates). Handles, views and plain-data descriptions. New plugins and apps use
  it and get full performance.
- **Compatibility API** (`syrp-compat-*` crates, optional). JUCE-shaped types such as `Component`,
  `LookAndFeel`, `ValueTree`, `Value`, `SynthesiserVoice` and per-channel processor duplication.
  Ported code uses it.

The model is Godot's nodes over servers, not Unity's GameObjects beside DOTS. Unity keeps two
stores and converts ("bakes") between them, and has been working to unify them since DOTS shipped.
Syrp must never have a second store. Three rules enforce that:

1. **Compat owns no storage.** A compat object is a handle into core storage plus, where needed, a
   boxed behaviour object. A ported `Component`'s bounds, transform, flags and focus state live in
   the same UI node columns as a native widget's. Nothing is mirrored or synchronised per frame.
2. **Dependencies point one way.** `syrp-compat-*` depends on core crates. No core crate depends on,
   or knows about, a compat crate. Contracts in `contracts/` are written against core types only.
   A build that does not enable the compat features compiles none of it.
3. **Every compat object has an escape hatch.** `.handle()` returns the core handle, so a port can
   move one hot object at a time to the native API while the rest stays as it is.

Both front-ends can be mixed in one tree, one graph and one state document.

### D2. Hot-path storage is data-oriented
| Area | Storage | Decided in |
|---|---|---|
| Audio buffers | One allocation per buffer; each channel starts on a 64-byte boundary; channel stride in bytes is a multiple of 64 (16 `f32` or 8 `f64` samples). Views carry the stride. | WP-0.2-02 |
| Parameters | Dense `ParamIndex(u32)` assigned at registration; values in one contiguous atomic array; one sorted change stream per block. String ids resolve to indices at prepare time only. | WP-0.2-06 |
| MIDI buffers | Packed: an array of fixed-size event heads plus one byte arena (or `u32` word arena for UMP). No per-event allocation, SysEx included. Iteration yields borrowed views. | WP-0.2-03 |
| Display list | An op-tag stream with per-kind payload arrays and shared arenas for path verbs, points, glyph runs and gradient stops, addressed by index. | WP-0.2-07 |
| Graph plan | `compile()` emits flat arrays: steps in topological order, buffer-slot indices, a pooled buffer allocation reused by liveness, pre-sized delay lines, dependency levels for a later parallel scheduler. | `syrp-processors` |
| UI nodes | A node table: a slotmap from `ComponentId` to a dense row, then columns for bounds, world transform, flags, parent and sibling links, widget kind and properties, and a cold column for boxed behaviour. Rows kept in tree pre-order so a subtree is a contiguous range. | `syrp-ui` |
| State | Typed schema store with interned identifiers and a change log as the source of truth (see §5.9). | `syrp-state` |
| Voices | `VoicePool` running voices in SIMD lane packs (D5). | `syrp-audio-basics` |

Cold paths (strings, files, networking, crypto, scanning, dialogs, app lifecycle) use ordinary
idiomatic Rust. Data-oriented layout is not applied there.

### D3. Contracts freeze shapes, not storage
Frozen L1 types on hot paths expose constructors, accessors, views, handles, indices and iterators.
They have **no public fields**, so their storage can change without a contract-change WP.
Small value types that are their own representation (`Point`, `Rect`, `ParamIndex`, an event head
read through accessors) are exempt.

### D4. No ECS framework
Syrp does not depend on `bevy_ecs`, `hecs` or similar in any core crate. The data counts are small
and fixed in shape (tens of graph nodes, hundreds of UI nodes, tens of voices), an ECS scheduler
conflicts with the `MessageThread` and `RtCtx` model, archetype moves allocate, and ECS types would
leak into frozen contracts. The patterns (columns, handles, passes) are used without a framework.
Struct-of-arrays types are written by hand inside one owning type; no derive crate is used for them.

### D5. DSP execution model
The goal is to make the gap between a ported plugin and a native one close to zero, and to cut the
DSP work itself where the framework can.

**Lane-generic DSP.** Voices, filters and other per-instance DSP are written against a `Sample`
trait instead of a concrete `f32`. The trait is implemented for `f32`, `f64` and SIMD lane packs
(`F32x4`, `F32x8`, `F32x16`, chosen at runtime). A struct generic over its sample type,
`struct SawVoice<S: Sample> { phase: S, increment: S, env: Adsr<S> }`, is its own lane pack:
`SawVoice<F32x8>` is eight voices in struct-of-arrays form. No macro is needed. Divergent control
flow (envelope stages, voice start and stop) is written with masks and `select`, which the trait
provides; the Agent Skill has a recipe for it.

**Batching.** `VoicePool<V>` stores voices as lane packs, assigns notes to lanes, and renders a
whole pack per call. Graph nodes of the same built-in kind can be batched the same way by the plan
compiler. The compat layer groups compat voices and duplicated per-channel processors of the same
concrete type and calls them once per pack when they are lane-generic, and once per object, with a
scalar sample type, when they are not.

**Porting step.** A straight port keeps `f32` and runs scalar, at about the speed of the same code
on JUCE. Changing the voice or filter to be generic over `S: Sample` is a mechanical edit, and after
it the compat layer runs it in packs. That edit is the main thing `cargo syrp migrate` suggests for
hot DSP.

**Cutting the work itself.**
- *Sleep.* Voices that have finished, graph nodes whose inputs are silent and whose tail has
  ended, and bypassed nodes are skipped. Processors report tail length (already in §5.8); the graph
  tracks silence per buffer slot.
- *Denormals.* Flush-to-zero and denormals-are-zero are set for the duration of every `process`
  call on every platform that has them, and restored after.
- *Control rate.* Parameter smoothing and coefficient updates can run at a control rate (every 16 or
  32 samples) chosen per processor, with per-sample interpolation only where the spec requires it.
- *Lane filling.* Packs combine voices and channels (four stereo voices fill eight lanes) so stereo
  material does not leave most lanes empty.

**Limits.** Recursive filters are serial along time, so SIMD comes from channels or voices, never
from samples within one channel. A single mono voice gains little from SIMD; sleeping and control
rate matter more there.

**SIMD crate.** SIMD uses `fearless_simd` (Apache-2.0 OR MIT), with runtime dispatch across SSE4.2,
AVX2, AVX-512, NEON and WASM SIMD. Vello already depends on it. `std::simd` is not used while it is
unstable. A new L0 crate, `syrp-simd`, owns the `Sample` trait, lane packs, aligned allocation, the
denormal guard and dispatch, so DSP crates do not each reinvent them. Every SIMD kernel has a scalar
implementation that the parity tests treat as the reference, compared at the `f32-arith` tolerance
profile (§9.3).

### D6. Porting path
1. Port as-is on the compat crates. The migration map (A18) points JUCE classes at compat types
   where the object model differs, and at core types where it does not (buffers, MIDI, parameters,
   `Processor`).
2. Profile. `cargo syrp` reports compat objects on hot paths.
3. Swap hot pieces through `.handle()` or by making them lane-generic.
4. Go native whenever you like. New projects start native; the Agent Skill teaches native first.

### D7. Verification
- Each column-owning type has a proptest that applies random operation sequences and checks that
  all columns have equal length and that links and handles stay valid.
- Hot crates carry instruction-count benchmarks (Callgrind-based, stable on CI) with budgets in
  their WP acceptance.
- Each lane-generic DSP type is tested with the scalar and every pack type and must agree within
  tolerance.

## Expected costs of the compatibility layer
Order-of-magnitude estimates from typical desktop CPU costs, to be replaced by the D7 benchmarks.
Budgets: 2,667 µs per audio block (128 samples at 48 kHz), 16,667 µs per UI frame at 60 Hz.

| Compat use | Extra work | Estimate |
|---|---|---|
| `Processor` per block | None; both front-ends share the trait | 0 |
| Any compat call | Slotmap index and generation check | ~1–2 ns |
| 500 ported components | One dynamic `paint` call each per repaint | ~2–5 µs per frame |
| ValueTree view | Interned-id lookup in a small sorted list | ~5–20 ns per property access, message thread |
| Scalar voices or per-channel filters | No SIMD across instances | roughly 4–8× slower than lane-generic code on that DSP, and about equal to the same code on JUCE |

## Alternatives considered
- **Keep JUCE's object model as the core, data-oriented internals only.** Keeps porting simplest,
  but limits UI and state to object-per-node designs and makes SIMD voices opt-in rather than the
  default. Rejected.
- **Own design with no JUCE-shaped layer.** Fastest and simplest core, but turns every port into a
  rewrite and forces the ledger and migration map to feature level. Rejected.
- **Two worlds with conversion (Unity's model).** Rejected for the sync cost, conversion step and
  two hierarchies (D1).

## Consequences
- The four Wave 0.2 specs are amended before any agent claims them (done in the same change as
  this record).
- The spec room adds work packages for `syrp-simd` (Phase 1), `syrp-compat-audio` (with
  `syrp-audio-basics`), `syrp-compat-state` (with `syrp-state`) and `syrp-compat-ui` (with
  `syrp-ui`), and a ValueTree semantics spike before the `syrp-state` WP.
- Shared files need change requests to the PR manager: `docs/SUMMARY.md` (link this record) and,
  later, the hot-crate benchmark gate in CI.
