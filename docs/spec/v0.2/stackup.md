# v0.2 Stackup

Source: [What does the Stackup store for a multilayer board?](https://github.com/openreda/reda/issues/331). It builds on the v0.1 Stackup ([pcb](../v0.1/pcb.md#stackup)) and on the research into [stackup editors](https://github.com/openreda/reda/issues/320), [fab stackups](https://github.com/openreda/reda/issues/328) and [impedance formulas](https://github.com/openreda/reda/issues/329).

This file says what a multilayer Stackup stores, where it comes from and what is worked out from it. How the tab looks is in [stackup-editor](stackup-editor.md). Via types, what happens when the Layer count changes, plane behaviour, and the Command ids and schema are decided in their own tickets on the [v0.2 map](https://github.com/openreda/reda/issues/319).

## Layer count

- A board has from 2 to 32 copper Layers.
- Odd counts are allowed. An odd count is not an error and shows only through the [symmetry check](#checks).
- Single-sided boards stay out: Top and Bottom Copper are fixed Layers ([pcb](../v0.1/pcb.md#layers)).

## Stored values

The Stackup keeps its v0.1 shape: `thickness` (the finished thickness ordered), `entries` ordered top to bottom, and `finish`, `mask_color` and `silk_color`. v0.2 adds to the entries, to the copper Layer and one field to the Stackup.

### Dielectric

| Field | Meaning | Required |
|---|---|---|
| `uuid` | identity, as in v0.1 | yes |
| `kind` | Core or Prepreg | yes |
| `name` | shown in the Layers table and beside the cross-section | no |
| `thickness` | finished (pressed) thickness | no |
| `material` | free text, such as `7628` or `FR408HR` | no |
| `dk` | relative permittivity, in milli-units | no |
| `df` | loss tangent, in micro-units | no |
| `test_frequency` | the frequency `dk` and `df` were measured at, in Hz | no |

- These are the [reserved names](../v0.1/architecture.md#reserved-names).
- Glass style, resin content and thickness tolerances are not separate fields. `material` carries what the fab calls the sheet.
- There is no material library in v0.2. Materials arrive with a [Construction](#constructions) or are typed.
- A Dielectric's colour in the cross-section comes from its kind and is not stored.
- `dk` is used as typed. Nothing corrects it for frequency.

### Several sheets between two copper Layers

- Any number of Dielectric entries may sit between two copper Layers, each with its own row and band.
- The distance between the two copper Layers is the sum of their thicknesses.
- Where one Dk is needed for the gap, it is the thickness-weighted average.

### Blank thickness

- A Dielectric with no `thickness` takes a share of the remainder: `thickness` ordered, less all copper and all typed Dielectrics, divided equally among the blank ones.
- A negative remainder is an error on the field, as in v0.1.
- Symmetry and impedance use the shared value.
- Constructions fill every thickness, so blanks appear only on a Stackup built by hand.

### Copper Layer

- A copper entry stores the finished `thickness` only, entered in oz, µm, mm or mil as in v0.1. There is no separate foil weight or plating figure.
- The Layer stores `role`: signal, plane or mixed. The default is signal.
- **What `role` changes in the Stackup:**
  - which Layers the [impedance readout](#impedance-readout) refers to (plane and mixed);
  - the Gerber file function written for the Layer ([outputs](https://github.com/openreda/reda/issues/339));
  - the Kind column of the Layers table.
- Whether `role` restricts or warns about what is drawn on a Layer is decided in [How do Pads, Vias and planes behave on inner Layers?](https://github.com/openreda/reda/issues/335).

### Construction name

- The Stackup stores `construction`: the name of the [Construction](#constructions) it was started from, as plain text, or nothing.

## Constructions

A **Construction** is a fab's named, complete Stackup, carried by a Fab Preset ([glossary](../../../GLOSSARY.md)).

- **A Construction holds:** a name, the Layer count, `thickness`, and the full `entries` (copper thicknesses, and each Dielectric's kind, thickness, material, Dk, Df and test frequency). It may also hold a list of Via types.
- **Constructions replace the v0.1 Stackup seeds.** A preset no longer has separate thickness and copper seeds ([outputs](../v0.1/outputs.md#stackup-seeds-and-suggestions)). Each preset has a 2-Layer Construction that gives what its seed gave.
- A preset marks one default Construction for each Layer count it covers.
- The suggestion lists for finish and colours stay as in v0.1.

### Where they live

- Constructions are stored in the `.redafab` file, so user presets and Project copies carry them like every other preset value.
- The preset's Stackup page lists them: add, duplicate, remove and set as default.
- Shipped presets are read-only, as in v0.1. To change a shipped preset's Constructions, duplicate the preset.

### Starting a board

- New PCB Document asks for the Layer count, with 2 as the default.
- It offers the Fab Preset's Constructions for that count, with the preset's default selected.
- If the preset has none for that count, the board takes Generic's evenly spaced build and has no Construction name.

### In the Stackup editor

- **Apply Construction…** lists the Project's preset Constructions.
  - With the same Layer count, it replaces thicknesses and materials and keeps Layer names, roles, finish and colours.
  - With a different Layer count, it goes through the Layer-count change decided in [What happens when a board's Layer count changes?](https://github.com/openreda/reda/issues/334).
  - If the Construction carries Via types, a tick box offers to apply them too. It is off by default when the board already has Via types.
- **Save as Construction…** writes the board's Stackup into a user preset or a Project copy.

### When the name holds

- The name is checked, not cleared. It is shown, and printed in [Fab Notes](../v0.1/outputs.md#fab-notes), only while the Stackup equals the named Construction in the Project's preset copy.
- **What is compared:** the order and kind of the entries, every thickness, each Dielectric's material, Dk and Df, and the `thickness` ordered.
- **What is not compared:** Layer and Dielectric names, roles, finish and colours.
- When they differ, the editor shows "modified from *name*" with **Revert**, and Fab Notes leave the name out.
- If the Project has no Construction of that name, the name is treated as not holding.

### Shipped Constructions

Only builds the [fab research](https://github.com/openreda/reda/issues/328) verified Layer by Layer are shipped.

| Preset | Constructions |
|---|---|
| Generic | evenly spaced builds at 2, 4, 6 and 8 Layers, 1.6 mm, with no material data |
| JLCPCB | its 2-Layer build, and all 18 named 4-Layer and 18 named 6-Layer builds at 1.6 mm; `JLC04161H-7628` is the 4-Layer default |
| PCBWay | its 2-Layer build, and its numbered 4, 6 and 8-Layer builds at nominal pressed thickness |
| OSH Park | its 2-Layer build and its one fixed build each at 4 and 6 Layers |

- Shipped Constructions carry no Via types.
- JLCPCB's 8-Layer builds, and NextPCB's and Aisler's builds, were not verified and are not shipped.
- PCBWay varies pressed Prepreg thickness with inner copper coverage. The shipped figures are nominal, and the Construction's notes say so.

### The OSH Park preset

- OSH Park becomes the fourth shipped Fab Preset.
- It holds OSH Park's rule limits, its Constructions and its Gerber naming.
- OSH Park does no assembly, so its BOM and CPL keys are Generic's.
- Its 2-layer limits come from the [earlier fab research](https://github.com/openreda/reda/issues/142), which covers OSH Park, and its 4 and 6-Layer limits from the [multilayer fab research](https://github.com/openreda/reda/issues/328).

## Checks

Both checks are badges in the Stackup editor's Board form and lines in Fab Notes. Neither is a DRC violation and neither blocks an output.

- **Symmetry** compares entries the same distance from the middle of the stack.
  - Dielectrics are compared on kind, thickness and material.
  - Copper is compared on thickness. Role is ignored.
  - An odd copper count is never symmetric.
- **Thickness** warns when the sum of the entries differs from the `thickness` ordered by more than 10%.

## Impedance readout

The readout is a Z0 figure per copper Layer in the Layers table ([stackup-editor](stackup-editor.md#impedance-readout)). It sets no rule and solves no width.

### Reference Layers

- From the Layer, look towards the top and towards the bottom for the nearest Layer whose role is plane or mixed.
- Signal Layers on the way are skipped. The Dielectrics passed are summed.
- A mixed Layer both gets a figure and serves as a reference.

### Models

| Case | Model | Marked `≈` |
|---|---|---|
| Outer Layer with a reference inside the board | surface microstrip: Hammerstad and Jensen with the thickness correction | no |
| Inner Layer centred between two references in one material | stripline: Wheeler 1978 | no |
| Inner Layer off centre, or between different materials | the offset stripline fit, with the thickness-weighted Dk | yes |
| Inner Layer with a reference on one side only | embedded microstrip | yes |
| No reference on either side | none: Z0 is empty | |

- `≈` carries a tooltip saying the figure is rough and why.
- A Dielectric with no `dk` is taken as 4.5, and the figure is marked `≈`.
- Solder mask is not modelled and the Stackup gains no mask entries. The readout is labelled "without solder mask". Mask lowers a surface figure by about 2 Ω.

### Track width

- Each figure is worked out at the Default Net Class's Track width as it applies on that Layer ([How do design rules vary by Layer?](https://github.com/openreda/reda/issues/336)).
- The width used is shown under the table. There is no separate width input.

## Left to other tickets

- The Command ids and the `.redapcb` and `.redafab` schema additions, and whether they need a format version step.
- What a Via type stores, in [How are Via types defined, picked and checked?](https://github.com/openreda/reda/issues/332).
- The Layer-count change behind Apply Construction, in [What happens when a board's Layer count changes?](https://github.com/openreda/reda/issues/334).
- The Gerber file function per role and the job file's material stackup, in [What do multilayer outputs contain?](https://github.com/openreda/reda/issues/339).
