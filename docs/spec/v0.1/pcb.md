# v0.1 PCB editor

Source: [Which features make up the v0.1 PCB editor?](https://github.com/openreda/reda/issues/8). Crate: `reda-pcb` (copper connectivity, Ratsnest, DRC, Zone fill, Commands). The editor and Tools are in `reda-ui`.

## Board

- One PCB Document (`.redapcb`) per Project.
- **Board Outline:**
  - It is an object of its own.
  - It is drawn as a polygon or rectangle, or created from selected lines and arcs.
- **Board Cutouts** are separate objects.
- **Creating the board:**
  - New Project creates the PCB Document, with its rules and DRC severities seeded from the chosen Fab Preset ([app](app.md#new-project)).
  - It may start with a rectangular Board Outline from the New Project dialog. Otherwise the canvas shows a hint: "No Board Outline: draw one, or **Create rectangle…**" (`pcb.board_outline.add_rect {width, height, corner_radius?}`).
  - The PCB Document can be deleted after confirmation. **Add PCB Document** (`document.add {kind: pcb}`) is available only while the Project has none. Output Jobs that name a missing PCB are flagged as broken until they are re-pointed. A Project opened without its PCB file shows an error in Messages and offers Add PCB Document.
- The board owns its [Stackup](#stackup), edited in **Board Setup**.
- `fab_notes` is optional free text stored in the PCB Document, edited on the Stackup page of Board Setup with `pcb.fab_notes.set`. It is copied into [Fab Notes](outputs.md#fab-notes).
- The board owns its design rules and DRC severities ([checks](checks.md#design-rules)). A Fab Preset seeds them when the board is created. They are edited in **Board Setup** ([app](app.md#settings-dialogs)).

## Stackup

Sources: [Which structural format changes does v0.1 make now?](https://github.com/openreda/reda/issues/124).

- `.redapcb` holds a `stackups` table with one entry in v0.1, seeded when the board is created from the [Fab Preset](outputs.md#stackup-seeds-and-suggestions), or else 1.6 mm with 1 oz copper. It is set with `pcb.stackup.set`.
- **Stored values:**
  - `thickness`: the finished thickness you order.
  - `entries`, ordered top to bottom:
    - copper: `{layer, thickness}`
    - dielectric: `{uuid, thickness?}`
  - `finish`, `mask_color`, `silk_color`: optional text with suggestions from the job's Fab Preset, written as typed in [Fab Notes](outputs.md#fab-notes) and left out when unset.
- A 2-layer board is copper, one dielectric with no thickness, copper. Solder mask, silkscreen and paste are not entries.
- **Copper units:** thickness is stored in nm and can be entered in µm, mm, mil or oz, where 1 oz is exactly 35 µm. Fab Notes show both for a standard weight, such as "1 oz (35 µm)", and µm only otherwise.
- **Totals:**
  - A dielectric left blank takes the remainder of `thickness`. A negative remainder is an error on the field.
  - If every dielectric is typed in, `thickness` stays as stored and any mismatch shows as a warning.
- The Stackup page in Board Setup is an editable table beside a drawn cross-section, with copper, dielectric and total thickness updating as you type.

## Layers

- A fixed set:
  - Top and Bottom Copper
  - Silkscreen, Solder Mask and Paste, top and bottom
  - Courtyard and Assembly, top and bottom
  - the Board Outline
  - Mechanical and Documentation layers
- Layers are stored as a generic ordered list, so v0.2 multilayer needs no format migration ([architecture](architecture.md#design-data-model)).
- Dielectrics are Stackup entries, not Layers.

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
- **Cross-select** with the schematic, with its order kept for placing parts in the order they were picked ([sync](sync.md#cross-select-and-net-highlighting)).
- **Properties panel:**
  - Designator, value and Footprint are editable here as write-through Commands ([sync](sync.md#write-through-edits)).
  - Each Footprint has a movable, rotatable Designator text.
- Pads come from the Library part and can be [edited on the board](#placed-pad-edits).
- **Free Pads** are plated or non-plated Pads with no Footprint, optionally on a Net. They serve as mounting holes and fiducials, as Mechanical Parts can. They have the whole [Pad model](library.md#pad-model).
- **Output Origin:** an optional object that sets where a `relative` Fab Preset measures Gerber, drill and CPL coordinates from ([outputs](outputs.md#output-origin)).

### Placed Pad edits

Sources: [How do mask, paste and tenting inherit, and what can a Pad express?](https://github.com/openreda/reda/issues/126), [ADR-0013](../../adr/0013-placed-pads-edit-a-local-footprint-copy.md).

- `pcb.pad.edit {instance, pad, ...}` changes one Pad of one placed Footprint, from the Properties panel. Every Pad property can be edited except `number`.
- Pads can't be added or removed on the board, and `number` stays fixed, because Nets come from the schematic.
- **A locally edited Footprint:**
  - The edit gives the instance its own entry in the PCB's part table, keyed by the new content hash.
  - The entry records `edited_from {uuid, hash}`, naming the copy it came from. Pads keep their UUIDs.
  - Instances with identical edits share one entry.
  - Copper, DRC and outputs read this entry as they read any other copy.
- Properties marks the instance as locally edited, and the DRC report lists these instances.
- Sync keeps local edits, and a Footprint change or Update from Library removes them ([sync](sync.md#what-sync-does-to-the-board)).

## Ratsnest

- A live minimum spanning tree per Net.
- It can be shown or hidden per Net and globally.
- A connection drops out of the Ratsnest once copper makes it.
- Net highlighting works across the schematic and the PCB.

## Routing

- **Manual single Tracks:**
  - Corners are 45° or 90°. The `/` key cycles the mode.
  - The width starts at the Net's [preferred value](checks.md#which-value-applies), and can be changed mid-route. A typed width is clipped to the Net's `min` and `max`.
- A route ends on its own when it reaches a Pad or Track of the same Net.
- **Clearance:**
  - A clearance violation stops or flags the route, checked live through engine queries.
  - Tracks on a Net Tie's Nets follow the [Net Tie rule](checks.md#net-ties), the same one DRC uses.
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
- Sizes start at the Net's [preferred `via_size`](checks.md#which-value-applies). A typed size is clipped to its `min` and `max`.
- A Via has the same `mask {top?, bottom?, from_hole_edge?}` as a [Pad](library.md#pad-model), with `top` and `bottom` as board sides. An untented side opens at the Via's diameter plus its expansion.

## Mask and paste

Source: [How do mask, paste and tenting inherit, and what can a Pad express?](https://github.com/openreda/reda/issues/126).

- **Board values** are two [rule records](checks.md#design-rules), edited on the Mask & Paste page of Board Setup:
  - `mask_expansion`: `{top, bottom, from_hole_edge, tent_vias_top, tent_vias_bottom}`
  - `paste_expansion`: `{expansion}` or `{percent}`
- **Inheritance:**
  - A Pad's or Via's own value wins, side by side.
  - Otherwise the rule applies, [resolved through the Net's classes](checks.md#which-value-applies). A Pad on no Net uses the board-wide record.
  - Tenting from the rule applies to Vias only. A Pad is tented by its own `mask`.
  - `from_hole_edge` from the rule applies only to Pads and Vias with holes.
- There is no Footprint-level default. A Footprint sets values on its Pads, and `footprint` is a reserved scope filter.
- **Resolved openings:** one function in `reda-pcb` works out each opening. Gerbers, silkscreen clearing and DRC all use it.
  - **Mask:** one opening per side for each Pad and untented Via. A non-plated Pad's opening comes from its shape. A tented side has none.
  - **Paste:** SMD Pads only, from the expansion, percent or windows. `none` has no opening.
  - An opening with nothing left after a negative value is not plotted, and DRC reports it.

## Zones

- A solid fill on a Net, on either copper Layer.
- Each Zone has:
  - a clearance and a minimum width
  - a thermal-relief or solid Pad connection, which a Pad's `zone_connection` overrides
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

Push-and-shove, differential pairs, length tuning and curved Tracks (v0.4); 4+ Layers, the full Stackup editor tab, blind/buried/micro Vias and teardrops (v0.2); 3D view and DXF (v0.3); multi-board Projects (v0.5); autorouter; hatched Zones, Rooms, and scoped or query rules; image logos; arrays and pattern placement; printing to paper.
