# Text and fill triangulation for REDA's wgpu canvas

Research for issue #23. Surveyed on 2026-11-01 against crates.io (versions, release dates, licences, dependency lists via the crates.io API), the crates' own repositories and READMEs (GitHub API: last push, archived flag), the egui 0.37 / epaint 0.37 source in the Cargo registry, and REDA's `prototype/board-renderer` branch. Checked against ADR-0002 (MIT, Apache-2.0, BSD, zlib, ISC; MPL-2.0 case by case; no GPL/LGPL), ADR-0010 (geometry uploaded once as instanced capsule/rect/ring primitives plus triangle lists for Zone fills; `reda-render` depends on wgpu, not egui, and builds for wasm32) and the AGENTS.md dependency policy.

## Summary

| Need | Recommendation | Licence | Alternatives |
|---|---|---|---|
| Stroke font (silkscreen, Designators, fab text) | **Hershey Roman Simplex data, vendored, parsed in-house.** Each glyph stroke is a capsule instance: the same primitive as a Track. | Hershey distribution terms (attribution; see §2) | Draw REDA's own stroke glyphs later for missing characters. Not KiCad's newstroke (§2). |
| Font parsing and glyph outlines (TTF/OTF) | **`skrifa`** | MIT OR Apache-2.0 | `ttf-parser` (maintenance mode) |
| Shaping | **`harfrust`** | MIT | `rustybuzz` (archived) |
| Polygon triangulation (Zone fills, filled shapes, outline glyphs) | **`i_triangle`**, integer API | MIT OR Apache-2.0 | `lyon_tessellation`; `earcut` (georust) |
| GPU text approach | **Tessellated outlines in world space**, triangulated once per glyph and cached, drawn in the same triangle pass as Zone fills. Stroke text as capsules. | | SDF/MSDF atlas later if text volume demands it. Not `glyphon`. |
| Anti-aliasing of triangle lists | MSAA from eframe's `multisampling` setting (the prototype already passes it to its pipelines). | | Analytic edge fringe in the shader |

`skrifa` and `harfrust` are already in REDA's dependency tree: epaint 0.37 depends on `skrifa ^0.44.0` and `harfrust ^0.12.0` and uses them for egui's own text. If `reda-render` pins the same minor versions as egui, they add no code to the app binary. `i_triangle` sits on `i_overlay`, which the rust-eda-crates research already recommends for polygon booleans, so the Zone pipeline is one crate family.

The text-to-geometry step (shaping, glyph outlines, Hershey strokes, triangulation) should live below `reda-render`, in a crate without wgpu. The same geometry is needed for Gerber silkscreen output and DRC clearances, not only for drawing.

## 1. Font parsing and shaping

| Crate | Latest | Released | Licence | Status | Notes |
|---|---|---|---|---|---|
| `skrifa` (+ `read-fonts`) | 0.49.0 | 2026-10-21 | MIT OR Apache-2.0 | Active (fontations pushed 2026-10-30; 88 releases) | Glyph outlines through a pen, metrics, charmap, variations, hinting |
| `harfrust` | 0.15.0 | 2026-10-22 | MIT | Active (pushed 2026-10-31) | HarfBuzz shaping port on `read-fonts` |
| `ttf-parser` | 0.25.1 | 2024-11-29 | MIT OR Apache-2.0 | Maintenance mode | |
| `rustybuzz` | 0.20.1 | 2024-11-12 | MIT | Archived | |
| `swash` | 0.2.10 | 2026-07-17 | Apache-2.0 OR MIT | Active, slow | Shaping and rasterising; depends on `skrifa >=0.31.1, <=0.44`, which caps skrifa |
| `cosmic-text` | 0.19.0 | 2026-04-22 | MIT OR Apache-2.0 | Active | Full layout stack (`fontdb`, bidi, line breaking, editor) |
| `fontdue` | 0.9.5 | 2026-10-19 | MIT OR Apache-2.0 OR Zlib | Active | Rasteriser plus simple layout, no shaping; on `ttf-parser` |
| `ab_glyph` | 0.2.32 | 2025-09-28 | Apache-2.0 | Active | Rasteriser on `owned_ttf_parser`, no shaping |

