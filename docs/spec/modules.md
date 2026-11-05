# Modules and milestones

The modules REDA plans to have, the milestone each one ships in, and where it sits in the app and in the crates. Only v0.1 is specified to ticket level, in [`v0.1/`](v0.1/README.md). Later milestones are described one module at a time and get their own planning before they are built.

Source: [What milestones follow v0.1, and what does each later module contain?](https://github.com/openreda/reda/issues/36)

## How modules fit the one app

- **One window and one Project per instance.** Every module adds Document types, Panels, Commands or view modes to the same window. A module never adds a separate application.
- **Every action is a Command** ([ADR-0001](../adr/0001-every-action-is-a-command.md)). A module's features can be reached from menus, toolbars, keys, the Command Palette, the CLI and MCP as soon as they exist.
- **Document types versus views.** A new Document type is added only when the user edits or views a different *thing*. Some features are a different way of looking at an existing Document, and they stay in that Document:
  - **3D** is a view mode of the PCB tab, toggled by a key in the same tab, with the same selection. To see 2D and 3D side by side, the user splits a second view of the same Document.
  - The **Stackup editor** opens as a tab of its own, bound to its PCB Document. It is saved in that `.redapcb` and has no file or Document type of its own.
  - **Hierarchy** adds no Document type: a child sheet is still a Schematic Sheet.
  - New Document types are only the **Panel Document**, the **Gerber Document** (read-only, for any Gerber/drill set) and the **Waveform Document** (simulation results).
- **Crate layering** ([ADR-0009](../adr/0009-one-model-crate-behaviour-crates-above.md)):
  - Persisted data goes in `reda-model`.
  - Behaviour goes in a domain crate above the model. Domain crates never depend on each other.
  - Cross-Document work goes in `reda-engine`.
  - Anything that makes network calls sits above the engine, because the engine stays offline.

## Milestones

Each milestone after v0.1 is a themed release that gives hobbyists a usable step forward. They are ordered by how much hobbyists want them. Finishing a milestone is a `release:minor` (v0.1 → `0.1.0`). **v1.0 is the public release.**

| Milestone | Contents | Leans on |
|---|---|---|
| **v0.1 Schematic to fab** | multiple flat sheets, a 2-layer PCB, manual routing, Zones, ERC/DRC, Gerber/Excellon/BOM/CPL, library editors and generators, Import part, MCP/CLI | see [`v0.1/`](v0.1/README.md) |
| **v0.2 Multilayer** | 4 or more copper Layers; Stackup editor; blind, buried and micro Vias; teardrops; CPL rotation corrections per Fab Preset | the N-layer model and Vias with a layer span; Fab Presets |
| **v0.3 Mechanical & 3D** | 3D view of the board; 3D models on Footprints; STEP export; DXF import and export (Board Outline, mechanical Layers) | Scene + wgpu backend ([ADR-0010](../adr/0010-canvas-drawn-by-own-wgpu-renderer.md)); Footprint/Part elements ([ADR-0008](../adr/0008-parts-bind-symbols-and-footprints.md)) |
| **v0.4 Interactive routing** | push-and-shove; differential pairs; length tuning | the v0.1 router that stops at clearance; Net Classes |
| **v0.5 Design scale** | hierarchical sheets with Sheet Instances, plus buses; multi-board Projects; Panel Documents; Variants; pin and gate swaps | [ADR-0004](../adr/0004-connectivity-derived-from-schematic-documents-own-their-data.md), [ADR-0006](../adr/0006-one-way-sync-with-write-through-edits.md) |
| **v0.6 Libraries & import** | KiCad project + library import; supplier part search (stock, price); library extras | `reda-import-kicad` |
| **v0.7 Outputs** | assembly PDFs; IPC-D-356; ODB++; Gerber Document (built-in Gerber viewer) | Output Jobs; the Gerber read-back test |
| **v1.0 Public release** | signing and notarization; updater; user docs; first translations and a language picker | packaging, i18n keys |
| **After 1.0** (unordered) | SPICE; autorouter; plug-ins; built-in git (commit/branch UI); KiCad export; WebAssembly build | see below |

## Modules after v0.1

### Multilayer (v0.2), in `reda-pcb`
- 4 or more copper Layers on the generic ordered Layer list that v0.1 already stores. Adding them needs no format migration.
- A **Stackup editor** tab bound to its PCB Document (see above).
- Blind, buried and micro Vias, using the layer span that every Via stores from v0.1.
- Teardrops, generated and cached the way Zone fills are.
- CPL rotation corrections per Fab Preset.

### Mechanical & 3D (v0.3)
- A **3D view** in a new `reda-3d` crate. It builds its scene from the model, and `reda-render`'s wgpu backend draws it.
- 3D models on Footprints. (Import part ignores 3D models until then.)
- STEP and DXF export in `reda-output`. DXF import in a new `reda-import-dxf`.

### Interactive routing (v0.4), in `reda-pcb`
- Push-and-shove, differential pairs and length tuning.
- v0.1 leaves room for them: Tracks are single segments with a Line|Arc geometry, and differential pairs and tuning groups arrive later as new object kinds.

### Design scale (v0.5)
- **Hierarchy**, in `reda-model` + `reda-schematic`:
  - One sheet file can be used several times, as Sheet Instances.
  - Designators and Net names are unique per instance.
  - A Component's identity becomes *Sheet Instance path + UUID*. That is why v0.1 Sync keys and Component references already allow a path prefix (see [Sync](v0.1/sync.md)).
  - Buses arrive in the same milestone.
- **Multi-board Projects**:
  - Each PCB Document Syncs from a chosen set of Schematic Sheets. The set belongs to the PCB Document, stored in `.redapcb` and edited on Board Setup ▸ Board, not to the Project.
  - Boards connect to each other through connector Parts.
  - There is no system-level or harness Document.
- **Panel Documents**:
  - A Panel places instances of the Project's own boards, with rails, mouse bites or V-score, and fiducials.
  - It takes boards from its own Project only. Cross-Project panels come from multi-board Projects.
  - v0.1 already commits that every Output Job names its PCB Document.
- **Variants**:
  - A Variant is a named, Project-level set of per-Component overrides: Not Fitted, value, and Part substitution.
  - Each Output Job picks one Variant, and the canvas shows the active one.
  - Variants generalise v0.1's Not Fitted flag.
- **Pin and gate swaps** back-annotated from the PCB, in `reda-pcb`, built as write-through Commands like the rest of [ADR-0006](../adr/0006-one-way-sync-with-write-through-edits.md).

### Libraries & import (v0.6)
- **KiCad import:**
  - A one-shot conversion of a whole KiCad project (schematic, PCB, project libraries) into a new REDA Project. There is no round-tripping.
  - It grows `reda-import-kicad`. KiCad export (a new `reda-export-kicad`) may come after 1.0.
- **Supplier part search** in a new `reda-supplier`, above the engine:
  - It shows stock and price.
  - It can only deep-link to a vendor's part page and pass the downloaded zip to Import part. The vendors' terms forbid automated fetching ([research](https://github.com/openreda/reda/issues/19)).
- **Library extras:** BGA generators, De Morgan bodies, custom polygon Pads, custom mask and paste shapes, Footprint copper graphics (usable as Net Tie copper), Graphical Parts, and similar.

### Outputs (v0.7), in `reda-output`
- Assembly PDFs, IPC-D-356 and ODB++.
- The Output Job editor may move from a dialog to a Document tab as outputs grow ([Projects and board setup](https://github.com/openreda/reda/issues/38)).
- A **Gerber Document** backed by a new `reda-gerber-read` crate. The Gerber read-back test shares that crate.

### Public release (v1.0)
- Signing and notarization, and an updater (`cargo-packager-updater` with minisign). Any automatic update check is an opt-in Preference.
- User docs.
- The first translations and a language picker. v0.1 already keys every UI string.

### After 1.0
- **SPICE**, in a new `reda-spice`:
  - It drives ngspice (BSD), installed separately as a shared library and loaded at runtime.
  - SPICE models are attached to Parts.
  - Results open in a Waveform Document.
- **Autorouter:** first as Specctra DSN export (`reda-output`) and SES import (`reda-engine`) to an external router. A native router would be a later, separate module.
- **Plug-ins:**
  - Plug-ins are external processes that drive Commands over the MCP/CLI bridge ([ADR-0007](../adr/0007-generic-mcp-tools-and-always-bridge.md)).
  - There is no in-process plug-in API, no dynamic libraries and no embedded scripting.
  - Plug-ins may add menu items bound to their own Commands, under the `plugin.<name>.` id prefix.
- **Version control:**
  - A visual Document diff (two versions overlaid in the canvas).
  - A `reda merge` git merge driver in `reda-cli`.
  - A built-in git client (commit/branch UI) in `reda-ui`.
- **WebAssembly** (`reda-web`): a full editing build if feasible, otherwise a read-only viewer (open a Project, browse the sheets and the board, cross-probe, view Layers). It has low priority. Its only standing cost is keeping the engine crates wasm32-clean.

## Later fog

These are not placed in any milestone yet. They were ruled out of scope for the [spec map](https://github.com/openreda/reda/issues/1), whose destination stops at a ticket-ready v0.1 and module-level later milestones, and are left for a future planning effort:

- **Fonts:** embedding fonts in a Project; searchable text in PDF exports; REDA-drawn stroke glyphs beyond Hershey's ASCII (µ, Ω, ±, °).
- **Checking:** background online DRC; a rule-query language with per-Layer and per-area rules; an editable pin-type conflict matrix; solder-mask slivers, acid traps and silk-to-silk spacing.
- **Performance:** benchmarks and regression tracking.
- **Fuzzing:** fuzz targets beyond file loading and KiCad import.
- **UI customisation:**
  - Keymap presets that copy other EDA tools' habits.
  - A Keymap picker, and Keymap export/import.
  - User-editable menus, toolbars and Active Bar, as an overrides layer on the stable item ids.
- **Text variables:** free Project parameters and `${...}` variables in sheet and board text (e.g. the revision on the silkscreen), substituted in the canvas, Gerber and PDF. v0.1 already reserves the syntax ([architecture](v0.1/architecture.md#file-format)).
- **Translations:** shipped languages; a language picker; locale number formatting; CJK fonts and right-to-left text.
