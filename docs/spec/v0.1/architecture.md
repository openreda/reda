# v0.1 architecture

The crate layout, the design-data model, the file format, and the Command system with its MCP and CLI surfaces.

## Crates

Sources: [How is the workspace split into crates?](https://github.com/openreda/reda/issues/16), [ADR-0009](../../adr/0009-one-model-crate-behaviour-crates-above.md), and the amendment in [How are text, Zone fills, highlighting and layer colours drawn on the canvas?](https://github.com/openreda/reda/issues/26).

Each crate lives in `crates/reda-<name>/`, and the directory name is the package name. Nothing is published to crates.io: the workspace sets `publish = false`.

| Crate | Holds | Depends on |
|---|---|---|
| `reda-geom` | nm and millidegree types, unit-string parsing, Line\|Arc shapes, wrappers over `i_overlay` and `rstar` | — |
| `reda-text` | font loading from bytes, Hershey parsing, shaping (`harfrust`), outlines (`skrifa`), glyph triangulation (`i_triangle`) | geom |
| `reda-model` | every persisted type (Project, Schematic Sheet, PCB Document, Symbol, Footprint, Part), UUIDs, transactions, change sets, serde derives. Data only. | geom |
| `reda-command` | the Command trait, kinds, `available(ctx)`, error codes, the edit result envelope, `batch`, cancellation and progress, the registry | model |
| `reda-library` | Library folder scanning, Footprint generators, the Library check, library Commands | command |
| `reda-import-kicad` | the S-expression parser and the mapping for Import part | library |
| `reda-schematic` | connectivity derivation, ERC, schematic Commands | command |
| `reda-pcb` | copper connectivity, Ratsnest, DRC, Zone fill, PCB Commands | command, text |
| `reda-render` | Scene building, the wgpu backend ([ADR-0010](../../adr/0010-canvas-drawn-by-own-wgpu-renderer.md)), the PDF and SVG writers, the `document.render` Command | command, text |
| `reda-output` | Gerber, Excellon, BOM/CPL, Output Jobs, Fab Presets | command, text (+ pcb as needed) |
| `reda-engine` | Project open/save, the canonical JSON writer, format migrations, Sync, Re-annotate, other cross-Document Commands, registry assembly from every `register_all()`, the Journal. It is the headless entry point. | all engine crates |
| `reda-control` | the loopback JSON-RPC protocol and token auth: the server side for the app, the client side for the CLI | engine |
| `reda-ui` | the window shell, docking, Panels, every Document editor and Tool, the Keymap, icons, the Fluent catalogue | engine, control |
| `reda-app` | the thin GUI binary, plus OS-specific work such as finding installed fonts (it hands font bytes to the engine) | ui |
| `reda-cli` | the `reda` binary. It is the only crate that depends on `rmcp`. | engine, control |
| `reda-testkit` | dev-only: `assert_golden`, the test-engine builder, the scenario harness | (dev-dependency only) |

Rules:
- Domain crates (library, schematic, pcb, render, output) don't depend on each other except where the table says so. Cross-Document work happens only in `reda-engine`.
- The UI reads `&Project` directly to draw, and changes it only by running Commands.
- There is one UI crate until compile times force a split per editor.
- Enforcement:
  - CI builds every crate below `reda-control` for `wasm32-unknown-unknown`.
  - A workspace test checks `cargo metadata` against a checked-in allow-list of dependency edges. That test also checks that `reda-testkit` appears only as a dev-dependency.
- Third-party crates already chosen:
  - geometry and fonts: `i_overlay`, `i_triangle`, `rstar`, `skrifa`, `harfrust`
  - outputs: `gerber-types`; the Excellon writer is REDA's own
  - Commands and MCP: `schemars` 1.x, `rmcp`
  - window and UI: egui/eframe and `egui_wgpu`, `egui_extras` (SVG through resvg), `rfd`, `muda`, `egui_kittest`
  - per-user state: `directories`, `toml_edit`
  - i18n: `fluent-bundle`
  - REDA stays on upstream egui and winit, with no forks ([research](https://github.com/openreda/reda/issues/2)).

## Design-data model

Sources: [How is design data modelled in the engine?](https://github.com/openreda/reda/issues/9), [ADR-0004](../../adr/0004-connectivity-derived-from-schematic-documents-own-their-data.md).

- **Coordinates:**
  - `i64` nanometres everywhere, on Schematic Sheets and the PCB Document alike, with one shared geometry type.
  - Angles are integer millidegrees. Any rotation is allowed. Rounding happens only when geometry is computed, never in stored data.
- **Identity:**
  - Every persisted object carries a UUIDv4, down to single Track segments and Layers.
  - Commands, MCP, files and Change Lists address objects by UUID. Dense in-memory handles may exist, but they never leak out.
- **Schematic connectivity is derived:**
  - Nets follow live from Wires, Net Labels, Ports and Power Ports, using fixed scope rules (see [schematic](schematic.md#connectivity)).
  - Derivation is incremental, driven by change sets. A full re-derive per transaction is an acceptable v0.1 fallback. There is no "compile" step.
- **Component data:**
  - The schematic is the master. Each Unit carries the Component UUID and a duplicated copy of Designator, value, fields, Footprint choice, Component Type and Not Fitted. Commands keep the copies equal, and ERC flags a mismatch.
  - The PCB Document holds a **Footprint instance** linked by Component UUID. It has its own copy of Designator, value, fields, Footprint and Not Fitted, and a Net per Pad, so outputs read only the PCB Document ([Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)).
  - Mechanical Components may exist only on the PCB. They keep their fields and Not Fitted on their Footprint instance, which has no Component link.
- **PCB Nets** are stored objects with a UUID, a name and a Net Class. Pads, Tracks, Vias, Zones and Free Pads reference them by UUID. Net Classes live on the PCB side.
- **Undo and redo:**
  - One Command is one transaction. It records a before/after change set per object, keyed by UUID.
  - The same change set feeds dirty tracking, incremental derived data (spatial index, Ratsnest, connectivity), the Journal and Change List previews.
  - There is **one undo stack per Document**. A cross-Document Command (Re-annotate, Sync, write-through edits) records an independent entry in each Document it touches.
  - The **project file** (`.reda`) has its own undo stack too. It covers adding, deleting, renaming and reordering Documents, Project properties, ERC severities, Output Jobs, Fab Preset copies and Library paths. Deleting a sheet that holds Components is one transaction across `.reda` and that sheet. In the GUI, focus picks the stack that `Mod+Z` uses: the Projects panel and the Project dialogs use the Project stack, and a Document tab uses its own ([app](app.md#projects-and-settings)).
- **Stored versus derived:**
  - Only user-authored data is stored.
  - Derived: schematic Nets, auto Net names, Junctions, PCB copper connectivity, the Ratsnest, spatial indices, ERC and DRC results.
  - Zone fills go in a separate, rebuildable cache, not in the PCB Document.
- **Board primitives:**
  - Layers are an ordered list of objects (UUID, kind, side), referenced by UUID and never by index.
  - A Track is one segment with a Line|Arc geometry. v0.1 creates only Lines.
  - A Via stores its layer span (from, to).
  - Later object kinds (differential pairs, tuning groups) are added as new kinds.
- **Model shape:**
  - Per-Document typed tables keyed by UUID, plus a single `Project` value that owns them. No UI or OS dependencies.
  - There is a single writer, and data changes only through transactions.
  - Background jobs (Zone fill, DRC, Recovery Snapshots) read a view and post their results back as Commands.
- **Engine inputs:** the engine takes an injected `IdSource` (random v4 in real sessions, a seeded counter in tests) and an injected `Clock`.

## File format

Sources: [What does REDA's native file format look like?](https://github.com/openreda/reda/issues/10), [ADR-0005](../../adr/0005-canonical-json-one-object-per-line.md), [ADR-0008](../../adr/0008-parts-bind-symbols-and-footprints.md).

- **Canonical JSON:**
  - Files are UTF-8 with 2-space indent and a fixed key order.
  - Values are integers only: i64 nm and millidegrees.
  - UUIDs are lowercase hyphenated strings.
  - The same data always serialises to the same bytes. Hand edits are tolerated on read and normalised on the next save.
- **One object per line:**
  - Small objects (Wire, Net Label, Unit instance, Track, Via, Pad…) are compact JSON on one line.
  - Containers (part-table entries, Footprint instances with their Pads, the Board Outline) span lines, with each child on its own line.
- **Order:** a file is grouped into tables by object kind, and each table is sorted by UUID, never by insertion order.
- **Project layout.** A Project is a folder:
  - `name.reda` lists the Documents in order, as relative path + UUID. That order drives sheet numbering and PDF output, and nothing is found by scanning the folder.
  - `name.reda` also holds the Project-wide settings: Project properties, ERC severities, Output Jobs, copied Fab Presets and Project Library paths.
  - There is one file per Document: `name.redasch` (Schematic Sheet) and `name.redapcb` (PCB Document).
  - A Document's name is its file stem. Renaming a Document renames its file on save. Adding or deleting a Document reaches disk on save, and a deleted Document's file goes to the OS trash ([app](app.md#projects-and-settings)).
- **Part tables:**
  - Each Document stores its part copies once, in a table keyed by library element UUID + content hash. Instances point at the table. A locally edited copy gets its own entry by hash.
  - A Schematic Sheet stores the whole Part: Symbol, pin-map overrides, fields, Component Type, and every alternate Footprint deduplicated by hash. The PCB stores only the Footprints in use.
  - Per-Component data (Designator, value, MPN and other fields, Footprint choice, Component Type, Not Fitted) stays on the instances.
  - Library elements use exactly the same JSON shape, so placing a part is a literal copy.
  - A Project opens and produces outputs with no Library present.
- **Versioning:**
  - Every file carries `"format": <int>`.
  - Loading runs a chain of raw-JSON migration steps before typed deserialisation: one step per version, kept forever, each golden-tested.
  - REDA writes only the current version.
  - Files from a newer REDA are refused with a clear message. Unknown fields are an error.
- **JSON Schema:** generated per file kind from the Rust types with `schemars`, checked in, and kept current by a test.
- **`.reda/` folder:**
  - REDA writes `.reda/.gitignore` containing `*`.
  - Zone fill caches: one binary file per PCB Document, carrying a hash of its inputs so a stale cache is refilled.
  - `view.json`: per-user view state ([app](app.md#per-project-view-state)).
  - `recovery/`: Recovery Snapshots ([app](app.md#crash-recovery)).
- **Saving:**
  - Dirty state and save are per Document, plus Save All.
  - Every write goes to a temp file and is then renamed.
  - REDA never saves the real files on its own.
- **Registered extensions:** `.reda`, `.redasch` and `.redapcb`. Other REDA files are `.redalib`, `.redasym`, `.redafp`, `.redapart` and `.redafab`.

## Commands

Sources: [How are commands defined, discovered and exposed over MCP?](https://github.com/openreda/reda/issues/12), [ADR-0001](../../adr/0001-every-action-is-a-command.md), [ADR-0007](../../adr/0007-generic-mcp-tools-and-always-bridge.md), [How do Commands get their labels, icons and shortcuts?](https://github.com/openreda/reda/issues/27).

### Definition
- **Ids:**
  - Dotted `domain.noun.verb` in lowercase snake_case, scoped by Document type where it matters: `schematic.wire.add`, `pcb.track.add`, `component.set_value`, `sync.apply`, `project.save`, `view.zoom`.
  - Ids are not promised to be stable before 1.0. `plugin.<name>.` is reserved.
- **Shape:**
  - A params struct (`Deserialize + JsonSchema`, schemars 1.x) and a result struct (`Serialize + JsonSchema`, which becomes the MCP `output_schema`).
  - Metadata: id, English title, description (written for tooltips and for agents alike), category and a **semantic icon key** (`"wire"`, `"save"`, `"zoom_fit"`).
  - Each crate registers its Commands explicitly with `register_all()`. There is no `inventory`.
- **Kinds.** Undoability follows from the kind:
  - **edit** mutates Documents in one transaction and is undoable.
  - **query** is read-only and returns data.
  - **view** changes GUI or session state. It is never undoable, never dirties a Document, and is unavailable headless.
  - **file** covers Project new/open/save/close and exports. It touches disk, is never undoable, and works headless. `project.new` is one file Command that builds a whole new Project ([app](app.md#new-project)).
- **Targets:**
  - Targets are always explicit UUIDs. The GUI fills them in from the selection, which stays view state.
  - The Document is inferred from the object UUIDs. Creation Commands take an explicit `document` and an optional UUID supplied by the caller.
- **Availability:**
  - An optional `available(ctx) -> Result<(), Unavailable { reason }>`. `ctx` is the Project, plus the active Document and the selection in the GUI.
  - Menus grey out an unavailable Command and show the reason as a tooltip. A call fails with the reason.
  - View Commands are unavailable headless, with the reason "requires the REDA app".
- **Errors:**
  - Errors are `{code, message, details?}`.
  - `code` is one of `invalid_params` (with a JSON pointer to the bad field), `not_found`, `unavailable`, `conflict` (a stale guard), `rejected` (a domain rule) or `internal`.
  - A failed edit rolls back fully. So does any failed step of a batch.
- **Edit result envelope:** `{changes: {<document>: {created, modified, deleted: [uuid]}}, result}`, taken from the change set.
- **Units:**
  - Length params accept integer nm **or** unit strings (`"1.27mm"`, `"10mil"`, `"0.5in"`), as a `oneOf` in the schema.
  - Angles accept millidegrees or `"45deg"`.
  - Results always return integer nm and millidegrees.
- **Long-running Commands:**
  - They are synchronous from the caller's view, with a cancellation token and a progress reporter.
  - These map to MCP progress notifications and cancellation, and to a GUI progress bar with Cancel.
  - There are no job ids.
- **Batch:**
  - `batch {commands: [...]}` is one atomic edit, with one undo entry per touched Document.
  - Later steps reference objects created by earlier steps through UUIDs the caller supplied.
  - There is no begin/commit API.
- **Presets:**
  - A Command may declare named Presets (`command:preset`), each with fixed params, a title and an icon key.
  - `tool.activate` generates its Presets from the Tool registry.
  - `panel.show:<id>` and `layout.apply:<name>` are Presets too.
- **Catalogue:**
  - `reda commands --json` dumps the registry, including Presets.
  - A generated `docs/commands.json` is checked in and verified by a test.

### Tools and dialogs
- **Tools are not Commands.** A Tool (place, Wire, route, drag-move…) is a GUI-side state machine:
  - It previews without mutating. Routing calls engine queries such as clearance.
  - It **commits by running ordinary edit Commands**, one undo entry per commit. A Wire run becomes `schematic.wire.add`, a drag release becomes a move, and each committed route becomes `pcb.track.add`.
- **Activating a Tool** is the view Command `tool.activate {tool}`.
- **Keys inside a Tool** are view Commands that are available only while a Tool that supports them is active: `tool.rotate`, `tool.mirror_x`, `tool.mirror_y`, `tool.flip_side`, `tool.edit_properties`, `tool.undo_segment`, `tool.cycle_corner`, `pcb.route.add_via`.
- **Dialogs:**
  - A GUI call that lacks required params opens a form generated from the schema. A command can replace that form with a hand-written dialog, as Sync's Change List does.
  - A Command called with full params never opens a dialog.

### MCP
- **SDK:** `rmcp`, with a hand-written `ServerHandler` driven by the registry.
- **Tools:**
  - Generic: `search_commands`, `describe_command` and `run_command {id, params}`.
  - Query Commands, also reachable from the CLI: `project.overview`, `objects.query` (by Document, object type, Net, Designator or region), `object.get`, `erc.run`, `drc.run`, `sync.preview` and `document.render`.
- **`document.render`** params are explicit and independent of the user's view:
  - view: fit the Document (default), a rectangle in nm, or fit a set of object UUIDs
  - size: in pixels (default 1024 px on the long side)
  - theme: dark or light
  - Layers: the visible Layers, plus an optional active Layer
  - highlight: UUIDs to highlight
  - format: `png` (default) or `svg`

  It draws through the same Scene and wgpu backend as the canvas ([app](app.md#canvas-rendering)). Headless, it uses a software adapter. With no adapter, it returns an error naming the cause. When bridged, the running app's GPU renders.
- **Transport:** stdio only.

### CLI and control port
- **One `reda` binary**, separate from the GUI app and shipped in the same package:
  - `reda mcp [--project p]`
  - `reda run <id> [--params json | --key value…] [--project p] [--save]`
  - `reda run --script journal.jsonl`
  - `reda commands [--json]`
  - `reda --version`
- **Always bridge:**
  - If a running instance has the target Project open, the CLI bridges to it. Otherwise it runs in-process.
  - With no `--project` and exactly one instance running, it bridges to that instance.
- **Saving:**
  - In-process `reda run` writes nothing to disk without `--save` or `project.save`.
  - In-process `reda mcp` saves only through `project.save`, and warns on stderr when it discards changes on exit.
  - Bridged sessions never save. Their edits show as unsaved changes in the app.
- **Control port:**
  - Loopback TCP on `127.0.0.1` with an OS-assigned port.
  - Newline-delimited JSON-RPC 2.0 with `commands.list`, `commands.describe` and `commands.run`.
  - Each instance writes `instances/<pid>.json` (port, a random 256-bit token, the open Project path) to an owner-only per-user runtime dir ([app](app.md#per-user-state)).
  - The first message must present the token.
  - The port is on by default. The app shows an on-screen indicator and asks "Allow this agent?" on the first connection. The Agents Preferences page holds the bridge switch and the allowed agents.
- **Concurrency:**
  - Bridged Commands queue to the app's single writer and apply between frames.
  - They land in the normal per-Document undo stacks, labelled with their source ("MCP: Add Track").
  - If a bridged Command modifies an object that a user's in-flight Tool depends on, the Tool cancels its preview and shows a status-bar note. The bridged Command never waits for the user.
- The bridge also routes file-association opens to a running instance ([packaging](quality-and-release.md#packaging)).

### Journal
- Each session keeps an in-memory Journal of every Command: id, params, source (GUI, CLI or MCP), and the result or error.
- The Journal is shown in a Panel, can be exported as JSON lines, and can be replayed with `reda run --script`.
- Replay remaps UUIDs: each new UUID in a recorded result maps to the UUID created on replay, and is substituted in later params. Replay fails if a result's shape differs from the recording.
- The only on-disk copy is the crash-evidence mirror at `.reda/recovery/journal.jsonl` ([app](app.md#crash-recovery)).
