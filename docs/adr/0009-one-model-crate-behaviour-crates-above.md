# All persisted data lives in one model crate; behaviour crates sit above it

Every persisted type (Project, Schematic Sheet, PCB Document, Library elements, UUIDs) plus transactions and change sets lives in a single `reda-model` crate that holds data and the transaction mechanism only. Behaviour (connectivity, ERC, DRC, generators, outputs, rendering, commands) lives in per-domain crates above it (`reda-schematic`, `reda-pcb`, `reda-library`, `reda-output`, `reda-render`), which never depend on each other. `reda-engine` alone does cross-Document work (Project open/save, file format, Sync, Re-annotate) and assembles the command registry. The UI reads `&Project` directly but changes it only by running commands.

## Considered Options

- **Per-domain crates owning their own data types** (a schematic crate with sheet types, a PCB crate with board types, a Project crate on top): rejected. Commands take the whole Project as context and Sync needs both sides at once, so `Project` would have to sit above every domain crate and anything working across Documents would have to go through it.
- **Command machinery inside the model crate**: rejected so the model stays plain data; it lives in `reda-command` instead.

## Consequences

- Allowed dependency directions are an allow-list checked by a workspace test over `cargo metadata`; every crate below `reda-control` must build for `wasm32-unknown-unknown` in CI.
- A change to any persisted type rebuilds every behaviour crate. That is accepted as the price of one place for the data.
