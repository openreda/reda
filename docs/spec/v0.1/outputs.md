# v0.1 manufacturing outputs

Sources: [Which manufacturing outputs are in v0.1, and is panelization in v0.1?](https://github.com/openreda/reda/issues/14), [fab requirements research](https://github.com/openreda/reda/issues/4). Crate: `reda-output`. Schematic PDF is written by `reda-render`.

## Output set

- **Gerber X2:**
  - mm, Protel file extensions, `.FileFunction` attributes
  - files for Top/Bottom Copper, Mask, Paste and Silkscreen, plus the Board Outline as the Profile
  - silkscreen text is plotted from the same text geometry as the canvas (`reda-text`)
  - Mask and Paste plot the [resolved openings](pcb.md#mask-and-paste)
  - the silkscreen is cleared over the solder-mask openings on its side, so no ink lands on bare copper
- **Excellon:** XNC-style, metric, with PTH and NPTH as separate files.
- **CSV BOM** and **CSV CPL**.
- Everything goes into one zip.
- Deferred to v0.7: assembly/layer PDFs, IPC-D-356, ODB++/Gerber X3, drill maps. STEP is v0.3. No in-app Gerber viewer: the run report lists the files, and the user checks them in the fab's own viewer.

## Output Jobs

- An Output Job is a named recipe stored at Project level in the `.reda` file. It is not a Document tab.
- Each job names:
  - one PCB Document
  - one Fab Preset
  - the outputs to make
  - a destination
- **Running a job:**
  - One Command runs a job: `output.job.run`, from the GUI or headless through the CLI and MCP.
  - It is long-running, with progress and cancellation ([architecture](architecture.md#definition)).
  - Zones are refilled first.
- **Jobs in a Project:**
  - A new Project gets one job, named after the Fab Preset the user picks ([app](app.md#new-project)).
  - More jobs are allowed, for example "JLCPCB assembled" and "OSH Park bare".
- **Editing:**
  - Jobs are listed in the Projects panel, with Run, Edit, Duplicate, Delete and New job ([How are Projects, Documents and board setup created and edited in v0.1?](https://github.com/openreda/reda/issues/38)).
  - Edit opens a modal **Output Job dialog** for one job: name, PCB Document, Fab Preset, outputs, destination and Run. It uses OK/Cancel/Apply like the other Project dialogs ([app](app.md#settings-dialogs)).
  - It is a dialog, not a Document tab, in v0.1. It may become a tab in v0.7, when outputs grow.
- **Destination:**
  - The default folder is `<project>/outputs/<job name>/`, overwritten on each run. It sits outside the git-ignored `.reda/`, so the user decides whether to commit it.
  - The zip is named `<project>-<job>.zip`.
- **Determinism:** output writers take time only from the injected `Clock`. Gerber X2 attributes and PDF metadata are byte-stable for the goldens ([testing](quality-and-release.md#testing)).

## What blocks a run and what only warns

- **Blocks:** the output files read only the job's PCB, so every blocking check reads only the PCB.
  - duplicate Designators
  - a Board Outline that is not closed
  - a Footprint on the PCB that the outputs cannot use, because the PCB does not hold its Footprint copy. The PCB keeps its own copy of every Footprint it places, so a committed Project never has one; the check stops the writers from leaving such a Footprint out silently.
- **Warns** (summarised in the run report):
  - the PCB is out of sync
  - a fitted Standard or Net Tie Component on a sheet with no Footprint assigned. It can never reach the PCB, so the warning says to assign a Footprint and Sync.
  - DRC errors and ERC errors
  - the board's rules allow less than the preset's minimums, for example "board allows 0.10 mm clearance, OSH Park requires 0.152 mm"
  - a text font that is missing on this machine

## Fab Presets

- **A Fab Preset holds:**
  - seed DRC rules
  - seed mask expansion, Via tenting and paste expansion:

    | Preset | Mask expansion | Vias | Paste expansion |
    |---|---|---|---|
    | Generic | 0.05 mm | tented on both sides | 0 |
    | JLCPCB | 0 | tented on both sides | 0 |
    | PCBWay | 0.05 mm | tented on both sides | 0 |

  - Gerber and drill switches: X2 attributes on or off, and file naming
  - the BOM/CPL column mapping: Component field → column, for example `LCSC` → "LCSC Part #"
- **Shipped presets:** Generic, JLCPCB and PCBWay.
- **User presets:**
  - Users can duplicate and edit presets.
  - User presets are `.redafab` canonical-JSON files in the per-user config folder ([app](app.md#per-user-state)), in the same shape as the shipped presets and the Project copies.
- **Project copies:** the Project keeps its own copy of every preset its jobs use, so outputs come out the same on another machine. They are managed on the Fab Presets page of Project Settings, each with "Update from source" (after a preview) and "Save as user preset".
- **The board owns its rules** ([checks](checks.md#design-rules)). A job only checks the board's rules against its preset's minimums.

## BOM

- One row per group of Part + chosen Footprint + value fields.
- Designators are listed in natural order ("R1, R2, R10").
- The columns come from the Fab Preset's mapping. The Generic and PCBWay presets have a Quantity column; the JLCPCB preset follows JLCPCB's BOM template (Comment, Designator, Footprint, LCSC Part #), which has none.
- **What is listed** ([How do Component Type, BOM inclusion and Net Ties work?](https://github.com/openreda/reda/issues/125)):
  - Every Footprint instance with In BOM true that isn't Not Fitted, whatever its Component Type.
  - Every Mechanical Component on a sheet with In BOM true, no Footprint choice, and not Not Fitted, so a screw needs no Footprint. Its Footprint column is empty.
  - Free Pads are never listed.
- Apart from those Mechanical Components, the BOM and CPL read only the job's PCB Document. Each Footprint instance carries the Component's fields, Component Type, In BOM, In CPL and Not Fitted, kept current by Sync ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).

## CPL

- **Origin:** the lower-left corner of the Board Outline's bounding box, unless the user has placed a Placement Origin on the PCB Document.
- **Rotation:** the Footprint's orientation, using the IPC-7351 zero orientation.
- There is no per-package rotation-correction table in v0.1. The fab's own preview fixes those, and the table comes in v0.2.
- **What is listed:** every Footprint instance with In CPL true that isn't Not Fitted, whatever its Component Type. Free Pads are never listed.
- A machine-placed shield can or a fiducial the fab asks for is one setting on its Part or Component.

## Not Fitted

- A true/false flag on each Component.
- A Not Fitted Component stays on the board, but is left out of the BOM and CPL.
- In BOM and In CPL are separate from it: they hold on every build, while Not Fitted says this build leaves the Component off.
- A Net Tie refuses Not Fitted, because its copper is on the board either way. Changing a Component's type to Net Tie clears the flag.
- Variants generalise this flag in v0.5.

## Schematic PDF

- A vector PDF of every sheet, in Document order, written from the Scene ([app](app.md#canvas-rendering)).
- Glyphs are written as filled outline paths. No fonts are embedded, so text in the PDF is not searchable.

## Panelization

Not in v0.1. It comes in v0.5 as the Panel Document ([modules](../modules.md#design-scale-v05)). The only v0.1 commitment is that every Output Job names its PCB Document.
