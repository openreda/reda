# v0.2 Commands and file formats

Source: [What Commands and file-format additions does the v0.2 data need?](https://github.com/openreda/reda/issues/352). It builds on the v0.1 [Commands](../v0.1/architecture.md#commands), [file format](../v0.1/architecture.md#file-format) and [reserved names](../v0.1/architecture.md#reserved-names), and on every other file in this folder, each of which left its Command ids and schema to this one.

This file gives the Command ids, the additions to `.redapcb`, `.redafab`, `.redarules` and the Library files, the format version step, how a board is created with more than two Layers, and how Pad Stacks show in the Library. It decides no behaviour: what each Command does is in the file it links to.

## Format version

- Every file kind moves to `"format": 2`. `0.1.0` was released with format 1, so the v0.1 rule of editing format 1 in place has ended.
- There is one migration step, 1 to 2, golden-tested against files written by `0.1.0`.

| File | What the step does |
|---|---|
| `.redapcb` | each Dielectric gains `kind: core`; the Stackup gains `via_types` holding the through type, its `hole` and `diameter.start` taken from the board-wide `via_size` record's `preferred` values; those `preferred` values are removed from the board-wide record; every Via's `diameter` integer becomes `{start}`; a `via_span` board-wide record is added; the new board-wide records (`microvia_size`, `teardrop`, `silk_width`, `silk_text_height`) are added with their defaults |
| `.redafab`, and preset copies in `.reda` | the `stackup` seeds become one 2-Layer Construction named `2 Layer`, marked default: copper, one Core with no thickness, copper. Empty seeds give 1.6 mm and 1 oz. `rules` gains `clearance_hole` at the Generic values |
| `.redarules` | only the board-wide `via_size` `preferred` values are removed |
| `.redasch`, `.redalib`, `.redasym`, `.redafp`, `.redapart` | the number only |

- A migrated board has no `construction` name, no `layers` or `net_class_pair` records, and every Teardrop target off, so it behaves as it did.
- The reserved names that v0.2 uses leave the reserved list: Layer `role`, the Dielectric fields, `layers`, `net_class_pair`, `silk_width`, `silk_text_height`, the teardrop opt-out and the Via's `zone_connection`. The Pad's "per-Layer pad stack mode" stays reserved for the full stack.

## `.redapcb`

### Layer and Stackup

- A copper Layer gains `role`: `signal`, `plane` or `mixed`. It is left out when `signal`. An inner Layer's `name` is left out until the user names it ([layer-editing](layer-editing.md#names-and-positions)).
- The one `stackups` entry gains:

| Key | Value |
|---|---|
| `construction` | the [Construction](stackup.md#construction-name) name, left out when none |
| `via_types` | an ordered list, in the order of the Via types table |
| a dielectric entry | `{uuid, kind, name?, thickness?, material?, dk?, df?, test_frequency?}`, with `kind` as `core` or `prepreg` |

- A copper entry stays `{layer, thickness}`.

### Via type and Via

One pair of names is used everywhere a Via's size appears: `hole` and `diameter`. The other v0.2 files say `drill` and `pad` for a Via type's fields; those are column headings in the Stackup editor, not stored names.

- **Via type:** `{uuid, name?, from, to, micro?, hole, diameter {start, middle?, end?}, unused_layers?}`. `from` and `to` are Layer UUIDs.
- **Via** gains to its v0.1 fields:

| Key | Value |
|---|---|
| `diameter` | `{start, middle?, end?}` in place of the v0.1 integer ([Pad Stacks](inner-layers.md#on-a-via-and-a-via-type)) |
| `unused_layers` | `remove` or `remove_keep_ends`, left out when `keep` |
| `zone_connection` | `{style?, spokes?, spoke_width?, gap?, angle?}`, as a Pad's |
| `teardrop` | `false` when opted out, left out otherwise |

- A Via stores no reference to a Via type.

### Pad, Free Pad and Track

- A plated through-hole Pad or Free Pad gains optional `middle` and `bottom`, each `{shape, width, height, corner_ratio?, chamfer_ratio?, corners?}`. Either is refused on an SMD or non-plated Pad.
- It gains `unused_layers`: `remove`, left out when `keep`. `remove_keep_ends` is refused on a Pad.
- A placed Pad, a Free Pad and a Track gain `teardrop: false`, left out otherwise.
- [Placed Pad edits](../v0.1/pcb.md#placed-pad-edits) carry `middle`, `bottom`, `unused_layers` and `teardrop` like any other Pad property.

### Rule records

- `scope` accepts two more filters, on the kinds [layer-rules](layer-rules.md) and [teardrops](teardrops.md) allow:

| Filter | Value |
|---|---|
| `layers` | `"outer"`, `"inner"`, `"plane"`, or a list of Layer UUIDs |
| `net_class_pair` | a list of two Net Class UUIDs, sorted, which may be equal |

- New and changed kinds:

| Kind | Values |
|---|---|
| `clearance` | gains `hole {via, pad}` |
| `via_size` | `diameter {min, max?}`, `hole {min, max?}`, `aspect_ratio {max?}`; a Net Class record may add `preferred` to `diameter` and `hole` |
| `microvia_size` | as `via_size`, and `max_stack?` |
| `via_span` | no board-wide values; a Net Class record may hold `allowed`, a list of Via type UUIDs |
| `teardrop` | `through_hole_pad`, `smd_pad`, `via`, `width_change`, each `{enabled, length, max_length, width, max_width, curved, skip_ratio}` |
| `silk_width`, `silk_text_height` | `min` |

- `length`, `width` and `skip_ratio` are whole percent. The DRC severities table gains `via_span`, `microvia_size`, `aspect_ratio`, `max_stack`, `silk_width` and `silk_text_height`.

## `.redafab`

The same shape serves shipped presets, user presets and the Project's copies.

- **`constructions`** replaces the `stackup` seeds section. `stackup_suggestions` stays.

  | Key | Value |
  |---|---|
  | `name` | its identity within the preset, compared ignoring case. There is no UUID, because the board stores the name |
  | `default` | `true` on at most one Construction per Layer count |
  | `notes` | optional text |
  | `thickness` | the finished thickness |
  | `entries` | top to bottom: `{copper: thickness}` or `{kind, name?, thickness?, material?, dk?, df?, test_frequency?}` |
  | `via_types` | optional list of `{name?, from, to, micro?, hole, diameter {start, middle?, end?}, unused_layers?}`, with `from` and `to` as copper positions |

  The Layer count is the number of copper entries and is not stored.
- **`rules`** gains `clearance_hole {via, pad}`, `silk_width`, `silk_text_height`, `microvia_size` (empty in every shipped preset) and `via_aspect_ratio` (empty), and an optional `inner {track_width_min, clearance, annular_ring_min}`. The "Via preferred" seed stays and fills a new board's through type.
- **`placement_corrections`** is an ordered list. A row is `{footprint, rotation, offset?}` or `{field, value, rotation, offset?}`, with `offset` as `{x, y}`.
- **`gerber.file_names`** gains `inner_copper`.

## `.redarules`

A Rules File has no UUIDs, so the two new things that name a Layer or a Via type are written by copper position.

- The file gains `copper_layers`: the copper count of the board that wrote it.
- **`layers`:** `outer`, `inner` and `plane` are written as they are. A list of Layers is written as positions, such as `[2, 3]`.
- **`net_class_pair`** is written as two class names.
- **`allowed`** entries are written as `{from, to, micro}` in positions.
- **Import onto a board with the same copper count:**
  - positions map straight across;
  - an `allowed` entry with no Via type of that span on the board is dropped, and the preview says so;
  - if none are left, the `allowed` value is not imported, because an empty list means every type.
- **Import onto a board with a different copper count:** every record with a position list or an `allowed` list is shown with a warning and starts unticked. Ticked, a position the board lacks is dropped, and a record with none left is skipped.
- The new kinds travel like any other. Via types themselves are not in a Rules File; they travel in a Construction.

## Commands

All ids follow the v0.1 [definition](../v0.1/architecture.md#definition). A Command called with full params never asks: the dialogs call the preview query first.

### Layers and Stackup

Edit Commands on the PCB Document's undo stack, one undo entry each.

| Command | Does |
|---|---|
| `pcb.layer.add {document, split?, uuid?}` | [adds](layer-count.md#adding-a-layer) one copper Layer, splitting the Dielectric `split`, or the middle one. `rejected` at 32 |
| `pcb.layer.remove {layer}` | [removes](layer-count.md#removing-a-layer) an inner Layer with its objects, Via and Via type changes and [rule changes](layer-rules.md#when-layers-change). `rejected` for an outer Layer |
| `pcb.layer.move {layer, direction}` | [swaps](layer-count.md#moving-a-layer) it with the next inner Layer, `up` or `down` |
| `pcb.layer.set {layer, name?, role?}` | name and `role`; a `null` name clears it |
| `pcb.stackup.set` | values only: `thickness`, copper thicknesses, each Dielectric's `kind`, `name`, `thickness`, `material`, `dk`, `df` and `test_frequency`, `finish` and the colours. It no longer changes which entries exist |
| `pcb.stackup.dielectric.add {beside, uuid?}`, `pcb.stackup.dielectric.remove {dielectric}` | a further [sheet](stackup.md#several-sheets-between-two-copper-layers) between two copper Layers. Removing the only one between two Layers is `rejected` |
| `pcb.stackup.apply_construction {document, preset, construction, remove?, via_types?}` | [applies](layer-count.md#apply-construction-with-a-different-layer-count) one; `remove` lists the Layers to drop when shrinking and is required then |
| `pcb.stackup.save_as_construction {document, preset, name}` | writes the board's Stackup, and its Via types, into a Project copy of a preset, as `pcb.rules.save_as_preset` does for rules. "Save as user preset" takes it further |

Queries:

| Command | Returns |
|---|---|
| `pcb.stackup.change_preview {change}` | for a `pcb.layer.add`, `.remove`, `.move`, `pcb.stackup.apply_construction` or `pcb.via_type.set` or `.remove` given as `change`: objects deleted by kind, Vias stretched, shortened and deleted, Via types and rule records changed or removed, Net Class `allowed` lists emptied, and Vias left without a type |
| `pcb.stackup.info {document}` | resolved thicknesses, the [symmetry and thickness checks](stackup.md#checks), Z0 per Layer with its model and `≈` mark, and whether the Construction name holds |

### Via types and Vias

| Command | Does |
|---|---|
| `pcb.via_type.add {document, from, to, micro?, hole?, diameter?, name?, unused_layers?, uuid?}` | adds a type; sizes default to the through type's. A span another type has is `rejected` |
| `pcb.via_type.set {via_type, …, move_vias?}` | changes it; with a new span, `move_vias` moves the Vias on the old span |
| `pcb.via_type.remove {via_type}` | removes it and takes it out of every `allowed` list. The through type is `rejected` |
| `pcb.via_type.reorder {via_type, before?}` | its place in the table |
| `pcb.via.add` | gains `from` and `to`, defaulting to the through span, and `unused_layers`, `zone_connection` and `teardrop`. Sizes left out are the [starting size](via-types.md#sizes). It takes no type |
| `pcb.via.set` | gains `from`, `to`, `diameter {start, middle, end}`, `unused_layers`, `zone_connection` and `teardrop` |

- A length is accepted for `diameter` and means `start` alone.
- Any span between two different copper Layers is accepted on a Via. DRC's `via_span` reports one that matches no type.

### Pads, rules, Teardrops and unused copper

| Command | Change |
|---|---|
| `library.footprint.pad.add` and `.edit`, `pcb.pad.edit`, `pcb.free_pad.add` and `.edit` | take `middle`, `bottom` and `unused_layers`; the two placed ones and the Free Pad ones take `teardrop`. `null` clears `middle` or `bottom` |
| `pcb.track.set` | takes `teardrop` |
| `pcb.rules.set`, `pcb.rules.affected`, `pcb.rules.validate` | take the `layers` and `net_class_pair` scopes and the new kinds. `net_class_pair` is two class names |
| `pcb.rules.applicable` | answers per Layer where the values differ ([layer-rules](layer-rules.md#board-setup-and-explaining)) |
| `pcb.rules.apply_fab_preset`, `pcb.rules.fab_preset_diff`, `pcb.rules.save_as_preset` | cover the `inner` block and the new limits |
| `pcb.rules.export`, `pcb.rules.import` | as [above](#redarules) |
| `pcb.unused_layers.set {document, objects?, pads?, vias?}` (new, edit) | the [bulk change](inner-layers.md#unused-inner-copper): `pads` and `vias` are each a mode, on `objects` or the whole board |
| `pcb.teardrop.status {document}` (new, query) | the generated, shortened and left-out counts, and the last two as lists |

- There is no Teardrop opt-out Command of its own. Properties on a mixed selection runs a `batch`.
- **Fill Layer with plane…** runs the v0.1 `pcb.zone.add_from_board_outline`.
- `pcb.zone.fill` and `pcb.zone.fill_status` cover Teardrops as [teardrops](teardrops.md#keeping-current) says.

### Fab Presets

No new Commands.

- `fab_preset.set` and `fab_preset.set_user` take:
  - `constructions` as a patch keyed by name: a value adds or replaces that Construction whole, and `null` removes it;
  - `placement_corrections` as the whole list;
  - the new `rules` keys and `gerber.file_names.inner_copper` like any other key.
- The Constructions list on the preset's Stackup page, the Placement page and **Add correction…** all run them.
- `fab_preset.update_from_source` and `fab_preset.source_diff` merge `placement_corrections` as [placement-corrections](placement-corrections.md#the-placement-page) says. Constructions are replaced with the rest of the page.

### Layer switching

View Commands, in the app's registry.

| Command | Key |
|---|---|
| `pcb.layer.next`, `pcb.layer.previous` | `+`, `-` |
| `pcb.layer.next_signal`, `pcb.layer.previous_signal` | `*`, `Shift+*` |
| `pcb.layer.activate {position}`, with Presets `:1` to `:9` | keypad digits |
| `pcb.layer.pick` | `Mod+L` |
| `pcb.route.previous_layer` | `V` |
| `pcb.route.cycle_via`, `pcb.route.pick_via` | `6`, `8` |
| `pcb.view.via_spans` | the [Via Span label](via-types.md#display) toggle |

- In a route the Layer Commands place the proposed Via, as [layer-editing](layer-editing.md#keys) says.
- **Preferences:** `pcb.layer.hover_solo` (on by default), and the colour keys `inner_1` to `inner_30` per theme.
- **`.reda/view.json`** holds the Previous Layer and the Via choice per Layer pair.

## Creating a board

- `project.new` and `document.add {kind: pcb}` both gain `layers?` (2 to 32, default 2) and `construction?` (a name in the preset).
- `document.add {kind: pcb}` also gains `fab_preset?`: one of the Project's preset copies, or a shipped or user preset by name. Left out, it is Generic, as in v0.1. A re-added board can so start from the Project's fab.
- **Which Stackup the board gets:**
  1. the named `construction`, which must have `layers` copper Layers or the call is `invalid_params`;
  2. otherwise the preset's default Construction for that count;
  3. otherwise Generic's evenly spaced build, with no Construction name.
- The through Via type is seeded from the preset's "Via preferred" values. A Construction's own Via types are applied when it has any.
- The dialogs are as [layer-count](layer-count.md#choosing-the-count-for-a-new-board) says.

## Pad Stacks in the Library

- **Footprint editor:** a plated through-hole Pad's Properties show three copper groups, Top, Middle and Bottom. Middle and Bottom read "same as Top" until set, and each has a clear button.
- **Canvas:** the editor draws the group the active Layer selects. It gains a Middle entry among its copper Layers while any Pad of the Footprint has a `middle` shape.
- **Generators** never set `middle`, `bottom` or `unused_layers`.
- **KiCad import:**

  | In the KiCad file | In REDA |
  |---|---|
  | padstack mode `front_inner_back` | top, `middle` and `bottom` |
  | padstack mode `custom` | the front, first inner and back shapes, with a warning |
  | `remove_unused_layers` | `unused_layers: remove` |
  | `keep_end_layers` | ignored: a Pad's top and bottom always stay |

- **Library check:** a `middle` or `bottom` shape smaller than the hole is reported with the Pad's other [hole checks](../v0.1/library.md#library-check).
- The table of Pads stays in v0.6.

## Left to other tickets

- Goldens for the migration step, the new schema and the new Commands: [Which tests does v0.2 add for multilayer boards?](https://github.com/openreda/reda/issues/353).
- The values in the shipped presets' `constructions`, `silk_width` and `silk_text_height`: the shipped Fab Preset data on the [v0.2 map](https://github.com/openreda/reda/issues/319).
- Replacing `drill` and `pad` with `hole` and `diameter` where [via-types](via-types.md) and [inner-layers](inner-layers.md) name a Via type's stored fields: when the spec is assembled.
