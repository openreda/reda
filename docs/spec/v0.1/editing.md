# v0.1 shared editing rules

The rules every Document editor follows: selection, snapping, the grid, Properties, numeric expressions, highlighting, the HUD and navigation. Crate: `reda-ui`, with the query Commands in `reda-engine`.

Source: [How do selection, snapping, grids and Properties work across editors?](https://github.com/openreda/reda/issues/139). Modelled on the leading suite. Each difference says why.

## Selection

- **Click:** a click selects one object and replaces the selection.
  - A click without movement on the only selected object deselects it, unless a narrower pick exists there, such as a [Wire segment](schematic.md#editing-a-placed-wire), which narrows instead.
  - A click on one object of a multi-selection selects just that object.
  - A drag never deselects.
- **Shift+click** toggles an object in or out of the selection.
- **Click Clears Selection** (a Preference, on): with it off, a click on empty space keeps the selection.
- **Rectangle:** dragged left to right, it selects what it encloses. Dragged right to left, it selects what it touches. Each direction has its own colour. **Directional Rectangle** (a Preference, on) makes every rectangle select what it encloses when off.
- **Field and Designator text** can be selected alone. Moving the parent includes it. Deleting the text alone is refused.
- **PCB Commands:** `pcb.selection.on_layer {layer}` selects everything on a Layer. `pcb.selection.net {net}` selects all copper and Pads of a Net, through a picker in the GUI.
- [Locks](pcb.md#locks) are specified with placement. With Protect Locked Objects on, click and rectangle skip locked objects.

### Selection Filter

One toggle per object kind, plus All and None, with the kind list set per Document type. It appears in [Properties with nothing selected](#properties-with-nothing-selected) and as an Active Bar pop-up, which stay in sync. `selection.filter.set` changes it.

- **It governs** click, rectangle, `Mod`+A, stack picking and Select Similar. Filtered kinds stay visible, and can't be picked or moved.
- **It doesn't govern:**
  - snapping, so the cursor still snaps to a filtered Pad
  - picks asked for explicitly: Messages, Find and cross-select
  - `selection.set` from scripts and MCP
- It applies on every canvas. The Part editor has none.

### Stacked objects

When several objects are under the pointer, the first tier with a hit wins, and the smallest object wins within a tier:

1. Pads, Vias, Tracks and text on the active Layer
2. the same kinds on other visible Layers
3. the Footprint, by its body or [courtyard](library.md#footprint-contents)
4. Zones and Keepouts
5. the Board Outline

A sheet has one Layer, so its objects rank by size alone, and a Component's body ranks after its Pins and text.

- **Cycling:** another click on the same spot, within 3 px and 600 ms, picks the next object in the stack. Shift+Tab does the same when idle. The repeat-click gesture is REDA's own addition, because stacks are constant on two-Layer boards.
- **Pick list:** a double-click on a stack opens a visual list sorted by Layer, with a live preview of the hovered entry and a lock icon, as in the leading suite. A double-click on a single object opens Properties.

## Select Similar

`objects.find_similar {template, rules, scope}` is a query Command that returns UUIDs and works headless. `rules` maps an attribute to `any`, `same`, `different` or a `*` pattern.

- **Opening:** Shift+F then a click, or the context menu.
- **Dialog:** non-modal, because the result is judged on the canvas. Rows are generated from the object schema and grouped as Kind, Object Specific and Graphical. Kind starts at Same and the rest at Any.
- **Options:** Select (on), Zoom (off), Clear Existing (on), Normal, Dim or Mask (Mask), Open Properties (off), and **Apply**, which tests the result without closing.
- **Selection Filter:** it is honoured, and the dialog won't open for a filtered kind.
- **Children:** Pads can be found. Other children of a Component or Footprint can't.
- **Scope:** sheet or Project; the board; or the current Library element.

Differences from the leading suite:

- No "open documents" scope, because a Project is loaded whole.
- No Whole Library scope, because Library elements are separate files outside the Document.
- No active-Layer limit on Zones, because no workflow wants it.

"Create Expression" comes with the query language.

## Cursor snapping

One model serves every editor. The built code has a separate one for sheets and boards.

- **Priority:** object Hotspot, then alignment guide, then grid.
- **Capture:** a length, checked on both axes, as in the leading suite. The cursor must be within it in X and in Y.
  - **PCB and Footprint:** one **Snap Distance**, default 8 mil.
  - **Schematic and Symbol:** an **Electrical Range**, set per [grid preset](#grid).
- **Hotspots:** a per-kind checklist per Document type, held as data.
  - PCB: Pad centre, Via centre, Track end, arc end, arc centre, outline vertex, Footprint origin and text anchor.
  - Schematic: Pin end, Wire end, Wire vertex, Port anchor and label anchor.
- **Object snap mode:**
  - PCB and Footprint: Active Layer, All Layers or Off. Through-hole Pads and Vias count as on every copper Layer.
  - Schematic and Symbol: On or Off.
- **Gridless:** a toggle turns the grid level off, and Hotspots keep working.
- **Suspending:** while a Tool or drag is in flight, hold `Mod` to suspend object snap, and `Mod`+Shift to suspend the grid too, as in the leading suite. If `Mod` was already held when the drag started, it means detach. Release it and press it again to suspend.
- **Alignment guides:** in v0.1 the [polygon editor](pcb.md#polygon-editor) and [Footprint moves](pcb.md#moving-footprints) use this level.
- **Marker:** a snapped point shows a marker naming what it hit.
- **Palette:** `Mod`+E opens a snap palette during any operation, with an Active Bar button for the same palette. It holds the grid toggle, the mode and the Hotspot checklist.

### Grab point

The Preferences **Object's Electrical Hot Spot** (on) and **Center of Object** (off) apply on every canvas ([schematic](schematic.md#editing)). On a board, Hot Spot holds a Footprint at its nearest Pad, and Center holds an object at its reference point. With neither, an object is held where it was clicked.

### Rotation

**Rotation Step** is a Preference, default 90°. Space rotates counter-clockwise and Shift+Space clockwise. Every move takes these keys and the mirror keys, Move Board Shape included.

## Grid

- **Presets:** editable tables per Document type, one imperial and one metric, with **Restore shipped presets**. Each row has an **In cycle** tick.
  - **Schematic and Symbol rows:** Snap, Electrical Range and Visible. Shipped: 10, 25, 50 and 100 mil with ranges 8, 20, 40 and 80 mil, plus metric equivalents. The default is 50 mil.
  - **PCB and Footprint rows:** Snap and Visible. Shipped: 0.1, 0.25, 0.5 and 1.0 mm, and 5, 10, 25 and 50 mil.
  - Visible starts equal to Snap.
- **Cycling:** G goes forward and Shift+G goes back, through the rows that are In cycle.
- **Custom value:** `Mod`+Shift+G opens a dialog that takes a value with units. Its range starts at 0.8 of the step. Cycling discards it, as in the leading suite.
- **A range at or above the step** gives a warning and isn't clamped, because the leading suite only advises against it.
- **Visible grid:** dots or lines, a coarse multiplier of 2, 5 or 10 (default 5), and a toggle key. It keeps its [coarsening and hiding](app.md#canvas-rendering).
- **Origin:** a board's grid and its coordinates run from the [Output Origin](outputs.md#output-origin), or from design zero when none is placed, as in the leading suite. Moving the origin shifts the grid and moves no objects. A sheet or Library element uses its own fixed origin.
- **Snap selection to grid** works on the PCB as well as the schematic.

## Working state

The current grid, units, object snap mode, Gridless and the Selection Filter go in `.reda/view.json`, per Document ([app](app.md#per-project-view-state)). Preferences hold only the defaults and presets.

This differs from the leading suite, which saves them in the design file. They are working state, which doesn't belong in git diffs, and viewing never dirties a Document.

Document-level grids and guides are design intent, so in v0.3 they become PCB Document data with undo. They are [reserved names](architecture.md#reserved-names) now.

## Properties

### Properties with nothing selected

Sections follow the leading suite's order, except Ready for fab?, which is REDA's own. Each one collapses.

1. **Ready for fab?** on a board, for one glance before running an Output Job: a line each, marked OK or `!`, for the Board Outline closed, unrouted connections (**Show**), Zones needing a refill (**Fill**), DRC not run since the last edit (**Run**), in sync with the schematic (**Sync**) and the Not Fitted count.
1. **Selection Filter**
2. **Snap Options:** Gridless, object snap mode, the Hotspot checklist, and the Snap Distance or Electrical Range
3. **Board** or **Sheet:**
   - Board: size, area, Component area, density, routed %, and counts of Components, Layers, Nets and primitives. Density is Component area ÷ (2 × board area). Routed % is the share of connections copper has made. `pcb.board.info` returns the same.
   - Sheet: name, size, and the title block read-only.
4. **Grid:** the current value, the preset picker, the visible grid on or off, and its style
5. **Other:** units
6. **Checks:** the [check summary](checks.md#check-summary)
7. **Links:** Board Setup or Project Settings

A Guide Manager comes with guides in v0.3. The Zone naming scheme stays in [Board Setup](app.md#settings-windows).

A running Tool's options replace all of this.

### Properties with a selection

Regions use the leading suite's names and order, then REDA's own. Each one collapses, and the open state is kept per region in `state.json`.

| Region | Component | Other objects |
|---|---|---|
| **General** | Designator, value, [Library Status](library.md#library-status) badge with **Update…**, Part name with **Replace…** and **Edit in Library**, Footprint choice, Unit rows, Not Fitted, In BOM, In CPL | Pin: number, name, type. Pad: number, `side`, and the locally edited badge with Revert |
| **Location** | X, Y, rotation, side | X, Y, rotation |
| **Parameters** | one row per field: value, visible eye, lock, autoposition | |
| **Graphical** | | Pin: length, [decorations](library.md#pins). Pad: shape. Track: [its values and the Net block](pcb.md#track-properties) |
| **Rules** | | [which rule applies](checks.md#explaining-a-rule) |
| **Checks** | Violations on the object | Violations on the object |

- **Kind chips:** a mixed selection's header shows chips with counts, such as "Wire (12) · Net Label (3)". A click narrows the selection to that kind, and a clear button restores it.
- **Mixed values** show `<mixed>`. An edit applies to every selected object.
- **A clicked Violation** selects its objects, so Properties shows those objects.
- **Units:** `Ctrl+Q` with the Panel focused toggles units for that Panel only, as in the leading suite.
- **Display precision:** Preferences, default 3 decimals for mm and 1 for mil. Rounding is for display only.

## Numeric expressions

Numeric fields in Properties accept:

- **A plain value**, with optional units. A suffix overrides the display units. `+0.1mm` means absolute +0.1 mm, as in the leading suite.
- **A relative value:** `!` is the current value, so `! + 0.1mm`, `! * 2` and `(! + 5mil) / 2` work, with `+ - * /` and brackets.
- **Other attributes of the same object**, by English schema name whatever the UI language, such as `Width + 5mil`.

On a multi-selection an expression runs per object and commits as one batch with one undo entry. Any failure flags inline and commits nothing. The GUI resolves the expression first, so the Journal records the resulting values.

Functions such as `sqrt` differ from the leading suite: they are [reserved](architecture.md#reserved-names), because no workflow here needs them yet. String substitution (`{old=new}`) is v0.2.

## Highlighting

Two visual states, as in the leading suite:

- **Dimmed:** shaded, and can still be picked and edited.
- **Masked:** greyed, and can't be hit.

**The shared highlight Preference** sets how a result is shown: select (on), zoom (on) with closeness (0.6, from fit to tight), and dim (on) with strength (0.6). A separate mask strength defaults to 0.75. Messages, Find, Select Similar, Show from a row and Cross-probe all use it. [Cross-select](sync.md#cross-select-and-net-highlighting) keeps its own switches and uses the same strengths.

- `[` and `]` change the mask and dim strength for the session only.
- **`view.highlight.clear`** (Shift+C) clears highlight, dim, mask and measurements. It never clears the selection.

### Net highlight

As in the leading suite, each canvas has its own gesture:

- **PCB:** `Mod`+click highlights a Net, Shift+`Mod`+click adds one, and `Mod`+click on empty space clears. A `Mod`+drag still detaches, because only a click without movement highlights. In the [polygon editor](pcb.md#polygon-editor), `Mod`+click on an edge of the polygon being edited still inserts a vertex.
- **Schematic:** Alt+click highlights a Net, Shift+Alt+click adds one, and the [existing clears](schematic.md#editing) stay. The context menu offers the same, because some Linux window managers take Alt+click.

## HUD

A small readout by the cursor on every canvas, drawn over the canvas like the Active Bar. It is on by default.

- **Items**, each with a checkbox in Preferences: X/Y, ΔX/ΔY from the last click, Layer and grid.
- **Keys:** Shift+H toggles it, Shift+D toggles the delta line, and Insert resets the delta, as a click does. Insert stays add-vertex in the Wire Tool, because the Tool context wins.
- **Follow cursor or parked** is a Preference only, because Shift+G is the reverse grid cycle.

It runs on every canvas because Wire and Symbol work benefit from the same deltas as the board. Hover mode is v0.2.

## Navigation

- **Zoom:**
  - PgUp and PgDn zoom at the cursor, and Shift gives fine steps.
  - `Mod`+PgDn and F fit all objects. `Mod`+Shift+F fits the sheet or board.
  - Shift+Home zooms to the selection.
  - Shift+Z then a drag zooms to an area, because the Keymap has no multi-key sequences and Z is the Zone Tool.
  - Alt+← goes to the previous view.
  - Home re-centres on the cursor.
- **Wheel:** REDA's plain wheel zooms, where the leading suite's scrolls and Ctrl+wheel zooms, because zooming is by far the most common wheel action on a canvas and right-drag already pans. A mapping page is v0.2, under [reserved names](architecture.md#reserved-names).
- **Go To:** J opens a dialog for an X/Y, absolute or as a delta, and the origin. `view.jump_to` does the same. `Mod`+Home jumps to design zero and `Mod`+End to the Output Origin.
- **Find** is the same Panel on every Document type ([schematic](schematic.md#editing)), on `Mod`+F. On a board it also matches `U1-6` for a Pad, and `pcb.find` returns the same list. With no options set, results rank exact, then contains, then ignoring case.
- **Violations:** `Mod`+Shift+F4 and `Mod`+Shift+F3 go to the next and previous Violation, from any focus.
- **Selected objects:** `view.selection.next` steps through the selection, zooming to each. It has no default key.
- **Animation:** zoom, flip and Layer-switch animation is a Preference (on), at 120 ms or less.
- **Auto-pan:** one Preference with the styles Off, Ballistic (default), Fixed Size Jump and Re-Center, a speed in px/s, and Shift for faster. The leading suite's two Shift variants and Adaptive stay out, because no workflow needs them.

## Later

Arrow keys moving the cursor, grid X ≠ Y, axis alignment guides, and HUD hover mode are v0.2. Guides, polar grids and Component-only grids are v0.3.
