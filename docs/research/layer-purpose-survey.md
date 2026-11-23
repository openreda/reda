# How other EDA tools give mechanical and documentation layers a purpose

Research for [#429](https://github.com/openreda/reda/issues/429), part of map
[#421](https://github.com/openreda/reda/issues/421) (v0.3 Mechanical & 3D).
All sources were read on 2026-11-23. This note records what tools do and where
they differ. It recommends nothing for REDA.

Claims marked **unverified** could not be followed to a primary source, and say
what they rest on. The "Not verified" section at the end collects them.

## Summary

Tools mark what a non-copper layer is for in one of three ways:

1. **A fixed layer per purpose.** KiCad, Horizon EDA, DipTrace, Fusion/EAGLE and
   EasyEDA Pro ship a closed list of layers whose identity is the purpose
   (courtyard, fab or assembly, package body, board outline). The program reads
   these layers directly. Extra "user" layers carry no purpose, or only a side.
2. **Generic layers with a type tag.** Altium Designer has an open-ended list of
   numbered mechanical layers. Each layer, or each top/bottom pair, may carry a
   Layer Type from a fixed list. The type is how the program finds the layer,
   and layers without a type fall back to matching by number.
3. **A two-level class and subclass tree.** OrCAD X / Allegro files every object
   under a class (Package Geometry, Board Geometry) and a subclass
   (Place_Bound_Top, Assembly_Top, Design_Outline). The subclass name is the
   purpose.

Across all of them the same small set of purposes is machine-read: the board
outline, the courtyard or placement boundary, and a body outline used for
height or 3D. Everything else (assembly outlines, notes, dimensions, glue,
coating) is a label that people and output setups pick by hand.

## REDA today

Where the current model already implies a purpose:

- **The Layer kind is already a purpose for four kinds.** `LayerKind` has
  `Courtyard`, `Assembly`, `BoardOutline`, `Mechanical` and `Documentation`
  beside the copper, silk, mask and paste kinds
  (`crates/reda-model/src/pcb.rs`, `LayerKind`). The v0.1 fixed set is
  "Courtyard and Assembly, top and bottom", "the Board Outline" and "Mechanical
  and Documentation layers" ([pcb.md, Layers](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/pcb.md#layers)).
- **Pairing is by `side`.** A Layer has `kind` and an optional `side`. Courtyard
  and Assembly are sided, so a top and bottom Layer of the same kind form a
  pair. Mechanical and Documentation have no side, and there is one of each
  ([drawings.md](https://github.com/openreda/reda/blob/main/docs/spec/v0.2.5/drawings.md#what-a-drawing-is):
  "the board's single Mechanical Layer or its single Documentation Layer").
- **Courtyard is machine-read.** It drives the courtyard overlap check, the
  Footprint-outside-outline check and the `footprints` Keepout check
  ([checks.md](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/checks.md#courtyards)),
  the Footprint's selection area, the `avoid` placement stop and alignment
  guides ([pcb.md, Placement](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/pcb.md#placement)),
  and the body box ("Its body box is its courtyard's bounding box",
  [library.md](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/library.md)).
  Courtyard Layers take only closed shapes.
- **Assembly holds the outline and texts.** Footprint `texts` sit on Silkscreen
  or Assembly Layers, and each Starter Library Footprint has an Assembly
  outline, an Assembly Designator and Value text
  ([library.md](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/library.md)).
  Assembly PDFs are deferred to v0.7
  ([outputs.md](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/outputs.md)).
- **Board Outline is its own kind**, not a tagged mechanical layer.
- **Height is a Footprint field**, not a layer property: "Footprints already
  store `height`" (map #421), and v0.1 has "no separate body outline"
  ([schematic.md](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/schematic.md)).
- **Mechanical and Documentation are told apart by convention only.** The v0.2.5
  built-in templates put "the host or mating context" on Documentation and "the
  key Dimensions" on Mechanical. Both take graphics, both have symbolic names
  (`"mechanical"`, `"documentation"`), and "No fab output plots either of them"
  ([drawings.md](https://github.com/openreda/reda/blob/main/docs/spec/v0.2.5/drawings.md)).
  A drawing group row names its `layer` as `mechanical` or `documentation`.
- **Import already maps onto purposes.** KiCad "Fab Layer lines and text" go to
  the Assembly Layer, and "Courtyard lines and arcs" are chained into a Polygon
  ([library.md, import table](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/library.md)).
- **The reserved name.** `purpose` is held "on Mechanical and Documentation
  Layers" for v0.3
  ([architecture.md, Reserved names](https://github.com/openreda/reda/blob/main/docs/spec/v0.1/architecture.md#reserved-names)).
  Layer Sets already include a `Mechanical` set and a `Fab` set (pcb.md).

## Altium Designer

Source for this section unless stated:
[Working with Mechanical Layers](https://www.altium.com/documentation/altium-designer/pcb/mechanical-layers)
(unversioned page, screenshots from AD21 to AD24).

- **Model.** Mechanical layers are numbered, user-named and open-ended: "There
  is no limit to the number of mechanical layers that can be added." Older
  versions were limited to 32. A layer is added either as an individual layer or
  as a Component Layer Pair.
- **Layer Type.** Each layer or pair has a Layer Type that "can be unassigned
  (N/A), or you can select a type from the list". Assigning a type names the
  layer after it, and a user name can override that, in which case the type is
  shown in brackets. The two lists are separate.
- **Individual layer types:** Assembly Notes, Board, Board Shape, Dimensions,
  Fab Notes, Route Tool Path, Sheet, V Cut.
- **Component Layer Pair types:** Assembly, Coating, Component Center, Component
  Outline, Courtyard, Designator, Dimensions, Glue Points, Gold Plating, Value,
  3D Body, Die, Wire Bonding.
- **System layers.** IPC-4761 via features add Capping and Filling layers and
  Covering, Plugging and Tenting pairs automatically. Their names and types
  cannot be changed, only their numbers.
- **What a type drives:**
  - *Courtyard:* "also used for component selection, to define the component
    area, and for collision detection when there is no 3D Body included in the
    component."
  - *3D Body:* "Use this layer for the 3D mechanical model of the component."
  - *Assembly:* "can be included in a Draftsman Board Assembly View, and can
    then be selected as the Geometry Source for the components".
  - *Designator and Value:* hold the `.Designator` and `.Comment` special
    strings so the pair "can then be included in assembly drawings".
  - *Component Center:* a visual centroid reference for assembly documentation.
  - *Board Shape:* "Use this layer for the overall board outline". It is a
    drawing of the shape, not the shape itself: the Gerber Board Profile "is
    derived directly from the actual board shape, not objects placed on a
    mechanical layer with a Layer Type of Board Shape." The type is created
    automatically when a Board Region is exploded to free primitives.
  - *Route Tool Path:* "the layer that contains the mechanical routing
    information". A user name is not allowed, because older versions find this
    layer by its name.
  - *Sheet:* the drawing template border.
  - *V Cut, Fab Notes, Assembly Notes, Board, Dimensions, Coating, Glue Points,
    Gold Plating:* described as places to put that information. The page names
    no program behaviour for them.
- **Pairing.** A Component Layer Pair is two layers. Footprint objects go on the
  top layer of the pair, and when the component is flipped "the contents of the
  top layer in the pair are automatically mirrored onto the bottom layer in the
  pair." Individual layer types do not pair. Dimensions exists in both lists.
- **Library to board mapping.** "If a mechanical layer or a Component Layer Pair
  defined in the PCB library has the Layer Type assigned, that mechanical layer
  / layer pair is automatically created on the PCB when a component using those
  layers is placed", or its content is mapped to the existing layer of that
  type. Without a type, "the software will fall back to matching by Layer
  Number", so the user must pre-define the same numbers on the board.
- **Layer structure exchange.** Tools » Export / Import Mechanical Layers moves
  the layer structure, not the content, through a `*.stackup` file (as reported
  from the same page by the fetch tool; I did not read that passage verbatim).
- **DXF import.** The import dialog has a Layer Mappings table: each source
  layer name gets a target PCB layer from a drop-down, grouped by function
  (Signal, Mechanical). "If set to Not Imported, the importer will ignore the
  related source layer." Mappings save to and load from an `*.ini` file
  ([AutoCAD-DXF Import-Export Support](https://www.altium.com/documentation/altium-designer/design-tools-interfacing/autocad-dxf-import-export)).
  The page states no default mapping.
- **DXF export.** The user ticks layers in an Include column. DXF layer names
  are the PCB layer names when the `Exporters.AutoCAD.LayerNames` advanced
  setting is on. Components export as blocks or as primitives (same page).
- **File format.** KiCad's Altium importer reads the Layer Type from the board
  file as a string, with separate top and bottom values for pair types:
  `AssemblyTop`, `AssemblyBottom`, `AssemblyNotes`, `Board`, `CoatingTop`,
  `ComponentCenterTop`, `ComponentOutlineTop`, `CourtyardTop`, `DesignatorTop`,
  `Dimensions`, `DimensionsTop`, `FabNotes`, `GluePointsTop`, `GoldPlatingTop`,
  `ValueTop`, `VCut`, `3DBodyTop`, `RouteToolPath`, `Sheet`, `BoardShape` and
  the `...Bottom` forms
  ([altium_parser_pcb.cpp](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/pcb_io/altium/altium_parser_pcb.cpp),
  master at `0f4d229ea6`). This is a third party's reading of the format, not
  Altium's own specification.

## KiCad

Sources: [PCB Editor manual 9.0](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)
and the source at master `0f4d229ea6`:
[layer_ids.h](https://gitlab.com/kicad/code/kicad/-/blob/master/include/layer_ids.h),
[layer_id.cpp](https://gitlab.com/kicad/code/kicad/-/blob/master/common/layer_id.cpp),
[board.h](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/board.h).

- **Model.** A closed enum, `PCB_LAYER_ID`. The non-copper members are
  `F/B_Mask`, `F/B_SilkS`, `F/B_Adhes`, `F/B_Paste`, `Dwgs_User`, `Cmts_User`,
  `Eco1_User`, `Eco2_User`, `Edge_Cuts`, `Margin`, `F/B_CrtYd`, `F/B_Fab`,
  `Rescue`, and `User_1` to `User_45` (`MAX_USER_DEFINED_LAYERS 45`). The layer
  is the purpose. Layers can be renamed and, except required ones such as
  `F.Cu` and `Edge.Cuts`, disabled.
- **What each drives:**
  - *Edge.Cuts:* "KiCad uses graphical objects on the Edge.Cuts layer to define
    the board outline." Without a valid closed outline "some functions such as
    the 3D viewer and some design rule checks will not be functional", and DRC
    reports "Board has malformed outline".
  - *F.Courtyard / B.Courtyard:* "Closed shapes on a footprint's F.Courtyard and
    B.Courtyard layers will form the footprint's front and back courtyard". DRC
    has "Courtyards overlap", "Footprint has no courtyard defined" (default
    severity Ignore) and "Footprint has malformed courtyard" for non-closed
    shapes. A footprint can be "exempt from courtyard requirement". Custom rules
    can test `intersectsCourtyard()`, `intersectsFrontCourtyard()` and
    `intersectsBackCourtyard()`, and a `courtyard_clearance` constraint sets the
    gap. Courtyards that collide are highlighted while a footprint moves.
  - *F.Fab / B.Fab:* called "fabrication layers" and "nonphysical". Plot options
    act on them by identity: "Indicate DNP on fabrication layers" and "Sketch
    pads on fabrication layers".
  - *User layers (User.Drawings, User.Comments, User.Eco1/2, User.N):* "can't be
    used for routing, but they can contain arbitrary graphics or other
    information." Any of them can be made a *private layer* of a footprint,
    whose objects "will not be shown on the board" or included in outputs.
  - *Margin, Adhesive:* present in the enum and the display-order text. The
    manual passages read do not say what reads them.
- **3D body.** The body is not a layer. The 3D model exporter "creates a 3D
  model file from the PCB and any STEP files specified in footprints".
- **Pairing.** Fixed by `FlipLayer()`: copper, silk, adhesive, mask, paste,
  courtyard and fab swap front and back, and "No change for the other layers".
  User layers have a type, `LT_AUX`, `LT_FRONT` or `LT_BACK` in `LAYER_T`. The
  manual: "By default, user layers are auxiliary layers", and they "can instead
  be set to Off-board, front or Off-board, back". "Adjacent front/back layers
  are treated as paired: if User.2 is defined as a front layer and User.3 is
  defined as a back layer, flipping an object on User.2 will move it to User.3".
  A user layer therefore has a side but no purpose.
- **DXF import.** DXF and SVG import through File → Import → Graphics. Imported
  shapes "have an assigned board layer". The manual text read describes one
  target layer for the import and no per-DXF-layer mapping table (**unverified**
  beyond that passage).
- **Altium import mapping**
  ([altium_pcb.cpp](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/pcb_io/altium/altium_pcb.cpp),
  `HelperFillMechanicalLayerAssignments`). Disabled mechanical layers are not
  imported. Enabled ones are placed by their Layer Type:

  | Altium Layer Type | KiCad layer |
  |---|---|
  | Assembly, Component Outline, Component Center, Designator, Value, Dimensions (pair), top / bottom | `F.Fab` / `B.Fab` |
  | Courtyard top / bottom | `F.CrtYd` / `B.CrtYd` |
  | Glue Points top / bottom | `F.Adhes` / `B.Adhes` |
  | Assembly Notes, Fab Notes | `Cmts.User` |
  | Dimensions (individual) | `Dwgs.User` |
  | Board, Board Shape | `Edge.Cuts`, or `Dwgs.User` when the file already gave a board outline |
  | V Cut | `Edge.Cuts` |

  The other types (Coating, Gold Plating, 3D Body, Route Tool Path, Sheet) have
  no case in that switch. The importer also offers a layer-mapping step, and
  unmatched layers are marked undefined so they are not imported.
- **EAGLE import mapping**
  ([pcb_io_eagle.cpp](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/pcb_io/eagle/pcb_io_eagle.cpp),
  `defaultKicadLayer`): Dimension → `Edge.Cuts` (required); tPlace and tNames →
  silkscreen; tValues and tDocu → `F.Fab`; tStop and tFinish → mask; tCream →
  paste; tGlue → adhesive; Document and Reference → `Cmts.User`.

## Horizon EDA

Sources: source at master `79e3422646`
([board_layers.hpp](https://github.com/horizon-eda/horizon/blob/master/src/board/board_layers.hpp),
[board.hpp](https://github.com/horizon-eda/horizon/blob/master/src/board/board.hpp),
[board.cpp](https://github.com/horizon-eda/horizon/blob/master/src/board/board.cpp)) and the
[Creating a Package](https://docs.horizon-eda.org/en/latest/create-package.html) page.

- **Model.** A closed list: Top Notes, Outline Notes, Outline, Top Courtyard,
  Top Assembly, Top Package, paste, silkscreen, mask, copper, the bottom
  mirrors of each, Bottom Notes, and User 1 to User 8.
- **Package is a separate layer from Assembly and Courtyard.** This is the
  body outline. The docs call its purpose "purely visual", but the source reads
  it:
  - The height restriction check compares polygons on Top Package / Bottom
    Package against restricted areas
    ([board_rules_check_height_restrictions.cpp](https://github.com/horizon-eda/horizon/blob/master/src/board/board_rules_check_height_restrictions.cpp)).
    A height restriction area is itself a polygon on a Package layer, and the 3D
    view draws it at its height
    ([canvas_mesh.cpp](https://github.com/horizon-eda/horizon/blob/master/src/canvas3d/canvas_mesh.cpp)).
  - A package's bounding box is taken from its pads and its Package and Outline
    polygons ([package.cpp](https://github.com/horizon-eda/horizon/blob/master/src/pool/package.cpp)).
- **Courtyard.** "The courtyard denotes the space needed by the part that
  mustn't be occupied by other parts." It is a polygon with parameter class
  `courtyard`, sized by a "Courtyard expansion" parameter (0.25 mm in the docs'
  example). The package check requires that class and rejects more than one
  courtyard polygon per side
  ([package_rules_check.cpp](https://github.com/horizon-eda/horizon/blob/master/src/package/package_rules_check.cpp)).
  A `CLEARANCE_PACKAGE` rule id exists
  ([rule.hpp](https://github.com/horizon-eda/horizon/blob/master/src/rules/rule.hpp));
  which geometry it measures was **not verified**.
- **Assembly.** Holds a polygon outline, optional pins and the `$RD` reference
  designator text. The docs say the assembly drawing is "(not yet
  implemented)". The ODB++ export names these layers `assembly_top` and
  `assembly_bottom`
  ([odb_util.cpp](https://github.com/horizon-eda/horizon/blob/master/src/export_odb/odb_util.cpp)).
- **Outline.** The 3D board mesh is built from the Outline layer
  (canvas_mesh.cpp).
- **Pairing.** Arithmetic on the layer number: `flip_package_layer` leaves
  Outline alone and maps every other non-inner layer to its mirror
  (`layer = -layer - 100`), so Courtyard, Assembly, Package and Notes all swap
  sides (board.cpp).
- **User layers carry a type.** `UserLayer::Type` is one of `DOCUMENTATION`,
  `STIFFENER`, `COVERLAY`, `COVERCOAT`, `BEND_AREA`, `FLEX_AREA`, `RIGID_AREA`,
  `PSA`, `SILVER_MASK`, `CARBON_MASK` (board.hpp). The types name flex and
  fabrication-process layers. None is a courtyard, assembly or 3D purpose. A
  user layer also has a name, a colour and a position in the stack order.

## DipTrace

Sources: [DipTrace complete help](https://diptrace.com/support/tutorials/help_html/)
and the [PCB XML specification](https://diptrace.com/support/tutorials/xml_specs_PCB_html/).

- **Model.** Default non-signal layers are fixed: "The user can not edit or
  delete default non-signal layers." The XML `Layer` enum for shapes lists Top
  and Bottom Assy, Silk, Mask, Paste, Dimension, Courtyard, Outline and
  Terminals, plus Route Keepout, Placement Keepout, Board Cutout, Signal/Plane
  and Non-Signal.
- **Custom non-signal layers** have only a name, a colour and a side. The XML
  `<NonSignal>` element has `Id`, `Side` (None, Top, Bottom) and `Color`. Help:
  "select the layer side (None means that layer is not connected to any side of
  the board)". There is no type or purpose field.
- **Top Outline drives a generated 3D body.** "place a shape in the Top Outline
  layer ... Specify the model height and DipTrace will generate a 3D shape to
  mimic a device." The pick and place export measures "Center X" as "relative to
  the component outline" and has a Package Height column.
- **Assembly is filled by a view command**, not only by library drawing: "You
  can automatically add pattern drawings, pads, pattern silk, borders, texts
  (RefDes, Value, etc.) and board outline to the Assembly layer" through View \
  Add to Assembly.
- **Board outline** is a dedicated `<BoardOutline>` object in the XML, not
  shapes on a layer. Board Cutout is a layer value.
- **Courtyard.** Listed as a layer for shapes and text and given its own line
  width setting. What reads it (a DRC check or the placement tools) was **not
  verified** in the help text searched.
- **DXF import** maps by DXF layer with a "Convert to" choice per layer, for
  example "Convert to: Pads" for one layer and "Convert to: All Holes" for
  another.
- **STEP export** offers a choice of objects and "does not include the copper
  traces".

## Fusion Electronics / EAGLE

Source: [Layer names](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-LAYER-NAME-CPT.htm)
in the Fusion help, as summarised by the fetch tool (table reproduced from that
summary, not read verbatim).

- **Model.** A fixed numbered list grouped into categories, plus user-defined
  layers (`Layer_100`) and legacy layers that keep their old names.
  - *Mechanical:* BoardOutline, TestTop/Bottom, Drills, Holes, Milling.
  - *Silkscreen:* SilkscreenTop/Bottom, NamesTop/Bottom, ValuesTop/Bottom.
  - *Mask Openings:* SolderMask, Stencil, Finish, Glue, each top and bottom.
  - *Exclusions:* ComponentExcludeTop/Bottom, RestrictCopper, RestrictVias.
  - *Documentation:* Measures, Document, Reference, DocumentTop/Bottom.
- **The category is a label for the list.** The purpose is still the layer.
- **No courtyard or assembly layer by that name.** ComponentExcludeTop/Bottom
  ("areas restricted for components", EAGLE's tKeepout/bKeepout) does the
  courtyard's job, and DocumentTop/Bottom ("detailed top-side screen print",
  EAGLE's tDocu/bDocu) does the assembly layer's job. The old-to-new name
  mapping is **unverified**: it rests on KiCad's EAGLE importer names above and
  on search-result snippets of an Autodesk blog that returned HTTP 403.
- **Board outline for 3D.** An Autodesk support article, seen only as a search
  snippet, gives an unclosed outline on layer 20 as the cause of a missing board
  in the 3D PCB (**unverified**: the article returned HTTP 403).
- **Pairing.** Top and bottom layers are separate fixed entries. Measures,
  Document, Reference, BoardOutline and Milling are unpaired.

## EasyEDA Pro

Source: [Layer Manager](https://prodocs.easyeda.com/en/pcb/tools-layer-manager/).

- **Model.** A fixed list plus up to 30 custom layers.
- **Fixed non-copper layers and their stated use:**
  - *Board outline:* "the board factory will produce board according to this
    shape", output as the GKO Gerber.
  - *Top / bottom assembly layer:* "The simplified outline of components is used
    for product assembly and maintenance. It is used to export document
    printing".
  - *Mechanical:* "only used for information recording", with a caveat that a
    closed shape on it may be taken as the board shape in production, called
    "the historical impact of AD files".
  - *Document:* "Similar to the mechanical layer ... generated in the Gerber
    file and does not participate in manufacturing."
  - *Component Shape:* the real component's shape, for "comparison with the size
    and physical size".
  - *Component Marking:* polarity and similar marks.
  - *Pin Soldering* and *Pin Floating:* the real pin's soldered and free parts,
    to compare with the pad.
  - *3D Shell Outline, 3D Shell Top, 3D Shell Bottom:* geometry for the
    enclosure ("3D shell") feature.
  - *Drill Drawing:* the drill table.
- **Four layers describe the physical part, separate from assembly.** Component
  Shape, Component Marking, Pin Soldering and Pin Floating are not paired top
  and bottom in the list. Whether the 3D view or any check reads them was **not
  verified**.
- **Custom layers** are "generally used for additional information recording",
  similar to the document, mechanical and assembly layers. They are renamable,
  have no type, and are left out of Gerber unless picked in a custom export.
- **Hidden layers still export:** hiding is "only visually hidden"
  ([Right Panel - Layers](https://prodocs.easyeda.com/en/pcb/side-panel-right-layer/)).

## OrCAD X / Allegro X

Cadence's own reference documentation sits behind a login, and its public
resource pages returned no body text. This section rests on pages by EMA Design
Automation (a Cadence channel partner) and on search-result snippets of Cadence
community threads, so it is the weakest in this note.

- **Model.** Every object belongs to a class and a subclass. The DXF importer
  shows this directly: each DXF layer is mapped to a Class and Subclass, for
  example the cutout to Board Geometry / Cutout and the outline to
  Design_Outline
  ([EMA, board outline from a DXF, OrCAD X Presto 24.1](https://www.ema-eda.com/how-to-page/how-to-create-a-board-outline-from-a-dxf-in-orcad-x-presto/)).
  DXF keepin and keepout geometry "can be mapped to the corresponding layers
  during import". Drawing an outline adds keepins automatically.
- **DFA_Bound_Top** is a shape each footprint needs, with a `DFA_DEV_CLASS`
  property, for the package-to-package spacing table
  ([EMA, DFA spacing constraints, 25.1](https://www.ema-eda.com/how-to-page/how-to-assign-dfa-spacing-constraints/)).
- **Place_Bound_Top** is used by 3D model mapping: the AUTO option aligns a STEP
  model "using the existing placebound geometry"
  ([EMA, PCB Footprints and 3D Models](https://www.ema-eda.com/courses/orcad-x-presto-walk-through/lessons/pcb-footprints-and-3d-models/)).
- **Unverified, from community snippets only:** Place_Bound_Top is the
  placement boundary that flags overlapping components; a package height
  property (`PACKAGE_HEIGHT_MAX`, `PACKAGE_HEIGHT_MIN`) sits on the Place_Bound
  shape, and several shapes with different heights are allowed; with no STEP
  model mapped, 3D uses the Place_Bound shape and its height, or a default
  symbol height; DFA checks fall back to Place_Bound_Top when DFA_Bound_Top is
  missing; Assembly_Top and Silkscreen_Top graphics move to the `_Bottom`
  subclass when a part is mirrored. The full subclass list was not read.

## Where they differ

| | Purpose is carried by | Open or closed list | Top/bottom pairing | Body outline separate from courtyard | Board outline |
|---|---|---|---|---|---|
| Altium Designer | Layer Type tag on a generic mechanical layer | open layers, closed type list (8 single, 13 pair) | Component Layer Pair, an explicit two-layer object | yes: Component Outline and 3D Body types | a board object; the Board Shape type is only a drawing of it |
| KiCad | the layer itself | closed, plus 45 user layers with a side only | fixed table; user layers pair when adjacent and set front and back | no body layer; Fab holds the outline, 3D is a model file | graphics on Edge.Cuts |
| Horizon EDA | the layer itself | closed, plus 8 user layers with a flex/process type | layer-number mirror | yes: Package layer | graphics on Outline |
| DipTrace | the layer itself | closed, plus custom layers with a side only | separate Top and Bottom layers; custom layers by Side | yes: Outline layer | a board outline object |
| Fusion / EAGLE | the layer itself, grouped in categories | closed, plus user layers | separate Top and Bottom layers | no (per the layer table) | graphics on BoardOutline |
| EasyEDA Pro | the layer itself | closed, plus 30 custom layers | separate top and bottom for assembly and 3D shell | yes: Component Shape | Board outline layer |
| OrCAD X | class / subclass name | not verified | `_Top` / `_Bottom` subclasses | Place_Bound with a height property (unverified) | Board Geometry / Design_Outline |

Further differences:

- **Courtyard and selection.** Altium uses the Courtyard shape for selection,
  component area and collision when there is no 3D Body. KiCad uses it for DRC
  and move-time collision highlighting. OrCAD X splits the job between
  Place_Bound and DFA_Bound. Fusion uses a component-exclusion layer.
- **Which purposes the program reads.** In every tool where it could be
  checked: board outline, courtyard or placement bound, and (Altium, Horizon,
  DipTrace, OrCAD X) a body shape with a height. Assembly layers are read by
  drawing and plot features (Altium Draftsman, KiCad plot options, DipTrace Add
  to Assembly, ODB++ naming in Horizon). Notes, dimensions, glue, coating and
  gold plating are labels only in the sources read.
- **Height.** Altium puts height on the 3D Body object, OrCAD X on the
  Place_Bound shape (unverified), DipTrace on the model generated from the
  Outline shape, Horizon on the package model and on height restriction areas
  drawn on the Package layer. KiCad has no height on a layer.
- **Untyped layers.** Altium falls back to layer numbers, which the page itself
  warns needs pre-defined matching numbers. KiCad, DipTrace and EasyEDA custom
  layers never gain a purpose.
- **Private layers.** Only KiCad lets a footprint mark a user layer private, so
  its content never appears on the board.
- **Import mapping.** Altium and DipTrace DXF import, OrCAD X DXF import and
  KiCad's Altium import all put a mapping table in front of the user, keyed by
  source layer name. KiCad's Altium importer pre-fills it from the Layer Type,
  and collapses six Altium pair types onto the one Fab layer. Altium's DXF page
  states no default mapping.
- **Default visibility** per purpose was not documented in any source read.

## Not verified

- **Altium:** default mechanical layers in a new board; which layers STEP export
  includes; default visibility; the release that introduced Layer Type. The
  `*.stackup` structure exchange and the 3D-view notes come from a fetch-tool
  summary of the page, not a passage read verbatim.
- **KiCad:** what reads the Margin and Adhesive layers; whether DXF import can
  split DXF layers across board layers; default visibility. The KiCad Library
  Convention pages (F5.2, F5.3) returned HTTP 403.
- **Horizon EDA:** which geometry the `CLEARANCE_PACKAGE` rule measures; how DXF
  import picks a layer; whether STEP export reads any layer besides Outline.
- **DipTrace:** what reads the Courtyard layer; default visibility.
- **Fusion / EAGLE:** the whole layer table is from a fetch-tool summary of the
  help page; the old EAGLE names and numbers, the 3D PCB's use of layer 20, and
  flip behaviour were not read from Autodesk (blog and support pages returned
  HTTP 403).
- **EasyEDA Pro:** whether Component Shape or the Pin layers feed 3D or checks;
  how imported files map onto layers; flip behaviour.
- **OrCAD X:** everything listed as unverified in its section, the full class
  and subclass list, and STEP and 3D behaviour. Cadence's reference manuals were
  not reachable.
