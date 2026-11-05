# v0.1 manufacturing outputs

Sources: [Which manufacturing outputs are in v0.1, and is panelization in v0.1?](https://github.com/openreda/reda/issues/14), [fab requirements research](https://github.com/openreda/reda/issues/4), [What Gerber and Excellon dialect does REDA write, and what proves what was sent?](https://github.com/openreda/reda/issues/129), [What do Output Jobs and Fab Presets control, including BOM and CPL?](https://github.com/openreda/reda/issues/130). Crate: `reda-output`. Schematic PDF is written by `reda-render`.

## Output set

- **Gerber X2:** see [Gerber](#gerber).
- **Excellon:** see [Excellon](#excellon).
- **CSV BOM** and **CSV CPL**.
- **[Fab Notes](#fab-notes)**, a [drill report](#drill-report) and a [Gerber job file](#gerber-job-file).
- **[Reports](#reports)** of DRC, ERC and the Change List.
- **[Schematic PDF](#schematic-pdf)**.
- **[Layout](#output-layout):** the fab's files go into one zip, with the assembly files, the [Output Manifest](#output-manifest) and the reports beside it.
- Deferred to v0.7: assembly/layer PDFs, IPC-D-356, ODB++/Gerber X3, drill maps. STEP is v0.3. No in-app Gerber viewer: the run report lists the files, and the user checks them in the fab's own viewer.

## Output layout

`<name>` is the job's resolved [name template](#name-template).

```
outputs/<job>/
  <name>.zip           Gerber, drill, Fab Notes, drill report, job file
  <name>-BOM.csv
  <name>-CPL.csv
  <name>-schematic.pdf
  manifest.json
  reports/
  fab/                 loose copies of what's in the zip
```

- BOM and CPL sit beside the zip because both fabs list them as uploads separate from the Gerber archive.
- The Fab Preset's `zip_assembly` also puts them inside the zip, instead of beside it. It is off in the shipped presets.
- Fab Notes, the drill report and the job file stay in the zip, because they are written for the fab.
- The schematic PDF is never in the zip, because it is for reading, not for the fab.
- `fab/` keeps the files readable in a git diff, which a zip is not, and keeps the files to upload easy to find.
- Files inside the zip and in `fab/` are named from `<project>`, so a date in the template changes the zip's name and not its bytes.

## File naming

- The Fab Preset's `naming` is `classic` or `descriptive`. The shipped presets use `classic`.
- **`classic`:** lowercase extensions, in the form a widely used open-source writer gives by default: `<project>.gtl`, `.gbl`, `.gts`, `.gbs`, `.gtp`, `.gbp`, `.gto`, `.gbo`, and `.gm1` for the Profile.
- **`descriptive`:** `<project>-<Layer>.gbr`, with spaces in the Layer name written as underscores.
- `file_names` overrides the name of any Layer's file.
- Drill files are `<project>-PTH.drl` and `<project>-NPTH.drl`, or `<project>.drl` with `merge_drill`.

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
  - Keepouts and Zone Cutouts are never plotted. [Shelved](pcb.md#shelving) Zones are left out.
  - The Fab Preset's `outline_on_every_plot` adds the Board Outline to every Gerber. It is off in the shipped presets.
- **Text:**
  - Stroke text is drawn as thick line segments.
  - An outline-font glyph is written as whole regions, with any holes joined by cut-ins. It uses no clear polarity, so it erases nothing underneath.
  - [Knockout text](pcb.md#text-style) is one flattened region, its box minus the glyphs, also with no clear polarity.
- **[Dimensions](pcb.md#dimensions)** plot as the lines and text of their Layer.
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
  | Copper text, graphics and Dimensions, including a filled circle | `NonConductor`, written as a draw or region, never a flash |
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
  - the run's warnings, each with its check and message
- Every run writes it. It has no toggle.

## Reports

- `reports/` holds DRC, ERC and Change List reports, in the formats [`checks.report`](checks.md) writes.
- They are never in the zip. They can change when the design has not, so the manifest lists them, and the zip's bytes don't depend on them.
- A job toggle controls them, on by default.

## Output Jobs

- An Output Job is a named recipe stored at Project level in the `.reda` file. It is not a Document tab.
- **The split:** a job holds what this build does, and a [Fab Preset](#fab-presets) holds what a fab expects, so several jobs share one set of conventions.
- Each job names:
  - one PCB Document
  - one Fab Preset
  - the outputs to make: Gerber, drill, BOM, CPL, Fab Notes, drill report, Gerber job file, reports and schematic PDF, all on by default
  - `schematic_pdf_colour`: `colour` (default) or `mono`
  - a destination and a [name template](#name-template)
  - `not_fitted_paste`: [keep or remove](#not-fitted) Not Fitted Pads' paste openings
- **Reserved names:** `checks` (ignore, warn or block per check), `overrides` (of preset keys) and `variant`. They are not in the v0.1 schema.
- **Running a job:**
  - One Command runs a job: `output.job.run`, from the GUI or headless through the CLI and MCP.
  - It is long-running, with progress and cancellation ([architecture](architecture.md#definition)).
  - Zones are refilled first.
  - Files are written in a fixed order by kind, which also orders the run report and the manifest. The order changes nothing in the files, so it can't be edited.
  - The run report has **Show in folder**.
- **Running every job:** `output.job.run_all {stop_on_block: true}` runs the Project's jobs in Projects panel order and gives a combined report. It is on the Output Jobs node's menu in the Projects panel.
- **Jobs in a Project:**
  - A new Project gets one job, named after the Fab Preset the user picks ([app](app.md#new-project)).
  - More jobs are allowed, for example "JLCPCB assembled" and "OSH Park bare".
- **Editing:**
  - Jobs are listed in the Projects panel, with Run, Edit, Duplicate, Delete and New job ([How are Projects, Documents and board setup created and edited in v0.1?](https://github.com/openreda/reda/issues/38)).
  - Edit opens a modal [Output Job dialog](#output-job-dialog) for one job. It uses OK/Cancel/Apply like the other Project dialogs ([app](app.md#settings-dialogs)).
  - It is a dialog, not a Document tab, in v0.1. It may become a tab in v0.7, when outputs grow.
- **Destination:**
  - The default folder is `<project>/outputs/<job name>/`, overwritten on each run. It sits outside the git-ignored `.reda/`, so the user decides whether to commit it.
  - The destination resolves the same tokens as the name template, so `outputs/${job}/${revision}` keeps old revisions.

### Name template

- `name_template` names the zip and the BOM and CPL stems. It defaults to `${project}-${job}`.
- **Tokens:** `${project}`, `${job}`, `${revision}`, `${date}` (ISO, from the `Clock`) and `${preset}`. `${variant}` is a reserved name.
- It uses the shared [text resolver](architecture.md#file-format).
- **Checked inline in the dialog:**
  - An unknown token, or any of `< > : " \ | ? *`, is an error, and so is a path separator in the name.
  - A result with non-ASCII characters only warns, because JLCPCB asks for English file names.
  - A live preview shows the resulting paths.

### Output Job dialog

```
Output Job ▸ <name>
  General    name · PCB Document · Fab Preset · destination · name template
             · live path preview · zip contents (from preset)
  Outputs    ☑ Gerber ☑ Drill ☑ BOM ☑ CPL ☑ Fab Notes ☑ Drill report ☑ Job file
             ☑ Reports ☑ Schematic PDF · PDF colour: colour / mono
             · Not Fitted paste: keep / remove
  Gerber     read-only table: Layer | File name | Plotted
  Drill      read-only: slot command · merged or split · file names
  BOM        read-only preview of this board's rows
  CPL        read-only preview of this board's rows
  Last run   status · warnings · files with size and hash · Show in folder
  [Run]
```

- Read-only values carry a "from preset" badge and **Edit preset copy…**, which opens that page of the [preset](#fab-preset-pages).
- The BOM and CPL previews use the writers' own rows, so a wrong header or an empty column shows before the upload.
- **Last run** reads `manifest.json` from the job's destination, so it also works after reopening the Project or on another machine with the committed outputs. With no manifest, it says "Not run yet".

## Determinism

- The same design and REDA version give an identical zip, so committed outputs change only when the design does. The schematic PDF is the exception: it can also change with [Preferences](app.md#preferences), such as the Unit Suffix.
- Gerber and drill files carry no date, and every zip entry has the fixed date 1980-01-01.
- The Output Manifest is the only output that holds the run's date, which it takes from the injected `Clock`. PDFs write no date.
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
  - a BOM row missing a [required field](#bom), naming its Designators

## Fab Presets

- **A Fab Preset holds:**
  - flat rule limits, and [preferred seeds](checks.md#fab-presets-and-the-rules)
  - seed mask expansion, Via tenting and paste expansion:

    | Preset | Mask expansion | Vias | Paste expansion |
    |---|---|---|---|
    | Generic | 0.05 mm | tented on both sides | 0 |
    | JLCPCB | 0 | tented on both sides | 0 |
    | PCBWay | 0.05 mm | tented on both sides | 0 |

  - Stackup seeds and suggestions ([below](#stackup-seeds-and-suggestions))
  - Gerber and drill switches: `x2_attributes`, `file_origin`, `slot_command`, `merge_drill`, `outline_on_every_plot`, `naming` and `file_names`
  - the [BOM](#bom) and [CPL](#cpl) keys
  - `zip_assembly` ([layout](#output-layout))
- **Shipped presets:** Generic, JLCPCB and PCBWay.
- **User presets:**
  - Users can duplicate and edit presets.
  - User presets are `.redafab` canonical-JSON files in the per-user config folder ([app](app.md#per-user-state)), in the same shape as the shipped presets and the Project copies.
- **Project copies:** the Project keeps its own copy of every preset its jobs use, so outputs come out the same on another machine. They are managed on the Fab Presets page of Project Settings, each with "Update from source" (after a preview) and "Save as user preset".
- **The board owns its rules** ([checks](checks.md#design-rules)). A job only checks the board's rules against its preset's limits.

### Fab Preset pages

Project copies, in Project Settings ▸ Fab Presets, and user presets, in Preferences ▸ Fab Presets, use the same pages. Each page has **Reset page to source**.

| Page | Holds |
|---|---|
| Rules | limits, preferred seeds, mask and paste seeds |
| Stackup | thickness and copper seeds, suggestion lists |
| Gerber | `naming`, `file_names`, `x2_attributes`, `file_origin`, `outline_on_every_plot` |
| Drill | `slot_command`, `merge_drill` |
| BOM | column grid, `layout`, `group_by`, `sort`, `line_numbers`, `designator_separator`, `not_fitted`, `required_fields`, `list_unplaced`, `mount_values` |
| CPL | column grid, `side_values`, `units`, `unit_suffix`, `bottom_rotation`, `bottom_position`, `exclude_through_hole` |
| Packaging | `zip_assembly` |

- `file_origin` also governs drill and CPL, so the Gerber page says so and the Drill and CPL pages show it read-only.
- Each row of a column grid is Source → Header, with up, down, add and remove.
- The CPL page shows `mount_values` read-only when the CPL has a mount column.

### Stackup seeds and suggestions

- A preset seeds a new board's `thickness` and copper. Generic, and any preset with empty seeds, use 1.6 mm and 1 oz.
- Finish and colours are not seeded, because a default finish in [Fab Notes](#fab-notes) could misstate the order.
- A preset holds suggestion lists for thickness, copper, finish, mask colour and silkscreen colour. They are never checked, and a value not listed gives no warning, because the lists go out of date.
- JLCPCB and PCBWay list their published 2-layer options, from the [fab research](https://github.com/openreda/reda/issues/142). JLCPCB's page lists no silkscreen colours, so that list is empty. Generic lists only the options both offer.

### Shipped BOM and CPL values

| Key | Generic | JLCPCB | PCBWay |
|---|---|---|---|
| BOM columns | `Designator`, `Quantity`, `Value`, `Footprint`, `Manufacturer`, `MPN` | `Comment`, `Designator`, `Footprint`, `LCSC Part #` | `Item #`, `Designator`, `Qty`, `Manufacturer`, `Mfg Part #`, `Description / Value`, `Package/Footprint`, `Type` |
| `required_fields` | none | `LCSC` | `MPN` |
| `line_numbers` | off | off | from 1, step 1 |
| `mount_values` | `SMT`, `THT` | `SMT`, `THT` | `SMD`, `THT` |
| `list_unplaced` | on | off | off |
| CPL columns | `Designator`, `Mid X`, `Mid Y`, `Layer`, `Rotation` | same as Generic | `Designator`, `Footprint`, `Mid X`, `Mid Y`, `Ref X`, `Ref Y`, `Pad X`, `Pad Y`, `TB`, `Rotation`, `Comment` |
| `unit_suffix` | off | off | on |
| `bottom_rotation` | `plus_180` | `negate` | `plus_180` |
| `exclude_through_hole` | off | off | on |

All three have `layout: grouped`, `not_fitted: omit`, `designator_separator: ","`, `side_values: Top, Bottom`, `units: mm` and `bottom_position: top_view`.

**Where they differ from the fabs' own documents:**

- **JLCPCB's part-number header:** its sample says `JLCPCB Part #（optional）`, while its export guide uses both spellings. REDA writes `LCSC Part #`.
- **JLCPCB's columns:** no Quantity or MPN. Its help page doesn't require them, though two of its blog pages do.
- **JLCPCB's `Layer`:** `Top`/`Bottom` from its help page, where its sample shows `T`/`B`.
- **PCBWay's headers:** its page and sample disagree, so REDA follows the sample, without the asterisks that mark required columns, and leaves out its notes column.
- **PCBWay's `TB`:** `Top`/`Bottom` from its page, where its sample shows `T`.
- **PCBWay's units:** mm with a suffix, because its pages give mm, mil and inches, and a bare number could be read in the wrong unit.
- **PCBWay's Not Fitted:** its sample marks such rows `DNS`. REDA leaves them out, because it also says the Designator count must match the quantity.

The [acceptance check](quality-and-release.md#testing) tests these.

## BOM

- **`layout`:** `grouped`, or `flat` with one row per Component.
- **`group_by`:** the fields a `grouped` row shares. The default is Part + chosen Footprint + value fields.
- **`sort`:** an ordered list of columns. The default is the first Designator, in natural order.
- **`line_numbers`:** on or off, with a start and a step.
- **Designators:** listed in natural order ("R1,R2,R10"), joined by `designator_separator`. Ranges are never written, because JLCPCB asks for comma lists and not ranges.
- **Columns:** the Fab Preset's list of Source → Header. The sources are line number, Designators, quantity, value, Footprint, mount, fitted and any Component field. A Mechanical Component with no Footprint leaves the Footprint column empty.
- **`mount_values`:** the spellings of the mount column. A Footprint is through-hole when any Pad is plated through-hole.
- **What is listed** ([How do Component Type, BOM inclusion and Net Ties work?](https://github.com/openreda/reda/issues/125)):
  - Every Footprint instance with In BOM true that isn't Not Fitted, whatever its Component Type.
  - Every Mechanical Component on a sheet with In BOM true, no Footprint choice, and not Not Fitted, so a screw needs no Footprint. `list_unplaced` off leaves these out, because a fab BOM that requires a package is likely to fail on a blank one.
  - Free Pads are never listed.
- **No special rows:** glue or a bare board is a Mechanical Component on a sheet with no Footprint.
- **`required_fields`:** a missing value [warns](#what-blocks-a-run-and-what-only-warns), and the row stays. Mechanical rows are skipped, so a screw doesn't warn about a missing supplier number.
- **`not_fitted`:** `omit`, or `list`, which writes Not Fitted parts on their own rows with quantity 0. The `fitted` column says "Fitted" or "Not Fitted".
- Apart from those Mechanical Components, the BOM and CPL read only the job's PCB Document. Each Footprint instance carries the Component's fields, Component Type, In BOM, In CPL and Not Fitted, kept current by Sync ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).

## CPL

- **Origin:** the Fab Preset's `file_origin`, the same as Gerber and drill ([Output Origin](#output-origin)).
- **Columns:** the Fab Preset's list of Source → Header. The sources are line number, Designator, value, Footprint, mount, side, rotation, any Component field, and three positions:
  - `centre_x/y`: the centre of the bounding box of the Footprint's copper Pads, leaving out non-plated holes. A Footprint with no copper Pads uses its origin.
  - `origin_x/y`: the Footprint's origin.
  - `pad1_x/y`: Pad 1.
- **`Mid X/Y`:** the shipped presets use `centre`, because both fabs ask for the part's centre.
- **`side_values`:** the spellings of top and bottom.
- **`units`:** `mm`, `mil` or `in`. `unit_suffix` writes the unit after each position.
- **Precision:** positions have 6 decimals and rotation 3.
- **Order:** natural Designator order.
- **Rotation:** the Footprint's orientation, using the IPC-7351 zero orientation.
- **Bottom side:** no fab documents a convention, so these are preset keys, and the [acceptance check](quality-and-release.md#testing) tests them.
  - `bottom_rotation`, from the stored rotation θ. For a part at 30° on top and flipped to the bottom:

    | Value | Formula | Writes | Used by |
    |---|---|---|---|
    | `stored` | θ | 330 | no shipped preset |
    | `plus_180` | θ + 180 | 150 | Generic and PCBWay: the form a widely used open-source writer gives |
    | `negate` | 360 − θ | 30 | JLCPCB: the form a widely used JLCPCB plugin for that writer gives |

  - `bottom_position`: `top_view`, `negate_x` or `negate_y`. The shipped presets use `top_view`.
- There is no per-package rotation-correction table in v0.1. The fab's own preview fixes those, and the table comes in v0.2.
- **What is listed:** every Footprint instance with In CPL true that isn't Not Fitted, whatever its Component Type. Free Pads are never listed.
- `exclude_through_hole` leaves out Footprints with a plated through-hole Pad. PCBWay's preset has it on, because its pages list SMT only.
- A machine-placed shield can or a fiducial the fab asks for is one setting on its Part or Component.

## Not Fitted

- A true/false flag on each Component.
- A Not Fitted Component stays on the board. It is left out of the CPL always, and out of the BOM unless the preset's [`not_fitted`](#bom) is `list`.
- **Paste:** the job's `not_fitted_paste` is `keep` or `remove`, default `keep`, so one stencil serves every build. Mask and copper never change.
- In BOM and In CPL are separate from it: they hold on every build, while Not Fitted says this build leaves the Component off.
- A Net Tie refuses Not Fitted, because its copper is on the board either way. Changing a Component's type to Net Tie clears the flag.
- **On the sheet:** the Project option `not_fitted_display` applies to the canvas and the PDF, as in the leading suite's Variant drawing options. `cross` (default) is a red cross, `mesh` a grey box with a mesh and `none` draws nothing extra. With `cross` and `mesh` the body is also drawn masked. A mono PDF draws both in black, without masking. `text` is a reserved name for a v0.5 text overlay. ([What schematic depth does v0.1 specify beyond the editing contracts?](https://github.com/openreda/reda/issues/133))
- Variants generalise this flag in v0.5.

## Schematic PDF

Source: [What schematic depth does v0.1 specify beyond the editing contracts?](https://github.com/openreda/reda/issues/133).

- A vector PDF of the sheets, in Document order, written from the Scene ([app](app.md#canvas-rendering)), one page per sheet at its paper size.
- Glyphs are written as filled outline paths. No fonts are embedded, so text in the PDF is not searchable.
- **Command:** `schematic.export_pdf {path, sheets: all | current, colour: colour | mono}`, behind a small Export PDF dialog. A PDF always uses paper colours, so `colour` replaces the `theme` parameter.
- **Output Job:** an output that writes every sheet beside the zip. The manifest lists it. CLI and MCP runs read the same [Preferences](app.md#preferences) as the GUI, with their defaults when none is set.
- **Colour modes:** colour and mono. The leading suite also has greyscale. REDA leaves it out because any printer can already print colour as grey.
- **Outline:** the Project title, then a bookmark per sheet ("2 Power"), then a bookmark per Component in natural sort ("R12 10k"), with the [Unit Suffix](schematic.md#units) where it applies.
- **Links:** each [cross-reference](schematic.md#port-cross-references) entry links to its page.
- **v0.2:** Net bookmarks, Component field pop-ups, hyperlinks and fit-to-paper.

## Panelization

Not in v0.1. It comes in v0.5 as the Panel Document ([modules](../modules.md#design-scale-v05)). The v0.1 commitments are that every Output Job names its PCB Document, and that the Gerber and drill writers take a list of board instances, each with a transform. v0.1 always passes one instance, whose translation is the [Output Origin](#output-origin).
