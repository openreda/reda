# In-house route: no crate at all (issue #442)

One package, `std` only, no dependencies. Rust 1.96 (the pinned toolchain). Nothing was fetched
or vendored; the tscircuit writer and the sample files were read for the entity layout only.

Two pieces:

- `src/board.rs`: a narrow STEP writer. It writes the PCB prism directly as Part 21 text.
- `src/embed.rs` with `src/p21.rs`: a re-embedder. It copies a component file's entities into
  the output as text with every `#n` shifted by one offset, and instances the component twice.

`src/check.rs` is the self-check, `src/main.rs` the command line.

## Commands

```
step-trial-inhouse write [--ap242] [--keep-app-context] <out.step> <component.step>
step-trial-inhouse write [--ap242] --board-only <out.step>
step-trial-inhouse check <file.step>
```

`write` prints the JSON line of `SPEC.md` and exits 0 also on failure (`ok` is false, nothing is
written). `check` prints one JSON line and exits 1 when it finds an error. There is no `read`:
this route has no reader and no tessellator.

## What the writer produces

- The PCB as `MANIFOLD_SOLID_BREP > CLOSED_SHELL > 18 ADVANCED_FACE`: top and bottom planes with
  one outer bound and five inner bounds each (four round holes, one slot), four planar and four
  cylindrical outline walls, one cylindrical wall per round hole (a closed circular edge top and
  bottom and one seam edge), two planar and two cylindrical slot walls. Curves are `LINE` and
  `CIRCLE` only. 32 vertices, 48 edges, 28 loops.
- Product structure: `PRODUCT`, `PRODUCT_DEFINITION_FORMATION`, `PRODUCT_DEFINITION`,
  `PRODUCT_DEFINITION_SHAPE`, `SHAPE_DEFINITION_REPRESENTATION`,
  `ADVANCED_BREP_SHAPE_REPRESENTATION`, a geometric context in millimetres and radians with a
  1e-6 mm uncertainty, `PRODUCT_RELATED_PRODUCT_CATEGORY`.
- Colour: one `STYLED_ITEM` on the solid through `PRESENTATION_STYLE_ASSIGNMENT`,
  `SURFACE_STYLE_USAGE`, `SURFACE_SIDE_STYLE`, `SURFACE_STYLE_FILL_AREA`, `FILL_AREA_STYLE`,
  `FILL_AREA_STYLE_COLOUR`, `COLOUR_RGB(0, 0.5, 0.2)`, collected in a
  `MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION`.
- The assembly `trial-board`: a `SHAPE_REPRESENTATION` holding one `AXIS2_PLACEMENT_3D` per
  child, and per child a `NEXT_ASSEMBLY_USAGE_OCCURRENCE`, a `PRODUCT_DEFINITION_SHAPE`, a
  `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION`, the complex
  `(REPRESENTATION_RELATIONSHIP() REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION() SHAPE_REPRESENTATION_RELATIONSHIP())`
  instance and an `ITEM_DEFINED_TRANSFORMATION`. Children: `PCB`, `U1`, `U2`. `U1` and `U2`
  point at the same component `PRODUCT_DEFINITION` and the same representation.
- The schema: `AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }` by default,
  `AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }` with `--ap242`. The
  flag changes the `FILE_SCHEMA` string, the `APPLICATION_CONTEXT` name and the
  `APPLICATION_PROTOCOL_DEFINITION`; every other record is the same in both.

The holes are cut in 2D: each boundary loop is extruded into bottom edges, top edges, vertical
edges and wall faces by one rule. No boolean operation exists in the package.

## What the re-embedder does

1. Tokenizes the component file: statements end at `;` outside strings; strings may hold
   doubled quotes; comments are dropped; records may span lines; a record is a simple instance
   or a complex instance `#n=(A(...)B(...));`.
2. Finds the root: the one `PRODUCT_DEFINITION` that is not the child of any
   `NEXT_ASSEMBLY_USAGE_OCCURRENCE` and has a `SHAPE_DEFINITION_REPRESENTATION`. Its
   representation is the one instanced.
3. Copies every record with each id shifted by the number of records written before it (626 in
   the trial files). Geometry, units contexts and colour chains are not touched.
4. Drops the component's `APPLICATION_CONTEXT` and `APPLICATION_PROTOCOL_DEFINITION` and points
   references to them at the output's. Reason: they name the component's source protocol, the
   output declares one schema, and each application context needs exactly one protocol
   definition, so keeping one without the other is not an option. `--keep-app-context` keeps
   both as they are, to compare importers.
5. Uses an identity `AXIS2_PLACEMENT_3D` among the root representation's items as the source of
   the `ITEM_DEFINED_TRANSFORMATION`. When there is none, it appends a new one to that
   representation's item list. This is the only record whose text is edited.

