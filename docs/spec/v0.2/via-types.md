# v0.2 Via types

Source: [How are Via types defined, picked and checked?](https://github.com/openreda/reda/issues/332). It builds on the v0.1 Via ([pcb](../v0.1/pcb.md#vias), [checks](../v0.1/checks.md#rule-records)), on the [Stackup](stackup.md) and on the research into [Via types in other tools](https://github.com/openreda/reda/issues/321) and [fab Via options](https://github.com/openreda/reda/issues/328).

This file says what a Via type is, where a Via's size comes from, how routing picks a Via on a Layer change, how Vias that aren't through are drawn, and what DRC reports. How the Via types table and the cross-section look is in [stackup-editor](stackup-editor.md). The Command ids and schema, drill outputs and what happens to Vias when the Layer count changes are decided in their own tickets on the [v0.2 map](https://github.com/openreda/reda/issues/319).

A new board has only the through type, so nothing in this file shows until someone adds a type. Nothing here limits what a board may use: any span, mixed stacks and micro Vias are all available.

## The Via types list

- A PCB Document holds one list of Via types, kept with its Stackup and edited in the Stackup editor's Via types table.
- Routing offers only the listed types, and DRC flags a Via that matches none ([`via_span`](#drc)).

### What a Via type stores

| Field | Meaning | Required |
|---|---|---|
| `uuid` | identity | yes |
| `name` | shown in the table, the cross-section and Properties | no |
| `from`, `to` | the span: the two copper Layers it joins, by Layer UUID. `from` is the side the drill or laser enters | yes |
| `micro` | laser-drilled | no, default false |
| `drill` | hole diameter | yes |
| `pad` | pad diameter | yes |

- **Names are automatic from the span** while `name` is empty: `Through`, `Blind 1:2`, `Buried 2:3`, `Micro 1:2`. The numbers are copper positions, with the top Layer as 1. Typing a name replaces the automatic one, and clearing it brings the automatic one back.
- **Through, blind and buried are worked out from the span** and never stored: through joins both outer Layers, blind reaches one outer Layer, buried reaches none. A micro type is called micro whatever it reaches.
- A new type starts with the through type's `drill` and `pad`.

### Which spans are allowed

- Any two different copper Layers may form a span. REDA does not judge whether a set of spans can be laminated, because that depends on the fab's process.
- **One type per span.** Two types may not join the same pair of Layers, in either direction. Different sizes on one span come from [Net Classes](#sizes).
- **The through type always exists.** It cannot be removed, its `micro` is always false, and its span follows the outer Layers when Layers are added or removed.
- **`micro` is offered only** when the span joins adjacent copper Layers or skips one. A longer span cannot be micro.

### How a Via belongs to a type

- A Via stores its own span and its own diameter and hole, as in v0.1. It stores no reference to a type.
- A Via is of the type whose span joins the same two Layers. It is stored in that type's direction.
- Whether a Via is micro comes from its type. A Via that matches no type is treated as drilled.

### Changing the list

- **Editing a type's span** when Vias use it: the editor says how many ("14 Vias use this span") and offers to move them to the new span. Declined, they stay and `via_span` flags them.
- **Removing a type** that Vias use is allowed after the same count. The Vias stay and `via_span` flags them.
- **Changing a type's `drill` or `pad`** leaves existing Vias alone, like any rule change in v0.1. Bulk changes use selection and Properties.

### Via types in a Construction

- A [Construction](stackup.md#constructions) may carry Via types. Each stores an optional `name`, `from` and `to` as copper positions, `micro`, `drill` and `pad`. Positions are used because Layer UUIDs mean nothing on another board.
- Applying them, by the tick box in Apply Construction, replaces the board's list. The dialog first says how many Vias would no longer match a type.
- Shipped Constructions carry none.

## Sizes

The type owns the size and the rules own the limits.

- **The starting size of a new Via:** the highest-ranked Net Class on the Net with an enabled record that sets a `preferred` Via size wins. Otherwise it is the type's `drill` and `pad`.
- **The board-wide `preferred` Via size of v0.1 is dropped.** The through type's size takes its place.
- **Seeding:** on a new board, the Fab Preset's "Via preferred" seed fills the through type's `drill` and `pad`.
- **Two rule kinds, one per drilling method:**

  | Kind | Applies to | Board-wide record | A Net Class record may add |
  |---|---|---|---|
  | `via_size` | drilled Vias | `diameter {min, max?}`, `hole {min, max?}`, `aspect_ratio {max?}` | `preferred` for `diameter` and `hole` |
  | `microvia_size` | micro Vias | the same, and `max_stack?` | the same |

- A power class that sets `preferred` in `via_size` gets its larger Vias on every drilled type. It needs its own `microvia_size` record to change micro Vias.
- **Limits clip as in v0.1.** The routing Tool clips to `min` and `max`, and `4` cycles `min`, the starting size, `max` and `user`.
- A type whose `drill` or `pad` is outside the board-wide limits is marked in the Via types table, and DRC flags the Vias placed with it.
- `microvia_size` is seeded empty in every shipped Fab Preset, because the fab research verified no HDI numbers.

## Routing

### Picking a Via on a Layer change

- **Candidates** for a change between two Layers:
  - every single type whose span covers both Layers;
  - every stack of types, of any kind, whose spans meet end to end and together cover both Layers.
- The through type always covers both, so there is always a candidate unless a [Net Class restricts it](#net-class-restrictions).
- **Proposed first:** the candidate that reaches the fewest Layers beyond the two being joined. On a tie, a single Via comes before a stack, then the order of the Via types table decides.
- **Keys:** `6` cycles the candidates and `8` lists them. The proposed Via or stack is drawn at the cursor before the click, as in v0.1.
- **The choice is remembered per Layer pair** in the PCB Document's editor state, so the same pair proposes the same candidate next session. It is not design data and never affects DRC.
- `V`, `2` and `E` keep their v0.1 meanings and place the proposed candidate. Which Layer `V` goes to is decided in [How does the PCB editor work across N copper Layers?](https://github.com/openreda/reda/issues/333).
- **A Via placed on its own,** for stitching, uses the type set in the Tool's Properties. The default is the through type.

### Stacks

- There is no stack object. A stack is separate Vias sharing a centre whose spans meet end to end, each of a listed type. A micro Via on a buried Via is a stack like any other.
- A stack placed by the router commits as separate `pcb.via.add` entries in the route's one batch. Each Via takes its own type's starting size.
- **Staggered Vias are routed by hand:** a Via, a short Track, another Via. The router never builds them. Automatic staggering would belong with the v0.4 interactive router.

### Net Class restrictions

- `via_span` is a rule kind. Board-wide it has no values, because the Via types list is the rule.
- A Net Class record may carry `allowed`, a list of Via types. Empty means all, which is the default.
- Routing offers only the allowed candidates for that Net.
- The through type may be left out of a class's list. If a Layer change then has no candidate, the Tool says so and places nothing.

## Display

- **Through Vias** are drawn as in v0.1.
- **Any other Via** has its hole drawn as two halves, in the colours of its `from` and `to` Layers.
- **The Via Span label** prints the span as copper positions, such as `1:3`, inside every Via. It is a toggle beside Via Nets, off by default, with the usual minimum on-screen size.
- Micro Vias get no extra mark on the board. Their size and the label tell them apart.
- The Properties panel shows the Via's type name, or "no Via type" when the span matches none.
- **Solder mask:** a side the Via doesn't reach has no opening and ignores the Via's `mask` values and the tenting rule for that side. Properties greys them.

## DRC

| Check | Severity |
|---|---|
| `via_span`: a Via whose span matches no type in the list, or a type its Net Class doesn't allow | E |
| Via diameter or hole outside `via_size` (drilled) or `microvia_size` (micro) | E |
| `aspect_ratio`: the hole's depth divided by its diameter is over the limit | E |
| `max_stack`: more micro Vias in one stack than the limit | E |

- Routing never makes a `via_span` violation. Pasting, a removed or edited type and a changed Layer count can.
- **Depth for `aspect_ratio`** is the distance between the outer faces of the span's two copper Layers, from the Stackup. A blank Dielectric uses its [shared thickness](stackup.md#blank-thickness).
- `aspect_ratio` and `max_stack` are empty by default, and an empty limit is skipped. No shipped Fab Preset sets them.
- `max_stack` counts only the micro Vias in a stack.
- **Hole-to-hole** skips two Vias of one Net at the same centre whose spans meet end to end. Vias at one centre whose spans overlap are still a violation.

## Left to other tickets

- The Command ids and the `.redapcb`, `.redafab` and Rules File schema additions, including how a Net Class's `allowed` list names Via types in a Rules File used on another board.
- What happens to Via types and Vias when Layers are added or removed, in [What happens when a board's Layer count changes?](https://github.com/openreda/reda/issues/334).
- Zone Connections, unused pads on inner Layers and annular ring per Layer, in [How do Pads, Vias and planes behave on inner Layers?](https://github.com/openreda/reda/issues/335) and [How do design rules vary by Layer?](https://github.com/openreda/reda/issues/336).
- One drill file per span and plating, in [What do multilayer outputs contain?](https://github.com/openreda/reda/issues/339).
