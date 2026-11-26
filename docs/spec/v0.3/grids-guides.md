# v0.3 Grids and guides

Source: [How do document grids and guides work?](https://github.com/openreda/reda/issues/437), after the survey in [How do other EDA tools handle grids, guides, wheel mapping, cursor keys and object defaults?](https://github.com/openreda/reda/issues/428) ([findings](https://github.com/openreda/reda/blob/research/grids-guides-input/docs/research/grids-guides-input.md)).

This file says what a PCB Document's `grids` and `guides` hold, how they are edited, how the cursor snaps to them, and what the other editors get. It extends the v0.1 [grid](../v0.1/editing.md#grid) and [cursor snapping](../v0.1/editing.md#cursor-snapping) rules, which stand unless changed here. Crates: `reda-model` and `reda-pcb` (data and Commands), `reda-ui` (snapping and editing), `reda-render` (drawing).

## The four things

| Thing | Stored | Where | What it is |
|---|---|---|---|
| **Board Grid** | PCB Document `grids` | PCB | A grid step saved with the board and offered in its grid cycle |
| **Local Grid** | PCB Document `grids` | PCB | A placed area with its own Cartesian or polar grid |
| **Guide** | PCB Document `guides`, Footprint `guides` | PCB, Footprint | A placed line or point the cursor snaps to |
| **Alignment Guide** | nowhere | every editor | A line drawn during a move when the cursor comes level with another object |

- **The grid in use stays working state.** The current step, units and Gridless stay in `.reda/view.json` ([working state](../v0.1/editing.md#working-state)). Cycling the grid never dirties a Document and is not an undo step.
- **Everything stored is edited by Commands with undo.**
- **Editor aids only.** No output, print, DXF export, STEP export or 3D view shows a grid or Guide, and no check reads one. There is no "off Local Grid" warning.

## Grid X ≠ Y

- A PCB or Footprint grid may have a second step. A preset row gains an optional **Step Y**, empty meaning the same as Step X. Visible follows the same pair.
- The custom-value dialog (`Mod`+Shift+G) takes one value or two: `0.5` or `0.5, 1.27`.
- The status bar shows one value for a square grid and `X × Y` otherwise.
- Sheets and Symbols keep a single step, because the connection grid and the `off_grid` checks assume one.

## Board Grids

A board carries its own grid steps, so a design that needs an unusual pitch takes it along in git. Other tools keep the grid list per user, and their users re-enter it on every machine.

- **Fields:** Step X, optional Step Y, Visible, **In cycle**, and at most one row marked **Default**. A value keeps the unit it was typed in.
- **Cycling:** while the board is open, G and Shift+G cycle the board's In-cycle rows together with the user's In-cycle presets, sorted by size. Equal steps show once. A Board Grid is in the cycle whichever units the board is displayed in.
- **Default:** a board opened with no view state (a fresh clone, another machine) starts on its Default Board Grid. Without one it starts on the user's default preset. After that the view state remembers the last grid used.
- **Adding:**
  - The custom-value dialog gains a **Keep on this board** tick. Ticked, the value becomes a Board Grid and cycling no longer discards it.
  - **Board Setup ▸ Grids** lists the Board Grids with add, remove, In cycle and the Default mark. The same page lists the [Local Grids](#local-grids), so every stored grid of the board is in one table.
- **PCB only.** A Footprint has no Board Grids, and a sheet's steps follow the Project's connection grid.

## Local Grids

A Local Grid is a named area of the board with its own grid, which replaces the everyday grid inside it.

- **Common fields:** name (unique on the board), shape, Applies to, Rotate Components to grid, shown, enabled, locked.
- **Cartesian:** origin, rotation, Step X, Step Y, width and height.
- **Polar:** centre, angular step, radial step, start and end angle, minimum and maximum radius. Its grid points are where the rings cross the spokes.
- There is no canvas-wide polar mode. A polar grid is always a placed area.
- **Applies to:** All, Components only, or All but Components. A Local Grid that doesn't apply to the object being moved is skipped as if it weren't there.
- **Overlap:** the Local Grid higher in the list wins. Rows are reordered by dragging. The everyday grid is always last.
- **Rotate Components to grid** (off by default): a Component placed or moved onto the grid is turned to it.
  - Polar: the Component is rotated by the angle of its position around the centre, so a ring of parts all face outward the same way.
  - Cartesian: the Component takes the grid's rotation.
  - Space and Shift+Space still rotate it by the Rotation Step from there.
- **Position:** a Local Grid marks a place on the board. Moving the [Output Origin](../v0.1/outputs.md#output-origin) shifts the everyday grid and leaves Local Grids where they are. Move Board Shape moves them with the board.
- **Status bar:** with the cursor inside a Local Grid, the grid readout shows that grid's name and step. Coordinates are still X and Y from the Output Origin, never radius and angle.
- G and Shift+G cycle only the everyday grid.

## Guides

- **Line Guide:** an infinite line through a point at any angle. While placing, horizontal, vertical, +45° and −45° are one key each (Space cycles them), and Properties takes a typed angle.
- **Point Guide:** a single point.
- **Fields:** kind, position, angle (line), optional name, shown, enabled, locked.
- A Guide sits on no Layer and shows on both sides of the board.
- **Position:** as a Local Grid, it stays on the board when the Output Origin moves and moves with Move Board Shape.
- **Footprint editor:** a Footprint stores Guides too, as drawing aids. They are never copied onto a board when the Footprint is placed and are not part of a Footprint comparison in Update from Libraries.

## Editing

Guides and Local Grids are ordinary canvas objects.

- **Tools:** Place ▸ Guide (line or point) and Place ▸ Local Grid (drag a rectangle, or for polar pick the centre and drag the radius). Both repeat until Esc.
- **Selecting:** each is its own kind in the [Selection Filter](../v0.1/editing.md#selection-filter). Under the pointer they are picked last, so they never take a click from copper or a Component.
- **Moving, deleting, Properties:** as any object. A Local Grid has handles for its size or radius range.
- **Lock:** they take the ordinary [object lock](../v0.1/pcb.md#locks).
- **Properties with nothing selected** gains a **Grids & Guides** section after Grid: a row per Local Grid and per Guide with shown and enabled switches, add and remove. Clicking a row selects the object. Local Grid rows drag to reorder. This replaces a separate manager dialog.
- **Show Grids & Guides:** one toggle hides them all. Hidden ones still snap.

### Drawing

- A Guide is a thin dashed line (a small cross for a point) in one canvas colour of its own.
- A Local Grid draws its own dots or lines inside a faint boundary, and the everyday grid is not drawn inside that area. It follows the visible-grid toggle and the [coarsening and hiding](../v0.1/app.md#canvas-rendering) rules.

## Snapping

- **Order:** Hotspot, Guide, Alignment Guide, Local Grid, grid.
- **Guides:**
  - The cursor snaps onto a line Guide within the Snap Distance. Along the line, the other axis still follows the grid, so the snap point is where the Guide crosses a grid line.
  - Where two Guides cross is a snap point, and so is a point Guide.
- **Switches:** the snap palette (`Mod`+E) and Snap Options gain **Guides**, **Local Grids** and **Alignment Guides**. Gridless turns off both the everyday grid and Local Grids.
- **Suspending:** `Mod` in flight suspends Hotspots, Guides and Alignment Guides. `Mod`+Shift suspends the grids too.
- **Commands are never snapped.** Coordinates given to a Command by an MCP agent or the CLI are used as given.

## Alignment Guides

v0.1 draws them in the polygon editor and for Footprint moves. v0.3 makes them general.

- **Where:** every Tool and drag that shows an edit cursor, in the PCB, Footprint, Schematic and Symbol editors.
- **What they align to:** in X or Y, the Hotspots of the kinds ticked in the Hotspot checklist, on objects in view. A Footprint move keeps its Courtyard-edge and Pad-centre guides to neighbours.
- **Alignment Range:** how close the cursor must come to the line before it snaps, as a Preference per editor family. It defaults to the Snap Distance on boards and Footprints (8 mil) and to the Electrical Range on sheets and Symbols.
- **Switch:** the snap palette's Alignment Guides switch replaces `pcb.place.alignment_guides`.
- They are transient: drawn while they apply and stored nowhere.

## Align to a key object

Not a grid matter, but settled in the same ticket.

- When several objects are selected, Align (left, right, top, bottom, centres) aligns to the **first-selected object**, which stays put. It is drawn with a heavier selection outline so the user can see which one it is.
- A rectangle selection has no first-selected object, and Align uses the selection's extremes as in v0.1.
- This holds in the Schematic and PCB editors. Distribute is unchanged.

## Board Templates

- A [Board Template](../v0.2.5/template-format.md) may carry Board Grids (with the Default mark), Local Grids and Guides. They are copied onto the new board with the rest of the frame, and positions follow the template's coordinates.
- Shipped templates add them only where the form factor's specification defines the grid, such as the 0.1 in header grid of an Arduino shield.
- **Save as Template** offers the board's Board Grids, Local Grids and Guides as three tick rows.

## Commands

| Command | Does |
|---|---|
| `pcb.board_grid.add / set / delete` | Board Grids, the Default mark included |
| `pcb.local_grid.add / set / delete / reorder` | Local Grids |
| `pcb.guide.add / set / delete` | Guides on a board |
| `library.footprint.guide.add / edit / remove` | Guides in a Footprint |
| `view.grids_guides.show {on}` | Show Grids & Guides |

Parameters are in [Commands and file formats](commands.md#grids-and-guides-1).

Moving, locking and deleting on the canvas use the existing move, lock and delete Commands, which take these kinds.

## File format

- PCB Document `grids`: `{board: [...], local: [...]}`. `local` is in priority order.
- PCB Document `guides`: a list.
- Footprint `guides`: a list. This is a new name beside the [reserved](../v0.1/architecture.md#reserved-names) ones.
- The exact fields are in [Commands and file formats](commands.md).

## Differences from other tools

- **The leading suite** keeps its global grid in the document and ranks local grids by number. REDA keeps the grid in use as working state, stores the board's steps as Board Grids, and ranks Local Grids by list order.
- **The leading suite's guides** are limited to 0°, 90° and ±45°. REDA's take any angle.
- **KiCad 9 and 10** keep grids per user. REDA's Board Grids travel with the board.
- **Aligning to a key object** and **Rotate Components to grid** are REDA's own.
