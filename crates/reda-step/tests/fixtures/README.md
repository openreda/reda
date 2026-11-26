# reda-step fixtures

Small, licence-clean STEP files for the reader and writer tests (docs/spec/v0.3/testing.md#step-fixtures). All are REDA's own, under the repository's MIT licence. No vendor, distributor or library model is committed here (ADR-0002); the real-model corpus is measured separately.

| File | Origin | What it covers |
|---|---|---|
| `ap214_face_colours.step` | Written for reda-step by a one-off script, in the layout OpenCascade's AP214 writer uses | AP214 header; a 10 x 20 x 5 mm block (6 planes, 12 lines, 8 vertices) with **per-face colours**, one `COLOUR_RGB` per `ADVANCED_FACE`, collected in a `MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION` |
| `ap203_cylinder.step` | Written for reda-step by a one-off script, in the layout of AP203 exporters | **AP203** header (`CONFIG_CONTROL_DESIGN`) with `MECHANICAL_CONTEXT`, `DESIGN_CONTEXT`, `PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE` and the approval, person, date and security records; a cylinder r = 2 mm, h = 5 mm: two circles, a cylindrical surface, and a `SEAM_CURVE` with two `PCURVE`s in a parametric context |
| `ap242_solid_colour.step` | Written for reda-step by a one-off script, in the layout of AP242 exporters | **AP242** header; a 4 x 3 x 2 mm block reached through `SHAPE_REPRESENTATION` and `SHAPE_REPRESENTATION_RELATIONSHIP`; a **per-solid colour** on the `MANIFOLD_SOLID_BREP` with `SURFACE_STYLE_RENDERING_WITH_PROPERTIES` and a transparency; an `ID_ATTRIBUTE` the reader drops |
| `inch_block.step` | Written for reda-step by a one-off script | **Inch units**: an `'INCH'` `CONVERSION_BASED_UNIT` of 25.4 mm, a `'DEGREE'` angle unit, a tolerance in inches; a 1 x 0.5 x 0.25 in block |
| `nested_assembly.step` | Written for reda-step by a one-off script, in the layout OpenCascade's AP214 writer uses for assemblies | A **nested assembly**: `board-assy` holds `module` (at 10, 0, 1.6 mm, turned 90° about z), a `pin` and a `cap` (flipped about x); `module` holds two `pin`s. All three pins share one definition. Per-solid colours, one `COLOUR_RGB` and one `DRAUGHTING_PRE_DEFINED_COLOUR` |
| `nested_assembly.stpz` | Written by reda-step's own writer from `nested_assembly.step` (`write_file` to a `.stpz` name) | A **`.stpz`** (gzip) file: the same records as its `.step` twin |

FreeCAD 1.1.4 (OpenCascade) opened each `.step` as a valid solid of the expected volume (1000, 62.832, 24, 2048.383 and 12 mm³) and bounding box, with the assembly placements above.

The fixtures for the mesher (every surface and curve type, and each tolerant-reading case) belong with the tessellator and are added with it.
