# How existing EDA tools model design data

Research for issue #6. Compares, **at the level of concepts only**, how KiCad, Horizon EDA, LibrePCB and a leading commercial single-window EDA suite model their design data. KiCad, Horizon EDA and LibrePCB are GPL: per ADR-0002 nothing here is, or may become, copied code. Source files were read only to confirm what a concept is, never to reuse its implementation.

Terms in **bold** are REDA's GLOSSARY.md terms. Each tool's own word is given in quotes where it differs.

Sources were checked in October 2026 against the versions documented then (KiCad 9 docs, Horizon EDA `latest` docs and `master` source, LibrePCB `master` source and developer docs). The commercial suite's claims come from its official online documentation. Those pages are deliberately not linked here (see "Sources"), so treat that column as lower-confidence.

## Summary

| Concern | KiCad | Horizon EDA | LibrePCB | Commercial suite |
|---|---|---|---|---|
| Project / Document | Project = `.kicad_pro` + one root sheet + **one** PCB, same basename | Project file pointing to a blocks file, a board file and a project pool | Project directory: separate `circuit`, `schematics`, `boards` files, more than one board allowed | "Project of documents": a project file lists any number of Schematic Sheets and PCB Documents |
| Where connectivity lives | Derived from schematic graphics (wires + labels); copied to PCB by an update step | Explicit netlist ("block"); schematic is a view of it | Explicit netlist ("circuit"); schematics and boards are views of it | Derived from schematic graphics; copied to PCB by an engineering change order |
| Net identity | Name derived from label priority; PCB uses an index plus name | Net is a first-class object with a UUID; name optional | Net signal has a UUID, a name and an "auto name" flag | Name derived from labels and ports; scope chosen per project |
| Library decomposition | Symbol, Footprint (Footprint chosen per placed Symbol through a field) | Unit → Entity (gates) → Part (= Entity + Package + pin-pad map) | Symbol → Component (signals, symbol variants) → Device (= Component + Package + pad map); Package holds several footprints | Component bundles Symbol parts and linked footprint models, each with its own pin-pad map |
| Schematic↔PCB identity | Footprint stores the sheet path + Symbol UUID | Board package points at the Component UUID in the shared netlist | Board device is keyed by the component-instance UUID | Unique ID given at placement, copied to the PCB on first transfer |
| Units | 1 nm integer internally (32-bit); mm with 4–6 decimals in files | 1 nm, `int64` | 1 nm, `int64`; mm text in files | Binary internal units; mil/mm in the UI |
| On-disk | S-expression text, one file per sheet / board | JSON, one file per pool item; SQLite index cache | S-expression text, canonical; one file per element | Binary compound-document by default, optional ASCII; text project file |

**Adopt:** a netlist that is stored explicitly as its own model; stable UUIDs for every object and library element; integer-nanometre (`i64`) coordinates; one canonical, diff-friendly text file per Document and per library element; a Symbol/Pin layer kept separate from the Footprint/pad layer, joined by an explicit pin-pad map; Projects that hold a copy of the library parts they use.
**Avoid:** connectivity that exists only as wire geometry and has to be re-derived; matching Components across Documents by reference designator; binary container formats; net references stored as positional ordinals; a hard limit of one PCB Document per Project.

## 1. Project vs Document structure

**KiCad.** A Project is a `.kicad_pro` file (settings shared by schematic and PCB), a `.kicad_prl` file of per-user local state, a root `.kicad_sch`, and a `.kicad_pcb`. All of them "have the same name", and "a project can only contain a single board". Each Schematic Sheet in a hierarchy is "its own `.kicad_sch` file and is itself a complete KiCad schematic". Library access goes through a global and an optional project-local `sym-lib-table` / `fp-lib-table`. [KiCad manual, Eeschema manual]
- Good: shared settings and per-user state are in separate files, so view state does not pollute diffs.
- Bad: one board per Project. Projects are tied to each other by file basename.

