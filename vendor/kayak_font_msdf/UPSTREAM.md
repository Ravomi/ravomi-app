# Kayak UI font maintenance port

Ravomi integration note (2026-10-11): this document preserves the font renderer's provenance and technical scope. Readability in the Arcraven / Ravomi workspace follows [DESIGN](../../DESIGN.md) and [AI UI rules](../../AI_UI_RULES.md); the cat's local grain and glow must not be applied to UI text. Product scope is defined in [PRD](../../PRD.md). Upstream attribution, licenses and the implementation history below remain unchanged.

Source: https://github.com/StarArawn/kayak_ui

Pinned revision: `7c5c6f77b72ce475cdd8e226934292d87d868a44` (2024-03-24).
The GitHub source archive has SHA-256
`e2944d3acfb47bb0a2b75fb8889a5016b3da998776f3bd4bba9e0223ed44aef0`.

`src/msdf/` is copied from `kayak_font/src/msdf/` at that revision.
John Mitchell and the Kayak UI contributors provide the source under
MIT OR Apache-2.0; both upstream license texts are retained here.
The MSDF algorithm originates with Viktor Chlumský's msdfgen work.

This is a maintained subset of Kayak's font subsystem, not a port of the entire
Kayak widget framework. The upstream main branch targets Bevy 0.13. Keeping the
generator engine-independent avoids bringing a second Bevy version into the app.
The Bevy 0.19.1 adapter and the adapted Kayak fragment shader live in
`../../src/msdf_text.rs` and `../../src/msdf_text.wgsl`.

Local changes:

- Upgrade ttf-parser to 0.25.1 and num-derive to 0.4.2; pin dependencies.
- Use num-traits directly instead of pulling in the num umbrella crate.
- Expose bounded, per-glyph generation instead of the old whole-character-range
  `.kttf` loader and its filesystem cache.
- Close implicit font contours, omit zero-length closing edges, and handle
  move-only contours.
- Fix shape bounds initialization and preserve multi-edge contours during
  normalization; fix contour bounds union comparisons.
- Run MSDF error correction once after the entire image is initialized, instead
  of scanning the full bitmap again after every row; express channel swaps with
  `std::mem::swap`.
- Keep only initialized split edges when coloring a one-corner contour.
- Repair cubic tangent degeneracy checks and use floating-point, inclusive
  Newton search seeds; normalize the endpoint tie-break direction. Cross-checked
  against msdfgen v1.9 `core/edge-segments.cpp`, with nearest-point regression tests.
- Establish the distance sign from padded exterior pixels for both CFF and
  TrueType contour orientations.
- Check pseudo-distance signs against the nearest true outline distance before
  storing texels. Acute stroke extensions otherwise create detached ink and
  missing strokes (reproduced with Noto Sans CJK's 路). Reuse the existing edge
  distance calculation; no extra contour pass or runtime shader cost. The
  ignored system-font regression compares CJK and Latin coverage against an
  independent flattened-outline winding test, including 4x bilinear sampling
  for 路径 outside a 0.6-source-texel edge tolerance.
- Restore msdfgen v1.9's second, diagonal-neighbor error-correction pass; the
  axis-only pass left interpolation gaps at Chinese stroke junctions.
- Use font-derived bounds, a 64 texel/em source resolution and an 8 texel distance
  range, with margins and consistent top-down RGBA output.
- Catch upstream generator panics at the public boundary and return an error.
- Reject bitmap, SVG and color glyphs explicitly so the adapter can display
  them with Bevy's raster fallback instead of silently emitting whitespace.
- Expose `generate_for_rendering` with a typed color-glyph result, so supported
  raster fallback does not count as a failed label or open an error dialog.
  Keep `generate` for outline-only callers and cold-generation measurements.

The standard library supplies collections, error handling and task data. Bevy
supplies text shaping/layout (Parley), atlas packing, asynchronous tasks and 2D
materials. No replacement shaping engine or atlas packer is implemented.
Kayak's old layout/Bevy renderer is deliberately excluded because the canvas
already uses current Bevy text layout and egui editing. Other evaluated options:
`fdsm` would replace the requested upstream; `msdfgen` adds native C++ integration;
`bevy_slugtext` uses a different technique and a 3D pipeline.

The port uses pure Rust dependencies and the application's Rust 1.95 minimum.
Windows/macOS runtime behavior is not validated by the Linux checks.

