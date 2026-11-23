# STEP board export: what EDA tools write and what MCAD tools expect

Research for [#425](https://github.com/openreda/reda/issues/425). All sources were read on 2026-11-23. The note records what each tool does and where the tools differ. It recommends nothing for REDA.

## How far the sources reach

| Tool | Source read | Coverage |
| --- | --- | --- |
| KiCad | Official docs (AsciiDoc source, `master`) and exporter source (`master`) | Full |
| Horizon EDA | Exporter source (`master`); the docs page for STEP export returns 404 | Full, from source only |
| Altium Designer | Official documentation page and one first-party knowledge-base article | Full for the dialog; the schema statement is one sentence |
| EasyEDA Pro | Official user guide page | Full for the dialog; nothing on schema, naming or origin |
| Fusion Electronics / Eagle | Only Autodesk's generic help pages could be read; the forum threads and blog returned HTTP 403 | Not verified, see below |
| OrCAD X | No first-party page with the export options could be read | Not verified, see below |
| DipTrace | Product page only; no help page for the export dialog was found | Mostly not verified |
| FreeCAD | Import and export dialog source (the wiki sits behind a bot check) | Options only |
| SolidWorks | Official web help, 2025 | Options and supported protocols |
| Onshape | Official help | Options, limits and supported protocols |
| Fusion as a STEP importer | Not read | Not verified |

Nothing here was checked by exporting a file and opening it. Every statement about file contents comes from documentation or source code.

## KiCad

Sources: the PCB editor manual, [`pcbnew_generating_outputs.adoc`](https://gitlab.com/kicad/services/kicad-doc/-/raw/master/src/pcbnew/pcbnew_generating_outputs.adoc) (section "3D models"); the CLI manual, [`cli.adoc`](https://gitlab.com/kicad/services/kicad-doc/-/raw/master/src/cli/cli.adoc) (section "PCB export: STEP"); the exporter, [`exporter_step.cpp`](https://gitlab.com/kicad/code/kicad/-/raw/master/pcbnew/exporters/step/exporter_step.cpp) and [`step_pcb_model.cpp`](https://gitlab.com/kicad/code/kicad/-/raw/master/pcbnew/exporters/step/step_pcb_model.cpp). The exporter is built on Open CASCADE (OCCT).

**One exporter, many formats.** The same dialog and the same options write STEP, STPZ (gzip-compressed STEP), GLB, BREP, XAO, PLY, STL, U3D and PDF. VRML and IDF use separate exporters (manual).

**Board solid.** "Export board body" models the non-copper board body and can be switched off (`--no-board-body`). `--board-only` drops all Component models (manual, CLI).

**Holes.** Pad holes are cut by the exporter (`AddHole` calls in `exporter_step.cpp`). Via holes are cut in the body only when conductor layers are exported, unless "Cut vias in board body" is on. "Fill all vias" leaves via holes uncut in the conductor layers (manual). The source also models backdrills and countersink or counterbore cuts. No option filters holes by diameter in the documentation read.

**Copper.** Off by default and split into separate switches: tracks and vias on outer layers, pads, zones, and inner conductor layers. The CLI marks tracks, pads and zones as "time consuming". Pads get 0.005 mm of extra thickness so that they stay separate faces from the surrounding metal; only the CLI can turn that off (`--no-extra-pad-thickness`). "Fuse shapes" merges intersecting geometry into one shape and is also marked time consuming. A net filter with wildcards limits copper to matching Nets (manual, CLI).

**Mask and silk.** Each is optional and written "as a set of flat faces; it is not three-dimensional" (manual).

**Component models.** Only STEP (and IGES, STPZ) models can go into the export. A Footprint that references a VRML model contributes nothing unless "Substitute similarly named models" is on, in which case a STEP file with the same base name is used. The manual says this option "must be enabled" to get models for Footprints from KiCad's own library, because that library references the VRML versions. Models may be embedded in the Footprint or board file (`exporter_step.cpp` resolves embedded files).

**Missing models.** A model file that cannot be found is reported as a warning ("Could not add 3D model for %s. File not found") and the export continues without it (`exporter_step.cpp`).

**Stand-in bodies.** The `master` source has an extruded body per Footprint: when a Footprint has a body with extrusion shown, the exporter builds a solid from an outline with a standoff and a height, adds metallic pin extrusions for through-hole pads, and names it `<refdes> (extruded)` (`exporter_step.cpp`, `step_pcb_model.cpp`). The manual sections read do not describe this, so which release carries it was not established.

**Filters.** Skip "Do not populate" Components; skip Components whose Footprint type is "Unspecified"; export all Components, only the selected ones, or those whose reference matches a filter such as `C1,R*`; choose a design variant. There is no filter by Component size in the documentation read.

**Colours.** Written, with OCCT colour mode on. Defaults are hard-coded in the source (board body 0.42/0.45/0.29, white silk, dark green mask, grey pins) and are replaced by the board stackup's colours when those are set. When the mask is not exported as a layer, the board body is painted in the mask colour. Visual materials named `soldermask`, `silkscreen`, `copper`, `pad` and `board` carry metalness and roughness. Component models keep the colours in their STEP files.

**Assembly structure and naming.** The file is an assembly. Each Component model is a located sub-assembly labelled with its reference designator. Board parts are separate assembly components named `<board name>_<kind>`, with kind `PCB`, `copper`, `pad`, `via`, `silkscreen` or `soldermask`, a Net name appended for copper, and a counter when there are several. The STEP product name is the output file's base name. Header fields are fixed: author "Pcbnew", organisation "Kicad", originating system "KiCad to STEP converter". Names are forced to ASCII in the product name and header (source comments).

**Origin.** Grid origin, drill origin, or a user-defined point (manual, CLI). Which origin applies when none is chosen was not checked.

**Application protocol.** The exporter never sets OCCT's `write.step.schema`, so it writes OCCT's default. OCCT's source sets that default to `AP214IS` ([`STEPControl_Controller.cxx`](https://raw.githubusercontent.com/Open-Cascade-SAS/OCCT/master/src/DataExchange/TKDESTEP/STEPControl/STEPControl_Controller.cxx)), while the [OCCT STEP user guide](https://occt3d.com/dev/doc/overview/html/occt_user_guides__step.html) lists `AP214CD` as the default. The two disagree; the code is the later authority. KiCad offers no choice of protocol.

**Size and speed controls.** "Don't write P-curves to STEP file" (GUI) is on the same switch as `--no-optimize-step` (CLI). The source comment says that not writing parametric curves "reduces file size and write/read times" but "might be less compatible in some cases". The CLI manual's sentence for `--no-optimize-step` reads as if writing the curves were what reduces size, which contradicts the GUI manual and the source. A "Board outline chaining tolerance" (default 0.01 mm) decides when two outline points count as the same; the manual says to raise it when the outline does not close. An invalid outline stops the export ("No valid PCB assembly").

## Horizon EDA

Sources: [`export_step.cpp`](https://raw.githubusercontent.com/horizon-eda/horizon/master/src/export_step/export_step.cpp), [`step_export_settings.hpp`](https://raw.githubusercontent.com/horizon-eda/horizon/master/src/board/step_export_settings.hpp), and the [feature overview](https://docs.horizon-eda.org/en/latest/feature-overview.html). The dedicated docs page (`export-step.html`) returns 404. The exporter is also built on OCCT and its model-handling code is close to KiCad's.

- **Options: four.** File name, a name prefix, "include 3D models" (default on), and a minimum hole diameter.
- **Board solid.** The Board Outline polygon is extruded by the summed substrate thickness of the stackup. Outline cut-outs are subtracted.
- **Holes.** Only through holes are cut ("we only care about through holes"). Round holes below the minimum diameter are skipped; slots are always cut.
- **Copper, mask, silk.** Not exported.
- **Component models.** STEP models from the pool, placed per package. Components marked "nopopulate" are skipped. A package with no model is skipped silently; there is no stand-in body. A model that fails to load logs an error and the export continues.
- **Colours.** The board is painted in the solder-mask colour. Per-face colours of the Component models are copied over.
- **Structure and naming.** One assembly named `<prefix>PCA`, the board named `<prefix>PCB`, each Component named `<prefix><refdes>`. The prefix exists to keep names apart when several boards meet in one MCAD file. Header fields are placeholders ("An Author", "A Company", name "Board").
- **Origin.** Board coordinates, with no option.
- **Application protocol.** Not set, so OCCT's default applies, as for KiCad.

## Altium Designer

Sources: [Mechanical Data Import-Export Support](https://www.altium.com/documentation/altium-designer/design-tools-interfacing/mechanical-data-import-export) (the old Export Options dialog URL redirects here) and the knowledge-base article [STEP Export Does Not Include Copper or Overlay](https://www.altium.com/documentation/knowledge-base/altium-designer/step-export-does-not-include-copper-or-overlay) (updated 2026-05-20).

**Board solid.** "The board is always exported", and "Board cutouts are always included in the export". A rigid-flex board can be exported folded, with a slider from 0% to 100% fold.

**Holes.** From File » Export: "Export All" or "Export Selected", with the note that all holes is "slower and increases the size of the exported file". From an Output Job: three checkboxes, for pad holes of mechanical Components, of electrical Components, and free pads. No filter by diameter.

**Copper, mask, silk.** Not in STEP, by design. The knowledge-base article says STEP export "is limited to exporting the board outline and thickness, component 3D bodies, and optionally pad holes", and that this "is not controlled by export options or license configuration". Copper options (without copper, copper only, selected only, by layer) exist only for Parasolid export, which "produces true 3D copper but can result in very large files". A legacy Altium Designer 17 exporter could embed copper and silkscreen in STEP; Altium calls it deprecated and says it "may generate extremely large files that are difficult to open in MCAD tools".

**Component models and stand-in bodies.** A Component can carry both simple bodies (extruded, cylindrical or spherical 3D Body objects) and a generic model (STEP and similar). The export chooses: "Prefer simple bodies", "Prefer generic 3D models", or "Export both". "If only extruded 3D bodies are available for components, they will always be exported." Free 3D bodies placed on the board (an enclosure, say) are exported unless "Skip Free 3D Bodies" is on; hidden bodies can be skipped too.

**Filters.** "Export All" or "Export Selected" Components; exporting with nothing selected gives the bare board. No fitted or not-fitted switch and no size filter appear on this page. Whether an Output Job applies a variant to the STEP output was not checked.

**Assembly structure and naming.** An assembly with one model per Component by default; "Export As Single Part" writes one part instead, which "prevents individual components from being selected in the receiving MCAD application". "In the STEP file, each component is identified by its designator." A Component Suffix (none, board file name, or custom text) exists because importing several boards into one MCAD file is "likely" to give designator clashes.

**Origin.** The model is positioned "relative to the origin point in the source PCB design", with no option on this page. Altium notes the importing tool "may or may not respond to the included location instruction".

**Application protocol.** One sentence: "both the STEP AP214 and the legacy AP203 formats are supported by Altium Designer – exported files are ISO-10303-21-compliant (AP214)." For STEP coming the other way, Altium advises "Use the AP214 format whenever possible." AP242 is not mentioned.

**Colours.** The page does not say what colours are written.

## EasyEDA Pro

Source: [Export 3D file](https://prodocs.easyeda.com/en/pcb/export-3d-file/), EasyEDA Pro user guide.

- **Formats.** STEP and OBJ only.
- **Objects, each a checkbox.** PCB (the substrate, including slots), Component model, Via (metal of holes and multi-layer pads), Silk Screen (text and graphics, top and bottom), and Signal layer circuits (copper on the top and bottom layers). Solder mask is not listed, and inner copper is not listed.
- **Structure.** "Assembly: … PCB and 3D models are separated" or "Part: … PCB and 3D models are a whole".
- **Component models.** Only models bound in STEP format are exported: "Otherwise, the exported STEP will not have a model."
- **Stand-in bodies.** An option generates a box for a Component without a bound model, from its rectangular extent and its "Height" property.
- **Missing models.** Before writing, a dialog lists Components whose model failed, by cause: no model bound, bound model not in STEP format, model id not found, or a network failure while loading. The user can reload and then continue.
- **Not stated.** Application protocol, colours, naming, origin, and any fitted or size filter.

## Fusion Electronics / Eagle

Not verified against a first-party source. Autodesk's forum threads and blog returned HTTP 403, and no Fusion help page describing STEP export of a 3D PCB was found.

What could be read:

- Fusion's generic [Export designs](https://help.autodesk.com/cloudhelp/ENU/Fusion-Designs/files/ASM-EXPORT-DESIGN.htm) page lists STEP among the export formats with no options described. Electronics has no STEP dialog of its own in the pages read: the board becomes a 3D PCB design, and that design is exported like any other Fusion design.
- A 3D PCB can also be made in the mechanical workspace from a sketch or a solid face, with an origin and axis choice and conversion of sketch circles up to a set diameter into non-plated holes ([Create an Independent PCB](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-3DPCB-CREATE-TSK.htm)).

What search-result excerpts of Autodesk forum threads say, unconfirmed because the threads themselves could not be opened: the STEP holds the board and the Component bodies; silkscreen is held as canvases or decals and is dropped by STEP; pads, traces and vias are reported missing; Components without a 3D package appear as boxes. Thread titles: "STEP export does not export copper or silkscreen" and "Eagle 9.6.2 → Fusion 360: STEP Export Missing Pads and Copper", both on forums.autodesk.com.

## OrCAD X

Not verified. No first-party page listing the STEP export options could be read; the Cadence resource pages found carry only a summary line, and the Cadence community forum returned HTTP 403. Search-result excerpts, unconfirmed, mention a File > 3D Export panel in OrCAD X Presto with "Selected objects only", output units and a file format choice, and a forum claim that Allegro 24.x changed its STEP engine from ST-Developer to Spatial InterOp with slower saves and a different hierarchy. The application protocols offered, and whether copper, mask and silk can be included, are unknown.

## DipTrace

Source: [3D PCB Preview & STEP Export](https://diptrace.com/diptrace-software/3d-modeling/). The page says the board "can be exported to STEP or VRML 2.0", that models in `*.wrl`, `*.step`, `*.iges` and `*.3ds` can be attached to patterns, and that models can be generated from an IPC code or a Component outline. The export dialog's options, the application protocol, and whether copper and silk reach the STEP file were not found in a first-party source. The tutorial PDF exists but could not be text-searched here.

## Where the EDA tools differ

| | KiCad | Horizon EDA | Altium Designer | EasyEDA Pro |
| --- | --- | --- | --- | --- |
| Board solid | Optional | Always | Always | Optional |
| Holes | Pad holes; via holes by option | Through holes only; minimum diameter | All, selected, or by pad class | Not separately controlled; "Via" metal is an object |
| Copper | Optional, split into tracks, pads, zones, inner layers; Net filter | No | No (Parasolid only) | Top and bottom only |
| Mask | Flat faces, optional | No (board takes the mask colour) | No | Not listed |
| Silk | Flat faces, optional | No | No | Optional |
| Stand-in body | Extruded body in `master` source | None | Simple bodies, with a preference setting | Box from Height |
| Missing model | Warning, continue | Skipped silently | Not stated | Dialog listing causes |
| Not-fitted filter | Yes (DNP), plus variant | Yes (nopopulate), always applied | Not on the page read | Not stated |
| Other filters | Selected, reference filter, Footprint type | None | Selected; free and hidden bodies | By object class |
| Size filter | None | Hole diameter | None | None |
| Single part option | "Fuse shapes" for geometry only | No | Yes | Yes |
| Component name | Reference designator | Prefix + reference designator | Designator + optional suffix | Not stated |
| Origin | Grid, drill or user point | Board coordinates | Board origin | Not stated |
| Protocol | OCCT default (AP214 IS per source) | OCCT default | AP214 | Not stated |

Three patterns stand out. No tool read offers a choice of application protocol at export, and no tool read documents AP242 output. Copper in STEP divides the tools: Altium removed it and names file size as the reason, KiCad and EasyEDA Pro offer it as an option. A name prefix or suffix for multi-board MCAD files exists in Altium and Horizon and not in KiCad.

## What the MCAD tools accept

### FreeCAD

Sources: the dialog definitions [`DlgImportStep.ui`](https://raw.githubusercontent.com/FreeCAD/FreeCAD/main/src/Mod/Part/Gui/DlgImportStep.ui) and [`DlgExportStep.ui`](https://raw.githubusercontent.com/FreeCAD/FreeCAD/main/src/Mod/Part/Gui/DlgExportStep.ui). The wiki page [Import Export Preferences](https://wiki.freecad.org/Import_Export_Preferences) is behind a bot check and was not read. FreeCAD reads STEP through OCCT, whose reader handles AP214 and AP203 "and some parts of AP242" ([OCCT STEP user guide](https://occt3d.com/dev/doc/overview/html/occt_user_guides__step.html)).

Import settings that decide whether a board arrives as a usable assembly:

- **Enable STEP compound merge.** Merges all shapes into one compound, "resolving part instances and applying transforms". With it on, the assembly structure is gone.
- **Use LinkGroup.** Chooses the container type for groups (`App::LinkGroup` or `App::Part`).
- **Reduce number of objects.** Uses a Link array for repeated parts.
- **Expand compound shape.** Splits a compound holding several solids into separate objects.
- **Import invisible objects.** Brings in objects marked invisible in the file.
- **Ignore instance names.** For "legacy STEP files with non-meaningful auto-generated instance names". A file without meaningful instance names needs this.
- **Mode.** Single document, or a document per assembly or per object.

### SolidWorks

Sources: SolidWorks 2025 web help, [3D Interconnect](https://help.solidworks.com/2025/english/SolidWorks/sldworks/c_sw_3d_interconnect.htm), [Importing STEP, IGES, and ACIS Files](https://help.solidworks.com/2025/english/SolidWorks/sldworks/t_reading_step_iges_acis_sw.htm), and [Applying Filters to Import STEP Files](https://help.solidworks.com/2025/english/SolidWorks/sldworks/t_applying_filters_import_STEP_files.htm).

- **Protocols.** STEP AP203, AP214 and AP242.
- **Assembly Structure Mapping.** The default keeps the structure "as per the file". Other choices import an assembly as one multibody part, or turn a multibody part into an assembly.
- **Entities.** Solid and surface bodies, with an option to knit surfaces.
- **Large files.** An import filter shows the file's product structure as a tree before import, so that only chosen components are loaded; the help introduces it for "importing a large STEP file". An import that "takes too long" can be cancelled.
- **Import Diagnostics.** Can run automatically after each import to check the geometry.

The help text was recovered from the page's embedded data, so sentence boundaries in the original may differ from the quotes here.

### Onshape

Sources: [Importing Files](https://cad.onshape.com/help/Content/Document/importing_files.htm) and [Supported file formats](https://cad.onshape.com/help/Content/File/supported_file_formats.htm).

- **Protocols.** "STEP (.stp or .step) AP203, AP214, and AP242 (geometry and face color only)". Names of parts and assemblies are taken from the file.
- **Import to a single document.** Keeps the assembly structure; meant for "assemblies with a few hundred parts". Above 100 unique parts at one level, extra documents are created.
- **One document per part or assembly.** Meant for "multi-level assemblies with thousands of parts".
- **Combine to a single Part Studio.** Flattens the assembly; every instance becomes a duplicate part, and it "should only be made if the assembly is small".
- **Composite part.** Bodies can be joined into one composite part, at import or afterwards.
- **Appearances.** Face appearances are imported by default and can be switched off.
- **Axis.** Onshape is Z-up; a switch reorients Y-up files.
- **Upload limit.** 4 GB per file.

### Fusion as an importer

Not read. No Autodesk page on STEP import behaviour was reached.

## Known failure cases

- **File size and export time from copper.** KiCad marks tracks, pads, zones and shape fusing as time consuming. Altium removed copper from STEP export and says the legacy path "may generate extremely large files that are difficult to open in MCAD tools"; it says the same of Parasolid with copper, in milder words.
- **File size from holes and Components.** Altium describes "Export All" for both holes and Components as slower and larger, and offers selection as the remedy. Horizon offers a minimum hole diameter.
- **P-curves.** KiCad leaves them out by default to cut file size and read time, and keeps a switch because some software may need them.
- **Missing Component models.** In KiCad a library Footprint that references only a VRML model exports nothing unless model substitution is on. In EasyEDA Pro a model bound in a format other than STEP exports nothing. Horizon skips a package without a model silently.
- **Open outline.** KiCad cannot build the board when the Board Outline does not close within the chaining tolerance.
- **Name clashes across boards.** Component names are reference designators, so two boards in one MCAD file collide; Altium and Horizon add a suffix or prefix for this.
- **Model off screen.** Altium warns that a model placed relative to a distant board origin may be out of view after import.
- **Flattening on import.** FreeCAD's compound merge, SolidWorks' multibody mapping and Onshape's single Part Studio each discard the assembly structure the file carried; Onshape adds that this slows large assemblies.
- **Large assemblies on import.** Onshape splits documents above 100 unique parts per level; SolidWorks offers a pre-import filter and a cancel.
- **Faulty geometry.** One user reports on the [Onshape forum](https://forum.onshape.com/discussion/19605/diptrace-step-file-issue) that a DipTrace STEP file opened with the board's top face missing in Onshape while it opened correctly in SolidWorks and Pro/E. This is a single user report, not a vendor statement.
- **Non-ASCII names.** KiCad's source restricts the product name and header to ASCII because other characters "are creating issues in the step file".

## Not verified

- Fusion Electronics / Eagle: everything about what its STEP holds and which options exist.
- OrCAD X: the export options and the protocols offered.
- DipTrace: the export dialog, the protocol, and whether copper and silk are written.
- Fusion as a STEP importer.
- The FreeCAD wiki's description of its import preferences (the dialog source was read in its place).
- The protocol actually written by KiCad and Horizon: inferred from the absence of a schema setting and OCCT's default, not from a file.
- KiCad's default origin when none is chosen, and the release that introduced extruded bodies.
- Whether Altium applies a variant (fitted or not fitted) to STEP output, and which colours Altium and EasyEDA Pro write.
- AP242 import problems in SolidWorks reported on the Cadence forum: seen only as a search excerpt.
