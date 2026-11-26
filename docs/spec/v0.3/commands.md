# v0.3 Commands and file formats

Source: [What Commands and file-format additions does the v0.3 data need?](https://github.com/openreda/reda/issues/440). It builds on the v0.1 [Commands](../v0.1/architecture.md#commands), [file format](../v0.1/architecture.md#file-format) and [reserved names](../v0.1/architecture.md#reserved-names), on the v0.2 [Commands and file formats](../v0.2/commands.md), and on every other file in this folder, each of which left its Command ids and schema to this one.

This file gives the format version step, the additions to `.redapcb`, the Library files, `.redaboard`, `.reda`, `.reda/view.json` and `preferences.toml`, and the Command ids. It decides no behaviour: what each thing does is in the file it links to.

## Format version

- **0.2.0 is released from main before the v0.3 build starts.** That release writes format 2, so it ends editing format 2 in place, as 0.1.0 did for format 1.
- Every file kind moves to `"format": 3`. One migration step, 2 to 3, is golden-tested against files written by 0.2.0. The 1 to 2 step is unchanged, so a 0.1.0 file runs both.
- The step is written although [Mechanical Layers](mechanical-layers.md#what-changes-from-v025) allow a break, because it is small and keeps the "one step per version" rule whole.

| File | What the step does |
|---|---|
| `.redapcb` | the Mechanical Layer becomes `{kind: mechanical, name: "Dimensions", purpose: dimensions}`; the Documentation Layer becomes `{kind: mechanical, name: "Reference", purpose: reference}`; both move to after the Board Outline, Dimensions first. A board-wide `component_height` record with no values is added |
| `.redaboard` | a drawing group's `layer: mechanical` becomes `purpose: dimensions`, and `layer: documentation` becomes `purpose: reference` |
| all others | the number only |

- A migrated Footprint has no `bodies`, so it shows its [Stand-in Body](bodies.md#no-body-the-stand-in). Nothing generates Bodies on load; regenerating a generated Footprint does.
- **Reserved names that v0.3 uses leave the reserved list:** Layer `purpose`, Footprint `bodies`, PCB Document `grids` and `guides`, and Preferences `canvas.wheel.*`. Footprint `guides` is new.
- `docs/schemas/v2/` is frozen from the 0.2.0 release, as `v1/` was, for the step's input validation.

## `.redapcb`

### Layers

- `LayerKind` loses `documentation`. `mechanical` now means a [Mechanical Layer](mechanical-layers.md#a-mechanical-layer):

| Key | Value |
|---|---|
| `name` | required on a Mechanical Layer, unique among them |
| `purpose` | `dimensions`, `fabrication_notes`, `assembly_notes` or `reference`; left out when `general` |
| `side` | the existing key, `top` or `bottom`; left out when unsided |

- `purpose` is refused on every other kind.
- Mechanical Layers sit after the Board Outline, in the user's order. That order is the Layer order the [first-with-Purpose rule](mechanical-layers.md#purpose) uses.
- A board may have no Mechanical Layer at all.

### Grids and guides

[Grids and guides](grids-guides.md#file-format):

- **`grids`**: `{board: [...], local: [...]}`, both left out when empty.
  - A Board Grid: `{uuid, step_x, step_y?, visible?, in_cycle, default?}`. Steps are [`Length`](../v0.1/editing.md#grid) text such as `"0.5mm"` or `"25mil"`, so a value keeps its unit. `visible` is the coarser shown grid, as in a preset row. At most one row has `default: true`.
  - A Local Grid: `{uuid, name, applies_to, rotate_components?, shown, enabled, locked, shape}`. `applies_to` is `all`, `components` or `not_components`. `shape` is `{kind: cartesian, origin, rotation, step_x, step_y, width, height}` or `{kind: polar, centre, angle_step, radial_step, start_angle, end_angle, min_radius, max_radius}`. Positions are in nm, angles in millidegrees, steps in `Length`. The list order is the priority, first wins.
- **`guides`**: `[{uuid, kind, at, angle?, name?, shown, enabled, locked}]`, with `kind` as `line` or `point` and `angle` only on a line, in millidegrees.

### Keepout and rules

- A Keepout gains `max_height` in nm, left out when none. It is refused unless `restrict` holds `footprints`, and must be positive ([Height Areas](component-height.md#height-areas)).
- A new rule kind `component_height` with `top_max?` and `bottom_max?`. Board-wide only: a `scope` is refused. The DRC severities table gains `component_height` (E) and `height_unknown` (W).

## Library files

### `.redafp`

- **`bodies`**: an ordered list ([Bodies](bodies.md#bodies)). Each entry is one of:
  - **Model Body:** `{uuid, kind: model, model, file_name, offset {x, y, z}, rotation {x, y, z}, material?}`. `model` is the Model's hash. `offset` is in nm, with `z` from the board surface on the Footprint's side. `rotation` is in millidegrees, applied about the fixed axes in the order KiCad applies them, so an imported `(rotate (xyz …))` carries across unchanged. The build checks that order against KiCad's source.
  - **Extruded Body:** `{uuid, kind: extruded, outline, bottom, top, material}`. `outline` is a Polygon or a Circle, as a graphic's shape. `bottom` (the standoff) and `top` are in nm from the board surface.
- **`material`**: `black_plastic`, `grey_plastic`, `tin`, `gold`, `copper`, `ceramic` or `glass`, or `{colour: "#rrggbb"}`.
- **`guides`**: as the PCB Document's.
- `height` is unchanged.
- **Generators** gain `standoff` in every family, and DIP and header gain their body dimensions ([Generated Bodies](bodies.md#generated-bodies)). They are generator parameters stored with the Footprint's generator record like the existing ones.

### Models

- `models/<hash>.stpz` beside `footprints/` in a Library, and `models/` in the Project folder, beside the Documents ([Models on disk](bodies.md#models-on-disk)).
- `<hash>` is SHA-256 over the uncompressed STEP text, in lowercase hex, using the in-house hasher the Output Manifest already uses.
- Neither folder has an index file: a Model's identity is its name.

## `.redaboard`

- A drawing group is `{uuid, name, purpose, when?}`, with `purpose` as on a Layer (`general` written out, since a group always names one).
- A template may carry `grids` and `guides`, with the PCB Document's shape ([Board Templates](grids-guides.md#board-templates)).
- A template Keepout may carry `max_height`.

## `.reda`

### Output Jobs

- `outputs` gains `step`, `dxf` and `board_images`, all `false` by default.
- The job gains three option blocks, each left out while it holds its defaults:

| Block | Keys | Defaults |
|---|---|---|
| `step` | `via_holes`, `skip_holes_under?`, `include_not_fitted`, `stand_ins`, `name_prefix?` | off, none, off, on, none |
| `dxf` | `entries`, `unit` | Board Outline and drilled holes, `mm` |
| `board_images` | `views`, `size`, `background`, `show`, `include_not_fitted` | `three_quarter`, `top`, `bottom`; 2048; `transparent`; all on; off |

- The keys are the same as the parameters of `pcb.export_step`, `pcb.export_dxf` and `document.render`.
- A DXF `entries` item is `board_outline`, `holes`, a Layer UUID, or `{kind: assembly | courtyard, side}`.
- The files are named `<stem>.step`, `<stem>.dxf` and `<stem>-<view>.png` beside the zip, and listed in the Output Manifest.

## `.reda/view.json`

Per PCB Document and per Footprint tab, never dirtying a Document:

- `mode`: `2d` or `3d`, the mode the canvas was last in. A board reopened in 3D enters with the tilt.
- `view3d`: `{projection, show}`, with `show` as in `document.render`, plus `height_areas`.
- The 3D camera is not kept.

## Preferences

| Key | Value |
|---|---|
| `canvas.wheel.zoom`, `.scroll_x`, `.scroll_y`, `.step_layer` | a modifier combination such as `""`, `"shift"` or `"ctrl+alt"`, or `"unbound"` |
| `canvas.wheel.zoom_speed`, `.reverse_zoom` | number, bool |
| `canvas.wheel.touchpad` | `auto`, `pan` or `wheel` |
| `canvas.arrows_move_pointer` | bool, on |
| `canvas.alignment_range.board`, `.sheet` | a length |
| `snap.guides`, `snap.local_grids`, `snap.alignment_guides` | bool, on |
| `defaults.remember_last_used` | bool, on |
| `defaults.<editor>.<kind>.<field>` | an Object Default; for the Line family and Text on boards and Footprints, `defaults.pcb.<kind>.<layer_kind>.<field>` |
| `import.kicad_3d_models_dir` | a path, filled by auto-detection |

- The wheel preset (REDA, Wheel scrolls, Custom) is read off the table and never stored.
- `snap.alignment_guides` replaces `pcb.place.alignment_guides`. `pcb.zone.tool.*`, the Keepout Tool values and the Footprint editor's Pad template move under `defaults.*`.
- `preferences.toml` has no format number, so old keys are dropped on load with a log line.
- The 3D Preferences are still open on the [map](https://github.com/openreda/reda/issues/421).

## Commands

All ids follow the v0.1 [definition](../v0.1/architecture.md#definition). A Command called with full params never asks. Each family keeps its existing verbs: PCB Document objects use `add`, `set` and `delete` (as `pcb.keepout.*`), and Library elements use `add`, `edit` and `remove` (as `library.footprint.pad.*`). This renames the verbs [grids-guides](grids-guides.md#commands) and [input-and-defaults](input-and-defaults.md#commands) wrote.

### Mechanical Layers

The `pcb.layer.*` family takes Mechanical Layers:

| Command | Change |
|---|---|
| `pcb.layer.add` | gains `kind?` (`copper` by default). With `kind: mechanical` it takes `name?`, `purpose?`, `side?` and `before?`, and `split` is refused |
| `pcb.layer.set` | takes `purpose` and `side` on a Mechanical Layer; its `name` can't be cleared |
| `pcb.layer.remove` | removes a Mechanical Layer with its objects; `pcb.stackup.change_preview` lists them |
| `pcb.layer.move` | moves a Mechanical Layer up or down among the Mechanical Layers |

### Grids and guides

| Command | Does |
|---|---|
| `pcb.board_grid.add / set / delete` | Board Grids, the `default` mark included |
| `pcb.local_grid.add / set / delete / reorder` | Local Grids; `reorder {local_grid, before?}` |
| `pcb.guide.add / set / delete` | Guides on a board |
| `library.footprint.guide.add / edit / remove` | Guides in a Footprint |
| `pcb.align` and `schematic.align` | gain `key?`, the object that stays put |

### Input

| Command | Does |
|---|---|
| `view.cursor.step {dx, dy}` | moves the Edit Cursor by whole grid steps |
| `view.cursor.click` | a left click at the Edit Cursor |
| `pcb.nudge`, `library.footprint.nudge`, `library.symbol.nudge` `{dx, dy}` | nudge the selection by grid steps, as `schematic.nudge` |
| `app.defaults.reset {kind?}`, `app.defaults.save {path, kind?}`, `app.defaults.load {path}` | Object Defaults; `load` returns the skipped count |

### Bodies and Models

Edit Commands on the Library element's undo stack, one entry each.

| Command | Does |
|---|---|
| `library.footprint.body.add {footprint, body}` | adds an Extruded Body, or a Model Body for a Model already embedded |
| `library.footprint.body.edit {body, …}`, `.remove {body}`, `.reorder {body, before?}` | edit, remove, reorder |
| `library.footprint.body.add_model {footprint, path}` | reads the file, runs the [attach checks](models.md#checks-when-a-file-is-attached), embeds it and adds a Model Body at zero offset. Returns the warnings |
| `library.footprint.body.replace_model {body, path}` | swaps the file, keeping placement and Material |
| `library.footprint.body.save_model {body, path}` | a query: writes the Model out as `.step` or `.stpz` |
| `library.footprint.body.seat_on_board {body}`, `.centre_on_pads {body}` | the [two aids](bodies.md#in-the-footprint-editor) |
| `library.footprint.body.from_courtyard {footprint}` | a new Extruded Body from the Courtyard, top at `height` |
| `library.footprint.set_height_from_bodies {footprint}` | `height` from the top of the Bodies |

- `library.import_part` takes the new inputs (zip, folder, EasyEDA files), `devices?` to tick devices in a multi-device file, and `models?`, a map from part to a STEP path. Its preview reports models found, not found and mismatched ([Models](models.md#import-part), [EasyEDA import](easyeda-import.md)).

### 3D view

View Commands in the app's registry.

| Command | Key |
|---|---|
| `view.3d`, `view.2d` | `3`, `2` |
| `view.3d.look {face}` | per face, left to the 3D view keys on the map |
| `view.3d.fit`, `view.3d.projection {projection}`, `view.3d.measure` | |
| `view.3d.show {kind, on?, opacity?}` | a row of the Show set |
| `view.3d.save_image {path, size?, background?}` | the on-screen camera and framing |

### Rules and Keepouts

- `pcb.rules.set`, `.validate` and `.applicable` take `component_height`. `pcb.rules.applicable` lists Height Areas for a Component.
- `pcb.keepout.add` and `pcb.keepout.set` take `max_height`, and `null` clears it.

### Outputs

| Command | Does |
|---|---|
| `pcb.export_step {path, via_holes?, skip_holes_under?, include_not_fitted?, selection_only?, stand_ins?, name_prefix?}` | writes the [STEP export](step-export.md); returns the problems list |
| `pcb.import_dxf.preview`, `pcb.import_dxf`, `pcb.export_dxf` | as [DXF](dxf.md#commands) |
| `document.render` | gains `mode` and the [3D parameters](board-image.md#parameters-in-3d) |
| `output.job.add`, `output.job.set` | take the new `outputs` switches and option blocks |

## Left to other tickets

- Goldens for the 2→3 step, the new schema and the new Commands: [Which tests does v0.3 add?](https://github.com/openreda/reda/issues/441).
- The 3D Preferences, the face-view keys and what the 2D Tool keys do in 3D: in the map's fog.
