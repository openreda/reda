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
- Copy and paste works between sheets, and [Move to Sheet](#move-to-sheet) moves circuitry between them with its identity intact.
- **Sheet frame:**
  - A sheet size (A4, A3, Letter…) or a custom width and height, and an orientation.
  - **Border Zones**, on by default, can be switched off per sheet. They are about 50 mm, derived from the sheet size, with numbered columns and lettered rows, so a location reads `B3`. The leading suite sets counts per sheet. REDA derives them, so a location means the same on every sheet of a size.
  - One built-in title block (title, revision, date, author, sheet n of m), filled from Project and sheet properties, which can be switched off per sheet.
  - Each sheet has an optional drawing number and revision. An empty revision shows the Project's, greyed in the editor. Empty title fields are tinted on the canvas, never in the PDF.
  - **Reserved names:** a frame `style` for v0.2 templates and a sheet `default_font`.
  - The Projects panel shows each sheet's number beside its name. ([What schematic depth does v0.1 specify beyond the editing contracts?](https://github.com/openreda/reda/issues/133))

## Connectivity

- **Wires** are polylines at any angle, stored as `vertices`, at least two ([Wires](#wires)). Contact must be exact. A Wire connects:
  - where its end touches a Pin's connection point or any point of another Wire
  - where it passes over a Pin's connection point, a Port or a Power Port
  - where a Net Label sits anywhere along it, or directly on a Pin's connection point with no stub Wire
- Two Wires crossing mid-span never connect.
- These rules don't depend on any setting, so a file that isn't normalised, such as a hand-edited or merged one, has the same connectivity.
- **Junctions** are derived automatically and never placed by hand. One appears at a T, at four Wire ends meeting at a point, and where a Wire passes over a Pin's connection point, a Port or a Power Port. A Net Label makes none.
- **Scope rules:**
  - **Net Labels** connect within their sheet.
  - **Ports** connect across sheets. They are named and have a direction: in, out or bidirectional. The arrow follows the direction and the side the Wire attaches to, and a Port sizes itself to its name. [Cross-references](#port-cross-references) show where the others are.
  - **Power Ports** connect across the whole Project. They are built-in objects in seven fixed styles (bar, arrow, circle, wave, ground, signal ground, earth), not Library elements. The name can be hidden, and GND, 3V3, 5V and VCC are Presets on the Active Bar.
- **No-Connect Markers** mark Pins that are deliberately left open.
- **Name syntax** ([Name syntax](#name-syntax)) covers overbars and reserved characters.
- **Net names:**
  - An unnamed Net gets a stable auto-name `Net-(<Designator>-<Pin number>)`, such as `Net-(R1-1)`, from its lowest Designator and then its lowest Pin number in natural sort (R2 before R10, Pin 2 before 10). It uses the Pin number, never the Pin name, and has no Unit suffix, because Pin numbers are unique across the whole Component. The name does not renumber when unrelated parts change ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).
  - Names compare case-insensitively everywhere, in connectivity, Sync, `duplicate_nets` and Assign by pattern, so `abc` and `ABC` are one Net ([What do Violations carry, and what do ERC and DRC add?](https://github.com/openreda/reda/issues/128)).
  - When a Net has several names, the winner is chosen by priority, and then the lowest by code point. The default is Net Label > Power Port > Port > auto-name. Two Project options change it:
    - **Power Port Names Take Priority** (off) gives Power Port > Net Label > Port > auto-name.
    - **Allow Ports to Name Nets** (on). Off, a Port never names a Net.
  - Different names are an ERC finding unless they differ only in case ([checks](checks.md#erc-checks)).
  - Different kinds of identifier never join by name. A Net Label `GND` joins a Power Port `GND` only through a Wire.
  - **Append Sheet Numbers to Local Nets** (a Project option, off) gives every Local Net named by a Net Label the board name `<name>_<n>`, where `n` is the sheet's position in the Projects panel. Sheets keep showing the plain name. Reordering sheets renames those Nets, which Sync treats as ordinary renames. Auto-names are already unique and stay as they are.
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
  - [Re-annotate](#re-annotate) renumbers Designators, with a per-Component Designator lock. It is a cross-Document Command in `reda-engine`.
- **Properties panel** for a Component:
  - Designator and value.
  - The chosen Footprint, from the Part's alternates.
  - Free-form fields (MPN, `LCSC`, notes…) that flow into the BOM.
  - The [Unit](#units) and [field](#fields) rows.
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
- **Grab point:** the Preferences **Object's Electrical Hot Spot** (on) and **Center of Object** (off) set where a moved object is held. Hot Spot holds an electrical object at its nearest connection point and overrides Center for it. Center holds an object by its reference point or centre. With neither, it is held where it was clicked.
- **Connectivity while dragging:** one Preference, **Display When Dragging** (on), shows predicted Junction dots and a chip by the cursor during Drag and Move. The chip compares Nets at the current snapped position and shows OK, or an alert naming the change ("joins GND and VCC"). It never blocks the drop. The leading suite shows only an icon. REDA adds the names because the comparison already has them.
- **Keys during a drag:** holding Alt constrains it to horizontal or vertical, and Shift+Space rotates clockwise. `Mod`+arrows nudge by 1 grid and Shift+`Mod`+arrows by 10, rubber-banding like a drag, with one undo per press.
- **Drag versus move:**
  - With **Always Drag** (a Preference, on), a plain drag keeps connections, so Wires rubber-band, and `Mod` inverts it for that drag.
  - On the keyboard, M is Move (detach) and D is Drag ([keymap](app.md#keymap)).
  - **Drag Orthogonal** (a Preference, on, with a **Drag Step**) keeps rubber-banded Wires at 90°. Off, they go oblique.
  - `Mod`+Space switches the corner direction of rubber-banded Wires during a drag.
- **While placing or moving:**
  - Rotate, mirror X and mirror Y.
  - Tab opens Properties for the object about to be placed.
  - Later placements in the same run inherit those edits, with an auto-incremented Designator.
  - Placement repeats until cancelled. Repeat placement is a Preference, on by default.
- **Editing Commands:**
  - Delete, cut, copy, paste and duplicate ([identity](#cut-copy-and-paste)).
  - Undo and redo, with one stack per sheet. Undo is unlimited by default. The *undo depth* Preference is an optional cap per Document stack ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).
  - Align left, right, top, bottom or by centres. Distribute horizontally or vertically. Snap the selection to the grid.
- **Grid:**
  - Snap-grid presets: 10, 25, 50 and 100 mil, plus metric equivalents. One key cycles them, and the default is 50 mil.
  - Electrical snap moves the cursor onto a nearby Pin, Wire end or Wire.
- **Graphics:** free text with 9-point justification and multiple lines, lines and rectangles.
- **Find** is a Panel:
  - **Results:** a list of every match with its kind, text and sheet. Picking one selects and zooms to it, and F3 and Shift+F3 step through the list.
  - **Options:** kinds (Designators, values, Net names, all text), case, whole word, `*` and `?` wildcards (always ignoring case), and regular expressions.
  - **Scope:** this sheet, the Project or the selection. **Mask Matching** dims everything else.
  - `schematic.find` returns the same list. Net names match ignoring case unless the case option is on.
- **Net highlighting:**
  - Alt+click on a Wire, Pin or label highlights the Net across every sheet, and on the PCB once the board is synced ([sync](sync.md#cross-select-and-net-highlighting)). A plain click selects. The context menu offers the same, because many Linux window managers take Alt+click.
  - Esc or a click on empty space clears it. The Projects panel marks the sheets that hold the Net.
  - **Select Net** selects all the wiring of the Net on this sheet.
- **Export:** a vector PDF of the sheets, in Document order ([outputs](outputs.md#schematic-pdf)).

## Wires

Source: [What are the schematic editing contracts?](https://github.com/openreda/reda/issues/132). Modelled on the leading suite's Wire editing, Preferences included.

### Normalisation

Normalisation runs inside every edit Command that creates or changes Wires or Units, in the same transaction and undo step. It touches only the Wires involved and those they contact. Loading never rewrites a file. Four per-user Preferences control it, and Commands take them as an optional `wiring` parameter:

| Preference | Default | On | Off |
|---|---|---|---|
| Optimize Wires | on | Overlapping collinear Wires merge, and so do collinear Wires meeting end to end with nothing else at that point. Collinear vertices inside a Wire are removed. | Wires stay as drawn. |
| Components Cut Wires (needs Optimize Wires) | on | See [Dropping a Component](#dropping-a-component-on-a-wire). | Pins connect, and the part may be shorted. |
| Break Wires At Autojunctions | on | A Wire splits at each Junction on its interior. | It stays one Wire through the Junction. |
| Convert Cross-Junctions | on | A Wire that would make a 4-way is offset into two adjacent 3-ways, one snap grid apart. | The 4-way stays as drawn. Wires that pass straight through a point don't join. |

- **Always:** zero-length segments are dropped.
- **Identity:** when a Wire splits, the piece starting at its first vertex keeps the UUID. When Wires merge, the lowest UUID survives. Deleting one branch of a T removes the Junction, and with Optimize Wires the other two merge.
- **Headless:** a Command with no `wiring` parameter uses the defaults above, not the user's Preferences. The GUI passes the Preferences, so MCP and CLI results are the same on every machine. The leading suite has no headless use to compare with.

### Dropping a Component on a Wire

Applies to placing, moving and pasting, when Components Cut Wires is on:

- **One Pin lands on a Wire:** the Pin connects, and it's a Junction.
- **Two or more Pins of one Unit land on one straight stretch:** the Wire is split at each, and the pieces between consecutive Pins are removed, so the part isn't shorted.
- **A connection between two Pins:** with a T or another contact between them, the pieces stay, because removing them would orphan the branch.
- **Moving the part away:** the Wire doesn't heal. Undo restores it.

The leading suite documents the two-Pin case only. The other cases are REDA's.

### Wire Tool

The Tool is a state machine that previews until it commits. A run commits as one `schematic.wire.add`, one Wire and one undo step, or several Wires if normalisation splits it.

| Input | Effect |
|---|---|
| Click or Enter | Fixes a corner. In every mode but Any Angle, the segment on the cursor is a look-ahead and the segment being placed comes before it. |
| Landing on a Pin, Wire, Net Label, Port or Power Port point | Commits and ends the run. The Tool stays active. |
| Clicking the same point twice | Ends the run there. |
| Esc or right-click during a run | Commits the fixed corners, drops the look-ahead and stays in the Tool. |
| Esc or right-click with no run | Leaves the Tool. |
| Backspace or Delete | Removes the last segment. With none left, the run ends with nothing drawn. |
| Insert | Adds a vertex. |
| Shift+Space | Cycles the mode: 90°, 45°, Any Angle, Auto Wire. The status bar shows it, and it can change during a run. The session remembers it. |
| Space | In 90° and 45°, toggles the start and end corner modes. |

- A segment that passes over a Pin without a click there connects, under the [connectivity rules](#connectivity), but doesn't end the run.
- **Auto Wire** routes from the last vertex to the clicked point, around placed objects, and draws the path as a thick dotted line. Tab opens Time Out After (3 s) and an Avoid cutting wires slider from low to high. The router is written in-house, and `schematic.wire.route` is its query.
- **Another Tool during a run:** starting one suspends the run. Finishing that Tool with Esc returns to the run. This goes one level deep.

### Editing a placed Wire

- **Selecting:** a click selects the Wire. A click on a segment of the selected Wire selects only that segment, with red handles. Shift+click twice adds a segment from another Wire.
- **Handles:** drag an endpoint, a vertex (the ends stay put) or a segment (the other vertices stay put). Dragging an unselected Wire moves the whole Wire.
- **Vertices:** Delete while holding a vertex removes it, and Insert while holding a segment adds one. Properties lists the vertices with Index, X and Y, plus Add and Remove.
- **Deleting segments:** a deletion stops at the Junction, and takes the Junction too when only two segments would remain at it.
- **Hover:** a tooltip shows the Net name, and the board name when [Append Sheet Numbers](#connectivity) changes it.

### Break Wire

The Break Wire Tool cuts a gap out of a Wire, and `schematic.wire.break` is its Command. The first piece keeps the UUID.

- **Cutting Length** cycles with Space: Snap To Segment, Snap Grid Size Multiple (2 to 10) or Fixed Length (default units of 10 mil). The cutter shrinks to fit a shorter segment.
- **Display Preferences:** Show Cutter Box and Show Extremity Markers (Never, Always, On Wire). With both Never, the part to be cut is highlighted instead.
- Changes made in the Tool write straight back to the Preference.

### Cross-junctions and cross-overs

- **Display Cross-Overs** (a Preference, off) draws small bridges at crossings on the focused sheet. It's canvas only and never reaches the PDF. The leading suite's docs describe it only on the focused sheet.
- **Convert Cross Junctions** (`schematic.junctions.convert_cross`) has a dialog:
  - scope: this sheet or the Project
  - All or Selected Objects. With Selected Objects, every Wire entering the 4-way must be selected.
  - Miter Size, one snap grid by default

## Cut, copy and paste

Source: [What are the schematic editing contracts?](https://github.com/openreda/reda/issues/132).

- **Identity:** every paste gives new UUIDs, as in the leading suite, including after a cut. A pasted Component is a new Component, so Sync adds a new Footprint and proposes the old one for removal, losing its placement and routing. [Move to Sheet](#move-to-sheet) is the way to keep that link.
- **Designators:** a pasted Component keeps its Designator if it's free, otherwise it takes the next free one. The leading suite can reset to `R?` through a Preference. REDA doesn't, because no Component is ever unannotated.
- **Reference point:** **Clipboard Reference** (a Preference, off) makes cut and copy ask for a click on a reference point. Off, it's the lower-left corner of what was copied.
- **Placing:** in the GUI, a paste floats on the cursor until a click, with a hotspot of the pasted objects on the grid, so Pins and Wire ends stay on grid. `schematic.paste` keeps its `at`.
- **Duplicate** is a repeating stamp, with Designators following the same rule.
- **Copy As Text** copies the text of Net Labels, Ports, Power Ports, text and Designators.

## Move to Sheet

Source: [What are the schematic editing contracts?](https://github.com/openreda/reda/issues/132).

`schematic.selection.move_to_sheet {objects, document, at?}` moves any selection, down to a single Unit, in one atomic Command:

- Every UUID and Designator is kept, and part-table entries move with their Units, so the Footprint, placement and routing stay linked.
- Objects keep their coordinates unless `at` is given.
- The result lists the Nets that changed, because a Net Label is sheet-local and moving one can split a Net.
- **GUI:** from the context menu, pick the sheet in a dialog. That tab becomes active and the selection floats, as in Move. A click places it, and only then does it leave the source sheet. Esc cancels the whole move.
- **Undo:** the Command records an independent entry on each sheet. Undoing on the source sheet first is [refused](architecture.md#design-data-model), and the message says which sheet to undo first.

## Name syntax

Source: [What are the schematic editing contracts?](https://github.com/openreda/reda/issues/132).

- **Overbars:**
  - A backslash after a character negates that character, as in `R\E\S\E\T\`, or `CSN\` for part of a name. This works in Net Label, Port, Power Port and Pin names.
  - With **Single '\' Negation** on, a leading backslash negates a whole Net Label, Port or Power Port name, as in `\RESET`. Off, a leading backslash has no effect and isn't drawn. Pin names don't use this form.
  - The setting is a Preference (off), as in the leading suite. It changes how a name is drawn, never the name itself, so connectivity is the same for every user.
  - The markup is part of the name, so `\RESET` and `RESET` are different names.
  - The canvas and the PDF draw the overbar. Importers convert other forms, such as `~{X}`.
- **Reserved:** Net Label, Port and Power Port names refuse `[`, `]`, `{`, `}`, `,`, `..` and control characters, in Commands and on load, so v0.5 bus syntax needs no format change. Pin names aren't restricted, because they never join Nets. Importers replace these characters.

## Units

Source: [What schematic depth does v0.1 specify beyond the editing contracts?](https://github.com/openreda/reda/issues/133).

- **Unit Suffix:** the Preference `schematic.unit_suffix` is `alpha` (`U1A`, the default) or `numeric` (`U1:1`), as in the leading suite.
  - It comes from the Unit's position in the Symbol, not its editable name. A Symbol with one Unit shows none.
  - It is drawn after the Designator on the canvas and in the PDF, and it appears in PDF bookmarks. It never reaches the Designator, BOM, CPL, Sync or Net names.
  - Find matches both `U1` and `U1A`.
- **Placing from the Library panel:** a drag places one instance. A click, double-click or Enter starts repeat placement.
- **Repeat placement of a multi-Unit Part** starts from the Unit tab the panel shows, fills the remaining Units of one Component, then starts a new Component at its first Unit.
- **Next Unit:** `schematic.unit.cycle` moves a placed symbol to the next unplaced Unit, and refuses if there are none. The leading suite can land on a Unit that is already placed, leaving a clash to fix later. REDA keeps every Unit unique.
- **Properties** shows which Unit this symbol is and lists the unplaced Units, each with a **Place** button.

## Fields

Source: [What schematic depth does v0.1 specify beyond the editing contracts?](https://github.com/openreda/reda/issues/133).

- **Autoposition:** a field stays autopositioned until it is moved by hand. Fields stack right of the body at 0° and 180°, and above it at 90° and 270°.
- **Moving a field:** each field can be selected on its own. `schematic.field.move` stores it in the Unit's sparse `field_text` table, keyed by field name, with an offset from the Symbol origin and an optional text size. A field with no entry is autopositioned.
- **Rotating:** a moved field follows its Unit, and rotating the Unit rotates the offset.
- **Reset position** (`schematic.field.reset_position`) removes the entry, which does the job of the leading suite's per-field Autoposition checkbox.
- **Right-reading:** field text is never upside down or reading from the right. This is a fixed rule. The leading suite has a Preference for true-angle text, and REDA leaves it out because it serves no workflow.
- **Properties** has one row per field: value, a visible eye, autoposition state and size.
- **In-place edit:** F2 or a slow second click edits Net Labels, Ports, Power Ports, Designators, values, visible fields and free text on the canvas.
  - Enter commits and Esc cancels. Shift+Enter adds a line in free text.
  - A Designator that is taken is refused inline, and a rename applies to every Unit.
  - **In-Place Editing** is a Preference, on.

## Auto-increment

Source: [What schematic depth does v0.1 specify beyond the editing contracts?](https://github.com/openreda/reda/issues/133).

The Preferences `schematic.auto_increment.primary` and `.secondary` default to 1. Secondary applies only to Pin names ([library](library.md)).

- Only a trailing number changes, and its padding is kept: `D09` → `D10`, `D01` → `D02`.
- A trailing overbar is ignored when finding the number: `CS1\` → `CS2\`.
- A name with no trailing number repeats unchanged. A negative step decrements and stops at 0.
- Designators skip numbers that are taken. Net Labels and Ports don't.
- **After Tab:** Enter commits and resumes placement, and Esc resumes without the edit. A "Paused" chip shows on the canvas.

## Re-annotate

Source: [What schematic depth does v0.1 specify beyond the editing contracts?](https://github.com/openreda/reda/issues/133).

`schematic.reannotate` and `schematic.reannotate.preview` take the same parameters. There is no hash guard, because the dialog is modal and the Command returns what it did. Each Component in the scope that isn't locked gets the smallest free number for its prefix, starting from any start index. Locked and out-of-scope Components keep theirs, and the Units of a Component stay together.

- **`source`:** `schematic` or `board`.
- **`mode`:** `renumber_all`, or `fix_duplicates_only`, where the first in processing order keeps its number. Locked duplicates stay, and the preview says so. The leading suite renumbers only unannotated `R?` parts, so it needs reset steps. REDA has no such state, and `mode` covers them.
- **`scope`:** `project`, `sheets[]` or `selection`.

**From the schematic:**

- **`order`:** `across_then_down`, `across_then_up`, `down_then_across` or `up_then_across`. Sheets always go in Projects panel order. The leading suite can process them in another order. REDA leaves that out so that numbers follow the pages.
- **`location`:** `part` (the body centre, the default) or `designator` (where its text sits, or the body centre if it is hidden).
- **`threshold`:** a length, default 100 mil. A part this close to the first part of a row or column joins it, so fixed bands can't split two nearly level parts.
- **`per_sheet`:** `[{sheet, start_index}]`. The dialog offers none, sheet×100 and sheet×1000, which start at R101, R201 and so on. Overflow takes the next free number.
- A Component with Units on several sheets belongs to the earliest.

**From the board** ([write-through](sync.md#write-through-edits), as in the leading suite):

- **`order`:** ascending X then ascending Y, ascending X then descending Y, ascending Y then ascending X, or descending Y then ascending X.
- **`sides`:** `top_then_bottom`, `top` or `bottom`. A `selection` scope is Selected Only.
- **`threshold`** (default 100 mil) and **`start_index`**.
- **Sort position:** the edge of the courtyard's bounding box, or the Pads' with no courtyard, because v0.1 has no separate body outline. The bottom side is mirrored in X.
- **Collision:** a new number taken by an unselected Component makes the selected one take the next free number. The leading suite appends `_1` to the other part. REDA differs because that renames a part the user didn't select, and the result isn't prefix + number.
- Components with no Footprint on the board come last.

**Dialog:** the settings, a sheet grid, and an old → new preview with **Only changed**, sorting by locked or changed, and a lock switch on each row.

**Last settings:** the dialog saves its settings in `.reda` when it applies. **Re-annotate (last settings)** shows a count to confirm, then runs them, and `{saved: true}` does the same in a Command. Project Settings ▸ Schematic shows a read-only summary with Reset.

**Left out, with the reason:**

- A per-sheet Designator suffix: start indices already show the sheet, the result isn't prefix + number, and the leading suite warns that short suffixes look like Unit letters.
- Merging loose Units into one package: merging deletes Components, so Sync would drop their Footprints with placement and routing. The Unit flow above avoids most loose Units.
- Name from Position: the result isn't prefix + number.

## Port cross-references

Source: [What schematic depth does v0.1 specify beyond the editing contracts?](https://github.com/openreda/reda/issues/133).

A cross-reference is a note drawn past a Port's name that says where its partners are, such as `[2:B3, 3:A1]`. It is derived and never stored.

- **Automatic Cross References** is a Project option, off, so a Project opts in. The leading suite's docs describe enabling it and give no default.
- **Sheet Style:** `none`, `name` or `number` (default). **Location Style:** `none`, `zone` (default) or `xy`. Both are Project options.
- **Follow Cross References settings in Preferences** (off) takes the two styles from Preferences instead, as in the leading suite.
- **Partners** are Ports of the same name on other sheets, in sheet order. A Port with none shows nothing, and on a sheet with Border Zones off, `zone` shows only the sheet.
- Each Port can hide its note, and Power Ports get none.
- **Jump to partner:** `Mod`+double-click or the context menu goes to the partner's sheet and highlights the Net, with a menu when there are several. `schematic.port.partners` lists them.
- **In the PDF,** each entry is its own link to that page. The leading suite opens a pop-up list for several partners. REDA uses separate links because they work in any PDF viewer.
- Display for sheet entries and off-sheet connectors waits for v0.5.

## Deferred past v0.1

Buses; hierarchical sheets (v0.5); images, arcs and circles, rich text and tables on sheets; custom title-block templates; find-and-replace; transforming paste and paste arrays; saved selection sets; persistent per-Net colours; printing to paper.
