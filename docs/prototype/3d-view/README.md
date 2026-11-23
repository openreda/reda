# PROTOTYPE (issue #433): how should the 3D view look and behave?

Throwaway. Never merged.

## Run it

```
git switch prototype/3d-view
cargo run -p reda-app          # open a Project, then its PCB, then press 3
```

`3` enters 3D on a PCB tab and `2` goes back to 2D. F9 and Shift+F9 step the variant.
`REDA_PROTO_3D=1` starts in 3D.

On a machine with no display, draw the screenshots instead (the ones in `shots/` were
made this way):

```
LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl REDA_PROTO_SWITCHER=0 REDA_PROTO_OUT=<dir> \
  cargo test -p reda-ui --test proto3d -- --nocapture
```

## The first three variants

| | A · Same canvas | B · Viewer | C · X-ray |
| --- | --- | --- | --- |
| Idea | The 2D canvas gains depth | A studio you step into | The 2D Layers pulled apart |
| Way in | `3` tilts the very picture 2D showed; `2` tilts back and 2D keeps the framing | A 2D \| 3D switch on the canvas in both modes, and the keys; opens three-quarter on, fitted | The keys; opens tilted, fitted |
| Projection | Orthographic (`P` switches) | Perspective (button switches) | Orthographic |
| Turn | Shift + the 2D pan drag (right or middle) | Left drag | Right drag, about the point under the pointer |
| Pan | Right or middle drag, as 2D | Right or middle drag | Middle drag |
| Zoom | Wheel, at the pointer | Wheel, at the pointer | Wheel, at the pointer |
| Face views | Keys only: `T B F K L R I`, `Home` fits | A view cube, and the keys | Keys only |
| Colours | Materials: mask, finish, silkscreen, FR-4 edge | Materials, on a neutral backdrop | The 2D Layer colours on the 2D background, flat-lit |
| Show and hide | The real Layers panel; `H` for Bodies | A Show card: a switch and an opacity each for Bodies, Silkscreen, Copper, Board; a Solder mask switch | A rail of the stack, one row a Layer; Spread and Cut sliders |
| Selection reads as | The 2D selection colour | The colour, plus a tag with Designator and height | The colour, and everything else dims |
| Measure | One line and one number, as the 2D ruler | A line on the board and a card: distance, ΔX, ΔY, ΔZ | X, Y and Z legs with a number each, and the span |
| Chrome added | One line of keys | Switch, cube, card, toolbar | One rail |

In all three a click selects through the real `selection.set`, so Properties and the 2D
canvas follow, and what the Layers panel hides is hidden. Nothing can be edited.

## Variant D · Same canvas plus

Added after the first reaction (A liked; B's switch covered the Active Bar; B's cube and
transparency liked, its bottom bar not; C not useful for now). It opens first. A's tilt in
and out, 2D mouse and material colours, with:

- the 2D | 3D switch at the canvas's top left, in both modes, clear of the Active Bar;
- a "3D view" chip where the active Layer's chip sits in 2D, opening a menu that holds a
  switch and an opacity for Bodies, Silkscreen, Copper and Board, the Solder mask switch,
  the projection, the views with their keys, Measure and Back to 2D;
- nothing at the bottom.

After the second reaction D also has: an axis gizmo under the chip in place of B's cube,
as 3D tools draw theirs (a click on an end looks along that axis, a drag on it turns the
board); a left drag on the canvas turns the board, beside Shift + the pan drag; and the
measurement shows its length with its X, Y and Z parts under it. Spread, Cut and the
Layer-colour look (C) are dropped for now.

## What building it showed

- The 2D Scene is enough to build from. Each of its primitives already names its object
  and Layer, so the 3D model is the Scene's copper and silkscreen extruded at the
  Stackup's heights, plus the Board Outline with the drill Layer's holes cut out.
  Selection in both directions came for free.
- `usb_c_mcu` is 14,164 triangles and builds in about 3 ms in a debug build. The
  reference Projects are all this small, so nothing here says how a large board behaves.
- Solder mask is a colour here, not a shape: the board's faces and the copper under them
  take the mask colour, Pads take the finish colour, and mask openings are not drawn.
  The Stackup's typed mask and silkscreen colours are honoured when they name a common
  colour.
- A stand-in Body is the Footprint's body box (its Courtyard, drawn a little inside) as
  tall as the stored `height`, or a guess when none is stored. Connectors come out as
  large blocks because their Courtyards are large.
- In materials (A, B) Pads disappear under the Bodies; in C they show through. That is
  the main reason C reads differently, more than its colours.
- The 3D keys (`T B F K L R I M H P`) are taken before the app's own key handling. In the
  real thing they would meet the 2D Tool keys, which the spec has to settle.
- Bottom is the board turned over left to right, so text on the bottom reads correctly.

## What it does not do

- The mouse was not tried by hand: this was built on a machine without a display. The
  camera's maths has a test (`camera_round_trips`), and the screenshots are real frames.
- No section caps, no shadows, no ray tracing, no 3D mouse, no split 2D and 3D pair.
- Translucent kinds are drawn in a fixed order, not sorted.
- Vias are solid-walled holes; blind and buried spans are not told apart.

## Where the code is

- `crates/reda-ui/src/proto3d.rs`: state, keys, the frame, picking and measuring, the
  switcher.
- `crates/reda-ui/src/proto3d/mesh.rs`: the board as triangles, from the Scene and the
  Document.
- `crates/reda-ui/src/proto3d/camera.rs`: the turntable camera.
- `crates/reda-ui/src/proto3d/gpu.rs`, `board.wgsl`: the wgpu pass (4× MSAA, depth).
- `crates/reda-ui/src/proto3d/variant_{a,b,c}.rs`: each variant's mouse and chrome.
- `crates/reda-ui/tests/proto3d.rs`: draws the screenshots.
- Hooks: `docking.rs` (the PCB tab), `app.rs` (keys, switcher, pipelines),
  `reda-render/src/gpu/blit.rs` (`source_view`).

The source scan and accessibility-name checks are not kept green on this branch.
