# v0.2 multilayer outputs

Source: [What do multilayer outputs contain?](https://github.com/openreda/reda/issues/339). It builds on the [v0.1 outputs](../v0.1/outputs.md), on the [Stackup](stackup.md), [Via types](via-types.md) and [inner Layers](inner-layers.md), and on the research into [multilayer outputs in other tools](https://github.com/openreda/reda/issues/327) and [what hobby fabs ask for](https://github.com/openreda/reda/issues/328).

This file says what the v0.1 output set adds for a board with inner Layers or with Vias that aren't through: the copper file functions and names, the drill files per Via Span, the material stackup in the Gerber job file, and the new parts of Fab Notes and the drill report. Everything in the v0.1 file that is not named here is unchanged. The Command ids and schema are decided in their own ticket on the [v0.2 map](https://github.com/openreda/reda/issues/319). CPL corrections are in [placement-corrections](placement-corrections.md).

A 2-Layer board with only through Vias gets the same files as in v0.1, apart from the copper type in the file function and the job file's material stackup. v0.1 already wrote the `PTH` and `NPTH` file functions.

## Copper Gerbers

- **File function:** every copper Layer writes `.FileFunction,Copper,L<p>,(Top|Inr|Bot),<type>`.
  - `p` is the Layer's physical position counted from the top, so the first inner Layer is `L2`. It never depends on the Layer's name or `role`.
  - `<type>` is `Signal`, `Plane` or `Mixed`, taken from the Layer's [`role`](stackup.md). It is written on the outer Layers too.
- **Polarity:** every copper Layer is positive. A plane is a filled [Zone](inner-layers.md), so there is no negative plane file.
- **Always written:** every copper Layer gets a file, even when it is empty, as in v0.1.
- **What is plotted:**
  - A Via's pad is flashed on each Layer of its span at that Layer's [Pad Stack](inner-layers.md) diameter, as `ViaPad`.
  - Where [`unused_layers`](inner-layers.md) leaves a Pad's or Via's copper out, nothing is plotted. No output removes or adds copper of its own.
  - [Teardrops](teardrops.md) are plain copper.
- `outline_on_every_plot`, and the comment form written when `x2_attributes` is off, apply to inner Layers as they do to outer ones.
- IPC-4761 Via protection attributes are not written. Via filling and protection are [after 1.0](../modules.md).

## File naming

- **`classic`:** the inner Layer at position `p` is `<project>.g<p>l`: `.g2l`, `.g3l` and so on. OSH Park and JLCPCB both read this form, and it can't be taken for a count of inner Layers that starts at 1.
- **`descriptive`:** `<project>-<Layer>.gbr` as in v0.1. An inner Layer with no name of its own uses [its position](layer-editing.md): `L2`, `L3`.
- **`file_names`** gains one `inner_copper` pattern, used for every inner Layer in place of the `naming` form. Its tokens are `${position}` (2, 3, …) and `${inner}` (1, 2, …), so a user preset can write `internalplane${inner}.ger` or `${project}.g${inner}`.
  - A pattern with neither token is an inline error on the preset's Gerber page, because every inner Layer would get one name.
- **Shipped presets:** Generic, JLCPCB, PCBWay and OSH Park all use `classic` and set no pattern. PCBWay asks for the copper file names in order on its order form, which [Fab Notes](#fab-notes) list.
- **Name clashes:** two output files that resolve to the same name [block the run](../v0.1/outputs.md#what-blocks-a-run-and-what-only-warns), and the message names both.

## Drill files

- **One file per Via Span and plating**, as the Gerber spec asks.

  | Holes | File | `.FileFunction` |
  |---|---|---|
  | plated, through | `<project>-PTH.drl` | `Plated,1,<n>,PTH` |
  | non-plated | `<project>-NPTH.drl` | `NonPlated,1,<n>,NPTH` |
  | a span ending on an outer Layer | `<project>-L<from>-L<to>.drl` | `Plated,<from>,<to>,Blind` |
  | a span between inner Layers | `<project>-L<from>-L<to>.drl` | `Plated,<from>,<to>,Buried` |

  - Each file function ends, as in v0.1, with `Drill`, `Rout` or `Mixed` for the file's holes, slots or both: `Plated,1,4,PTH,Drill`.
  - `from` and `to` are physical positions with `from` the smaller, so both ends are in the name, as JLCPCB's HDI service asks.
- **The files follow the Vias, not the Via types.**
  - A file is written for every span that at least one Via on the board has. That includes a Via that matches no Via type, which [`via_span`](via-types.md#drc) reports.
  - A Via type that no Via uses writes no file.
  - A [Via Stack](via-types.md) is separate Vias, so each of its holes is in its own span's file.
- **Micro Vias:** a micro span is an ordinary span file. The file function has no marker for it, so the [drill report](#drill-report) and [Fab Notes](#fab-notes) say "micro".
- **Tools:** holes in a span file carry `ViaDrill`.
- **Always through:** through-hole Pads, non-plated holes and slots. A non-plated hole with a partial span would mean back-drilling, which is [after 1.0](../modules.md).
- **`merge_drill`** merges only the two through files. Every other span keeps its own file.
- The format, precision and slot commands are those of [v0.1](../v0.1/outputs.md#excellon).

## File order

- Copper files are written top to bottom, where v0.1 wrote the two copper files.
- Drill files are written as PTH, NPTH, then the other spans by `from` and then `to`.
- The zip, the run report, the Output Manifest, the job file and the Output Job window's Gerber and Drill tables all use this order.

## Gerber job file

- `MaterialStackup` is now always written. v0.1 left it out because its Dielectrics had no material.
- **Entries**, in physical order from the top: legend, solder mask, then copper and Dielectric entries as the Stackup has them, then solder mask and legend.
  - Each entry has its `Type` and what the board knows: `Name`, `Thickness`, `Material`, `DielectricConstant`, `LossTangent` and `Color`.
  - Legend and solder mask entries come from the files written and the board's colours. The Stackup itself still has [no mask entries](stackup.md). A side with no silkscreen file has no legend entry.
  - Several Dielectric sheets between two copper Layers are written as separate entries.
- **A blank Dielectric thickness is left out.** The [shared thickness](stackup.md#blank-thickness) is for display and calculation, and writing it would state a thickness nobody chose.
- The layer count and each file's function follow the sections above.
- No fab is known to read `MaterialStackup`. It is written for the same reasons as the job file itself.

## Fab Notes

Fab Notes gain three parts, and keep the [Construction name and the symmetry and thickness lines](stackup.md).

- **Stackup table**, top to bottom, in place of v0.1's copper-per-Layer lines: Layer, kind (copper, Core or Prepreg), `role`, thickness, material and Dk. Unset values are left blank.
- **Via types table:** name, span, micro, drill, pad, how many Vias use it and its drill file. Vias that match no type are listed by span on rows marked as such.
- **Copper files in order:** the copper file names from top to bottom, one per line.

The [impedance readout](stackup.md) is not printed. It is an estimate, and a number in Fab Notes could be read as an order for controlled impedance.

## Drill report

- One section per drill file, headed by its span, its kind (through, blind or buried) and "micro" where the span's Via type is micro.
- The rows and totals are those of [v0.1](../v0.1/outputs.md#drill-report).

## Output Job window and preset pages

- The Output Job window's Gerber table lists every copper Layer, and its Drill page lists every drill file the board would write.
- The preset's Gerber page gains the `inner_copper` pattern, with a preview of the names for a 4-Layer board.

## Not in v0.2

- **Drawings:** no drill map, no Stackup table on a drawing, and no PDF or print of PCB Layers. They need the PCB print pipeline of [v0.7 Outputs](../modules.md), where v0.1 already put Layer PDFs and drill maps. Fab Notes and the job file carry the same facts as text.
- **Layer markers:** no generated Layer-number or stacking-stripe marker. The user puts text on copper, which v0.2 [indexes and checks](inner-layers.md).
- **HDI limits in a Fab Preset:** no preset key says "through Vias only", and a run adds no warning for Blind, Buried or Micro Vias. The board's Via types and `via_span` say what is allowed, and shipped Constructions carry no Via types, so a new board has only the through type. Fabs sell these Vias as a quoted service, not as a fixed limit.

## Left to other tickets

- The `.redafab` schema for `inner_copper`, with the other schema additions.
- Goldens and read-back tests for inner Layers and span drill files, with the rest of the v0.2 testing.
