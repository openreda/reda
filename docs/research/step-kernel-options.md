# STEP without a CAD kernel: what each option costs a Rust application

Research for issue #422. Sources read on 2026-11-23: source code at the commits named below, crates.io API metadata, first-party READMEs and documentation, licence texts, and ISO 10303-21 as published by STEP Tools. "Recent" downloads are crates.io's 90-day count.

Licences are checked against ADR-0002: MIT, Apache-2.0, BSD, zlib and ISC are allowed, MPL-2.0 case by case, and "no GPL or LGPL code may be linked or copied".

**Nothing was built or run for this note.** Compiling the candidate crates was not permitted in the research session, so there are no measured binary sizes, build times or test results on real models here. Every size below is a figure published by someone else, and section 8 lists what that leaves open.

## Summary

| Option | Reads STEP to a coloured mesh | Writes solids as an assembly | Licence against ADR-0002 | Viable |
|---|---|---|---|---|
| OpenCascade through `opencascade-rs` | Geometry yes; the bindings expose no colours or assembly structure | Solids yes; no colours, names or assembly structure exposed | LGPL-2.1 with exception: excluded | No, unless ADR-0002 changes |
| OpenCascade through `cadrum` | Yes, with colours | Solids with colours; its writer deliberately writes no assembly | Wrapper MIT, linked OCCT LGPL-2.1: excluded | No, unless ADR-0002 changes |
| `truck` (`truck-stepio` + `truck-meshalgo`) | Geometry yes; no colours | Yes on master, without colours; released crates (2024) have no assembly support | Apache-2.0: allowed | Yes, with gaps REDA would fill |
| `step-io` | Parses and exposes B-rep, colours and assembly tree; does not tessellate | Yes, AP242 with colours and placements, from topology the caller supplies | MIT OR Apache-2.0: allowed | Yes as a file-format layer; meshing must come from elsewhere |
| `ruststep` | Parser only | No (serialisation is on its to-do list) | Apache-2.0: allowed | Only as the parser under `truck-stepio` |
| Foxtrot (Formlabs) | Yes, with colours; its README says some surfaces fail | No | MIT OR Apache-2.0: allowed | Only as code to study or vendor: unmaintained since 2021, not on crates.io |
| Fornjot | No STEP | No STEP | 0BSD | No: shut down 2026-06-19 |
| `monstertruck`, `arris` | Claimed | Claimed | Apache-2.0; MIT OR Apache-2.0 | Unproven: 9 and 3 months old |
| `brepkit`, `BREP_kernel` | Claimed | Claimed | AGPL-3.0-only; "non-standard" | No (`brepkit`); licence not read (`BREP_kernel`) |
| In-house narrow writer | n/a | Yes for prisms with holes and revolved shapes, with colours and assembly | REDA's own code | Yes: one open tool ships exactly this |
| Re-embedding imported entities | n/a | Yes within limits: renumber, place, keep colours | REDA's own code | Yes within limits: one open tool ships exactly this |

No option that passes ADR-0002 does both jobs completely in one maintained, released crate. Reading is the harder half: turning trimmed NURBS and analytic faces into triangles is the kernel-shaped piece, and the only permissively licensed code that does it is `truck-meshalgo`, Foxtrot's `triangulate` and the months-old kernels.

## 1. What the two jobs need

### (a) Read a component model and draw it as a coloured mesh

Four steps, in order:

