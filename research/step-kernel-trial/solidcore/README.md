# STEP kernel trial: solidcore

Harness for REDA issue #442, following `../SPEC.md`. Candidate: `solidcore`, the B-rep kernel
developed inside SolidCraft (`https://github.com/tangency-labs/solidcraft`, folder `solidcore/`).

Everything below was run on Linux x86-64 with Rust 1.96.1 unless a line says "read in the source".

## Versions and commits

- SolidCraft repository at `34d5055290364589caa8d1548d4e3563cb26b3a0` (2026-11-25). SolidCraft's
  own crates are version 0.49.1; `solidcore` is 1.0.1 (the number of the upstream it was ported
  from); `solidcore-record`, `-ops`, `-step`, `-tess`, `-convert` are 0.1.0.
- All dependencies are git dependencies on that one repository, pinned with `rev`. None of the
  crates is on crates.io (`publish = false`).
- Crates used: `solidcore`, `solidcore-record`, `solidcore-ops`, `solidcore-step`,
  `solidcore-tess` (all under `solidcore/crates/`, Apache-2.0), and with the default feature
  `smlib-trims` one SolidCraft application crate, `solidcraft-smlib` (`crates/smlib`,
  MIT OR Apache-2.0), which brings `solidcraft-geom` and `solidcraft-profile`.
- Toolchain: the repository pins 1.99.1 in `rust-toolchain.toml` (root and `solidcore/`), but the
  declared `rust-version` is 1.95 (SolidCraft), 1.85 (solidcore workspace) and 1.88 (record, ops,
  tess, convert). Everything used here builds on REDA's 1.96.1 without changes.

## Build

```sh
CARGO_TARGET_DIR=/data/mij/scratch/claude/reda-442/target/solidcore cargo build --release -j 8
step-trial-solidcore read <file.step>
step-trial-solidcore write <out.step> <component.step>
```

`--no-default-features` builds on the five `solidcore` crates alone (see "Trims" below).
`TRIAL_ERR=<n>` lengthens the `error` field beyond 300 characters, for reading a refusal in full.

## What it is

A pure-Rust port of the SMLib and SM_API parts of NVIDIA's USD BRep (C++), now developed on its
own, with a STEP reader and writer, a product-structure reader and a mesher written in the
SolidCraft project. No C or C++ is built or linked; `unsafe` is forbidden in every crate used.
The kernel crate is about 1.18 million lines of Rust, the `solidcore/crates` tree about 2.0 million.

Layers used by the harness:

| Step | API |
|---|---|
| Part 21 text to entities | `solidcore_step::p21::parse` |
| Schema declared | `solidcore_record::step::protocol::declared` |
| Products, occurrences, placements, styles | `solidcore_record::step::product::read` |
| Placements composed down the tree | `solidcore_ops::step_assembly::placed_bodies` |
| One B-rep record per solid | `solidcore_step::topology::read_solids_with` |
| Mesh | `solidcore_tess::plan` at `MeshTolerance::chord(0.01)`, then `TessPlan::evaluate` |
| Board solid | `solidcore_ops::prism::extrude_planar_regions`, then `Body::to_record` |
| AP242 file | `solidcore_step::write::product::products` |

## Read: what works

- Colours: yes. `ProductStructure::styles` lists every `STYLED_ITEM` with the entity id it
  styles (a solid, a face, a mapped item), its surface styles (side, fill colour, rendering
  colour, transparency) and curve colours. Each record face carries the id of the file face it
  came from (`Solid::face_origin`), so colours resolve per face. The harness takes a face's own
  style, else its solid's. Choosing one colour per face is left to the caller.
- Assembly tree: yes. Products, definitions, occurrences (`NEXT_ASSEMBLY_USAGE_OCCURRENCE`,
  `ASSEMBLY_COMPONENT_USAGE`, `SPECIFIED_HIGHER_USAGE_OCCURRENCE`), occurrence names and
  reference designators, `MAPPED_ITEM`s, and placements as data. `placed_bodies` returns every
  body entity with its 4x4 matrix in the root frame. `bbox_min`/`bbox_max` in the JSON are in
  the root frame.
