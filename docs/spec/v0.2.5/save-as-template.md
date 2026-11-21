# v0.2.5 Save Board as Template

Source: [What does Save board as template capture, and how is it written?](https://github.com/openreda/reda/issues/402), with [drawings](https://github.com/openreda/reda/issues/404) (which supersedes its "no Dimensions, silk or text" for the Mechanical and Documentation Layers) and [plated edges](https://github.com/openreda/reda/issues/403).

This file says how a user turns the active board into a Board Template.

## Entry points

- **The Command:** `library.board_template.save {library, name, description?, category?, objects?}` acts on the active PCB Document ([commands](commands.md#libraryboard_templatesave)).
  - It returns the new element UUID, its path and any warnings.
  - There is no MCP-specific tool; agents use `run_command`.
- **The UI:** **Design ▸ Save Board as Template…** opens a dialog with the name, description, category and target Library, and the capture list below.

## Choosing objects

The dialog lists the candidates with tick boxes, grouped like the creation dialog's [Includes](creation-dialog.md#includes).

| Candidate | Default |
|---|---|
| Board Outline | Always captured. It can't be unticked. A board with no outline is refused. |
| Board Cutouts | ticked |
| Keepouts | ticked, except inner-layer Keepouts, which are [listed but can't be ticked](#layers) |
| Components, Mechanical Parts, Free Pads | ticked only when Locked |
| "Mechanical drawings" | ticked, whatever their Locked state |
| "Documentation drawings" | ticked, whatever their Locked state |

- `objects` lists the board UUIDs to capture beyond the outline. A drawing tick is sent as the Layer's UUID, which stands for every graphic, text and Dimension on it.
- Leaving `objects` out applies the dialog's defaults.
- **Any Component can be captured.** It doesn't have to be a connector. "Connector" names the typical use, not a rule.

## Parts

- The part table takes the Component's embedded Part copy. The footprint-copy row takes the Footprint copy it uses.
- Both keep their element UUID and `hash` unchanged.
- **Local Pad edits are not carried.** A placed Footprint's local Pad edits (ADR [0013](../../adr/0013-placed-pads-edit-a-local-footprint-copy.md)) stay on the board. The template uses the unedited copy, and the dialog warns "J1 has local Pad edits that won't be saved".

## Keepout ties

No `protects` ties are written. Every captured Keepout always comes along. Adding a tie is a hand-edit to the file.

## Layers

- **A Keepout's Layer becomes symbolic:**
  - a Keepout on every copper Layer becomes `all_copper`;
  - one on the top Layer becomes `top_copper`;
  - one on the bottom Layer becomes `bottom_copper`.
- **An inner-layer Keepout** is listed but can't be ticked, and the dialog gives the reason: "inner-layer Keepouts can't go in a template yet". The Command refuses one named in `objects`.
- Components and holes take `side` from their placement.
- `suggested_layers` is the board's copper Layer count.

## Drawings

- Each of the two drawing ticks becomes one drawing group, named after its Layer: "Mechanical drawings" or "Documentation drawings" ([drawings](drawings.md#save-board-as-template)).
- A Dimension keeps its reference when its target is captured. Otherwise its end becomes free.

## Board properties and edges

- The template carries `edge_connector {bevel?, hard_gold?}` when the board has it set.
- Plated Edges keep their `plated` mark, and their Net is dropped ([plated edges](plated-edges.md#board-templates)).
- **Nothing else is carried:**
  - no Fab Preset, Design Rules or Output Origin;
  - no silk, copper text or graphics;
  - nothing on Layers other than Mechanical and Documentation.

## Coordinates and Locked state

- The geometry is shifted so the lower-left corner of the outline's bounding box lands at (0,0).
- All values are written as literal integers, with no expressions.
- Each row's `locked` records the object's current state.
- The template's objects get fresh UUIDs.

## Writing the file

- **Target Library.** The template goes into a writable Library the user picks. The Starter Library is refused, as is any read-only Library.
- **File.** It is written to `board-templates/<name>.redaboard`, and `name` must be unique within the Library.
- **Name clash.** A name clash is refused, as every Library write is today: rename, or delete the old template in the Library panel first.
- **No overwrite.** There is no overwrite or Replace, which protects any parameters, gates or ties added by hand.

## Not captured

- Parameters and `when` gates. Making a template parametric is a hand-edit to the JSON, checked when the file loads ([format](template-format.md#when-the-file-loads)).
