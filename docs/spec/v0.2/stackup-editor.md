# v0.2 Stackup editor

Source: [How should the Stackup editor tab look and behave?](https://github.com/openreda/reda/issues/330), settled by prototype. The prototype (four variants, of which this is D) is on the throwaway branch [`prototype/stackup-editor`](https://github.com/openreda/reda/tree/prototype/stackup-editor). The editor is in `reda-ui`.

This file says what the tab shows and how it is used. What the Stackup stores, how Via types are defined and checked, and the Command ids are decided in their own tickets on the [v0.2 map](https://github.com/openreda/reda/issues/319) and are only named here.

## The tab

- The Stackup editor is a tab bound to its PCB Document ([modules](../modules.md)). It edits that board's one Stackup.
- **Two panes:**
  - On the left, a column of tables that scrolls as one: [Layers](#layers-table), [Via types](#via-types-table), then [Board](#board-form).
  - On the right, the [cross-section](#cross-section), filling the rest of the tab.
- **The tables are where the Stackup is edited.** Every value can be typed and every structural change made from the left pane, with the keyboard alone.
- **The cross-section is a faithful drawing of the same Stackup.** It selects, and it offers the two direct edits whose targets are easy to hit. Everything it can do, a table can do too.
- Selection is shared: a click on a band or a barrel selects its row, and a click on a row marks its band or barrel.
- **Size:** the tab is designed for a 2.5K or 4K screen. On a small window the left column scrolls and the cross-section keeps its pane.

## Layers table

- One row per Stackup entry, top to bottom: copper Layers and the dielectrics between them.
- **Columns:**

  | Column | Copper Layer | Dielectric |
  |---|---|---|
  | # | `L1`, `L2`, … counted from the top | a swatch of its material colour |
  | Name | the Layer's name | the dielectric's name |
  | Kind | its role: Signal, Plane or Mixed | Core or Prepreg |
  | Thickness | editable | editable |
  | Z0 | the [impedance readout](#impedance-readout) | empty |

- A selected dielectric shows its material, Dk and Df in a row under the table.
- **Actions under the table,** on the selected row:
  - **Add Layer** adds a copper Layer and the dielectric that goes with it. On a dielectric it splits that dielectric around the new Layer. With nothing selected it adds in the middle of the stack.
  - **Remove** removes an inner copper Layer and joins the dielectrics around it. The outer two Layers cannot be removed. What happens to the objects on a removed Layer is decided in [What happens when a board's Layer count changes?](https://github.com/openreda/reda/issues/334).
  - **Move up** and **Move down** move a copper Layer one place among the copper Layers.
- Reordering is done here and not by dragging in the cross-section: to scale, a copper band is a few pixels tall and is a poor drag target.

### Impedance readout

- Z0 is a column of the Layers table, so each figure sits on its own Layer's row. There is no separate impedance table.
- It is a readout only. It sets no rule and solves no width.
- It is shown for signal and mixed Layers that have a plane or mixed Layer to refer to, and is empty otherwise.
- The Track width it is worked out at is shown under the table.
- The formulas follow [Which impedance formulas suit a Stackup readout?](https://github.com/openreda/reda/issues/329).

## Via types table

- One row per Via type the board allows, with **Name**, **From**, **To**, **Drill** and **Pad**. From and To are drop-downs of the copper Layers.
- **Add Via type** and **Remove** sit under the table. The board always keeps at least one.
- What a Via type stores, how routing picks one and what DRC flags are decided in [How are Via types defined, picked and checked?](https://github.com/openreda/reda/issues/332).

## Board form

- Under the tables: the thickness ordered, then the copper, dielectric and total thickness as readouts, then the finish and the solder mask and silkscreen colours.
- Two badges:
  - the total against the thickness ordered, as a warning when they differ by more than the tolerance;
  - **Symmetric** or **Not symmetric**, comparing entries the same distance from the middle of the stack. The entries that differ are listed on hover.
- The finish and colours keep the Fab Preset's suggestion lists ([outputs](../v0.1/outputs.md#stackup-seeds-and-suggestions)).

## Cross-section

- **To scale by default,** with a **To scale · Schematic** switch over the drawing. Schematic sizes each band by its kind, for stacks whose copper is too thin to see.
- Copper, core and prepreg are drawn in their own material colours, with the solder mask over and under the stack in the board's mask colour. These are data colours and are the same in light and dark ([design language](../v0.1/design-language.md)).
- Every band is labelled beside the drawing with its name and thickness. Labels of thin bands are moved apart and joined to their band by a leader.
- The total thickness is dimensioned along the stack's side.
- **Via types** are drawn as barrels across the copper Layers they span, each with its name over the board and a pad on every copper Layer it meets. A micro Via is a cone, wide at the Layer the laser enters.
- **Direct edits,** both of which the tables also offer:
  - A selected Via type has a handle at each end. Dragging a handle to another copper Layer changes the span.
  - Pointing at a dielectric shows **+ Layer**, which splits it with a new copper Layer.
- There is no dragging of bands to reorder them and no dragging of edges to change a thickness.

## Board Setup

- The Stackup page of Board Setup no longer edits the Stackup. It keeps [Fab Notes](../v0.1/pcb.md#board), a summary of the Stackup, and a button that opens the Stackup editor tab.
- This replaces the v0.1 Stackup page ([app](../v0.1/app.md#settings-windows), [pcb](../v0.1/pcb.md#stackup)).

## Variants not taken

| Variant | Idea | Why not |
|---|---|---|
| A · Drawing | The cross-section is the editor, with no table and an inspector card | Numbers cannot be compared or tabbed through; thin bands are hard to hit; every gesture needs a keyboard equivalent |
| B · Ledger | One grid whose rows are the bands, with Via types as columns | The widest layout; it never looks like a board; to scale is only a side strip |
| C · Workbench | Tables and a read-only preview, with impedance in its own table | Became D, with the impedance moved onto its Layer and two edits moved into the drawing |

## Left to other tickets

- The Commands behind each edit, and the file-format additions.
- Starting a Stackup from a Fab Preset's named construction.
- Several dielectric sheets between two copper Layers.
- Where the readout's Track width comes from.