**Horizon EDA.** The project file records the filenames of a *blocks* file (the netlist plus its schematic), a *board* file, a *planes* file (computed copper fill, kept apart from the design) and a *project pool* directory [src/project/project.hpp]. Since 2.0 "each project is accompanied by a project pool": a pool item is copied into it on first use and "is kept there until explicitly updated" [Project pool docs].
- Good: generated data (plane fills) lives outside the design file. The Project carries its own copies of its parts.

**LibrePCB.** A project directory holds `project/metadata.lp`, `project/settings.lp`, `circuit/circuit.lp`, `circuit/erc.lp`, `schematics/schematics.lp`, `boards/boards.lp`, and per-user `settings.user.lp` files, with one sub-directory per Schematic Sheet and per board [projectloader.cpp]. "A LibrePCB project is 100% self-contained — enforced by the tool" [file-format page].
- Good: the circuit (logical model) is a file of its own, separate from every view of it. Several boards can share one circuit. User settings are kept apart.

**Commercial suite.** A project is a text file that lists its member documents: any number of Schematic Sheets, one or more PCB Documents, output jobs and so on. Each opens as a tab. This is the model REDA's glossary already follows ("Document … opened as a tab").
- Good: the Project is a flexible container. Bad: a Document file can belong to several projects, or exist with no project at all, which complicates identity.

## 2. Connectivity: wires, labels, ports, power, Net derivation

**KiCad.** Connectivity is *derived* from schematic geometry: wires, junctions and labels. Local labels act within one sheet, global labels across all sheets, and hierarchical labels pair with sheet pins on the parent sheet. A sheet pin "must have an identically named hierarchical label" in the child [schematic format]. Power symbols join everything that shares their value, on every sheet. Net names are resolved by priority (global label > power symbol > local label > hierarchical label > sheet pin), alphabetically within a type. Unlabelled nets get auto-generated names from their pins [Eeschema manual]. The board file has its own `nets` table of `(net ORDINAL "NAME")`, and tracks and zones refer to "the net ordinal number" [PCB format].
- Bad: a Net exists only as the result of a geometric solve. Renaming a label silently re-wires the design. PCB net references are positional indices.

**Horizon EDA.** A *block* "stores Components (instances of Entities), Buses and Nets" [src/block/block.hpp]. Each Net is an object with a UUID and an optional name, with flags such as `is_power`, `is_port` and `is_nc`. Each Component holds a map from *(gate UUID, pin UUID)* to its Net [src/block/net.hpp, component.hpp]. The schematic draws this netlist: wires and labels are edits to it, not its source. Hierarchy works by block instances, with port nets.
- Good: a Net has identity of its own, so renaming it changes no connections. Connectivity questions (ERC, the PCB) are answered without solving geometry.

**LibrePCB.** The *circuit* holds net signals (UUID, name, `hasAutoName`), net classes, buses and component instances whose *component-signal instances* join net signals [circuit/netsignal.h, componentinstance.h]. Schematics and boards refer to these by UUID.
- Good: the same separation as Horizon. Auto names and user names are told apart explicitly, so a name the user forced survives edits.

**Commercial suite.** Connectivity is derived from wires, net labels, ports, sheet entries and power ports. A project-wide **net identifier scope** decides how far names reach: *Flat* (ports join across all sheets), *Hierarchical* (ports join only through matching sheet entries), *Global* (ports and labels join everywhere), *Strict Hierarchical* (hierarchical, with power ports local to each sheet), and *Automatic*, which picks one from what the sheets contain.
- Bad: the same drawing means a different circuit depending on a project setting. Good: it shows that scope rules are a real design axis. REDA should choose one rule and fix it, rather than make it a toggle.

## 3. Component / Symbol / Footprint / Library

