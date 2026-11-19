# v0.2 Teardrops

Source: [How do teardrops work?](https://github.com/openreda/reda/issues/337). It builds on the v0.1 [Zone fills](../v0.1/pcb.md#filling) and [rule records](../v0.1/checks.md#rule-records), on [inner Layers](inner-layers.md) and [rules by Layer](layer-rules.md), and on the research into [teardrops in other tools](https://github.com/openreda/reda/issues/324).

This file says what a Teardrop is, what gets one, its shape and defaults, the rule kind that controls it, the per-object opt-out, how it is generated and kept current, and how DRC, the Copper index and the outputs treat it. The Command ids and the schema are decided in their own work on the [v0.2 map](https://github.com/openreda/reda/issues/319).

A v0.1 board needs no change: a new or existing board has every target off and so has no Teardrops.

## Derived copper

A Teardrop is generated, never stored and never edited.

- The PCB Document holds only the [`teardrop` rule records](#the-teardrop-rule-kind) and the [opt-outs](#opt-out). The copper itself goes in the `.reda/` cache, in the same per-PCB-Document file as the Zone fills.
- Each Teardrop has its own input hash: the two objects it joins, the resolved rule values and the obstacles near it. A stale one is regenerated.
- There is no "add teardrops" or "remove teardrops" command to rerun. You change what a Teardrop hangs on, change a rule, or opt an object out.
- No other tool researched works this way: they store a region or a small Zone per teardrop, and most need the command run again after an edit.

## What gets one

Four targets, each switched on its own:

| Target | Where |
|---|---|
| `through_hole_pad` | a Track ends on a through-hole Pad or Free Pad |
| `smd_pad` | a Track ends on an SMD Pad |
| `via` | a Track ends on a Via |
| `width_change` | two Tracks of different widths meet end to end |

- Tracks of equal width meeting at a T-junction get none, and neither does a Track meeting a Zone edge.
- Several Tracks into one Pad or Via each get their own, and the shapes may overlap.
- v0.2 has straight Tracks only. Curved Tracks (v0.4) get Teardrops by the same rules when they arrive.

## Shape

- **Values, per target:**

  | Value | Meaning | Default |
  |---|---|---|
  | `length` | percent of the target's size | 50 |
  | `max_length` | cap, in mm | 1 |
  | `width` | percent of the target's size, measured where the Teardrop meets the target | 100 |
  | `max_width` | cap, in mm | 2 |
  | `curved` | curved edges instead of straight | off |
  | `skip_ratio` | no Teardrop when the Track is at least this percent of the target's size | 90 |

- **The target's size** is the Pad's or Via's copper on that Layer, measured across the Track's direction. For `width_change` it is the wider Track's width.
- The same defaults ship for all four targets. They follow KiCad's Board Setup figures; the leading suite's percentages change their reference from one target to the next, which is harder to explain.
- **A Track that ends inside the copper** without reaching the centre still gets a Teardrop, anchored where the Track crosses the copper's edge.
- **Short segments and corners:** the Teardrop follows the Track's path through its vertices, so a Track that bends 0.3 mm from the Pad still gets a full-length Teardrop along the bend. It stops early at a junction, a width change, a Via or another Pad.
- Curved edges are flattened at the same fixed 0.001 mm as [Zone fills](../v0.1/pcb.md#filling).

## The `teardrop` rule kind

Teardrops are controlled by a rule kind, not by a block of board settings.

- **Values:** `through_hole_pad`, `smd_pad`, `via` and `width_change`, each `{enabled, length, max_length, width, max_width, curved, skip_ratio}`.
- **Scopes:** `net_class` and [`layers`](layer-rules.md#the-layers-scope), alone or together. `net_class_pair` is refused.
- **Which value applies:** the [order for one Net on one Layer](layer-rules.md#which-value-applies), value by value. It is judged on the Track's Net and the Track's Layer. A Track with no Net uses the board-wide records.
- **The board-wide record** can't be deleted and ships with every target's `enabled` off and the default shape values.
  - "Teardrops on the outer Layers only" is one `{layers: outer}` record that sets `enabled`.
  - "None on the RF class" is one `{net_class}` record.
- Like `zone_connection`, it shapes copper rather than setting a limit, so it has no DRC severity.
- **Changing a rule regenerates the Teardrops it governs.** "Changing a rule leaves existing copper alone" is about placed copper; derived copper follows its rules.
- It travels in a Rules File like any other kind. No Fab Preset sets it.

## Opt-out

- A placed Pad, a Via or a Track stores `teardrop: false` when opted out. The field is left out otherwise. It is the "teardrop opt-out" v0.1 [reserved](../v0.1/architecture.md#reserved-names).
- It beats every rule. On a Track it removes the Teardrops at both ends. On a Pad or Via it removes every Teardrop that meets it, on every Layer.
- There is no per-object "on". A Net Class or `layers` record does that job, so a board is never half teardropped with nothing to say why.

## Pad Stacks and absent copper

- A Teardrop is generated per Layer against the object's real copper on that Layer: a through-hole Pad's `top`, `middle` or `bottom` shape, or a Via's `start`, `middle` or `end` diameter ([Pad Stacks](inner-layers.md#pad-stacks)).
- Where [`unused_layers`](inner-layers.md#unused-inner-copper) has left the copper out there is nothing to meet and no Teardrop. An inner Pad that stays because a Track connects to it gets one as usual.

## Zones

- Where the Pad or Via connects **solid** to a filled Zone of its Net on that Layer, no Teardrop is generated there. The pour already does the job. This is not a setting.
- With a relief or no connection, the Teardrop is generated as normal, and the Zone's relief gap or clearance is cut around the Pad and the Teardrop together.
- Teardrops are generated before Zone fills, because fills must keep clear of them.

## Clearance wins

A Teardrop never causes a DRC violation.

- Generation shrinks it until it clears other-Net copper, holes, Keepouts that exclude Tracks, and the board edge by the applicable `clearance` and `edge_clearance`. If nothing useful is left it is dropped.
- There is no option to force a Teardrop in spite of a violation.
- **No DRC check:** nothing reports a Teardrop as missing or shortened, and there is no severity to set. A Teardrop that could not fit is not a fault in the board.
- **Counts:** the Teardrop page of Board Setup shows how many were generated, shortened and left out ("212 generated, 3 shortened, 1 left out"). The shortened and left-out ones can be listed and jumped to from there.

## DRC and the Copper index

- A Teardrop is copper of its Track's Net. It joins the Copper index for connectivity and as an obstacle, and Zone fills pour around it like any other copper.
- It is checked for `clearance` only. `track_width`, `annular_ring`, `micro_segment` and the other width checks ignore it.
- `annular_ring` is still measured on the Pad or Via alone, so a Teardrop never hides a thin ring.

## Keeping current

- Same triggers as [Zone fills](../v0.1/pcb.md#filling): stale Teardrops regenerate in the background after a committed edit, never during a drag. Outputs and DRC always regenerate stale ones first.
- The Preference `pcb.zone.auto_refill` governs both kinds of derived copper. `pcb.zone.fill` with no arguments also regenerates stale Teardrops.
- A Lock on a Track, Pad or Via does not stop its Teardrops from appearing, changing or going, the same way a Lock allows Zone refills.

## On the canvas

- A Teardrop is not an object. It can't be selected, has no Properties and is not in the Board navigator. Clicking it selects its Track.
- It is drawn in the Layer's copper colour as part of the copper.
- It appears once the edit is committed. The routing preview shows none.

## Outputs

- Teardrops are written as plain copper regions on their Layer, with the Track's Net attribute and the conductor aperture function. The Gerber format has no teardrop-specific primitive or attribute, and no tool researched writes one.
- They appear in every copper output (Gerber, PDF) and nowhere else: not in the BOM, the CPL, the drill files or the netlist.

## Left to other tickets

- The Command ids and the schema for the `teardrop` rule kind and the `teardrop` opt-out field: the Command and file-format work on the [v0.2 map](https://github.com/openreda/reda/issues/319).
- Goldens and read-back tests for Teardrops: the testing work on the same map.
- Teardrops at equal-width T-junctions and where a Track meets a Zone edge: not in v0.2.
