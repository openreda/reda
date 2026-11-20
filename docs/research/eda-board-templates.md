# What other EDA tools offer for new-board templates

Research for #393, part of map #392 (Board Templates for new boards, v0.2.5).
Gathered 2026-11-20. Where a vendor's docs could not be reached (login walls,
script-rendered pages, retired pages), the claim is marked **unverified** and
says what it rests on.

## Summary

No tool surveyed has a first-class "board template" object that is separate
from a whole project. The patterns are:

1. **Project-as-template (copy a folder).** This is the most common pattern:
   KiCad, Altium, and Xpedition/PADS Professional through the central library.
   A template is an ordinary project or board file. Creating from it copies the
   files and renames them. It carries everything in the file (outline, holes,
   connector footprints, stackup, rules, title block), with no parameters, and
   the copy keeps no link to the template.
2. **Wizard with built-in profiles.** Allegro/OrCAD has the Board Wizard, and
   older Altium had the PCB Board Wizard. A step-by-step dialog asks for units,
   layer count, a rectangular or circular outline with sizes, corner cutoffs and
   keepin distances. It fixes the variants a template can have, and can start
   from a template board or a board mechanical symbol.
3. **Shape dialogs only.** EasyEDA, DipTrace, Horizon EDA and LibrePCB offer
   rectangle, circle or rounded-rectangle outlines, typed in by numbers or drawn
   by hand, plus DXF import. They have no form-factor catalogue. LibrePCB and
   Horizon also offer whole-board copy or rule import.
4. **Mechanical-first (outline from MCAD).** Fusion derives the board from a
   sketch or enclosure, and can keep an associative link to that sketch.
   Altium, Xpedition and DipTrace import a STEP surface or a DXF.
5. **Templates as code (outside the requested list).** tscircuit ships
   form-factor boards as parameterized components with variants. Examples are
   `RaspberryPiHatBoard`, `ArduinoShield`, `XiaoBoard` and `MicroModBoard` with
   `variant: "processor" | "function"`.

Where form factors do ship, they are hobby and maker boards (KiCad) or legacy
bus cards (old Altium wizard). No surveyed tool ships PCIe, M.2 or Pi HAT+ as a
built-in. Those come from the community or from third parties.

## Per-tool findings

### KiCad (9.0 / 10.0)

- **Mechanism.** File → New Project from Template (Ctrl+T) opens the *Project
  Template Selector*. A template is a directory of project files plus a `meta/`
  subdirectory. `meta/info.html` is required: it is shown in the selector, and
  its `<title>` gives the display name. `meta/icon.png` (64×64) is optional.
  Creating from a template copies every file except `meta/` and dotfiles, and
  renames any file whose name contains the template name to the new project
  name. [KiCad 9 manual, Project templates][kicad9]
