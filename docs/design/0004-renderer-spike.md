# 0004: Renderer spike: vello_cpu vs tiny-skia

- **Status:** spike result, for the spec room. It feeds `syrp-render-cpu` and the golden
  tooling; it does not change a contract.
- **Work package:** WP-0.2-09. Code: `crates/spikes/render-spike/` (`syrp-render-spike`),
  throwaway.
- **Question (master plan §5.7, §11, risk R2):** should the reference CPU renderer be built on
  `vello_cpu` (as the plan assumes) or `tiny-skia`, can its output be byte-stable enough for
  goldens, and what should the golden comparator look like?

**Short answer: keep `vello_cpu`, in a pinned deterministic mode, with tolerance-based goldens.** It
was 1.7x faster over the scene set at 1x and 1.7–2.0x at 2x, has native layers, clips and blend
modes, and was byte-identical across 10 runs of every scene. Across SIMD levels and architectures
its output splits into two families that differ by at most one unit on at most 0.32% of pixels
(x86_64 SSE2 on one side; aarch64 NEON and x86_64 AVX-512 on the other, bit-identical to each
other). One golden set compared with a one-unit tolerance therefore holds everywhere we measured;
a hash does not. Keep `tiny-skia` as an independent cross-check in tests, not as a backend.


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
  `push_layer` with opacity and `Mix`, paint transforms for images, `TargetInit::SrcOver` so a
  render draws over the target's contents). `TinySkiaRenderer` maps onto
  `tiny-skia` 0.12.0, which has no layer or clip stack: layers are offscreen pixmaps composited
  on pop, clips are full-size coverage masks intersected on push.
- **Golden comparator** (`compare`, B4): a pixel differs when any premultiplied channel is more
  than `per_channel` away; the verdict passes when differing pixels are at most `max_ratio` of
  the image. The report gives counts, the ratio, the largest channel delta and a diff PNG
  (differing pixels red, brighter for larger deltas, the rest a pale grey of the golden).
- **Measurement** (`check_determinism`, `time_scene`, `fingerprint`): FNV-1a fingerprints of
  output bytes; wall-clock medians after one warm-up render.

