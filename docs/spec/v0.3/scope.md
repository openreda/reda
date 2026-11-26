# v0.3 scope

Sources: [Which adjacent work ships in 0.3.0, and is it one release or two?](https://github.com/openreda/reda/issues/439) and [What is the v0.3 spec layout and build order?](https://github.com/openreda/reda/issues/468).

This file says what ships in `0.3.0` besides the Mechanical & 3D milestone itself, what was left out, whether the canvas extras get their own release, and the order v0.3 is built in. The [build order](#build-order) links every file of the v0.3 spec, so it doubles as the index.

## One release

- **0.3.0 holds both halves:** the Mechanical & 3D milestone and the canvas extras moved from v0.2 (document grids and guides, wheel mapping, the arrow-key cursor, Object Defaults).
- There is no separate canvas release, as Board Templates had with 0.2.5. Nobody uses REDA yet and 0.x releases are dev releases, so an earlier release would gain little.
- The canvas extras come early in the [build order](#build-order), because the 3D view uses the same wheel mapping.

## In 0.3.0

- **A rendered 3D board image.** `document.render` gains a 3D view, drawn headless from the same `reda-3d` scene as the 3D view, so CLI and MCP agents can see the board in 3D. An Output Job entry writes board images beside the zip. The 2D `document.render` from v0.1 is unchanged. Details: [Board Image](board-image.md).
- **A Component height rule.** A new Rule kind checks each Component's `height` against a maximum. Details: [Component height](component-height.md).

## Left out

| Item | Why |
|---|---|
| Body collision checks (3D Body against Body) | they need solid or mesh intersection on imported Models, which `reda-step` does not have at first; `courtyard_overlap` already catches most clashes in 2D. Later work, together with checking fit against an enclosure Model |
| DXF into the Footprint editor, and DXF on copper | already left for a later milestone by [DXF](dxf.md) |

## Build order

0. **Release 0.2.0 from main**, Board Templates included, and freeze `docs/schemas/v2/` ([format version](commands.md#format-version)). No v0.3 build work starts before it, and there is no separate 0.2.5 release.
1. Wheel mapping, the arrow-key cursor, Object Defaults ([input and defaults](input-and-defaults.md)) and matching the digit row by physical key ([3D view keys](3d-view.md#keys)). They are Preferences only, with no format change, and the 3D view reuses the wheel table.
2. Format 3 and Mechanical Layers ([Mechanical Layers](mechanical-layers.md), [Commands and file formats](commands.md)): the 2 to 3 step and `pcb.layer.*`.
3. Board Grids, Local Grids and Guides, with Alignment Guides and Align's key object ([grids and guides](grids-guides.md)).
4. DXF import and export ([DXF](dxf.md)), which map onto Mechanical Layers by Purpose.
5. The Component height rule and Height Areas in 2D ([Component height](component-height.md)). Height Areas join the 3D view at step 7.
6. `reda-step`: the reader, mesher and writer, with the corpus test ([STEP](step.md)). It has no UI and is the riskiest piece. **It runs in a second lane from step 0**, beside steps 1 to 5, and must be done before step 8 draws Models.
7. `reda-3d` and the 3D view, drawing the board and Stand-in Bodies ([3D view](3d-view.md)). It comes before Bodies because the Footprint editor's live 3D view needs it.
8. Bodies and Models: the data, `models/<sha256>.stpz`, Add Model Body, placing by numbers, the generators and the checks `model_unreadable` and `body_height_mismatch` ([Bodies](bodies.md), [Models](models.md)).
9. The rest of the 3D view: its Preferences page, Document views, the face keys, Measure height and Measure Clearance ([3D view](3d-view.md)), then placing a Model by picking geometry ([Bodies](bodies.md)).
10. Import part bringing Models along from KiCad files and vendor zips ([Models](models.md)), then the EasyEDA/LCSC importer ([EasyEDA import](easyeda-import.md)).
11. STEP export ([STEP export](step-export.md)).
12. Board Images ([Board Image](board-image.md)).

Every step adds its tests as [testing](testing.md) lays them out. Apart from `reda-step`, the steps run in this one sequence, since at most two implementers build at once.

## Decisions taken while tying the files together

- **Numpad digits count as digits only in the 3D view.** The digit-row rule from [3D view keys](3d-view.md#keys) matched numpad digits too, which clashed with the v0.2 keypad Layer jumps (keypad 2 and 3 would have fired both). In 2D the numpad keeps the Layer jumps. Amends [What do the 3D Preferences, a side-by-side view, the 3D keys and further measuring look like?](https://github.com/openreda/reda/issues/466).

## Smaller calls made while tying the files together

These close gaps that showed up when the files were read side by side. None of them changes a decision, and any of them can be overturned by editing this spec.

- **New Command names:** `view.3d.turn_over` (`*` in 3D), `view.new_document_view`, and `view.grids_guides.show` for Show Grids & Guides ([commands](commands.md#commands)).
- **Save as Template's new tick rows** go through `library.board_template.save`'s existing `objects?`, which can now name Board Grids, Local Grids, Guides and Mechanical Layers.
- **`.reda/view.json`** keeps a list of Document views, each with its mode and projection, and one shared Show set, as [side by side](3d-view.md#side-by-side) requires.
- **Height Areas are a Show set kind** in the 3D view menu and in a Board Image's `show`, off by default.
- **The Stand-in Body** is drawn when a Footprint's Models draw nothing, in the 3D view and in Board Images alike. STEP export still writes a Model that parsed but could not be drawn.
