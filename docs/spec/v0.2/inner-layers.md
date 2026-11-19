# v0.2 Pads, Vias and planes on inner Layers

Source: [How do Pads, Vias and planes behave on inner Layers?](https://github.com/openreda/reda/issues/335). It builds on the v0.1 [Pad model](../v0.1/library.md#pad-model), [Zone connections](../v0.1/pcb.md#zone-connections), [Zones](../v0.1/pcb.md#creating-zones) and [rule records](../v0.1/checks.md#rule-records), on [Via types](via-types.md) and the [Stackup](stackup.md), and on the research into [inner-Layer Pads, Vias and planes in other tools](https://github.com/openreda/reda/issues/323).

This file says what copper a through-hole Pad or a Via has on each Layer, when that copper is left out, how close other copper may come to a plated hole, how a fill joins them, what a plane Layer does and does not do, and how free text and graphics on copper are checked. The Command ids and schema, rules that differ by Layer and the outputs are decided in their own tickets on the [v0.2 map](https://github.com/openreda/reda/issues/319).

The choices lean towards the leading suite, then KiCad. Where REDA differs from both, the entry says so.

A v0.1 board needs no change: every new value here is optional and its default gives the v0.1 behaviour.

## Pad Stacks

A plated through-hole Pad has copper on every copper Layer, and a Via on every Layer of its span. v0.1 gives that copper one shape. v0.2 lets it differ in three groups, as the top / middle / bottom mode of the leading suite and KiCad.

### On a through-hole Pad

- The Pad's v0.1 shape and size are its **top** copper.
- `middle` and `bottom` are optional. Each is a whole copper shape: shape, size, and its corner and chamfer values.
  - `middle` applies on every inner Layer.
  - Each one left out is the same as the top copper, so a v0.1 Pad is a Pad Stack with one shape.
- The hole, `at` and `offset` are shared by all three.
- Top and bottom are in the Footprint's frame and swap when it is flipped, like the [mask sides](../v0.1/library.md#pad-model). On a Free Pad they are board sides.
- Mask openings follow the copper on their own side.
- SMD and non-plated Pads have no Pad Stack.
- [Placed Pad edits](../v0.1/pcb.md#placed-pad-edits) cover `middle` and `bottom` like any other Pad property.

### On a Via and a Via type

- A Via's copper is round, so its Pad Stack is three diameters: `start`, `middle` and `end`, in the order of its span. A blind Via has no board top or bottom, only the two ends of its span.
- The Via's v0.1 diameter is `start`. `middle` and `end` are optional and each one left out is the same as `start`.
- A [Via type's `pad`](via-types.md#what-a-via-type-stores) becomes the same three diameters, with `middle` and `end` optional. A new Via starts with all three.
- A Via with a span of two Layers has no middle.

### The full stack stays reserved

A size per individual Layer, the third mode in both tools, is not in v0.2. Top / middle / bottom covers smaller inner copper and a different bottom land, and stores nothing against an individual Layer, which is why [a change of Layer count](#when-the-layer-count-changes) leaves Pads alone.

## Unused inner copper

A pad on an inner Layer that joins nothing can be left out, which gives room for routing and fills. Fabs disagree on whether they want this, so nothing is removed unless asked.

- Each through-hole Pad and each Via stores `unused_layers`, left out when `keep`:

  | Value | Pads | Vias | Effect |
  |---|---|---|---|
  | `keep` (default) | yes | yes | copper on every Layer |
  | `remove` | yes | yes | copper only on the Layers where it is connected |
  | `remove_keep_ends` | | yes | as `remove`, but the Via's two end Layers always keep their copper |

- **A Pad's top and bottom copper always stay,** because they are its solder lands. `remove` on a Pad affects inner Layers only.
- **A Via under `remove`** can lose unconnected copper on its end Layers too, including on the board's outer Layers.
- **Connected, on one Layer,** means either:
  - a Track of the same Net ends on it, or
  - a Zone of the same Net covers it and the [Zone connection](#zone-connections) that applies there is `relief` or `solid`.

  This is defined by what is meant to connect, not by the finished fill, so the fill and the pad never depend on each other.
- **It is live.** When a Track or Zone reaches the object on a Layer, its copper is back on that Layer, and it goes again when the connection does. There is no command to update it. This is KiCad's stored mode. The leading suite's command leaves a snapshot that goes stale when routing changes.
- **It is real board copper.** Where the copper is left out, it is absent from the Copper index, DRC, Zone fills and every output alike. No output removes copper that the board shows, so there is no export-time option.
- **Where a new object gets it:**
  - A Via type carries `unused_layers`, default `keep`, and a new Via of that type starts with it.
  - A new through-hole Pad is `keep`. A Library Footprint's Pads may set `remove`.
  - Fab Presets do not set it.
- **Bulk change:** one Command sets the mode on the whole board or the selection, for Pads, Vias or both. Its dialog follows the leading suite's Remove Unused Pad Shapes: what to change, where, and whether Vias keep their ends. Properties sets it on a selection too.
- Changing the mode marks the Zones around the object stale, like any copper edit.

## Clearance to a hole

Where copper is left out, or a ring is thin, other copper can come close to the drilled hole. Fabs publish a limit for this, measured from the hole.

- The `clearance` rule gains `hole {via, pad}`: the least distance from the edge of a plated hole to copper of another Net, on every copper Layer the hole passes through.
  - `via` is for Via holes and `pad` for the holes of plated Pads and Free Pads.
  - The leading suite has this as the Hole entry of its clearance rule, with one value. REDA has two because fabs publish two, and one value would put the stricter figure on every Via.
- **Both distances hold at once:** `clearance.value` from the copper's edge where the object has copper on the Layer, and `clearance.hole` from the hole's edge on every Layer.
- **Zone fills keep it,** as they keep `clearance.value`.
- **Same-Net copper** that does not connect on a Layer (a Zone whose connection is `none`, or copper passing a hole whose pad is left out) keeps `clearance.hole` from the hole too. A `none` connection's `gap` is measured from the pad where there is one, and from the hole where there is not.
- It resolves like the rest of `clearance`: by Net Class, then board-wide, and the larger of the two objects' values applies.
- Non-plated holes keep using [`edge_clearance`](../v0.1/checks.md#rule-records).
- **Seeds:**

  | Fab Preset | `hole.via` | `hole.pad` |
  |---|---|---|
  | Generic | 0.25 mm | 0.25 mm |
  | JLCPCB | 0.20 mm | 0.30 mm |
  | PCBWay | 0.20 mm | 0.20 mm |
  | OSH Park | 0.254 mm | 0.254 mm |

  The figures are the fabs' published inner-Layer limits from the research. The Generic value is REDA's own.

## Annular ring

- `annular_ring` is checked on every Layer where the object has copper, against that Layer's shape from its Pad Stack.
- A Layer where the copper is left out has no ring to check. [`clearance.hole`](#clearance-to-a-hole) covers it instead.
- `via_size` and `microvia_size` check all three diameters of a Via.
- Whether the minimum differs between inner and outer Layers is decided in [How do design rules vary by Layer?](https://github.com/openreda/reda/issues/336).

## Zone connections

- **No Layer mechanism of its own.** `zone_connection` stays one rule kind. If rule records gain a `layers` scope in [How do design rules vary by Layer?](https://github.com/openreda/reda/issues/336), `zone_connection` takes it like any other kind, so "solid on the plane Layers, relief outside" is one scoped record. A Pad Stack holds no connection values per Layer and a Zone holds none, as in v0.1.
- **A Via gains the override a Pad has:** an optional `zone_connection {style?, spokes?, spoke_width?, gap?, angle?}`. Each value left out inherits.
- **Which value applies, value by value:** the object's own `zone_connection`, then the highest-ranked Net Class record that sets it, then the board-wide record. This is the v0.1 order with Vias added to its first step.
- A spoke is drawn against the object's copper on that Layer, so it follows the Pad Stack.

## Plane Layers

A plane is one or more ordinary Zones on a Layer whose `role` is plane. There is no negative plane Layer.

- **`role` never restricts and never warns.** Tracks, text, Vias and several Zones may be on a plane Layer, and no DRC check looks at `role`.
  - A split plane is several Zones on the Layer, ordered by [priority](../v0.1/pcb.md#pour-order).
  - A Track on a plane Layer is an ordinary Track with its own Net. A fill treats it by the Zone's `pour_over`.
  - A check for a Track that crosses a split in the plane next to it is a return-path rule, and belongs to v0.4 Interactive routing.
- **What `role` does do** is unchanged: the [impedance reference and the Gerber file function](stackup.md), and in the editor the [Layer stepping and the Plane Layer set](layer-editing.md).
- **Filling a plane Layer** uses the v0.1 Zone that [follows the Board Outline](../v0.1/pcb.md#creating-zones) (`pcb.zone.add_from_board_outline`). It stays the board's shape when the outline is edited, respects Board Cutouts and `edge_clearance`, and can be detached into ordinary vertices. This is the one behaviour of the leading suite's negative planes that REDA keeps, on a positive Zone.
  - **Fill Layer with plane…** asks for a Net and runs that Command for one Layer. It is in the Layer's context menu in the Stackup editor and the Layers panel.
  - Setting a Layer's `role` to plane offers the action when the Layer has no Zone. It never makes a Zone by itself, and changing `role` away from plane leaves the Layer's Zones alone.

## When the Layer count changes

- A through-hole Pad means every copper Layer and stores nothing against an individual inner Layer, so [adding, removing or moving a Layer](layer-count.md) changes no Footprint, Pad or Free Pad.
  - A new inner Layer gets each Pad's `middle` copper, or none where `unused_layers` leaves it out.
  - A board going from 2 Layers to 4 shows `middle` shapes for the first time.
- An SMD Pad stays on its outer Layer.
- A Via's `middle` likewise applies to whatever Layers lie inside its span after the change.
- Zones refill as after any edit.

## Text and graphics on copper

Free text and graphics on a copper Layer are etched copper, and v0.1 does not check them ([issue](https://github.com/openreda/reda/issues/121)).

- Free text and graphics on any copper Layer of a PCB Document join the Copper index as their own Item kind, with no Net. Text is indexed as its strokes.
- **DRC:** `clearance` covers them against every Track, Pad, Via, Zone fill and each other, at the board-wide value or the other object's larger one. Touching a Net's copper is a clearance violation. `clearance.hole` and `edge_clearance` cover them too.
- **Zone fills** keep the clearance around them.
- They never join a Net and never connect anything.
- The outputs are unchanged for a board without them.
- A Footprint's own graphics are not affected: v0.1 Footprints have none on copper.

## Left to other tickets

- Which rule kinds take a `layers` scope, and inner-Layer seeds for `annular_ring`, `clearance` and `zone_connection`: [How do design rules vary by Layer?](https://github.com/openreda/reda/issues/336).
- The Command ids, the schema for `middle`, `bottom`, the Via diameters, `unused_layers` and `clearance.hole`, and how the Footprint editor and the generators show a Pad Stack: the Command and file-format work on the [v0.2 map](https://github.com/openreda/reda/issues/319).
- Teardrops against a Pad Stack's shapes: [How do teardrops work?](https://github.com/openreda/reda/issues/337).
- Inner-Layer Gerbers and drill files: [What do multilayer outputs contain?](https://github.com/openreda/reda/issues/339).
