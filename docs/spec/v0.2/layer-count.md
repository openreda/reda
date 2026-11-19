# v0.2 Changing the Layer count

Source: [What happens when a board's Layer count changes?](https://github.com/openreda/reda/issues/334). It builds on the [Stackup](stackup.md), the [Stackup editor](stackup-editor.md), [Via types](via-types.md) and [editing across many copper Layers](layer-editing.md), and on the research into [many-Layer editing in other tools](https://github.com/openreda/reda/issues/322).

This file says what happens to a board that already has copper when a copper Layer is added, removed or moved, how Apply Construction changes the Layer count, and where a Layer count is chosen. Footprint Pads, rules scoped to a Layer, the Command ids and the schema are decided in their own tickets on the [v0.2 map](https://github.com/openreda/reda/issues/319).

## The three changes

- A board's Layer count changes only by **Add Layer**, **Remove** and **Apply Construction…** in the [Stackup editor](stackup-editor.md#layers-table). **Move up** and **Move down** change the order and not the count.
- There is no "Set Layer count" action, and Board Setup has no count control. It keeps its summary line and the button that opens the Stackup editor.
- **Each change is one edit Command** on the PCB Document's undo stack. One undo brings back every deleted object, shortened Via and removed Via type.
- Objects and spans follow their Layers by UUID, as in v0.1. Nothing below re-points an object to another Layer.

## Adding a Layer

- **Where:** as the Stackup editor says. On a dielectric the new Layer splits it; with nothing selected it goes in the middle of the stack. Add is unavailable at 32 copper Layers.
- **One Layer at a time.** No partner Layer is added on the other side, so the count may be odd. The [symmetry badge](stackup.md) is the only prompt.
- **A new copper Layer starts with:**

  | Field | Value |
  |---|---|
  | `role` | Signal |
  | `name` | empty, so it shows as its position |
  | thickness | that of the nearest inner Layer, or of the outer Layers on a 2-Layer board |

- **The split dielectric** becomes two equal halves of the same kind and material. The total thickness grows by one copper thickness, and the thickness badge shows it.
- **Spans that the new Layer lands inside** keep their ends and now pass through it. This goes for Vias and for Via types.
  - A type between L1 and L2 with a Layer inserted between them reads `Blind 1:3` by its automatic name.
  - Before the add, the editor says how many Vias and Via types will be stretched.
- **A stretched micro type.** When the add stretches a `micro` type past "adjacent or skip one":
  - the type keeps its flag, and the Via types table shows a warning badge on it;
  - `via_span` reports each of its Vias, with its own message ("micro Via type spans more than a micro Via can").
  - The fix is the user's: clear `micro` or change the type's span.

## Removing a Layer

Only inner Layers can be removed, and the dielectrics around a removed Layer are joined ([Stackup editor](stackup-editor.md#layers-table)).

### Objects on the Layer

- Tracks, Zones, text and graphics on the removed Layer are **deleted**.
- **A confirmation counts them first,** by kind: "12 Tracks, 1 Zone, 2 texts on L3 will be deleted." An empty Layer is removed without asking.
- There is no "move them to another Layer" choice in the dialog. To keep the copper, select it with `pcb.selection.on_layer` and change its Layer before removing.
- KiCad lists the affected items because its removal cannot be undone. REDA's can, so a count is enough.

### Vias

| The Via | What happens |
|---|---|
| passes through the removed Layer | nothing; it keeps its span |
| ends on the removed Layer | its span is **shortened** inward to the next copper Layer still inside the span |
| ends on it, and nothing is left to join (a 2:3 Via when L3 goes) | deleted |

- A shortened Via keeps its diameter and hole.
- The confirmation counts both groups: "4 Vias shortened, 2 deleted."
- This is REDA's own rule. KiCad deletes every Via that ends on a removed Layer, and no tool researched shortens one. Shortening keeps a connection that did not need to break.

### Via types

Via types follow the same rule as their Vias, so the two stay matched and a removal never makes a `via_span` violation by itself.

| The type | What happens |
|---|---|
| passes through the removed Layer | nothing |
| ends on the removed Layer | its span is shortened inward |
| has nothing left to join | removed |
| is shortened onto a span another type already has | removed; the existing type is kept, and the Vias belong to it |

- `micro` is kept, because shortening only makes a span shorter.
- The through type is never touched: only inner Layers can be removed.
- **Net Class `allowed` lists:** a removed type drops out of every list, and a type removed in favour of an existing one is replaced there by that one.
  - An emptied list means every type again, so the confirmation says so: "Net Class Power will allow every Via type."

### View state

View state is not part of the Command and is not undone.

| The removed Layer was | Then |
|---|---|
| the active Layer | the nearest copper Layer above it becomes active |
| the [Previous Layer](layer-editing.md#the-previous-layer) | the Previous Layer becomes the outer Layer opposite the active one |
| a Pinned Layer, or in a user Layer set | it drops out |

## Moving a Layer

- **Inner Layers only.** Move swaps a copper Layer with the next inner copper Layer. The outer two stay where they are, and the dielectrics stay in place.
- The outer Layers carry `side`, which Footprints, solder mask and silkscreen hang on. Bringing an inner Layer to the outside would be a different operation and is not offered.
- Objects follow their Layer.
- **Spans are not fixed up.** A Layer moved into or out of a Via's span changes what that Via touches, and connectivity and DRC report the result.
  - The move is silent when no span is crossed.
  - When one is, a confirmation counts the affected Vias first.
- Inner colours and automatic names follow position ([layer-editing](layer-editing.md#colours)), so a moved Layer takes the colour of its new place.

## Apply Construction with a different Layer count

With the same count, Apply Construction is as [Stackup](stackup.md#in-the-stackup-editor) says. With a different count it first changes the board's Layers, then applies the Construction's thicknesses and materials by position. The whole thing is one undo entry.

- **Growing** (4 to 6):
  - the outer Layers stay;
  - the existing inner Layers keep their order and split to the top and bottom halves, the upper half taking the extra one when their number is odd;
  - the new Layers go in the middle, each [starting](#adding-a-layer) as an added Layer does;
  - nothing is asked beyond the stretched-span count. Move reorders afterwards.
- **Shrinking** (6 to 4):
  - the dialog lists the inner Layers with their object counts and asks which to remove;
  - it preselects empty Layers first, then the ones nearest the middle;
  - each removal follows [Removing a Layer](#removing-a-layer), with one confirmation for them all.
- There is no position-by-position mapping table.
- Layer names and roles are kept on the Layers that stay.

## Choosing the count for a new board

- **New PCB Document** has a number field, 2 to 32, with 2 as the default.
- Beside it are quick picks for the counts the Fab Preset has Constructions for, typically 2, 4 and 6.
- The Construction list for the chosen count follows, as [Stackup](stackup.md#starting-a-board) says.

## Left to other tickets

- Footprint Pads and their copper on inner Layers when Layers are added or removed: [How do Pads, Vias and planes behave on inner Layers?](https://github.com/openreda/reda/issues/335).
- A design rule scoped to a removed Layer: [How do design rules vary by Layer?](https://github.com/openreda/reda/issues/336).
- The Command ids and the schema: the Command and file-format surface on the [v0.2 map](https://github.com/openreda/reda/issues/319).