- `ttf-parser`'s README: "This crate is in maintenance mode. Bug fixes only — no new features." and "For new projects, we recommend fontations (`read-fonts` and `skrifa`)". [ttf-parser README](https://github.com/harfbuzz/ttf-parser)
- `rustybuzz`'s README: "This project is not developed further, unmaintained, and archived. We recommend that all users switch to HarfRust instead." The repo is archived. [rustybuzz README](https://github.com/harfbuzz/rustybuzz)
- `skrifa` "ships as the font rendering backend for Chrome on Android and Linux" and "is tested glyph by glyph against FreeType". [skrifa README](https://github.com/googlefonts/fontations/tree/main/skrifa)
- epaint 0.37 depends on `skrifa ^0.44.0`, `harfrust ^0.12.0` and `vello_cpu ^0.2.0` (crates.io dependency list). Its `src/text/font_face.rs` uses `skrifa::outline::OutlineGlyphCollection` for outlines and `vello_cpu` (with `vello_cpu::kurbo`) to rasterise them. So skrifa, harfrust and kurbo are in every REDA build already.
- `cosmic-text` 0.19 depends on `skrifa ^0.40.0` and `harfrust ^0.5.0`, different minors from egui's, so it would put a second copy of both in the binary. Its layout features (rich text, editing, system font discovery) are more than canvas text needs: Designators and silkscreen strings are single lines, or a few lines with explicit breaks.
- All of the above are pure Rust and target wasm32. `skrifa`, `read-fonts` and `harfrust` have optional `core_maths` features for `no_std`.

**Recommendation:** `skrifa` for outlines and metrics, and `harfrust` for shaping, at egui's minor versions while that is practical. REDA lays out single-line and explicit-multi-line strings itself (advances, alignment, rotation, mirroring for bottom-side text). That is a small amount of code. Shaping matters for kerning and for any non-Latin text users put on a board.

## 2. Stroke font for silkscreen and Designators

PCB tools draw most board text in a single-stroke font: each glyph is a set of polylines plotted with a pen of the text's stroke width, which is exactly how Gerber draws it (D01 draws with a circular aperture). In REDA's renderer, each stroke segment is then one **capsule instance**: the same primitive, shader and anti-aliasing as a Track. That needs no glyph atlas or extra pipeline, looks the same at every zoom, and is uploaded once with the rest of the Document.

**Hershey fonts (recommended).**

- Terms: "This distribution of the Hershey Fonts may be used by anyone for any purpose, commercial or otherwise", provided the acknowledgements to Dr. A. V. Hershey (National Bureau of Standards) and James Hurt (Cognition, Inc.) ship with the font data, and the data is not converted to the format distributed by the U.S. NTIS. [hershey-fonts COPYING](https://github.com/kamalmostafa/hershey-fonts/blob/master/COPYING) (the package's code is GPL-2.0; only the `.jhf` data is under the Hershey terms, and that data is all REDA needs).
- These are attribution-only data terms, not one of ADR-0002's named licences. Using them needs an explicit case-by-case acceptance recorded against ADR-0002. Note that eframe's default fonts already bring in non-listed font licences: `epaint_default_fonts` 0.37 declares `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`. ADR-0002 will need a line on font licences anyway.
- Coverage: `futural.jhf` (Roman Simplex) has 96 glyphs, printable ASCII. Greek, Cyrillic and math symbols are in separate files (`greek.jhf`, `cyrillic.jhf`, `mathlow.jhf`, `mathupp.jhf`, ...). Accented Latin is not covered. [hershey-fonts/hershey-fonts](https://github.com/kamalmostafa/hershey-fonts/tree/master/hershey-fonts)
- Format: one line per glyph, a vertex count, left/right bearings, then coordinate pairs as characters offset from `R`, with ` R` as pen-up. A parser is tens of lines. Write it in-house (AGENTS.md), or convert the data at build time to a Rust table.
- The `hershey` crate (0.1.2, 2025-03-26, Apache-2.0 OR LGPL-3.0, 14.5k downloads) parses the format. It is usable under its Apache-2.0 option, but too small to be worth a dependency.

**KiCad's newstroke font: do not use.** KiCad's `newstroke_font.h` carries "Copyright (C) 2010 vladimir uryvaev" with a GPL-2.0-or-later notice [KiCad doxygen](https://docs.kicad.org/doxygen/newstroke__font_8h_source.html). Other statements conflict: a README in a mirror of the newstroke build tools says CC-BY, and a third-party viewer's LICENSE says CC0 amended with an MIT-like licence plus OFL-1.1 glyphs. Without a clear primary statement from the author it fails ADR-0002. KiCad files that name the KiCad font will render in REDA's own stroke font, with different metrics. This is a known import difference, to be recorded with the import work.

For characters outside Hershey's sets, REDA can fall back to outline text (§3) or draw its own stroke glyphs, MIT, in the repo.

## 3. GPU text approaches

| Approach | World-space, zoom | Rotation, mirroring | Upload-once fit | Cost |
|---|---|---|---|---|
| Stroke glyphs as capsules (§2) | Exact at every zoom | Free (transform the points) | Same instance buffer as Tracks | None beyond the font parser |
| Tessellated outline glyphs | Exact at every zoom | Free | Triangle list, like Zone fills; triangulate each glyph once, cache by (font, glyph id) | Triangulator (shared with Zones); needs MSAA for smooth edges |
| SDF / MSDF atlas | Good over a range; SDF rounds sharp corners when magnified, MSDF keeps them | Free (quads) | Quad instances plus an atlas texture | An atlas generator, a texture, a third shader |
| `glyphon` | Rasterised per pixel size | Not supported | Re-prepare when zoom changes | Pulls in `cosmic-text` |

- **glyphon** (0.12.0, 2026-07-09, MIT OR Apache-2.0 OR Zlib; depends on `cosmic-text ^0.19`, `etagere`, `wgpu ^30.0.0`) shapes and rasterises with cosmic-text, packs glyphs into an atlas and samples it. [glyphon README](https://github.com/grovesNL/glyphon) Its `TextArea` has only position and a uniform `scale`, and glyph rasters are keyed by physical pixel size (`src/text_render.rs`). The PR that would have added `TextArea.transform` (rotation) was closed unmerged ([glyphon#64](https://github.com/grovesNL/glyphon/pull/64)). Board text is rotated and mirrored, and zoom changes the pixel size continuously, so glyphon does not fit the canvas. (It does match REDA's wgpu 30 today.)
- **MSDF generators:** the `msdfgen` crate (0.2.1, last release 2023-01-17, MIT) binds the C++ library through `msdfgen-sys`, which makes wasm32 hard. `fdsm` (0.8.0, 2025-10-02, MIT) is the pure-Rust option but depends on `image` and `nalgebra`. If an atlas is ever needed, generate it offline or write a small in-house SDF generator from skrifa outlines.
- **KiCad precedent:** KiCad's `OUTLINE_GLYPH::Triangulate` calls `SHAPE_POLY_SET::CacheTriangulation`, the same triangulation its zones use, and caches it per glyph because "glyphs are invariant after creation". [common/font/glyph.cpp](https://gitlab.com/kicad/code/kicad/-/blob/master/common/font/glyph.cpp) Stroke-font glyphs are polylines (`STROKE_GLYPH`). Read for ideas only (GPL).

**Recommendation:** stroke text as capsules. Outline (TTF) text: skrifa outline, flattened (kurbo is already in the tree, or a small in-house quad/cubic flattener), triangulated with the Zone triangulator, cached per glyph in glyph units, and instanced or baked into the Document triangle list with the text's transform. Every piece of Document geometry then uses two pipelines: SDF instances and triangles. Light overlay text (rulers, measurement read-outs, hover labels) can stay with egui's painter, as ADR-0010 allows.

## 4. Polygon triangulation

| Crate | Latest | Released | Licence | Downloads (all / 90 d) | Status | Input |
|---|---|---|---|---|---|---|
| `i_triangle` | 0.51.0 | 2026-10-30 | MIT OR Apache-2.0 | 96k / 27k | Active (75 releases; pushed 2026-10-30) | `i16`/`i32`/`i64` integer or `f32`/`f64` |
| `lyon_tessellation` | 1.0.23 | 2026-10-20 | MIT OR Apache-2.0 | 9.5M / 3.7M | Active (since 2016; pushed 2026-10-23) | `f32` paths, with curves |
| `earcut` (georust) | 0.4.11 | 2026-07-26 | MIT OR Apache-2.0 | 2.4M / 1.1M | Active | `f64` or `EarcutI32` |
| `earcutr` | 0.5.0 | 2025-05-29 | ISC | 14.2M / 3.8M | Deprecated, archived | `f64` |
| `spade` | 2.15.1 | 2026-03-24 | MIT OR Apache-2.0 | 22.6M / 5.7M | Active | Constrained Delaunay of points and edges |

- **i_triangle**: "reliable, deterministic 2D polygon triangulation for complex shapes with holes and self-intersections". It supports holes, degenerate edges and mixed winding, and its "geometry is processed with integer math". It offers sweep-line triangulation in `O(n log n)`, Delaunay, convex decomposition and Steiner points. Its output vertices are "naturally ordered by X", which helps GPU cache locality. [iTriangle README](https://github.com/iShape-Rust/iTriangle) It depends on `i_overlay ^9.1.0`, so Zone outlines from `i_overlay` booleans go straight in. Wasm32: the same code ships as a WebAssembly package ([iShape-js](https://github.com/iShape-Rust/iShape-js)). Risk: a much smaller user base than lyon, and frequent minor releases (0.x API churn).
- **lyon_tessellation**: a sweep-line `FillTessellator` with `FillRule` and a `tolerance` for flattening curves, plus a `StrokeTessellator` with SVG line caps and joins. Output is `VertexBuffers<T>` of vertices and indices for triangle lists. [docs.rs](https://docs.rs/lyon_tessellation/1.0.23/lyon_tessellation/) It is the most proven option and handles curves directly. But it works in `f32` only, which loses precision on integer-nm board coordinates unless REDA rebases to a local origin, and the geometry would differ from what `i_overlay` computed.
- **earcut** (georust): a port of mapbox/earcut 3.2.3, built to reuse allocations, with an `EarcutI32` exact-integer variant. [georust/earcut README](https://github.com/georust/earcut) Ear clipping assumes simple polygons with holes and does not resolve self-intersections. That is fine after `i_overlay` has normalised the shape, but gives no safety net. `earcutr` says "This crate is deprecated. Instead you should use https://github.com/georust/earcut." [earcutr README](https://github.com/frewsxcv/earcutr)
- **spade**: constrained Delaunay triangulation of points and edges. It is a mesh library, not a polygon filler: holes and fill rules are REDA's job.

**Recommendation: `i_triangle`, integer API**, fed by `i_overlay` output in the same integer coordinate type (`i32` or `i64`, the open decision noted in the rust-eda-crates research). Triangulate in Document coordinates, convert to `f32` relative to a tile or Layer origin when building the GPU buffer, and upload once per Zone refill. Fallback: `lyon_tessellation`, if `i_triangle`'s churn or bugs become a problem. It also covers curved outlines, so it could serve outline glyphs without a separate flattener.

## 5. Anti-aliasing the triangle pass

SDF instances get analytic anti-aliasing in the fragment shader. Triangle lists (Zone fills, outline glyphs) do not. The prototype creates its pipelines with `MultisampleState { count: msaa.max(1), .. }` from eframe's setting (`crates/prototype-board-render/src/gpu.rs` on `prototype/board-renderer`), and the egui_wgpu callback must match egui's sample count. WebGPU guarantees sample counts 1 and 4, so 4× MSAA works on wasm32 as well. The alternative, an analytic fringe (extruded edge quads with coverage falloff, as egui's own tessellator does), avoids MSAA but complicates every fill. Decide when the Zone fill pass is built.

## Open questions

- ADR-0002 needs a ruling on font data licences: the Hershey attribution terms, and OFL-1.1 / Ubuntu-font-1.0 already arriving with eframe's default fonts.
- Which TTF REDA bundles, if any, for outline text, or whether outline text uses only user-supplied fonts.
- Integer coordinate width (`i32` vs `i64`), shared with the `i_overlay` decision.
