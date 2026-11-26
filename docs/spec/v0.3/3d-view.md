# v0.3 3D view

Source: [How should the 3D view look and behave?](https://github.com/openreda/reda/issues/433), settled by prototype. The prototype (four variants, of which this is D) is on the throwaway branch [`prototype/3d-view`](https://github.com/openreda/reda/tree/prototype/3d-view). The scene is built in `reda-3d` and drawn by `reda-render` ([modules](../modules.md)).

This file says how the [3D view](../../../GLOSSARY.md) is entered, moved around in, and what it shows. What a Body is, which models ship, the 3D Preferences, the rendered image and the mechanical checks are decided in their own tickets on the [v0.3 map](https://github.com/openreda/reda/issues/421) and are only named here.

## What it is

- The 3D view is a view mode of the PCB tab. The same canvas shows the board in 2D or in 3D; it is not a separate window or Document.
- It looks, selects and measures. Nothing is edited in it: no object is placed, moved or deleted, and the 2D Tools do not run.
- The selection is the same one 2D has. Selecting in either mode shows in the other, and Properties follows.

## Entering and leaving

- A **2D | 3D switch** sits at the canvas's top left in both modes, clear of the Active Bar and of the Layer chip at the top right.
- Two Commands switch the mode, with `3` and `2` as their default keys.
- **Entering tilts the 2D picture.** The 3D view starts looking straight down at exactly what the 2D canvas showed, same centre and same zoom, and then tilts to a three-quarter view.
- **Leaving tilts back.** The view returns to straight down, and the 2D canvas takes over looking at what 3D was looking at.
- The tilt follows the motion settings: with reduced motion the mode changes at once.

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
- **Orthographic on arrival.** Perspective is a choice in the 3D view menu.
- The face views are Top, Bottom, Front, Back, Left, Right and a three-quarter view. Bottom is the board turned over left to right, so text on the bottom reads correctly.

## What is drawn

- **Material colours:** the solder mask, the surface finish on Pads, the silkscreen, bare board at the edge and in holes. The mask and silkscreen take the colours typed into the Stackup when they name a known colour.
- The board with its outline, Board Cutouts and holes; copper on the outer Layers; silkscreen; and each Footprint's Body.
- Until a Footprint has a Body, a stand-in box is drawn from its body box and stored `height`. What a Body is, and where better models come from, is decided in [What is a Body on a Footprint, and how is it placed?](https://github.com/openreda/reda/issues/431) and [Which models does REDA ship, and how does the user bring in others?](https://github.com/openreda/reda/issues/432).
- A Layer hidden in the Layers panel is hidden in 3D too.

## 3D view menu

- In 3D a **3D view chip** takes the place the active Layer's chip has in 2D, at the canvas's top right. It opens a menu, as the Layer chip does. There is no toolbar at the bottom of the canvas.
- The menu holds, top to bottom:
  - **Show:** a switch and an opacity for each of Bodies, Silkscreen, Copper and Board, then a Solder mask switch. With the board faded, inner copper shows through it.
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

- Measure takes two points. A point snaps to a corner of what is under the pointer.
- It draws the span on the board and shows its length, with how far apart the ends are along the board's X and Y and in height under it.
- Esc ends measuring.
- More ways to measure are wanted, as long as the common case stays this simple. Which ones is open, and is listed on the map.

## Not in v0.3

- Spreading the Layers apart, and a plane that cuts the board open.
- A look that draws copper in the 2D Layer colours with the board and Bodies ghosted.
- Placing or moving anything in 3D (out of scope on the map).

## Open, decided elsewhere

- What the 2D Tool keys do while the canvas is in 3D, and the keys of the face views.
- The 3D Preferences, what a split 2D and 3D pair shares, and the frame-time budget on large boards.
