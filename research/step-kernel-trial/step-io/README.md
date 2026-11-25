# step-io trial (issue #442)

Candidate: [`step-io`](https://github.com/elgar328/step-io) 0.3.1 from crates.io
(checksum `22cc66e0…eb4dae` in `Cargo.lock`), MIT OR Apache-2.0. Source read at commit
`4865dad`. Runtime dependencies: `logos` 0.16.1 and `memchr` 2.8.3. Built with Rust 1.96.1.

step-io is a file-format layer: Part 21 parser, a typed entity model, a navigation layer
(`scene`), and a writer (`StepBuilder`). It does not evaluate geometry and does not tessellate.

## Commands

    step-io-trial read <file.step>
    step-io-trial write <out.step> <component.step> [--no-name-patch]
    step-io-trial rewrite <out.step> <in.step>      # not in SPEC.md, see below

## What was run

Read: SOIC-8, R_0603, Wurth WE-PMI-0805, AP203 `470531000.stp`, Adafruit BME680. All parse
with 0 entities dropped, in 1 to 17 ms. Write: SOIC-8, the AP203 file, BME680 and the Wurth
part as the component. No panics.

The written files were also read by two other candidates' harnesses (foxtrot, truck-master),
which tessellate:

- Board alone: 22 faces meshed, closed, volume 2311.95 / 2312.19 mm³ (analytic 2311.94),
  area 3231.25 / 3231.41 mm² (analytic 3231.28).
- SOIC-8 after the round trip: truck gives the same volume and area as for the source file
  (30.176 mm³, 77.869 mm²), closed.
- AP203 part after the round trip: foxtrot gives 182.16 mm³ for the source and the same per
  instance for the output, closed.
- BME680: foxtrot gives board + 2 x 771.794 mm³ for the output to five digits, bounding box
  consistent with both placements.

## Read

Provided: solids, shells, faces, surface and curve kind of every face and edge, colours
(solid, shell, face), the product tree with placements, units, the source schema, and a
report of what was dropped or normalized.

Not provided: triangles, bounding box, area, volume. Every mesh field is null. Display
meshes are exposed only when the file itself stores tessellated geometry.

`SurfaceKind` and `CurveKind` are `#[non_exhaustive]`. A rational B-spline surface or curve
arrives as `Other("COMPLEX")`, not as a kind of its own; `to_nurbs()` handles it.

## Write

The board is built vertex by vertex: 2D loops swept into planes and cylinders, top and bottom
planes with the holes as inner bounds. `StepBuilder` covers this directly.

Output schema is always AP242 edition 2
(`AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 3 1 4 }`).

### Re-emitting a read model inside our assembly

step-io has no call for this. A read `StepModel` is read-only, and the writer accepts new
entities only through its constructors, so there is no copy or merge between models.
`transcribe.rs` (about 550 lines) walks the scene handles and rebuilds the component through
`StepBuilder`: vertex, edge, face, solid, colour, part, placement.

What that gives:

| Component | In | Out (component alone) | Colours | Notes |
| --- | --- | --- | --- | --- |
| SOIC-8 (AP214) | 2984 entities, 1 solid, 147 faces | 5667 entities, 1 solid, 147 faces | 3 in, 3 out | 16 surfaces and 24 edges through NURBS |
| `470531000.stp` (AP203) | 2910 entities, 1 solid, 101 faces | 2797 entities, 1 solid, 101 faces | 0 in, 0 out | cone angle converted from degrees |
| BME680 (AP214, nested) | 19425 entities, 22 solids, 610 faces, 11 products | 22169 entities, 22 solids, 610 faces, 11 products | 17 in, 17 out | 15 nested placements kept, 4 surfaces through NURBS |
| Wurth WE-PMI (AP214) | 855 entities, 1 solid, 26 faces, 2 products | 867 entities | 2 in, 2 out | |

U1 and U2 share one definition (one product, two `NEXT_ASSEMBLY_USAGE_OCCURRENCE`).

What it loses or cannot do:

- Everything the walk does not visit: PMI, layers, product metadata, names of faces and
  edges, the source header. Only solids, their colours and transparency, product names and
  placements cross.
- Entity sharing. The builder makes a new surface, placement and style chain per face and
  per colour, so the SOIC-8 nearly doubles in entity count.
- B-spline flags. A B-spline goes through `NurbsCurve` / `NurbsSurface`, which carry degree,
  control points, weights and knots only. `closed`, `self_intersect` come out as `.U.` and
  `knot_spec` as `.UNSPECIFIED.`. The foxtrot reader rejects those records (26 faces of the
  SOIC-8 output, 4 of the BME680 output); truck reads them. With the `.U.` flags edited to
  `.F.` in the text, foxtrot meshes all 169 faces of the SOIC-8 output, closed, at
  board + 2 x 30.186 mm³.
- Lines are rewritten from their two vertices, not copied.
- Vertex loops (a cone apex): `StepBuilder::face` takes edge loops only. Counted and dropped.
- Surface models (open shells): `StepBuilder` has no call for them, and a part's item list
  is private, so the low-level author cannot attach one to a part either. Counted and dropped.
- `MAPPED_ITEM` instancing is not followed by the transcription.
- A solid whose outer shell is used reversed is written as it is.
- An extrusion or revolution over a profile other than a line, circle or ellipse falls back
  to a NURBS patch: the scene's `Curve` handle cannot be made from the `CurveRef` such a
  surface holds.

None of the last five occurred in the four test components.

### Instance names

`StepBuilder::place` writes each occurrence with its running number as id and an empty name,
and a finished model cannot be edited. The harness sets `U1` and `U2` by replacing the two
records in the written text. `--no-name-patch` leaves the file as step-io wrote it.

### `rewrite`

`step_io::write` on a model that was read (no builder) writes the same entities back under
the source's own `FILE_SCHEMA`: the AP203 file comes back as `CONFIG_CONTROL_DESIGN`, 2910
entities in and out; the SOIC-8 as AP214, 2984 in and out, colours kept. This is a lossless
pass-through, but nothing can be added to such a model.

## API friction

- No model-to-model copy; see above.
- Reading analytic geometry out of the model means following `…Ref` enums by hand
  (`Plane` → `Axis2Placement3d` → `CartesianPoint` / `Direction`); the scene layer stops at
  the entity.
- Units are not applied by the reader or reconciled by the writer: the transcription scales
  lengths to millimetres and angles to radians itself.
- `Orientation::flag` / `from_flag` are crate-private.
- The crate describes itself as experimental with breaking changes at any time.

## Cross-target

`cargo check --release --target x86_64-pc-windows-msvc` and `--target aarch64-apple-darwin`
both pass.
