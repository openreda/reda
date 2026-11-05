# v0.1 ERC and DRC

Sources: [What are the v0.1 ERC and DRC rule sets?](https://github.com/openreda/reda/issues/15), [What do Violations carry, and what do ERC and DRC add?](https://github.com/openreda/reda/issues/128). ERC lives in `reda-schematic` and DRC in `reda-pcb`. The Library check is in [library](library.md#library-check).

## ERC checks

Default severity: **E** = Error, **W** = Warning.

| Check | Default |
|---|---|
| `unconnected_pin`: a Pin connected to nothing. A Pin with a No-Connect Marker, or on an unplaced Unit, is exempt. | [Pin Matrix](#pin-matrix) |
| `pin_conflict`: two Pins or Ports on a Net whose types conflict | [Pin Matrix](#pin-matrix) |
| Undriven power: a Net with Power In Pins and no Power Out Pin. A Power Port counts as a driver, so no "power flag" symbol is needed. | W |
| `no_driving_source`: a Net with Input Pins and no Output, Bidirectional, Tri-state, Open Collector, Open Emitter or Power Out Pin, and no Power Port | Off |
| Single-Pin Net. Nets with a No-Connect Marker, and Nets whose only Pin is on a Mechanical Component, are exempt. | W |
| Dangling Wire end. A Wire with both ends dangling is one Violation. | W |
| Net Label, Port or Power Port attached to nothing | W |
| No-Connect Marker on a connected Pin | W |
| `net_names_on_sheet`: one Net with different names on one sheet. The message lists every name and says which won and why. Names differing only in case are one name. | E |
| `net_names_across_sheets`: one Net with different names on different sheets, reported the same way | W |
| `duplicate_nets`: different Nets with the same board name, such as one Net Label on two sheets. It [blocks Sync](sync.md#duplicate-nets), and [Append Sheet Numbers](schematic.md#connectivity) is one fix. | E |
| Port on only one sheet | W |
| `port_no_driver`: a Net whose Ports are all `in` | Off |
| Duplicate Designators | E |
| Unplaced Units of a placed multi-unit Component | W |
| `unit_placed_twice`: one Unit of a Component placed more than once | E |
| `no_footprint_choice`: a Standard or Net Tie Component with no Footprint choice | W |
| All of a Net Tie's Pins on one Net, so the tie does nothing | W |
| Unit data mismatch: Designator, value, fields, Footprint choice, Component Type, In BOM, In CPL or Not Fitted differing between the Units of one Component ([model](architecture.md#design-data-model)) | E |
| `off_grid`: a Pin or Wire vertex off the Project's connection grid | W |
| `off_sheet`: an object outside the sheet border | W |
| `library_out_of_date`: a placed Component whose [Library status](library.md#library-status) is Library Differs | Off |

There is no Fatal level, because [Output Job blocks](outputs.md#what-blocks-a-run-and-what-only-warns) do that job.

Commands keep Unit data equal, so a Unit data mismatch only comes from a hand-edited or merged file. There is no automatic fix in v0.1: the user makes the Units agree in the Properties panel, which shows `<mixed>` where they differ ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).

### Pin Matrix

The Pin Matrix sets the severity of `unconnected_pin` and `pin_conflict`, one cell at a time. Neither check has its own severity.

- **Rows and columns:** the ten Pin types and Port `in`, `out` and `bidirectional`, plus an Unconnected column for Pin types. The matrix is symmetric.
- **Each cell** is Error, Warning or Off. The cells that aren't Off by default:

  | Cell | Default |
  |---|---|
  | Output↔Output, Output↔Power Out, Power Out↔Power Out | E |
  | Tri-state, Open Collector or Open Emitter↔Output or Power Out | W |
  | Bidirectional↔Power Out | W |
  | Open Collector↔Open Emitter or Tri-state, Open Emitter↔Tri-state | W |
  | Input or Power In↔Unconnected | E |
  | Output, Bidirectional, Tri-state, Passive, Power Out, Open Collector or Open Emitter↔Unconnected | W |
  | Output, Power Out or Bidirectional↔Port `in`, on one sheet | W |
  | Port `out`↔Port `out` | W |

- **What is compared:**
  - Pin to Pin and Port to Port across the whole Net.
  - Pin to Port only within one sheet, because a driver on one sheet and an `in` Port on another is how a signal crosses sheets.
- **Storage:** `erc.pin_matrix_overrides` in `.reda` holds only cells that differ from the default, as `{"output|output": "warning", "passive|unconnected": "off"}`. Each key's two names follow the Pin type table's order, so a pair has one spelling.
- **`erc.set_pin_matrix {cells: [{a, b, severity | null}]}`** sets one cell, many or all, as one undo step. A null severity resets the cell.
- **The page:** [Project Settings ▸ ERC](app.md#settings-dialogs) draws one triangle. A click cycles Off, Warning, Error. Each cell shows its level by colour and a letter, a dot when it isn't default, and a hover sentence such as "Output Pin connected to Output Pin: Error". Right-click offers All Off, All Warning, All Error and Default.
- **From Messages:** **Set severity** on one of these Violations changes that pair's cell, and the menu says so.

## DRC checks

| Check | Default |
|---|---|
| Clearance between copper of different Nets (per Net Class) | E |
| Short circuit: different Nets touching. One Violation per Net pair and Layer, with the contact point, and no clearance Violation for that pair. | E |
| `shorted_copper`: Pads with different numbers touching on a Part that isn't a Net Tie | E |
| `net_tie_not_joined`: a Net Tie's Footprint isn't a valid tie ([Net Ties](#net-ties)) | E |
| Track width outside `track_width` | E |
| Via diameter or hole outside `via_size` | E |
| Pad, Free Pad or non-plated hole outside `hole_size` | E |
| Annular ring below the minimum. A land smaller than its hole has its own message. | E |
| Hole-to-hole spacing. Coincident holes have their own message. | E |
| Copper too close to the Board Outline, a Board Cutout or the edge of a non-plated hole, using `edge_clearance` | E |
| Keepout violation | E |
| Footprint outside the Board Outline, using its courtyard or the [fallback box](#courtyards) | E |
| Unrouted connections (any Ratsnest left), with completion %, sub-net count and unrouted length as arguments | E |
| Copper with no Net | W |
| Dangling Tracks and Vias, ignoring stubs up to the `dangling` tolerance | W |
| `poor_connection`: a Track end touches copper, but its centre lies outside that copper | W |
| `via_under_smd`: a Via inside an SMD Pad. Off means allowed. | W |
| Courtyard overlap, closer than `courtyard_clearance`. [Courtyards](#courtyards) that exactly touch pass at 0. | W |
| `silk_to_mask`: silkscreen closer than `silk_to_mask` to exposed copper or to a mask opening, using the [resolved openings](pcb.md#mask-and-paste) | W |
| `silk_to_edge`: silkscreen closer than `silk_to_edge` to the Board Outline or a Board Cutout | W |
| `zero_length_track`, `micro_segment`, `duplicate_copper` and `zero_area_zone`: [health checks](#health-checks) | W |
| `opening_collapsed`: a mask or paste opening with nothing left after a negative value | W |
| PCB out of sync with the schematic, including differing fields, In BOM, In CPL and Not Fitted | W |
| `rule_invalid`: a [blocking rule problem](#validation) in the file, which only a hand edit or a merge can cause | E |
| `rule_outside_preset`: a rule allows less than a [Fab Preset limit](#fab-presets-and-the-rules). One Violation per rule record and preset. | W |

- Before checking, DRC refills stale Zones, so it never checks against an old fill.
- Deferred: solder-mask slivers, acid traps, silk-to-silk spacing.

### Courtyards

A courtyard is always [closed shapes](library.md#footprint-contents), so the overlap check never meets an open one. A Footprint with no courtyard uses a fallback box around its copper, silk and mechanical shapes, ignoring Designator text, so it can't escape the courtyard and outline checks.

### Health checks

Health checks find data that passes the other checks but harms outputs. They are ordinary DRC checks with one severity list and one report.

- **Run on open:** only this group runs when a board opens. It never opens Messages or dirties the file.
- **`drc.fix {check, objects?}`** removes the zero-length Tracks, micro-segments, duplicates or zero-area Zones it is given, or all of them, as one undo step.
- A rotation stored as 360° is normalised on load, so it needs no check.
- The [check summary](#check-summary) lists them with **Fix**.

## Design rules

Sources: [Which structural format changes does v0.1 make now?](https://github.com/openreda/reda/issues/124), [How are design rules entered, reviewed and explained?](https://github.com/openreda/reda/issues/127), [ADR-0012](../../adr/0012-design-rules-are-scoped-records.md).

### Rule records

The PCB Document holds one `rules` table of `{uuid, kind, scope, values, enabled?, comment?}`.

- `scope` is an object of optional filters that must all match, in the style of `objects.query`. Empty means board-wide. v0.1 accepts only empty or `{net_class}`, on every kind.
- **Board-wide records:** exactly one per kind, which can't be deleted. Each holds every value its kind requires.
- **Net Class records:** at most one per class and kind. Each holds only the values it overrides.
- `enabled` is left out when true.

| Kind | Values |
|---|---|
| `clearance` | `value`, `ignore_pad_to_pad` (within one Footprint, default off) |
| `track_width` | `min`, `preferred`, `max?` |
| `via_size` | `diameter {min, preferred, max?}`, `hole {min, preferred, max?}` |
| `hole_size` | `min`, `max`. Pad, Free Pad and non-plated holes. Via holes use `via_size`. |
| `annular_ring` | `min` |
| `hole_to_hole` | `min` |
| `edge_clearance` | `value`. Also applies to the edges of non-plated holes. |
| `silk_to_edge` | `value`, seeded at 0.254 mm |
| `silk_to_mask` | `value`, seeded at 0.10 mm; `mode`, `exposed_copper` (default) or `mask_opening` |
| `courtyard_clearance` | `value`, default 0 |
| `dangling` | `tolerance`, default 0 |
| `micro_segment` | `max_length`, default 0.01 mm |
| [`mask_expansion`, `paste_expansion`](pcb.md#mask-and-paste) | These shape outputs rather than set a limit, so they have no DRC severity. |

[How deep do Zones, Keepouts and the outline editor go?](https://github.com/openreda/reda/issues/137) adds the Zone connection defaults.

### Which value applies

- **For one Net, value by value:** the highest-ranked Net Class on the Net with an enabled record that sets the value wins. Otherwise the board-wide record applies.
- **An object with no Net** uses the board-wide record.
- **Between two objects** (`clearance`, `hole_to_hole`): each one's value is resolved, and the larger applies.
- **A disabled Net Class record** is ignored.
- **A disabled board-wide record:**
  - Limit kinds: the limit is absent, so DRC skips it and the Track Tool stops clipping, unless an enabled class record sets the value. `preferred` values still supply routing defaults.
  - `mask_expansion` and `paste_expansion`: expansion 0 and Vias untented, unless an enabled class record sets the value.
  - A check's severity stays separate: it sets how the check reports, and `enabled` sets whether a record counts.
- **Min, preferred and max:** the Track Tool uses `preferred` and clips typed values to the range ([routing](pcb.md#routing)). DRC checks `min` and `max`. Commands and the Properties panel accept any positive value, and DRC flags it.
- **Changing a rule leaves existing copper alone.** DRC flags what breaks the new limits, and bulk changes use selection and Properties.
- There is no rule-query language and no per-Layer or per-area rules in v0.1. Rule records leave room for both.
- The routing Tool enforces clearance live.

### Net Classes

- A Net Class is a name and a `rank`. Its values are rule records scoped to it. Ranks are set by dragging rows in the Net Classes grid.
- A Net holds a list of `classes`, so it can be in several.
- **The Default Net Class** is the class every Net is in. It isn't stored, can't be renamed or deleted, ranks lowest, and its values are the board-wide records. A Net with an empty list is in Default only, and new Nets start that way.
- Deleting a class removes it from its Nets and deletes its records.
- **Assignment:** `pcb.net.add_to_class` and `pcb.net.remove_from_class`, each `{nets | pattern, class}`, from the Net table in Board Setup or from the Properties panel for a selected Track, Via, Pad, Zone or highlighted Net. A multi-selection applies to all their Nets.
- **`pattern`** is a `*` and `?` glob resolved once, on the Nets that exist then. A Net that Sync adds later is not matched, so there are no stored pattern rules.

### Validation

One function serves the dialog, `pcb.rules.set` and `pcb.rules.validate`.

- **Blocking:** a negative value, `min` above `max`, or `preferred` outside the range, judged on a record's effective values against the board-wide record. Board Setup shows it in red and disables OK and Apply, and `pcb.rules.set` refuses it.
- **Notes**, which never block:
  - a value outside the Fab Preset limit
  - a class weaker than the board
  - a class with no Nets
  - an override equal to what it inherits
  - a disabled `mask_expansion` or `paste_expansion` board-wide record, which changes outputs
  - a Net whose classes combine into an invalid range, naming the Net. Neither class is wrong alone. The Track Tool clips `preferred` into the range.
- A file with a blocking problem still opens, because a merge can produce one and the user must be able to fix it. DRC reports it as `rule_invalid`. The load check still refuses a missing or duplicate board-wide record.

### Explaining a rule

- **`pcb.rules.applicable {objects}`** takes one or two objects and returns, per kind:
  - the winning values and the source of each
  - the records that lost, and any disabled ones
  - the check's severity
  - for two objects, each one's clearance and which applied
- **The Properties panel** has a Rules section with the same result: a tick on winners, a cross on losers and strike-through on disabled records. Each row opens its Board Setup page. The section hides above two objects. Right-click **Applicable Rules** goes to it.
- **`pcb.rules.affected {rule | net_class}`** returns the objects a rule or class affects, leaving out those another record overrides. Board Setup's **Show on board** uses it ([app](app.md#settings-dialogs)).

### Fab Presets and the rules

- A Fab Preset holds flat limits with no scope, and seeds for the rest. It seeds the board-wide records when the board is created. Only limits are compared with the board, so `preferred` never produces a finding.
- **Seeds** for 2-layer boards come from the [fab research](https://github.com/openreda/reda/issues/4). The Generic preset is conservative (0.20 mm Track, 0.16 mm clearance, 0.30 mm Via drill…), and the per-fab presets are looser. All values in mm, and Track and Via maximums start empty:

  | Preset | Track preferred | Via preferred (diameter / hole) | Via hole min | `hole_size` |
  |---|---|---|---|---|
  | Generic | 0.25 | 0.70 / 0.30 | 0.30 | 0.50 to 6.30 |
  | JLCPCB | 0.25 | 0.60 / 0.30 | 0.15 | 0.20 to 6.30 |
  | PCBWay | 0.25 | 0.60 / 0.30 | 0.15 | 0.20 to 6.30 |

- **Which preset:** a rule is compared with every Fab Preset used by an Output Job for this PCB Document. Board Setup shows the strictest limit beside each value and names its preset. With no jobs it shows nothing.
- **Apply Fab Preset rules** re-seeds the board-wide records after a preview listing "board 0.15 → preset 0.127" with a tick per row. Limit rows start ticked. `preferred` rows start ticked only if they'd fall outside the new range. Class records never change, and are only flagged if they now break [validation](#validation). `pcb.rules.apply_fab_preset` takes the chosen rows.
- Output Jobs never change the rules ([outputs](outputs.md#fab-presets)).

### Moving rules between boards

- **Save rules as Fab Preset** (`pcb.rules.save_as_preset`) writes the board-wide values only, because presets have no scope. The dialog says that classes are left out.
- **Export and import** (`pcb.rules.export`, `pcb.rules.import`) use a `.redarules` file holding rule records, DRC severities and class names, matched by name.
  - It holds no Net membership, because Net names differ per board.
  - Import previews each overwrite, lets the user pick kinds, and creates missing classes empty.

### Setting rules

`pcb.rules.set {kind, scope?, values, enabled?, comment?}` sets the given values. A null value clears it, and a class record with nothing left is deleted. It fills in nothing, and refuses a blocking result, so agents stay explicit.

## Severities

- Each check is set to Error, Warning or Off.
- ERC severities are stored in the Project (`.reda`). DRC severities are stored in the PCB Document, next to its rules.
- **Where they are edited** ([How are Projects, Documents and board setup created and edited in v0.1?](https://github.com/openreda/reda/issues/38)):
  - ERC severities on the ERC page of **Project Settings**, in a grouped grid with multi-select **Set severity** and **Reset to defaults**. The [Pin Matrix](#pin-matrix) checks show as "Set by matrix", linking to it.
  - DRC severities in **Board Setup**: on each rule's page beside the rule, and all together on its DRC page ([app](app.md#settings-dialogs)).
  - Right-clicking a Violation in Messages offers **Set severity of "<check>" → Error / Warning / Off**, next to **Waive**.

## Running

- On demand, through `erc.run` and `drc.run` (query Commands), from the GUI, the CLI and MCP.
- There is no background online DRC in v0.1.
- **Parameters:**
  - `drc.run {objects?, area?, layers?, checks?, include_waived?}`
  - `erc.run {documents?, objects?, checks?, include_waived?}`
- **Scope:** a Violation is in scope when any of its objects is. The whole design is still checked for connectivity, then the result is filtered. A narrowed run returns `partial: true`.
- **Never stops at a count.** Results are never capped. Only the [markers drawn](#canvas-markers) are.
- **The Run dialog** holds the check ticks, scope and report path, with its last choices in per-Project view state, so a user can test in stages. Severities stay in Board Setup and Project Settings.
- **Partial runs in the GUI** replace only the Violations that run covered.
- **Staleness:** results stay on show until the next run or `view.checks.clear`. `erc.stale` and `drc.stale` mark them stale once the design is edited after the run, and undoing back to the checked design clears the mark.
- **`view.checks.clear`** clears the list and markers. It is view state and resolves nothing, so the next run lists them again.

## Reporting

### Violations

A run produces a list of Violations. The UI turns the check id and arguments into text ([i18n](app.md#internationalisation)). The Command result carries the same structured list, so agents can read it.

| Field | Meaning |
|---|---|
| `check` | The check id |
| `severity` | Error or Warning |
| `args` | Structured message arguments |
| `objects` | The UUIDs of the objects involved, the most specific last |
| `waiver?` | The Waiver that waives it |
| `measured?`, `required?` | A tagged value, such as `{"length": 120000}` in nm. v0.1 has `length` and `area`. |
| `limit?` | `min` when `measured` is below `required`, `max` when it is above |
| `at?`, `document?` | The marker point and the Document it is in |
| `points?` | The two closest points, on distance checks |
| `layers?` | The Layers involved |
| `rule?` | The UUID of the rule record that set the limit. A board-wide limit names the board-wide record. Where a pair's two values tie, it names the first object's record. |

What each kind of check fills in:

| Checks | `measured`, `required`, `limit` | `points` | `layers` | `rule` |
|---|---|---|---|---|
| `clearance`, `hole_to_hole`, `edge_clearance`, `silk_to_edge`, `silk_to_mask`, courtyard overlap | yes | yes | yes | yes |
| `track_width`, `via_size`, `hole_size`, `annular_ring`, `micro_segment` | yes | | yes | yes |
| Dangling Tracks and Vias | | | yes | yes |
| `rule_outside_preset`: the rule's value against the preset limit, with `preset` in `args` | yes | | | yes |
| `rule_invalid` | | | | yes |
| Short circuit, `poor_connection`, `via_under_smd` and other DRC checks | | | yes | |
| ERC checks | | | | |

`at` and `document` are set wherever the Violation has a place, which is every check except `rule_outside_preset` and `rule_invalid`.

### Canvas markers

- **Detail marker:** a ring at `at`, at every zoom. Zoomed in far enough to read, it adds the value, over or under, and a line between the closest points. There are no per-check glyphs.
- **Overlay:** colours the whole offending object, in a style set in [Preferences](app.md#preferences).
- **Per-check display:** Preferences has a grid with Details and Overlay columns per check. Details are on for all, and Overlay is on for clearance, Track width and courtyard overlap. The grid never changes what is detected.
- **Layers:** markers draw only on the Layers involved. Clicking one makes its Layer active, if shown.
- **Waived Violations** draw muted, with a view toggle that is on by default. On a sheet, this muted marker is how a Waiver shows.
- **Stale results** draw dimmed, and a deleted object's marker stays at `at`.
- **Cap:** the `checks.max_markers` Preference (500) caps markers drawn, with "N not shown".

### Messages panel

- **The list:** Group By Document (default), check or severity, with sorting, severity chips and next and previous Violation keys.
- **Clicking a Violation** opens its Document, zooms to it and selects the objects involved.
- **Details pane:** under the list, the selected Violation's message and its objects, each clickable.
- **Right-click:** Details, Waive, Unwaive, Set severity, Run this check only, and Show rule.
- **Auto-open:** the panel opens only when a run finds Errors.
- **On the canvas:** right-click an object ▸ Violations, and a key that lists the Violations under the cursor.
- **Groups:** waived Violations and stale Waivers each have a collapsed group.

### Violation Details

A window, opened by double-clicking a Violation or from its right-click menu. The canvas stays usable while it is open, so Highlight and Jump can be seen working.

- the check, and measured against required
- the rule, with a link to its Board Setup page
- the objects, with their Layers and locations
- **Highlight** and **Jump**
- the Waiver's author, date and reason, which can be edited there, with **Waive** and **Unwaive**

### Check summary

Properties with nothing selected has a Checks section: last run time, stale flag, Error, Warning and waived counts, **Run**, and [health checks](#health-checks) with **Fix**. A board shows DRC and a sheet shows ERC.

### Report files

`checks.report {kind, path, format, ...}` runs a check with the same parameters as its run Command and writes a file. `erc.run` and `drc.run` stay pure queries.

- **Formats:** JSON, and HTML with a per-check summary and waived, stale-Waiver and health sections.
- **Export** in Messages and in the [Change List dialog](sync.md#change-list-dialog) uses it. `kind: sync` writes the Change List from `sync.preview`, with each row's status.
- HTML rows don't cross-probe in v0.1, because that needs a registered URL handler.

## Net Ties

Sources: [How do Component Type, BOM inclusion and Net Ties work?](https://github.com/openreda/reda/issues/125).

A Net Tie needs no Waiver. DRC and the Track Tool share one rule, based on the Net, so they always agree, however a Track was made:

- **Pads of one tie** may touch each other, with no clearance between them.
- **Tracks and Vias on one of the tie's Nets** need no clearance to the tie's other Pads, because a Track leaving a small tie can't keep clearance from the Pad beside it. Touching another Net's Pad is still a short.
- **Everything else follows the normal rules:** other Nets, other Footprints, and Zones, even Zones on the tie's own Nets.
- **Nets stay separate.** A tie never merges Nets, and each keeps its own Ratsnest.

`net_tie_not_joined` fires when a tie's Pads don't form one connected copper group on one Layer, aren't all SMD on one side, or have fewer than two different numbers. It and `shorted_copper` use the same geometry in the Library check and in DRC. DRC reads the placed copies, which catches a Footprint swap or a type change.

## Waivers

- **Waiving:** `drc.waive` and `erc.waive` take a list of Violations with one reason, as one undo step. In Messages, Waive opens a small window for the reason, covering everything selected. An unconnected test Pin is a typical case, and one connector whose silk crosses the edge is a typical batch.
- **Unwaiving:** `drc.unwaive` and `erc.unwaive`, from Messages or Violation Details.
- **Storage:**
  - A Waiver is stored in the Document that holds the objects.
  - It records the check id and the UUIDs of the objects involved, plus `author`, `created` and an optional `reason`.
  - `author` comes from the `user.name` [Preference](app.md#preferences), which falls back to the OS user name. `created` comes from the Clock.
- **Re-applied each run:** everything is always detected, and Waivers are laid over the result. A Waiver stops matching once those objects no longer break that check.
- **Stale Waivers:** a Waiver is stale when its check ran across the whole design and it matched nothing. A scoped run, or a check set to Off, makes no Waiver stale. Stale Waivers have their own group and report section, with **Delete**. They are never removed automatically, because an undo or a revert can make one match again.
