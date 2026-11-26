# v0.3 Board Image

Source: [What does a rendered 3D board image contain, and how is it asked for?](https://github.com/openreda/reda/issues/458), in scope by [Which adjacent work ships in 0.3.0, and is it one release or two?](https://github.com/openreda/reda/issues/439).

A [Board Image](../../../GLOSSARY.md) is a PNG of the board drawn in 3D, headless, from the same `reda-3d` scene as the [3D view](3d-view.md). It is asked for in three ways: the `document.render` Command, an Output Job output, and Save image… in the 3D view menu. All three draw through one renderer in `reda-render`.

## `document.render` in 3D

`document.render` gains `mode: "2d" | "3d"`. `"2d"` is the default, and the v0.1 2D render ([architecture](../v0.1/architecture.md#mcp)) is unchanged.

- **PCB only.** `mode: "3d"` on a Schematic Sheet is `invalid_params`.
- **2D-only parameters are refused in 3D**, not ignored: `layers`, `active_layer`, `inactive_layers`, `from_bottom`, `ratsnest` and `format: "svg"`. The error names each one given.

### Parameters in 3D

| Parameter | Values | Default |
|---|---|---|
| `camera` | a face name: `top`, `bottom`, `front`, `back`, `left`, `right`, `three_quarter`; or `{azimuth, elevation}` in degrees | `three_quarter` |
| `projection` | `orthographic` or `perspective` | `orthographic` |
| `view` | `fit` (the board) or `{objects: [uuid…]}` (fit those objects) | `fit` |
| `show` | `bodies`, `silkscreen`, `copper`, `board`, each `{on, opacity}`; `solder_mask` and [`height_areas`](component-height.md#in-3d) on or off | all on at full opacity, except `height_areas` off |
| `include_not_fitted` | bool | false |
| `highlight` | UUIDs, drawn in the selection colour with everything else dimmed | none |
| `size` | the longer side in pixels, 1 to 8192 | 1024 |
| `theme` | `dark` or `light` | as in 2D |
| `background` | `theme` (the canvas background of `theme`) or `transparent` | `theme` |

- The face names and their meaning are the 3D view's ([orientation gizmo](3d-view.md#orientation-gizmo)): `bottom` is the board turned over left to right, so text on the bottom reads correctly. `{azimuth, elevation}` turns and tilts the same way the view does, with no roll.
- There is no rectangle in nm and no free camera position.
- The `show` set is the 3D view menu's Show set ([3D view menu](3d-view.md#3d-view-menu)). The image never follows what the user's screen shows, as in 2D.
- What is drawn is what the 3D view draws: material colours, the board with outline, Board Cutouts and holes, outer copper, silkscreen and each Footprint's Bodies. A Footprint with no Body, or whose Models draw nothing, gets its [Stand-in Body](bodies.md#no-body-the-stand-in).
- Not Fitted Components are left out unless `include_not_fitted` is true, as in the [STEP export](step-export.md).
- **Format:** PNG only.

### What comes back

The same shape as 2D:

- The PNG, base64 in `png`, or an image content block over MCP.
- `width` and `height` in pixels.
- `camera`: the view used, as a face name or angles, and the projection.
- `problems`: one entry for each Model left out or replaced by a Stand-in Body, naming the Designator and the Model's file name. This is the same list as the STEP export gives.

`reda run document.render` behaves as it does for 2D.

## Output Job: Board images

- A new Output Job output kind, **Board images**, off by default.
- **Options:**
  - a tick list of views from the face names, defaulting to three-quarter, Top and Bottom
  - size, defaulting to 2048 px on the longer side
  - background, defaulting to transparent
  - the Show set, defaulting to everything on
  - Include Not Fitted, off by default
- **Files:** one PNG per ticked view, named `<stem>-<view>.png` from the job's [Name Template](../v0.1/outputs.md#name-template). For example, `myboard-JLCPCB-three_quarter.png` and `myboard-JLCPCB-top.png`. They are written **beside the zip, never inside it**, as are the STEP and DXF outputs, because the zip is what the fab receives.
- Each image is listed in the Output Manifest with its hash.
- Model problems go to the run report's warnings. They never block the run.
- The schema is in [Commands and file formats](commands.md).

### Determinism

Board Images are exempt from the rule that the same design and REDA version give identical outputs ([outputs](../v0.1/outputs.md#determinism)), as the schematic PDF already is. GPU and software rasterisers differ by a few pixels across machines, and pinning one software renderer would cost speed and still vary across operating systems. The manifest still records each image's hash. The images write no date.

## Save image… in the 3D view

- The [3D view menu](3d-view.md#3d-view-menu) gains **Save image…**, between View and Measure.
- It draws what is on screen through the same renderer: the camera angles, the projection, the Show set and the exact framing (centre and zoom) in use. The selection is not highlighted.
- It is a Command of its own ([ADR-0001](../../adr/0001-every-action-is-a-command.md)), because `document.render` frames only the board or a set of objects. Its arguments are in [Commands and file formats](commands.md).
- The save dialog asks for the file name and a size in pixels, defaulting to 2048, with a background choice of theme or transparent.

## Not in v0.3

- SVG or other vector output of the 3D view.
- Ray-traced or photorealistic rendering.
- Images of a Schematic Sheet in 3D, or of a Footprint in the Footprint editor's 3D view.
- Animations or turntable sequences.
