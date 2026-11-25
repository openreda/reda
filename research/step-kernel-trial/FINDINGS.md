# STEP kernel trial: what each option does with real models and real importers

Trial for issue #442, run on 2026-11-25, following the desk research in #422 ([`research/step-kernel-options`](https://github.com/openreda/reda/blob/research/step-kernel-options/docs/research/step-kernel-options.md)), which built and ran nothing. Everything below was built and run on Linux x86_64 (Ubuntu 24.04, 16 cores, 31 GB) with Rust 1.96.1, unless a line says otherwise.

The harness contract is [`SPEC.md`](SPEC.md). Each candidate is a standalone package in this folder with its own `README.md` (versions, API friction, what it could not do). Raw results are in [`results/`](results/): one JSON line per model per reader, the FreeCAD reference figures, and [`results/models.tsv`](results/models.tsv), which lists every sample model and where it came from. The models and the written STEP files are **not** committed: the vendor models' terms do not allow redistribution (see #426).

## Summary

| Option | Licence | Reads real models (of 58, right volume) | Reader colours | Writes board + assembly + colours, accepted by FreeCAD | Stripped binary | Clean release build |
|---|---|---|---|---|---|---|
| OpenCascade via `opencascade-rs` | LGPL-2.1 (excluded by ADR-0002) | 54 | not exposed by the binding | binding exposes one shape, no assembly or colours (not tried) | 16.7 MB | 4 min 46 s |
| solidcore (SolidCraft) | Apache-2.0, crates MIT OR Apache-2.0 | 51 | yes, per face, 55 of 55 | yes | 36.7 MB | 3 min 6 s |
| truck 0.3 (released) | Apache-2.0 | 38, plus 3 crashes | no | geometry only: no assembly, names or colours | 2.6 MB | 17 s |
| truck master `117470c2` | Apache-2.0 | 35, plus 3 crashes | no | assembly yes, colours no; schema header invalid | 2.8 MB | 19 s |
| Foxtrot `69fb3768` | MIT OR Apache-2.0 | 35 | per solid only, 22 of 55 | does not write | 2.7 MB | 17 s |
| step-io 0.3.1 | MIT OR Apache-2.0 | does not tessellate (parses 58 of 58) | yes, 55 of 55 | yes (component rebuilt entity by entity) | 6.1 MB | 48 s |
| In-house writer + re-embedder | REDA's own | does not read | n/a | yes, from about 1,140 lines with no dependencies | 0.6 MB | 0.9 s |

- **No permissive reader matches OpenCascade, but solidcore comes close.** OpenCascade draws 54 of 58 models at the right volume, solidcore 51, and the rest 35 to 38. solidcore is the only reader that shows every model's colours, and it never crashed.
- **Writing needs no kernel.** The in-house writer, step-io and solidcore all wrote the trial board, kept the component's colours and placed two copies of one definition. FreeCAD opened all three as valid solids with the right volume, names, placements and colours, and BRL-CAD built the board from all three.
- **truck is weak on both jobs.** It has no colours and a third of models came out wrong or crashed. One Würth model made the released crates allocate 26 GB until the out-of-memory killer stopped them.
- **Foxtrot fails outright on a third of models** and ignores per-face colours, which is how KiCad and Würth colour their models.

## 1. Method

### Model sample (58 files)

| Group | Files | Where from | Schemas |
|---|---|---|---|
| KiCad library | 26 | `kicad-packages3D` master: SMD and THT packages, connectors, modules, a BGA, a relay | AP214 (19), AP214 CC2 (7) |
| Würth Elektronik | 14 | Würth's KiCad library | AP242 (6), AP214 (8) |
| Adafruit | 6 | `Adafruit_CAD_Parts` (Fusion exports, some nested assemblies) | AP214 |
| Espressif | 3 | Espressif's KiCad library (5 to 6 MB SolidWorks exports) | AP214 |
| SamacSys / Component Search Engine | 4 | distributor-route models committed in public projects | AP214 |
| AP203 parts | 4 | three manufacturer part models from public projects, plus cadrum's multicolour SolveSpace test file | AP203 |
| Raspberry Pi Pico | 1 | Raspberry Pi's published board model | AP203 |

