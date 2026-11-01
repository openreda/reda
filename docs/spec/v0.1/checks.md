# v0.1 ERC and DRC

Source: [What are the v0.1 ERC and DRC rule sets?](https://github.com/openreda/reda/issues/15). ERC lives in `reda-schematic` and DRC in `reda-pcb`. The Library check is in [library](library.md#library-check).

## ERC checks

Default severity: **E** = Error, **W** = Warning.

| Check | Default |
|---|---|
| Unconnected Pin: Input and Power In | E |
| Unconnected Pin: other types. Pins of type Not Connected, and Pins with a No-Connect Marker, are exempt. | W |
| Pin-type conflict on a Net: Output↔Output, Output↔Power Out, Power Out↔Power Out | E |
| Pin-type conflict on a Net: mixes such as Tri-state↔Output and Open Collector↔Output | W |
| Undriven power: a Net with Power In Pins and no Power Out Pin. A Power Port counts as a driver, so no "power flag" symbol is needed. | W |
| Single-Pin Net | W |
| Dangling Wire end | W |
| Net Label or Port attached to nothing | W |
| No-Connect Marker on a connected Pin | W |
| One Net with several names | W |
| Port on only one sheet | W |
| Duplicate Designators | E |
| Unplaced Units of a placed multi-unit Component | W |
| Unit data mismatch: Designator, value, fields, Footprint choice, Component Type or Not Fitted differing between the Units of one Component ([model](architecture.md#design-data-model)) | E |

The pin-type matrix is built in and fixed. Individual cells can't be edited, but the whole pin-conflict check can be switched Off.

Commands keep Unit data equal, so a Unit data mismatch only comes from a hand-edited or merged file. There is no automatic fix in v0.1: the user makes the Units agree in the Properties panel, which shows `<mixed>` where they differ ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).

## DRC checks

| Check | Default |
|---|---|
| Clearance between copper of different Nets (per Net Class) | E |
| Short circuit: different Nets touching (reported apart from clearance) | E |
| Track width below the minimum | E |
| Via or Pad hole below the minimum | E |
| Annular ring below the minimum | E |
| Hole-to-hole spacing | E |
| Copper too close to the Board Outline or a Board Cutout | E |
| Keepout violation | E |
| Footprint outside the Board Outline | E |
| Unrouted connections (any Ratsnest left) | E |
| Copper with no Net | W |
| Dangling Tracks and Vias | W |
| Courtyard overlap | W |
| Silkscreen over exposed Pads | W |
| PCB out of sync with the schematic, including differing fields and Not Fitted | W |

- Before checking, DRC refills stale Zones, so it never checks against an old fill.
- Deferred: solder-mask slivers, acid traps, silk-to-silk spacing.

## Design rules

- The PCB Document holds one set of board-wide values:
  - minimum clearance
  - minimum Track width
  - minimum Via diameter and hole
  - minimum annular ring
  - hole-to-hole spacing
  - copper-to-edge clearance
- **Net Classes:**
  - Each Net Class can override the clearance, the Track width and the Via size.
  - Where two Net Classes meet, the larger clearance applies.
  - New Nets get the Default Net Class.
  - A Net is assigned to a Net Class with `pcb.net.set_class {nets, class}`, from the Net table in Board Setup or from the Properties panel for a selected Track, Via, Pad, Zone or highlighted Net. A multi-selection sets it for all their Nets. Assignment is per Net only, with no pattern rules in v0.1.
- **Fab Presets and the rules:**
  - A Fab Preset seeds the rules when the board is created.
  - "Apply Fab Preset rules" re-seeds them, after a preview like a Change List.
  - Output Jobs never change the rules ([outputs](outputs.md#fab-presets)).
- **Seed values** for 2-layer boards come from the [fab research](https://github.com/openreda/reda/issues/4). The Generic preset is conservative (0.20 mm Track, 0.16 mm clearance, 0.30 mm Via drill…), and the per-fab presets are looser.
- There is no rule-query language and no per-Layer or per-area rules in v0.1. The model must leave room for both.
- The routing Tool enforces clearance live.

## Severities

- Each check is set to Error, Warning or Off.
- ERC severities are stored in the Project (`.reda`). DRC severities are stored in the PCB Document, next to its rules.
- **Where they are edited** ([How are Projects, Documents and board setup created and edited in v0.1?](https://github.com/openreda/reda/issues/38)):
  - ERC severities on the ERC page of **Project Settings**, beside the read-only pin-type matrix.
  - DRC severities in **Board Setup**: on each rule's page beside the rule, and all together on its DRC page ([app](app.md#settings-dialogs)).
  - Right-clicking a Violation in Messages offers **Set severity of "<check>" → Error / Warning / Off**, next to **Waive**.

## Running

- On demand, through `erc.run` and `drc.run` (query Commands), from the GUI, the CLI and MCP.
- There is no background online DRC in v0.1.
- Results stay on show until the next run. They are marked stale once the design is edited after the run.

## Reporting

- **Violations:**
  - A run produces a list of Violations.
  - Each has a check id, a severity, structured message arguments and the UUIDs of the objects involved.
  - The UI turns the check id and arguments into text ([i18n](app.md#internationalisation)). The Command result carries the same structured list, so agents can read it.
- **Messages panel:**
  - The list appears in the shared Messages panel, grouped by Document.
  - Clicking a Violation opens its Document, zooms to it and selects the objects involved.
  - The canvas shows a marker at each Violation.

## Waivers

- A single Violation can be waived, for example a deliberate net tie or an unconnected test Pin.
- **Storage:**
  - A Waiver is stored in the Document that holds the objects.
  - It records the check id and the UUIDs of the objects involved, with an optional comment.
- A Waiver stops matching once those objects no longer break that check.
- Waived Violations are hidden by default, with a toggle to show them.
