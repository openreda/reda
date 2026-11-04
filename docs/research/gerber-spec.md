# What the Gerber specification requires of REDA's writers

Research for [issue 141](https://github.com/openreda/reda/issues/141), under [map issue 123](https://github.com/openreda/reda/issues/123). This file reports facts only. Decisions belong to a later grilling ticket.

The publisher's documents are paraphrased, not quoted. Attribute names and syntax patterns are given exactly because they are the facts in question.

## Sources

All were read on 2026-11-04 from the publisher's [official downloads page](https://www.ucamco.com/en/gerber/downloads).

| Short name | Document | Published |
|---|---|---|
| **Layer spec** | [Gerber Layer Format Specification, revision 2026.05](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf) | 2026-05-19, the current revision |
| **XNC spec** | [XNC Format Specification, revision 2021.11](https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf) | 2021-12-06 |
| **Job spec** | [Gerber Job Format Specification, revision 2020.08](https://www.ucamco.com/files/downloads/file_en/435/gerber-job-format-specification-revision-2020-08_en.pdf) | 2020-08-27 |
| **Job schema** | [Gerber Job File JSON schema](https://www.ucamco.com/files/downloads/file_en/397/gerber-job-file-schema_en.json) | 2023-08-30 |
| **Job page** | [Gerber Job overview page](https://www.ucamco.com/en/gerber/gerber-job-file) | undated |

Revision 2026.05 records only corrections and text improvements (§11.1). The most recent writer-facing change was in 2023.03, which made the version field of `.GenerationSoftware` mandatory (§11.4). Older copies of the specification on third-party sites are out of date.

Code was read at `origin/main` commit `516cba3`. Paths are under `crates/reda-output/src/`. Besides `gerber.rs` and `drill.rs`, the comparison includes `plot.rs`, `plot/openings.rs` and `plot/artwork.rs`, because they choose the attribute values that `gerber.rs` writes. The goldens in `crates/reda-output/tests/golden/` confirm the written form.

## Answers

### Coordinate format

- The format must have exactly 6 decimals. The grammar is `coord_digits = /[1-6][6]/`, so `FSLAX16Y16` to `FSLAX66Y66` are the only valid forms (§3.5, §4.2.2).
- The integer digit count is free: use enough for the image size, and 3 is described as typical (§4.2.2).
- `4.6` is therefore valid but is not the value the publisher calls typical. The specification's examples use `2.6` and `3.6`. No section recommends `4.6` by name.
- Metric is the recommended unit. Imperial is described as historical and due for deprecation (§4.2.1).
- With mm and 6 decimals the resolution is 1 nm (§4.2.2).
- Coordinates must also fit a signed 32-bit integer (§3.4.1). At 6 decimals in mm, that limits them to ±2147.483647 mm, whatever the integer digit count.

### File attributes

`%TF<name>,<value>*%` is the syntax. All of these belong in the header. Values are comma-separated fields, and `,` must be escaped inside a field (§3.4.3, §3.4.4, §5.2).

In PCB fabrication data, `.FileFunction`, `.FilePolarity` and `.SameCoordinates` are mandatory (§6.2). The others are optional, and the rules below apply when they are present.

| Attribute | Syntax | Rules | Section |
|---|---|---|---|
| `.FileFunction`, copper | `Copper,L<p>,(Top\|Inr\|Bot)[,<type>]` | `p` counts from 1 at the top, consecutively. `<type>` is `Plane`, `Signal`, `Mixed` or `Hatched`. | §5.6.3 |
| `.FileFunction`, mask | `Soldermask,(Top\|Bot)[,<index>]` | `<index>` only when a side has more than one mask. | §5.6.3 |
| `.FileFunction`, silk | `Legend,(Top\|Bot)[,<index>]` | | §5.6.3 |
| `.FileFunction`, paste | `Paste,(Top\|Bot)` | | §5.6.3 |
| `.FileFunction`, profile | `Profile,(P\|NP)` | The label says whether the board edge is plated. | §5.6.3 |
| `.FileFunction`, drill | `Plated,i,j,(PTH\|Blind\|Buried)[,<label>]` and `NonPlated,i,j,(NPTH\|Blind\|Buried)[,<label>]` | `i,j` is the copper span. `<label>` is `Drill`, `Rout` or `Mixed`. | §5.6.3 |
| `.FilePolarity` | `Positive` or `Negative` | Positive is recommended. Solder mask is normally `Negative` because the image shows openings. Drill and rout files are always positive. | §5.6.4 |
| `.Part` | `Single`, `Array`, `FabricationPanel`, `Coupon` or `Other,<field>` | | §5.6.2 |
| `.GenerationSoftware` | `<vendor>,<application>,<version>` | All three fields are part of the syntax, and the version has been mandatory since 2023.03. | §5.6.7, §11.4 |
| `.CreationDate` | Full ISO 8601 with time zone, such as `2026-01-01T00:00:00+00:00` | | §5.6.6 |
| `.ProjectId` | `<Name>,<GUID>,<Revision>` | The GUID must be RFC 4122 version 1 or 4. | §5.6.8 |
| `.SameCoordinates` | `.SameCoordinates[,<ident>]` | Files with the same ident align. The ident is optional, can be chosen freely, and a GUID is recommended. It matters when files can be output at different times with a changed origin. | §5.6.5 |

### `.AperFunction` values

| Object | Value | Rules | Section |
|---|---|---|---|
| Through-hole component pad | `ComponentPad` | Follows the drill tool's value, on all layers. Only for through-hole components. | §5.6.10 |
| SMD pad | `SMDPad,(CuDef\|SMDef)` | The specifier is mandatory. Outer layers only. For the normal electrical pads of an SMD footprint. Thermal pads use `HeatsinkPad`, BGA pads use `BGAPad,(CuDef\|SMDef)`. | §5.6.10 |
| Via pad | `ViaPad` | Only for pads whose sole function is the layer connection. The specification calls via pads the bare minimum to identify. | §5.6.10 |
| Other pads | `ConnectorPad`, `TestPad`, `CastellatedPad`, `FiducialPad,(Local\|Global\|Panel)`, `ThermalReliefPad`, `WasherPad`, `AntiPad`, `OtherPad,<field>` | | §5.6.10 |
| Conductor | `Conductor` | Tracks and copper pours. Conductive pours must carry it. | §5.6.10 |
| Copper without electrical function | `NonConductor` | Text and similar copper, not drawing elements. | §5.6.10 |
| Profile | `Profile` | The draws and arcs that exactly define the outline. | §5.6.10 |
| Drill tools | `ViaDrill[,<IPC-4761>]`, `ComponentDrill[,PressFit]`, `MechanicalDrill[,(Tooling\|Breakout\|Other)]`, `CastellatedDrill`, `BackDrill`, `OtherDrill,<field>` | Plating is stated in `.FileFunction`, not here. | §5.6.10 |
| Mask openings | `Material` | Pad values are reserved for copper layers. | §5.6.10, p. 160 |
| Anything else | `Other,<field>` | The specification prefers this to leaving the attribute off. | §5.6.10 |

One function means one aperture: objects with different functions need different apertures, even at the same size, and the same applies to drill tools (§5.6.10).

The backdrill example in §6.6 says the via tool's value is `Via`, while the value table says `ViaDrill`. The table is the normative list and `Via` appears nowhere else.

### The Gerber job file

**Contents.** A `.gbrjob` file is UTF-8 JSON with lengths in mm and no image data (Job spec §2.1, §2.2). It has five sections:

- `Header`: generation software, creation date and comment.
- `GeneralSpecs`: project ID, owner, board size, layer count, thickness, finish, materials, IPC class and more.
- `MaterialStackup`: an ordered list of layers. If it is present it must be complete.
- `DesignRules`: clearances and minimum widths per layer class.
- `FilesAttributes`: path, file function, polarity and format of each file. Formats include `Gerber` and `XNC`.

**Required content.**

- Partial files are allowed, and missing values have no defaults (Job spec §2.1, §2.2).
- The schema requires no top-level section (Job schema).
- Job spec §3.1 nevertheless says CAD must output a minimal job file with board size and layer count, and it calls these the basis for pricing and automatic panelling.
- The same specification says legacy applications can ignore the job file (§2.1).
- The Layer spec does not mention job files at all, and its list of what fabrication data must contain (§6) does not include one.

**Actual use by fabs and CAM tools: not established from primary sources.**

- The publisher says it is intended for quoting, planning and CAM, says several software suppliers committed to it, and offers its own [job file editor](https://www.ucamco.com/en/software/precam-engineering/gerber-job-file-editor) ([Job page](https://www.ucamco.com/en/gerber/gerber-job-file)). It names no fab that consumes the file.
- No fab's own documentation that says it parses `.gbrjob` was found.
- A European fab's [Gerber X2 page](https://www.eurocircuits.com/?p=9972) describes using X2 attributes for stack-up placement and pad identification, and does not mention job files.
- A high-volume fab's [ordering instructions](https://jlcpcb.com/help/article/instructions-for-ordering) say production follows the Gerber files and other attachments are disregarded. They do not mention job files or X2 attributes.
- Third-party articles describe support as inconsistent. They are not primary sources and are not relied on here.

### Excellon or Gerber for drill data

The publisher says Gerber. The Layer spec's note in §6.6 advises Gerber rather than an NC format for drill and rout data, on the grounds that mixing formats risks registration problems and that fabricators load incoming files into CAM rather than sending them to a machine. If an NC format is needed, it says to use XNC, the only one it treats as clearly specified for CAD-to-CAM transfer. The XNC spec repeats the preference for Gerber (§2.1).

In a Gerber drill file (§6.6):

- Only circular apertures are allowed: flashes for holes, and a draw or arc, or a non-closed chain of them, for slots.
- PTH and NPTH go in separate files, and each drill span gets its own file.
- Files are positive, and the image shows the end (after-plating) diameter.

### Board Cutouts in the Profile

The Profile is a separate, mandatory file containing only the profile (§5.6.3, §6.5).

- It is either a region (G36/G37) or a cleanly connected closed path of D01 draws and arcs, under the same rules as contour segments (§4.10.3).
- With a non-zero aperture, the centre line is the board edge.
- Openings and cutouts are part of the profile (§6.5).
- Cutouts of general shape belong in the Profile file, and centre-line cutouts do not belong in a rout file (§6.6).
- Slots stay in the drill/rout file (§6.6).
- A rough outline or corner marks are not acceptable (§6.5).
- Contour rules forbid touching, overlapping or self-intersecting segments (§4.10.3).

## Differences

Each row compares the code with the source's own wording. The groups reflect how strongly the source words the rule, not how much the difference matters in practice.

### A. The source requires it, and the code differs

| # | Source says | Code does | Where |
|---|---|---|---|
| A1 | `.GenerationSoftware` has `<vendor>,<application>,<version>`, version mandatory (§5.6.7, §11.4). | Writes `openreda,REDA` with no version, in Gerber and drill files. | `plot.rs:112` |
| A2 | Drill tool values are `ViaDrill`, `ComponentDrill`, `MechanicalDrill` (Layer spec §5.6.10; XNC spec §4.2 example). | Writes `Plated,PTH,ViaDrill`, `Plated,PTH,ComponentDrill`, `Plated,PTH,MechanicalDrill` and `NonPlated,NPTH,MechanicalDrill`. These forms are in neither specification. | `drill.rs:61,70,75,92` |
| A3 | Mask openings cannot take pad values and take `Material` (§5.6.10, p. 160). | Mask flashes carry `SMDPad,CuDef` or `ComponentPad`. Non-plated openings carry nothing. | `plot/openings.rs:26`, `plot.rs:272-280` |
| A4 | Paste pads cannot take pad values; they take their function from the copper beneath (§5.6.10, p. 160). `Material` is the general value for the proper part of a data file (p. 158). | Paste flashes carry `SMDPad,CuDef`. | `plot/openings.rs:49` |
| A5 | All flashes on copper must be pads (§6.4). | A filled Footprint circle on a copper Layer is flashed with `NonConductor`. | `plot/artwork.rs:115` |
| A6 | A pad's value follows its drill tool's value, and `ComponentPad` is only for through-hole components (§5.6.10). | A plated Free Pad is `ComponentPad` on copper but `MechanicalDrill` in the drill file. | `plot.rs:275`, `drill.rs:75` |
| A7 | `SMDPad` is for pads of an SMD component's footprint (§5.6.10). | A surface-mount Free Pad, which has no footprint, also gets `SMDPad,CuDef`. | `plot.rs:274` |
| A8 | `CuDef` means copper-defined and `SMDef` means mask-defined (§5.6.10). | Always `CuDef`, including when a negative mask margin makes the opening smaller than the copper. | `plot.rs:274`, `plot/openings.rs:73-77` |
| A9 | XNC tool numbers in declarations have 2 digits, `01` to `99` (XNC spec §3.4). | Writes `T1C0.300` with no zero fill and no limit at 99. | `drill.rs:166` |
| A10 | XNC defines a closed command list (XNC spec §2.7, §2.10), and the example has neither of these. | Also writes `FMAT,2` and `G90`, which are not in the XNC spec. | `drill.rs:161,168` |
| A11 | XNC files allow printable ASCII plus CR and LF. Comments have no `;` and at most 255 characters (XNC spec §2.2, §3.1). XNC §4.1 also says comment attributes follow Gerber's character set and escapes, so the two sections pull apart on non-ASCII. | Project names and revisions are escaped for `%*,\` only, so raw non-ASCII, `;` and over-long lines can reach the comments. | `drill.rs:157`, `gerber.rs:132-144` |
| A12 | `.FileFunction`, `.FilePolarity` and `.SameCoordinates` are mandatory in fabrication data (§6.2). | With `x2_attributes` off, a Fab Preset writes none of them. The G04 comment form (§5.1.1) is not offered. The built-in presets have it on. | `gerber.rs:375`, `presets.rs:27-30` |
| A13 | `.FilePolarity` is mandatory in Gerber fabrication data, and drill files are positive (§5.6.4, §6.2). The XNC example has no polarity, so this applies only if XNC follows the Gerber set. | The drill writer deliberately skips `.FilePolarity`. | `drill.rs:156` |
| A14 | A fabrication data set must contain a Profile with a clean closed path (§6.5). | With no Board Outline, the writer emits a Profile file with no objects. The run path was not traced to see if something else stops this. | `plot.rs:283-305` |
| A15 | Integers must fit 32 bits, so ±2147.483647 mm at this resolution (§3.4.1). | Coordinates are `i64` nanometres with no range check. `FSLAX46Y46` declares up to 9999.999999 mm. | `gerber.rs:247` |
| A16 | Arcs shorter than about 2 µm must become draws, and near-full arcs must be cut in two (§4.7.2). Arcs with end points under 1 µm apart must not be written (§4.14.2). | Any non-zero sweep is one arc. The writer has no such check. Full circles are correctly split in two half turns. Whether earlier stages prevent these arcs was not checked. | `gerber.rs:316-337` |
| A17 | Non-coincident contour points and segments must be at least 1 µm apart, and objects under 1 µm should be avoided (§4.14.2). | The writer has no such check. Whether Zone fill output always satisfies it was not checked. | `gerber.rs:341-367` |

### B. The source recommends it, and the code differs

| # | Source says | Code does | Where |
|---|---|---|---|
| B1 | Standard extension is `.gbr`, and other extensions are listed as a bad practice (§3.6, §7.2). | Writes per-layer extensions (`.GTL`, `.GBL`, `.GTS`, `.GKO` and so on), the only variant `FileNaming` has. The [fab requirements research](https://github.com/openreda/reda/issues/4) chose these because they suit the surveyed fabs. | `plot.rs:50-54`, `reda-model/src/output.rs:15` |
| B2 | Standard XNC extension is `.xnc` (XNC spec §2.2). | Writes `-PTH.drl` and `-NPTH.drl`. | `drill.rs:29,34` |
| B3 | Use Gerber for drill data (§6.6). | Writes XNC-style NC, which §6.6 accepts as the fallback. | `drill.rs` |
| B4 | Put `.N` on all copper objects, using `%TO.N,*%` for no net and optionally `N/C` for single-pad nets, because absence is ambiguous (§5.6.13). | No `.N` on objects without a Net, on copper text or drawing, or on Zone clearing regions. `N/C` is never used. Drill objects never carry `.N`, though §5.6.13 allows it on plated holes. | `plot.rs:144-147,177,241` |
| B5 | Including the pin function in `.P` is strongly recommended (§5.6.14). | Writes `.P,<refdes>,<number>` only. | `plot.rs:224` |
| B6 | Use attributes as fully as possible, and prefer `Other,<field>` to omitting `.AperFunction` (§5.6.1, §5.6.10). | Silkscreen objects carry no `.AperFunction`. | `plot/artwork.rs:22`, `gerber.rs:15` |
| B7 | Include only the files needed (§5.6.3). The NPTH line in the §6.9 example is marked as conditional. | Both drill files, both mask files and both paste files are always written, even with nothing in them. The doc comment says this is deliberate for the drill files. | `drill.rs:18-20`, `plot.rs:69-82` |
| B8 | Keep single objects whole for robustness, and text is best as regions (§4.14.1, §5.7). | Outline-font text is written as separate touching triangles, so a glyph is many regions rather than one contour. | `plot/artwork.rs:66-79` |

### C. The source makes it optional, or the job format asks for it, and the code does not write it

- **Job file.** No `.gbrjob` is written, and nothing in `crates/` or `docs/spec/` mentions one. Job spec §3.1 says CAD must output a minimal one. The Layer spec does not ask for it.
- **Optional file attributes not written:** `.Part` (§5.6.2) and `.MD5` (§5.6.9).
- **Optional aperture and object attributes not written:** `.DrillTolerance` (§5.6.11), `.FlashText` (§5.6.12), `.C` (§5.6.15), `.Cxxx` (§5.6.16).
- **Optional fields not written:** copper `<type>`, drill `<label>`, `ViaDrill,<IPC-4761>`, `ComponentDrill,PressFit`, `MechanicalDrill,<kind>` (§5.6.3, §5.6.10).
- **Component layers not written:** `Component,L<p>,(Top|Bot)` (§5.6.3, §6.9). REDA writes a CSV CPL instead (`docs/spec/v0.1/outputs.md`).
- **Pad values the writer has no input for:** `PadKind` has only `Smd`, `PlatedThroughHole` and `NonPlated`, so `BGAPad`, `HeatsinkPad`, `ConnectorPad`, `TestPad`, `FiducialPad`, `CastellatedPad` and `WasherPad` are never written. A BGA or thermal pad modelled as an SMD Pad comes out as `SMDPad,CuDef`, while §5.6.10 gives those pads their own values.
- **Edge plating.** `Profile,NP` is fixed, so the writer cannot state `Profile,P` (`plot.rs:304`).

### D. Differences that appear only beyond v0.1

v0.1 is two-layer with through Vias only (`docs/spec/v0.1/pcb.md:76,123`). These are correct today but do not generalise.

- **Inner copper.** `gerber_files` plots only the first and last copper Layers. A multilayer board would have gaps in `L<p>` and no `Inr` files (`plot.rs:56-68`).
- **Drill spans.** All holes go in one file spanning `1` to the Layer count, using `PTH`. `via.span` is ignored, though §5.6.3 and §6.6 require a separate file per span with `Blind` or `Buried` (`drill.rs:23-37,57-64`).
- **Single-copper boards.** With one copper Layer, the same Layer is written twice, as `Copper,L1,Top` and `Copper,L1,Bot` (`plot.rs:58-68`).

### Observations

- `.SameCoordinates` uses the PCB Document's UUID as its ident, so it stays the same across output runs (`plot.rs:123`). §5.6.5 describes the ident as distinguishing coordinate systems between output times. The Placement Origin affects CPL output only, not Gerber coordinates (`assembly.rs:61,91`).
- `.ProjectId` writes `-` for an empty revision (`plot.rs:107-110`). The syntax does not forbid this and does not mention it.

## What already matches

- **Format:** `%FSLAX46Y46*%`, `%MOMM*%`, one FS and one MO in the header, and all apertures defined there with their macros ahead of them (§4.2, §4.3).
- **Commands:** `G75` before any arc, explicit D01/D02/D03 on every coordinate, `M02*` at the end, and none of the deprecated commands in chapter 8.
- **Macros:** primitive 21 and 1 parameters follow §4.5, and rotation about the macro origin is what the centred macros need (§4.5.1.5). Zero-size center lines from fully rounded corners are allowed (width and height ≥ 0).
- **Attributes:** `%TA`, `%TO` and `%TD` are placed correctly around apertures and regions, with none inside a region (§5.1, §5.3, §5.4).
- **Escapes:** `%`, `*`, `,` and `\` are escaped as `\uXXXX`, and non-ASCII is allowed in Gerber attribute values (§3.4.3).
- **File functions:** the copper, mask, legend, paste, profile and drill forms are valid, and the mask is `Negative` (§5.6.3, §5.6.4).
- **Other file attributes:** `.CreationDate` has a time zone. `.ProjectId` has an escaped name, a version-4 UUID and a revision. `.SameCoordinates` has a GUID ident.
- **Copper functions:** `ViaPad`, `Conductor` on Tracks and Zone regions, `NonConductor` on copper text, and `.P` plus `.N` on Footprint Pads (the CAD netlist of §6.8). Free Pads get no `.P`.
- **Separate apertures:** apertures are keyed by template and function, and drill tools by diameter and function, as §5.6.10 asks.
- **Pads and pours:** Pads are flashed, including inside pours (§6.4). Pours are regions, and their holes are clear-polarity regions, which §4.10.3 suggests in place of cut-ins.
- **Profile:** a separate file with `.AperFunction,Profile`, connected closed paths, and Board Cutouts as further closed paths. The 0.1 mm pen is allowed because the centre line is the edge (§6.5). Cutouts are not in the drill file (§6.6).
- **Drill:** PTH and NPTH in separate files, metric, decimal-point coordinates, attributes as `; #@! ` comments, `M48` first and `M30` last, and slots as `G00`, `M15`, `G01`, `M16`, then `G05` (XNC spec §3, §4).
- **Packaging:** everything in one zip, which is the only archive format §6.1 allows.

## Not verified

- **Fab and CAM use of `.gbrjob`.** No first-party confirmation or denial was found. Testing a real upload would answer this.
- **Validation by tool.** No output was run through the publisher's Reference Gerber Viewer or a parser generated from its [published grammar](https://www.ucamco.com/files/downloads/file_en/457/gerber-layer-format-peg-parsing-expression-grammar_en.ebnf). Conformance here comes from reading the code and goldens against the text.
- **Hole diameters.** Both specifications define the value as the finished diameter (§6.6, XNC §2.3). The writer passes `Via.hole` and the Pad hole through unchanged. REDA's docs were not found to say whether those values mean finished size.
- **Earlier-stage guards.** Items A14, A16 and A17 describe the writer alone. Commands, Zone filling or DRC may already prevent these cases.
- **Fab reaction.** This research covers what the publisher's documents say. It does not show whether any fab rejects a file for these differences.