- Schemas: AP203, AP214 and AP242 (editions 1 to 4) are recognised from `FILE_SCHEMA`; a file is
  read the same whatever it declares, and an unknown schema is read with a warning (read in the
  source; AP203 and AP214 input and AP242 edition 1 of the writer's own output were run).
- Units are converted to millimetres by the reader.
- Bodies read: `MANIFOLD_SOLID_BREP`, `BREP_WITH_VOIDS`, `FACETED_BREP`,
  `SHELL_BASED_SURFACE_MODEL`. Surfaces: plane, cylinder, cone, sphere, torus, B-spline and
  rational B-spline, surface of revolution, surface of linear extrusion, offset surface,
  rectangular trimmed surface. Curves: line, circle, ellipse, hyperbola, parabola, B-spline,
  polyline, trimmed curve, surface/seam/intersection curve (read in the source).
- What it cannot read: an unsupported entity refuses its solid with a message
  (`not supported yet: surface X`), listed in `Solids::failures`; the other solids are still
  read. `topology::read_solid_without` reads a refused solid again without named faces, as open
  shells. AP242 tessellated geometry (`TESSELLATED_SOLID` and the like) is not read by the
  `solidcore` crates; SolidCraft handles it in its application crate `solidcraft-kernel`.
- Errors are `Result<_, String>` messages throughout the STEP reader. No panic and no hang on
  the files run.

## Test files run

| File | Schema | Solids | Faces meshed | Triangles | Open edges | Colours | Faces coloured | Instances |
|---|---|---|---|---|---|---|---|---|
| kicad/SOIC-8_3.9x4.9mm_P1.27mm.step | AP214 | 1 | 147 of 147 | 1088 | 0 | 3 | 147 | 1 |
| kicad/R_0603_1608Metric.step | AP214 | 1 | 26 of 26 | 404 | 0 | 3 | 26 | 1 |
| wurth/L_Wurth_WE-PMI-0805_H1.step | AP214 | 1 | 26 of 26 | 692 | 0 | 2 | 26 | 2 |
| ap203/470531000.stp | AP203 | 1 | 101 of 101 | 636 | 0 | 0 | 0 | 1 |
| adafruit/3660 BME680.step | AP214 | 22 | 610 of 610 | 10764 | 0 | 17 | 610 | 16 |
| out/solidcore.step (own output) | AP242 e1 | 2 | 169 of 169 | 3622 | 0 | 4 | 169 | 4 |

"Instances" is the JSON `products` field: nodes of the expanded assembly tree.

`wurth/L_Wurth_WE-PMI-0805_H1.step` declares `AUTOMOTIVE_DESIGN` (AP214) in its `FILE_SCHEMA`,
not AP242. No AP242 file from another writer was run.

Times: parsing plus reading into records took 6 to 200 ms per file, meshing 2 to 25 ms; the
whole BME680 run is 0.23 s and 39 MB peak.

## Trims: the one piece outside the solidcore crates

The STEP reader does not use the file's `PCURVE` entities. It makes each edge's curve in the
face's parameter space itself, in closed form where one exists. For the rest it calls a
`solidcore_ops::pcurve::Trims` object supplied by the caller. The `solidcore` crates define the
trait and implement it only in tests. SolidCraft's implementation is
`solidcraft_smlib::pcurves::SmlibImportTrims`, in its application crate `solidcraft-smlib`.

Measured on `adafruit/3660 BME680.step` (one solid has B-spline edges on B-spline surfaces):

| Build | Result |
|---|---|
| no `Trims` (`read_solids`) | 1 of 22 solids refused, 537 of 610 faces |
| `--no-default-features`: the harness's own `Trims` over `solidcore_ops::pcurve::pcurves_within` | same refusal (`not supported yet: no exact pcurve of a NURBS on a spline surface`); read again without the 12 named faces: 598 of 610 faces, 72 open edges |
| default (`smlib-trims`) | 22 of 22 solids, 610 of 610 faces, 0 open edges |

The other four files read fully without any `Trims`.

## Write: what works

`out/solidcore.step` (293,977 bytes, AP242 edition 1 header) holds:

- `trial-board` with three `NEXT_ASSEMBLY_USAGE_OCCURRENCE`s: `PCB`, `U1`, `U2`.
- `PCB`: one `MANIFOLD_SOLID_BREP`, one closed shell of 22 faces (8 planes, 14 cylinders; each
  round hole is two half cylinders), one `STYLED_ITEM` with `COLOUR_RGB(0, 0.5, 0.2)`. Built by
  extruding one planar region with five holes; no 3D boolean is needed for a single region (the
  call does unite regions in the plane first).
- `U1` and `U2`: two occurrences of one product (one `PRODUCT_DEFINITION`, one solid written
  once), at (20, 10, 1.6) rotated 90 degrees about +Z and at (35, 20, 1.6).
- The component is re-written from the record the reader made of it, with its 147 face colours.

Read back with this harness: 2 solids, 169 of 169 faces meshed, closed, 4 colours, bounding box
(0, 0, 0) to (50, 30, 3.35), volume 2372.60 mm3 (board 2312.23 by mesh against 2311.94 exact,
plus two components of 30.18).

It has not been opened in any MCAD importer here.

## What it could not do, and surprises

- Board edge curves are not all lines and circles. The surfaces are `PLANE` and
  `CYLINDRICAL_SURFACE`, but of the board's 60 edges 12 are `LINE`, 6 `CIRCLE`, 22 rational
  B-spline curves (the arcs) and 20 non-rational `B_SPLINE_CURVE_WITH_KNOTS`. A body made by the
  kernel holds its curves as NURBS. The re-written component keeps `LINE` and `CIRCLE`
  (304 and 65), because its record came from the STEP reader.
- Every edge is written as a `SURFACE_CURVE` with `PCURVE`s (722 `PCURVE` entities in the file).
- Colours on export are `f32`: 0.2 is written as `0.20000000298023224`.
- The writer always writes AP242 edition 1; there is no AP214 or AP203 output.
- Full import fidelity needs a `Trims` from outside the `solidcore` crates (above).
- No healing was needed on the files run. The reader reports its own repairs per solid in
  `Solid::notes` (6 notes on the BME680). SolidCraft's further fallbacks (stitching faces read
  one by one, tolerance heals) live in `solidcraft-kernel` and were not used.

