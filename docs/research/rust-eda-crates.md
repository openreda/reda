# Rust crates as REDA's EDA building blocks

Research for issue #5. Surveyed on 2026-10-31 against crates.io (API metadata: licence, latest release, downloads), GitHub (repo stats, issues, READMEs, changelogs, source) and docs.rs. "Recent" downloads are crates.io's 90-day count. "Open" is GitHub's open issues plus PRs count.

All candidates are checked against ADR-0002 (MIT, Apache-2.0, BSD, zlib, ISC allowed; MPL-2.0 case by case; no GPL/LGPL) and the workspace toolchain (Rust 1.96, edition 2024, `unsafe_code = "forbid"` for REDA's own crates).

## Summary

| Area | Recommendation | Licence | Runner-up |
|---|---|---|---|
| Polygon booleans and offsetting | **`i_overlay`** (direct, integer API) | MIT OR Apache-2.0 | `geo` for convenience algorithms; `cavalier_contours` if arc-exact offsets are needed later |
| Spatial indexing | **`rstar`** | MIT OR Apache-2.0 | `kiddo` (points only) |
| Gerber writing | **`gerber-types`** (+ `gerber_parser` for round-trip tests) | MIT OR Apache-2.0 | none viable |
| Excellon writing | **Hand-roll** a small writer in a REDA crate | n/a | no permissive, maintained crate exists |
| Docking/tiling | **`egui_dock`** | MIT | `egui_tiles` |
| Multi-viewport tear-off | **Build in-house** on egui's viewport API; neither docking crate supports it | n/a | n/a |
| MCP server | **`rmcp`** (official SDK), using `ServerHandler` directly, not the `#[tool]` macros | Apache-2.0 (legacy MIT parts) | `rust-mcp-sdk`; hand-rolling rejected |
| JSON Schema for command parameters | **`schemars` 1.x** (+ `jsonschema` for validating incoming parameters) | MIT | none needed |

## 1. 2D polygon booleans and offsetting

| Crate | Latest | Licence | Downloads (all / recent) | Repo stars / open | Notes |
|---|---|---|---|---|---|
| `i_overlay` | 9.1.0, 2026-10-14 | MIT OR Apache-2.0 | 11.1M / 4.7M | 216 / 1 | Pure Rust. 132 releases since 2023 |
| `geo` | 0.33.1, 2026-04-20 | MIT OR Apache-2.0 | 25.3M / 5.1M | 1957 / 133 | f64 only; booleans and `Buffer` delegate to `i_overlay` |
| `clipper2` | 0.6.0, 2026-05-06 | MIT OR Apache-2.0 (wraps BSL-1.0 C++) | 444k / 98k | 29 / 1 | FFI to vendored C++; needs a C++17 toolchain |
| `clipper2-rust` | 1.2.0, 2026-09-18 | BSL-1.0 | 184k / 164k | 29 / 0 | Pure-Rust port, created 2026-02 |
| `geo-clipper` | 0.9.0, 2025-02-08 | ISC | 1.0M / 353k | 36 / 0 | Old Clipper1 C++ bindings; no release in 20 months |
| `cavalier_contours` | 0.9.0, 2026-08-20 | MIT OR Apache-2.0 | 100k / 59k | 240 / 10 | Arc-aware polylines |
| `boolmesh` | 0.2.2 | MPL-2.0 | 24k | n/a | 3D mesh booleans; out of scope |
| `polyclip` | 0.0.6, 2026-10-05 | MIT | 639 | 0 / 0 | Integer booleans and offsetting, but days old |

Findings:

- `i_overlay` provides union, intersection, difference and XOR; fill rules even-odd, non-zero, positive and negative; holes and self-intersections; and buffering of paths (strokes, with `LineCap` and `LineJoin::{Miter, Round, Bevel}`) and polygons (outlines). It offers native `i16`/`i32`/`i64` integer engines as well as `f32`/`f64` front ends. [iOverlay README](https://github.com/iShape-Rust/iOverlay/blob/main/iOverlay/README.md)
- Integer range: an N-bit engine accepts coordinates in `-2^(N-2)..=2^(N-2)-1`. So `i32` covers about ±1.07e9, which is ±1.07 m at 1 nm resolution, enough for PCBs. `i64` is there if more range is needed. [README, "Integer Coordinate Limits"](https://github.com/iShape-Rust/iOverlay/blob/main/iOverlay/README.md#integer-coordinate-limits)
- `EdgeOverlay` carries per-edge provenance (source ids, layer ids) through a boolean. That is directly useful for copper pours and for tracing DRC results back to a Net or object. (Same README, "Edge Attributes and Provenance".)
- `geo` 0.31.0 added `Buffer`, and `BooleanOps` re-exports `i_overlay`'s `FillRule`/`LineCap`/`LineJoin`. Both sit on `i_overlay`. [geo CHANGES.md](https://github.com/georust/geo/blob/main/geo/CHANGES.md) `geo` works only in floating point, so it adds a layer without adding capability for booleans. Its other algorithms (area, distance, simplification, convex hull) may still be worth pulling in later.
- Clipper2 is the industry reference. The `clipper2` crate builds the C++ through `clipper2c-sys` and "requires a working C++17 toolchain on the build host" [README](https://github.com/tirithen/clipper2#building), which complicates cross-platform builds. `clipper2-rust` avoids that, but its licence is BSL-1.0. BSL-1.0 is OSI-approved and permissive, yet it is not on ADR-0002's list, so it would need an explicit case-by-case decision. The crate is also only eight months old.
- None of these, except `cavalier_contours`, keep true arcs: arcs get flattened to segments. `cavalier_contours` keeps arcs ("arcs are not approximated as line segments"), but it only supports round joins and only booleans between two closed polylines. [README](https://github.com/jbuckmccready/cavalier_contours) It is a niche tool for arc-exact clearance or outline generation, not a general engine.

**Recommendation: `i_overlay`, used directly through its integer (`i32`, or `i64` if needed) API**, behind a REDA geometry module so the engine can be swapped. Revisit `cavalier_contours` only if arc-exact offsets become a requirement.

## 2. Spatial indexing

| Crate | Latest | Licence | Downloads (all / recent) | Repo stars / open |
|---|---|---|---|---|
| `rstar` | 0.13.0, 2026-05-24 | MIT OR Apache-2.0 | 52.8M / 15.9M | 564 / 47 |
| `kiddo` | 6.3.0, 2026-09-06 | MIT OR Apache-2.0 | 8.2M / 929k | n/a |

Findings:

- `rstar` is an n-dimensional R*-tree for objects with extent (rectangles, lines, polygons via `geo`), not just points. [README](https://github.com/georust/rstar)
- The API fits interactive editing and DRC well. It has `bulk_load`, incremental `insert`/`remove`, `locate_in_envelope_intersecting` (viewport culling, hit-testing), `locate_all_at_point`, `nearest_neighbor(s)`, `drain_in_envelope_intersecting`, and `intersection_candidates_with_other_tree`, which gives a DRC broad phase between two layers. [rtree.rs](https://github.com/georust/rstar/blob/master/rstar/src/rtree.rs)
- Integer coordinates work (`RTreeNum` includes integer types). Optional `serde` and `mint` features are available.
- `kiddo` is a k-d tree for points only. It is a poor fit for objects with extent.

**Recommendation: `rstar`.** It is still 0.x, so pin the minor version.

## 3. Gerber and Excellon writing

| Crate | Latest | Licence | Downloads | Notes |
|---|---|---|---|---|
| `gerber-types` | 0.7.0, 2025-12-19 | MIT OR Apache-2.0 | 36k | Gerber X2 AST plus code generation. Maintained by MakerPnP since 2025-05; repo pushed 2026-10-08 |
| `gerber_parser` | 0.5.0, 2026-05-20 | MIT OR Apache-2.0 | 27k | Parses into `gerber-types` |
| `gerberx2` | 0.1.2, 2026-05-18 | MIT OR Apache-2.0 | 2.5k | Parser; writer is "scaffolding" only |
| `lib_gerber_edit` | 0.7.1 | **non-standard** | 902 | Reads and writes Excellon, but the licence is unclear. Excluded |
| `excellon` | n/a | n/a | n/a | Not on crates.io |

Findings:

- `gerber-types` "implements the basic building blocks of Gerber X2 (compatible with Gerber RS-274X) code. It focuses on the low-level types (to be used like an AST) and code generation and does not do any semantic checking." It targets the 2024.05 spec. [README](https://github.com/MakerPnP/gerber-types)
- Its changelog shows serialization of file and aperture attributes (0.4.0), standard-comment attributes (0.6.0) and G54/G70/G71/G90/G91 (0.7.0). An unreleased fix escapes reserved characters in attribute strings. [CHANGELOG](https://github.com/MakerPnP/gerber-types/blob/main/CHANGELOG.md)
- The repo has five open issues/PRs, and the same maintainer runs the parser and the viewer crates. It is small but alive.
- Because there is no semantic checking, REDA needs a thin layer on top. That layer handles aperture-table management, regions (G36/G37), attributes per `FileFunction`, and polarity. This is expected work in any case.
- Excellon: no maintained, permissively licensed writer crate exists. The format is a small text format (header, tool table, `T`/`X`/`Y` hits, `G85` slots). A REDA-owned writer is a few hundred lines and is easy to test against `gerber_parser`-style golden files. The Gerber X2 spec also allows drill data as Gerber with `.FileFunction,Plated/NonPlated`, which `gerber-types` can already emit.

**Recommendation: `gerber-types` for Gerber output, `gerber_parser` as a dev-dependency for round-trip tests, and a hand-rolled Excellon writer.**

## 4. egui docking/tiling and multi-viewport tear-off

| Crate | Latest | egui dep | Licence | Downloads (all / recent) | Repo stars / open |
|---|---|---|---|---|---|
| `egui_dock` | 0.21.1, 2026-08-06 | `^0.36` | MIT | 4.9M / 300k | 645 / 38 |
| `egui_tiles` | 0.18.0, 2026-10-19 | `^0.37.0` | MIT OR Apache-2.0 | 2.6M / 673k | 597 / 35 (monorepo `rerun-io/egui_crates`) |

The current egui is 0.37.0 (2026-10-15). Both crates need MSRV 1.95, which is within REDA's 1.96 toolchain.

Findings:

- `egui_dock` features include tab close buttons, built-in scroll areas, tab context menus, and "dragging tabs out into new `egui` windows". Those are floating windows *inside* the same native window (`Surface`s in `DockState`). [README](https://github.com/anhosh/egui_dock) It only supports binary splits. It lags egui by one release right now (it depends on `^0.36`).
- `egui_tiles` supports horizontal, vertical and grid layouts, tabs, drag-and-drop docking and a `Behavior` trait for customisation. Its own README says `egui_dock` "supports some features that `egui_tiles` does not yet support, such as close-buttons on each tab, and built-in scroll areas". [README](https://github.com/rerun-io/egui_crates/blob/main/crates/egui_tiles/README.md) Rerun maintains it and ships it in lock-step with egui. The README badge says unsafe is forbidden.
- Multi-viewport (native OS windows): egui supports this through `Context::show_viewport_immediate`/`show_viewport_deferred` on native `eframe`, not on web. [egui viewport.rs docs](https://github.com/emilk/egui/blob/main/crates/egui/src/viewport.rs) **Neither docking crate integrates with it:**
  - `egui_dock` has had issue [#202 "Multi-viewport"](https://github.com/anhosh/egui_dock/issues/202) and draft PR [#203 "Multi-viewport support"](https://github.com/anhosh/egui_dock/pull/203) open since 2023-11. The PR was last touched 2026-06-28 and is still a draft.
  - `egui_tiles` has issue [#30 "Docking of tiles"](https://github.com/rerun-io/egui_crates/issues/30) open, and POC PR [#31 "Undocking of tiles into their own viewports"](https://github.com/rerun-io/egui_crates/pull/31) closed unmerged.
- So tear-off to a native window is REDA's own work under either crate. The workable shape: each torn-off native viewport hosts its own dock tree (an `egui_dock` `DockState`/`DockArea`, or an `egui_tiles` `Tree`) and is shown with `show_viewport_immediate`. Moving a tab between viewports is then a model operation, which fits ADR-0001 (it could be a command). Drag-across-windows feedback is the hard part and should be prototyped.

**Recommendation: `egui_dock`**, because its Document-tab metaphor (close buttons, context menus, scroll areas, floating surfaces) matches REDA's Documents-as-tabs model out of the box. Plan native tear-off as an in-house layer on egui viewports, behind a REDA docking module, and prototype it early. Switch to `egui_tiles` if `egui_dock`'s lag behind egui releases becomes a blocker. It is the more actively released crate, but REDA would then have to build the tab chrome itself.

Related: `rerun-io/egui_crates` also publishes `egui_mcp`, an MCP server that drives a live egui app through its AccessKit tree (click, type, screenshot). It is not REDA's command MCP (ADR-0001), but it could be useful for agent-driven UI testing. [README](https://github.com/rerun-io/egui_crates/blob/main/crates/egui_mcp/README.md)

## 5. MCP server SDK

| Crate | Latest | Licence | Downloads (all / recent) | Repo stars / open |
|---|---|---|---|---|
| `rmcp` | 3.8.0, 2026-10-26 | Apache-2.0 | 42.9M / 24.5M | 4077 / 70 (`modelcontextprotocol/rust-sdk`) |
| `rust-mcp-sdk` | 2.0.0, 2026-08-27 | MIT | 330k / 116k | 199 / 1 |
| `mcp-sdk` | 0.0.3, 2025-01-20 | Apache-2.0 | 13k | abandoned |

Findings:

- `rmcp` is the official SDK, in the `modelcontextprotocol` GitHub org. It "implements the stable MCP `2026-07-28` specification while remaining fully compatible with the `2025-11-25` release and earlier". [README](https://github.com/modelcontextprotocol/rust-sdk) The protocol has had several revisions in about 18 months, which is the strongest argument against hand-rolling: keeping up with version negotiation, transports and capability changes is ongoing work an SDK absorbs.
- Licence: the repo is moving from MIT to Apache-2.0. New code is Apache-2.0, and contributions without relicensing consent stay MIT. [LICENSE](https://github.com/modelcontextprotocol/rust-sdk/blob/main/LICENSE) Both are allowed by ADR-0002, and the crate declares `Apache-2.0`.
- Default features are `base64`, `macros`, `server`. Transports include `transport-io` (stdio) and `transport-streamable-http-server`. It depends on `tokio` and optionally `schemars ^1.0`. [Cargo.toml](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/Cargo.toml) Per ADR-0001, MCP lives in a separate CLI binary, so the tokio dependency stays out of the engine.
- API fit with a dynamic command registry: the `#[tool]`/`#[tool_router]` macros generate tools from static Rust methods, which does not match REDA's runtime registry of commands. Instead, implement `ServerHandler::list_tools`, `call_tool` and `get_tool` yourself and build `Tool { name, input_schema: Arc<JsonObject>, output_schema, .. }` values from the registry. [handler/server.rs](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/handler/server.rs), [model/tool.rs](https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/model/tool.rs)
- `rust-mcp-sdk` is a credible MIT alternative with a much smaller user base.

**Recommendation: `rmcp`, with a hand-written `ServerHandler` driven by the command registry. Do not hand-roll the protocol.**

## 6. JSON Schema generation for command parameters

| Crate | Latest | Licence | Downloads (all / recent) | Repo stars / open |
|---|---|---|---|---|
| `schemars` | 1.2.3, 2026-10-12 | MIT | 600.6M / 228.9M | 1426 / 123 |
| `jsonschema` (validation) | 0.60.2, 2026-10-26 | MIT | 113.3M / 36.2M | n/a |

Findings:

- `schemars` derives `JsonSchema`, emits JSON Schema draft 2020-12 by default, and aims for the generated schema to "match how serde_json would serialize/deserialize". It honours `#[serde(...)]` attributes (rename, default, untagged, `deny_unknown_fields`), with `#[schemars(...)]` overrides. [README](https://github.com/GREsau/schemars)
- `rmcp` depends on `schemars ^1.0`, so a command parameter struct deriving `Deserialize + JsonSchema` gives the MCP tool `input_schema`, CLI help and validation from one definition. That is exactly the "schemas from day one" consequence in ADR-0001.
- `schemars` cannot validate. Pair it with `jsonschema` to validate parameters arriving over MCP/CLI before deserializing, or rely on serde deserialization errors if those messages are good enough.

**Recommendation: `schemars` 1.x for parameter types**, with `jsonschema` as an optional validator at the CLI/MCP boundary.

## Follow-ups this suggests

- Prototype native tear-off of dock tabs on egui viewports (the riskiest item above).
- Decide coordinate representation (integer nm in `i32` vs `i64`). It drives the `i_overlay` engine choice and the `rstar` scalar type.
- Add `cargo-deny` with the ADR-0002 allow-list when the first of these dependencies lands. Note `clipper2-rust` (BSL-1.0) would need an explicit exception if ever chosen.
