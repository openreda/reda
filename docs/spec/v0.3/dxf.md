# v0.3 DXF import and export

Source: [How do DXF import and export work?](https://github.com/openreda/reda/issues/436), after the survey in [How do other EDA tools import and export DXF?](https://github.com/openreda/reda/issues/427) ([findings](https://github.com/openreda/reda/blob/research/dxf-import-export/docs/research/dxf-import-export.md)).

This file says what DXF import reads and where it lands, how a [Board Outline](../../../GLOSSARY.md) is built from a drawing, what the import dialog shows, what DXF export writes, and the Commands for both. Mechanical Layers and their Purpose are in [mechanical-layers](mechanical-layers.md). Crates: `reda-import-dxf` (reader, import) and `reda-output` (writer).

## Own reader and writer

- Both are written in-house, with no DXF crate. The reader is a tag-pair parser plus about ten entity types, and the writer is smaller.
- **Reference to read:** [cadcraft](https://github.com/storytold/cadcraft) and its `craft-dxf` crate in [craft-libs](https://github.com/storytold/craft-libs) (`MIT OR Apache-2.0`, written from Autodesk's DXF Reference). It is not a dependency.
  - These small pieces may be ported, with its copyright notice kept: the tag parser, the object coordinate system transform, bulge to arc, the ellipse handling for a flipped extrusion, the MTEXT format-code tokenizer, spline evaluation, and the minimal AC1015 file skeleton with handles.
  - The project was seven weeks old when read (2026-11-25) and is largely agent-written. Every ported piece is checked against the DXF Reference and REDA's own fixture files.
  - It has no spline-to-arc fitting. That is REDA's to write.

## Import

Import goes into the PCB Document only.

### Files and entities

- **Files:** ASCII and binary DXF of any version. DWG is refused with a clear message.
- **Text encoding:** UTF-8 and Windows-1252 are decoded. Any other `$DWGCODEPAGE` imports with a warning that non-ASCII text may be wrong.
- **Read:** LINE, ARC, CIRCLE, ELLIPSE, LWPOLYLINE and POLYLINE with bulges, SPLINE, TEXT, MTEXT, and INSERT with its block exploded in place.
- **Skipped:** HATCH, DIMENSION, SOLID, TRACE, 3DFACE, IMAGE, POINT and every other entity. The report counts them per type.
- Nothing becomes a Pad, Track, Zone or Component.

### Units, scale and position

- **Unit:** read from `$INSUNITS`. When it is missing or unitless, mm is assumed. The user can always change it.
- **Scale:** a factor, 1 by default.
- **Size check:** the dialog shows the drawing's resulting width and height in mm, so a wrong unit is noticed.
- **Position:** the drawing lands at its file coordinates, with the DXF origin on the [DXF origin point](#origin) and the Y direction converted to the document's. Repeated imports from the same MCAD file line up.
- **Place with cursor** drops it by hand instead.
- The imported objects are selected afterwards. The whole import is one undo step.

### Conversion

- Splines and ellipses are fitted with arcs, within 0.005 mm of the true curve. A circular ellipse becomes a true arc.
- A bulged polyline segment becomes an exact arc.
- A polyline on a graphics Layer becomes separate lines and arcs. The model gains nothing for DXF.
- **Line width:** one value for the whole import, pre-filled from the default for graphic lines. DXF lineweights, colours and line types are ignored.
- **Text:** TEXT and MTEXT become free text in the default stroke font, with the DXF height, rotation and justification. MTEXT format codes are stripped and line breaks kept. The DXF font is ignored.

### Layer mapping

The mapping has one row per DXF layer that holds readable entities, with its entity count and a target.

- **Targets:** the Board Outline, any Layer that takes free graphics (Mechanical, silk, copper, Courtyard and Assembly), "New Mechanical Layer" (named after the DXF layer, Purpose Reference), or "Skip".
- **Pre-fill**, by the first rule that matches:
  1. The DXF layer has the same name as a board Layer: that Layer. A REDA export maps straight back.
  2. The name contains "outline", "edge", "board", "profile" or "contour", in any case: the Board Outline, for at most one row.
  3. The name suggests a Purpose, such as "dim" for Dimensions: the first Mechanical Layer with that Purpose.
  4. Anything else: the first Mechanical Layer with the Purpose Reference, added if the board has none.
- A drawing with many layers therefore lands on one Reference Layer unless the user asks for more.

### Board Outline from the drawing

REDA builds the outline during the import, where other tools place graphics and convert them in a second step. The Board Outline is an object here, so graphics that only look like a board would mislead.

- The row mapped to the Board Outline goes through the [loop builder](../v0.1/pcb.md#closed-loops-from-graphics).
- **The loop with the largest area** becomes the Board Outline.
- **Closed loops wholly inside it** become Board Cutouts. That includes circles: a DXF can't say whether a circle is a drilled hole.
- **Loops outside it, or nested inside a cutout,** land as graphics on the Reference Layer, with a Messages entry.
- **Text** on that DXF layer is skipped and counted.
- **An existing Board Outline** and its Board Cutouts are replaced. The dialog says so beside the row, and the preview draws the old outline faintly behind the new one. Other objects don't move.
- **Open or self-crossing shapes:**
  - The preview marks each gap or crossing and gives its size, before anything is imported.
  - **Close gaps up to** sets the tolerance. Its default is 0.001 mm, the loop builder's own.
  - If the user imports anyway, those entities land as graphics on the Reference Layer and Messages says why. They can be repaired there and converted with `pcb.board_outline.add_from_graphics`.
  - The import never builds a guessed outline, and never fails as a whole because of the outline.
- The two-step path stays: map the row to a Mechanical Layer and convert from the selection.

### Import dialog

One modal dialog, shown after the file is picked.

- **Left:** a preview of the drawing, coloured by target, with outline problems marked.
- **Right, top:** unit, scale, the resulting size in mm, line width, and Close gaps up to.
- **Right, middle:** the mapping table.
- **Right, bottom:** Place with cursor, and a one-line count of skipped entities that opens the full list.
- **Buttons:** Import and Cancel.
- It remembers only the last folder.

## Export

### What is written

A tick list. Each ticked entry becomes one DXF layer named after it.

- The Board Outline with its Board Cutouts. Ticked by default.
- Drilled holes, as circles and slots. Ticked by default.
- Each Mechanical Layer.
- The Assembly and Courtyard Layers of each side.

### File form

- **Format:** ASCII DXF marked AC1015 (R2000), with handles, a LAYER table and `$INSUNITS`.
- **Contours:** each closed contour is one closed LWPOLYLINE with bulges, so an MCAD tool can extrude it directly.
- **Graphics:** LINE, ARC and CIRCLE, at zero width.
- **Text:** TEXT entities. A Dimension is written as its lines and text.
- **Options:** only the unit, mm (the default) or inch.
- **Coordinates** are measured from the [DXF origin point](#origin).
- Whether real MCAD importers accept the file is tested under [Which tests does v0.3 add?](https://github.com/openreda/reda/issues/441).

### Where it lives

One writer in `reda-output`, reached two ways:

- **An Output Job output kind,** off by default, written beside the zip.
- **File ▸ Export ▸ DXF…** (`pcb.export_dxf`), for a one-off hand-over.

## Origin

Import and export use the same point, so a file exported and imported again lands where it was: the [Output Origin](../v0.1/outputs.md#output-origin) if the user has placed one on the PCB Document, otherwise design zero. The Fab Preset's `file_origin` plays no part.

## Commands

- **`pcb.import_dxf.preview {path, unit?, scale?, close_gaps?, mapping?}`** changes nothing. It returns the DXF layers with entity counts, the pre-filled mapping, the size, the skipped entities, and the outline problems with their locations. The dialog and MCP agents both use it.
- **`pcb.import_dxf {path, unit?, scale?, close_gaps?, line_width?, mapping?, at?}`** imports as one undo step. It returns the same report plus the new objects' UUIDs. Fields left out take the preview's defaults. `at` is the Place with cursor point.
- **`pcb.export_dxf {path, entries?, unit?}`** writes the file. The Output Job kind takes the same `entries` and `unit`.

## Not in v0.3

Noted in [modules](../modules.md#later-fog) for a later milestone:

- DXF import into the Footprint editor.
- Copper, silk, mask and paste Layers in DXF export.
- HATCH and DIMENSION on import, and filled shapes (SOLID, TRACE).
- DWG.
