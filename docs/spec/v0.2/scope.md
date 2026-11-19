# v0.2 scope

Source: [Which other work marked v0.2 ships in 0.2.0?](https://github.com/openreda/reda/issues/340).

This file says what ships in `0.2.0` besides the Multilayer milestone itself, and where everything else that v0.1 had marked "v0.2" went. The rule was to keep what multilayer work touches and move the rest, so that 0.2.0 is one theme.

## In 0.2.0

- **The Multilayer milestone:** the other files in this folder.
- **Free text and graphics on copper in the Copper index and DRC** ([#121](https://github.com/openreda/reda/issues/121)). The behaviour is in [inner Layers](inner-layers.md).
- **Two UI refactors left from the v0.1 review** ([#122](https://github.com/openreda/reda/issues/122)), with no behaviour change:
  - one selection state machine for the Symbol and Footprint editors, which each still carry their own `select_step` and `box_select`;
  - typed Tool Command dispatch, in place of `Shell::after_edit` matching `tool.*` id strings.

  They come first in the build order, because Layer stepping, hover to solo and the Via type choice all add Tool Commands. The issue's other two items, one snap model and one home for highlight state, were already done by the v0.1 UI rework (`reda-ui/src/snap.rs`, `reda-ui/src/highlight.rs`).
- **A Layer count or Construction choice when a board is created.** v0.1's new-board page left room for it. It is specified with the v0.2 Commands.

## Moved to a later milestone

Each item's marker in the v0.1 spec and in the [reserved names](../v0.1/architecture.md#reserved-names) now names its new milestone, and [`modules.md`](../modules.md) lists it there.

| Item | Now in | Why there |
|---|---|---|
| Wheel mapping (`canvas.wheel.*`), arrow-key cursor, grid X ≠ Y, axis alignment guides, the object defaults page | v0.3 Mechanical & 3D | v0.3 already holds `grids` and `guides` and is the next rework of the canvas |
| Sync's `relink_component` and the live pending-changes Panel | v0.5 Design scale | v0.5 changes a Component's identity, which relinking depends on |
| String substitution (`{old=new}`) in expressions; a paste array for sheets | v0.5 Design scale | both serve editing many objects at once |
| The Jumper Component Type and `jumper_id`; Footprint `keepouts`; more Footprint generators and generating from a CSV; unit-aware values; a table of Pads and a list of primitives | v0.6 Libraries & import | Library work with no tie to Layers |
| `through_hole_paste` on the Pad and on `paste_expansion` | v0.6 Libraries & import | decided together with `mask_shape` and `paste_shape` |
| Supplier fields and ordering BOMs ([#120](https://github.com/openreda/reda/issues/120)) | v0.6 Libraries & import | supplier part search needs the same fields |
| Sheet frame templates (`style`, `default_font`) and `project.new {template?}` | v0.7 Outputs | frames matter most on printed sheets |
| PDF Net bookmarks, Component field pop-ups, hyperlinks and fit-to-paper | v0.7 Outputs | output work |

The table of Pads may come back into 0.2.0 if the Footprint editor needs it to edit a Pad Stack's `middle` and `bottom` copper. That is decided with the Pad Stacks in the Library, on the [v0.2 map](https://github.com/openreda/reda/issues/319).
