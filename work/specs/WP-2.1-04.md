# WP-2.1-04: syrp-audio-basics: the AudioSource family (memory, tone, mixer, resampling, remapping, IIR and reverb sources)

## Purpose
Wave 2.1 item 4: the `AudioSource` family from `juce_audio_basics` (master plan §3 and §5.2). Pull-based sources are what `AudioSourcePlayer`, `AudioTransportSource` (`syrp-devices`) and the format reader sources (`syrp-formats`) build on. `BufferingAudioSource` needs a background thread and belongs with `syrp-formats` ("buffering", master plan §5.2), so it is not here. It uses `AudioBuffer` (WP-2.1-01), the interpolators, `IirFilter` and `Reverb` (WP-2.1-16), and `syrp-rt`'s handoff and deferred drop. Delivery: 3 PRs, each at most 1,200 changed lines (B1–B2, then B3, then B4–B6).

## Behaviour
- B1. Traits: `AudioSourceChannelInfo<'a>` wraps an `AudioBlockMut<'a, f32>` with a start frame and frame count and has `clear_active_region()`. `trait AudioSource: Send` has `prepare_to_play(samples_per_block_expected, sample_rate)`, `release_resources()` and `get_next_audio_block(&mut self, &mut AudioSourceChannelInfo)`, which fills exactly the active region, as the `AudioSource` documentation describes. `trait PositionableAudioSource: AudioSource` adds `set_next_read_position(i64)`, `next_read_position()`, `total_length()`, `is_looping()` and `set_looping(bool)`. Both are object-safe, and `Box<dyn AudioSource>` implements `AudioSource`.
- B2. Simple sources: `MemoryAudioSource` (plays an owned `AudioBuffer<f32>`, positionable and loopable, zero past the end when not looping), `ToneGeneratorAudioSource` (sine with `set_amplitude` and `set_frequency`, default values as documented), `ChannelRemappingAudioSource` (input and output channel maps with `set_input_channel_mapping`, `set_output_channel_mapping`, `clear_all_mappings` and the documented behaviour for unmapped channels), `IirFilterAudioSource` (one `IirFilter` per channel, `set_coefficients`, `make_inactive`) and `ReverbAudioSource` (`set_parameters`, `set_bypassed`).
- B3. `MixerAudioSource`: `add_input_source(Box<dyn AudioSource>)`, `remove_input_source(id)` and `remove_all_inputs()` from the message thread; the new set reaches the audio thread through `syrp_rt::RtHandoff`, and removed sources are dropped off the audio thread through `syrp_rt::DeferredDrop`, so `get_next_audio_block` never locks, allocates or frees. A source added to a prepared mixer is prepared first, as the documentation says. This replaces JUCE's lock; record it in the crate `AGENTS.md`.
- B4. `ResamplingAudioSource`: wraps a source, `set_resampling_ratio(source samples per output sample)` takes effect without a click, `flush_buffers()`, and a channel count fixed at construction, as documented. It uses a B1 interpolator from WP-2.1-16, with a low-pass before decimation when the ratio is above 1; measure it with the `resample` profile (master plan §9.3) and document the quality and latency.
- B5. Determinism: for every source except `ResamplingAudioSource`, output is identical whatever block sizes the caller uses (proptest over random block-size sequences); `ResamplingAudioSource` stays within `resample` tolerance across block sizes. Every built-in `get_next_audio_block` is allocation-free (`syrp_rt::no_alloc` tests).
- B6. Errors, if any, continue `errors/audio.toml`. Update the crate `AGENTS.md` (module table, thread affinity for each source, ledger rows).

## Not in this WP
`BufferingAudioSource` and `AudioFormatReaderSource` (`syrp-formats`), `AudioTransportSource` and `AudioSourcePlayer` (`syrp-devices`), XML save and restore of channel mappings (JUCE's `createXml`/`restoreFromXml`; a later WP once `XmlElement` lands), and `rubato`.

## Crate setup
The whole crate `crates/audio/audio-basics/`, which WP-2.1-01 creates; this WP starts after WP-2.1-16 merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-2.1-04` first and `cargo syrp wp check WP-2.1-04` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
