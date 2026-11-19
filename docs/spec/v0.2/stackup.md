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
- Shipped Constructions fill every thickness. Migrating a v0.1 preset preserves its automatically sized Core.

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

The ordered thickness is 1.6 mm for every shipped build. Shipped Constructions carry no Via types. Generic, JLCPCB and PCBWay use the released nominal dimensions for their `2 Layer` builds. Subtracting the two 35 µm copper faces from 1.6 mm gives a calculated Core thickness of 1.53 mm. Generic uses 35 µm on every Layer of its 4-, 6- and 8-Layer builds. Its equal Core gaps are rounded to whole nanometres in mirror pairs. A board started at a count the chosen fab does not cover gets the same Generic spacing without a Construction name.

The following catalogues record the verified entries top to bottom, in the units used by the file format. The source tables and order codes are recorded in the [multilayer research](https://github.com/openreda/reda/blob/b7afb0c5f10cfbcb1fff03ab783a9725538b78dd/docs/research/fab-multilayer.md).

- **JLCPCB.** REDA selects `JLC04161H-7628` at four Layers and the first named build, `JLC06161H-3313`, at six. The latter uses a 0.1088 mm centre prepreg. The catalogue's separate “No requirement” service specifies 0.1164 mm. Outer copper is 35 µm and inner copper is 15.2 µm after processing. The relative permittivities are 4.6 for the core and 4.4, 4.1, 3.91 and 4.16 for styles 7628, 3313/2313, 1080 and 2116 respectively. The 1080 ply of `JLC06161H-3313C` is 76.4 µm above and 78.4 µm below, so this build shows a symmetry mismatch. [All 36 ordered builds](../../../crates/reda-output/data/constructions/jlcpcb.json) · [JLCPCB catalogue](https://jlcpcb.com/impedance)
- **PCBWay.** The 1.6 mm catalogue numbers are 1–7 and 44–57 at four Layers, 1–4 and 24–33 at six, and 1–4 and 19–26 at eight. REDA selects regular build #1 at each count. Each Construction's notes record its assumed inner copper coverage and the fab's finished estimate. A multi-ply gap has one published combined pressed thickness. Its entry name records the plies in physical order and its material identifies the same layup on either face. The core Dk is 4.6. Prepreg Dk is 4.74 for 7628, 4.45 for 2116 and 3313, and 4.21 for 1080. A group whose plies have different Dk has a blank effective Dk because the individual pressed thicknesses are unknown. The symmetry check compares these combined gaps. Six-Layer #31 and #33 each have two different plies in the centre gap, so their individual ply order is asymmetric even though the stored gap matches itself. [All 47 numbered builds](../../../crates/reda-output/data/constructions/pcbway.json) · [PCBWay lamination tables](https://www.pcbway.com/multi-layer-laminated-structure.html)
- **OSH Park.** REDA selects the standard Prototype build for each supported count. At two Layers the core is 1.524 mm FR-4 with Dk 4.5 at 10 MHz between 35.6 µm copper faces. Four and six Layers have 43.2 µm outer and 17.5 µm inner copper. The four-Layer build has two 199.9 µm 2113 prepregs, Dk 3.61 at 1 GHz, around a 990.6 µm core. The six-Layer build has mirrored 110.7 µm 106 prepregs (Dk 3.23, Df 0.011) and 101.6 µm 2113 cores (Dk 3.66, Df 0.009), measured at 1 GHz, around a 925.6 µm core. The central FR408HR cores have no published Dk or Df. [All three builds](../../../crates/reda-output/data/constructions/osh-park.json) · [Two](https://docs.oshpark.com/services/two-layer/), [four](https://docs.oshpark.com/services/four-layer/) and [six-Layer specifications](https://docs.oshpark.com/services/six-layer/)

JLCPCB eight-Layer builds and the NextPCB and Aisler catalogues await verified dimensions for every entry.

### The OSH Park preset

The fourth shipped preset uses one flat set of rules across its three Prototype services. The strictest published minimum for each dimension applies on every Layer. Each Construction's notes also give the limits of its own service. [OSH Park design rule setup](https://docs.oshpark.com/design-tools/kicad/kicad-design-rules/)

| Dimension (mm) | Two Layers | Four Layers | Six Layers | Preset |
|---|---:|---:|---:|---:|
| Track width and copper clearance | 0.1524 | 0.127 | 0.127 | 0.1524 |
| Annular ring | 0.127 | 0.1016 | 0.1016 | 0.127 |
| Via diameter | 0.508 | 0.4572 | 0.4064 | 0.508 |
| Minimum drill | 0.254 | 0.254 | 0.2032 | 0.254 |
| Hole to hole | 0.127 | 0.127 | 0.127 | 0.127 |
| Copper to board edge | 0.381 | 0.381 | 0.381 | 0.381 |
| Plated hole to unrelated inner copper | — | 0.254 | 0.254 | 0.254 |

The default hole range ends at 6.35 mm, above which OSH Park switches to milling with wider tolerances. REDA starts routing at 0.25 mm with a 0.60/0.30 mm Via. Mask expansion is 0.0508 mm on both sides with tented Vias. Finish and colour suggestions are ENIG, purple mask and white silk. [OSH Park fabrication specifications](https://docs.oshpark.com/services/two-layer/) · [Mask and drill setup](https://docs.oshpark.com/design-tools/kicad/kicad-design-rules/)

The preset writes Gerber X2 with classic filenames (`.gtl`, `.g2l` through the last inner Layer, `.gbl`) and separate plated and non-plated drill files with G85 canned slots and absolute coordinates. OSH Park offers bare boards, so its BOM and CPL settings follow Generic and its fabrication archive excludes assembly files. [OSH Park Gerber guide](https://docs.oshpark.com/design-tools/kicad/generating-kicad-gerbers/)

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
