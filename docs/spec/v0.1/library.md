# v0.1 libraries and parts

Sources: [Where does the v0.1 parts library come from, and what does a library part contain?](https://github.com/openreda/reda/issues/13), [ADR-0008](../../adr/0008-parts-bind-symbols-and-footprints.md), [vendor download research](https://github.com/openreda/reda/issues/19), [KiCad format research](https://github.com/openreda/reda/issues/3). Crates: `reda-library` (scanning, generators, Library check, Commands) and `reda-import-kicad` (Import part). The editors are in `reda-ui`.

## Where parts come from

- A **starter Library** authored by REDA. It is MIT, lives in this monorepo, ships with the app and is read-only.
- **Library editors** in the app.
- **Footprint generators**, which are Commands.
- **Import part**: a one-shot Command that turns one KiCad symbol + footprint pair, the format vendor portals provide, into REDA elements in a chosen Library.

Not in v0.1: full KiCad project or library import (v0.6), direct EasyEDA/LCSC import, supplier search, and 3D models.

## Elements

- **Symbol:**
  - A Symbol has Units, and each Unit has Pins.
  - Each Pin has a name, a number and an electrical type. A Pin name marks an overbar with a [backslash after each character](schematic.md#name-syntax).
  - The Symbol also holds the Designator prefix.
- **Footprint:**
  - A Footprint has Pads, each with a number.
  - Numbers may repeat. Pads that share a number belong to the same Pin.
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

- When a Component is placed, the sheet's part table stores the whole Part: the Symbol, the pin-map overrides, the fields, the Component Type, In BOM, In CPL, and a copy of **every** alternate Footprint, deduplicated by hash. The PCB stores only the Footprints in use ([architecture](architecture.md#file-format)).
- Library copies on a sheet are never edited in place. A Pad of a Footprint placed on the PCB can be edited, which gives that instance a [locally edited Footprint](pcb.md#placed-pad-edits).
- **Update from Library:**
  - A content-hash mismatch marks a Component with "library has a newer version".
  - **Update from Library** previews the changes before applying them. It never updates automatically.
  - Footprint changes then reach the PCB through Sync. The preview lists any local Pad edits this removes.
- **Edit in Library** on a placed Component opens its element in the matching editor.

## On disk

- A Library is a folder holding:
  - `<name>.redalib` (name, UUID, `format`, description)
  - `symbols/*.redasym`, `footprints/*.redafp` and `parts/*.redapart`, one canonical-JSON file per element
- Elements are found by scanning the folder. There is no manifest. Elements reference each other by UUID, across Libraries too.
- **Registered Libraries:**
  - the starter Library
  - user Libraries, listed in Preferences (Libraries page)
  - Project Libraries, listed in the `.reda` file by relative path
- **New Library** (`library.create`) writes the `.redalib` into a new or empty folder. It is offered in the Library panel's New… menu and on the Project Settings Libraries page. A user Library is added to Preferences at once; a Project Library is then registered with `project.library.add`, which is undoable.

## Library panel

- **On a sheet** it lists Parts that have a Symbol. **On the PCB** it lists Mechanical Parts.
- **Search** is a substring search over name, Description, Keywords, Value, MPN and field values, with filters for Library and Component Type.
- **Preview** shows the Symbol (Units as tabs) and the Footprint, with a picker for the alternates.
- Drag or click to place. "Edit" and "New…" open the editors.

## Editors

Symbol, Footprint and Part editors open as Document tabs in the same window. Each has its own undo stack. They take Recovery Snapshots ([app](app.md#crash-recovery)). Starter Library elements are read-only.

- **Symbol editor:**
  - multiple Units
  - Pins on the 50 mil grid, with name and number visibility set per Pin
  - lines, rectangles, arcs, circles, polygons and text
  - a pin table you can paste into
- **Footprint editor:**
  - **Pads:** every property in the [Pad model](#pad-model)
  - graphics on the Silkscreen, **Courtyard** and **Assembly** Layers, top and bottom (Courtyard and Assembly join the fixed v0.1 Layer set)
  - placement of the Designator and value text, plus the origin
  - the zero orientation follows IPC-7351, which the CPL relies on
- **Part editor:** picks the Symbol and the alternate Footprints, edits the pin-map overrides, the default fields, the Component Type, In BOM and In CPL.

Deferred: De Morgan body styles, Pins that swap between Units, bus Pins, custom polygon Pads, custom mask and paste shapes, 3D models, Pad stacks that differ per Layer, and Graphical Parts.

## Pad model

Source: [How do mask, paste and tenting inherit, and what can a Pad express?](https://github.com/openreda/reda/issues/126). Footprint Pads and Free Pads share all of this. Where a value inherits from is in [pcb](pcb.md#mask-and-paste).

- **Kind:** SMD, plated through-hole or non-plated.
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
  - An SMD Pad refuses `bottom` and `from_hole_edge`.
- **Solder paste:** `paste`, SMD Pads only.
  - Left out, it inherits. Otherwise it is `{expansion}`, `{percent}`, `none` or `{windows}`.
  - `percent` is the change in area: 0 matches the copper and -10 has 10% less area. The copper's shape is scaled about its centre, and the value must be above -100.
  - `windows` is a list of `{at, width, height, corner_ratio?}` in the Pad's frame. Together they replace the opening.
- **Zone connection:** `zone_connection` is `solid`, `relief` or `none`. Left out, it inherits. [How deep do Zones, Keepouts and the outline editor go?](https://github.com/openreda/reda/issues/137) settles what it inherits from and the spoke values.

## Footprint generators

Generators are Commands. They produce **IPC-7351 Nominal** density for:
- 2-terminal chips, 0201–2512
- SOT-23-3/5/6 and SOT-223
- SOIC, SSOP and TSSOP
- QFN and DFN with an exposed pad
- QFP and DIP
- 1×N and 2×N headers and sockets at 2.54 and 1.27 mm

Other density levels and BGA come later.

## Library check

A Command. It never blocks saving, and it runs automatically after an import or a generator run. It reports through the same Violation list and Messages panel as ERC and DRC ([checks](checks.md#reporting)). It finds:
- off-grid Pins
- duplicate Pin numbers
- Pins with no Pad in some alternate Footprint
- Pads mapped to no Pin and not marked unconnected
- a missing courtyard
- broken references
- `shorted_copper` and `net_tie_not_joined`, per Part and per alternate Footprint, because a Footprint alone doesn't know its type ([checks](checks.md#net-ties))
- `paste_window_outside_copper`: a paste window not fully on its Pad's copper (warning)
- `hole_outside_copper`: a plated Pad whose offset leaves its hole outside the copper

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
  - A downloaded tie stays Standard, and `shorted_copper` reports it for the user to retype. Tie pad groups wait for project import in v0.6.
- **Provenance:**
  - Each imported element records its source service and terms URL, and keeps any copyright fields.
  - A one-time notice explains that vendor parts stay under the vendor's terms, that most terms forbid redistribution, and that SnapMagic allows at most 10 files per location, so the user should check before committing the Library publicly.
- No vendor files go into REDA's repo or test fixtures ([ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md)).
- The Library check runs after the import.

## Starter Library contents

- generic passives and discretes, with Footprint alternates: R, C, polarised C, L, ferrite, LED, diodes, BJTs, MOSFETs, crystal, button, fuse
- a generic op-amp, a 555 and an LDO
- headers, sockets and screw terminals
- Mechanical mounting holes and fiducials, with In BOM and In CPL both false. Fiducials have paste `none`.
- a Net Tie (`NT`) in two sizes, with its Pads tented and paste `none`
- a test point (`TP`): a Standard Part with one Pad, paste `none`, and In BOM and In CPL both false
- a small, curated set of popular vendor parts (for example the STM32F042F6P6 MCU and a USB-C receptacle). Each is drawn by hand, or with a generator, from the manufacturer's datasheet: its pinout, Pin types and Footprint are checked against the datasheet, and the element's description and source comment name the datasheet, its document number and the figure used. No vendor CAD files are copied in.
- hand-drawn Footprints cite their source: a named manufacturer drawing or the IPC-7351 nominal