## Results
Machine: x86_64, Intel Xeon @ 2.10 GHz (vello's best level here is AVX-512), 4 vCPUs, shared VM;
release build; single thread; median of 51 renders into one target cleared outside the timed
region. Absolute numbers are noisy on this VM: two runs differed by up to 1.9x on single scenes
and by 5–29% on the totals (the second run gave 1782 / 3061 / 4827 / 9692 µs); vello_cpu was
faster on 17 of 20 scenes at 1x in both runs and on 18–19 at 2x.

| scene | vello 1x | skia 1x | vello 2x | skia 2x | differing @1x (tol 8 / 32) |
|---|---:|---:|---:|---:|---:|
| rects_solid | 12 µs | 27 µs | 32 µs | 59 µs | 1.28% / 0.00% |
| path_star_nonzero | 17 µs | 22 µs | 42 µs | 45 µs | 0.99% / 0.04% |
| path_star_evenodd | 21 µs | 27 µs | 48 µs | 54 µs | 1.26% / 0.06% |
| path_cubic_blob | 19 µs | 24 µs | 41 µs | 51 µs | 2.45% / 0.94% |
| stroke_joins_caps | 39 µs | 96 µs | 79 µs | 144 µs | 1.35% / 0.06% |
| stroke_dashes | 93 µs | 58 µs | 133 µs | 184 µs | 4.84% / 1.68% |
| stroke_hairlines | 55 µs | 55 µs | 112 µs | 211 µs | 10.35% / 0.00% |
| gradient_linear | 39 µs | 132 µs | 125 µs | 493 µs | 39.17% / 30.00% |
| gradient_radial | 42 µs | 137 µs | 141 µs | 513 µs | 0.00% / 0.00% |
| gradient_conical | 33 µs | 54 µs | 84 µs | 167 µs | 1.86% / 0.52% |
| gradient_hard_stop_path | 53 µs | 93 µs | 92 µs | 234 µs | 0.98% / 0.00% |
| glyphs_solid | 57 µs | 90 µs | 89 µs | 164 µs | 2.88% / 0.45% |
| glyphs_gradient_rotated | 63 µs | 160 µs | 150 µs | 441 µs | 0.60% / 0.02% |
| image_stretched | 131 µs | 143 µs | 499 µs | 589 µs | 0.00% / 0.00% |
| image_nearest_rotated | 37 µs | 50 µs | 115 µs | 150 µs | 0.45% / 0.00% |
| image_alpha_centred | 386 µs | 336 µs | 1956 µs | 1511 µs | 0.00% / 0.00% |
| clip_rects_nested | 44 µs | 182 µs | 117 µs | 600 µs | 0.00% / 0.00% |
| clip_path_mixed | 79 µs | 231 µs | 228 µs | 842 µs | 1.14% / 0.00% |
| layer_opacity | 60 µs | 178 µs | 168 µs | 622 µs | 1.47% / 0.00% |
| layer_blend | 116 µs | 285 µs | 363 µs | 984 µs | 1.52% / 0.00% |
| **total** | **1394 µs** | **2380 µs** | **4615 µs** | **8059 µs** | |

Each renderer converts the built-in images once and shares them across draws, so these numbers
contain no per-draw image copies. The image scenes are still the slowest because they resample
most of the canvas (bicubic for the upscaled sprite), where the two crates are level.

**Determinism (B2).**
- 10 runs, fresh renderer each time, every scene at every scale: **byte-identical on both
  backends** (`b2_ten_runs_give_identical_bytes_on_both_backends`). Reusing one renderer for
  10 runs is also identical.
- **SIMD level matters for `vello_cpu`.** With the level pinned to the baseline (SSE2) versus
  the CPU's best (AVX-512 on the spike machine), 69 of 80 renders were identical and 11 differed
  by exactly one unit in up to 0.32% of pixels (`b2_vello_simd_levels_differ_by_at_most_one_unit`).
  The u8 pipeline rounds differently per instruction set.
- **Across x86_64 and aarch64 (macos-15 CI, Apple silicon), measured.** The deterministic-mode
  fingerprints of all 80 x86_64 renders are pinned in the test; on aarch64 69 of 80 match and
  the same 11 renders differ by at most one unit (worst: `image_stretched` at 1x, 34 pixels,
  0.32%). Those 11 aarch64 renders are bit-identical to x86_64 AVX-512 output, so all 80 aarch64
  renders equal the AVX-512 ones: the split follows the SIMD family, not the architecture. Both
  NEON and AVX-512 have fused multiply-add and SSE2 does not, which is the likely cause (not
  traced). Linux and Windows x86_64 CI reproduce the SSE2 table exactly
  (`b2_vello_deterministic_renders_are_pinned_per_simd_family`). Scene geometry is built
  without the platform libm, so these are renderer differences, not input differences.
- On aarch64, `Level::baseline()` and `Level::new()` both resolve to NEON, so
  `b2_vello_simd_levels_differ_by_at_most_one_unit` passes there trivially (it passed on
  macos-15).
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
   at 1x and 18–19 of 20 at 2x (1.7x and 1.7–2.0x over the whole set), native clip and
   layer stacks, the same code family as `vello_gpu`, and all 16 blend modes. `tiny-skia` needs
   offscreen layers and full-size clip masks emulated on top, uses `f32` transforms, and releases
   rarely (0.11.4 in 2024, 0.12.0 in February 2026).
2. **Deterministic mode** for goldens = single thread, u8 pipeline, SIMD level pinned to the
   architecture baseline (`Level::baseline()`), exact crate version pinned. Use it in CI. It is
   byte-stable per SIMD family, not across families (see Determinism).
3. **One golden set for every platform, compared with tolerance**, not by hash:
   `per_channel = 1`, `max_ratio = 0.005` for same-renderer goldens (the measured cross-family
   worst is one unit on 0.32% of pixels), and a separate looser profile (for example 32 / 0.02)
   for cross-renderer checks. If byte-exact goldens are ever needed, keep one set per SIMD
   family (SSE2; NEON or AVX-512), not per architecture. Ship the comparator here as the seed of
   `syrp-testkit`'s image comparison.
4. **Keep `tiny-skia` as a test-only oracle** to catch gross regressions in `syrp-render-cpu`;
   do not ship it as a backend.
5. **Risks to track:** `vello_cpu` is pre-1.0 (0.1 in July 2026, 0.3 in October) so API and
   output change between releases: pin exactly and regenerate goldens on upgrade. Its filter
   support is incomplete (complex filter graphs panic), which matters for `DropShadow`/`Glow`.

## Open
- x86_64 AVX2 (without AVX-512) was not measured; it has fused multiply-add, so it probably
  joins the NEON / AVX-512 family.
- The gradient interpolation space and fractional clip-edge behaviour need JUCE parity
  scenarios before goldens are frozen.
- Real text (parley/skrifa glyphs, hinting) was replaced by synthetic outlines; text fidelity,
  the other half of risk R2, needs its own spike with `syrp-text`.
- Clip masks (`PushClipMask`) and multi-threaded `vello_cpu` were not exercised.
