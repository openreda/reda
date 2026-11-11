# v0.1: schematic to fab

v0.1 takes a hobbyist from an empty Project to a zip they can upload to a fab. They draw the circuit on several flat Schematic Sheets using Parts from a Library, Sync it to a 2-layer PCB Document, place and route the board by hand with Zones, check it with ERC and DRC, and run an Output Job that writes Gerber X2, Excellon drill, a BOM and a CPL. Every step is a Command, so an agent can do the same through MCP or the `reda` CLI.

v0.1 is a tester release, shipped unsigned. Finishing it is released as `0.1.0`.

## Files

| File | Area |
|---|---|
| [`architecture.md`](architecture.md) | Crates and dependency rules; the design-data model; the file format; Commands, Tools, MCP, the CLI and the control port; the Journal |
| [`schematic.md`](schematic.md) | Schematic editor |
| [`editing.md`](editing.md) | Shared editing rules: selection, Select Similar, snapping, the grid, Properties, numeric expressions, highlighting, the delta, navigation |
| [`library.md`](library.md) | Libraries, Parts, the library editors, Footprint generators, Import part, the starter Library |
| [`pcb.md`](pcb.md) | PCB editor |
| [`sync.md`](sync.md) | Schematic → PCB Sync, write-through edits, cross-select |
| [`checks.md`](checks.md) | ERC, DRC, design rules, Violations, Waivers |
| [`outputs.md`](outputs.md) | Output Jobs, Fab Presets, Gerber/Excellon/BOM/CPL, schematic PDF |
| [`app.md`](app.md) | Window shell, docking, New Project, the Projects panel and settings dialogs, canvas rendering, Keymap, icons, Preferences and per-user state, crash recovery, i18n, accessibility |
| [`quality-and-release.md`](quality-and-release.md) | Testing, CI, merge gate, packaging and releases |

## Constraints that cut across v0.1

These hold in every area. Each one keeps a later milestone from needing a rewrite.

- **Hierarchy-ready identity.** Sync keys and Component references must allow a Sheet Instance path prefix, so that v0.5 hierarchy (Component identity = *Sheet Instance path + UUID*) does not break them. In v0.1 the path is always empty. ([milestones](https://github.com/openreda/reda/issues/36))
- **N-layer model.** Layers are an ordered list of objects referenced by UUID, never by index. Every Via stores a layer span, and every Track stores a Line|Arc geometry. v0.1 uses only 2 copper Layers, through Vias and Lines. ([PCB](https://github.com/openreda/reda/issues/8), [model](https://github.com/openreda/reda/issues/9))
- **Room for rules.** Design rules are board-wide values plus Net Class overrides, but the model must leave room for a later rule-query language and for per-Layer and per-area rules. ([checks](https://github.com/openreda/reda/issues/15))
- **Room for more outputs and Variants.** Every Output Job names its PCB Document and reads only that Document, ready for Panels and multi-board Projects. The one exception is the BOM, which also lists Mechanical Components that have no Footprint ([outputs](outputs.md#bom)). Not Fitted is a per-Component flag that Variants will later generalise. ([outputs](https://github.com/openreda/reda/issues/14), [loose ends](https://github.com/openreda/reda/issues/39))
- **No string literals in the UI.** Every user-facing string is looked up by key, even though v0.1 ships only English. ([i18n](https://github.com/openreda/reda/issues/34))
- **Stable ids on UI items.** Menus, toolbars, the Active Bar and every item in them have stable ids and bind only to Commands. Customisation can then come later as an overrides layer. ([customisation](https://github.com/openreda/reda/issues/35))
- **wasm32-clean engine.** Every crate below `reda-control` builds for `wasm32-unknown-unknown`. ([crates](https://github.com/openreda/reda/issues/16))
- **Deterministic engine.** UUIDs and time come only from an injected `IdSource` and `Clock`. ([testing](https://github.com/openreda/reda/issues/29))
- **The engine stays offline.** The only network call in v0.1 is `app.update.check`, and it runs only when the user asks. ([packaging](https://github.com/openreda/reda/issues/30))

## Build order implied by the layering

The crate layering ([ADR-0009](../../adr/0009-one-model-crate-behaviour-crates-above.md)) and the testing approach suggest building bottom-up. Command-level scenario tests can then drive each layer from the start:

1. `reda-geom` and `reda-text`. Then `reda-model`, with transactions and change sets. Then `reda-command`, with the registry and `batch`. Then the `reda-engine` skeleton: Project open/save, canonical JSON and `reda-testkit`. After these, the headless engine runs Commands and round-trips files.
2. The domain crates, each tested through Commands: library elements and generators; schematic connectivity and ERC; PCB connectivity, Ratsnest, Zone fill and DRC; Sync in the engine; outputs.
3. `reda-control` and `reda-cli`, which give MCP and `reda run`.
4. `reda-render`, giving the Scene and the wgpu backend, then `reda-ui` and `reda-app`: the shell and docking first, then each editor and its Tools.
5. The release workflow, which lands with the first `reda-app` that opens a window.

The tickets made from this spec decide the actual slicing. This order only reflects the dependencies.
