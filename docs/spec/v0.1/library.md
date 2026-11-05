# v0.1 libraries and parts

Sources: [Where does the v0.1 parts library come from, and what does a library part contain?](https://github.com/openreda/reda/issues/13), [ADR-0008](../../adr/0008-parts-bind-symbols-and-footprints.md), [vendor download research](https://github.com/openreda/reda/issues/19), [KiCad format research](https://github.com/openreda/reda/issues/3), [How deep do the Library editors, generators and checks go?](https://github.com/openreda/reda/issues/135). Crates: `reda-library` (scanning, generators, Library check, Commands) and `reda-import-kicad` (Import part). The editors are in `reda-ui`.

## Where parts come from

- A **starter Library** authored by REDA. It is MIT, lives in this monorepo, ships with the app and is read-only.
- **Library editors** in the app.
- **Footprint generators** and the **Symbol Wizard**, which are Commands.
- **Import part**: a one-shot Command that turns one KiCad symbol + footprint pair, the format vendor portals provide, into REDA elements in a chosen Library.

Not in v0.1: full KiCad project or library import (v0.6), direct EasyEDA/LCSC import, supplier search, and 3D models.

## Elements

- **Symbol:**
  - A Symbol has Units, and each Unit has Pins.
  - Each Pin has a name, a number and an electrical type. A Pin name marks an overbar with a [backslash after each character](schematic.md#name-syntax).
  - The Symbol also holds the Designator prefix.
  - See [Pins](#pins) for the rest of a Pin.
- **Footprint:**
  - A Footprint has Pads, each with a number.
  - Numbers may repeat. Pads that share a number belong to the same Pin. A Pin or Pad number is at most 20 characters.
  - See [Footprint contents](#footprint-contents) for the rest of a Footprint.
- **Part:**
  - A Part references one Symbol and an ordered list of alternate Footprints. The first Footprint is the default.
  - It holds the default fields, the Component Type (Standard, Mechanical or Net Tie), and the defaults for In BOM and In CPL.
  - Parts may be generic ("Resistor") or specific (with an MPN). The shape is the same, and only the fields differ.
- **Pin-pad map:**
  - The map is implicit: a Pin connects to the Pads with the same number.
  - The Part records only overrides: a Pin mapped to another Pad, a Pin mapped to several Pads, or a Pad left unconnected.
- **Pin electrical types:** Input, Output, Bidirectional, Tri-state, Passive, Power In, Power Out, Open Collector, Open Emitter, Not Connected.
- **Fields:**
  - Built-in: Value, Description, Datasheet, Manufacturer, MPN, Keywords.
  - Free-form fields with any name, such as `LCSC` or `DigiKey`.
  - Every field is a default that is copied onto each Component, where it can be edited.
  - `show_name` on a field draws it as `Name=Value`.
  - **Links:** a field whose value starts with `http://` or `https://` appears in an **Open ▸** submenu on the Component and the Library panel row. No `kind` is stored, because the value already says what it is. The leading suite has separate Link rows.
  - **Datasheet** takes a URL, or a path relative to the Library or the Project. Dragging a file or URL onto the field sets it, and Open Datasheet opens it.
- **Parts without a Symbol or Footprint:**
  - Mechanical Parts need a Symbol or a Footprint, or both. One with no Symbol is placed directly on the PCB (mounting holes, fiducials). One with no Footprint stays on the schematic (a screw, for the BOM).
  - Standard and Net Tie Parts need a Symbol and at least one Footprint.
- **Component Type and its defaults:**

  | Type | In BOM | In CPL |
  |---|---|---|
  | Standard | true | true |
  | Mechanical | true | false |
  | Net Tie | false | false |

  - In BOM and In CPL are stored explicitly on the Part and copied onto each Component, where they can be edited. Loading a file never infers them from the type.
  - Changing the type in the Part editor resets both to the new type's defaults.
- **Net Tie Footprint:** two or more SMD Pads with at least two different numbers, which touch or overlap so they form one connected copper group on one Layer. v0.1 has no other tie copper. A narrow neck means narrow Pads. Footprint copper graphics may join this group in v0.6 ([checks](checks.md#net-ties)).

## In the Project

- When a Component is placed, the sheet's part table stores the whole Part: the Symbol, the pin-map overrides, the fields, the Component Type, In BOM, In CPL, and a copy of **every** alternate Footprint, deduplicated by hash. A Footprint's [`generator` block](#regenerating) stays in the Library and isn't copied or hashed. The PCB stores only the Footprints in use ([architecture](architecture.md#file-format)).
- Library copies on a sheet are never edited in place. A Pad of a Footprint placed on the PCB can be edited, which gives that instance a [locally edited Footprint](pcb.md#placed-pad-edits).
- **Edit in Library** on a placed Component opens its element in the matching editor. A Starter Library element opens read-only. On a Component whose status is Source Missing, it offers [Extract to Library](#extract-to-library).

## Library status

Source: [What exactly do Update from Library and Replace Part change?](https://github.com/openreda/reda/issues/134).

`schematic.component.library_status {components?}` compares each placed Component's part-table entry with its Part in the registered Libraries. It gives one of three states:

- **Up to Date:** the content hashes match.
- **Library Differs**, naming the aspects: Symbol, Fields or Footprints.
- **Source Missing:** no registered Library holds the Part, or a Symbol or Footprint it references.

The hash of each aspect is computed from the copy in the part table, so no source hashes are stored, even after a [partial update](#aspects). A partial update stays Library Differs for the aspects not taken.

A content hash can't say which version is older, so there is no "Library newer" or "Library older". The leading suite's update from file-based libraries also only compares.

**Where it shows:**
- a badge in Properties, with **Update…**
- the Library panel row ("3 placed, 1 differs")
- the Update from Library dialog, where a Source Missing row can't be updated, as in the leading suite
- the ERC check [`library_out_of_date`](checks.md#erc-checks), Off by default, which gives canvas markers for Library Differs

## Update from Library

It never updates automatically, and nothing changes without the preview.

- **Commands:**
  - `schematic.component.update_preview` changes nothing, and returns the changes with a hash.
  - `schematic.component.update_from_library {preview_hash, excluded_components?, excluded_cells?, ...}` applies them. It fails with `conflict` if the result no longer matches the hash, like `sync.apply`.
  - Anything that reads the PCB, such as lost local edits, runs in `reda-engine`.
- **Scope:** the sheets and Parts ticked on page 1. A Component on several sheets is updated or excluded as a whole, and its Units stay equal, so ticking a sheet includes every Component with a Unit on it.
- **Undo:** one entry per sheet.
- **After Apply:** Footprint changes and per-Component data reach the PCB through [Sync](sync.md), which is never run by the update.

### Aspects

Each Component takes **Full Replace** or any of three aspects, as in the leading suite:

| Aspect | Holds |
|---|---|
| Symbol | graphics, Units and Pins |
| Fields | the Part's name, default fields, Component Type, In BOM and In CPL |
| Footprints | the alternates and their order, their geometry, and the pin-map overrides |

- The pin map sits with Footprints, because it only means something against them.
- If a mix leaves a Pin with no Pad, the preview shows a Problem row.
- **Full Replace** ticks all three. The field grid stays available, and edited and locked cells keep their values unless overridden. The leading suite disables its grid here and overwrites them. REDA keeps it, so that the default action never overwrites a value unseen.

### Field cells

Each field, Component Type, In BOM and In CPL gets one cell with the old default (the part-table copy), the new default (the Library) and the Component's value. A built-in field that is empty counts as not set. The three-way compare only sets how a cell starts, and any cell but a locked one can be overridden.

| Case | Cell | Starts as |
|---|---|---|
| Default changed, Component holds the old default | Changed | Update |
| Default changed, Component already holds the new one | Same | no change |
| Default changed, Component holds another value | Edited | Reject, from `locally_edited` |
| Field new in the Library, Component has no value | Added | from `library_field_missing` |
| Field gone from the Library, Component holds the old default | Removed | Update, which removes it |
| Field gone from the Library, Component holds another value | Edited | from `component_field_missing` |
| A field only the Component has | Component only | from `component_field_missing` |
| Field on the Component's `locked_fields` | Locked | no change, and no override until unlocked |

- **Accepting or rejecting moves the baseline.** With the Fields aspect ticked, the entry takes the new defaults whichever way each cell went. A rejected value then reads as edited and is no longer flagged.
- **Differs from the leading suite:** it compares only the library value with the sheet value, so a changed cell updates unless it is locked or rejected. REDA starts an edited value as rejected, and `locally_edited: take_new` gives the suite's behaviour. Generic Parts routinely carry edited Values, and the part table already knows the old default, so nobody has to lock every placed resistor first.
- **Also differs:** a field gone from the Library that still holds the old default starts as removed, whatever the policy says. The suite has one setting for all of these. REDA can tell a stale default from a user's value, and a stale supplier number would otherwise stay on the BOM.
- **Locks:** a [Locked Field](schematic.md#components) blocks updates only, and can still be edited by hand. The suite has two controls that keep a parameter out of updates. REDA has one, because the second one's other use, its Comment field, has no counterpart here.
- **Not Fitted** is never updated, except that a Part becoming Net Tie clears it in a visible row, because a Net Tie refuses Not Fitted.
- **Field positions and visibility** are kept unless the settings below say otherwise. A new field starts hidden.

### What the preview shows

Per Component, per aspect:
- **Symbol:** whether the graphics changed.
- **Pins:** added, removed, renamed, retyped and moved, with the Nets each Pin would leave or join.
- **Units:** Units are matched by UUID. A placed Unit gone from the Symbol blocks that Component until the user deletes the Unit, and the preview says why. New Units appear as unplaced.
- **Footprints:** alternates added or removed, which Pads moved or resized, Pad numbers added or removed, the pin map old and new, and the Footprint choice.
- **Fields:** the [cells](#field-cells).
- **Local Pad edits this removes:** see [Placed Pad edits](pcb.md#placed-pad-edits).

**Wires never move,** in an update or a Replace, because a Wire connects where its end touches a Pin. Rerouting Wires automatically could join Nets with nobody seeing it. The preview lists what changes and ERC catches the rest.

### Settings

Page 1's choices are saved in `.reda` and shown read-only on [Project Settings ▸ Schematic](app.md#projects-and-settings), with Reset, as in the leading suite. Page 2's choices aren't saved. Command parameters override the saved values, so an agent can be explicit.

| Setting | Values, default first |
|---|---|
| `library_field_missing` (in the Library, not on the Component) | `add`, `add_if_not_blank`, `skip` |
| `component_field_missing` (on the Component, not in the Library) | `keep`, `remove` |
| `locally_edited` | `keep`, `take_new` |
| `fields_to_update` | every field, as a checklist |
| `add_new_footprints` | on |
| `remove_missing_footprints` | on |
| `footprint_choice` | `keep_if_present`, `default` |
| `preserve_field_locations` | on |
| `preserve_field_visibility` | on |

- Turning off `preserve_field_locations` returns moved fields to Autoposition. Turning off `preserve_field_visibility` returns them to the Designator and value.
- The leading suite's documents don't state most of its defaults, so these are REDA's own.
- **Left out:** the suite's designator and part ID lock options, because Parts carry no such data. Also its variant and latest-revision options, because v0.1 has neither.

### Dialog

A modal dialog with two pages, as in the leading suite.

- **Page 1:** the sheets with All On and All Off, the Parts with instance counts, Selected Parts Only, the aspects and the settings.
- **Page 2:** a row per Component with its status and aspect ticks, Choose Component and Return Selected to Default, and the sections of the preview with the field grid. Update Selected and Reject Selected work on cells.

There is no change order after page 2, unlike the suite, because page 2 already shows and excludes every change.

**Entry points:**
- Design ▸ Update from Library… for the whole Project.
- Component ▸ Update from Library… on a selection, which sets Selected Parts Only.
- The Properties badge and the Library panel row.
- The Library editors' Update in Project…, filtered to that element and saving it first. The suite's push from its editors skips any preview. REDA's doesn't, because updates always preview.

## Replace Part

`schematic.component.replace_part {components, part, keep_fields, footprint}` with `schematic.component.replace_preview` is the update compare with the chosen Part as the new one, for every aspect.

- **Opens from** the Part row in Properties, on one Component or a multi-selection, where all take the chosen Part, and from Choose Component on the update dialog's page 2, with Return Selected to Default to undo it there.
- **Dialog:** the Library panel's picker, with the current Part on top. For a generic Part, its values seed the search. The pin differences show under the picker.
- **Always kept:** the Component UUID, Designator, Not Fitted, Unit positions and field positions. The UUID means Sync sees a Footprint change, not a removal and an addition.
- **`keep_fields`:** `edited` (default), `all` or `none`. `edited` keeps values that differ from the old Part's defaults, and the rest take the new Part's.
- **`footprint`:** by default the same Footprint by UUID, then the same name, then the new default.
- **Component Type** is always the new Part's, because the Part is what is being changed. In BOM and In CPL follow `keep_fields`.
- **Units** are matched by index. A placed Unit with no match blocks that Component until the user deletes it. New Units appear as unplaced.
- **Pins** follow the rule that [Wires never move](#what-the-preview-shows).
- **Refused:** a Part with no Symbol.
- The Designator prefix stays even if it no longer matches the Part, and Re-annotate can fix it.

The leading suite's documents say that replacing keeps connectivity, and don't list what else carries over, so the kept list and the options are REDA's own. REDA has no Replace from a BOM document, as the suite has, because v0.1 has no BOM document.

## Extract to Library

`library.extract_from_project {components, library}` writes a Component's Part, Symbol and Footprints from the part table to a chosen writable Library, **keeping their UUIDs**, so the status becomes Up to Date. Elements that Library already holds are skipped and listed in the result. Edit in Library offers it on Source Missing, and Choose Component stays available.

Leaving out `components` extracts the whole Project, which the Library panel's New… menu offers as **Make Project Library**, as in the leading suite. A `generator` block isn't in the part table, so an extracted Footprint has none and offers no Regenerate.

## Where used

`library.where_used {element}` returns the Parts that use a Symbol or Footprint across the registered Libraries, and for any element the Components that use it in the open Project. It scans the part tables. It backs the Library panel's Where Used, "Used in Project: N" and the [shared-element dialog](#shared-elements).

## On disk

- A Library is a folder holding:
  - `<name>.redalib` (name, UUID, `format`, description)
  - `symbols/*.redasym`, `footprints/*.redafp` and `parts/*.redapart`, one canonical-JSON file per element
- Elements are found by scanning the folder. There is no manifest. Elements reference each other by UUID, across Libraries too.
- **Registered Libraries:**
  - the starter Library
  - user Libraries, listed in Preferences (Libraries page)
  - Project Libraries, listed in the `.reda` file by relative path
- **Active:** Preferences ▸ Libraries has an Active checkbox for each user Library and the Starter Library. Project Libraries have none, because they belong to the Project. An inactive Library isn't scanned or searched, so a Component placed from it reads Source Missing. Nothing on a sheet changes.
- **Resolution order:** Project, then user, then Starter. If two Libraries hold the same UUID, the first in that order wins and a scan problem is reported, so a Project Library can deliberately hold its own copy.
- **Names:** saving refuses an element name already used in that Library.
- **Copying:** `library.element.copy {element, library}` gives a new UUID, adding `_1`, `_2`… to a name already taken. A copied Part still references its original Symbol and Footprints, because Parts can reference across Libraries.
- **New Library** (`library.create`) writes the `.redalib` into a new or empty folder. It is offered in the Library panel's New… menu and on the Project Settings Libraries page. A user Library is added to Preferences at once; a Project Library is then registered with `project.library.add`, which is undoable.

## Library panel

- **On a sheet** it lists Parts that have a Symbol. **On the PCB** it lists Mechanical Parts. **In a Library editor tab** it lists the Symbols, Footprints and Parts of that element's Library.
- **Search:**
  - A substring search over name, Description, Keywords, Value, MPN and field values, with filters for Library and Component Type.
  - `field:value` tokens such as `mpn:` and `lcsc:`, and `*` wildcards. A token naming a field a Part doesn't have doesn't match that Part.
  - Unit-aware values come in v0.2.
- **Preview** shows the Symbol (Units as tabs) and the Footprint, with a picker for the alternates, a datasheet link and "Used in Project: N".
- **Placing:**
  - Click, double-click or Enter starts placement, which repeats subject to the repeat placement Preference. A drag places exactly one.
  - On a multi-Unit Part, the previewed Unit goes first and repeats walk the unplaced Units. The leading suite's documents don't confirm this, so it is REDA's own choice.
- **Menu:** Place, Edit, Duplicate, Delete, Where Used, Update in Project…, Check, Copy to Library… and Open Datasheet. "New…" opens the editors.

## Editors

Symbol, Footprint and Part editors open as Document tabs in the same window. Each has its own undo stack. They take Recovery Snapshots ([app](app.md#crash-recovery)). Starter Library elements are read-only.

Deferred: De Morgan body styles, Pins that swap between Units, bus Pins, custom polygon Pads, custom mask and paste shapes, 3D models, Pad stacks that differ per Layer, and Graphical Parts. A table of Pads and a list of primitives come in v0.2.

### Symbol editor

- multiple Units
- Pins on the 50 mil grid, always drawn above body graphics
- lines, rectangles, arcs, circles, polygons and text
- **Active Bar:** Pin, Line, Rectangle, Arc, Polygon and Text, plus Unit switching.
- **Origin:** Set Origin (Pin 1, Centre or Pick) with `library.symbol.origin.set`, and Jump to Origin.
- **Designator and Value** show where Autoposition puts them. Nothing is stored on the Symbol. The leading suite stores default positions, but [fields keep Autoposition until moved](schematic.md#fields), and a second source would have to be reconciled on every update.

### Pins

- A Pin has `length` and `description`, as well as its name, number, type, position, and name and number visibility. Description shows on hover.
- **Decorations** have four positions, each optional:

  | Position | v0.1 glyphs |
  |---|---|
  | `inside_edge` | `clock` |
  | `outside_edge` | `dot`, `active_low_input`, `active_low_output` |
  | `inside`, `outside` | [reserved](architecture.md#reserved-names) for v0.6 |

  The leading suite fills all four. Its other glyphs are its IEEE logic set, which v0.6 takes up.
- There are no hidden Pins, and no per-Pin fonts or margins, because fixed renderer rules keep printouts the same for every user.

### Placing Pins and Pads

- **Auto-increment:** Pin and Pad placement follows the [schematic's rules](schematic.md#auto-increment). Numbers use `primary` and Pin names use `secondary`.
- **Letters:** a Pin or Pad number made only of letters also steps, A → B → Z → AA, as in the leading suite. Names and Designators keep the trailing-number rule.
- **Paste Array:** a dialog in the Symbol and Footprint editors, for Pins and Pads, with count, number step, name step, dx and dy, and Remove Leading Zeroes. It runs a `batch` of the add Commands, as one undo step. Sheets get a paste array in v0.2.

### Symbol Wizard

`library.symbol.generate {layout, pins, target}`, opened from the Symbol editor's Tools menu and the Part editor's Symbol ▸ New.

- **Layouts:** DIL, quad, connector, zig-zag, single-row and manual. A rectangular body puts Pin 1 at the origin ([style](../../library-style.md)).
- **Target:** a new Symbol or a new Unit of this one.
- **Dialog:** modal, with a pin grid (number, name, type, side, group) and a live preview.
- **Reopening** reads the existing Pins, and Pins keep their UUIDs by number. No parameters are stored, because the Pins are the wizard's whole input and a second copy could disagree with them.
- **Column-mapping paste** is a sub-dialog with delimiters, a heading per column, automatic header detection, and Remove Row and Remove Column. It also serves paste into the Pins Panel.
- **Type aliases,** in paste and in Import part, ignoring case:

  | Text | Type |
  |---|---|
  | `I/O`, `IO`, `BIDIR` | Bidirectional |
  | `HIZ`, `3S` | Tri-state |
  | `PWR`, `POWER` | Power In |
  | `OC` | Open Collector |
  | `NC`, `N/C` | Not Connected |

### Footprint contents

- **Pads:** every property in the [Pad model](#pad-model).
- **Graphics** on the Silkscreen, **Courtyard** and **Assembly** Layers, top and bottom (Courtyard and Assembly join the fixed v0.1 Layer set).
- **`height`:** the part's greatest height. Generators fill it in. It also seeds later 3D bodies and rules.
- **`texts`** replaces `designator` and `value`. It is a list of `{field, at, rotation, size, layer, visible}` on the Silkscreen or Assembly Layers, where `field` is `Designator`, `Value` or any field name. A Footprint can hold both a Silkscreen Designator and an Assembly Designator. A field a Component doesn't have draws nothing. `library.footprint.text.add`, `.edit` and `.remove` replace `.set`.
- **Courtyard:**
  - Courtyard Layers take only closed shapes: Rect, Circle or Polygon. Several shapes on one side union. The shape's edge is on the centreline of its stroke.
  - The leading suite allows loose lines and checks them. REDA makes an open courtyard impossible, because one that fails DRC's overlap check helps nobody. So there is no `courtyard_not_closed` check.
  - Clicking inside a courtyard selects the Footprint on the PCB, as in the leading suite. [How do selection, snapping, grids and Properties work across editors?](https://github.com/openreda/reda/issues/139) settles how this ranks against other hits.
- **Origin:** Set Origin offers Pin 1, Centre or Pick. Every Footprint's origin belongs at the body centroid, which the CPL relies on ([style](../../library-style.md)). The zero orientation follows IPC-7351.

### Footprint editor

- **Active Bar:** SMD Pad, through-hole Pad, Line, Arc, Polygon, Courtyard, Text, Origin and Measure.
- **Mask and paste preview:** a Library element has no board, so the editor draws openings from the Generic Fab Preset's seed values.

### Part editor

One scrolling form with four regions: **Part** (name, Description, Component Type, In BOM, In CPL, Keywords), **Symbol** (a preview tile with Pick, New, From Wizard, Edit and Duplicate & Edit), **Footprints** (ordered tiles, the first being the default, with Add ▸ Pick, Generate or New, and Edit, Duplicate & Edit and Remove) and **Fields** (a grid with built-ins first). The pin map is in the Pins Panel.

### Pins Panel

A dockable Panel that follows the active editor, replacing the pin table and the pin-map section that are inline today. The leading suite's bulk pin editor is modal. A Panel stays visible while you draw, which is where this table gets used.

- **In the Symbol editor:** number, name, type, Unit, side, length, description, decorations, name visible and number visible.
- **In the Part editor,** also one column per alternate Footprint holding comma-separated Pad numbers. An empty cell, an unknown Pad and a Pad on two Pins are marked red.
- **Controls:** multi-column sort, multi-cell edit, [paste](#symbol-wizard) and Jump to Pin.
- **Cross-probe:** hovering a row highlights the Pin and Pad in the Part editor's previews, and hovering a Pad highlights its row.

### Shared elements

The first change to a Symbol or Footprint that two or more Parts use opens a modal dialog listing those Parts, with **Edit Shared**, **Make a Copy** and **Cancel**. Opening an element to look at it raises nothing.

**Make a Copy** gives a new UUID and a `_1` name suffix. From a Part editor, that Part is repointed to the copy. The Part editor's Duplicate & Edit does the same without asking.

## Pad model

Source: [How do mask, paste and tenting inherit, and what can a Pad express?](https://github.com/openreda/reda/issues/126). Footprint Pads and Free Pads share all of this. Where a value inherits from is in [pcb](pcb.md#mask-and-paste).

- **Kind:** SMD, plated through-hole or non-plated.
- **Side:** an SMD Pad has `side`, `top` by default and `bottom` for card-edge connectors and double-sided Pads. It is in the Footprint's frame, swaps when the Footprint is flipped, and sets the Pad's copper, mask and paste Layers. A plated Pad has copper on every Layer. A Free Pad is never SMD.
- **Shape:**
  - `circle`, `rect` or `oval`.
  - `rounded_rect`, with `corner_ratio` and optional `corners`.
  - `chamfered_rect`, with `chamfer_ratio`, optional `corners` and optional `corner_ratio`. An octagon is a chamfered rectangle.
  - A ratio is a percentage of half the shorter side, so 100% is fully round. It is stored as an integer in thousandths of a percent, and a resized Pad keeps its look. The entry field also accepts a length, which it converts at the Pad's current size.
  - `corners` lists which of `top_left`, `top_right`, `bottom_left` and `bottom_right` apply, in the Pad's frame. Left out, it means all four.
  - On a chamfered rectangle, `corners` are chamfered, and `corner_ratio` rounds the others.
- **Position:**
  - `at` is the Pad's anchor. A hole sits there, Tracks connect there and the Pad rotates about it.
  - `offset` (x, y, in the Pad's frame) moves the copper away from `at`. Any Pad kind can have one.
- **Hole:** `{width, height, rotation?, tolerance?}`.
  - Sizes are the finished diameter.
  - Equal width and height give a round hole, and otherwise a slot along the longer side.
  - `rotation` turns a slot in the Pad's frame.
  - `tolerance` is `{plus, minus}`. It is stored and shown, and has no effect on geometry.
- **Solder mask:** `mask {top?, bottom?, from_hole_edge?}`.
  - Each side is left out to inherit, `{expansion}` or `tent`. A positive expansion grows the opening and a negative one shrinks it.
  - `top` and `bottom` are in the Footprint's frame and swap when it is flipped. On a Free Pad they are board sides.
  - `from_hole_edge` measures the expansion from the hole, and the opening takes the hole's shape. An expansion of 0 then covers the land up to the hole.
  - An SMD Pad refuses the side it isn't on, and `from_hole_edge`.
- **Solder paste:** `paste`, SMD Pads only.
  - Left out, it inherits. Otherwise it is `{expansion}`, `{percent}`, `none` or `{windows}`.
  - `percent` is the change in area: 0 matches the copper and -10 has 10% less area. The copper's shape is scaled about its centre, and the value must be above -100.
  - `windows` is a list of `{at, width, height, corner_ratio?}` in the Pad's frame. Together they replace the opening.
- **Zone connection:** `zone_connection` is `{style?, spokes?, spoke_width?, gap?, angle?}`, with the values of the [`zone_connection` rule](pcb.md#zone-connections). Each value left out inherits from the rule, by the Pad's kind and Net. Custom spoke locations are out, as a niche feature.

## Footprint generators

Sources: [How deep do the Library editors, generators and checks go?](https://github.com/openreda/reda/issues/135), [What values do the generators and current estimate need from the IPC standards?](https://github.com/openreda/reda/issues/143). IPC-7351B is paywalled, so its values here come from secondary sources, mainly two open-source transcriptions.

Each family is its own Command, `library.footprint.generate.<family>`, which keeps each schema exact for agents. All of them share the sections below.

**Families:**
- 2-terminal chips, 0201–2512 or from a datasheet's dimensions
- molded 2-terminal parts: SOD-123, SOD-323, SMA, SMB and SMC
- SOT-23-3/5/6 and SOT-223
- SOIC, SSOP and TSSOP, with an optional exposed pad
- QFN and DFN with an exposed pad
- QFP and DIP
- 1×N and 2×N headers and sockets at 2.54 and 1.27 mm

SOT-89, DPAK, MELF, PLCC, axial and radial parts, TO-92, TO-220 and generating from a CSV come in v0.2. BGA comes in v0.6. SOT-23 pin orders are [Part pin-map overrides](#elements), not a generator option.

### Parameters

| Section | Holds |
|---|---|
| Target | `library`, and an optional `name` and `description`. Both default to suggested values. |
| Package | Datasheet ranges as `{min, max}`, plus pitch, pin count, greatest height and least standoff, as each family needs. `pin1_location`, `absent_pins`, a chamfered body corner and a polarity mark where they apply. Standard sizes such as `size: "0603"` are shorthand that resolves to these. |
| `goals` | `density` (`nominal`, `most` or `least`), and `fillet {toe, heel, side}`. |
| `tolerances` | `fabrication`, `placement`, `courtyard_excess`, `round_off {size, place}`, and component tolerances, calculated from the ranges. |
| `pads` | `shape` (`rounded_rect` or `rect`), `corner_ratio`, Pad size and spacing, `paste_coverage`, and the paste array's size. |
| `outputs` | On or off and line width for Silkscreen, courtyard and Assembly outline. |

- Every calculated value can be overridden, as in the leading suite. An override that breaks IPC compliance gives one warning. The suggested name doesn't change.
- Defaults are fixed in the code. The leading suite has wizard defaults, but a per-user default would make one Command give different Footprints for different users.
- **Density** applies to surface-mount families and sets the name's last letter (`N`, `M` or `L`). Through-hole families stay at level B, because the research covers no other through-hole levels.

### Values

Fillet goals and courtyard excess in mm, as Most / Nominal / Least (IPC levels A / B / C):

| Family | Toe | Heel | Side | Courtyard excess |
|---|---|---|---|---|
| Gull-wing, pitch > 0.625 | 0.55 / 0.35 / 0.15 | 0.45 / 0.35 / 0.25 | 0.05 / 0.03 / 0.01 | 0.50 / 0.25 / 0.10 |
| Gull-wing, pitch ≤ 0.625 | 0.55 / 0.35 / 0.15 | 0.45 / 0.35 / 0.25 | 0.01 / -0.02 / -0.04 | 0.50 / 0.25 / 0.10 |
| Chip, 1608 and larger | 0.55 / 0.35 / 0.15 | 0.00 | 0.05 / 0.00 / -0.05 | 0.50 / 0.25 / 0.10 |
| Chip, smaller than 1608 | 0.30 / 0.20 / 0.10 | 0.00 | 0.05 / 0.00 / -0.05 | 0.20 / 0.15 / 0.10 |
| QFN, SON | 0.40 / 0.30 / 0.20 | 0.00 | -0.04 | 0.50 / 0.25 / 0.10 |

- **F and P** are 0.10 and 0.05 mm, used unscaled in `√(C² + F² + P²)`.
- **Round-off** is to the nearest 0.05 mm, and to the nearest 0.02 mm on chips smaller than 1.6 × 0.8 mm, tested on length and width.
- **Courtyard** rounds outward to 0.05 mm. No source gives a standard grid, so this is REDA's own.
- **Molded parts'** goals aren't in this table. The [research file](https://github.com/openreda/reda/blob/research/ipc-values/docs/research/ipc-values.md) points at the transcription that holds them, and the build ticket takes them from there.

### Names

- **Surface-mount** names keep today's forms, such as `RESC1608X55N` and `SOP65P640X120-14N`.
- **QFN and SON:** sources disagree, so REDA keeps width before length and counts the exposed pad in the pin count.
- **Through-hole** names follow a published form: `DIP<span>W<lead width>P<pitch>L<body length>H<height>Q<pins>B`, as in `DIP762W52P254L1905H508Q14B`, and `HDRV<span>W<lead width>P<pitch>R<rows>L<length>T<width>H<height>B`, as in `HDRV200W52P200R2L4400T400H900B`. The sources give examples rather than a rule, so reading the fields this way is REDA's own.

### Pads and paste

The leading suite's documents give the threshold and trimming. The 25% and 50% figures are REDA's own.

- **Lead Pads** are `rounded_rect` at `corner_ratio` 25%, capped at a 0.25 mm radius.
- **Exposed pads** have `zone_connection: {style: solid}`. All other Pads inherit.
- **Split paste:** an exposed pad of 2.1 × 1.6 mm or larger gets an n × m array of [paste windows](#pad-model) at about 50% coverage. Smaller ones inherit.
- **Trimming:** gull-wing Pads don't run under the body, and lead Pads keep clear of the exposed pad, with a warning for each trim. This replaces `keep_body_middle_clear`.
- Each Footprint has a closed courtyard, an Assembly outline, a Silkscreen Designator, an Assembly Designator, Value text and its `height`.

### Regenerating

- A Footprint stores `generator {command, params}` with every value resolved, so a later change to REDA's defaults can't quietly alter what its parameters say. It lives only in the `.redafp`, and it is left out of the part-table copy and the [hash](#library-status). Regenerating to the same geometry doesn't make a placed Component read Library Differs.
- `library.footprint.regenerate {footprint, params?}`, with a preview, keeps the Footprint's UUID, and Pads keep their UUIDs by number. It replaces everything, including hand edits, and the preview lists what is lost.
- There is no stored version or "edited" flag. Running the stored parameters again and comparing tells whether a Footprint still matches, and [`generator_out_of_date`](#library-check) reports when it doesn't, whether from a hand edit or from a changed generator.

### Dialog

One modal form with a live preview: family list, then the sections above, with dimension callouts and warnings on the preview. **Generate** works at any time. The leading suite uses a wizard of 10 to 15 pages, but any page can be skipped, so a single form loses nothing.

It opens from the Footprint editor's Tools menu, the Library panel's New… menu, the Part editor's Footprints ▸ Add ▸ Generate, and Regenerate… in Footprint properties.

## Library check

Source: [How deep do the Library editors, generators and checks go?](https://github.com/openreda/reda/issues/135).

A Command. It reports through the same Violation list and Messages panel as ERC and DRC ([checks](checks.md#reporting)).

- **When it runs:** on demand from the Library panel's Check and the editors' Check Element, and automatically after saving, generating or importing.
- **It never blocks a save.** The leading suite's highest level does.
- **Severities** are Error, Warning and Off, the same as ERC and DRC, set per user in a grid in Preferences ▸ Libraries with Reset, as in the leading suite. The suite has an Info level too, but it isn't worth a second scale for a few checks. A Preference fits here because the check changes no output.

| Check | Finds | Default |
|---|---|---|
| `duplicate_pin_number` | two Pins with one number | E |
| `pin_without_pad` | a Pin with no Pad in some alternate Footprint | E |
| `broken_reference` | a reference that resolves to nothing | E |
| `shorted_copper`, `net_tie_not_joined` | per Part and per alternate Footprint, because a Footprint alone doesn't know its type ([checks](checks.md#net-ties)). SMD Pads on different sides don't touch. | E |
| `missing_pad_number` | a Pad with no number. Checked per Part. Non-plated Pads, and any Pad on a Mechanical Part's Footprint, are exempt, so mounting holes and fiducials pass. | E |
| `mirrored_footprint` | SMD copper only on the bottom, with no through-hole Pads | E |
| `origin_outside_body` | the origin outside the [body box](#body-box) | E |
| `missing_designator_prefix` | a Symbol with none | E |
| `missing_pin_number` | a Pin with none | E |
| `hole_outside_copper` | a plated Pad whose offset leaves its hole outside the copper | E |
| `off_grid_pin` | a Pin off the 50 mil grid | W |
| `unmapped_pad` | a Pad mapped to no Pin and not marked unconnected | W |
| `missing_courtyard` | a Footprint with no courtyard | W |
| `paste_window_outside_copper` | a paste window not fully on its Pad's copper | W |
| `duplicate_primitives` | the same graphic, place and Layer twice | W |
| `origin_off_centroid` | the origin more than 0.1 mm from the [body box's](#body-box) centre | W |
| `missing_description` | an element with none | W |
| `missing_pins_in_sequence` | a gap in a Symbol's numeric Pin numbers. A number in the `absent_pins` of a Part's generated Footprint doesn't count. | W |
| `duplicate_part_name`, `duplicate_mpn` | across the registered Libraries | W |
| `generator_out_of_date` | [stored parameters](#regenerating) that no longer produce this Footprint | W |
| `missing_pin_name` | a Pin with none | Off |

### Body box

A Footprint has no separate body object. Its body box is its courtyard's bounding box, or the [fallback box](checks.md#courtyards) with no courtyard.

## Import part

- **Input:**
  - a KiCad 6+ `.kicad_sym` with `version` ≥ 20210619
  - a `.kicad_mod` in either the legacy `(module …)` or the `(footprint …)` syntax
  - It must not branch on `generator`.
  - These come from manual downloads from SnapMagic, Ultra Librarian and SamacSys. EasyEDA has no KiCad export.
- **Mapping:**
  - One symbol + one footprint become a Symbol, a Footprint and a Part in the chosen Library.
  - Pins bind to Pads by number, so no overrides are needed.
  - MPN, manufacturer, datasheet and supplier numbers arrive as symbol properties under names that vary by service (`MP`/`MF`, `Manufacturer_Part_Number`, `Mouser Part Number`, `LCSC Part`…). Map them to the built-in fields where they fit, and keep the rest as free-form fields.
  - 3D models are ignored.
  - **Pads:** corner ratios and flags, chamfers, the drill offset as `offset`, and `zone_connect` as `zone_connection` all map across. Each Pad's layer list decides its mask and paste:
    - A side with no mask layer is `tent`.
    - An SMD Pad with no paste layer has paste `none`.
    - A paste-only Pad becomes a window on the copper Pad that contains it, or a warning if there is none. It never becomes copper.
    - A mask-only Pad, and paste on a through-hole Pad, are dropped with a warning.
  - A `B.Cu` SMD Pad gets `side: bottom`.
  - A downloaded tie stays Standard, and `shorted_copper` reports it for the user to retype. Tie pad groups wait for project import in v0.6.
  - **Other cases:**

    | In the KiCad files | In REDA |
    |---|---|
    | `exclude_from_bom`, `in_bom no` | In BOM false |
    | `exclude_from_pos_files` | In CPL false |
    | `dnp` | ignored, because Not Fitted belongs to a placed Component |
    | `board_only`, `on_board no` | ignored with a warning, because an import always makes a Symbol and Footprint pair |
    | hidden Pins | visible name and number, with a warning |
    | Pin alternates | dropped with a warning, because `alt_names` is [reserved](architecture.md#reserved-names) |
    | Pin styles `inverted`, `clock`, `inverted_clock`, `input_low`, `clock_low`, `output_low` | the matching [decorations](#pins). The rest warn and stay undrawn. |
    | custom Pad shapes | the anchor Pad with a warning, until Polygon Pads in v0.6 |
    | Reference and Value text | `texts` entries, keeping layer and position |
    | Fab Layer lines and text | the Assembly Layer |
    | Courtyard lines and arcs | chained into a Polygon. If the chain doesn't close, a bounding Rect with a warning. |
    | thermal width and gap | `zone_connection`'s `spoke_width` and `gap` |
- **Provenance:**
  - Each imported element records its source service and terms URL, and keeps any copyright fields.
  - A one-time notice explains that vendor parts stay under the vendor's terms, that most terms forbid redistribution, and that SnapMagic allows at most 10 files per location, so the user should check before committing the Library publicly.
- No vendor files go into REDA's repo or test fixtures ([ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md)).
- The Library check runs after the import.

## Starter Library contents

Everything follows [`docs/library-style.md`](../../library-style.md), which tests check.

- generic passives and discretes, with Footprint alternates: R, C, polarised C, L, ferrite, LED, diodes, BJTs, MOSFETs, crystal, button, fuse
- a generic op-amp, a 555 and an LDO
- headers, sockets and screw terminals
- Mechanical mounting holes and fiducials, with In BOM and In CPL both false. Fiducials have paste `none`.
- a Net Tie (`NT`) in two sizes, with its Pads tented and paste `none`
- a test point (`TP`): a Standard Part with one Pad, paste `none`, and In BOM and In CPL both false
- a small, curated set of popular vendor parts (for example the STM32F042F6P6 MCU and a USB-C receptacle). Each is drawn by hand, or with a generator, from the manufacturer's datasheet: its pinout, Pin types and Footprint are checked against the datasheet, and the element's description and source comment name the datasheet, its document number and the figure used. No vendor CAD files are copied in.
- hand-drawn Footprints cite their source: a named manufacturer drawing or the IPC-7351 nominal