## API friction

- No single "import a STEP file" call in the `solidcore` crates. The caller joins four results
  by entity id: solids by item id, placements by body entity, styles by styled item, faces by
  `face_origin`. SolidCraft does this in `crates/kernel/src/step_in` (about 2,000 lines).
- A surface model is read as several pieces; `topology::items` maps a body entity to them.
- Two distinct `units::Context` types (`solidcore_record::step::units` and
  `solidcore_step::units`); `read_solid_without` wants the second, the product structure holds
  the first.
- Matrices: the reader's are documented row-major for row vectors, the writer's column-major;
  the sixteen numbers are in the same order (axes first, origin last).
- `solidcore_tess::plan` takes a `size` argument (the body's box diagonal) the caller must
  supply; 0.0 is accepted.
- The mesh shares vertices within a face only; the caller welds across faces. Edge nodes are
  bit-identical between neighbours, so welding at 1e-4 mm gave closed meshes.
- Loop winding for `Region2` is the caller's job: outer counter-clockwise, holes clockwise.

## Dependency weight

- `Cargo.lock`: 53 packages with `smlib-trims` (34 without). Third-party crates that compile on
  Linux: serde, serde_json, thiserror, spade (Delaunay), rayon, log, web-time and their
  dependencies. The 53 include wasm-only packages of `web-time` that are never built natively.
- A git dependency makes cargo clone the whole SolidCraft repository: 671 MB of git database
  and a 786 MB checkout in `CARGO_HOME`; the first resolve took 73 s.
- Clean release build: 3 min 43 s wall at `-j 8` (about 18 CPU minutes), nearly all of it the
  `solidcore` and `solidcore-ops` crates. Binary: 41.6 MB, not stripped.
- For comparison, not used here: `solidcraft-kernel` alone locks 100 packages;
  `solidcraft-kernel` plus `solidcraft-io` lock 358 (pgp, rsa, ed25519-dalek, gltf, image, zip,
  tiny-skia, pdf-writer, sysinfo, the cadmpeg codecs).

## Licence and provenance facts

- `solidcore/Cargo.toml`: `license = "Apache-2.0"`. `solidcore/LICENSE` is the Apache 2.0 text
  headed "Copyright (c) 2020-2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved."
- `solidcore/NOTICE`: "This product is a Rust port (a derivative work) of the SMLib and SM_API
  components of USD BRep, https://github.com/NVIDIA-Omniverse/usd-brep (v1.0.1, commit 3fd5b95),
  licensed under the Apache License, Version 2.0."
- `solidcore/ATTRIBUTION.md`: "The upstream sources are read, never built or linked", "No
  GPL/LGPL code is used anywhere in solidcore", and "solidcore is an independent project. It is
  not affiliated with or endorsed by NVIDIA". Upstream's OCCT translators, USD integration and
  Python bindings are listed as not read, ported, built or linked.
- Root `NOTICE`: "Licensed under the MIT License or the Apache License, Version 2.0, at your
  option", and "SolidCraft's brand and interface are not under MIT OR Apache-2.0" (`crates/ui-kit`,
  `crates/look`, `crates/theme` and the name and logo are under `LICENSE-BRAND`). None of those
  crates is a dependency here.
