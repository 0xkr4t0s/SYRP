# 0004: Renderer spike: vello_cpu vs tiny-skia

- **Status:** spike result, for the spec room. It feeds `syrp-render-cpu` and the golden
  tooling; it does not change a contract.
- **Work package:** WP-0.2-09. Code: `crates/spikes/render-spike/` (`syrp-render-spike`),
  throwaway.
- **Question (master plan §5.7, §11, risk R2):** should the reference CPU renderer be built on
  `vello_cpu` (as the plan assumes) or `tiny-skia`, can its output be byte-stable enough for
  goldens, and what should the golden comparator look like?

**Short answer: keep `vello_cpu`, in a pinned deterministic mode, with tolerance-based goldens.**
It was 1.7–2.0x faster over the scene set, has native layers, clips and blend modes, and was
byte-identical across 10 runs of every scene. Byte identity does *not* survive a change of SIMD
level, so goldens must be compared with a small tolerance, never by hash, once more than one
machine produces them. Keep `tiny-skia` as an independent cross-check in tests, not as a backend.

## What was built
- **20 scenes** (`scenes()`), each a `syrp_gfx_api::DisplayList` on a 120 x 90 logical canvas,
  rendered at 1x, 1.25x, 1.5x and 2x (80 renders per backend): solid and fractional rects;
  non-zero and even-odd self-crossing paths; cubic and quadratic outlines; strokes with every
  join and cap, dashes and 0.25–1 px hairlines; linear, radial and two-point conical gradients,
  one with a hard stop; text-like glyph runs (solid, gradient-filled, rotated) from a synthetic
  outline font; images stretched (bilinear), rotated (nearest) and with alpha (bicubic);
  nested rect clips under rotation; path clips; opacity and blend-mode layers.
- **One lowering pass** (`replay`) turns the list into a few backend calls, so both renderers
  get the same geometry: glyphs become outlines, images become a placed rectangle plus an
  image-to-user transform, and the device scale is the root transform.
- **Backends.** `VelloCpuRenderer` maps ops 1:1 onto `vello_cpu` 0.3.0 (`push_clip_path`,
  `push_layer` with opacity and `Mix`, paint transforms for images). `TinySkiaRenderer` maps onto
  `tiny-skia` 0.12.0, which has no layer or clip stack: layers are offscreen pixmaps composited
  on pop, clips are full-size coverage masks intersected on push.
- **Golden comparator** (`compare`, B4): a pixel differs when any premultiplied channel is more
  than `per_channel` away; the verdict passes when differing pixels are at most `max_ratio` of
  the image. The report gives counts, the ratio, the largest channel delta and a diff PNG
  (differing pixels red, brighter for larger deltas, the rest a pale grey of the golden).
- **Measurement** (`check_determinism`, `time_scene`, `fingerprint`): FNV-1a fingerprints of
  output bytes; wall-clock medians after one warm-up render.

## Results
Machine: x86_64, Intel Xeon @ 2.10 GHz (AVX2 and AVX-512 present), 4 vCPUs, shared VM; release
build; single thread; median of 51 renders, including target allocation. Absolute numbers are
noisy on this VM (two runs differed by up to 2x on a few small scenes); ratios were stable.

| scene | vello 1x | skia 1x | vello 2x | skia 2x | differing @1x (tol 8 / 32) |
|---|---:|---:|---:|---:|---:|
| rects_solid | 17 µs | 51 µs | 43 µs | 112 µs | 1.28% / 0.00% |
| path_star_nonzero | 24 µs | 42 µs | 55 µs | 85 µs | 0.99% / 0.04% |
| path_star_evenodd | 30 µs | 49 µs | 67 µs | 102 µs | 1.26% / 0.06% |
| path_cubic_blob | 28 µs | 44 µs | 55 µs | 89 µs | 2.45% / 0.94% |
| stroke_joins_caps | 55 µs | 132 µs | 115 µs | 262 µs | 1.35% / 0.06% |
| stroke_dashes | 119 µs | 110 µs | 173 µs | 323 µs | 4.84% / 1.68% |
| stroke_hairlines | 82 µs | 89 µs | 160 µs | 338 µs | 10.35% / 0.00% |
| gradient_linear | 62 µs | 234 µs | 184 µs | 786 µs | 39.17% / 30.00% |
| gradient_radial | 67 µs | 237 µs | 210 µs | 800 µs | 0.00% / 0.00% |
| gradient_conical | 49 µs | 85 µs | 124 µs | 243 µs | 1.86% / 0.52% |
| gradient_hard_stop_path | 77 µs | 163 µs | 137 µs | 414 µs | 0.98% / 0.00% |
| glyphs_solid | 80 µs | 170 µs | 120 µs | 320 µs | 2.88% / 0.45% |
| glyphs_gradient_rotated | 100 µs | 272 µs | 224 µs | 716 µs | 0.60% / 0.02% |
| image_stretched | 191 µs | 178 µs | 732 µs | 775 µs | 0.00% / 0.00% |
| image_nearest_rotated | 51 µs | 88 µs | 150 µs | 238 µs | 0.45% / 0.00% |
| image_alpha_centred | 518 µs | 441 µs | 1983 µs | 1766 µs | 0.00% / 0.00% |
| clip_rects_nested | 68 µs | 293 µs | 174 µs | 894 µs | 0.00% / 0.00% |
| clip_path_mixed | 79 µs | 231 µs | 222 µs | 852 µs | 1.16% / 0.00% |
| layer_opacity | 60 µs | 183 µs | 163 µs | 629 µs | 1.47% / 0.00% |
| layer_blend | 104 µs | 400 µs | 344 µs | 1017 µs | 1.52% / 0.00% |
| **total** | **1859 µs** | **3493 µs** | **5437 µs** | **10761 µs** | |