1. **Parse the Part 21 text** into entity instances. This is a small, well-defined grammar: instances are `#n = NAME(...)`, references may point forwards, and instances need not be ordered. [ISO 10303-21 clauses 6.4.4.3 and 11.2](https://www.steptools.com/stds/step/IS_final_p21e3.html)
2. **Resolve the entities that matter** out of a schema with hundreds of types. AP214 edition 3 declares 915 entity types; Foxtrot uses 26 of them. [Foxtrot write-up](https://mattkeeter.com/projects/foxtrot/)
3. **Tessellate each face**: evaluate its surface (plane, cylinder, cone, sphere, torus, B-spline, swept), trim it by its edge loops, and triangulate. This is the step a kernel exists for.
4. **Attach colours**, which live outside the geometry in a chain of presentation entities.

Eight models from KiCad's `kicad-packages3D` repository were downloaded and their entity types counted with `grep` (the models were not committed; they are CC-BY-SA with an exception). Two of them, a 0603 resistor and an SOIC-8, contain between them:

| Group | Entity types found (count across the two files) |
|---|---|
| Topology | `ORIENTED_EDGE` 817, `EDGE_CURVE` 461, `VERTEX_POINT` 293, `FACE_BOUND` 174, `EDGE_LOOP` 173, `ADVANCED_FACE` 173, `CLOSED_SHELL` 2, `MANIFOLD_SOLID_BREP` 2 |
| Surfaces | `PLANE` 102, `CYLINDRICAL_SURFACE` 40, `B_SPLINE_SURFACE_WITH_KNOTS` 16 |
| Curves | `LINE` 300, `CIRCLE` 81, `B_SPLINE_CURVE_WITH_KNOTS` 24 |
| Placement | `CARTESIAN_POINT` 471, `AXIS2_PLACEMENT_3D` 217, `DIRECTION` 175, `VECTOR` 68 |
| Colour | `STYLED_ITEM` 173, `PRESENTATION_STYLE_ASSIGNMENT` 29, `SURFACE_STYLE_USAGE` 29, `SURFACE_SIDE_STYLE` 29, `SURFACE_STYLE_FILL_AREA` 29, `FILL_AREA_STYLE` 29, `FILL_AREA_STYLE_COLOUR` 29, `COLOUR_RGB` 6 |
| Product structure | `PRODUCT`, `PRODUCT_DEFINITION`, `PRODUCT_DEFINITION_FORMATION`, `PRODUCT_DEFINITION_SHAPE`, `SHAPE_DEFINITION_REPRESENTATION`, `ADVANCED_BREP_SHAPE_REPRESENTATION`, contexts and units |

Observations from those files:

- **Even simple packages carry B-spline surfaces**, so a reader limited to planes and cylinders would not draw an SOIC-8 completely.
- **Colours are per face**: 173 `STYLED_ITEM`s for 173 `ADVANCED_FACE`s. The chain in the 0603 file is `STYLED_ITEM` → `PRESENTATION_STYLE_ASSIGNMENT` → `SURFACE_STYLE_USAGE` → `SURFACE_SIDE_STYLE` → `SURFACE_STYLE_FILL_AREA` → `FILL_AREA_STYLE` → `FILL_AREA_STYLE_COLOUR` → `COLOUR_RGB`, gathered under one `MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION`.
- **All eight files declare AP214** (`AUTOMOTIVE_DESIGN` or `AUTOMOTIVE_DESIGN_CC2`) and millimetres. Models from other sources may use AP203, AP242, inches or metres; that was not sampled.

### (b) Write a board with real solids as an assembly

The board is a prism: the Board Outline extruded by the board thickness, with holes cut through it. Its faces are planes and cylinders, its edges lines and circles (plus arcs of circles where the outline has arcs). Nothing in it needs boolean operations in 3D if the holes are subtracted from the outline in 2D first, which REDA's polygon code already does for other purposes.

The file needs three more things:

- **Product structure**: a `PRODUCT` chain per part, and for each placed part a `NEXT_ASSEMBLY_USAGE_OCCURRENCE` with a `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` and `ITEM_DEFINED_TRANSFORMATION` that carry its placement. These are the entities `truck-stepio`'s assembly writer emits ([`out/assy.rs`](https://github.com/ricosjp/truck/blob/117470c2e2/truck-stepio/src/out/assy.rs)) and the ones tscircuit's merger strips from an imported model before giving it new ones ([`excluded-entity-types.ts`](https://github.com/tscircuit/circuit-json-to-step/blob/main/lib/step-model-merger/excluded-entity-types.ts)).
- **Colours**, through the chain above.
- **Component bodies.** A generated Body (a box, a cylinder, a revolved shape) is the same kind of solid as the board. An imported model is arbitrary B-rep, and writing it out again means either understanding it (a kernel's data model) or copying its entities through (section 5).

## 2. OpenCascade bindings

OpenCascade Technology (OCCT) is the C++ kernel every open EDA tool surveyed here uses (section 6). Its STEP translator reads AP214, AP203 and parts of AP242, and writes AP214, AP203 or AP242; colours, names, layers and assembly structure go through the XDE classes `STEPCAFControl_Reader` and `STEPCAFControl_Writer`. [OCCT STEP translator guide](https://occt3d.com/dev/doc/overview/html/occt_user_guides__step.html)

**Licence.** "OCCT is distributed under LGPL 2.1 with an additional exception." [occt3d.com](https://occt3d.com/open-cascade-technology/index.html) The exception lets object code that incorporates material from OCCT's *header files* be distributed "under terms of your choice", with a prominent notice. [`OCCT_LGPL_EXCEPTION.txt`](https://github.com/Open-Cascade-SAS/OCCT/blob/master/OCCT_LGPL_EXCEPTION.txt) It covers headers, not the library itself. ADR-0002 excludes linked LGPL code without qualification, so every binding below fails it as written, whatever the wrapper's own licence.

### `opencascade-rs` (`opencascade`, `opencascade-sys`, `occt-sys`)

| | |
|---|---|
| Latest | 0.3.0, published 2026-08-24; created 2023-01-27 |
| Licence | LGPL-2.1 (all three crates) |
| Downloads (all / recent) | `opencascade` 7.1k / 2.0k; `occt-sys` 196k / 92k |
| Repo | 275 stars, 65 open issues and PRs, last push 2026-11-03 |

- **Maturity.** The README opens: "The code is currently a major work in progress", by one author working in spare time. [README](https://github.com/bschwind/opencascade-rs/blob/3b6d77be7b/README.md)
- **STEP coverage is narrower than OCCT's.** The bridge binds only `STEPControl_Reader` and `STEPControl_Writer`, the basic translator, and the writer bridge carries the comment "TODO(bschwind) - Expose the full list of parameters here". [`step_control.rs`](https://github.com/bschwind/opencascade-rs/blob/3b6d77be7b/crates/opencascade-sys/src/step_control.rs) `TKXCAF` is in the link list, but no XDE class is bridged, so colours, names and assembly structure are not reachable without writing more C++ bindings. "Support parts assembly" has been an open issue since 2023-07-03. [issue #70](https://github.com/bschwind/opencascade-rs/issues/70)
- **Build.** Needs CMake and a C++ compiler. By default (`builtin` feature) `occt-sys` compiles OCCT from source and links it statically; the alternative is dynamic linking against a system OCCT. [README](https://github.com/bschwind/opencascade-rs/blob/3b6d77be7b/README.md), [`occt-sys/src/lib.rs`](https://github.com/bschwind/opencascade-rs/blob/3b6d77be7b/crates/occt-sys/src/lib.rs) The `occt-sys` crate is 14.4 MB compressed, and crates.io raised its size limit to host it. [`occt-sys` README](https://github.com/bschwind/opencascade-rs/blob/3b6d77be7b/crates/occt-sys/README.md)
- **Platforms.** All four CI jobs run on `ubuntu-latest`. On Windows the maintainer wrote "I also don't currently own a windows machine so I can't provide a lot of support for builds on windows" and recommended a separately installed OCCT. [issue #179](https://github.com/bschwind/opencascade-rs/issues/179)
- **Cost reported by users.** One user reported the linker using 3 GB of RAM and a 270 MB `bottle` example executable; the maintainer replied "right now the resource consumption is absolutely massive" and that dynamic linking gave a 488 KB binary. The report does not say whether it was a debug or release build. [issue #171](https://github.com/bschwind/opencascade-rs/issues/171) An earlier issue shows a release build of one example finishing in 4 min 43 s on the maintainer's machine. [issue #95](https://github.com/bschwind/opencascade-rs/issues/95)

### `cadrum` (formerly `chijin`)

| | |
|---|---|
| Latest | 0.9.2 in the repo; `chijin` was created 2026-03-02 and is now marked deprecated in favour of `cadrum` |
| Licence | MIT for the wrapper; "Compiled binaries include OpenCASCADE Technology (OCCT), which is licensed under the LGPL 2.1. Users who distribute applications built with cadrum must comply with the LGPL 2.1 terms." |
| Repo | 75 stars, last push 2026-11-19 |

- **Packaging.** Its build script downloads a prebuilt static OCCT 8.0.1 for `x86_64`/`aarch64` Linux, `x86_64` Windows (MSVC and GNU), both macOS architectures and `wasm32-unknown-unknown`, so no CMake run or system OCCT is needed. Other targets build OCCT from source and need CMake and a C++17 compiler. The build needs network access to GitHub Releases. [README](https://github.com/lzpel/cadrum/blob/c0873f5c1d/README.md)
- **Size indication.** The prebuilt static archives are 17.8 to 42.1 MB each as `.tar.xz` (Linux x86_64 21.7 MB, macOS arm64 17.8 MB, Windows MSVC 39.0 to 42.1 MB). [release `occt-8_0_1_rev4`](https://github.com/lzpel/cadrum/releases) These are whole-library archives, not the size they add to a linked executable.
- **Coverage.** Reads and writes STEP with "per-face and per-solid color preserved across STEP / BRep / STL / glTF / SVG round-trips", and tessellates. Its STEP writer overrides OCCT so that "a compound is never split into an assembly, so the solids share one shape representation". [README](https://github.com/lzpel/cadrum/blob/c0873f5c1d/README.md), [`ffi.cpp`](https://github.com/lzpel/cadrum/blob/c0873f5c1d/src/ffi.cpp)

### OCCT's own size

Debian trixie's OCCT 7.8.1 shared-library packages, installed size on amd64: foundation 5.1 MB, modeling-data 8.9 MB, modeling-algorithms 29.2 MB, ocaf 6.0 MB, data-exchange 20.6 MB, together 69.8 MB. The data-exchange package also depends on the visualization package (7.5 MB). [packages.debian.org](https://packages.debian.org/trixie/libocct-data-exchange-7.8)

### What it cannot do

Pass ADR-0002. Technically OCCT does both jobs; through `opencascade-rs` the assembly and colour parts would need new bindings.

## 3. Pure-Rust kernels and STEP crates

### `truck` (RICOS)

| | |
|---|---|
| Latest release | `truck-stepio` 0.3.0, `truck-modeling` 0.6.0, `truck-meshalgo` 0.4.0, all published 2024-09-20 |
| Licence | Apache-2.0 |
| Downloads (all / recent) | `truck-stepio` 150k / 104k; `truck-modeling` 219k / 153k |
| Repo | 1,701 stars, 45 open issues and PRs, last commit 2026-11-23 |

- **Reading.** `truck-stepio` parses with `ruststep` and converts to truck's B-rep. Its input table covers lines, polylines, circles, ellipses, hyperbolas, parabolas, B-spline, Bézier and rational curves; planes, spheres, cylinders, cones, tori, B-spline, Bézier and rational surfaces, linear extrusions and revolutions; the topology entities; and the product and assembly entities. [`in/mod.rs`](https://github.com/ricosjp/truck/blob/117470c2e2/truck-stepio/src/in/mod.rs) `truck-meshalgo` then tessellates, and the `step-to-mesh` example does the whole path to an OBJ file. [`step-to-mesh.rs`](https://github.com/ricosjp/truck/blob/117470c2e2/truck-stepio/examples/step-to-mesh.rs)
- **No colours.** A search of `truck-stepio/src` for `colour`, `color` and `styled_item` finds nothing. A coloured mesh would need REDA to read the presentation chain itself and match it to truck's faces.
- **Writing.** Emits `MANIFOLD_SOLID_BREP` and `BREP_WITH_VOIDS`, and on master a full assembly with `NEXT_ASSEMBLY_USAGE_OCCURRENCE`. [`out/topology.rs`](https://github.com/ricosjp/truck/blob/117470c2e2/truck-stepio/src/out/topology.rs), [`out/assy.rs`](https://github.com/ricosjp/truck/blob/117470c2e2/truck-stepio/src/out/assy.rs) `truck-modeling` builds the solids: `try_attach_plane` makes a planar face from wires (outline plus holes), `tsweep` extrudes and `rsweep` revolves. [`builder.rs`](https://github.com/ricosjp/truck/blob/117470c2e2/truck-modeling/src/builder.rs) No colour output.
- **Released crates lag master by two years.** "Read assembly from step file" and "Write assembly to step file" are under "Unreleased" in the changelog, and `truck-assembly` is not on crates.io. [CHANGELOG](https://github.com/ricosjp/truck/blob/117470c2e2/CHANGELOG.md) An open issue asks for a release because fixes for "a boolean panic and a hang" are "only on master". [issue #135](https://github.com/ricosjp/truck/issues/135) Using master means a git dependency.
- **Open STEP defects**, all filed in 2026: import fails on a B-spline face with a `VERTEX_LOOP` boundary (#144); files from Fusion fail to load because an optional attribute is declared required, and a cone to a point is meshed the wrong way (#136); an edge that fails to convert is dropped with only an `eprintln` and later edges are misnumbered (#133); two panics on placements (#123, #128). [issues](https://github.com/ricosjp/truck/issues)
- **Dependencies.** `cargo tree -p truck-stepio -e normal` on master lists 85 distinct crates (99 with `truck-meshalgo`), among them `chrono`, `rayon`, `regex`, `nom`, `cgmath` and `serde`. Declared licences are MIT, Apache-2.0, BSD-2-Clause, Zlib, Unlicense-or-MIT and Unicode-3.0 combinations, all on ADR-0002's list or its `deny.toml`; `truck-assembly` has no `license` field in its manifest. `chrono` is already the subject of an advisory exception in REDA's `deny.toml`.
- **Build and packaging.** Pure Rust; no C++ toolchain. Binary size and compile time were not measured.

### `ruststep` (RICOS)

Apache-2.0; 0.4.0 published 2024-09-20; 177 stars; last commit 2025-03-20. The README's first line is "This project is still in experimental stage. DO NOT USE FOR PRODUCT." It deserialises Part 21 into Rust structs generated from EXPRESS schemas; serialising back to STEP is on its "TODO until 1.0" list. [README](https://github.com/ricosjp/ruststep/blob/cd89e5a7cd/README.md) The `monstertruck` README says two syntax fixes real exports need (`''` as an escaped apostrophe and `()` as an empty aggregate) "had sat unreleased on ruststep master for four years", which is why it published a fork, `step-p21`. [monstertruck README](https://github.com/virtualritz/monstertruck/blob/467ac2f50b/README.md)

### `step-io`

| | |
|---|---|
| Latest | 0.3.1, published 2026-11-18; created 2026-04-24; 10 releases |
| Licence | MIT OR Apache-2.0 |
| Downloads (all / recent) | 15.9k / 15.5k |
| Repo | 5 stars, 0 open issues, one author |

- **What it is.** A file-format layer, not a kernel: "reads every mainstream AP, writes only modern AP242", marked "Experimental — early stage; expect breaking API changes at any time". [README](https://github.com/elgar328/step-io/blob/4865dad8f4/README.md)
- **Reading.** Models 501 entity types, among them `colour_rgb`, `styled_item`, `next_assembly_usage_occurrence`, `mapped_item`, `advanced_face` and the B-spline and analytic surfaces. [`docs/entities.md`](https://github.com/elgar328/step-io/blob/4865dad8f4/docs/entities.md) Its scene layer exposes B-rep geometry, "the assembly tree with placements", and "presentation (colour / layer / visibility)". [`lib.rs`](https://github.com/elgar328/step-io/blob/4865dad8f4/src/lib.rs)
- **It does not tessellate.** Its `Mesh` type reads only the pre-triangulated AP242 tessellated items some files carry. [`scene/mesh.rs`](https://github.com/elgar328/step-io/blob/4865dad8f4/src/scene/mesh.rs) The eight KiCad models sampled carry none.
- **Writing.** `StepBuilder` takes vertices, edges, faces and solids from the caller and has `part`, `place`, `style` and `layer` methods; output is AP242 edition 2. The caller supplies the topology; nothing extrudes for it.
- **Dependencies.** Two direct (`logos`, `memchr`); 12 crates in the normal tree, all MIT or Apache-2.0 options. `unsafe_code = "deny"`. About 230,000 lines of Rust, mostly generated from the schemas.

### Foxtrot (Formlabs)

MIT OR Apache-2.0; 347 stars; last commit 2021-08-13. Not published: the `foxtrot` crate on crates.io is an unrelated Bevy demo. It parses with a 39,611-line parser generated from the AP214 schema, which "take[s] a _very_ long time to compile", triangulates with its own constrained Delaunay and NURBS crates, and reads `STYLED_ITEM` colours. [README](https://github.com/Formlabs/foxtrot/blob/69fb376888/README.md), [`triangulate.rs`](https://github.com/Formlabs/foxtrot/blob/69fb376888/triangulate/src/triangulate.rs) Its README says: "Foxtrot is a proof-of-concept demo, not an industrial-strength CAD kernel. It may not work for your models! Even in the screenshots above, there are a handful of surfaces that it fails to triangulate", and "No one at Formlabs is paid to maintain this". It has no STEP writer.

### Fornjot

0BSD. The repository was archived on 2026-06-19 and its README now begins "This project has been shut down. Its goals were never reached." [README](https://github.com/hannobraun/fornjot/blob/7e856686b8/README.md) The author's closing post says reaching a useful foundation would take "another 2-3 years". [Shutting Down Fornjot](https://www.fornjot.app/blog/shutting-down-fornjot/) The last crates.io release is 0.49.0 from 2024-03-21. It never had STEP import or export in the crates listed in its README.

### Newer kernels

| Crate | Created | Latest | Licence | Note |
|---|---|---|---|---|
| `monstertruck` | repo 2023-02, crate 2026-03-03 | 0.4.3, 2026-11-23 | Apache-2.0 | Fork of `truck` with a rewritten tessellation pipeline, STEP assembly output and the `step-p21` parser; 42 stars. [README](https://github.com/virtualritz/monstertruck/blob/467ac2f50b/README.md) No colour handling found in its STEP module |
| `arris` | 2026-09-05 | 0.12.0, 2026-11-23 | MIT OR Apache-2.0 | New kernel; README claims STEP reading with repair, assemblies, tessellation and AP214 export; "Pre-1.0: the API breaks between minor versions"; 4 stars, 553 downloads. [README](https://github.com/arris-labs/arris/blob/4408202aa4/README.md) |
| `brepkit-io` | 2026-08-07 | 4.11.44, 2026-11-23 | AGPL-3.0-only | Excluded by licence |
| `BREP_kernel` | 2026-08-25 | 0.27.0, 2026-11-23 | "non-standard" on crates.io | The project Fornjot's site points to; licence text not read |
| `vcad` | 2026-01-27 | 0.10.0 | MIT | A search result says its STEP export needs OpenCASCADE; not checked |
| `cadk` | 2025-07-11 | 0.1.0 | MIT | No STEP |

None of the claims in this table was tested.

## 4. A narrow in-house STEP writer

**An open tool ships one.** tscircuit's `circuit-json-to-step` (MIT, TypeScript) writes a board with holes, component bodies and an assembly as one STEP string, with no kernel: its dependencies are `stepts`, a typed STEP entity library with no dependencies of its own, plus polygon clipping and format converters. [package.json](https://github.com/tscircuit/circuit-json-to-step/blob/main/package.json), [`stepts`](https://github.com/tscircuit/stepts) It has separate modules for circular, oval, pill and rectangular hole geometry. Whether its output opens cleanly in each MCAD tool was not tested here.

**Size of the job, by comparison.** `truck-stepio`'s whole output side is about 1,900 lines of Rust (`out/mod.rs` 402, `geometry.rs` 819, `topology.rs` 381, `assy.rs` 300), and that covers B-splines and every analytic surface, more than a prism writer needs.

**What it needs to emit** for a prism with holes: the topology entities, `PLANE` and `CYLINDRICAL_SURFACE`, `LINE` and `CIRCLE`, placements, the product chain, a units-and-uncertainty context, the assembly entities and the colour chain, all listed in section 1. A revolved body adds `CONICAL_SURFACE`, `TOROIDAL_SURFACE` or `SPHERICAL_SURFACE` depending on its profile. The Part 21 syntax itself is plain text.

**Cost.** No dependency, no build or packaging cost, negligible binary size.

**What it cannot do.** Read anything. Write shapes it has no closed form for: fillets, chamfers on curved edges, or the union of overlapping bodies. Catch its own mistakes: a kernel validates topology as it builds, a hand-written emitter does not, so correctness rests on tests that open the output in real MCAD tools. Write an imported model, except by re-embedding.

## 5. Re-embedding an imported model's entities

**The format allows it.** In Part 21 an instance name is `#` plus digits; it must be "unique within the exchange structure"; "The entity instances need not be ordered in the exchange structure"; and "An instance name may be referenced before it is defined." [ISO 10303-21 clauses 6.4.4.3 and 11.2](https://www.steptools.com/stds/step/IS_final_p21e3.html) The numbers carry no meaning beyond identity, so a model's instances can be renumbered into a free range of the output file by rewriting every `#n` token, with no knowledge of what each entity is.

**Two open tools do it.**

- tscircuit's `step-model-merger` parses an imported STEP, drops 14 product-structure entity types (`PRODUCT`, `PRODUCT_DEFINITION`, `SHAPE_DEFINITION_REPRESENTATION`, `ADVANCED_BREP_SHAPE_REPRESENTATION`, `NEXT_ASSEMBLY_USAGE_OCCURRENCE` and the like), allocates new ids, remaps references, and gives the geometry a new product and placement. Its README says imported parts "retain their geometry, colors, model origins, uniform unit scales, rotations, and placements", and states the limit: "Imported STEP files that contain their own assembly occurrences are rejected ... nested imported assembly hierarchies are not yet supported." [`step-model-merger.ts`](https://github.com/tscircuit/circuit-json-to-step/blob/main/lib/step-model-merger.ts), [`excluded-entity-types.ts`](https://github.com/tscircuit/circuit-json-to-step/blob/main/lib/step-model-merger/excluded-entity-types.ts), [README](https://github.com/tscircuit/circuit-json-to-step/blob/main/README.md)
- LibrePCB's `OccModel::minifyStep` rewrites a STEP file as text without the kernel: it splits the data section into instances, extracts `#n` references by regular expression, merges duplicates and renumbers. A comment in it records a hazard: "It seems some entries must not be merged even if they are identical. When merged, the STEP model won't be rendered anymore and FreeCAD displays a wrong shape object tree." [`occmodel.cpp`](https://github.com/LibrePCB/LibrePCB/blob/667da84736/libs/librepcb/core/3d/occmodel.cpp)

**What the embedder must still understand**, even when treating geometry as opaque:

- **The roots.** It has to find the model's shape representation and attach it to a new product placed in the assembly, so the product-structure entities are parsed, not copied.
- **Units.** Each model's representation context names its own length unit. The eight sampled files are all millimetres; a file in inches or metres keeps its own context when copied, and the placement must account for it. tscircuit's README limits itself to "uniform unit scales".
- **One schema per output file.** `FILE_SCHEMA` "identifies the EXPRESS schemas that specify the entity instances in the data sections" (clause 8.2.4). Copying AP203 or AP242 entities into a file that declares AP214 produces instances the declared schema may not define. How strictly each MCAD importer checks this was not tested.
- **Strings and comments.** A textual `#n` rewrite must skip quoted strings and `/* */` comments.

**What it cannot do.** Draw the model: a mesh still needs tessellation. Repair a broken model, change it, or merge it with other geometry. Convert between schemas. Guarantee the result is valid when the source was not.

## 6. How open EDA tools do it

| Tool | Licence | Reads STEP for the 3D view | Writes board STEP | Kernel |
|---|---|---|---|---|
| KiCad | GPL-3.0-or-later | `STEPCAFControl_Reader` with colour mode on, then `BRepMesh_IncrementalMesh` per face | Builds the board with `BRepPrimAPI_MakePrism` and `BRepAlgoAPI_Cut`, loads each model into an XDE document, writes with `STEPCAFControl_Writer` | OCCT, required |
| Horizon EDA | GPL-3.0 | Same classes | `BRepPrimAPI_MakePrism`, `BRepAlgoAPI_Cut`, `STEPCAFControl_Writer` with colour mode | OCCT (or OCE), required |
| LibrePCB | GPL-3.0-or-later | Same classes | Same classes | OCCT or OCE, optional: `USE_OPENCASCADE=0` builds without STEP features |
| tscircuit | MIT | n/a in this package | Own writer plus entity re-embedding | None |

- **KiCad** refuses to configure without OCCT: `find_package(OCC)` followed by a fatal error "OpenCASCADE was not found!", and a version check against 7.5.0. [`CMakeLists.txt`](https://gitlab.com/kicad/code/kicad/-/blob/master/CMakeLists.txt) The viewer's model loader is [`plugins/3d/oce/loadmodel.cpp`](https://gitlab.com/kicad/code/kicad/-/blob/master/plugins/3d/oce/loadmodel.cpp); the exporter is [`pcbnew/exporters/step/step_pcb_model.cpp`](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/exporters/step/step_pcb_model.cpp), 4,500 lines, which also reads IGES and VRML models and writes glTF, BREP, XAO, STL, PLY, U3D and PDF from the same document. Its documentation says Footprint models "can be in STEP, VRML, or IDF format", that "VRML models cannot be used in STEP, BREP, or XAO exports", and that the exporter can "look for an identically named STEP model" instead. [PCB editor docs source](https://gitlab.com/kicad/services/kicad-doc/-/blob/master/src/pcbnew/pcbnew_generating_outputs.adoc)
- **Horizon EDA**: [`step_importer.cpp`](https://github.com/horizon-eda/horizon/blob/79e3422646/src/import_step/step_importer.cpp), [`export_step.cpp`](https://github.com/horizon-eda/horizon/blob/79e3422646/src/export_step/export_step.cpp), [`meson.build`](https://github.com/horizon-eda/horizon/blob/79e3422646/meson.build).
- **LibrePCB**: [`occmodel.cpp`](https://github.com/LibrePCB/LibrePCB/blob/667da84736/libs/librepcb/core/3d/occmodel.cpp), [`FindOpenCascade.cmake`](https://github.com/LibrePCB/LibrePCB/blob/667da84736/cmake/FindOpenCascade.cmake). It refuses OCCT 7.8.0 "because it contains a critical bug", and widened its build precision after 1e-6 "caused STEP export errors in rare cases".
- **All three GPL tools make the board the same way**: a 2D face from the outline, holes cut, one prism extrusion. None of them re-embeds; each loads every model into the kernel and writes it out again.
- **Differences.** Only LibrePCB can be built without the kernel. Only KiCad accepts non-STEP models (VRML, IDF) on a Footprint. Only tscircuit writes STEP without a kernel, and it is the only one of the four under a permissive licence.

Fusion Electronics, EasyEDA Pro, OrCAD X, DipTrace and Altium Designer are closed; what they use internally was not researched.

## 7. Cost comparison

| | Maturity | Linux / Windows / macOS build | Size |
|---|---|---|---|
| `opencascade-rs` | "major work in progress"; 0.3.0 | CMake and C++ on all three; CI on Linux only; maintainer cannot support Windows | OCCT shared libs about 70 MB installed (Debian); user report of a 270 MB static example, build profile unstated |
| `cadrum` | 0.9.x, 9 months old | Prebuilt static OCCT downloaded at build time for all three | Prebuilt archives 18 to 42 MB compressed |
| `truck` | Kernel active since 2020; releases stalled since 2024-09 | Pure Rust | 85 to 99 crates in the tree; binary size not measured |
| `step-io` | 7 months old, "Experimental" | Pure Rust | 12 crates in the tree; binary size not measured |
| Foxtrot | Unmaintained since 2021 | Pure Rust; generated parser slow to compile | Not measured |
| In-house writer | n/a | None | Negligible |
| Re-embedding | n/a | None | Negligible |

## 8. Not verified

- **No builds or runs.** Binary size and compile time for `truck`, `step-io`, Foxtrot, `cadrum` and `opencascade-rs` were not measured. `truck`'s and Foxtrot's readers were not run against the downloaded component models, so how many real models each tessellates correctly is unknown.
- **MCAD acceptance.** No STEP file written by `truck-stepio`, `step-io` or tscircuit was opened in FreeCAD, Fusion, SolidWorks or any other MCAD tool.
- **Mixed-schema files.** Whether MCAD importers accept AP203 or AP242 entities re-embedded in a file declaring AP214 was not tested.
- **CAx-IF recommended practices.** The "Model Styling and Organization" v1.10 PDF was downloaded from mbx-if.org but could not be read (no PDF text tool available). The colour chain above is taken from the sampled files, not from that document. The assembly recommended practice was not read either.
- **LGPL-2.1 terms.** The licence's static-linking conditions were not analysed; ADR-0002 excludes LGPL regardless. The OCCT exception text was read in full.
- **The 270 MB figure** in `opencascade-rs` issue #171 does not state the build profile.
- **KiCad's own Footprint library.** The KiCad documentation (master) says library Footprints reference only VRML models; the library itself was not checked.
- **`BREP_kernel`'s licence**, **`vcad`'s kernel dependency**, and every capability claim in the "Newer kernels" table.
- **Model sample.** Eight KiCad models, all AP214 and millimetres. Manufacturer and distributor models were not sampled.
- **Closed tools** were not researched.

## Sources

Code, read at these commits:

- `opencascade-rs` [3b6d77be7b](https://github.com/bschwind/opencascade-rs/tree/3b6d77be7b) (2026-11-03)
- `truck` [117470c2e2](https://github.com/ricosjp/truck/tree/117470c2e2) (2026-11-23)
- `ruststep` [cd89e5a7cd](https://github.com/ricosjp/ruststep/tree/cd89e5a7cd) (2025-03-20)
- Fornjot [7e856686b8](https://github.com/hannobraun/fornjot/tree/7e856686b8) (2026-06-19)
- Foxtrot [69fb376888](https://github.com/Formlabs/foxtrot/tree/69fb376888) (2021-08-13)
- `cadrum` [c0873f5c1d](https://github.com/lzpel/cadrum/tree/c0873f5c1d) (2026-11-19)
- `monstertruck` [467ac2f50b](https://github.com/virtualritz/monstertruck/tree/467ac2f50b) (2026-11-23)
- `step-io` [4865dad8f4](https://github.com/elgar328/step-io/tree/4865dad8f4) (2026-11-18)
- `arris` [4408202aa4](https://github.com/arris-labs/arris/tree/4408202aa4) (2026-11-22)
- Horizon EDA [79e3422646](https://github.com/horizon-eda/horizon/tree/79e3422646) (2026-11-17)
- LibrePCB [667da84736](https://github.com/LibrePCB/LibrePCB/tree/667da84736) (2026-11-21)
- KiCad `master` on [gitlab.com/kicad/code/kicad](https://gitlab.com/kicad/code/kicad) and [kicad-doc](https://gitlab.com/kicad/services/kicad-doc)
- tscircuit [`circuit-json-to-step`](https://github.com/tscircuit/circuit-json-to-step) 0.0.63 and [`stepts`](https://github.com/tscircuit/stepts) 0.0.7, `main`
- Sample models from [kicad-packages3D](https://gitlab.com/kicad/libraries/kicad-packages3D), `master`

Documents:

- [OCCT STEP translator user guide](https://occt3d.com/dev/doc/overview/html/occt_user_guides__step.html) (first 100,000 characters of the page)
- [OCCT overview and licence statement](https://occt3d.com/open-cascade-technology/index.html), [`OCCT_LGPL_EXCEPTION.txt`](https://github.com/Open-Cascade-SAS/OCCT/blob/master/OCCT_LGPL_EXCEPTION.txt)
- [ISO 10303-21 edition 3 text at STEP Tools](https://www.steptools.com/stds/step/IS_final_p21e3.html) (first 200,000 characters)
- [Foxtrot write-up](https://mattkeeter.com/projects/foxtrot/)
- [Shutting Down Fornjot](https://www.fornjot.app/blog/shutting-down-fornjot/)
- [Debian trixie OCCT packages](https://packages.debian.org/trixie/libocct-data-exchange-7.8)
- crates.io API, `https://crates.io/api/v1/crates/<name>`, for every crate named
