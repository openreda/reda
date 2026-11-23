# How EDA tools import and export DXF, and the Rust DXF crates

Research for #427, part of map #421 (v0.3, Mechanical & 3D). Gathered
2026-11-23; every URL below was read on that date unless a line says
otherwise. The note records what each tool does and where they differ. It
recommends nothing for REDA.

Where a source could not be reached, or a fact could not be checked against a
first-party source, the claim is marked **unverified** and says what it rests
on. The list of those gaps is collected in [Not verified](#not-verified).

## Summary

1. **Every importer is lossy, and each draws the line in a different place.**
   KiCad reads lines, arcs, circles, ellipses, polylines with bulges, splines,
   text, points and blocks, and reports hatches, dimensions, solids, traces and
   images as unsupported. Horizon EDA reads only lines, arcs, circles and
   polylines, and draws bulged polyline segments as straight lines. DipTrace
   states that it does not read splines. Altium lists the widest set, including
   hatches, regions, dimensions and splines.
2. **Units come from the user more often than from the file.** KiCad reads
   `$INSUNITS` and falls back to a "DXF default units" choice. Altium, DipTrace,
   Fusion/EAGLE and OrCAD X ask the user for the unit or scale. EasyEDA Pro
   fixes the unit at mm and offers a scale factor. Horizon EDA assumes 1 DXF
   unit = 1 mm with no option.
3. **Layer handling splits into three patterns.** A per-DXF-layer mapping table
   (Altium, DipTrace, OrCAD X, KiCad on its development branch), one target
   layer for the whole file (KiCad releases, Fusion/EAGLE, Horizon EDA), and a
   pick-list of DXF layers to bring in (EasyEDA Pro).
4. **Nobody turns a DXF into a board outline at import time.** The import
   places plain graphics on a layer. The outline comes from the layer they land
   on (KiCad Edge.Cuts, Fusion layer 20, OrCAD X Design_Outline) or from a
   second, explicit command run on the selection (Altium "Define from Selected
   Objects"). An open outline is caught later: by DRC in KiCad, by a failure
   message in Altium.
5. **Gap repair is offered by two tools.** KiCad has "Fix discontinuities" with
   a tolerance in the import dialog and again in Cleanup Graphics. Altium
   offers a fallback tracing algorithm that follows the outer edge of the
   tracks instead of their centreline, so the shape differs slightly.
6. **Only DipTrace and Altium recognise more than graphics.** DipTrace can turn
   DXF layers into pads, holes and filled polygons and DXF blocks into
   components. Altium can turn donuts into pads and blocks into components.
   KiCad reaches copper and Zones through separate convert-from-selection
   tools after the import.
7. **Export is a plot of the chosen layers.** KiCad writes LINE, ARC, CIRCLE,
   POLYLINE, POINT and TEXT, one DXF layer per board layer, with options for
   contours against centrelines, text as strokes against TEXT, one file against
   one file per layer, and mm against inches. Altium adds a choice of AutoCAD
   version, DXF or DWG, components as blocks, and pads as hatches or polylines.
   DipTrace and EasyEDA Pro pick layers and object kinds. Fusion exports the
   visible layers through a script. Horizon EDA has no DXF export.
8. **Rust crates.** `dxf` (MIT, since 2016, about 386,000 downloads) reads and
   writes ASCII and binary DXF from R10 to R2018 but has no HATCH entity.
   `acadrust` (MPL-2.0, first published 2026-02) reads and writes DXF and DWG
   with the widest entity coverage. The rest (`cadkit-dxf`, `undxf`,
   `cadrender-dxf`, `cadboss-dxf`, `dxfscan`, `dxf-tools-rs`) were all first
   published in 2026 and have a few hundred to a few thousand downloads.
   `uncad` wraps LibreDWG and is GPL-3.0-or-later.
9. **What the format demands of a small reader and writer.** Autodesk's
   reference says a writer may omit the whole HEADER and TABLES sections and
   need only end with `EOF`. A reader that matches KiCad's coverage needs the
   tag-pair parser, `$INSUNITS`, the LAYER table, BLOCKS with INSERT, about ten
   entity types, bulge arcs, spline conversion and the object coordinate system
   transform.

## The format facts the tools build on

These come from Autodesk's DXF reference.

- **File structure.** A DXF file is pairs of a group code and a value, each on
  its own line, organised into sections: HEADER, CLASSES, TABLES, BLOCKS,
  ENTITIES, OBJECTS and an optional THUMBNAILIMAGE.
  <https://help.autodesk.com/cloudhelp/2017/ENU/AutoCAD-DXF/files/GUID-D939EA11-0CEC-4636-91A8-756640A031D3.htm>
- **Versions.** `$ACADVER` names the drawing database version: AC1006 = R10,
  AC1009 = R11 and R12, AC1012 = R13, AC1014 = R14, AC1015 = 2000,
  AC1018 = 2004, AC1021 = 2007, AC1024 = 2010, AC1027 = 2013, AC1032 = 2018.
  <https://help.autodesk.com/cloudhelp/2018/ENU/AutoCAD-DXF/files/GUID-A85E8E67-27CD-4C59-BE61-4DC9FADBE74A.htm>
- **Units.** `$INSUNITS` (group 70) is described as the "default drawing units
  for AutoCAD DesignCenter blocks": 0 unitless, 1 inches, 2 feet, 4
  millimetres, 5 centimetres, 6 metres, 8 microinches, 9 mils, 13 microns, and
  others up to 24. `$MEASUREMENT` is 0 English or 1 Metric. Neither is
  mandatory, since the whole HEADER may be omitted. (Same page.)
- **Bulge.** On a polyline vertex, group 42 is "the tangent of one fourth the
  included angle for an arc segment, made negative if the arc goes clockwise
  from the start point to the endpoint. A bulge of 0 indicates a straight
  segment, and a bulge of 1 is a semicircle".
  <https://help.autodesk.com/cloudhelp/2018/ENU/AutoCAD-DXF/files/GUID-0741E831-599E-4CBF-91E1-8ADBCFD6556D.htm>
- **LWPOLYLINE.** Carries a vertex count (90), a flag (70, bit 1 = closed), an
  optional constant width (43), and per vertex X, Y, start and end width and
  bulge.
  <https://help.autodesk.com/cloudhelp/2018/ENU/AutoCAD-DXF/files/GUID-748FC305-F3F2-4F74-825A-61F04D757A50.htm>
- **Object coordinate systems.** Planar entities (circle, arc, text, insert, 2D
  polyline, lwpolyline, hatch and others) store points in an object coordinate
  system given by an extrusion vector. Lines and points are in world
  coordinates. The X axis of the object system is derived by the arbitrary
  axis algorithm: if `|Nx| < 1/64` and `|Ny| < 1/64` then `Ax = Wy × N`,
  otherwise `Ax = Wz × N`; `Ay = N × Ax`.
  <https://help.autodesk.com/cloudhelp/2017/ENU/AutoCAD-DXF/files/GUID-D99F1509-E4E4-47A3-8691-92EA07DC88F5.htm>
  <https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-E19E5B42-0CC7-4EBA-B29F-5E1D595149EE.htm>
- **Binary DXF.** Starts with the 22-byte sentinel
  `AutoCAD Binary DXF<CR><LF><SUB><NULL>`, then the same pairs in binary: a
  2-byte group code (1 byte before R14), then a little-endian integer, an
  8-byte IEEE double or a NUL-terminated string. It uses the same `.dxf`
  extension.
  <https://help.autodesk.com/cloudhelp/2017/ENU/AutoCAD-DXF/files/GUID-FC1C3C69-DBC2-49E4-893A-000D6538C0FE.htm>
- **What a writer may leave out.** "The entire HEADER section can be omitted if
  you don't set header variables. Any of the tables in the TABLES section can
  be omitted if you don't need to make entries, and the entire TABLES section
  can be dropped if nothing in it is required. Within the ENTITIES section, you
  can reference layer names even though you haven't defined them in the LAYER
  table. Such layers are automatically created with color 7 and the CONTINUOUS
  linetype. The EOF item must be present at the end of file."
  <https://help.autodesk.com/cloudhelp/2017/ENU/AutoCAD-DXF/files/GUID-5D1DFE5C-94FC-43B7-B535-43001D1662C1.htm>
- **What a reader should tolerate.** The reference advises writing readers "in
  a table-driven way, ignore undefined group codes, and make no assumptions
  about the order of group codes in an entity".
  <https://help.autodesk.com/cloudhelp/2018/ENU/AutoCAD-DXF/files/GUID-A35B8C2A-1885-4A8E-8533-E61D8A423D62.htm>

## Import, tool by tool

### KiCad

Sources: the PCB Editor manual for 10.0
(<https://docs.kicad.org/10.0/en/pcbnew/pcbnew.html>, sections "Importing
vector graphics", "Board outlines (Edge Cuts)", "Cleanup Graphics"), and the
source on the `master` branch at commit `0f4d229e` (2026-11-23):
<https://gitlab.com/kicad/code/kicad/-/blob/master/common/import_gfx/dxf_import_plugin.cpp>,
<https://gitlab.com/kicad/code/kicad/-/blob/master/common/import_gfx/dxf_import_plugin.h>,
<https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/import_gfx/dialog_import_graphics.cpp>.

- **Entry point.** File → Import → Graphics, or drag and drop onto the canvas.
  The same dialog imports DXF and SVG. (Manual.)
- **Parser.** The plugin is a `DL_CreationAdapter` subclass, the callback
  interface of the bundled dxflib library. The plugin stores no version: the
  constructor comment reads "the dxf version, not yet used", and
  `setVariableString`, which receives `$ACADVER`, is empty. (Source.)
- **Entities read.** LINE; ARC; CIRCLE; ELLIPSE and elliptical arcs (a ratio of
  1 becomes a circle or arc, otherwise a native ellipse shape); POLYLINE and
  LWPOLYLINE with bulges and the closed flag; SPLINE; TEXT and MTEXT; POINT
  (drawn as a small filled circle); BLOCK and INSERT. (Source.)
- **Entities reported as unsupported.** Each of these adds a line to a warning
  box shown after the import ("Items in the imported file could not be handled
  properly"): XLINE and RAY ("construction lines"), arc-aligned text, every
  DIMENSION kind and LEADER, HATCH, TRACE, 3DFACE, SOLID and IMAGE. Attributes
  and extended data are ignored silently. (Source, header file.)
- **Bulges.** A bulge below 0.0218 in magnitude (about 5 degrees) is drawn as a
  straight segment; otherwise an arc of `4·atan(bulge)`. (Source.)
- **Splines.** Converted with the tinyspline library from control points and
  knots, raised to degree 3 if lower, and split into cubic Béziers, which the
  board stores natively. Only the X and Y of each control point are passed on,
  so weights are not used. An invalid spline is dropped with "Invalid spline
  definition encountered". (Source.)
- **Blocks.** Each BLOCK is buffered and each INSERT clones its shapes with the
  insert's rotation, X and Y scale and position. A shape on DXF layer `0`
  inside a block takes the INSERT's layer. (Source.)
- **3D.** Object coordinates are run through the arbitrary axis transform, and
  Z is dropped: "We silently drop the z coordinate on 3d polylines". (Source.)
- **Text.** Imported as board text with mapped justification. MTEXT formatting
  codes are parsed and mostly discarded. The source says "There is no sane way
  to make text look perfect like the original CAD."
- **Units.** `$INSUNITS` values 1, 2, 4, 5, 6, 8, 9, 10, 11, 12, 13 and 14 set
  the unit. The dialog's "DXF default units" (Inches, Feet, Millimeters,
  Centimeter, Mils) "sets the default unit for DXF files with unspecified
  units". A separate "Import scale" multiplies the result. As read in the
  source, an `$INSUNITS` that is present with an unmapped value (0 unitless
  among them) resets the unit to `DEFAULT`, for which the scale is 1.0, that
  is, millimetres. (Manual and source.)
- **Line width.** The entity's lineweight in hundredths of a millimetre, else
  the DXF layer's, else the dialog's "DXF default line width". (Source.)
- **Layers.** In 10.0 everything goes to one board layer: the chosen "Layer" or
  the active layer (manual). On `master` the dialog has "Single layer",
  "Current layer" and "Map layers"; the last opens a mapping dialog with one
  row per DXF layer, pre-filled where the DXF layer name equals a board layer
  name (source, `getAutoMappedLayer`). The 10.0 manual does not describe "Map
  layers".
- **Other options.** "Place at" X and Y or interactive placement, "Group
  imported items", and "Fix discontinuities" with a tolerance: "any shape
  discontinuities smaller than the specified tolerance are filled by extending
  each segment until they intersect or adding an additional segment".
  (Manual.)
- **Where it may land.** Any enabled board layer, and the Footprint Editor uses
  the same importer (the plugin has an `m_importAsFPShapes` mode and the dialog
  permits all layers when there is no board). "Imported vector graphics are
  part of the design like any other graphic shape ... shapes on copper layers
  can make electrical connections." Zones, rule areas, polygons and tracks are
  made afterwards with Create Zone / Rule Area / Polygon / Tracks From
  Selection. (Manual.)
- **Open or self-crossing outline.** The import does not check. The outline is
  whatever sits on Edge.Cuts: "the endpoints of any shapes in the outline must
  coincide exactly ... Outline shapes also cannot intersect each other or
  overlap. In such cases, DRC will report a 'Board has malformed outline'
  violation that points to the problematic parts". With an invalid outline
  "some functions such as the 3D viewer and some design rule checks will not
  be functional". Closed shapes inside the outer one are cutouts. Repair is by
  the import option above, the Heal Shapes tool, or Tools → Cleanup Graphics →
  "Fix discontinuities in board outlines". (Manual.)

### Altium Designer

Sources: "AutoCAD-DXF Import-Export Support"
(<https://www.altium.com/documentation/altium-designer/design-tools-interfacing/autocad-dxf-import-export>,
page updated 2026-11-02), "Defining the Board Shape"
(<https://www.altium.com/documentation/altium-designer/defining-board-shape>,
updated 2026-10-05), and two knowledge-base articles:
<https://www.altium.com/documentation/knowledge-base/altium-designer/how-to-determine-the-scale-units-of-a-dxf-dwg-file>
and
<https://www.altium.com/documentation/knowledge-base/altium-designer/import-a-mechanical-outline-to-pcb>.
These pages were read through a fetch tool that summarises; the option names
below are as it quoted them.

- **Formats and versions.** DXF and DWG, for AutoCAD R12, R13, R14, 2000, 2004,
  2007, 2010, 2013 and 2018, both directions.
- **Supported object types** (one list for import and export): Arc, Circle,
  Dimension, Ellipse, Elliptical Arc, Hatch, Insert, Leader, Line, Multileader,
  Multiline, Multitext, Point, Polyline, Rectangle, Region, Solid, Spline,
  Text, Trace. The page does not say what each becomes on a board.
- **Entry point.** File → Import → DXF/DWG in the PCB editor; File → Import →
  AutoCAD in the PCB library editor (the dialog is captioned "PCB or PCB
  footprint"). Schematic and schematic library have their own.
- **Blocks.** "Import as components" places blocks as PCB components; "Import
  as primitives" breaks everything, block contents included, into tracks, arcs
  and so on.
- **Drawing Space.** Model (recommended, 1:1) or Paper.
- **Scale.** "1 AutoCAD unit =" a chosen unit, or a custom factor. A Size
  readout shows the resulting extent in imperial and metric. The knowledge base
  tells the user to open the DXF as text, find `$INSUNITS`, look its value up
  in Autodesk's table, and pick the matching scale. It does not describe the
  dialog reading `$INSUNITS` itself.
- **Default Line Width.** Used "when a line object in the source AutoCAD file
  does not include width data".
- **Fonts.** Stroke or TrueType for converted text.
- **Locate AutoCAD (0,0) at.** X and Y, or pick a point on the board.
- **Layer Mappings.** A table of source layer name to PCB layer; "Not Imported"
  skips a layer.
- **Donut To Pad Conversion.** Circles are turned into pads sized to the outer
  diameter: All, None, Larger Than or Smaller Than a size.
- **Settings.** Load and save the mapping as an `.ini` file.
- **Board outline.** Not created by the import. The documented flow is: map the
  DXF layer to a mechanical layer, select the tracks and arcs, then Design →
  Board Shape → Define from Selected Objects. "The software will attempt to
  find the shape based on the centerline of the selected objects."
- **Open or mismatched outline.** "If the coordinates for the end of one
  track/arc segment do not exactly match the coordinates of the next ... the
  boundary identification algorithm will fail and a message will be displayed
  showing the failure location. It will offer to use a tracing algorithm
  instead"; "the tracing algorithm follows the outer edge of the track/arc
  objects, so the Board Shape will be slightly different." No tolerance setting
  is documented.
- **Cutouts.** "Internal cutouts are not automatically created (but can be
  defined later from imported objects)", with Tools → Convert → Create Board
  Cutout from Selected Primitives.
- **Other outline source.** The board shape can also be defined from a planar
  face of an imported STEP, Parasolid or SOLIDWORKS body.

### Fusion Electronics and EAGLE

Sources: Autodesk support article "How to import a DXF file into Fusion/EAGLE
as board contour or footprint", read through
<https://help.autodesk.com/view/fusion360/ENU/?caas=caas%2Fsfdcarticles%2Fsfdcarticles%2FHow-to-import-a-DXF-file-into-a-PCB-footprint-in-Fusion-360-Electronics.html>
(the `autodesk.com/support` copy returned HTTP 403).

- **Fusion, native.** In the 2D PCB or the library footprint editor: Place →
  Insert DXF/DWG, pick the file, choose one target layer ("20 Dimension" for a
  board outline, "21 SilkscreenTop" for a footprint), choose the units, place
  the shape, Done.
- **Fusion, through the mechanical side.** Insert the DXF into a sketch, then
  Create PCB (associative or independent) from the sketch profile, then link
  or push it to a 2D PCB.
- **EAGLE.** Run the `import-dxf` ULP: browse for the file, choose the target
  layer ("20 Dimension" or "21 tPlace"), choose "Input Units" Imperial or
  Metric.
- **Entities, scale, open outlines.** Not covered by the article.
  **Unverified**, from search summaries of Autodesk forum threads that could
  not be opened (HTTP 403): users report that the old ULP did not read
  splines, that curved outlines import as coarse segments, and that the dialog
  has a scale field.

### EasyEDA Pro

Sources: <https://prodocs.easyeda.com/en/pcb/file-import-dxf/> and a forum
thread
<https://easyeda.com/forum/topic/Bug-report-DXF-version-too-old-c9bfab1bbc914719a1247ee74c2399cc>.

- **Purpose.** "EasyEDA Pro supports importing DXF files as the border of the
  board", and for other purposes. File → Import → Import DXF.
- **Options.** DXF unit: mm by default and not changeable. Scaling factor.
  Line width (its unit is given as inch, not changeable). Width and height,
  which follow the file's extent. Reference point, relating the DXF origin to
  the drawing's centre. A list to select which DXF layers to import.
- **Versions.** In April 2023 JLCEDA Pro 1.9.30 refused a file with "DXF
  version too old, please import DXF 2007 and above version", and support
  replied that this would be solved in v2.0. Whether it was is **unverified**.
- **Entities, target layer choice, outline handling.** Not described in text on
  the page (the dialog is shown only as screenshots). **Unverified.**

### OrCAD X

No first-party Cadence documentation could be reached (the product help is not
public and community.cadence.com returned HTTP 403). What follows rests on a
tutorial by EMA Design Automation, a Cadence reseller, for OrCAD X Presto 24.1,
updated 2025-03-06:
<https://www.ema-eda.com/how-to-page/how-to-create-a-board-outline-from-a-dxf-in-orcad-x-presto/>.
Treat it as **secondary**.

- **Entry point.** ECO → DXF Import opens the "DXF In" window.
- **Units.** A DXF units field, set to mm in the tutorial.
- **Layers.** "Edit/View Layers" builds a layer conversion file. Each DXF layer
  is selected and mapped to a Class and Subclass, for example DXF `Cutout` to
  Board Geometry / Cutout and DXF `Outline` to Board Geometry /
  Design_Outline. Unselected DXF layers are not imported.
- **Board outline.** Mapping to Design_Outline makes the outline directly, with
  no second command. Conductor and component keep-ins are then set as offsets
  in the outline's properties.
- **Log.** The import writes a log; the tutorial's file produced an extents
  warning that it says can be ignored.
- **Unverified**, from search summaries of Cadence forum threads: an
  "Incremental addition" option adds to the open design instead of a blank
  one, a "Fill Shapes" option fills closed polylines, the conversion file is a
  `.cnv` file used for import and export alike, and the dialog assumes the DXF
  units equal the board units unless told otherwise.

### DipTrace

Source: the online help, sections 3.2.6.2 "DXF" (PCB Layout import) and
5.2.12.8 / 5.3.16 (Pattern Editor import):
<https://diptrace.com/support/tutorials/help_html/#pcb-dxf-import>. The help
does not state which DipTrace version it describes.

- **Scope.** "Import entire board layout or any part from DXF files", into PCB
  Layout and into the Pattern Editor (a footprint).
- **Units.** A "DXF Units" drop-down: inches, millimetres or mils, "in which
  the .dxf file was saved".
- **Import mode.** New (make a new project) or Add (add to the open layout).
- **Layers.** Each DXF layer is assigned a PCB layer from a "Convert to" list;
  check marks show or hide DXF layers in the preview.
- **Splines.** "DipTrace currently does not support splines in DXF files. If
  you have DXF import related issues, most probably these were caused by DXF
  splines. Convert splines to regular polygons before importing".
- **Pad recognition.** A layer can be converted to "Pads" and another to "All
  Holes", giving through pads, surface pads and mounting holes; or "Pads in
  Signal Layers" recognises pads among traces by a list of shapes and sizes.
- **Fill closed areas.** Closed areas "formed by lines, arcs, polylines,
  rectangles" become filled polygons; "Embedded Polygons" alternates fill and
  hole by nesting depth.
- **Blocks.** "DXF blocks can be converted to groups or components."
- **Open outlines.** Not described.

### Horizon EDA

Source: the code at commit `79e34226` (2026-11-17):
<https://github.com/horizon-eda/horizon/blob/master/src/import_dxf/dxf_importer.cpp>
and
<https://github.com/horizon-eda/horizon/blob/master/src/core/tools/tool_import_dxf.cpp>.

- **Parser.** dxflib, through a `DL_CreationAdapter`.
- **Entities read.** LINE; ARC; CIRCLE (stored as two half arcs, added in the
  change the changelog calls "DXF import: support circles"); POLYLINE and
  LWPOLYLINE vertices joined by straight lines, with the closed flag honoured.
  The vertex handler does not use the bulge, so bulged segments become
  straight.
- **Entities counted as unsupported.** ELLIPSE and SPLINE. The tool bar shows
  "Unsupported items: N Ellipse(s), N Spline(s)" in red. Every other entity
  type is ignored without a message.
- **Units and scale.** The tool calls `set_scale(1)`, and coordinates are
  multiplied by 1e6 nm, so 1 DXF unit is 1 mm. `$INSUNITS` is not read.
  Coordinates are rounded to the nearest micrometre.
- **Layers.** DXF layers are ignored. Everything goes to the current work
  layer, and changing the work layer while the import is still attached to the
  cursor moves it.
- **Deduplication.** Endpoints that land on the same coordinate share one
  junction, and a duplicate LINE between the same two junctions is skipped.
- **Interaction.** The result follows the cursor and can be rotated, mirrored
  and given a line width before it is placed.
- **Where it may land.** The tool is available in any editor whose document has
  lines and arcs; the importer is referenced from the board, package and decal
  editors.
- **Outline handling.** Not in this code. **Unverified** how Horizon turns
  lines into its board outline.

## Import compared

| | Units | DXF layers | Splines | Bulges | Blocks | Text | Hatches |
|---|---|---|---|---|---|---|---|
| KiCad | `$INSUNITS`, else a dialog default, times a scale | one layer (10.0); mapping table (master) | Béziers | arcs | expanded | yes | reported unsupported |
| Altium Designer | user picks scale | mapping table | listed as supported | listed | components or primitives | stroke or TrueType | listed as supported |
| Fusion / EAGLE | user picks units | one target layer | unverified | unverified | unverified | unverified | unverified |
| EasyEDA Pro | fixed mm, scale factor | pick which to import | unverified | unverified | unverified | unverified | unverified |
| OrCAD X (secondary) | user picks units | mapping to class/subclass | unverified | unverified | unverified | unverified | unverified |
| DipTrace | user picks in, mm or mil | mapping table | not read | not stated | groups or components | not stated | not stated |
| Horizon EDA | 1 unit = 1 mm | ignored | counted unsupported | drawn straight | ignored | ignored | ignored |

| | Lands on | Outline made by | Open outline |
|---|---|---|---|
| KiCad | any layer, board or footprint; copper and Zones by later conversion | graphics being on Edge.Cuts | DRC "Board has malformed outline"; optional gap fix with tolerance |
| Altium Designer | any mapped layer, board or footprint; pads from donuts; components from blocks | a second command on the selection | failure message at the spot; offers outer-edge tracing |
| Fusion / EAGLE | one chosen layer, board or footprint | graphics being on layer 20 | unverified |
| EasyEDA Pro | board border and "other purposes" | unverified | unverified |
| OrCAD X (secondary) | mapped class/subclass | mapping to Design_Outline | unverified |
| DipTrace | any mapped layer, board or footprint; pads, holes, filled polygons, components | not stated | not stated |
| Horizon EDA | the work layer of a board, package or decal | unverified | unverified |

## Export, tool by tool

### KiCad

Sources: the manual's "DXF options" under Plotting, the command-line reference
(<https://docs.kicad.org/10.0/en/cli/cli.html>, `pcb export dxf`), and
`common/plotters/DXF_plotter.cpp` on the `9.0`, `10.0` and `master` branches
(<https://gitlab.com/kicad/code/kicad/-/blob/master/common/plotters/DXF_plotter.cpp>).

- **What is written.** DXF is one output format of the Plot dialog, so any
  selected board layer can be plotted, copper included, with the dialog's
  common choices (drawing sheet, reference designators and values, drill
  marks, scale, drill/place origin).
- **Options.** "Plot graphic items using their contours" (outer contour of a
  thick shape, or else its centreline, "Graphic shapes in DXF files have no
  width"); "Use KiCad font to plot text" (strokes, or else DXF text objects
  that "will not appear in exactly the same position and size"); "Single
  document" ("each PCB layer plotted as a separate DXF layer", or else one
  file per layer); "Export units" mm or inches ("Since the DXF format has no
  specified units system, you must export using the same units setting that
  you want to use for importing into other software").
- **Entities.** LINE with a linetype, CIRCLE, ARC, POLYLINE with VERTEX and
  SEQEND (a filled circle is a two-vertex closed polyline with bulge 1 and a
  width), POINT, and TEXT with justification codes. Text falls back to strokes
  when it has non-ASCII characters, several lines, or superscript or
  subscript. (Source.)
- **Header and version.** The 9.0 plotter writes `$ANGBASE`, `$ANGDIR`,
  `$MEASUREMENT` and `$INSUNITS` but no `$ACADVER`. The 10.0 and master
  plotters write `$ACADVER` AC1018, and the source explains why: "Tagged
  AC1018 (R2004) because the LAYER table emits the 420 true-color group, which
  AutoCAD only accepts in R2004+. That in turn requires handles on every
  record and entity plus a full R2000 table and blocks skeleton." (Source.)
- **Layers.** A LAYER table with layer `0` and one DXF layer per plotted board
  layer, named after it. (Source.)

### Altium Designer

Source: the "AutoCAD-DXF Import-Export Support" page above.

- **Format.** "AutoCAD Version" (R12 to 2018), "Format" DWG or DXF, "Units"
  Imperial or Metric.
- **Layers.** A tree of layers with an Include box each, and an "All On" list.
  Layer names can follow the Layer Stack Manager when the advanced setting
  `Exporters.AutoCAD.LayerNames` is on.
- **Components.** "Export as blocks" or "Export as primitives".
- **Tracks and arcs.** "Export with rounded ends" (adds small arcs) or "Export
  with square ends" (advised when geometry will be extracted in MCAD).
- **Zero width.** "Primitives To Export With Zero Line Widths": None, All or
  Selected.
- **Pads and vias.** "Export with hatches" or "Export with polylines", and
  "Export pads and via holes".
- **Other editors.** Schematic, schematic library, PCB library and Draftsman
  each have an export.

### DipTrace

Source: help section 3.2.7.2,
<https://diptrace.com/support/tutorials/help_html/#pcb-dxfexport>.

- "You can select PCB layers and define objects which will be included in the
  DXF file. All PCB layers will become logical layers of the DXF file."
- Vector text is written "as AutoCAD text object or as polylines (Convert
  Vector Text to Lines checkbox)".
- Edge_Top and Edge_Bottom layers hold "automatically generated edges for
  traces", meant for isolation milling through a DXF-to-G-code tool.
- The schematic exports DXF too, per sheet, with text, tables and titles
  switchable and a coordinate offset or the design origin.

### EasyEDA Pro

Source: <https://prodocs.easyeda.com/en/pcb/export-dxf/>.

- File → Export → DXF. Export presets are kept on the left. Each preset sets
  which layers are exported, whether each is mirrored, and which objects are
  included. The DXF keeps the chosen layers as DXF layers.
- The option names, the object list, the units and the DXF version are shown
  only in screenshots. **Unverified.**

### Fusion Electronics and EAGLE

Source: Autodesk support article "How to export copper or other layers of 2D
PCB in .dxf format in Fusion Electronics", read through
<https://help.autodesk.com/view/fusion360/ENU/?caas=caas%2Fsfdcarticles%2Fsfdcarticles%2FHow-to-export-copper-or-other-layers-of-2D-PCB-in-dxf-format-in-Fusion-Electronics.html>.

- Make the wanted layers visible, switch text from the proportional to the
  vector font "so the text displays properly in the DXF", then Automation →
  Run ULP → `dxf.ulp`, set the export options and the output path.
- The export is of the visible layers. The script's own options are not
  listed in the article. **Unverified.**

### OrCAD X

**Unverified.** No first-party page was reached. Search summaries of Cadence
forum posts say File → Export → DXF writes DXF Revision 12 or Revision 14,
uses the same layer conversion file as import, exports the visible classes
and subclasses, and has an "Export symbols and padstacks as BLOCKs" option.

### Horizon EDA

A GitHub code search for `dxf` in `horizon-eda/horizon` returns only the
import files, the tool registration, the dialogs and the changelog. No DXF
export code was found.

## Rust DXF crates

Figures are from the crates.io API
(<https://crates.io/api/v1/crates/NAME>) and the GitHub API, read 2026-11-23.
"Downloads" is all-time.

| Crate | Version | Licence | First published | Last release | Downloads | Reads | Writes |
|---|---|---|---|---|---|---|---|
| [`dxf`](https://crates.io/crates/dxf) | 0.6.2 | MIT | 2016-09-14 | 2026-10-03 | 385,704 | ASCII and binary DXF, DXB | DXF, DXB |
| [`acadrust`](https://crates.io/crates/acadrust) | 0.6.8 | MPL-2.0 | 2026-02-07 | 2026-10-27 | 185,983 | ASCII and binary DXF, DWG | DXF, DWG |
| [`cadkit-dxf`](https://crates.io/crates/cadkit-dxf) | 0.5.4 | MIT OR Apache-2.0 | 2026-10-01 | 2026-11-16 | 2,854 | ASCII and binary DXF | ASCII DXF |
| [`undxf`](https://crates.io/crates/undxf) | 0.11.0 | MIT | 2026-09-29 | 2026-11-05 | 777 | ASCII DXF | nothing |
| [`dxf-tools-rs`](https://crates.io/crates/dxf-tools-rs) | 0.1.3 | MIT | 2026-01-20 | 2026-01-20 | 649 | ASCII and binary DXF | DXF |
| [`dxfscan`](https://crates.io/crates/dxfscan) | 0.1.0 | ISC | 2026-03-31 | 2026-03-31 | 215 | binary DXF | nothing |
| [`cadcraft-dxf`](https://crates.io/crates/cadcraft-dxf) | 1.4.7 | MIT OR Apache-2.0 | 2026-10-24 | 2026-10-24 | 92 | DXF tags | DXF tags |
| [`cadrender-dxf`](https://crates.io/crates/cadrender-dxf) | 0.2.0 | MIT OR Apache-2.0 | 2026-11-14 | 2026-11-16 | 61 | ASCII DXF | nothing |
| [`cadboss-dxf`](https://crates.io/crates/cadboss-dxf) | 0.1.1 | MIT OR Apache-2.0 | 2026-11-05 | 2026-11-05 | 55 | DXF | nothing |
| [`bilingua-dxf`](https://crates.io/crates/bilingua-dxf) | 0.10.0 | Apache-2.0 | 2026-11-09 | 2026-11-19 | 37 | ASCII DXF (a text filter) | not checked |
| [`uncad`](https://crates.io/crates/uncad) | 0.18.0 | GPL-3.0-or-later | 2026-08-24 | 2026-11-05 | 582 | DWG and DXF through LibreDWG | not checked |

The Reads and Writes columns for the crates below `undxf` come from their
one-line crates.io descriptions only.

- **`dxf`** (<https://github.com/ixmilia/dxf-rs>, 144 stars, 26 open issues,
  last push 2026-11-18, one author).
  - Versions: the `AcadVersion` enum runs from Version_1_0 through R10, R12,
    R13, R14, R2000, R2004, R2007, R2010, R2013 to R2018.
  - Entities, generated from `spec/EntitiesSpec.xml`: 3DFACE, 3DSOLID, ARC,
    ARCALIGNEDTEXT, ATTDEF, ATTRIB, BODY, CIRCLE, DIMENSION, ELLIPSE, HELIX,
    IMAGE, INSERT, LEADER, LIGHT, LINE, LWPOLYLINE, MLINE, MTEXT, POINT,
    POLYLINE, RAY, REGION, SHAPE, SOLID, SPLINE, TEXT, TOLERANCE, TRACE,
    VERTEX, WIPEOUT, XLINE and the underlays.
  - **No HATCH.** The spec file has no hatch entity, and issues #25 "Support
    Hatch entities" and #102 "feat: support hatch" are open.
  - It parses; it does not convert. Bulges, splines and object coordinates
    arrive as stored values.
  - Dependencies: byteorder, chrono, encoding_rs, enum_primitive, image (bmp
    only), itertools, num, uuid, and optional serde.
  - Its interoperability tests "currently only run on Windows when the ODA
    File Converter tool has been installed" (README).
- **`acadrust`** (<https://github.com/hakanaktt/acadrust>, 189 stars, 13 open
  issues, last push 2026-11-23).
  - Versions: DXF and DWG read and write for AC1009 through AC1032, with "DXF
    export of R11/R12 documents is promoted to R13 or later" (README).
  - "48 top-level `EntityType` variants" (README).
  - MPL-2.0 is a file-level copyleft licence, not one of the permissive
    licences. The README points to a third-party notices file "for DWG code
    provenance and attribution".
  - Dependencies: anyhow, bitflags, byteorder, encoding_rs, flate2, foldhash,
    indexmap, itoa, memmap2, nom, oem_cp, once_cell, ryu, thiserror, web-time,
    and optional quick-xml, rayon, serde, serde_json.
  - 37 releases in under ten months.
- **`cadkit-dxf`** (<https://github.com/TNYCL/cadkit>, 11 stars; the GitHub API
  reports the repository licence as Apache-2.0 while the crate declares MIT OR
  Apache-2.0). Reads ASCII and binary DXF R12 to 2018 and writes ASCII DXF R12
  to 2018 into the cadkit document model; the README calls it "Implemented;
  entity coverage is still growing". Dependencies: cadkit-core, encoding_rs.
- **`undxf`** (<https://github.com/iyulab/undxf>, 0 stars). The README calls it
  "0.x, a spike", "not a writer", and lists LINE, CIRCLE, ARC, ELLIPSE,
  LWPOLYLINE, POLYLINE, SPLINE, TEXT, MTEXT and more, with unknown entity
  types kept under their names. Dependencies: encoding_rs, serde, uncad-model.
- **`uncad`** is built on LibreDWG through `libredwg-sys` and takes its
  GPL-3.0-or-later licence.

## What a minimal reader and writer would have to cover

This section lists what the format reference and the surveyed importers imply.
It is an inventory, not a design.

### Reader

- **Tag pairs.** Lines alternate between an integer group code and a value.
  Sections are opened by `0 SECTION`, `2 <name>` and closed by `0 ENDSEC`; the
  file ends with `0 EOF`. Unknown group codes and unknown entity types must be
  skipped, and group order within an entity must not be assumed (Autodesk).
- **Binary files.** Recognised by the 22-byte sentinel. KiCad and Horizon EDA
  rely on dxflib for parsing; whether dxflib reads binary DXF was not checked.
- **HEADER.** `$INSUNITS` for the unit (KiCad maps twelve of its values),
  optionally `$MEASUREMENT`, and `$DWGCODEPAGE` for the text encoding of older
  files (KiCad stores it). A file may have no HEADER at all, so a user-chosen
  unit or scale is needed as well; every surveyed tool has one except Horizon
  EDA.
- **TABLES.** The LAYER table for layer names, needed for any mapping table,
  and each layer's lineweight if widths are taken from the file as KiCad does.
  Entities may name layers that the table does not define.
- **BLOCKS and INSERT.** Block definitions with a base point; inserts with
  position, rotation, X and Y scale, applied recursively. The DXF layer `0`
  inside a block inherits the insert's layer (KiCad).
- **Entities.** The set KiCad reads: LINE, ARC, CIRCLE, ELLIPSE, LWPOLYLINE,
  POLYLINE with VERTEX and SEQEND, SPLINE, TEXT, MTEXT, POINT. The set it
  reports and skips: HATCH, DIMENSION, LEADER, SOLID, TRACE, 3DFACE, IMAGE,
  XLINE, RAY. Horizon EDA's set is LINE, ARC, CIRCLE and straight polylines.
- **Geometry rules.**
  - Arc angles are in degrees and counter-clockwise; ellipse parameters are in
    radians (KiCad source comment).
  - Bulge to arc: included angle `4·atan(bulge)`, negative for clockwise.
  - Polyline flag bit 1 closes the shape with a last segment that may itself
    be bulged.
  - Splines carry degree, knots, control points, optional weights and optional
    fit points; KiCad converts through cubic Béziers and ignores weights.
  - Planar entities are in object coordinates and need the arbitrary axis
    transform. KiCad's source notes that SolidWorks output can contain
    mirrored circles that only come out right after this transform.
  - Z is dropped.
- **Text.** TEXT has two alignment points and horizontal and vertical
  justification codes. MTEXT has inline formatting codes (`\P`, `\U+XXXX`,
  `\S`, font, height and colour changes) that must at least be stripped, and
  `%%c`, `%%d`, `%%p` style symbols.
- **Reporting.** KiCad and Horizon EDA both tell the user which entity kinds
  were skipped.

### Writer

- **Smallest valid file.** An ENTITIES section and `EOF`. HEADER and TABLES
  may be omitted, and layers named by entities are created on load with
  colour 7 and the CONTINUOUS linetype (Autodesk).
- **Units.** Writing `$INSUNITS` and `$MEASUREMENT` is what lets a reader such
  as KiCad scale the file without asking. KiCad writes both.
- **Version.** KiCad 9.0 writes no `$ACADVER`. KiCad 10.0 writes AC1018 and
  had to add handles on every record and entity, a full table set and a blocks
  skeleton to do so. Altium lets the user pick from R12 to 2018.
- **Entities used by KiCad's writer.** LINE, ARC, CIRCLE, POLYLINE with VERTEX
  and SEQEND, POINT, TEXT. Altium additionally writes blocks for components
  and hatches or polylines for pads.
- **Choices the surveyed exporters expose.** Which layers; one file or one per
  layer; mm or inches; centreline or outline for shapes with width; text as
  TEXT or as strokes; components as blocks or exploded; holes in or out;
  mirroring per layer (EasyEDA Pro); origin (KiCad drill/place origin,
  DipTrace design origin).

## Not verified

- **Autodesk DXF reference, 2012 PDF.** Downloaded but its text could not be
  extracted here. The HTML references for 2017, 2018 and 2024 were used
  instead.
- **Altium Designer.** How each supported object type is converted on import
  (what a hatch, spline or dimension becomes), and whether the dialog
  pre-fills the scale from `$INSUNITS` or the layer table from layer names.
- **Fusion Electronics / EAGLE.** No help-reference page for the Insert DXF/DWG
  dialog was found. Entity support, scale, handling of splines and open
  outlines, and the options of `dxf.ulp` and `import-dxf.ulp` are unverified;
  the Autodesk forum threads returned HTTP 403.
- **EasyEDA Pro.** Supported DXF versions today, supported entities, how the
  board border is formed from the imported shapes, and the export dialog's
  option names, units and DXF version.
- **OrCAD X.** Everything: no first-party source was reached. The import
  section rests on a reseller's tutorial and the export section on search
  summaries of forum posts.
- **DipTrace.** Which version the online help describes; treatment of bulges,
  ellipses, text and hatches; what happens with an open outline; which DXF
  version is written.
- **Horizon EDA.** How imported lines become the board outline, and whether
  the absence of DXF export is deliberate.
- **dxflib.** Its supported DXF versions and whether it reads binary DXF.
- **Rust crates.** None was built or run. Entity coverage and version support
  are as their READMEs and source files state. The crates below `undxf` in the
  table were not opened beyond their crates.io metadata.
