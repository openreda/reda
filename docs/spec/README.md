# REDA product spec

REDA is an MIT-licensed electronic design automation (EDA) tool written in Rust with egui. It is one application with one window. In it a user captures a circuit on Schematic Sheets, lays out the PCB Document that realises it, and produces the files a fab needs to make and assemble the board.

This spec gathers the decisions made on the [wayfinder map](https://github.com/openreda/reda/issues/1) so that implementers do not have to read the issue threads. It restates only what an implementer needs. Each section links the ticket or ADR that holds the reasoning. When this spec and a ticket disagree, the ticket's resolution comment wins, and this spec should be fixed. Domain terms (capitalised: Project, Component, Net…) are defined in [`GLOSSARY.md`](../../GLOSSARY.md).

## Contents

| File | What it holds |
|---|---|
| [`modules.md`](modules.md) | Every planned module, the milestones from v0.1 to after 1.0, and how each module fits the one app and the crate layering. |
| [`v0.1/README.md`](v0.1/README.md) | The v0.1 milestone: schematic → 2-layer PCB → Gerber/drill/BOM/CPL. It is specified tightly enough to slice into agent tickets, with one file per area. |
| This file | Product principles, the ADR index, and the open points. |

## Product principles

- **Audience: hobbyists first.** REDA takes the *structure and feel* of a professional integrated suite rather than KiCad's separate windows. There is one window. A Project's Documents open as tabs. Panels are shared and dockable. Menus and toolbars follow the active Document type, and the Properties panel follows the selection. Dense and feature-rich is fine, but the most-used features are built first. Fab-friendly flows (one-click outputs for JLCPCB, PCBWay and others, without being tied to one fab) are first-class.
- **Its own identity.** REDA borrows proven workflow ideas, but it is not a UI clone of KiCad or of any commercial suite, and it never aims for 1:1 parity with another tool.
- **Platforms.** Windows, macOS and Linux desktops come first. A WebAssembly build (maybe a read-only viewer) is a far-future goal, so the engine crates stay free of UI and OS dependencies and build for `wasm32-unknown-unknown`.
- **Every action is a Command** ([ADR-0001](../adr/0001-every-action-is-a-command.md)). The GUI, the `reda` CLI and MCP all run the same schema-described Commands. AI agents drive REDA through MCP and the CLI. REDA has no in-app assistant.
- **Permissive only** ([ADR-0002](../adr/0002-permissive-licensing-and-third-party-data.md)). REDA is MIT. Dependencies are permissive, and no GPL or LGPL code is linked or copied. REDA can read third-party library data that the user supplies, but never ships it.
- **Few dependencies.** REDA uses crates for substantial pieces that are hard to write well (polygon ops, GUI, MCP, file dialogs, fonts) and writes small utilities itself (see `AGENTS.md`).
- **Built by agents.** A solo maintainer builds REDA, with AI agents doing much of the implementation. That is why v0.1 is specified for AFK agents: tests are Command-level scenarios, outputs are checked with goldens, and every change lands through a PR that passes CI (see [Testing](v0.1/quality-and-release.md#testing)).

## Architecture decisions

| ADR | Decision |
|---|---|
| [0001](../adr/0001-every-action-is-a-command.md) | Every action is a schema-described Command. MCP and the CLI drive REDA. There is no in-app AI assistant. |
| [0002](../adr/0002-permissive-licensing-and-third-party-data.md) | Dependencies are permissive only. Third-party library data is supported but never shipped. Bundled fonts may be Hershey, OFL-1.1 or Ubuntu-font-1.0. |
| [0003](../adr/0003-monorepo.md) | Everything lives in one monorepo. Only forks of upstream dependencies live elsewhere. |
| [0004](../adr/0004-connectivity-derived-from-schematic-documents-own-their-data.md) | Schematic Nets are derived from the drawing. Each Document owns its copy of Component data, and the PCB keeps stored Nets. |
| [0005](../adr/0005-canonical-json-one-object-per-line.md) | Documents are canonical JSON, one object per line, with tables sorted by UUID. |
| [0006](../adr/0006-one-way-sync-with-write-through-edits.md) | Sync runs one way, from schematic to PCB. Component edits made on the PCB write through to the schematic. |
| [0007](../adr/0007-generic-mcp-tools-and-always-bridge.md) | MCP offers a few generic tools over the Command registry. The CLI always bridges to a running instance that has the Project open. |
| [0008](../adr/0008-parts-bind-symbols-and-footprints.md) | A Part binds one Symbol to alternate Footprints. Schematic Sheets carry the whole Part. |
| [0009](../adr/0009-one-model-crate-behaviour-crates-above.md) | All persisted data lives in one model crate, with behaviour crates above it. |
| [0010](../adr/0010-canvas-drawn-by-own-wgpu-renderer.md) | Canvases are drawn by REDA's own wgpu renderer inside egui. |
| [0011](../adr/0011-own-docking-layer.md) | REDA writes its own docking layer, and Documents are ordinary tabs. |

## Research behind the decisions

These research tickets hold facts, not decisions. Implementers reach for them when building the piece they inform.

- [KiCad file formats and library licences](https://github.com/openreda/reda/issues/3): needed for `reda-import-kicad`.
- [How other EDA tools model design data](https://github.com/openreda/reda/issues/6)
- [egui/winit forks and companion crates](https://github.com/openreda/reda/issues/2): `rfd`, `muda`, `directories`, `toml_edit` and `egui_kittest`.
- [Rust crates as EDA building blocks](https://github.com/openreda/reda/issues/5): `i_overlay`, `rstar`, `gerber-types`, an Excellon writer of REDA's own, `rmcp` and `schemars`.
- [What fabs require in manufacturing files](https://github.com/openreda/reda/issues/4): Gerber/Excellon dialects and the seed DRC numbers per fab.
- [Vendor part download formats and terms](https://github.com/openreda/reda/issues/19): needed for Import part.
- [Text and fill triangulation crates](https://github.com/openreda/reda/issues/23): `skrifa`, `harfrust`, `i_triangle` and the Hershey font.
- [egui painter vs own wgpu renderer](https://github.com/openreda/reda/issues/18)
- [Packaging, signing and updating](https://github.com/openreda/reda/issues/24)
- [Name and extension availability](https://github.com/openreda/reda/issues/25), then [crate and CLI naming](https://github.com/openreda/reda/issues/31): nothing is published to crates.io, crates are named `reda-*`, the CLI is `reda`, and the domain is `openreda.com`.

## Open points

Two kinds of question remain open. Gaps in v0.1 are open tickets on the map. Implementers should not decide them on their own:

- [How are Projects, Documents and board setup created and edited in v0.1?](https://github.com/openreda/reda/issues/38)
- [Which loose ends between earlier v0.1 decisions need settling?](https://github.com/openreda/reda/issues/39)

Later fog is listed under *Not yet specified* on the [map](https://github.com/openreda/reda/issues/1): font embedding and searchable PDF text, extra checks and a rule language, performance tracking, extra fuzz targets, UI customisation, and translations.

**Out of scope** for REDA: importing proprietary commercial EDA formats, signal-integrity analysis, enterprise library vaults and PLM, real-time multi-user editing, and an in-app AI chat assistant.
