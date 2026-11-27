# reda-step fixtures

Small, licence-clean STEP files for the reader and writer tests (docs/spec/v0.3/testing.md#step-fixtures). All are REDA's own, under the repository's MIT licence. No vendor, distributor or library model is committed here (ADR-0002); the real-model corpus is measured separately.

| File | Origin | What it covers |
|---|---|---|
| `ap214_face_colours.step` | Written for reda-step by a one-off script, in the layout OpenCascade's AP214 writer uses | AP214 header; a 10 x 20 x 5 mm block (6 planes, 12 lines, 8 vertices) with **per-face colours**, one `COLOUR_RGB` per `ADVANCED_FACE`, collected in a `MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION` |
| `ap203_cylinder.step` | Written for reda-step by a one-off script, in the layout of AP203 exporters | **AP203** header (`CONFIG_CONTROL_DESIGN`) with `MECHANICAL_CONTEXT`, `DESIGN_CONTEXT`, `PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE` and the approval, person, date and security records; a cylinder r = 2 mm, h = 5 mm: two circles, a cylindrical surface, and a `SEAM_CURVE` with two `PCURVE`s in a parametric context |
| `ap242_solid_colour.step` | Written for reda-step by a one-off script, in the layout of AP242 exporters | **AP242** header; a 4 x 3 x 2 mm block reached through `SHAPE_REPRESENTATION` and `SHAPE_REPRESENTATION_RELATIONSHIP`; a **per-solid colour** on the `MANIFOLD_SOLID_BREP` with `SURFACE_STYLE_RENDERING_WITH_PROPERTIES` and a transparency; an `ID_ATTRIBUTE` the reader drops |
| `inch_block.step` | Written for reda-step by a one-off script | **Inch units**: an `'INCH'` `CONVERSION_BASED_UNIT` of 25.4 mm, a `'DEGREE'` angle unit, a tolerance in inches; a 1 x 0.5 x 0.25 in block |
| `nested_assembly.step` | Written for reda-step by a one-off script, in the layout OpenCascade's AP214 writer uses for assemblies | A **nested assembly**: `board-assy` holds `module` (at 10, 0, 1.6 mm, turned 90° about z), a `pin` and a `cap` (flipped about x); `module` holds two `pin`s. All three pins share one definition. Per-solid colours, one `COLOUR_RGB` and one `DRAUGHTING_PRE_DEFINED_COLOUR` |
| `mapped_items.step` | Written for reda-step by a one-off script, with the helpers of the one above | **Mapped items**: a 2 x 1 x 0.5 in `tile` in a shape no product names, offered by a `REPRESENTATION_MAP` and placed twice in `tile array` by `MAPPED_ITEM`s, at the origin and 5 in along x turned 90° about z; in inches. FreeCAD reads two solids, 32774.128 mm³ in all, box (0, 0, 0) to (127, 50.8, 12.7) mm |
| `nested_assembly.stpz` | Written by reda-step's own writer from `nested_assembly.step` (`write_file` to a `.stpz` name) | A **`.stpz`** (gzip) file: the same records as its `.step` twin |

FreeCAD 1.1.4 (OpenCascade) opened each `.step` as a valid solid of the expected volume (1000, 62.832, 24, 2048.383 and 12 mm³) and bounding box, with the assembly placements above.

## Mesher fixtures

One file per analytic surface and curve the mesher evaluates (`tests/mesh.rs`). All but `half_apple.step` were drawn by a FreeCAD 1.1.4 script with Part primitives and booleans and written by FreeCAD's STEP export (OpenCascade's AP214 writer, which writes no pcurves); they are REDA's own drawings under the repository's MIT licence. The volumes are FreeCAD's, which match the closed forms given.

| File | What it covers | Volume (mm³) |
|---|---|---|
| `cone_frustum.step` | A **cone** r = 3 to 1.5, h = 4: a conical face between two circles, joined by its seam line | 65.973446 |
| `cone_apex.step` | A **cone** r = 2, h = 5 run to its **apex**: the seam line ends in the apex, where the face's bound meets the axis | 20.943951 |
| `sphere.step` | A whole **sphere** r = 3: one face bounded only by a `VERTEX_LOOP` at its pole | 113.097336 |
| `torus.step` | A whole **torus** R = 5, r = 1: one face bounded by its two seam circles, each used both ways | 98.696044 (2π²Rr²) |
| `ellipse_cut.step` | A cylinder r = 2 cut by a plane tilted 30°: an **ellipse** edge between a cylinder and a plane | 62.831853 |
| `hyperbola_cut.step` | A cone r = 3, h = 6 cut by the plane x = 1, parallel to its axis, the apex kept: two **hyperbola** edges (`HYPERBOLA`, a = 2, b = 1) between the cone and the plane, as on a hex nut's chamfer | 44.529942 |
| `parabola_cut.step` | The same cone cut by a plane through (1, 0, 0) parallel to a generator, the apex cut away: a **parabola** edge (`PARABOLA`, focal distance 1/√5) between the cone and the plane | 29.991623 |
| `plate_with_holes.step` | A 20 x 10 x 1.6 plate with three holes (r = 0.75, 1, 1.25): planar faces with **inner bounds**, cylinders facing inwards | 304.292037 |
| `led_dome.step` | A cylinder r = 2.5, h = 5 under a **hemisphere**: the sphere's meridian seam runs into its pole | 130.899694 |
| `rounded_pin.step` | A cylinder r = 1, h = 3 with its top edge filleted r = 0.3: a **part of a torus** between two circles | 9.311555 |
| `hollow_block.step` | A 4 x 4 x 4 block with a spherical cavity r = 1: `BREP_WITH_VOIDS`, the void an `ORIENTED_CLOSED_SHELL` used inside out | 59.81121 (64 - 4π/3) |
| `half_apple.step` | Written for reda-step by a one-off script, in the layout of `led_dome.step`: the upper half of a **degenerate torus** (`DEGENERATE_TOROIDAL_SURFACE`, R = 1, r = 2, outer part) on a disk, its meridian seam running into the pole on the axis, with a dimple at the stem | 42.643139 |