- **Dialog and preview.** In KiCad 10 the selector lists the installed templates
  and filters them by system or user. Clicking one shows its `info.html`, which
  may include images, in a panel. A built-in *Default* template is always
  present. [KiCad 10 manual][kicad10] The official templates ship a
  `meta/brd.png` board image that `info.html` shows, so the "preview" is a static
  picture the author made, not a live render.
  [kicad-templates RaspberryPi-HAT][kt-hat] The 10.0.x point releases fixed
  "Project Template Selector dialog preview layout" (10.0.5) and brought back
  browsing arbitrary template folders (10.0.7, #24145).
  [10.0.5 release][kicad1005], [10.0.7 release][kicad1007]
- **Locations.** System templates live under `KICAD9_TEMPLATE_DIR` and user
  templates under `KICAD_USER_TEMPLATE_DIR`. [kicad9]
- **Built-in form factors.** The official `kicad-templates` repo, last converted
  to the v9 format in Feb 2025, ships these in `Projects/`: API_Series-500,
  Arduino_Mega, Arduino_Micro, Arduino_Nano, Arduino_Pro_Mini, Arduino_Uno,
  BeagleBone-Black-Cape, Edgeberry_Cartridge, EuroCard160mmX100mm,
  Hammond_1593K_Enclosure, RaspberryPi-HAT, RaspberryPi-uHAT, STM32H7_DevEBox,
  STM32_Nucleo-64_Morpho, TI-LaunchPad-BoosterPack-20pin and -40pin, and
  stm32f100-discovery-shield. [kicad-templates tree][kt-tree] There is no PCIe,
  M.2, Feather or Pi HAT+ template. A Feather template was proposed in a merge
  request but is not in the tree. [kt MR 23][kt-mr23]
- **What a template carries.** Whatever the project files hold. The
  RaspberryPi-HAT template `.kicad_pcb` holds the Edge.Cuts outline built to the
  HAT mechanical spec, 4 × `MountingHole_2.7mm_M2.5`, a 2×20 socket
  footprint "placed correctly to align the two boards", cutouts for the camera
  and display cables, an alternate edge on User.Drawings for an SMT connector, a
  title block and a 2-layer stackup. The schematic holds an ID EEPROM circuit.
  [kt-hat info.html][kt-hat] Design rules and net classes travel in the
  `.kicad_pro`.
- **Parameters and variants.** None. Variants are separate templates
  (RaspberryPi-HAT vs RaspberryPi-uHAT, BoosterPack 20-pin vs 40-pin), and
  alternates are extra drawings on a user layer.
- **Saving your own.** There is no "save as template" command. You File → Save
  As a project into the user template folder and hand-write `meta/info.html`.
  [kicad10] A separate route for rules only is Board Setup → *Import Settings
  from Another Board*. It imports any of layers/stackup, mask/paste, text and
  graphics defaults, constraints, predefined track/via sizes, teardrops,
  tuning patterns, net classes, custom rules and severities. The manual says
  "This technique can be used to create a 'template' board". [KiCad 9 PCB
  manual][pcbnew9]
- **Community templates.** Many exist on GitHub. Examples are the Pi HAT+
  (RPi5) template [MisterHW/RPi5-HATplus][hatplus], which documents PCIe/CSI
  cutouts and links the HAT+ spec, the Feather Wing template
  [Bucknalla/kicad-feather-wing-template][feather] and the
  [devbisme RPi_Hat_Template][devbisme]. All use the same folder format, and you
  install one by dropping it into the template directory.
- **Link to template.** None. The result is a plain copy.

### Altium Designer (current AD / Altium 365)

- **Mechanism.** *Project templates.* File » New » Project opens the
  **Create Project** dialog. You pick a Project Type (PCB, multi-board, harness)
  and then one of its templates. `<Empty>` makes a shell project.
  [Creating a Project][ad-create] A template holds "a full set of common project
  files (including Output Jobs)" plus "design rules, parameters, units settings,
  document title blocks, grid settings, project release options".
  [Creating a Project Template][ad-tpl] Altium's board-shape docs say that for
  project templates "the associated PCB document in each case has a specific
  board shape, according to the form factor of the board".
  [Defining the Board Shape][ad-shape]
- **Storage and saving your own.** There are two routes:
  - Workspace templates: File » *Save Project As Template to Server*, or
    Preferences » Data Management » Templates » Save to Server. These are stored
    as revisioned items in a *Project Templates* folder. Sample templates arrive
    in `Managed Content\Templates\Project Templates` if Sample Data was chosen.
  - Local templates: plain project folders copied into the *Local Template
    folder*. *Template visibility* must be set to *Server & Local*.

  Template document names can use `[ProjectName]`, which is replaced on
  creation. [ad-tpl], [ad-create]
- **Dialog and preview.** The dialog has Locations, Project Type with a
  template list, name and description, and an Advanced section (folder, local
  storage, project parameters). It shows no board preview. Template revisions
  can be previewed only in the Explorer panel. [ad-create], [ad-tpl]
- **Built-in form factors.** **Unverified.** The pre-AD18 *PCB Board Wizard*
  offered "a range of industry-standard board outlines or you can define your
  own custom board size". [AD 17 New Board Wizard (now redirects)][ad-wiz] Older
  guides list AT bus, PCI bus and Eurocard among them. The AD 17 wizard page now
  redirects to the generic PCB docs, so the wizard is gone from current
  documentation. Current docs name no built-in form-factor templates, and a new
  PCB is a 6000 × 4000 mil rectangle. [ad-shape] Altium used to distribute extra
  "Board Templates" (PCIe, PC/104, COM Express) as downloadable design content.
  [fedevel blog][fedevel]
- **Parameters and variants.** None for the template. Project *parameters* are
  name/value pairs on the project, not geometry inputs. The old wizard had fixed
  fields such as units, size, corner cutoffs and layers.
- **Board-shape tooling.** Shapes can come from selected objects, from DXF/DWG
  on a mechanical layer of the Board Shape type, or from a planar face of a
  STEP/Parasolid/SOLIDWORKS body. Cutouts are negative regions. [ad-shape]
- **Link to template.** Not documented. A new project is "based on" the
  template, and nothing describes re-sync. [ad-tpl]

### Fusion (Electronics) / EAGLE

- **Mechanism.** There are no board templates. A new electronics design gets a
  default rectangle board if none is defined. A custom shape comes from the
  Design (MCAD) workspace: sketch on XY, or project an enclosure, then Solid »
  Create » *Create PCB*, which can be **associative (linked)** or
  **independent** of the sketch. A linked board "follows its sketch".
  [Autodesk: create a board outline][fus-outline]; also
  [derive outline from enclosure][fus-enc]
- **Reuse.** Fusion cannot create EAGLE-style design blocks. It inserts existing
  Fusion electronic designs directly instead. [fus-blocks] Autodesk's
  community answer for templates is to save a schematic/board pair with the
  outline and base setup, then *Save as*. [fus-tpl]
- **Form factors.** Only through libraries and example projects. Examples are
  the Adafruit and SparkFun libraries, which have Arduino form-factor outlines,
  and an EAGLE project page "blank shield" with "board dimensions, holes and
  headers already positioned". [Autodesk Arduino shield blog][fus-ard]
- **Link.** The only surveyed tool with a *live* link, and the link is to an
  MCAD sketch, not to a template.

### EasyEDA / LCEDA (Std and Pro)

- **Mechanism.** EasyEDA Std has a *Set Board Outline* wizard (Tools menu) with
  rectangle, circle and rounded rectangle. [EasyEDA Board Outline][ee-std] Pro
  has Place » Board Frame with rectangle, circle or polygon, rounding/chamfer of
  corners afterwards, and DXF import. "Only one plate frame" is allowed, and
  extra frames turn into slots. [EasyEDA Pro Board Outline][ee-pro]
- **Templates.** There are no board templates. The nearest features are
  *reuse blocks* (a project in the reuse-module library whose PCB acts as the
  layout template for later updates) and *drawing templates* for sheet frames.
  [Pro reuse template][ee-reuse], [Pro drawing settings][ee-draw] Users clone
  public projects (OSHWHub/Explore) as de-facto templates. [EasyEDA forum: PCB
  Template][ee-forum]
- **Form factors, parameters, link.** No catalogue. The outline dialog takes
  numbers for its basic shapes only, and there is no link.

### Cadence OrCAD X / Allegro X

- **Mechanism.** File » New » Board gives `Untitled.brd` with a default
  rectangular design outline. [OrCAD X Presto guide][presto] When a board is
  made from Capture, an *Input Board File* can name a template `.brd`.
  [EMA: new layout from Capture][ema-cap]
- **Board Wizard.** **Unverified** (Cadence's user guide is behind login, and
  the public blog body is script-rendered). Allegro PCB Editor's Board Wizard
  goes step by step through:
  - optional import of a template board, a technology file (`.tcf`), a parameter
    file (`.prm`) and a board mechanical symbol (`.bsm`);
  - units, drawing size and origin;
  - the etch cross-section (layer count and names);
  - default spacing, line width and via padstack;
  - a rectangular or circular outline with width and height, corner cutoff, and
    route-keepin and package-keepin distances.

  A Cadence forum reply confirms the wizard takes X/Y sizes and suits "standard
  rectangular/square board only". [Cadence forum, defining board size][cad-size]
- **Reuse units.** *Board mechanical symbols* (`.bsm`) package an outline,
  mounting holes and keepins as a placeable symbol on the `psmpath`. [Cadence
  training video][cad-bsm], [forum][cad-mech] *Parameter files* (`.prm`) carry
  settings such as artwork, colours and text sizes between boards. *Technology
  files* (`.tcf`) carry the cross-section and constraints. [forum][cad-mech]
- **Form factors.** None ship.
- **Link.** None. A placed mechanical symbol can be replaced, but the board is
  not tied to a template.

### Siemens Xpedition / PADS Professional

- **Mechanism.** **Layout templates** live in a `Templates` directory of the
  *central library* and are controlled by the librarian. [Expedition PCB User's
  Guide EE2007.1, third-party copy][xp-guide] A template is picked when the
  layout is created from the schematic, for example a "4 Layer
  QuickStart_Template" in PADS Professional, or in the Job Management Wizard
  from templates archived to the central library. [PADS Pro QuickStart][pads-qs],
  [Siemens community][xp-comm] Because each template is named after a layer
  count, it carries the layer stackup and setup. **Unverified:** the exact
  contents (outline, route border, rules), since Siemens docs need a support
  login.
- **Board outline tooling.** STEP import: pick a surface as the outline and an
  origin, import holes as contour, cavity or mounting hole, and auto-create a
  route border at a clearance. [Siemens blog: best way to create a board
  outline][xp-outline] PADS has a separate *constraint template*.
  [PADS VX.2.5 constraint template][pads-ct]
- **Form factors, parameters, link.** None documented.

### DipTrace

- **Mechanism.** There are no board templates. Objects » *Board Points* edits
  the outline numerically. *Create Rectangular Board* takes base, width and
  height, and the dialog can "make circular and rectangular boards with rounded
  corners automatically". Outlines can also be drawn, converted from shapes, or
  imported from DXF with a preview of the DXF layers. If no outline exists, the
  autorouter makes a rectangle. [DipTrace tutorial][dt-tut]
- **Templates elsewhere.** Sheet/title templates (ANSI A …) and *pad templates*
  exist. These are templates for parts of a document, not boards. [dt-tut]
- **Form factors, saving, link.** None found.

### Horizon EDA

- **Mechanism.** The New Project dialog has only name, location and pool.
  [Horizon: new project][hz-new] `Project::create` writes a `board.json` with a
  via rule and Gerber prefix but **no outline**. [horizon `project.cpp`][hz-src]
  You draw the outline with the polygon-rectangle tool on the Outline layer.
  [Horizon: create a board][hz-board]
- **Reuse.** Rule export/import arrived in v1.4.0. [v1.4.0 release][hz-140]
  Panelisation references other boards instead of copying them. [Horizon board
  editor][hz-imp]
- **Form factors, templates, link.** None.

### LibrePCB (2.x)

- **Mechanism.** The New Project wizard's *Initialization* page has "Add
  Schematic" and "Add Board", each with name and directory.
  [`newprojectwizardpage_initialization.ui`][lp-ui] A new board gets
  `addDefaultContent()`, a fixed **100 × 80 mm rectangle ("1/2 Eurocard
  size")** on Board Outlines. [`board.cpp`][lp-src] The quick-start guide says
  you choose fab design rules (e.g. LibrePCB Fab) when you initialize the
  board. [LibrePCB: create board][lp-doc]
- **Reuse.** A board can be copied within a project (`Board::copyFrom` copies
  stackup, rules, DRC and fab-output settings, and devices). [lp-src]
- **Form factors, templates, link.** None.

### Outside the requested list: tscircuit

These are templates as **parameterized code components**:
`@tscircuit/common` ships `ArduinoShield`, `RaspberryPiHatBoard`, `XiaoBoard`,
`ProMicroBoard`, `MicroModBoard` (`variant: "processor" | "function"`) and
`ViaGridBoard`. Each one sets the outline and places the connector as a real
chip with named pins, and `boardProps` passes through. Snapshot tests render the
PCB, schematic and 3D for each one. [tscircuit/common][tsc] This is the only
surveyed system with true variants and parameters, and with per-template golden
snapshots.

## Comparison

| Tool | Built-in form factors | Carries | Params / variants | Preview | User-saved | Link kept |
|---|---|---|---|---|---|---|
| KiCad | ~19 (Arduino, Pi HAT/uHAT, BeagleBone, Eurocard, Nucleo, LaunchPad, enclosure) | everything in project: outline, holes, connectors, stackup, rules, title block, user-layer drawings, schematic | no; separate templates | static `brd.png` in `info.html` | yes, by hand (folder + `meta/`) | no |
| Altium | none documented today; legacy wizard had AT/PCI/Eurocard (unverified) | whole project: PCB with shape, rules, OutJobs, title blocks, grids | no (project parameters are not geometry) | none in Create Project | yes: Workspace (revisioned) or local folder | not documented |
| Fusion/EAGLE | none; library outlines | n/a | no | n/a | Save As workaround | associative to MCAD sketch (optional) |
| EasyEDA | none | n/a (shape dialog) | rect/circle/rounded sizes | none | clone projects; reuse blocks | no |
| OrCAD/Allegro | none | wizard: layers, spacing, via, outline, keepins; `.bsm`/`.tcf`/`.prm` units | wizard fields (rect/circle, size, corner cut, keepins) | none | template `.brd`, `.bsm`, `.tcf`, `.prm` | no |
| Xpedition/PADS Pro | none documented | layer stackup + setup (contents unverified) | no; per-layer-count templates | none documented | yes, central library | no |
| DipTrace | none | n/a | rect/circle/rounded via Board Points | DXF import preview | no | no |
| Horizon | none (no outline at all) | n/a | no | none | rules export/import | no |
| LibrePCB | 100 × 80 mm default | outline + chosen fab rules | no | none | copy board in project | no |
| tscircuit | Arduino, Pi HAT, Xiao, ProMicro, MicroMod | outline + connector component | **yes** (props, variants) | snapshot renders | yes (it's code) | n/a (code) |

## REDA should match

- **A catalogue with a Default/Custom-rectangle entry that is always present.**
  KiCad always keeps Default, and Altium has `<Empty>`.
- **The popular maker form factors that KiCad ships.** These are Pi HAT and
  uHAT/Zero, Arduino Uno/Mega/Nano, a Eurocard size, Nucleo-64 and BeagleBone
  cape. Missing them would feel like a regression next to KiCad.
- **Templates that carry the full mechanical frame.** That is the Board Outline,
  Board Cutouts, mounting holes, connector Footprints at spec positions and
  cable cutouts, plus helper drawings such as KiCad's alternate edge on a user
  layer.
- **Built-ins in the user format, with user templates in a user directory.**
  KiCad has system and user template directories, and Altium has local and
  Workspace templates. This matches the map's standing decision.
- **Name, description and image per template.** KiCad's `info.html` and icon
  show this.
- **No link after creation.** This is universal, apart from Fusion's MCAD
  associativity.
- **Basic parametric shapes.** Rectangle, circle and rounded rectangle with
  typed sizes are offered by EasyEDA, DipTrace, Allegro's wizard and old
  Altium.
- **Keepin/keepout distance from the edge** as part of creating a board, as in
  the Allegro wizard's route and package keepin.

## Worth leading on

- **Live 2D preview with key dimensions.** Every surveyed tool shows either
  nothing or a static, author-made picture (KiCad `brd.png`). A rendered preview
  that updates as variants and parameters change has no equivalent anywhere.
- **Real families with variants and parameters.** For example Pi HAT+
  full/mini, PCIe x1–x16 at full height or low profile, and M.2 2230–2280. Only
  tscircuit has variants, and only as code. GUI tools make one template per
  variant or offer fixed wizard fields.
- **Form factors nobody ships built in.** These are Pi HAT+ (the RPi5
  PCIe/CSI cutouts exist only in community templates), PCIe add-in cards, M.2
  and Feather/FeatherWing.
- **Connector as a real Component in schematic and PCB.** KiCad's HAT template
  does put the 2×20 header in the schematic, but only because the template is a
  whole project. tscircuit models the connector as a component with named pins.
  REDA can do this without shipping a full project.
- **Save board as template from the app and over MCP.** KiCad has no
  save-as-template command, and Altium's is tied to the Workspace. A
  `template.save` that writes the same format as the built-ins, plus
  list/show over MCP, would be unique.
- **Checks against the published spec.** These are golden tests per built-in
  template and preview snapshots (tscircuit does snapshots). No GUI tool
  claims its templates are tested against the mechanical spec.
- **A suggested layer count without overriding the Fab Preset.** Tools that
  carry rules either copy them all (KiCad, Altium) or put them in a separate
  file (Allegro `.tcf`/`.prm`). Keeping the frame apart from the fab choice is
  cleaner than both.

## Sources

[kicad9]: https://docs.kicad.org/9.0/en/kicad/kicad.html
[kicad10]: https://docs.kicad.org/10.0/en/kicad/kicad.html
[kicad1005]: https://www.kicad.org/blog/2026/07/KiCad-10.0.5-Release/
[kicad1007]: https://www.kicad.org/blog/2026/10/KiCad-10.0.7-Release/
[pcbnew9]: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html
[kt-tree]: https://gitlab.com/kicad/libraries/kicad-templates/-/tree/master/Projects
[kt-hat]: https://gitlab.com/kicad/libraries/kicad-templates/-/tree/master/Projects/RaspberryPi-HAT
[kt-mr23]: https://gitlab.com/kicad/libraries/kicad-templates/-/merge_requests/23
[hatplus]: https://github.com/MisterHW/RPi5-HATplus
[feather]: https://github.com/Bucknalla/kicad-feather-wing-template
[devbisme]: https://github.com/devbisme/RPi_Hat_Template
[ad-create]: https://www.altium.com/documentation/altium-designer/creating-project?version=22
[ad-tpl]: https://www.altium.com/documentation/altium-designer/creating-project-template
[ad-shape]: https://www.altium.com/documentation/altium-designer/pcb/defining-board-shape
[ad-wiz]: https://altium.com/documentation/altium-designer/pcbmaker-dlg-form-boardwizardaltium-designer-new-board-wizard-ad?version=17
[fedevel]: https://fedevel.com/blog/how-to-download-altium-templates-pcie-card-com-express-pc104-and-other-form-factors
[fus-outline]: https://www.autodesk.com/support/technical/article/caas/sfdcarticles/sfdcarticles/How-to-create-a-board-outline-for-a-PCB-in-Fusion-360.html
[fus-enc]: https://www.autodesk.com/support/technical/article/caas/sfdcarticles/sfdcarticles/How-to-derive-a-PCB-outline-from-an-existing-enclosure-3D-model-in-Fusion-360.html
[fus-blocks]: https://www.autodesk.com/support/technical/article/caas/sfdcarticles/sfdcarticles/How-to-create-electronics-design-blocks-in-Fusion.html
[fus-tpl]: https://forums.autodesk.com/t5/fusion-electronics-forum/create-and-use-a-pcb-template/td-p/12045973
[fus-ard]: https://www.autodesk.com/products/fusion-360/blog/arduino-shield-buying-designing/
[ee-std]: https://docs.easyeda.com/en/PCB/Board-Outline/
[ee-pro]: https://prodocs.easyeda.com/en/pcb/place-board-outline/
[ee-reuse]: https://prodocs.easyeda.com/en/pcb/design-update-place-and-route-from-reuse-template/
[ee-draw]: https://prodocs.easyeda.com/en/schematic/design-project-drawing-setting/
[ee-forum]: https://easyeda.com/forum/topic/PCB-Template-658c780f084846ac9af60bb28dd9c061
[presto]: https://resources.pcb.cadence.com/orcad-x-presto-user-guide/02-creating-a-pcb-layout-in-orcad-x-presto-basic-design-tasks
[ema-cap]: https://www.ema-eda.com/how-to-page/how-to-create-a-new-pcb-layout-from-a-schematic-in-orcad-capture/
[cad-size]: https://community.cadence.com/cadence_technology_forums/pcb-design/f/allegro-x-pcb-editor/48501/defining-board-size
[cad-bsm]: https://resources.pcb.cadence.com/trainingvideos/creating-a-board-outline-symbol-using-the-allegro-pcb-editor
[cad-mech]: https://community.cadence.com/cadence_technology_forums/pcb-design/f/pcb-design/13179/mechanical-symbols
[xp-guide]: https://www.scribd.com/document/624511486/Chapter-2
[pads-qs]: https://solidedge.siemens.com/wp-content/uploads/2020/11/PADS_Professional_QuickStart_Guide.pdf
[xp-comm]: https://community.sw.siemens.com/s/question/0D54O00006eo5mOSAQ/how-to-link-a-pcb-to-another-schematics-in-pads-pro
[xp-outline]: https://blogs.sw.siemens.com/electronic-systems-design/2019/06/12/what-is-the-best-way-to-create-a-board-outline/
[pads-ct]: https://blogs.sw.siemens.com/electronic-systems-design/2019/03/25/whats-new-in-pads-vx-2-5-constraint-template/
[dt-tut]: https://diptrace.com/support/tutorials/tutorial_html/
[hz-new]: https://docs.horizon-eda.org/en/latest/new-project.html
[hz-board]: https://docs.horizon-eda.org/en/latest/create-board.html
[hz-imp]: https://docs.horizon-eda.org/en/latest/imp-board.html
[hz-src]: https://github.com/horizon-eda/horizon/blob/master/src/project/project.cpp
[hz-140]: https://newreleases.io/project/github/horizon-eda/horizon/release/v1.4.0
[lp-ui]: https://github.com/LibrePCB/LibrePCB/blob/master/libs/librepcb/editor/project/newprojectwizard/newprojectwizardpage_initialization.ui
[lp-src]: https://github.com/LibrePCB/LibrePCB/blob/master/libs/librepcb/core/project/board/board.cpp
[lp-doc]: https://librepcb.org/docs/quickstart/create-project/board
[tsc]: https://github.com/tscircuit/common

- KiCad 9 manual, project templates: <https://docs.kicad.org/9.0/en/kicad/kicad.html>
- KiCad 10 manual: <https://docs.kicad.org/10.0/en/kicad/kicad.html>
- KiCad 10.0.5 / 10.0.7 release notes: <https://www.kicad.org/blog/2026/07/KiCad-10.0.5-Release/>, <https://www.kicad.org/blog/2026/10/KiCad-10.0.7-Release/>
- KiCad 9 PCB Editor manual (Import Settings from Another Board): <https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html>
- kicad-templates repo: <https://gitlab.com/kicad/libraries/kicad-templates/-/tree/master/Projects>
- Altium, Creating a Project / Project Template / Defining the Board Shape: <https://www.altium.com/documentation/altium-designer/creating-project?version=22>, <https://www.altium.com/documentation/altium-designer/creating-project-template>, <https://www.altium.com/documentation/altium-designer/pcb/defining-board-shape>
- Autodesk Fusion support articles: see link references above
- EasyEDA Std/Pro docs: <https://docs.easyeda.com/en/PCB/Board-Outline/>, <https://prodocs.easyeda.com/en/pcb/place-board-outline/>
- Cadence OrCAD X Presto guide: <https://resources.pcb.cadence.com/orcad-x-presto-user-guide/02-creating-a-pcb-layout-in-orcad-x-presto-basic-design-tasks>
- Siemens PADS Professional QuickStart: <https://solidedge.siemens.com/wp-content/uploads/2020/11/PADS_Professional_QuickStart_Guide.pdf>
- DipTrace tutorial: <https://diptrace.com/support/tutorials/tutorial_html/>
- Horizon EDA docs and source: <https://docs.horizon-eda.org/en/latest/>, <https://github.com/horizon-eda/horizon>
- LibrePCB docs and source: <https://librepcb.org/docs/quickstart/create-project/board>, <https://github.com/LibrePCB/LibrePCB>
- tscircuit common: <https://github.com/tscircuit/common>
