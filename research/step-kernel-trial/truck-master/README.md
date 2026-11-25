# truck at master: STEP trial harness (issue #442)

Throwaway harness for `../SPEC.md`. Binary: `truck-master read <file>` and
`truck-master write <out.step> <component.step>`.

## Versions

- truck (https://github.com/ricosjp/truck, Apache-2.0) at rev
  `117470c2e2a8ce184177c6d76b41c8dd5b1b2935` (2026-11-23): truck-stepio 0.3.0, truck-assembly
  0.1.0, truck-modeling 0.6.0, truck-meshalgo 0.4.0, truck-topology 0.6.0, truck-geometry 0.5.0,
  truck-polymesh 0.6.0. The version numbers equal the crates.io releases; the code does not.
- ruststep 0.4.0 (Part 21 parser, crates.io), spade 2.15.1 (triangulation), rayon 1.12.0.
- 124 packages in `Cargo.lock`. truck-meshalgo is built without its `vtk` feature.
- `cargo build --release` passes on Rust 1.96. `cargo check --release` passes for
  `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`.

## Read: what it does

- Parses with `ruststep::parser::parse`, builds `truck_stepio::in::Table`, then walks
  `Table::step_assy()` (products, NEXT_ASSEMBLY_USAGE_OCCURRENCE, placements) and tessellates each
  product's shells with `robust_triangulation(0.01)`. `TRUCK_TESS=plain` selects `triangulation`.
- When `step_assy` yields no B-rep it falls back to every shell in the table, in the file frame.

## Read: what truck could not do

- No colour. The reader has no STYLED_ITEM, COLOUR_RGB or presentation entities at all.
- `step_assy` only finds a B-rep that is an item of the representation named by
  SHAPE_DEFINITION_REPRESENTATION. It does not follow SHAPE_REPRESENTATION_RELATIONSHIP to an
  ADVANCED_BREP_SHAPE_REPRESENTATION. KiCad models work; `wurth/L_Wurth_WE-PMI-0805_H1.step` and
  `ap203/470531000.stp` come back as products without shapes.
- PRODUCT_DEFINITION_SHAPE with `$` as description fails to deserialise (the field is a plain
  `String`); the entity is dropped with a line on stderr and the product structure is empty
  (`adafruit/3660 BME680.step`: 78 such lines, 0 products).
- Entities that fail to deserialise are printed to stderr and skipped. The API gives no count
  and no error.
- `to_compressed_shell` skips faces it cannot convert without reporting them.
- Tessellation panics on a SPHERICAL_SURFACE whose radius is not larger than the tolerance:
  `espressif/ESP32-C3-MINI-1U.STEP` has spheres of radius 0.01 and panics at
  `truck-geometry/src/specifieds/sphere.rs:135` with tolerance 0.01.
- Meshes are not always closed: `ap203/470531000.stp` has 42 open edges with all 101 faces
  meshed; the Wurth file has 2 of 26 faces without triangles.

## Write: what it does

- Board: one planar face (outline wire plus five hole wires) from `builder::line`,
  `builder::circle_arc` and `builder::try_attach_plane`, extruded with `builder::tsweep`. No
  boolean operations. The builder is generic at master, so it is run with the STEP reader's
  geometry types (`Curve3D`, `Surface`) and the arcs are true CIRCLE entities.
- Component: read with `step_assy`, its product nodes grafted into the output assembly. `U1` and
  `U2` are two NEXT_ASSEMBLY_USAGE_OCCURRENCE edges to the same product.
- Output: `StepDesign::new(assembly)` and `StepDisplay`. Reading the result back with this
  harness gives 3 products, 2 solids, 169 faces, a closed mesh and volume 2372.5 mm³
  (analytic: 2372.3).

## Write: what truck could not do

- No colour: nothing is read from the component and the writer has no colour entities.
- `tsweep` makes every side wall a SURFACE_OF_LINEAR_EXTRUSION, also the flat ones. The harness
  replaces them by hand with 6 PLANE and 14 CYLINDRICAL_SURFACE (`analytic_walls` in
  `src/main.rs`). `TRUCK_RAW_SWEEP=1` keeps truck's own output.
- FILE_SCHEMA is written as `'ISO-10303-203'`, which is not a schema name, and the API has no
  way to set it. The harness writes a second file `<out>.schema-fixed.step` with
  `CONFIG_CONTROL_DESIGN` by text replacement.
- The product structure is AP203-style: DESIGN_CONTEXT, MECHANICAL_CONTEXT, a plain
  SHAPE_REPRESENTATION holding the MANIFOLD_SOLID_BREP, no APPLICATION_PROTOCOL_DEFINITION, no
  PRODUCT_RELATED_PRODUCT_CATEGORY.
- REPRESENTATION_RELATIONSHIP is written as (parent, child); the Wurth sample writes
  (child, parent). Not tested against an MCAD importer here.
- When `step_assy` exposes no B-rep for the component, the harness takes the solids straight
  from the entity table as one product.

## API friction

- Geometry types are split: truck-modeling has `Curve`/`Surface` (no circle, no cylinder), the
  STEP reader has `Curve3D`/`Surface`. A STEP file has one geometry type, so mixing built and
  imported parts means building with the reader's types.
- `StepModel` borrows the compressed shape, so every shape must outlive the assembly.
- `StepDesign<Model, Models, Matrix>` needs its type parameters spelled out.
- `Table::from_step` returns `Option`; the parse error is only available by calling ruststep
  directly.
- Entity numbering is by precomputed `StepLength`; adding an entity kind (colour, say) means
  implementing `DisplayByStep` and `StepLength` inside the writer's scheme.
