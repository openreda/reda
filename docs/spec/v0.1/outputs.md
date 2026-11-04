# v0.1 manufacturing outputs

Sources: [Which manufacturing outputs are in v0.1, and is panelization in v0.1?](https://github.com/openreda/reda/issues/14), [fab requirements research](https://github.com/openreda/reda/issues/4), [What Gerber and Excellon dialect does REDA write, and what proves what was sent?](https://github.com/openreda/reda/issues/129). Crate: `reda-output`. Schematic PDF is written by `reda-render`.

## Output set

- **Gerber X2:** see [Gerber](#gerber).
- **Excellon:** see [Excellon](#excellon).
- **CSV BOM** and **CSV CPL**.
- **[Fab Notes](#fab-notes)**, a [drill report](#drill-report) and a [Gerber job file](#gerber-job-file).
- Everything above goes into one zip. The [Output Manifest](#output-manifest) sits beside the zip.
- Deferred to v0.7: assembly/layer PDFs, IPC-D-356, ODB++/Gerber X3, drill maps. STEP is v0.3. No in-app Gerber viewer: the run report lists the files, and the user checks them in the fab's own viewer.

## Output Origin

- The Fab Preset's `file_origin` sets where coordinates are measured from, for Gerber, drill and CPL together:
  - `absolute`: design zero.
  - `relative`: the Output Origin if the user has placed one on the PCB Document, otherwise the lower-left corner of the Board Outline's bounding box.
- One setting covers all three because a fab's preview misplaces parts when they disagree.
- **Shipped presets:** Generic and JLCPCB are `absolute`. PCBWay is `relative`, because PCBWay asks for the lower-left corner of the board.
- `.SameCoordinates` is a UUID derived from the PCB Document's UUID and the resolved origin, so files written with a moved origin don't claim to align.

## Gerber

- **Format:** one fixed format with no options: mm, `%FSLAX46Y46*%`, absolute, leading zeros omitted, and X and Y written on every command.
- **Geometry the format can't carry:**
  - An arc shorter than 2 µm is written as a draw.
  - A near-full arc is split in two.
  - Contour points under 1 µm apart are merged.
  - A coordinate outside ±2147.483647 mm [blocks the run](#what-blocks-a-run-and-what-only-warns).
- **Files:**
  - Copper, Solder Mask and the Profile are always written. A missing mask file could read as "no mask", not "no openings".
  - Silkscreen and Paste are skipped when empty, and the run report says so.
  - The Profile holds the Board Outline and every Board Cutout as closed paths. Slots stay in the drill file.
  - Mask and Paste plot the [resolved openings](pcb.md#mask-and-paste).
  - The silkscreen is cleared over the solder-mask openings on its side, so no ink lands on bare copper.
  - The Fab Preset's `outline_on_every_plot` adds the Board Outline to every Gerber. It is off in the shipped presets.
- **Text:**
  - Stroke text is drawn as thick line segments.
  - An outline-font glyph is written as whole regions, with any holes joined by cut-ins. It uses no clear polarity, so it erases nothing underneath.
- **File attributes:** `.GenerationSoftware` (`openreda,REDA,<version>`), `.ProjectId`, `.SameCoordinates`, `.Part`, `.FileFunction` and `.FilePolarity`. `.CreationDate` is not written ([determinism](#determinism)). `.Part` is a writer parameter: `Single` in v0.1.
- **With `x2_attributes` off:** the same attributes are written as `G04 #@! …` comments, so the data set stays complete for readers that take them.
- **Aperture functions:**

  | Object | `.AperFunction` |
  |---|---|
  | Footprint SMD Pad | `SMDPad,CuDef`, or `SMDPad,SMDef` when the resolved opening is inside the copper on all sides |
  | Footprint plated Pad | `ComponentPad` |
  | Via | `ViaPad` |
  | Free Pad on copper | `OtherPad,FreePad` |
  | Track and Zone fill | `Conductor` |
  | Copper text and graphics, including a filled circle | `NonConductor`, written as a draw or region, never a flash |
  | Solder Mask, Paste and Silkscreen | `Material` |
  | Profile | `Profile` |

  - A Pad with non-uniform corners or chamfers is one flash of a generated macro.
  - `HeatsinkPad`, `BGAPad`, `FiducialPad` and the other Pad roles wait until the Pad model can express them.
- **Object attributes:**
  - `.N` is on every copper object. An object with no Net carries an empty `.N`.
  - `.P` is `<designator>,<number>`. The pin function stays out because the writers read only the PCB.
  - A Net Tie Pad keeps its own Net, so a fab's netlist check sees a short. Fab Notes list each tie as deliberate.

## Excellon

The commands keep the form a widely used open-source writer uses, which fabs are likely to see often. The [Layer spec's preferred Gerber drill format](https://github.com/openreda/reda/issues/141) and strict XNC stay out, because JLCPCB expects Excellon and strict XNC has not been tried with a fab.

- **Header:** `M48`, attribute comments, `FMAT,2`, `METRIC`, tools as `T1C0.300`, `%`, `G90`, `G05`.
- **Precision:** coordinates and tool sizes have 3 decimals in mm, rounded to 1 µm. A hole may sit up to 0.5 µm from its 6-decimal Gerber pad, far below what a fab can drill.
- **Holes:** a hole size is the [finished diameter](library.md#pad-model).
- **Slots:** routed by default (`G00`, `M15`, `G01`, `M16`, `G05`). The Fab Preset's `slot_command` switches to `g85`.
- **Non-plated slots** stay in the NPTH file in every preset, although JLCPCB's capabilities page asks for them in `.GKO` or `.GM1`. The [acceptance check](quality-and-release.md#testing) tests this.
- **Files:**
  - PTH and NPTH are separate, and each is skipped when it has no holes.
  - The Fab Preset's `merge_drill` writes one file with `.FileFunction,MixedPlating,1,<n>`. It is off in the shipped presets.
  - A plated and a non-plated hole never share a tool.
- **Attributes, as `; #@! ` comments, in printable ASCII with other characters escaped:**
  - the file attributes above, with `.FileFunction` carrying `Drill`, `Rout` or `Mixed` and `.FilePolarity,Positive`
  - tools with `.AperFunction`: `ViaDrill`, `ComponentDrill` or `MechanicalDrill`, where a Free Pad and a non-plated hole are `MechanicalDrill`. Plating is stated by `.FileFunction`.
  - `.DrillTolerance` on a tool whose holes have a `hole.tolerance`

## Fab Notes

- `<project>-fab-notes.txt`: UTF-8 plain text in a fixed order, for a person at the fab. It does not replace the fab's order form, and a fab may ignore it.
- **It lists:**
  - board size and layer count
  - thickness, copper per Layer, finish, and mask and silkscreen colours, left out when unset ([Stackup](pcb.md#stackup))
  - minimum features, from the board-wide rule records
  - holes with a `hole.tolerance`
  - each Net Tie and its Nets, as deliberate shorts
  - the files in the zip
  - the board's `fab_notes` free text ([board](pcb.md#board))

## Drill report

- `<project>-drill-report.txt`: for each drill file, every tool with its size, plating, hole count, slot count and tolerance, and the totals.
- Tool travel is left out: REDA doesn't optimise the drill path, and the fab sets its own.

## Gerber job file

- `<project>-job.gbrjob`, following the publisher's Job spec.
- **It holds:** the generation software, project ID, board size, layer count, thickness, the finish when it is one the Job spec names, and each file's path, function and polarity.
- `MaterialStackup` is left out, because when present it must be complete and v0.1 dielectrics have no material.
- No fab is known to read it. REDA writes it because the Job spec asks CAD tools to, and because a widely used open-source writer does by default.

## Output Manifest

- `manifest.json`, canonical JSON, beside the zip and not inside it: a zip can't hold its own hash, and fabs never need to parse the manifest.
- **It lists:**
  - each file: path, size, SHA-256 and kind
  - the zip's SHA-256
  - REDA version, job, Fab Preset and its hash, PCB Document and revision
  - the Clock date
- Every run writes it. It has no toggle.

## Output Jobs

- An Output Job is a named recipe stored at Project level in the `.reda` file. It is not a Document tab.
- Each job names:
  - one PCB Document
  - one Fab Preset
  - the outputs to make: Gerber, drill, BOM, CPL, Fab Notes, drill report and Gerber job file, all on by default
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

## Determinism

- The same design and REDA version give an identical zip, so committed outputs change only when the design does.
- Gerber and drill files carry no date, and every zip entry has the fixed date 1980-01-01.
- The Output Manifest is the only output that holds the run's date, which it takes from the injected `Clock`.
- PDF metadata takes its time only from the same `Clock`.
- The goldens inject the Clock and the REDA version, so they stay stable across releases ([testing](quality-and-release.md#testing)).

## What blocks a run and what only warns

- **Blocks:** the output files read only the job's PCB, so every blocking check reads only the PCB.
  - duplicate Designators
  - a Board Outline that is not closed
  - a Footprint on the PCB that the outputs cannot use, because the PCB does not hold its Footprint copy. The PCB keeps its own copy of every Footprint it places, so a committed Project never has one; the check stops the writers from leaving such a Footprint out silently.
  - a coordinate outside ±2147.483647 mm, which a Gerber file can't hold
- **Warns** (summarised in the run report):
  - the PCB is out of sync
  - a fitted Standard or Net Tie Component on a sheet with no Footprint assigned. It can never reach the PCB, so the warning says to assign a Footprint and Sync.
  - DRC errors and ERC errors
  - `rule_outside_preset` for this job's preset: the board's rules allow less than the preset's limits, for example "board allows 0.10 mm clearance, OSH Park requires 0.152 mm"
  - a text font that is missing on this machine

## Fab Presets

- **A Fab Preset holds:**
  - flat rule limits, and [preferred seeds](checks.md#fab-presets-and-the-rules)
  - seed mask expansion, Via tenting and paste expansion:

    | Preset | Mask expansion | Vias | Paste expansion |
    |---|---|---|---|
    | Generic | 0.05 mm | tented on both sides | 0 |
    | JLCPCB | 0 | tented on both sides | 0 |
    | PCBWay | 0.05 mm | tented on both sides | 0 |

  - Gerber and drill switches: `x2_attributes`, `file_origin`, `slot_command`, `merge_drill`, `outline_on_every_plot` and file naming
  - the BOM/CPL column mapping: Component field → column, for example `LCSC` → "LCSC Part #"
- **Shipped presets:** Generic, JLCPCB and PCBWay.
- **User presets:**
  - Users can duplicate and edit presets.
  - User presets are `.redafab` canonical-JSON files in the per-user config folder ([app](app.md#per-user-state)), in the same shape as the shipped presets and the Project copies.
- **Project copies:** the Project keeps its own copy of every preset its jobs use, so outputs come out the same on another machine. They are managed on the Fab Presets page of Project Settings, each with "Update from source" (after a preview) and "Save as user preset".
- **The board owns its rules** ([checks](checks.md#design-rules)). A job only checks the board's rules against its preset's limits.

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

- **Origin:** the Fab Preset's `file_origin`, the same as Gerber and drill ([Output Origin](#output-origin)).
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

Not in v0.1. It comes in v0.5 as the Panel Document ([modules](../modules.md#design-scale-v05)). The v0.1 commitments are that every Output Job names its PCB Document, and that the Gerber and drill writers take a list of board instances, each with a transform. v0.1 always passes one instance, whose translation is the [Output Origin](#output-origin).