The image scenes copy the source pixels on every draw in both adapters (the spike has no image
cache), which is why they are the slowest and roughly level; a real renderer registers images
once.

**Determinism (B2).**
- 10 runs, fresh renderer each time, every scene at every scale: **byte-identical on both
  backends** (`b2_ten_runs_give_identical_bytes_on_both_backends`). Reusing one renderer for
  10 runs is also identical.
- **SIMD level matters for `vello_cpu`.** With the level pinned to the baseline (SSE2) versus
  the CPU's best (AVX2), 69 of 80 renders were identical and 11 differed by exactly one unit
  in up to 0.3% of pixels (`b2_vello_simd_levels_differ_by_at_most_one_unit`). The u8
  pipeline rounds differently per instruction set. So a hash golden recorded on one CPU can
  fail on another CPU of the same architecture unless the level is pinned, and will almost
  certainly fail on another architecture.
- **Across x86_64 and aarch64: open.** CI gave this spike only x86_64. Expect at least the
  one-unit drift above (NEON is a different code path); measure when an aarch64 runner exists.
- `tiny-skia` picks its SIMD path at compile time from target features, so the same binary is
  stable, but a build with other target features (or another architecture) can drift the same
  way. Not measured.

**Agreement between the two (B1).** With a per-channel tolerance of 32 the backends agree on
all but 2% of pixels in every scene at every scale except one. Differences at tolerance 8 are
anti-aliasing coverage at edges (largest for hairlines and dashes, which use different stroke
flattening and dash phase arithmetic). The exception is a **semantic** one:

- `gradient_linear` ramps to a transparent stop. `tiny-skia` interpolates unpremultiplied
  colour and `vello_cpu` premultiplied colour, so 30% of pixels differ by up to 64. Which one
  JUCE's `ColourGradient` matches is not stated in its public documentation: a parity scenario
  must decide it, and `syrp-render-cpu` must then match it (peniko gradients expose an
  `interpolation_alpha_space` setting).
- `clip_rects_nested` at 1.25x differed by up to 51 along one clip edge: `vello_cpu`'s rect
  clip fast path treats a fractional device-space clip edge differently from `tiny-skia`'s
  anti-aliased mask. Worth a parity scenario too.

## Recommendation
1. **Reference renderer: `vello_cpu`** (as master plan §5.7 assumes). Faster on 17 of 20 scenes
   at 1x (1.7–2.0x over the whole set across two runs), native clip and layer stacks, the same
   code family as `vello_gpu`, and all 16 blend modes. `tiny-skia` needs offscreen layers and
   full-size clip masks emulated on top, uses `f32` transforms, and releases rarely (0.11.4 in
   2024, 0.12.0 in February 2026).
2. **Deterministic mode** for goldens = single thread, u8 pipeline, SIMD level pinned to the
   architecture baseline (`Level::baseline()`), exact crate version pinned. Use it in CI.
3. **Goldens are per architecture and compared with tolerance**, not by hash: start with
   `per_channel = 1`, `max_ratio = 0.005` for same-renderer goldens (covers the SIMD drift),
   and a separate looser profile (for example 32 / 0.02) for cross-renderer checks. Ship the
   comparator here as the seed of `syrp-testkit`'s image comparison.
4. **Keep `tiny-skia` as a test-only oracle** to catch gross regressions in `syrp-render-cpu`;
   do not ship it as a backend.
5. **Risks to track:** `vello_cpu` is pre-1.0 (0.1 in July 2026, 0.3 in October) so API and
   output change between releases: pin exactly and regenerate goldens on upgrade. Its filter
   support is incomplete (complex filter graphs panic), which matters for `DropShadow`/`Glow`.

## Open
- aarch64 determinism (no runner).
- The gradient interpolation space and fractional clip-edge behaviour need JUCE parity
  scenarios before goldens are frozen.
- Real text (parley/skrifa glyphs, hinting) was replaced by synthetic outlines; text fidelity,
  the other half of risk R2, needs its own spike with `syrp-text`.
- Clip masks (`PushClipMask`) and multi-threaded `vello_cpu` were not exercised.