**KiCad.** A Library holds Symbols. A Symbol may contain several units, numbered with unit 0 "common to all units", and at most two body styles (De Morgan) [S-expr intro]. Footprints live in separate libraries. A placed Symbol names its Footprint in a *Footprint* field, narrowed by footprint filters. Pins and pads are matched "by pin/pad number" [Pcbnew manual]. Pins can carry alternate functions (name, electrical type, graphic) [Eeschema manual]. Each schematic embeds a `lib_symbols` cache of every Symbol it uses [schematic format].
- Good: simple. Embedding the Symbols keeps a Document readable without its Libraries. Bad: there is no "orderable part" object, so pin-pad agreement rests on number strings matching by convention, and the Footprint choice is free text.

**Horizon EDA.** Parts are built from layers: *Unit* (logical pins with direction, plus optional alternate names) → *Entity* (the netlist view, made of *gates*, each pointing at one Unit) → *Part* (Entity + *Package*, plus the pin-pad map). Symbols only display a Unit's pins: "a symbol just displays the pins from its unit and doesn't define these". A Part can inherit from another Part, for families such as resistor values [pool-elements]. A board package can override its Component's Package with an `alternate_package` [src/board/board_package.hpp].
- Good: Units are reused (a dual and a quad gate share one Unit). The pin-pad map belongs to the orderable Part. Alternate footprints are modelled.

**LibrePCB.** *Symbol* → *Component* ("generic (abstract) electrical device": component signals, plus symbol variants for IEC or IEEE styles, each mapping Symbol Pins to signals) → *Device* ("concrete (i.e. purchaseable)": Component + Package, plus the pad-to-signal map). A *Package* holds several footprints. Every element's "interface … must never change": changing pins means a new UUID [library docs, library-concept page]. A placed component instance records `lib_component` and `lib_variant`, and each board's device records `lib_device` and `lib_footprint` [componentinstance.cpp, bi_device.cpp]. So the physical part can differ from board to board, and assembly variants pick among Devices.
- Good: the clearest split between abstract and concrete parts. Library interfaces stay stable. Footprint and part choice are separated cleanly.

**Commercial suite.** A Library Component bundles one or more schematic *parts* (multi-part, with a hidden "Part Zero" for pins shared by all parts, such as power) and any number of linked footprint models. Each model has its own pin-to-pad mapping column in the pin editor. Libraries come as file libraries, compiled "integrated" libraries, database-backed libraries, or server-managed components with lifecycle states.
- Good: alternate footprints and per-model pin-pad maps sit inside one Component. Bad: several overlapping library mechanisms have built up over time.

## 4. Object identity across Schematic Sheet and PCB Document

- **KiCad:** every object has a v4 UUID. A placed Symbol has `instances` keyed by a sheet-UUID `path`, each with its own `reference` and `unit`, so a sheet reused in a hierarchy gives one Component per path. A board footprint's `path` is "the hierarchical path of the schematic symbol linked to the footprint". "Update PCB from Schematic" moves changes across, and footprints with no symbol are deleted unless locked. The PCB can edit nets, but that "is not a typical workflow" [format docs, Pcbnew manual].
- **Horizon EDA:** schematic and board both point into the same block's Component objects. A Component "is identified by its UUID, not by its reference designator" [component.hpp], so no linking step can go wrong.
- **LibrePCB:** a board device is serialised under its component-instance UUID [bi_device.cpp]. Identity lives in the circuit, as in Horizon.
- **Commercial suite:** a Unique ID is given to the schematic Component at placement and copied to the PCB Component on the first transfer. Later syncs run as an **engineering change order** (a reviewable list of changes) in either direction. When IDs fail to match, the tool offers to match by designator. Its documentation stresses that ID matching lets designators drift (re-annotation) without the two sides losing sync.

For REDA: give each **Component** its identity in the Project model, not in any one Document. The engineering change order idea (a reviewable, atomic list of changes) fits ADR-0001 well if it is built from commands.

## 5. Units and coordinate precision

