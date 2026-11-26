# v0.3 Bodies on Footprints

Source: [What is a Body on a Footprint, and how is it placed?](https://github.com/openreda/reda/issues/431), after [How do other EDA tools store and attach 3D bodies and models on footprints?](https://github.com/openreda/reda/issues/424) ([findings](https://github.com/openreda/reda/blob/research/footprint-3d-bodies/docs/research/footprint-3d-bodies.md)) and [Where do 3D models for electronic parts come from, and under what licences?](https://github.com/openreda/reda/issues/426) ([findings](https://github.com/openreda/reda/blob/research/3d-model-sources/docs/research/3d-model-sources.md)).

This file says what the reserved Footprint `bodies` field holds, how a Body is placed and stored, what the Footprint Generators make, and what is drawn when a Footprint has no Body. Which Models REDA ships and how the user brings others in is in [Models](models.md). What a STEP export contains, including whether it holds stand-in Bodies, is in [STEP export](step-export.md). Commands and file-format details are in [Commands and file formats](commands.md).

## Bodies

- A Footprint's `bodies` is an ordered list of any length, including none. Every Body is drawn and exported. There are no alternatives to pick from per placed Component.
- There are two kinds:
  - **Model Body:** one embedded Model, placed on the Footprint.
  - **Extruded Body:** a closed outline (Polygon, or Circle for a cylinder) raised from a bottom Z to a top Z, in a Material.
- Altium's sphere and other primitive kinds are not taken.
- An Extruded Body stores its own outline. It does not follow the Courtyard. The Footprint editor's **From Courtyard** action copies the Courtyard's outline into a new Extruded Body once, with top Z defaulting to `height`.

## Placement

- **Model Body:** X, Y and Z offset in mm, and rotation about X, Y and Z. Z is measured from the board surface on the Footprint's side.
- **No scale.** A STEP assembly placement is a rigid transform, so a scaled Model could not be exported faithfully. A KiCad footprint's model scale other than 1 is applied once when it is imported, or else reported.
- **Extruded Body:** in Footprint coordinates. Bottom Z is the standoff and top Z the height, both from the board surface.
- A Footprint placed on the bottom side mirrors its Bodies with it.

### In the Footprint editor

- The 2D | 3D switch of the [3D view](3d-view.md) works in the Footprint editor too, with the same navigation.
- A selected Body is edited by numbers in Properties, and the 3D view updates live.
- Two aids for a Model Body:
  - **Seat on board** sets Z so the Model's lowest point sits at the standoff (default 0).
  - **Centre on pads** centres the Model's bounding box in the board plane on the Pads' bounding box.
- Two picks for a Model Body, decided in [Does placing a Model by picking geometry land in v0.3, and which picks?](https://github.com/openreda/reda/issues/467). They are described under [Placing a Model by picking](#placing-a-model-by-picking).

### Placing a Model by picking

- **Face on board:** pick a planar face of the Model, or a circular edge.
  - The Model turns by the smallest rotation that points the face's outward normal straight down. For a circular edge, the normal of the edge's plane is taken on the side away from the Model's centre.
  - Z is then set so the picked face sits at the standoff (default 0), as with Seat on board.
  - The Model's bounding-box centre keeps its X and Y. A quarter turn about Z after that is typed.
- **Point to point:** pick a Hotspot on the Model, then a target on the Footprint. The Model moves in X and Y so the two meet. Z and the rotations do not change, so a Model is never lifted off the board.
  - Hotspots on the Model: a vertex, an edge midpoint, a circle or arc centre, a planar face centre.
  - Targets: a Pad centre (also its hole centre), the Footprint origin, a Hotspot of the Footprint's graphics such as a Courtyard or Assembly corner. They are shown in the 3D view.
  - Picking a lead and then its Pad gives a pin-to-pad placement.
- **Where:** only in the Footprint editor's 3D view. A PCB's 3D view has no picks, because Bodies belong to the Footprint.
  - Both are buttons in a selected Model Body's Properties, beside Seat on board and Centre on pads. Starting one from 2D switches the canvas to 3D.
  - During a pick, the pointer snaps and highlights what it will take. Esc cancels.
  - This is the one exception to the [3D view](3d-view.md#what-it-is)'s rule that nothing is edited in it.
- **Geometry:** Hotspots come from the Model's exact geometry as `reda-step` reads it: edges, circles and planar faces.
  - A Model the reader could only mesh offers vertex Hotspots only. Face on board then takes the plane of the flat triangles under the pointer.
  - Extruded Bodies and Stand-in Bodies have no picks. They are placed by their own numbers.
- **Nothing about a pick is stored.** There are no stored snap points on a Body, as in Altium Designer. Each pick ends as one Command and one undo step:
  - `library.footprint.body.face_on_board {body, point, normal}`, with a point on the face and its outward normal in Model coordinates;
  - `library.footprint.body.move_point_to {body, from, to}`, with `from` in Model coordinates and `to` an X/Y point in Footprint coordinates.
  - Both return the resulting placement. An MCP client that knows the coordinates calls them directly. They are listed with the other [Body Commands](commands.md).
- **One snapping engine.** The same Hotspots serve [Measure](3d-view.md#measuring) and Height above board in every 3D view. Point-to-plane and point-to-face measuring stay out of v0.3.

## `height`

- `height` stays a stored value, and it stays authoritative: height checks, the stand-in Body and Board Templates read it, not the Bodies.
- When the top of the Bodies differs from `height`, the Footprint editor shows a hint with **Set height from Bodies**.

## Materials

- An Extruded Body has a Material from a short list (black plastic, grey plastic, tin, gold, copper, ceramic, glass) or a custom colour.
- A Model Body draws the Model's own colours. An optional Material overrides them all, for Models that carry no colour.

## Models on disk

- A Model is a STEP file embedded in a Library or Project. Only embedding exists: no path links and no path variables, so a Model cannot go missing.
- **Library:** `models/<hash>.stpz` beside `footprints/`, gzip-compressed STEP. `<hash>` is taken over the uncompressed STEP text, so Footprints using the same file share one Model.
- **Project:** the PCB Document's Footprint copies keep their Models in a Project-level `models/` folder, in the same form. Placing a Footprint copies its Models there.
- `.stpz` is the standard compressed STEP form, opens in any CAD tool, and needs no new dependency (deflate is already in the workspace). One file per Model keeps diffs small and matches the one-file-per-element Library.
- A Model no Body refers to any more is deleted on save. Undo works on the element JSON, so it cannot leave a dangling Body.
- Library Differs includes each Model's hash.
- The Starter Library ships no third-party Models ([ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md)). A user Library may embed any Model the user obtained.

## No Body: the stand-in

- A Footprint with no Body, or whose Models draw nothing, is shown with a **Stand-in Body**: the Footprint's [body box](../v0.1/library.md#body-box) raised to `height`, drawn translucent and muted so it reads as a stand-in.
- It is generated, never stored.
- A Footprint with no `height` gets a stand-in 1 mm high.

## Generated Bodies

- Every Footprint Generator emits Extruded Bodies from its package dimensions: the case plus a lead per pin, all extruded along Z only.
  - Gull-wing leads (SOIC, SSOP, TSSOP, QFP, SOT) are a foot box on the Pad plus a shoulder box up to the case.
  - Chip parts get a ceramic or black-plastic body with tin end caps.
  - QFN and DFN get flush terminals and the exposed pad.
  - Through-hole leads (DIP, header) go through the board to a fixed 3 mm below it.
  - Cases are black plastic and leads tin, unless the family says otherwise.
- Every generator gains a **standoff** parameter (the spec's "least standoff"), with a family default: 0 for chip, molded and QFN/DFN, about 0.1 mm for gull-wing and QFP.
- DIP and header gain body dimensions, defaulting to JEDEC MS-001 and the usual header sizes.
- Regenerating replaces the Bodies with everything else, because a generator owns its whole Footprint.
- The Starter Library's hand-drawn Footprints each get one Extruded Body made with From Courtyard at their `height`, written once into the Library. So every Starter Library Footprint has a Body and none is a Model.

## Library checks

| Check | When | Default |
|---|---|---|
| `model_unreadable` | an embedded Model draws nothing; the stand-in is shown | W |
| `body_height_mismatch` | the top of the Bodies differs from `height` | W |

There is no check for a Footprint with no Body, because the stand-in covers it, and none for a missing Model file, because Models are embedded.

## Out

- Scale on a Model Body.
- Linked Models and path variables.
- Mesh formats (see [STEP](step.md)).
- REDA-authored Models for common packages in the Starter Library: later work, after v0.3. ADR-0002 allows them, because they would be REDA's own MIT content.
