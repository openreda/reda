# v0.2 Editing across many copper Layers

Source: [How does the PCB editor work across N copper Layers?](https://github.com/openreda/reda/issues/333). It builds on the v0.1 [View](../v0.1/pcb.md#view), [Layer control](../v0.1/pcb.md#layer-control), [Vias](../v0.1/pcb.md#vias) and [keys](../v0.1/app.md), on the [Stackup](stackup.md) and [Via types](via-types.md), and on the research into [many-Layer editing in other tools](https://github.com/openreda/reda/issues/322).

This file says how a board's copper Layers are named, coloured, listed, switched between and shown once there are more than two. What happens to copper when a Layer is added or removed, the Command ids and the schema are decided in their own tickets on the [v0.2 map](https://github.com/openreda/reda/issues/319).

A 2-Layer board behaves as in v0.1. Everything here works the same from 2 to 32 copper Layers.

## Names and positions

- **Position:** every copper Layer has a position counted from the top, written `L1`, `L2`, and so on. It is worked out from the Stackup's order and never stored.
- **Names:** Top and Bottom keep their v0.1 names. A new inner Layer has no stored `name`.
- **How a Layer is shown,** wherever Layers are listed:
  - an unnamed inner Layer shows as its position: `L2`;
  - a named inner Layer shows as its position and name: `L2 GND`.
- Inserting or removing a Layer therefore never leaves a stale automatic name. The leading suite's `Mid-Layer n` and KiCad's `In1.Cu` are stored names; REDA's are not.

## Colours

- **Inner colour follows position, not the Layer.** The colour keys are `inner_1` to `inner_30`, for the first to the thirtieth inner Layer counted from the top.
- **Defaults:** a palette of 8 distinct colours per app theme, repeating every 8, kept clear of the Top and Bottom copper colours.
- **Overrides** are Preferences, per key and per theme, with the other [Layer colour overrides](../v0.1/app.md#preferences). Nothing is stored per board.
- Inserting a Layer shifts the colours of the inner Layers below it. In return, `L2` looks the same on every board.

## Switching Layers

### The Previous Layer

- The editor keeps the **Previous Layer**: the copper Layer that was active before the current one ([glossary](../../../GLOSSARY.md)).
- **Every change of the active copper Layer updates it:** a key, a click on a Layer chip, a jump, or a route picking up copper on another Layer.
- On a board never opened before, the Previous Layer is the outer Layer opposite the active one.
- It is kept in the PCB Document's editor state, with the [Via choice per Layer pair](via-types.md). It is not design data.
- **There is no stored Layer pair,** no pair setting and no pair presets. KiCad has them; the Previous Layer gives the same two-Layer back-and-forth with nothing to set up.

### Keys

| Key | Outside a route | In a route |
|---|---|---|
| `+`, `-` | next or previous shown copper Layer, in Stackup order, wrapping round | the same, placing the proposed Via |
| `*`, `Shift+*` | next or previous shown copper Layer whose `role` is signal or mixed | the same, placing the proposed Via |
| `V` | nothing | place the proposed Via and go to the Previous Layer |
| keypad `1` to `9` | make `L1` to `L9` active | place the proposed Via and continue on that Layer |
| `Mod+L` | open the Layer list at the cursor | open it; the chosen Layer is jumped to in the same way |

- **Hidden Layers are skipped** by `+`, `-` and `*`, so a [Layer Set](#layer-sets) narrows the stepping.
- **`*` skips Layers whose `role` is plane.** On a signal, plane, plane, signal board it swaps Top and Bottom, as `*` did in v0.1.
- **`V` twice returns to where it started:** from Top, `+` to `L2`, `V` goes to Top and `V` again goes to `L2`.
- **A jump to a hidden Layer shows it first.** A keypad digit with no such Layer does nothing.
- **The Layer list** (`Mod+L`) holds every copper Layer in Stackup order, each as a chip. It is the way to reach Layers past `L9`, and the direct jump on a keyboard with no keypad.
- **A click on a Layer chip during a route** is a jump too: it places the proposed Via and the route continues on that Layer. This holds on the Layer control and in the Layers panel.
- The Track Tool's main-row digits keep their v0.1 meanings, and `6` and `8` choose among the [Via candidates](via-types.md). `L` stays the start Layer.
- The proposed Via for any of these is the one [Via types](via-types.md) picks for the two Layers being joined.
- The keypad jump and the list are the leading suite's (`Ctrl+L` there). `V` to the Previous Layer is REDA's own.

## Layer control and Layers panel

Both list copper in Stackup order, each Layer as a chip with its position, its name and a small mark for its `role`.

- **Swatch** (still the default): the dropdown scrolls once it holds more than about 16 rows.
- **Strip:** it holds the shown Layers only.
  1. Where the names do not all fit, every chip shows its swatch alone, as in v0.1.
  2. Where the swatches do not fit either, the strip scrolls sideways with an arrow at each end, and keeps the active Layer's chip in view.
- **Layers panel:** the copper group can be collapsed and scrolls with the panel.
- There is no filter box, and there are no per-Layer letters.

## Layer Sets

- **Signal:** the copper Layers whose `role` is signal or mixed. In v0.1 it was all copper.
- **Plane** (new): the copper Layers whose `role` is plane or mixed.
- All, NonSignal, Mechanical, Top, Bottom and Fab are unchanged. Top and Bottom hold no inner Layers.
- **User-saved sets** stay in Preferences. They record inner Layers by position and apply to those the board has.

## Single-Layer mode

- No new modes: Normal, Dimmed and Hidden on `Shift+S`, with Pinned Layers, as in v0.1.
- **A Via or a through-hole Pad counts as on the active Layer** when its span includes that Layer. It is then drawn at full strength; otherwise it is dimmed or hidden with the rest.
- What can be selected on a dimmed Layer follows the v0.1 [selection rules](../v0.1/editing.md#selection). KiCad makes dimmed items unselectable; REDA does not.

## Hover to solo

- Resting the pointer on a Layer chip for about 300 ms shows that Layer and the Pinned Layers alone, as Hidden single-Layer mode would.
- It ends the moment the pointer leaves the chip.
- It works on the Layer control and in the Layers panel.
- It changes no state and is not a Command.
- It does not happen while a Tool has an operation in progress.
- A Preference turns it off. It is on by default.

## Draw order and flip

- Layers are drawn in physical order, the far side first, with the active Layer last, as in v0.1.
- **View from bottom reverses the whole copper order.** The v0.1 wording, "swap draw order in pairs", was written for two Layers.
- There is no draw-order setting per board.

## The Via Span label

Decided in [Via types](via-types.md#display): the span as copper positions, such as `1:3`, inside every Via, off by default.

## Left to other tickets

- What happens to copper, Via types and the Previous Layer when a Layer is added, removed or moved: [What happens when a board's Layer count changes?](https://github.com/openreda/reda/issues/334).
- Whether `role` restricts what may be drawn on a Layer: [How do Pads, Vias and planes behave on inner Layers?](https://github.com/openreda/reda/issues/335).
- The Command ids for the jump and the Layer list, and the Preference keys: the Command and file-format surface on the [v0.2 map](https://github.com/openreda/reda/issues/319).
