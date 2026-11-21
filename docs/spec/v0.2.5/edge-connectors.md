# v0.2.5 Edge Connector Pads and Castellated Pads

Source: [How do card-edge fingers pass edge clearance and reach fab with their bevel?](https://github.com/openreda/reda/issues/399). Amended by the M.2 half-moon decision in [scope](scope.md#decisions-taken-while-writing-this-spec).

This file says how copper that must reach the Board Outline is modelled, checked and written to fab. Two kinds of copper do: card-edge fingers and castellated half-holes. It also covers the board's `edge_connector` property. Glossary terms: **Edge Connector Pad** and **Castellated Pad**.

## Edge Connector Pads

An Edge Connector Pad is a Pad role, `role: edge_connector`.

- **Which Pads:** only SMD Pads can have the role. It is set in the Footprint editor's Pad properties.
- **Edge clearance:**
  - The Pad is exempt from `edge_clearance` against the **Board Outline** only.
  - It may touch the outline but not cross it.
  - Board Cutouts, non-plated holes and normal `clearance` still apply.
- **Gerber:** it plots as `.AperFunction,ConnectorPad` (Gerber 2026.05 §5.6.10), on the outer Layers only.
- **Both sides:** a double-sided finger stays two Pads, one per side.
- **Plated Edges:** an Edge Connector Pad on a [Plated Edge](plated-edges.md#drc) is a DRC error.
- `edge_clearance.by_object` stays reserved and unused.

## Castellated Pads

A Castellated Pad is a plated Pad with the property `castellated`: a plated hole that the Board Outline cuts through.

- **Edge clearance:** it is exempt from `edge_clearance` against the outline.
- **Gerber and drill:** it plots `CastellatedPad` on copper and `CastellatedDrill` on the drill file.
- **Job file:** it sets `"Castellated": true`.
- **Which Pads:** only plated through-hole Pads can be castellated. The property is set in the Footprint editor's Pad properties, and on a Free Pad.

## The card-edge Parts

- The shipped finger Parts ([catalogue](catalogue.md#new-starter-parts)) mark their fingers `edge_connector`. They keep the research setbacks: PCIe pads start 1.40 from the edge, and M.2 pads 0.50.
- **The M.2 half-moon** is the Mechanical Part `MountingHole_M2_Castellated`. Its one Castellated Pad has a Ø3.5 hole, and the template places it centred on the card's far edge.
  - The outline stays straight there. The fab's drill-then-rout makes the half-moon.
  - This replaces the R1.75 outline notch from [form factors](https://github.com/openreda/reda/issues/397).
  - It is a Mechanical Part, not a Pad inside the card-edge Part, because the far edge moves with the `length` choice.

## The `edge_connector` board property

`edge_connector?: {bevel?: {angle, depth}, hard_gold?}` lives on the PCB Document.

- **Where it is edited:** the Stackup page of Board Setup, beside finish. The Command is `pcb.edge_connector.set` ([commands](commands.md#board-commands)).
- **Not stored:** whether the board has an edge connector. That is derived from whether any Pad has the `edge_connector` role.
- **`hard_gold`** means selective hard gold on the Edge Connector Pads only. The board finish is unchanged.

### Job file and Fab Notes

| When | Job file `GeneralSpecs` | Fab Notes |
|---|---|---|
| any Edge Connector Pad exists | `EdgeConnector: true` | |
| `bevel` is set | `EdgeConnectorBevelled: true`, and the angle and depth in `Notes` | "Bevel edge-connector edge: 20°, 1.30 mm" |
| `hard_gold` is set | `HardGoldArea`: the summed copper area of the Edge Connector Pads, in mm² | "Hard gold on edge-connector Pads, N mm²" |

- Neither Gerber spec has a field for the bevel's angle or depth, which is why they go in `Notes` and Fab Notes.
- No Goldmask Gerber is written, because `ConnectorPad` already shows the fab where the gold goes.

### DRC

A warning is raised when `bevel` or `hard_gold` is set on a board with no Edge Connector Pads.

### Templates

- PCIe sets `bevel {20°, 1.30}`, from TE drawing 2364427, the only source.
- M.2 sets no bevel, because no source gives a figure. Its description says so.
- All templates leave `hard_gold` off, since it is a paid option. Their descriptions recommend it for cards that are inserted often.

## Sources

- Gerber Layer Format 2026.05 §5.6.3 and §5.6.10.
- Gerber Job File 2020.08 / schema 2023.06 §2.4.
- KiCad master `drc_test_provider_edge_clearance.cpp` (hard-coded pad-type skip), `gerber_jobfile_writer.cpp` and `plot_brditems_plotter.cpp`.
- Altium's Board Outline Clearance rule (per-object-kind values, no finger property).