A component that is itself an assembly (the BME680 breakout, the Würth AP242 file) needs no
special case: its inner occurrences are copied as text and its root product is instanced.

## Schema mixing

A Part 21 file declares one schema; the embedded text is not converted. The `notes` field names
both protocols for each run.

| Output | Component declares | Output declares |
| --- | --- | --- |
| `inhouse-soic-ap214.step` | AP214 (`AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }`) | AP214, same string |
| `inhouse-soic-ap242.step` | AP214 | AP242 |
| `inhouse-ap203-part.step` | AP203 (`CONFIG_CONTROL_DESIGN`) | AP214 |
| `inhouse-wurth242.step` | AP242 (`AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF. {1 0 10303 442 1 1 4 }`) | AP214 |
| `inhouse-bme680.step` | AP214 (`AUTOMOTIVE_DESIGN { 1 0 10303 214 3 1 1 }`) | AP214, other edition string |
| `inhouse-board-only.step` | no component | AP214 |

The AP203 file brings `CC_DESIGN_APPROVAL`, `CC_DESIGN_DATE_AND_TIME_ASSIGNMENT` (2),
`CC_DESIGN_PERSON_AND_ORGANIZATION_ASSIGNMENT` (4), `CC_DESIGN_SECURITY_CLASSIFICATION` and
`DESIGN_CONTEXT` into a file that declares AP214. The tool finds these by name pattern
(`CC_DESIGN_*`, `DESIGN_CONTEXT`, `*TESSELLATED*`); it holds no EXPRESS schema, so it cannot say
which entity types of a component are outside the declared schema. Whether an importer accepts
the mix is for the MCAD import test to show.

`wurth/L_Wurth_WE-PMI-0805_H1.step` declares AP214 (`AUTOMOTIVE_DESIGN {1 0 10303 214 3 1 1}`),
not AP242. `inhouse-wurth242.step` was therefore made from `wurth/L_Wurth_WE-HCIM-100605.step`,
which does declare AP242.

## Self-check

`check` re-tokenizes a file and reports:

- every `#n` reference resolves, no id is defined twice, every record parses;
- the PCB shell: every edge is used once in each direction, every loop is connected, and
  `V - E + F - (L - F) = -8`, the value for genus 5;
- PCB geometry: every edge's vertices lie on its line or circle, every face's vertices on its
  plane or cylinder, each planar face's outer bound runs counter-clockwise about the face normal
  and its inner bounds clockwise, each cylindrical face has the material on the correct side,
  and the top area and the volume (flux through the horizontal faces) match the closed-form
  values 1444.9628 mm² and 2311.9405 mm³;
- every `CLOSED_SHELL` in the file, the component's included, passes the edge-use test;
- `trial-board` has exactly the children `PCB`, `U1`, `U2`, and `U1` and `U2` share a definition.

All six outputs pass. Eight edited copies of the board file (a flipped oriented edge, a deleted
record, a duplicated id, a flipped hole wall, all face senses flipped, a moved vertex, a changed
radius, a face removed from the shell) each fail it.

The check proves the text is well formed and the PCB is a closed, outward-oriented solid of the
right size. It does not prove that an MCAD importer accepts the files; no CAD tool was run.

## What this route cannot do

- Read geometry: no tessellation, no bounding box, no volume of a component. The component is
  opaque text; the tool knows its root product, its representation and its record count.
- Convert between protocols or validate against a schema.
- Convert units. A component in inches keeps its own units context and the importer has to
  scale it; the tool only notes an `'INCH'` unit when it sees one. None of the trial files has one.
- Components with more than one root product that has a shape, or with none: `write` fails with
  a message naming the roots.
- A root representation that is a complex instance: `write` fails with a message.
- A root whose shape hangs off something other than
  `SHAPE_DEFINITION_REPRESENTATION > PRODUCT_DEFINITION_SHAPE > PRODUCT_DEFINITION`.
- Deduplication: instancing the same file under two footprints would embed it twice unless the
  caller keeps one embedded copy per file, as the trial does for `U1` and `U2`.
- Rename the instance's product: `U1` and `U2` are occurrence names; the product keeps the
  component's own name.
- Part 21 edition 3 sections (`ANCHOR`, `REFERENCE`, `SIGNATURE`) are skipped, not copied.
- The board writer handles lines and arcs extruded along Z, and rotations about Z. Nothing else.

## Surprises

- ST-Developer output (Fusion, the BME680 file) wraps long strings across lines. Part 21 does
  not allow a line break in a string, so the tokenizer drops it; 12 records of that file differ
  from the source in this way and in no other.
- Apart from that, a separate Python comparison found every copied record of the four component
  files equal to the source after shifting the ids.
- The 6 MB `espressif/ESP32-S3-WROOM-1.STEP` (76,196 records) embeds in under 0.1 s.
