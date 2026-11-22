# v0.2.5 Plated Edges

Source: [How are plated board edges marked, connected and written to fab outputs?](https://github.com/openreda/reda/issues/403), spun off from [card-edge fingers](https://github.com/openreda/reda/issues/399).

This file says how edges of the Board Outline or a Board Cutout are marked for edge plating, how the plating joins copper and affects clearance, and how it reaches fab. Glossary term: **Plated Edge**, an edge of the Board Outline or of a Board Cutout that the fab plates with copper.

## Marking

- **Plating is set per edge.** A stored vertex already holds the edge that leaves it, so the mark is stored there as `plated: {net?}`.
- There is no board-wide flag and no per-contour flag.
- **"Plate all edges" and "Unplate all"** on a contour are just shortcuts that set or clear every edge. They run `pcb.contour.edge.set_all {contour, plated: {net?} | null}`, one undoable edit, beside the per-edge `pcb.contour.edge.set` ([Commands](commands.md#board-commands)).

### Editing the contour

| Edit | What happens to the marks |
|---|---|
| Insert a vertex | It splits an edge, and both halves keep the mark. |
| Delete a vertex | It joins two edges, and the joined edge takes the mark of the edge before the deleted vertex. |
| Move a vertex | The marks are kept. |
| Replace the whole contour (`set_outline`, a rectangle, a template) | Only the marks the new vertices carry are kept. |

### UI

- While editing the Board Outline or a Board Cutout, clicking an edge selects it.
- The edge's Properties have a **Plated** checkbox and a **Net** picker.
- A Plated Edge is drawn thicker, in the top copper Layer's colour.

### Command

`pcb.contour.edge.set {contour, edge, plated: {net?} | null}`.

- `contour` is the outline's or a cutout's UUID.
- `edge` is the index of the vertex that starts the edge.

## Net and copper

Each Plated Edge carries an optional Net.

### DRC

| Check | Severity |
|---|---|
| Plated Edges that meet at a vertex carry different Nets. They are one piece of metal. | error |
| A Plated Edge has no Net (floating metal). | warning |
| No copper of the edge's Net touches it on the outer Layers. | warning |
| An Edge Connector Pad sits on a Plated Edge. The plating would short the fingers. | error |

### No generated copper

REDA adds no copper band along a Plated Edge. The user draws Zones or Tracks of the Net up to the edge.

### Connectivity

Copper of the Net that touches a Plated Edge, on any copper Layer, counts as connected through it, as a Via does. This affects the ratsnest, the unrouted-net checks and highlighting.

### Clearance

- **Copper of the edge's own Net** is exempt from `edge_clearance` at that edge. It may touch the edge but not cross it, and its Zones fill up to it.
- **Copper of other Nets** treats the Plated Edge as copper of its Net on every copper Layer, and keeps the larger of `clearance` and `edge_clearance`.
- **Unchanged:** edges that aren't plated, Castellated Pads and Edge Connector Pads behave as before.

## Outputs

These apply when any edge is plated. Nothing here is stored; it is all worked out from the edges.

- **The Profile Gerber** writes `.FileFunction,Profile,P` instead of `NP`.
- **The job file** sets `GeneralSpecs.EdgePlating: true`.
- **A separate Gerber file** holds only the Plated Edges, drawn as lines along the contour, with `.FileFunction,OtherDrawing,Plated edges`.
  - It is named `<project>-plated_edges.gbr` under both naming schemes.
  - The job file and the Output Manifest list it.
- **A Fab Notes line** names that file and the Nets, e.g. "Edge plating on the edges drawn in `<file>`; plated edges carry net GND". When the whole outline is plated, the line says "whole outline".

## Board Templates

- A template contour marks an edge with `plated: true` and stores no Net ([format](template-format.md#object-rows)).
- A board created from a template starts with Plated Edges that have no Net, so they show the warning until the user sets one.
- Save Board as Template keeps the mark and drops the Net.
- No built-in template uses Plated Edges.