- KiCad: "The internal measurement resolution of all objects in KiCad is 1 nanometer", stored in 32-bit integers (about ±2.1 m, roughly a 4 m board) [Pcbnew manual]. Files hold mm with 6 decimals (board, footprint) or 4 (schematic, symbol); "any precision beyond … will be truncated" [S-expr intro].
- Horizon EDA: `Coordi` is `int64_t` nm for everything except on-screen geometry [src/common/common.hpp].
- LibrePCB: `Length` is an `int64_t` count of nanometres [core/types/length.h]; files hold mm text.
- Commercial suite: an internal integer unit, with mil or mm chosen in the UI.

For REDA: use `i64` nanometres everywhere, and write values as exact decimal mm. Integer types avoid float drift, and 64 bits remove KiCad's size limit.

## 6. On-disk format

- KiCad: lowercase S-expressions, "Human readability is a design goal", one file per Schematic Sheet, board and library item (`.kicad_sym` holds a whole Library). Format version is a `YYYYMMDD` date [S-expr intro]. The PCB's ordinal net table makes renumbering diffs noisy.
- Horizon EDA: each pool element is "a single json file". Pool metadata "is aggregated into a sqlite database" used only as a rebuildable search index [pool-why].
- LibrePCB: S-expressions, "canonical, i.e. for any imaginable data set there exists only one exact representation", line breaks controlled for small diffs. Libraries are directories with one UUID-named folder per element. The format is frozen between major releases [file-format page, library docs].
- Commercial suite: Schematic Sheet and PCB Document files are binary compound-document containers by default, with an optional ASCII save. The project file is plain text. Binary Documents cannot be diffed or merged in VCS, and third parties have had to reverse-engineer the format.

For REDA: use a canonical text serialisation (stable key order, stable float-free numbers, one object per line where possible). Write one file per Document and per library element. Keep caches and generated data out of the design files, in a form that can be rebuilt.

## Open questions this raises (for later tickets)

- Does a Schematic Sheet *own* connectivity (the KiCad and commercial way) or *view* a Project-level netlist (the Horizon and LibrePCB way)? The evidence favours a Project-level netlist. Issues #9 and #10 should decide.
- Should REDA add glossary terms for the abstract/concrete split ("Component" in LibrePCB or "Entity" in Horizon vs "Device" or "Part"), and for "unit/gate" in multi-unit parts?

## Sources

KiCad (GPL; read for concepts only):
- File format intro: https://dev-docs.kicad.org/en/file-formats/sexpr-intro/index.html
- Schematic format: https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/index.html
- Board format: https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/index.html
- KiCad 9 manual: https://docs.kicad.org/9.0/en/kicad/kicad.html
- Schematic Editor manual: https://docs.kicad.org/9.0/en/eeschema/eeschema.html
- PCB Editor manual: https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html

Horizon EDA (GPL; read for concepts only):
- Why a pool: https://docs.horizon-eda.org/en/latest/pool-why.html
- Elements of a Part: https://docs.horizon-eda.org/en/latest/pool-elements.html
- Project pool: https://docs.horizon-eda.org/en/latest/project-pool.html
- Source (`master`): https://github.com/horizon-eda/horizon — `src/block/{block,component,net}.hpp`, `src/board/board_package.hpp`, `src/project/project.hpp`, `src/common/common.hpp`

LibrePCB (GPL; read for concepts only):
- Library concept: https://librepcb.org/features/library-concept/
- File format: https://librepcb.org/features/file-format/
- Library specification: https://developers.librepcb.org/df/d4f/doc_library.html
- Source (`master`): https://github.com/LibrePCB/LibrePCB — `libs/librepcb/core/project/{projectloader.cpp,circuit/netsignal.h,circuit/componentinstance.{h,cpp},board/items/bi_device.cpp}`, `libs/librepcb/core/types/length.h`

Commercial suite: the vendor's official online documentation (pages on project options and net identifier scope, the component pin editor, and schematic-to-PCB linking and component-link synchronisation). Not linked, by project policy.