- `solidcore-step/src/lib.rs`: "Written from the public ISO 10303-21 and -42 specifications" and
  "Origin: SolidCraft's `crates/step` (MIT OR Apache-2.0), moved here with its history".
- `docs/step-import.md`: "written from the public standards; no third-party STEP code".
- `DISCLAIMER.md`: the files are provided "as is"; "Results produced with the files, such as
  geometry, meshes, analyses and simulations, can be wrong. A qualified person must check them
  before anyone relies on them for a design, manufacturing or safety decision." It ends: "It
  does not restrict any right that licence grants; where the two differ, the licence governs."
- `solidcore/AGENTS.md`: "Many agents work here at once". The shallow clone holds 2,519 commits
  dated 2026-10-27 to 2026-11-25.

## Maturity facts

- `solidcore/README.md`, Status: "Every SMLib kernel module has a Rust counterpart, and
  primitive construction is exercised end to end. Integration remains in progress: deferred
  paths still limit coverage. The API is experimental (0.0.x)."
- Root `README.md`: "Early but usable for simple parts: 75 of 76 public reference parts match
  their analytic or FreeCAD-computed volume, area, centre of mass and bounding box".
- `solidcore/CONTRIBUTING.md`: "solidcore replaced SMLib as SolidCraft's kernel in 0.40"; its
  `AGENTS.md` adds "The switch came with bugs: not everything that worked before works now".
- STEP reader tests in `solidcore-step`: 103 topology, 33 assembly and 25 geometry unit tests,
  a mutation fuzz that fails on any panic (`tests/panic_fuzz.rs`), a write-and-read-back volume
  test, 4 fixture files. Product structure: 10 tests. In SolidCraft: 158 tests in
  `crates/kernel/src/step_in/tests.rs`, 17 generated parts of known shape
  (`tests/step-conformance`, CC0), 14 regression files (`tests/step-regressions`), and a nightly
  external corpus with one source, the NIST PMI files in AP203 and AP242
  (`tests/external-corpus.toml`). A larger Part 21 corpus test exists but its files are not in
  the repository.
- `tests/KNOWN_FAILING_TESTS` lists kernel tests that fail today (booleans, fillets, offsets);
  none of the entries read concerns STEP reading.

## Cross-target checks

`cargo check --release --target x86_64-pc-windows-msvc` and `--target aarch64-apple-darwin`
both pass.
