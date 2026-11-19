# v0.2 testing

Source: [Which tests does v0.2 add for multilayer boards?](https://github.com/openreda/reda/issues/353). It builds on the [v0.1 testing](../v0.1/quality-and-release.md#testing) and on every other file in this folder, each of which left its goldens and scenarios to this one.

This file says which reference Projects, goldens, read-back tests, scenarios, property tests, UI tests and snapshots v0.2 adds. Everything in the v0.1 testing section that is not named here is unchanged: the kinds of test, the deterministic engine, `reda-testkit`, the rule against GPL tools, and the Command coverage test. CI gets no new job and the merge gate does not change.

## Reference Projects

The three v0.1 reference Projects stay 2-Layer boards. They prove that a 2-Layer board behaves as it did. Two are added under `tests/projects/`:

| Project | Board | What it carries |
|---|---|---|
| `four_layer` | signal / plane / plane / signal, from a shipped JLCPCB [Construction](stackup.md#construction-name) | through Vias only; a rule record with a `layers: inner` scope; a `net_class_pair` clearance; `unused_layers: remove` on some Vias; Teardrops on for all four targets; Placement Corrections, with Components on both sides that a Footprint row and a field row each match |
| `six_layer_hdi` | six copper Layers, one of them `mixed`, one inner Layer named | Blind, Buried and Micro Via types, and Micro Vias stacked end to end; a through-hole Pad with top / middle / bottom [Pad Stacks](inner-layers.md); a Net Class with an `allowed` Via type list |

- Each is built by its own script in `tests/scenarios/`, then kept as files, as in v0.1.
- Each gets the full golden set: ERC, DRC, Sync and every output.
- Both are DRC-clean. The new checks fail on purpose [in the scenarios](#drc-checks-that-must-fire).

## Migration

The step from format 1 to 2 is in [commands](commands.md#format-version).

- **Frozen inputs,** exactly as `0.1.0` wrote them:
  - the three v0.1 reference Projects;
  - one `.redafab`;
  - one `.redarules`;
  - one of each Library file kind.
- **Goldens:** the migrated files.
- **Unchanged outputs:** a test runs the outputs of each migrated board and compares them with the outputs `0.1.0` wrote.
  - The Gerber, drill, BOM and CPL files must be byte-identical apart from what [outputs](outputs.md) says a 2-Layer board gains: the copper type in the file function and the `PTH` file function on the plated drill file.
  - The job file, Fab Notes and the Output Manifest may differ, and are goldens of their own.
- `proptest` covers the migration with the new fields, as it covers save → load → save identity.

## Output goldens

Byte-exact goldens for the two new reference Projects, under the v0.1 rules (fixed `Clock` and REDA version). Between them they hold:

- inner copper Gerbers, in `classic` naming and with an `inner_copper` pattern;
- one drill file per Via Span and plating, with `Blind` and `Buried` file functions;
- the job file with its `MaterialStackup`;
- Fab Notes with the Stackup and Via types tables;
- the drill report;
- CPL files for top and bottom with Placement Corrections applied.

## Read-back tests

v0.1 reads its Gerbers back. v0.2 reads back four outputs, each with an in-house reader and no new dependency.

| Output | Compared with the model |
|---|---|
| Gerber | every copper Layer, inner ones included: copper left out by `unused_layers` is absent, and Teardrop copper is present |
| Drill | a minimal Excellon reader: every Via and hole is in exactly one file, and that file's span and plating are the Via's or hole's |
| Job file | `MaterialStackup`, entry by entry against the Stackup |
| CPL | each row's rotation and position are the Component's with its [Placement Correction](placement-corrections.md) applied, on top and on bottom |

## Scenarios

- **One data-driven `.jsonl` scenario per area,** beside the two that build the reference Projects:

  | Scenario | Area |
  |---|---|
  | `stackup` | [stackup](stackup.md) values, Dielectric sheets, saving and applying a Construction |
  | `via_types` | [via-types](via-types.md) |
  | `layer_count` | [layer-count](layer-count.md): add, remove, move, and Apply Construction with more and with fewer Layers |
  | `inner_layers` | [inner-layers](inner-layers.md): Pad Stacks, `unused_layers`, Zone connection on a Via |
  | `layer_rules` | [layer-rules](layer-rules.md), with a Rules File exported and imported onto boards with the same and a different copper count |
  | `teardrops` | [teardrops](teardrops.md) |
  | `placement_corrections` | [placement-corrections](placement-corrections.md) |

- **Refusals are Rust scenarios,** because a Journal records only Commands that succeeded. Every `rejected` and `invalid_params` case that [commands](commands.md) names has one: a 33rd Layer, removing an outer Layer, a second Via type on a span, removing the through type, removing the only Dielectric between two Layers, `middle` on an SMD Pad, `remove_keep_ends` on a Pad, and a `construction` with the wrong Layer count.
- **Command coverage:** the v0.1 test is unchanged, so every new Command id, edit, query and view, must be run by a test.

### DRC checks that must fire

Each new check has a deliberately broken case, with a DRC golden, in its area's scenario:

| Check | Scenario |
|---|---|
| `via_span`, `microvia_size`, `aspect_ratio`, `max_stack` | `via_types` |
| `clearance` `hole {via, pad}`, copper text and graphics | `inner_layers` |
| `silk_width`, `silk_text_height` | `layer_rules` |

### The 32-Layer limit

A Rust scenario generates a 32-Layer board. It is not a reference Project.

- A through Via spans all 32 Layers.
- The outputs name `L2` to `L31` by position.
- A 33rd Layer is refused.
- An `#[ignore]` timing test runs on the same board. There is still no performance gate.

## Property tests

`proptest` gains:

- **Layer changes:** a random sequence of Layer add, remove, move and Apply Construction leaves a valid board, where every Via and Via type ends on Layers that exist and no rule names a missing Layer. Undoing the whole sequence restores the saved bytes.
- **Rule resolution:** the resolved value equals that of a naive reference implementation of [the order](layer-rules.md): pair, then Net Class, then board-wide, with a Layer record ahead at each step.
- **Teardrops:** turning Teardrops on never adds a clearance violation.
- **Identity and migration:** save → load → save and the 1 to 2 step cover the new fields.

## Impedance and shipped data

- **Impedance readout:** unit tests against the reference values in [the formula research](https://github.com/openreda/reda/issues/329), within a stated tolerance, including the cases the readout marks as rough.
- **Shipped Constructions:** a data test loads every one, checks that its entries sum to its stated thickness, and checks symmetry where the fab publishes it as symmetric.

The tolerance and the Construction values are set with the shipped Fab Preset data on the [v0.2 map](https://github.com/openreda/reda/issues/319).

## UI tests

`egui_kittest`, asserting on the Journal as in v0.1:

- **Stackup editor,** the two [direct edits](stackup-editor.md) in the cross-section:
  - dragging a Via type's end gives exactly one `pcb.via_type.set`;
  - adding a Layer in a Dielectric gives exactly one `pcb.layer.add`.
- **Layer keys:** `V`, `+`/`-`, `*` and `Mod+L`, asserted on the active Layer. In a route, a Layer key places the proposed Via.

## Rendering

- **Scene text snapshots** of both new reference Projects: inner Layers coloured by position, the `L2` style names, Teardrops, and Pads left out by `unused_layers`.
- **The Stackup editor's cross-section** as a Scene text snapshot.
- **PNG snapshots,** Linux only: one of `six_layer_hdi` and one of the Stackup editor tab.

## Fab check by hand

A maintainer check. It never gates a 0.x release.

- **`four_layer`:** upload the zip, BOM and CPL to JLCPCB's and PCBWay's viewers, and open the zip in the publisher's reference Gerber viewer. Check the inner-Layer order, the drill files and the corrected CPL rotations.
- **`six_layer_hdi`:** the reference Gerber viewer only, because hobby fab viewers don't take HDI orders. Check that each span drill file is read with its span.

A failure reopens the decision it tests.
