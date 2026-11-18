# PROTOTYPE (issue #330): how should the Stackup editor tab look and behave?

Throwaway. Never merged.

## Run it

```
git switch prototype/stackup-editor
REDA_PROTO_STACKUP=1 cargo run -p reda-app     # open a Project, then its PCB
```

A PCB Document's tab shows the Stackup editor in place of the canvas. F9 and Shift+F9 step
the variant, F10 turns the prototype off and on. The picker at the top left loads a sample:
2, 4, 6 or 8 Layers (the 8 Layer one has four Via types).

On a machine with no display, draw the screenshots instead:

```
LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl REDA_PROTO_SWITCHER=0 REDA_PROTO_OUT=<dir> \
  cargo test -p reda-ui --test stackup_proto -- --nocapture
```

## The three variants

| | A · Drawing | B · Ledger | C · Workbench |
| --- | --- | --- | --- |
| Idea | The cross-section is the editor; no table | One grid whose rows are the bands | Tables to edit, a preview to check |
| Where you edit | In the drawing and an inspector card | In grid cells | In tables |
| Reordering Layers | Drag a copper Layer, or buttons | Buttons | Buttons |
| Via type span | Drag a barrel's end | Click a copper row in its column | From and To drop-downs |
| To scale | Switch; a dielectric's edge drags | A side strip only | Switch |
| Impedance | Beside the Layer | A column | Its own table |
| Total | Badge, top right | Rows under the stack | Dimensioned on the drawing |
| Finish, colours, Fab Notes | Stay in Board Setup | Stay in Board Setup | Move into the tab |

## What building it showed

- With both Docks and the Messages panel open, the tab has about 985 × 650 points in a
  1600 × 1000 window. Three panes side by side do not fit; variant C became two.
- To scale, 0.5 oz copper on a 1.6 mm board is about 5 px tall in that space. It can be
  seen but is hard to hit, so selection needs the labels or a table as well.
- B is the widest: with four Via types its last column runs off the side.
- A Via type's kind (through, blind, buried, micro) can be read off its span; none of the
  variants stores it.

## Where the code is

- `crates/reda-ui/src/proto.rs`: on and off, the variant switcher.
- `crates/reda-ui/src/proto/model.rs`: the in-memory Stackup all three edit. Not the v0.2
  model. The impedance formula is a rough stand-in.
- `crates/reda-ui/src/proto/variant_{a,b,c}.rs`: the variants.
- `crates/reda-ui/tests/stackup_proto.rs`: draws the screenshots.
- Hooks: `docking.rs` (the PCB tab's content and title), `app.rs` (keys, switcher).

The source scan and accessibility-name checks are not kept green on this branch.
