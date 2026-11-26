# v0.3 3D view

Sources: [How should the 3D view look and behave?](https://github.com/openreda/reda/issues/433), settled by prototype, and [What do the 3D Preferences, a side-by-side view, the 3D keys and further measuring look like?](https://github.com/openreda/reda/issues/466). The prototype (four variants, of which this is D) is on the throwaway branch [`prototype/3d-view`](https://github.com/openreda/reda/tree/prototype/3d-view). The scene is built in `reda-3d` and drawn by `reda-render` ([modules](../modules.md)).

This file says how the [3D view](../../../GLOSSARY.md) is entered, moved around in, and what it shows. What a Body is, which models ship, the rendered image and the mechanical checks are decided in their own tickets on the [v0.3 map](https://github.com/openreda/reda/issues/421) and are only named here.

## What it is

- The 3D view is a view mode of the PCB tab. The same canvas shows the board in 2D or in 3D; it is not a separate window or Document.
- It looks, selects and measures. Nothing is edited in it: no object is placed, moved or deleted, and the 2D Tools do not run.
  - The one exception is in the Footprint editor, whose canvas has the same 2D | 3D switch: a Model Body can be placed there by [picking geometry](bodies.md#placing-a-model-by-picking). A PCB's 3D view has no picks.
- The selection is the same one 2D has. Selecting in either mode shows in the other, and Properties follows.

## Entering and leaving

- A **2D | 3D switch** sits at the canvas's top left in both modes, clear of the Active Bar and of the Layer chip at the top right.
- Two Commands switch the mode, with `3` and `2` as their default keys.
- **Entering tilts the 2D picture.** The 3D view starts looking straight down at exactly what the 2D canvas showed, same centre and same zoom, and then tilts to a three-quarter view.
- **Leaving tilts back.** The view returns to straight down, and the 2D canvas takes over looking at what 3D was looking at.
- The tilt follows the canvas animation Preference under Editing: with it off, or with reduced motion, the mode changes at once.

## Moving around

| To | Do |
|---|---|
| Zoom | The wheel, about the point under the pointer, as in 2D. |
| Pan | Drag with the right or the middle button, as in 2D. |
| Turn | Drag with the left button; or hold Shift while dragging to pan; or drag the [orientation gizmo](#orientation-gizmo). |
| Look at a face | Click an end of the orientation gizmo, pick a view in the [3D view menu](#3d-view-menu), or press its key. |
| Fit the board | Fit in the 3D view menu, or its key. |

- The view turns like a turntable about the board's normal, and tilts from straight above to straight below. It does not roll.
- It turns about the centre of the view.
- **Orthographic on arrival,** unless the [Projection on arrival](#preferences) Preference says Perspective. The 3D view menu switches it for this view.
- The face views are Top, Bottom, Front, Back, Left, Right and a three-quarter view. Bottom is the board turned over left to right, so text on the bottom reads correctly.

## What is drawn

- **Material colours:** the solder mask, the surface finish on Pads, the silkscreen, bare board at the edge and in holes. The mask and silkscreen take the colours typed into the Stackup when they name a known colour, and the [default colours](#preferences) otherwise.
- The board with its outline, Board Cutouts and holes; copper on the outer Layers; silkscreen; and each Footprint's Body.
- Until a Footprint has a Body, a stand-in box is drawn from its body box and stored `height`. What a Body is, and where better models come from, is in [Bodies](bodies.md) and [Models](models.md).
- A Layer hidden in the Layers panel is hidden in 3D too.

## 3D view menu

- In 3D a **3D view chip** takes the place the active Layer's chip has in 2D, at the canvas's top right. It opens a menu, as the Layer chip does. There is no toolbar at the bottom of the canvas.
- The menu holds, top to bottom:
  - **Show:** a switch and an opacity for each of Bodies, Silkscreen, Copper and Board, then a Solder mask switch and a [Height Areas](component-height.md#in-3d) switch, off by default. With the board faded, inner copper shows through it.
  - **Projection:** Orthographic or Perspective.
  - **View:** Fit and the face views, each with its key.
  - **Measure,** and **Back to 2D.**
- The menu stays open while its switches and sliders are used.

## Orientation gizmo

- Under the 3D view chip: the board's X, Y and Z axes drawn as the view sees them, in the manner of 3D tools and engines, with a labelled end for each direction.
- A click on an end looks along that axis: +Z is Top, −Z Bottom, −Y Front, +Y Back, −X Left, +X Right.
- A drag on the gizmo turns the board.

## Selecting

- A click selects the object under the pointer: a Footprint by its Body or its Pads, a Track, a Via, a Zone, text. A click on nothing clears the selection.
- A selected object takes the selection colour, as in 2D.
- There is no selection box in 3D in v0.3; the left drag turns the board.

## Measuring

- **Measure** (`view.3d.measure`, `Mod`+M) takes two points. A point snaps to a Hotspot of what is under the pointer: a vertex, an edge midpoint, a circle or arc centre, or a planar face centre. These are the Hotspots that [placing a Model by picking](bodies.md#placing-a-model-by-picking) uses, from one snapping engine; a Model the reader could only mesh offers its vertices only.
- It draws the span on the board and shows its length, with how far apart the ends are along the board's X and Y and in height under it.
- **Height above board:** while measuring, the readout by the pointer also gives the snapped point's height above the nearest board surface, before the first click and between clicks. There is no separate mode for it.
- **Measure Clearance** (`view.3d.measure_clearance`) picks two objects, each a Body (Stand-in Bodies included) or the board, and shows the shortest distance between their meshes, drawn as a span with its length and its X, Y and Z parts.
- Both are in the 3D view menu's Measure entry. Esc ends either.
- Point to plane and point to face are not in v0.3.

## Keys

The Keymap gains a **3D view** context. It sits where the active Tool's context sits, above the pcb Document type, because no Tool runs in 3D.

- **Face views,** Shift and the digit row, opposite faces side by side:

  | Key | View |
  |---|---|
  | Shift+`1` / Shift+`2` | Top / Bottom |
  | Shift+`3` / Shift+`4` | Front / Back |
  | Shift+`5` / Shift+`6` | Left / Right |
  | Shift+`7` | three-quarter |
  | `*` | turn the board over, as `*` swaps top and bottom in 2D |

  Plain `2` and `3` stay `view.2d` and `view.3d`.
- **Tool keys** (T, V, Z, K and the rest) tilt back to 2D and start that Tool.
- **Editing keys** (Del, M, D, Space, X and Y, `Mod`+arrows) do nothing, and the status bar says "Editing is in 2D: press 2".
- **View keys** keep working where they mean something in 3D: F fits, Shift+Home zooms to the selection, `Mod`+M measures, Ctrl+Q switches units, Shift+C clears the Highlight, Alt+← goes to the previous view, and the Layer visibility keys hide and show.
- **The digit row is matched by physical key.** v0.1 binds keys by the character they produce, which makes Shift+`1` mean `!` on a US layout, Shift+`2` mean `"` on a Danish one, and a different key again on AZERTY, where the top row needs Shift for digits. A binding on a digit, with or without modifiers, therefore fires on the top-row key in the US digit's position, whatever it produces, and on the numpad digit. Every other key is still bound by logical key. The Keyboard Shortcuts page writes it as "Shift+1". This amends the [v0.1 Keymap](../v0.1/app.md#keymap) and holds in 2D too.

## Side by side

- **New view of this Document,** in the tab menu and beside *Split right* and *Split down*, opens a second [Document view](../../../GLOSSARY.md): another tab of the same Document. A Document can have any number.
- **Shared** by every view of a Document: the Document and its undo stack, the selection, the Highlight, Layer visibility, the active Layer and the 3D view's Show set. A Layer hidden in one view is hidden in all.
- **Each view's own:** the mode (2D or 3D), camera, zoom, projection and the grid [working state](../v0.1/editing.md#working-state).
- Cameras are never linked in v0.3. Shift+Home in each view lines them up on the selection.
- An edit in any 2D view shows in every 3D view as it commits.
- `.reda/view.json` stores the open views, and each view's own state under the view.

## Preferences

A **3D view** page under the PCB pages, keyed `pcb.view3d.*`:

| Setting | Key | Control (default) |
|---|---|---|
| Projection on arrival | `pcb.view3d.projection` | Segmented: Orthographic · Perspective (Orthographic) |
| Solder mask | `pcb.view3d.colour.mask` | Colour (green) |
| Silkscreen | `pcb.view3d.colour.silk` | Colour (white) |
| Surface finish | `pcb.view3d.colour.finish` | Colour (gold) |
| Board core | `pcb.view3d.colour.core` | Colour (FR-4 tan) |
| Background | `pcb.view3d.background` | Segmented: Theme · Gradient (Theme) |
| Turn speed | `pcb.view3d.turn_speed` | Slider |

- The colours apply where the Stackup doesn't name a known colour.
- There is no navigation-style choice: the wheel follows the shared [wheel mapping](input-and-defaults.md), and the drag buttons are fixed.
- The tilt has no switch of its own; it follows the canvas animation Preference.

## Frame time

Targets, reported by the `#[ignore]` timing tests in [testing](testing.md#performance) on the Mesa CI runner and never gating:

- 60 frames a second while turning the `mechanical_3d` board, and 30 on the [32-Layer board](../v0.2/testing.md#the-32-layer-limit), on integrated graphics.
- The 32-Layer board's scene builds in under 2 s, off the UI thread. The last scene stays on screen while a build runs.
- An edit rebuilds only the objects it changed.

## Not in v0.3

- Spreading the Layers apart, and a plane that cuts the board open.
- A look that draws copper in the 2D Layer colours with the board and Bodies ghosted.
- Placing or moving anything in 3D (out of scope on the map), apart from the Footprint editor's two Model picks.
- Linked cameras between views, and a navigation-style choice.
