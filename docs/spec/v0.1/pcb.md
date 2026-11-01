# v0.1 PCB editor

Source: [Which features make up the v0.1 PCB editor?](https://github.com/openreda/reda/issues/8). Crate: `reda-pcb` (copper connectivity, Ratsnest, DRC, Zone fill, Commands). The editor and Tools are in `reda-ui`.

## Board

- One PCB Document (`.redapcb`) per Project.
- **Board Outline:**
  - It is an object of its own.
  - It is drawn as a polygon or rectangle, or created from selected lines and arcs.
- **Board Cutouts** are separate objects.
- Board thickness and copper weight are Project properties. They are used only in fab notes.
- The board owns its design rules and DRC severities ([checks](checks.md#design-rules)). A Fab Preset seeds them when the board is created.
- Creating the PCB Document and editing its setup is open: [How are Projects, Documents and board setup created and edited in v0.1?](https://github.com/openreda/reda/issues/38).

## Layers

- A fixed set:
  - Top and Bottom Copper
  - Silkscreen, Solder Mask and Paste, top and bottom
  - Courtyard and Assembly, top and bottom
  - the Board Outline
  - Mechanical and Documentation layers
- Layers are stored as a generic ordered list, so v0.2 multilayer needs no format migration ([architecture](architecture.md#design-data-model)).

## Placement

- **Footprints arrive through Sync only**, in clusters outside the board, grouped per Schematic Sheet ([sync](sync.md#placement-of-new-footprints)).
- The only exception is Mechanical Parts, which are placed directly from the Library panel.
- Footprints can be placed on either side.
- **Moving and turning:**
  - Drag to move. Space rotates.
  - L flips to the other side, mirroring Pads and silk.
  - Rotate and flip also work while placing.
- **Arranging:**
  - Align and distribute.
  - A per-Component lock.
  - An **Arrange within rectangle** Command.
- **Cross-select** with the schematic ([sync](sync.md#cross-select-and-net-highlighting)).
- **Properties panel:**
  - Designator, value and Footprint are editable here as write-through Commands ([sync](sync.md#write-through-edits)).
  - Each Footprint has a movable, rotatable Designator text.
- Pads come from the Library part and can't be edited on the board.
- **Free Pads** are plated or non-plated Pads with no Footprint, optionally on a Net. They serve as mounting holes and fiducials, as Mechanical Parts can.
- **Placement Origin:** an optional object that sets the CPL origin ([outputs](outputs.md#cpl)).

## Ratsnest

- A live minimum spanning tree per Net.
- It can be shown or hidden per Net and globally.
- A connection drops out of the Ratsnest once copper makes it.
- Net highlighting works across the schematic and the PCB.

## Routing

- **Manual single Tracks:**
  - Corners are 45° or 90°. The `/` key cycles the mode.
  - The width comes from the Net Class, and can be changed mid-route.
- A route ends on its own when it reaches a Pad or Track of the same Net.
- **Clearance:**
  - A clearance violation stops or flags the route, checked live through engine queries.
  - Nothing shoves.
- **Editing Tracks:**
  - Dragging a segment keeps its neighbours attached. `Mod`+drag detaches it.
  - Delete a segment or a whole Track.
  - Select connected copper.
- A committed route runs `pcb.track.add` ([architecture](architecture.md#tools-and-dialogs)).
- **The model leaves room** for push-and-shove, differential pairs, length tuning and curved Tracks:
  - Tracks are single segments with a Line|Arc geometry. v0.1 creates only Lines.

## Vias

- Through Vias only. The model stores a layer span on every Via, so blind, buried and micro Vias can come later.
- One key places a Via mid-route and switches the active Layer.
- A Via can also be placed on its own onto a Net, for stitching.
- Sizes come from the rules and the Net Class.

## Zones

- A solid fill on a Net, on either copper Layer.
- Each Zone has:
  - a clearance and a minimum width
  - a thermal-relief or solid Pad connection
  - island removal
  - a priority for overlaps
- **Filling:**
  - Zones refill on demand, and automatically before outputs and DRC.
  - Fills go in the `.reda/` cache, never in the Document ([architecture](architecture.md#file-format)).
  - How fills are drawn, including stale fills, is in [app](app.md#canvas-rendering).
- **Keepouts**, for copper and for Vias.

## Text and graphics

- Free text and lines, arcs and rectangles on the silk, copper and mechanical Layers.
- Text on bottom Layers can optionally be mirrored.
- **Fonts:**
  - The default is the vendored Hershey Roman Simplex stroke font, drawn as capsules.
  - TTF/OTF text uses bundled Noto Sans (OFL) or user-installed fonts.
  - Characters Hershey lacks fall back, glyph by glyph, to Noto Sans outlines.
  - A text object names its font family. If that font is missing on this machine, the text draws as Noto Sans, Messages warns, and Output Jobs that plot it warn.
  - Sources: [rendering](https://github.com/openreda/reda/issues/26), [ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md).

## View

- **A dockable Layers panel:**
  - show/hide and colour per Layer
  - an active-Layer selector
  - the Inactive-Layer mode: Normal, Dimmed or Hidden
  - a view-from-bottom flip
  - visibility per object type
- Colour schemes are fixed per app theme. Colour overrides are Preferences ([app](app.md#preferences)).
- **Units:** mm by default, with a one-key mm/mil toggle.
- **Grid:**
  - Snap-grid presets: 0.1, 0.25, 0.5 and 1.0 mm, and 5, 10, 25 and 50 mil. One key cycles them.
  - Electrical snap to Pads, Track ends and Via centres.
- A user-set origin and a measure tool.

## Deferred past v0.1

Push-and-shove, differential pairs, length tuning and curved Tracks (v0.4); 4+ Layers, the Stackup editor, blind/buried/micro Vias and teardrops (v0.2); 3D view and DXF (v0.3); multi-board Projects (v0.5); autorouter; hatched Zones, Rooms, and scoped or query rules; image logos; arrays and pattern placement; printing to paper.
