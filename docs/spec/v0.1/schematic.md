# v0.1 schematic editor

Source: [Which features make up the v0.1 schematic editor?](https://github.com/openreda/reda/issues/7). Editing behaviour is modelled on professional integrated suites, not on KiCad. Crate: `reda-schematic` (connectivity, ERC, Commands); editor and Tools in `reda-ui`.

## Sheets

- A Project has multiple **flat** Schematic Sheets, one `.redasch` file each.
- The Projects panel can add, rename, reorder and delete sheets ([app](app.md#projects-and-settings), [How are Projects, Documents and board setup created and edited in v0.1?](https://github.com/openreda/reda/issues/38)):
  - A new Project starts with one sheet, "Sheet 1". Added sheets default to `sheet-<n>.redasch`.
  - The changes reach disk on save, and each one is undoable on the Project's undo stack.
  - The order gives "sheet n of m" and the PDF page order.
  - Each sheet opens as a tab.
  - Deleting a sheet that holds Components asks for confirmation and removes those Components from the Project. Their Footprints go at the next Sync, as ordinary removals ([sync](sync.md)).
- Copy and paste works between sheets.
- **Sheet frame:**
  - A sheet size (A4, A3, Letter…).
  - One built-in title block (title, revision, date, author, sheet n of m), filled from Project and sheet properties.

## Connectivity

- **Wires:**
  - Wires are orthogonal. A key toggles the corner direction, and Backspace removes the last segment.
  - A Wire run ends on its own when it reaches a Pin or another Wire.
  - A Wire connects only where its end touches a Pin or a Wire exactly.
- Dropping a Component so that its Pins land on a Wire splits the Wire and connects the Component.
- **Junctions** are derived automatically and never placed by hand.
- **Scope rules:**
  - **Net Labels** connect within their sheet.
  - **Ports** connect across sheets. They are named and have a direction: in, out or bidirectional.
  - **Power Ports** connect across the whole Project. They are built-in objects in fixed styles (ground, bar, arrow, circle…), not Library elements.
- **No-Connect Markers** mark Pins that are deliberately left open.
- **Net names:**
  - An unnamed Net gets a stable auto-name `Net-(<Designator>-<Pin number>)`, such as `Net-(R1-1)`, from its lowest Designator and then its lowest Pin number in natural sort (R2 before R10, Pin 2 before 10). It uses the Pin number, never the Pin name, and has no Unit suffix, because Pin numbers are unique across the whole Component. The name does not renumber when unrelated parts change ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).
  - When a Net has several names, the winner is chosen by priority: Power Port > Port > Net Label > auto-name. ERC warns about the conflict.
- Nets are derived live from the drawing. They are never stored ([architecture](architecture.md#design-data-model)).

## Components

- **Units:**
  - Multi-unit Symbols (dual op-amps, quad gates) have explicit power Pins. There are no hidden Pins.
  - Pins show the mapped Pad number(s) of the Component's current Footprint ([library](library.md)).
- **Placement:**
  - Components are placed from the dockable **Library panel** ([library](library.md#library-panel)), by dragging or by clicking.
  - SMD and through-hole parts are supported equally.
- **Annotation:**
  - A Component gets the next free Designator when it is placed, so no Component is ever unannotated.
  - **Re-annotate** offers:
    - a processing order (across then down, down then across, …), or by board position (see [sync](sync.md#write-through-edits))
    - a per-Component Designator lock
    - a scope: Project, sheet or selection
    - an old → new preview before it applies
  - It keeps the Units of a multi-unit Component together.
  - It is a cross-Document Command in `reda-engine`.
- **Properties panel** for a Component:
  - Designator and value.
  - The chosen Footprint, from the Part's alternates.
  - Free-form fields (MPN, `LCSC`, notes…) that flow into the BOM.
  - A Not Fitted flag ([outputs](outputs.md#not-fitted)).
  - In BOM and In CPL, defaulted from the Part ([library](library.md#elements)).
  - Which fields are visible on the sheet.
- **Multi-select editing:**
  - The panel shows the shared fields, with `<mixed>` where values differ.
  - An edit applies to every selected object.
- **Select Similar** selects the objects that match on the chosen fields, on the current sheet or across the Project.

## Editing

- **Selection:**
  - Click selects.
  - A box dragged left to right selects what it encloses. A box dragged right to left selects what it touches.
  - Shift adds to the selection, and `Mod`+click toggles.
- **Drag versus move:**
  - A plain drag keeps connections, so Wires rubber-band.
  - `Mod`+drag detaches and moves.
  - On the keyboard, M is Move (detach) and D is Drag ([keymap](app.md#keymap)).
- **While placing or moving:**
  - Rotate, mirror X and mirror Y.
  - Tab opens Properties for the object about to be placed.
  - Later placements in the same run inherit those edits, with an auto-incremented Designator.
  - Placement repeats until cancelled. Repeat placement is a Preference, on by default.
- **Editing Commands:**
  - Delete, cut, copy, paste and duplicate.
  - Undo and redo, with one stack per sheet. Undo is unlimited by default. The *undo depth* Preference is an optional cap per Document stack ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).
  - Align left, right, top, bottom or by centres. Distribute horizontally or vertically. Snap the selection to the grid.
- **Grid:**
  - Snap-grid presets: 10, 25, 50 and 100 mil, plus metric equivalents. One key cycles them, and the default is 50 mil.
  - Electrical snap moves the cursor onto a nearby Pin, Wire end or Wire.
- **Graphics:** free text, lines and rectangles.
- **Find:**
  - Searches by Designator, value or Net name across sheets.
  - It selects the match and zooms to it.
- **Net highlighting:**
  - Clicking a Net highlights it across every sheet, and on the PCB once the board is synced ([sync](sync.md#cross-select-and-net-highlighting)).
- **Export:** a vector PDF of all sheets, in Document order ([outputs](outputs.md#schematic-pdf)).

## Deferred past v0.1

Buses; hierarchical sheets (v0.5); 45° and any-angle Wires; images, arcs and circles, rich text and tables on sheets; custom title-block templates; find-and-replace; paste arrays; saved selection sets; persistent per-Net colours; printing to paper.
