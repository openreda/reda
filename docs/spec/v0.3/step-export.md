# v0.3 STEP export

Source: [What does a STEP export contain?](https://github.com/openreda/reda/issues/434), after the survey in [How do other EDA tools export a board as STEP, and what do MCAD tools expect?](https://github.com/openreda/reda/issues/425) ([findings](https://github.com/openreda/reda/blob/research/step-board-export/docs/research/step-board-export.md)).

This file says what a board's STEP export holds, how it is structured and named, where it is placed, and how it is offered. How STEP is read and written, with no CAD kernel, is in [STEP](step.md). What a Body is, and the Stand-in Body, is in [Bodies](bodies.md). Crates: `reda-step` (records and writer) and `reda-output` (the export).

## What is written

### The board

- Always written, with no switch to leave it out. It is one solid: the Board Outline extruded through the Stackup's total thickness, with Board Cutouts subtracted.
- Exact B-rep geometry, as [STEP](step.md#the-reda-step-crate) says: planes, and cylinders for arcs and holes.
- **Colours** come from the Stackup's typed colours. The top and bottom faces get the solder-mask colour, the edge and cut-out walls get the substrate colour, and plated hole walls get the finish colour.
- A missing Board Outline, or one that is open, stops the export with a message. Nothing is guessed.

### Holes

- Pad holes and slots that go through the board are always cut, plated or not.
- **Via holes:** through vias are cut only when "Via holes" is ticked. It is off by default.
- **Skip holes under:** an optional diameter, empty by default. Round holes below it are not cut. Slots are always cut.
- Blind and buried vias never reach a surface, so they are never cut.

### Components

- Every Component on the PCB Document, Mechanical Components included.
- **Not Fitted** Components are left out unless "Include Not Fitted" is ticked. A Variant filter waits for Variants (`variant` is reserved).
- **Selection only:** from File ▸ Export, a tick that is enabled when something is selected. An Output Job always exports the whole board.
- There is no filter by Footprint type or size.

### Bodies

- Every Body of each exported Component is written:
  - an **Extruded Body** as an exact prism, or a cylinder for a Circle outline, in its Material's colour;
  - a **Model Body** from the Model's parsed records, converted to millimetres and to AP214 ([STEP](step.md#the-reda-step-crate)), in the Model's own colours, or all in its Material when one is set.
- **Stand-ins:** "Export stand-ins", on by default, writes the [Stand-in Body](bodies.md#no-body-the-stand-in) for a Component that has no Body, or whose Models all fail to parse: its body box raised to `height` (1 mm when there is none), in neutral grey. With the tick off, such a Component contributes nothing.
- **Bottom side:** the Bodies of a Footprint on the bottom side are flipped by a 180° rotation about the board's Y axis, then lowered so they hang below the board. They are never mirrored, because a mirrored solid is inside out and an asymmetric part would be the wrong hand.

### What is not written

- Copper (tracks, pads, zones, vias, inner layers), solder mask as faces and silkscreen. The mask shows only as the board's colour.
- Mechanical, Assembly and Courtyard Layer graphics. [DXF export](dxf.md#export) carries those.
- Whatever the 3D view or the Layers panel happens to hide. The export writes what its own options say, so an Output Job gives the same file whatever the screen shows.

## File form

- **Protocol:** AP214 only, with no choice. Models read as AP203 or AP242 are converted ([STEP](step.md#formats)).
- **Units:** millimetres.
- **Compression:** a `.step` or `.stp` name writes plain text, and a `.stpz` name writes gzip.
- **Header:** the originating system is `REDA <version>`. Author and organisation are empty.
- **Names** that are not ASCII are written with Part 21's `\X2\` escapes, never dropped or replaced.

## Assembly structure and names

```
<board>                    top assembly, named after the PCB Document
├── <board>_PCB            the board solid
├── <Designator>           one sub-assembly per Component
│   ├── <model file name>  a Model Body, as a nested product
│   ├── <Designator> body 2  an Extruded Body
│   └── …
└── <Designator>
    └── <Designator> stand-in
```

- Extruded Bodies are numbered by their place in the Footprint's `bodies` list.
- **Name prefix:** optional text put in front of every product name, empty by default, so that two boards in one MCAD file do not collide.
- There is no "single part" option. Each Component stays separately selectable in the MCAD tool.

## Origin

- The same origin as [DXF](dxf.md#origin): the Output Origin if the user has placed one, otherwise design zero. There is no per-export choice, so a DXF and a STEP of the same board line up.
- **Z:** zero is the board's bottom face and +Z points up. The top face sits at the Stackup thickness, and top-side Components stand on it.

## Problems

- The export completes whatever the Models do. Each Model that is left out, or swapped for a Stand-in Body, gets one Messages entry naming the Designator and the Model's file name.
- A Command caller (CLI or MCP) gets the same list in the result.
- Only a missing or open Board Outline stops the export.

## Where it lives

One exporter in `reda-output`, reached two ways, with the same options in both:

- **File ▸ Export ▸ STEP…** opens a small dialog: Via holes, Skip holes under, Include Not Fitted, Selection only, Export stand-ins, Name prefix. The file name is typed in the save dialog.
- **An Output Job output kind,** off by default. It is written beside the zip, not inside it, because the zip is what the fab receives. It is named by the job's [Name Template](../v0.1/outputs.md#name-template), as `<stem>.step` beside `<stem>.zip`, and listed in the Output Manifest. The DXF output is named the same way, as `<stem>.dxf`.

**Command:** `pcb.export_step`. It reaches the menu, the Command Palette, the CLI and MCP by [ADR-0001](../../adr/0001-every-action-is-a-command.md). There is no preview Command. Arguments and the Output Job schema are settled in [What Commands and file-format additions does the v0.3 data need?](https://github.com/openreda/reda/issues/440).

## Out

- Copper, mask and silkscreen in STEP.
- A choice of protocol, AP242 output, and a single-part export.
- Variant filtering, until Variants exist.
- Rigid-flex folding, glTF, IDF/IDX and other formats (see the map's Out of scope).
- Checking the file in commercial MCAD tools is optional ([STEP](step.md#tests)). FreeCAD and BRL-CAD checks are decided in [Which tests does v0.3 add?](https://github.com/openreda/reda/issues/441).
