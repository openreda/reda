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
| All of a Net Tie's Pins on one Net, so the tie does nothing | W |
| Unit data mismatch: Designator, value, fields, Footprint choice, Component Type, In BOM, In CPL or Not Fitted differing between the Units of one Component ([model](architecture.md#design-data-model)) | E |

The pin-type matrix is built in and fixed. Individual cells can't be edited, but the whole pin-conflict check can be switched Off.

Commands keep Unit data equal, so a Unit data mismatch only comes from a hand-edited or merged file. There is no automatic fix in v0.1: the user makes the Units agree in the Properties panel, which shows `<mixed>` where they differ ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).

## DRC checks

| Check | Default |
|---|---|
| Clearance between copper of different Nets (per Net Class) | E |
| Short circuit: different Nets touching (reported apart from clearance) | E |
| `shorted_copper`: Pads with different numbers touching on a Part that isn't a Net Tie | E |
| `net_tie_not_joined`: a Net Tie's Footprint isn't a valid tie ([Net Ties](#net-ties)) | E |
| Track width outside `track_width` | E |
| Via diameter or hole outside `via_size` | E |
| Pad, Free Pad or non-plated hole outside `hole_size` | E |
| Annular ring below the minimum | E |
| Hole-to-hole spacing | E |
| Copper too close to the Board Outline or a Board Cutout | E |
| Keepout violation | E |
| Footprint outside the Board Outline | E |
| Unrouted connections (any Ratsnest left) | E |
| Copper with no Net | W |
| Dangling Tracks and Vias | W |
| Courtyard overlap | W |
| Silkscreen over exposed Pads or untented Vias, using the [resolved openings](pcb.md#mask-and-paste) | W |
| `opening_collapsed`: a mask or paste opening with nothing left after a negative value | W |
| PCB out of sync with the schematic, including differing fields, In BOM, In CPL and Not Fitted | W |
| `rule_invalid`: a [blocking rule problem](#validation) in the file, which only a hand edit or a merge can cause | E |
| `rule_outside_preset`: a rule allows less than a [Fab Preset limit](#fab-presets-and-the-rules). One Violation per rule record and preset. | W |

- Before checking, DRC refills stale Zones, so it never checks against an old fill.
- Deferred: solder-mask slivers, acid traps, silk-to-silk spacing.

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
| `edge_clearance` | `value` |
| [`mask_expansion`, `paste_expansion`](pcb.md#mask-and-paste) | These shape outputs rather than set a limit, so they have no DRC severity. |

[What do Violations carry, and what do ERC and DRC add?](https://github.com/openreda/reda/issues/128) adds the silk, courtyard and dangling values. [How deep do Zones, Keepouts and the outline editor go?](https://github.com/openreda/reda/issues/137) adds the Zone connection defaults.

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

## Net Ties

Sources: [How do Component Type, BOM inclusion and Net Ties work?](https://github.com/openreda/reda/issues/125).

A Net Tie needs no Waiver. DRC and the Track Tool share one rule, based on the Net, so they always agree, however a Track was made:

- **Pads of one tie** may touch each other, with no clearance between them.
- **Tracks and Vias on one of the tie's Nets** need no clearance to the tie's other Pads, because a Track leaving a small tie can't keep clearance from the Pad beside it. Touching another Net's Pad is still a short.
- **Everything else follows the normal rules:** other Nets, other Footprints, and Zones, even Zones on the tie's own Nets.
- **Nets stay separate.** A tie never merges Nets, and each keeps its own Ratsnest.

`net_tie_not_joined` fires when a tie's Pads don't form one connected copper group on one Layer, aren't all SMD, or have fewer than two different numbers. It and `shorted_copper` use the same geometry in the Library check and in DRC. DRC reads the placed copies, which catches a Footprint swap or a type change.

## Waivers

- A single Violation can be waived, for example an unconnected test Pin.
- **Storage:**
  - A Waiver is stored in the Document that holds the objects.
  - It records the check id and the UUIDs of the objects involved, with an optional comment.
- A Waiver stops matching once those objects no longer break that check.
- Waived Violations are hidden by default, with a toggle to show them.
