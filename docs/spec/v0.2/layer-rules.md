# v0.2 Design rules by Layer and by Net Class pair

Source: [How do design rules vary by Layer?](https://github.com/openreda/reda/issues/336). It builds on the v0.1 [rule records](../v0.1/checks.md#rule-records) and [ADR-0012](../../adr/0012-design-rules-are-scoped-records.md), on the [Stackup](stackup.md), [changing the Layer count](layer-count.md) and [inner Layers](inner-layers.md), and on the research into [rules that vary by Layer in other tools](https://github.com/openreda/reda/issues/325).

This file says how a design rule can differ from one copper Layer to another, how a clearance can be set between two Net Classes, which record wins, what Fab Presets hold for inner Layers, what happens to a rule when its Layer goes, and the two silkscreen minimums. The Command ids and the schema are decided in their own work on the [v0.2 map](https://github.com/openreda/reda/issues/319).

A v0.1 board needs no change: a board with no `layers` and no `net_class_pair` records resolves exactly as in v0.1, and a new board starts with none.

## The `layers` scope

`layers` is a scope filter on a rule record, beside `net_class`. It is not a table of per-Layer values inside a record.

- It takes one of three forms:

  | Form | Matches |
  |---|---|
  | `outer` or `inner` | the two outer copper Layers, or every copper Layer between them |
  | `plane` | every copper Layer whose [`role`](stackup.md) is plane |
  | a list of Layer UUIDs | those copper Layers |

- **A record matches by the Layer where the thing is measured,** not by the Layers an object belongs to.
  - A Track uses its own Layer.
  - A Via or a through-hole Pad is judged Layer by Layer: its ring, its clearance and its `clearance.hole` on L3 use the records for L3.
  - A Zone connection uses the Zone's Layer.
- **Four kinds accept it:** `track_width`, `clearance` (`value` and `hole {via, pad}`), `annular_ring` and `zone_connection`. On any other kind a `layers` scope is refused.
  - `via_size` and `microvia_size` follow the [Via type](via-types.md). `hole_size`, `hole_to_hole` and `aspect_ratio` are about a hole, which passes through Layers and has no single one.
  - `edge_clearance` is the next candidate if a need appears. No fab researched publishes an inner figure for it.
- A `layers` record holds only the values it overrides, like a Net Class record. That includes a board-wide one, which can be deleted: the undeletable board-wide record per kind is still the one with an empty scope.
- `layers` combines with `net_class` or with `net_class_pair`, so "the 50 Ω class on inner Layers" is one record.

## The `net_class_pair` scope

A record scoped to `net_class_pair` sets the clearance between the Nets of two Net Classes.

- Only `clearance` accepts it. It covers `value` and `hole`, value by value.
- **The pair is unordered,** and the same class twice is allowed. The Default Net Class may be one side.
  - `{HV, Default}` is "HV to every Net".
  - `{HV, HV}` is "HV to HV".
- **A matching pair record replaces "the larger of the two objects' values".** It may loosen as well as tighten. Board Setup notes a pair value lower than either class's own.
- `zero_keepout_clearance` stays board-wide only. `ignore_pad_to_pad` is not a pair value.

## Which value applies

This extends the v0.1 [order](../v0.1/checks.md#which-value-applies). There is still no priority field. Everything is resolved value by value, on one Layer.

**For one Net on one Layer:**

1. The highest-ranked Net Class on the Net with an enabled record that sets the value for this Layer wins. Within that class, a record with `layers` beats the one without.
2. Otherwise the board-wide records apply, a record with `layers` before the one without.
3. Among records with `layers`, a named Layer beats `plane`, which beats `inner` or `outer`.

Net Class comes before Layer. A Net Class says what a Net needs and a Layer rule says what the fab can do there, so a board-wide Layer rule never undoes a class.

- Example: the board allows 0.125 mm Tracks on inner Layers and the Power class sets a 0.5 mm minimum. A Power Track on L2 keeps 0.5 mm.
- Example: a 50 Ω class that needs 0.3 mm outside and 0.2 mm inside holds two records, `{net_class, layers: outer}` and `{net_class, layers: inner}`.

**Between two objects** (`clearance`):

1. If any enabled pair record matches the two Nets and sets the value for this Layer, the pair whose higher-ranked class ranks highest wins, and the other class breaks a tie. So `{HV, HV}` beats `{HV, Default}`. Within that pair, `layers` records come first, in the order above.
2. Otherwise each object's value is resolved as for one Net, and the larger applies, as in v0.1.

A disabled record is skipped and the search goes on. `hole_to_hole` is unchanged.

## Validation

These join the v0.1 [validation](../v0.1/checks.md#validation).

- **Blocking:**
  - Per kind and per owner (board-wide, a Net Class or a pair), a Layer named in two records, or a second record for `outer`, `inner` or `plane`. With one record per form the order above always gives one winner.
  - A `layers` list naming a Layer that is not a copper Layer of the board.
  - `min` above `max`, or `preferred` outside the range, judged on the effective values on each copper Layer.
- **Notes:**
  - a record that matches no Layer, such as `inner` on a 2-Layer board or `plane` with no plane Layers
  - a Layer record equal to what it inherits
  - a pair value lower than either class's own

## When Layers change

- **[Removing a Layer](layer-count.md):** a record naming Layers by UUID loses the removed Layer, and a record with none left is deleted. The confirmation counts both ("2 rules changed, 1 rule deleted"). It is part of the same Command, so undo restores them.
- `outer`, `inner` and `plane` records are never touched by a Layer count change. A board can drop to 2 Layers and come back with its inner rules intact.
- Changing a Layer's `role` makes `plane` records start or stop applying to it, with no confirmation.
- Adding or moving a Layer changes no record.

## Routing

- **The Track Tool follows the Layer.** When it changes Layer through a Via:
  - a Track at the `preferred` width takes the new Layer's `preferred`
  - a width the user chose is kept, clipped into the new Layer's range
- Live clearance uses the values for the Layer being routed on.
- As in v0.1, changing a rule leaves existing copper alone.

## Fab Presets

ADR-0012 said a Fab Preset holds flat limits with no scope. v0.2 adds one exception.

- A Fab Preset may hold an optional `inner` block with `track_width.min`, `clearance.value` and `annular_ring.min`. Nothing finer: fabs publish inner and outer figures, never one per Layer.
- **Where it is set:**
  - a new board is seeded with board-wide `{layers: inner}` records
  - `rule_outside_preset` and the limit shown in Board Setup compare inner Layers with the inner limits
  - **Apply Fab Preset rules** previews the inner rows with the rest
- **Where it is absent,** the flat limits apply on every Layer.
- **Save rules as Fab Preset** writes the board-wide `inner` records into the block. Other Layer records, like classes, are left out, and the dialog says so.
- **No shipped preset sets it.** JLCPCB and OSH Park publish one figure for all Layers. PCBWay's inner figure (4 mil spacing against 5 mil outside) belongs to one copper weight and process, so its flat limit stays the stricter one. A user's own preset for a fab with real inner limits (AISLER: 125 µm inside, 175 µm outside) uses the block.
- **A new board therefore has no `layers` records,** for any kind. Zone connections are relief on every Layer, and solid on plane Layers is the user's one record to add. [`clearance.hole`](inner-layers.md#clearance-to-a-hole) already carries the fabs' inner-Layer hole limits as flat values.

## Board Setup and explaining

The exact layout is left to the build.

- **Layer rows:** each of the four kinds shows its board-wide row with its Layer rows under it. **Add Layer rule** offers Outer, Inner, Plane or picked Layers. A row in the Net Classes grid expands to the same child rows.
- **Class pairs** have their own grid on the Clearance page.
- **By Layer** is a read-only view listing every copper Layer against the resolved values for a chosen Net Class, so "what applies on L3?" can be read without working through the order.
- **`pcb.rules.applicable`** and the Properties panel's Rules section give the result for a Via or a through-hole Pad per Layer where the values differ, and once where they don't. A pair record shows as the winner over both objects' own values.
- `pcb.rules.set` and `pcb.rules.affected` take the new scopes like any other.
- **Rules Files** hold `layers` and `net_class_pair` records. The group forms and class names travel as they are. How a named Layer is matched on another board is part of the Command and file-format work.

## The impedance readout's width

The [Stackup editor's figure](stackup.md) for a copper Layer uses the Default Net Class's `track_width.preferred` as resolved on that Layer: a named Layer, then `plane`, then `inner` or `outer`, then the board-wide record. The width is shown beside the figure.

## Silkscreen minimums

The two rule kinds reserved in v0.1 arrive.

| Kind | Values | Check | Default |
|---|---|---|---|
| `silk_width` | `min` | a silkscreen line, arc or text stroke `thickness` below it | W |
| `silk_text_height` | `min` | silkscreen text `height` below it | W |

- Both are board-wide only, with one value for both sides. Fabs publish one figure, and they take no `layers` or `net_class` scope.
- They cover free objects and Footprint silkscreen alike.
- Text in an outline font has no stroke, so `silk_width` skips it.
- Fab Presets hold them as limits and seed them. The values come with the shipped Fab Preset data.

The shipped presets use the following standard-service values.

| Fab Preset | Minimum stroke width | Minimum text height | Source |
|---|---|---|---|
| Generic | 0.15 mm | 1.0 mm | REDA selects these defaults for a board with no chosen fab. |
| JLCPCB | 0.15 mm | 1.0 mm | [Standard order instructions, Silkscreen/Texts](https://jlcpcb.com/help/article/instructions-for-ordering) |
| PCBWay | 0.15 mm | 0.8 mm | [Positive legend design instructions](https://www.pcbway.com/helpcenter/design_instruction/How_to_make_my_silkscreen_clear_and_beautiful_on_PCB__.html) |
| OSH Park | 0.127 mm | Unspecified | OSH Park recommends a 0.127 mm stroke and leaves text height to the designer. [Service guidance](https://docs.oshpark.com/services/six-layer/) and [DRC setup](https://docs.oshpark.com/design-tools/kicad/kicad-design-rules/) |

OSH Park also publishes a 0.0762 mm exception for short lines, text and graphics. REDA uses the general recommendation for its single board-wide width rule. Applying a preset with no published height leaves that minimum at zero.

## Left to other tickets

- The Command ids, the schema for the two scopes and the Fab Preset `inner` block, and how a Rules File names a Layer on another board: the Command and file-format work on the [v0.2 map](https://github.com/openreda/reda/issues/319).
- `through_hole_paste` (paste on a through-hole Pad) has nothing to do with Layers or scopes. Whether it ships in 0.2.0 is decided in [Which other work marked v0.2 ships in 0.2.0?](https://github.com/openreda/reda/issues/340).
- The `priority` field and the `footprint`, `pcb_net`, `zone` and `keepout` scope filters stay reserved for later.