Ultra Librarian and SnapMagic models were not found in public repositories with a usable path before the GitHub search quota ran out, so the distributor route is represented by SamacSys only. 55 of the 58 files carry colours.

### Reference

FreeCAD 1.1.4 (its release AppImage, run headless) read every model through OpenCascade and gave the volume, area, bounding box and validity used as ground truth. All 58 read; one (Adafruit's TRS jack) was reported not valid by `isValid()`.

### Grading a reader

Each reader ran once per file in a subprocess with a 300 s timeout and a 4 GB address-space cap. The models were tessellated at 0.01 mm chordal deviation or the nearest setting. A result counts as **right volume** when every face meshed and the mesh volume is within 1 % of FreeCAD's. It counts as **closed** when, after welding vertices within 1e-4 mm, no mesh edge is used by only one triangle. On screen, an unclosed mesh shows as hairline cracks; a missing face shows as a hole.

### Grading a writer

Each route wrote the same assembly: a 50 x 30 x 1.6 mm board with rounded corners, four 3.2 mm holes and an 8 mm slot, coloured green, plus the SOIC-8 model placed twice (U1 rotated 90° about Z, U2 unrotated) sharing one definition. Some routes also wrote the AP203 part, the Würth AP242 inductor and the nested BME680 breakout as the component. The files were then checked in:

- **FreeCAD 1.1.4** (OpenCascade XDE): `Part.read` for validity and volume, `Import.insert` for structure and placements, and `ImportGui.insert` under an offscreen display for colours.
- **BRL-CAD 7.46** `step-g`, whose STEP importer is its own (OpenNURBS), not OpenCascade. `mged` would not start (it needs `libssl.so.1.1`, absent on Ubuntu 24.04), so each converted database was tessellated with `g-stl` and measured.

No commercial MCAD tool (SolidWorks, Fusion, Onshape) was tried.

## 2. Reading

| Reader | Parsed | Every face meshed | + right volume | + closed | Colours shown (of 55) | Crashes | Peak memory (max) |
|---|---|---|---|---|---|---|---|
| OpenCascade (`opencascade-rs`) | 58 | 56 | 54 | 44 | 0 (binding) | 0 | 84 MB |
| solidcore | 58 | 51 | 51 | 50 | 55 | 0 | 92 MB |
| truck released | 55 | 51 | 38 | 32 | 0 | 3 | capped at 4 GB |
| truck master | 55 | 47 | 35 | 31 | 0 | 3 | capped at 4 GB |
| Foxtrot | 58 | 37 | 35 | 35 | 22 | 0 | 57 MB |
| step-io | 58 | does not mesh | | | 55 | 0 | 20 MB |

Right volume, by source group:

| Reader | Adafruit (6) | AP203 (4) | Espressif (3) | KiCad (26) | Pico (1) | SamacSys (4) | Würth (14) |
|---|---|---|---|---|---|---|---|
| OpenCascade | 5 | 3 | 1 | 26 | 1 | 4 | 14 |
| solidcore | 4 | 3 | 3 | 25 | 1 | 4 | 11 |
| truck released | 3 | 1 | 1 | 20 | 1 | 4 | 8 |
| truck master | 3 | 1 | 0 | 20 | 1 | 4 | 6 |
| Foxtrot | 4 | 2 | 0 | 17 | 1 | 4 | 7 |

### OpenCascade (comparison only)

- Through `opencascade-rs` `3b6d77be`, which compiles OCCT 7.8.1 from source and links it statically.
- It took CMake 4's `CMAKE_POLICY_VERSION_MINIMUM=3.5` to build; without it CMake 4 rejects the binding's `cmake_minimum_required`.
- Two Espressif modules fail with `UntriangulatedFace`. The 12 unclosed meshes are OpenCascade meshing faces independently at this tolerance; their volumes are right.
- The binding returns one merged shape, so colours, names and the assembly tree cannot be reached. That is a binding limit, not an OpenCascade one: FreeCAD, on the same kernel, showed every colour.

### solidcore

- A pure-Rust B-rep kernel, ported from the SMLib part of NVIDIA's USD BRep (Apache-2.0) and developed inside SolidCraft since 2026-10. Used here at `34d50552` by git dependency.
- **Colours and structure:** colours are exposed per face and per solid, and the assembly tree comes with names, reference designators and placements. Units are converted to millimetres.
- **Failures:** 7 files fail, each by refusing one solid with a message and reading the rest. Three are "not supported yet: a face with several loops around its axis" (Würth), two are "the record's geometry disagrees", one is an open surface of revolution and one an unclosed shell. One more file meshes at the right volume but not closed.
- **One application crate is needed.** The `solidcore` crates leave the trimming of B-spline edges on B-spline surfaces to a caller-supplied `Trims` object. SolidCraft's implementation lives in its application crate `solidcraft-smlib`, which the harness uses by default. Without it the BME680 breakout loses 1 of 22 solids.
- **No single import call.** The caller joins solids, placements and styles by entity id. SolidCraft does this in about 2,000 lines.
- **Maturity:**
  - Its README calls the API "experimental (0.0.x)".
  - The repository pins Rust 1.99.1, but everything used built unchanged on 1.96.1.
  - Its crates are not on crates.io; the git dependency clones the whole SolidCraft repository (about 1.4 GB).
  - The kernel crate is about 1.2 million lines.

### truck

- **Released 0.3 (2024-09):** reads shells only, with no product structure.
- **Master:** adds the assembly reader, but it does not follow `SHAPE_REPRESENTATION_RELATIONSHIP`, so the Würth and AP203 models come back without shapes and the harness meshed them flat.
- **Neither has colours.**
- **Crashes:** two Espressif modules panic ("Tolerance is larger than the radius of sphere": a 0.01 mm sphere). The Würth `D_Wurth_WL-SMRD-1205` diode ran away on memory; uncapped, the released build reached 26 GB resident and was killed by the kernel.
- **Silent losses:** entities that fail to deserialise are skipped with a line on stderr, and the API gives no count.

### Foxtrot

- **Licence:** MIT OR Apache-2.0 at the repository level; four of its five crates have no `license` field.
- **Build:** it builds unchanged on Rust 1.96 in 22 s, but nom 6 and nalgebra 0.27 trigger future-incompatibility warnings.
- **Failures:** 19 models fail in its constrained Delaunay triangulator (`CrossingFixedEdge`, `PointOnFixedEdge` and others), including common KiCad parts: electrolytic caps, a 5 mm LED, the 6 mm push button and USB-C.
- **Colours:** read only from a `STYLED_ITEM` on the solid, so KiCad and Würth models, which colour faces, come out grey.
- **Maintenance:** unmaintained since 2021.

### step-io

- Parses every file and exposes B-rep, colours (55 of 55), the assembly tree and units, but has no tessellator.
- Re-writing a model as read is lossless and keeps the source schema.

## 3. Writing

| Route | FreeCAD: valid, volume, placements | FreeCAD: colours | FreeCAD: names | BRL-CAD `step-g` + `g-stl`: board |
|---|---|---|---|---|
| In-house, AP214 and AP242 headers | yes (2372.3 mm³ = board 2311.9 + 2 x SOIC 30.2) | board green + 3 SOIC colours | PCB, SOIC x 2 | yes, 2312.2 mm³, closed |
| step-io | yes | board green + 3 SOIC colours | PCB, SOIC x 2 (U1/U2 set by text patch) | yes, 2312.3 mm³, closed |
| solidcore | yes | board green + 3 SOIC colours | PCB, SOIC x 2 | yes, 2312.3 mm³, closed |
| truck master | yes | none | PCB, SOIC x 2 | only after fixing the header by hand (`ISO-10303-203` is not a schema name) |
| truck released | yes | none | `Shapes`, `Shapes001`, … | fails, even with a fixed header |

- **The other components:**
  - **In-house and step-io:** the AP203 part and the nested BME680 breakout opened in FreeCAD as valid solids at the right volume, with the expected placements and every source colour (15 distinct in the BME680 file; the AP203 part has none).
  - **In-house only:** the Würth AP242 inductor (itself a five-part assembly) also opened in FreeCAD at the right volume (2850.24 mm³ = board + 2 x 269.15), with its colours. step-io's Würth output was checked only by reading it back with the trial's own readers.
  - **In-house only:** the component text is copied unchanged, so AP203 entities (`CC_DESIGN_*`, `DESIGN_CONTEXT`) sat inside a file declaring AP214. FreeCAD accepted it.
  - **solidcore** was tried with the SOIC only.
- **BRL-CAD and the components:** BRL-CAD failed to tessellate the SOIC component in every route, and also failed on the original KiCad SOIC file. That failure is BRL-CAD's, so the BRL-CAD column records the board and assembly only.
- **In-house route:**
  - **Size:** 393 lines for the board B-rep, product structure and colours; 366 for the re-embedder; 381 for its Part 21 tokenizer. It has no dependencies.
  - **How it re-embeds:** it renumbers the component's entities, drops the component's own application context in favour of the file's, and instances the root product twice.
  - **Speed:** it embedded the 6 MB ESP32-S3 model (76,196 records) in 0.07 s.
  - **Limits:** it reads no component geometry and converts nothing (schema, units). It fails cleanly on a model with several root products.
- **step-io route:**
  - **No model-to-model copy.** step-io has no way to copy a read model into a new one, so the harness rebuilt each component entity by entity through `StepBuilder`, in about 550 lines.
  - **Lost B-spline flags.** That rebuild writes `closed` and `self_intersect` as `.U.`; Foxtrot then refuses those faces, while FreeCAD and truck accept them.
  - **Names and schema.** Occurrence names cannot be set through the API, and output is always AP242.
- **solidcore route:**
  - The board was built by `extrude_planar_regions` with no 3D boolean.
  - Kernel-made edges come out as B-spline curves rather than `LINE` and `CIRCLE` (22 rational, 20 non-rational of 60). The re-written component keeps its analytic curves.
  - Output is AP242 edition 1 only.
- **truck route:** the board needed a hand patch to write planes and cylinders instead of `SURFACE_OF_LINEAR_EXTRUSION`. Released truck has no circles, so arcs become rational B-splines, and it writes the component's cylinders as surfaces of revolution.

## 4. Builds

Clean release builds, one at a time, `-j 16`, crates already downloaded:

| Package | Wall time | Lockfile packages | Stripped binary |
|---|---|---|---|
| In-house | 0.9 s | 1 | 0.6 MB |
| step-io | 48 s | 14 | 6.1 MB |
| Foxtrot | 17 s | 44 | 2.7 MB |
| truck released | 17 s | 118 | 2.6 MB |
| truck master | 19 s | 124 | 2.8 MB |
| solidcore | 3 min 6 s | 53 | 36.7 MB |
| OpenCascade (`opencascade-rs`) | 4 min 46 s | 44 | 16.7 MB (links only `libstdc++`, `libgcc_s`, `libm`, `libc`) |

Binaries are the trial harnesses, so they include a few hundred lines of harness code.

**Windows and macOS:**
- `cargo check --release --target x86_64-pc-windows-msvc` and `--target aarch64-apple-darwin` pass for every permissive candidate.
- Nothing was linked or run on those platforms.
- OpenCascade was not cross-checked: its build script compiles C++ through CMake and needs a C++ toolchain for each target, which this machine lacks.

## 5. Not verified

- **Commercial importers:** SolidWorks, Fusion and Onshape were not tried; FreeCAD and BRL-CAD are the only importers here.
- **Real AP242 product files** beyond Würth's six, and AP242 tessellated geometry, were not tried.
- **Ultra Librarian and SnapMagic** models are not in the sample.
- **solidcore's full import path:** SolidCraft's own (stitching, tolerance healing) was not run, only the `solidcore` crates plus `solidcraft-smlib`.
- **Exact board edges in solidcore:** whether solidcore's public record-assembly API can write the board with `LINE` and `CIRCLE` edges was not tried.
- **OpenCascade colours and assembly** through a binding that exposes XDE (`cadrum`, or a custom `cxx` bridge) were not built; FreeCAD shows that OpenCascade itself handles both.