The planes, lines and circles of the reader fixtures above, and `ap203_cylinder.step`'s `SEAM_CURVE` with pcurves, are meshed too. FreeCAD opened `half_apple.step` as a valid solid of 42.6431 mm³ and box (-3, -3, 0) to (3, 3, 2); the closed form is π times the integral of (1 + √(4 - z²))² over z from 0 to 2, less that of (1 - √(4 - z²))² from √3 to 2.

## Freeform fixtures

One file per freeform surface and curve the mesher evaluates (`tests/freeform.rs`), drawn by a FreeCAD 1.1.4 script (Part's `toNurbs`, `BSplineSurface`, `BSplineCurve`, `extrude` and `revolve`) and written by FreeCAD's STEP export; REDA's own drawings under the repository's MIT licence. Where the shape has a closed form the volume is that; FreeCAD's own volume of a rational B-spline solid is off by up to 1% (21.388 for the cylinder, 33.4957 for the sphere), so it is used only where there is no closed form.

| File | What it covers | Volume (mm³) |
|---|---|---|
| `bspline_bulge.step` | A 10 x 8 x 2 block whose top is a non-rational bicubic **B-spline surface** (a Bézier patch, its four inner control points raised 1.5), bounded by four **B-spline curves** | 190 (160 + 1.5 · 80 · (1/2)²) |
| `nurbs_cylinder.step` | A cylinder r = 1.5, h = 3 converted to NURBS: a **rational B-spline surface** closed on itself, joined by a degree-1 B-spline seam, between two **rational B-spline curves** (circles) on bilinear B-spline planes | 21.205750 (π · 1.5² · 3) |
| `nurbs_sphere.step` | A sphere r = 2 converted to NURBS: one rational B-spline face whose first and last columns of control points **collapse to poles**, bounded by its seam | 33.510322 (4/3 · π · 2³) |
| `extruded_spline.step` | A closed periodic B-spline profile extruded 3 mm: a **surface of linear extrusion** between two planes bounded by the B-spline | 56.976972 (FreeCAD) |
| `revolved_spline.step` | A vase: an open B-spline profile from the axis out and up, revolved a whole turn and capped by a plane: a **surface of revolution** whose curve meets the axis at a pole | 32.400102 (FreeCAD) |

## Tolerant-reading fixtures

One file per tolerant-reading case (docs/spec/v0.3/step.md#tolerant-reading, `tests/tolerant.rs`). Each is a well-made fixture with its bounds or shells edited by a one-off script, so what it should draw is known; all are REDA's own, under the repository's MIT licence. `banded_pin.step` and `cup_wall.step` start from FreeCAD 1.1.4 drawings (a cylinder cut by a cross hole; a B-spline profile revolved a whole turn), the others from the fixtures above.

| File | Edit | What it should draw |
|---|---|---|
| `open_box.step` | `ap214_face_colours.step` without its top, the five faces an `OPEN_SHELL` of a `SHELL_BASED_SURFACE_MODEL` | Five faces, 500 mm², **meshed open** |
| `slit_loop.step` | `ap214_face_colours.step` with an inner bound on its top that runs out along a line and back | The bound **encloses no area and is dropped**; the block closed, 1000 mm³ |
| `banded_pin.step` | A pin r = 2, h = 5 drilled across by a hole r = 0.5; its side face's seam edges taken out, leaving it bounded by its two circles and the hole's two ends, one of which the seam ran through | The **band face split along a seam** clear of the holes; closed, 59.714956 mm³ (FreeCAD, which reads it as valid) |
| `revolved_cap.step` | `revolved_spline.step` with its side face bounded by its top circle alone, its seam and pole taken out | The **open surface of revolution closed at its pole**; closed, 32.400102 mm³ |
| `cup_wall.step` | A cup wall (a B-spline profile off the axis at both ends, revolved) bounded by its top circle alone, its seam and bottom circle taken out | The face **split along a seam** down to its curve's end and left open; FreeCAD's area of the whole wall, 36.302046 mm² |
| `failing_solid.step` | `ap214_face_colours.step` plus a second block, `broken block`, 20 mm along x, whose bottom lies on a sphere of negative radius | The **failing solid skipped and reported**, the first block drawn |
| `draws_nothing.step` | `ap214_face_colours.step` with its bottom on a sphere of negative radius | **Nothing drawn**, and reported as such; the records still read |

FreeCAD does not read `revolved_cap.step` or `cup_wall.step` as valid: it does not close a face that the file leaves without its seam.
