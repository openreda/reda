# truck as released: STEP trial harness (issue #442)

Throwaway harness for `../SPEC.md`. Binary: `truck-released read <file>` and
`truck-released write <out.step> <component.step>`.

## Versions

- crates.io releases, all published 2024-09-20, Apache-2.0: truck-stepio 0.3.0 (git
  `c23d2c25c1cd83b615d35db72cc5a4432c693e17` per the crate's `.cargo_vcs_info.json`),
  truck-modeling 0.6.0, truck-meshalgo 0.4.0, truck-topology 0.6.0, truck-geometry 0.5.0,
  truck-polymesh 0.6.0, truck-base 0.5.0, truck-geotrait 0.4.0, truck-derivers 0.1.0.
- ruststep 0.4.0 (Part 21 parser), spade 2.15.1, rayon 1.12.0. 118 packages in `Cargo.lock`.
  truck-meshalgo is built without its `vtk` feature.
- `cargo build --release` passes on Rust 1.96. `cargo check --release` passes for
  `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`.

## Read: what it does

- Parses with `ruststep::parser::parse`, builds `truck_stepio::in::Table`, converts every
  CLOSED_SHELL/OPEN_SHELL with `to_compressed_shell` and tessellates it with
  `robust_triangulation(0.01)`. `TRUCK_TESS=plain` selects `triangulation`.

## Read: what truck could not do

- No colour: the reader has no style or colour entities.
- No solids, products or assemblies: the table has no MANIFOLD_SOLID_BREP, PRODUCT or
  NEXT_ASSEMBLY_USAGE_OCCURRENCE. `solids` and `products` are `null`, placements are never
  applied, and an instanced part appears once, in its own frame.
- Entities that fail to deserialise are printed to stderr and skipped; faces that cannot be
  converted are skipped without a report.
- Tessellation panics on a SPHERICAL_SURFACE whose radius is not larger than the tolerance
  (`espressif/ESP32-C3-MINI-1U.STEP`, `truck-geometry-0.5.0/src/specifieds/sphere.rs:118`).
- Meshes are not always closed: `ap203/470531000.stp` has 58 open edges with all 101 faces
  meshed; the Wurth file has 2 of 26 faces without triangles.

## Write: what it does

- Board: same construction as the master harness (planar face with hole wires, `tsweep`, no
  booleans), in truck-modeling's own geometry types.
- The writer takes one geometry type per file and the reader's types differ from
  truck-modeling's, so the board is written to STEP text and read back to get it into the
  reader's types.
- Component: every shell from the reader, wrapped as a one-shell solid, copied twice with the
  `U1` and `U2` placements applied to the geometry.
- Output: `StepModels` in `CompleteStepDisplay`. Reading it back gives 3 shells, 316 faces, a
  closed mesh and volume 2372.5 mm³ (analytic: 2372.3).

## Write: what truck could not do

- No assembly. The writer emits one PRODUCT with an empty name and one
  ADVANCED_BREP_SHAPE_REPRESENTATION holding all three solids. No `trial-board`, `PCB`, `U1`,
  `U2` names, no shared definition.
- No colour.
- No circle or cylinder in truck-modeling 0.6.0: the board's 28 arcs are rational B-spline
  curves and its 14 arc walls rational B-spline surfaces. Flat walls and caps are PLANE.
- The component's 32 CYLINDRICAL_SURFACE faces are written back as SURFACE_OF_REVOLUTION.
- FILE_SCHEMA is written as `'ISO-10303-042'`, which is not a schema name, and the API has no
  way to set it. The harness writes a second file `<out>.schema-fixed.step` with
  `AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }` by text replacement.

## API friction

- `builder` functions are tied to truck-modeling's `Curve`/`Surface`; they cannot produce the
  reader's types.
- `StepModels` starts entity numbering at a fixed offset behind a hard-coded product block, so
  there is no place to add products or names.
- `Table::from_step` returns `Option`; the parse error is only available by calling ruststep
  directly.
