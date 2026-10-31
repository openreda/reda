# KiCad file formats and library licences

Research for issue #3. Question: what do KiCad's current file formats look like, at the level needed to judge an importer's effort? What are the exact licences of the official KiCad symbol, footprint and 3D libraries? And what does ADR-0002 ("read user-obtained third-party library data, never ship it") permit and forbid in practice?

Sources were checked on 2026-10-31. KiCad 10 is the current major release (announced March 2026). This is a factual reading of licence texts, not legal advice. Points that a lawyer would need to settle are flagged **Uncertain**.

## Answer in brief

- **Formats.** Every modern KiCad design and library file is a Specctra-DSN-style S-expression text file, except the project file, which is JSON. The official format docs are thin. Most of the shared token grammar sits on one page, and the per-file pages contain gaps marked `???` or "DOCUMENT ME". An importer would therefore have to rely on real files from each KiCad version as its test corpus. The format changes with every major release. Each file carries a `version` token in YYYYMMDD form, which an importer can branch on.
- **Licences.** The official symbol, footprint and 3D-model libraries are all licensed **CC-BY-SA 4.0 with a design exception**. Designs and generated outputs that use the data are released from the licence's Article 3 conditions (attribution and ShareAlike). Redistributing the libraries, whole or in part, modified or not, is **not** covered by the exception. Anyone who redistributes them must keep the same licence, the attribution and the licence documents.
- **What ADR-0002 permits.** REDA can read libraries the user installed, show and place their parts, keep converted copies on the user's machine, and import KiCad projects that embed library data.
- **What ADR-0002 forbids.** REDA must not bundle KiCad library files, or converted copies of them, in its repo, binaries, installers or test fixtures. It must also not serve or share them from infrastructure REDA controls. If REDA did either, those files would have to be distributed under CC-BY-SA 4.0 with attribution. The ADR is right to rule this out.

## 1. File formats

### Common S-expression grammar

Source: KiCad dev docs, "S-Expression Format", https://dev-docs.kicad.org/en/file-formats/sexpr-intro/index.html

- The syntax is based on the Specctra DSN format. "Human readability is a design goal."
- "All values are given in millimeters." Exponential notation is never used. The internal unit is 1 nm. Board and footprint files are written to 6 decimal places, and schematic and symbol files to 4.
- Shared tokens are defined once and reused by every file type: `at` (X, Y and an optional angle; symbol text angles are in tenths of a degree, all other angles in degrees), `pts`/`xy`, `stroke`, `effects` (font and justification), `property` (key/value pairs; symbols have the mandatory keys Reference, Value, Footprint and Datasheet, and `ki_`-prefixed keys are reserved), and `layer`.
- Layer names in files are canonical names such as `F.Cu`, `B.SilkS` and `Edge.Cuts`. Wildcards are allowed, e.g. `*.Cu`. User-defined layer names are display-only.
- `uuid` is a v4 UUID. Older files that used `tstamp` timestamps are re-encoded as UUIDs. Some board objects still use `tstamp` as the token name.
- Footprints reference 3D models with `(model "<path>" (offset|at (xyz ..)) (scale (xyz ..)) (rotate (xyz ..)))`. The path usually starts with a path variable such as `${KICAD10_3DMODEL_DIR}`.
- Each token's history is documented inline, e.g. "Prior to version 6, the footprint token was referred to as module." An importer for old files has to accept both spellings.

### Per-file summary

| File | Ext. | Root token | Notes for an importer | Source |
|---|---|---|---|---|
| Project | `.kicad_pro` | none (JSON) | Settings shared by the schematic and the PCB (net classes, text variables, board setup defaults, etc.). It is a JSON document: in KiCad's source, `PROJECT_FILE` derives from `JSON_SETTINGS`. A per-user `.kicad_prl` file holds UI state and need not be imported. A legacy `.pro` file is converted on open. | docs.kicad.org 10.0 "KiCad" manual; `include/project/project_file.h` in the KiCad source |
| Schematic | `.kicad_sch` | `kicad_sch` (`version`, `generator`, `uuid`) | Contains `lib_symbols`, an **embedded copy of every library symbol the sheet uses**, so a schematic opens without its libraries. Also wires, buses, bus entries, junctions, no-connects, local, global and hierarchical labels, placed `symbol` instances with an `instances` block keyed by project and sheet path, and `sheet` objects for hierarchy. Instance paths are chains of UUIDs, starting at the root schematic's UUID. Covers KiCad 6.0 and later. The docs contain known errors and gaps (e.g. a pin attribute documented as `???`). | https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/index.html |
| Board | `.kicad_pcb` | `kicad_pcb` (`version`, `generator`) | Sections: `general` (thickness), `paper`, `layers` (ordinal, canonical name, type), `setup` with an optional `stackup`, `property`, `net` (ordinal plus name), then footprints, graphics, images, `segment`/`arc`/`via` tracks, `zone`, and `group`. Placed footprints are **stored in full inside the board**, including pads, graphics and model references. The docs show this only in an old example, so it is inferred, but it is observable in any real board file. Net classes moved to the separate design-rules file. Section order does not matter except that the header comes first. Several attributes are marked "DOCUMENT ME", and the example uses outdated version-3 syntax. | https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/index.html |
| Symbol library | `.kicad_sym` | `kicad_symbol_lib` (`version`, `generator`) | One file holds zero or more `symbol` definitions. Their contents follow the shared symbol grammar on the intro page: units and body styles as nested sub-symbols, pins, properties, and `extends` for derived symbols. New in KiCad 10: **"unpacked" libraries**, a directory with one `.kicad_sym` per symbol. The library team says packagers will likely pack the libraries back into one file per library. KiCad 10 also adds native pin stacks. | https://dev-docs.kicad.org/en/file-formats/sexpr-symbol-lib/index.html; docs.kicad.org 10.0 manual; KiCad library team post, https://forum.kicad.info/t/kicad-libraries-news/66090 |
| Footprint library | directory (conventionally `*.pretty`) of `.kicad_mod` | `footprint "<name>"` (`version`, `generator`) | One footprint per file. Contents follow the shared footprint, pad and graphics grammar on the intro page. | https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/index.html |
| 3D models | `.step`/`.stp`, `.wrl` | n/a | Not a KiCad format: footprints refer to them by path. KiCad 10 ships **only STEP** files for the official 3D library. Older libraries also shipped VRML (`.wrl`), and older footprints may still reference `.wrl` paths. An importer needs path-variable expansion and a fallback from `.wrl` to `.step`. | Elektor, "KiCad 10 release" (2026-03-30), https://www.elektormagazine.com/news/kicad-10-release |
| Library tables | `sym-lib-table`, `fp-lib-table` (also `design-block-lib-table`) | `sym_lib_table` / `fp_lib_table` | One global table in the user config directory and an optional per-project table. They map library nicknames to URIs, which usually contain path variables. Symbol references in designs take the form `Nickname:Name`, so an importer must resolve them through these tables. The KiCad manual describes the role of these tables but not their grammar. The root-token names here come from observing real files. | docs.kicad.org 10.0 "KiCad" manual |
| Worksheet | `.kicad_wks` | `kicad_wks` | Title-block and page-border template. Low priority. | https://dev-docs.kicad.org/en/file-formats/sexpr-worksheet/index.html |
| Legacy | `.sch`, `.lib`, `.dcm`, `.mod`, old `.kicad_pcb` | various | Formats from before 6.0 are documented separately. Out of scope unless old projects become a requirement. | https://dev-docs.kicad.org/en/file-formats/ |

### Versioning and compatibility

- KiCad "can open files created in earlier versions, but can only write files in the latest formats". Files written by a newer KiCad cannot be opened by older versions (docs.kicad.org 10.0 manual). The KiCad 10 docs say each major release almost always changes the file formats.
- Each file's `version` (YYYYMMDD) marks the format revision, so an importer can gate features on it. The docs ask third-party tools not to write KiCad's own `generator` names (`eeschema`, `pcbnew`).
- KiCad 10 opens older symbol libraries read-only until they are migrated.

### Effort assessment

- **Parsing is cheap.** One generic S-expression reader covers every file except the JSON project file.
- **Semantic mapping is the real cost.** It covers layer and stackup mapping, pad shapes and stacks, zones and fills, symbol units and body styles with `extends`, hierarchical instance paths, embedded `lib_symbols` and footprints versus library references, and path-variable and library-table resolution.
- **The specification is incomplete.** Expect to work from observed behaviour, using a corpus of real files from KiCad 6 through 10. **Uncertain:** copying text from KiCad's dev docs into REDA could carry over the docs' own licence. The user manual is GPL-3.0-or-later or CC-BY-3.0-or-later; no licence statement was checked for dev-docs. REDA should cite the dev-docs rather than copy them. Writing a reader for a file format is generally not considered copying the code that writes it, but ADR-0002 already forbids reading KiCad's GPL source for implementation, so the docs and sample files are the only safe references.

## 2. Library licences

### Licence terms

The official library repositories at https://gitlab.com/kicad/libraries carry identical `LICENSE.md` files:

- `kicad-symbols`: https://gitlab.com/kicad/libraries/kicad-symbols/-/blob/master/LICENSE.md
- `kicad-footprints`: https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/master/LICENSE.md
- `kicad-packages3D`: https://gitlab.com/kicad/libraries/kicad-packages3D/-/blob/master/LICENSE.md

The canonical statement is at https://www.kicad.org/libraries/license/.

- **Licence:** Creative Commons **CC-BY-SA 4.0** (https://creativecommons.org/licenses/by-sa/4.0/legalcode), with one exception.
- **The exception, verbatim:** "To the extent that the creation of electronic designs that use 'Licensed Material' can be considered to be 'Adapted Material', then the copyright holder waives article 3 of the license with respect to these designs and any generated files which use data provided as part of the 'Licensed Material'." Article 3 holds the attribution and ShareAlike conditions.
- **Stated intent:** "free use of library data for commercial, closed, and non-commercial projects without restriction". KiCad does not want to force attribution *within* designs.
- **Redistribution is not covered by the exception:** "Redistributed library collections must be shared under the same license agreement." Redistributors must "retain attribution information", "including the license documents which are distributed with the library files". This applies to modified collections and partial collections too.
- **Warranty:** none, express or implied.
- KiCad's licences page (https://www.kicad.org/about/licenses/) confirms the libraries are CC-BY-SA 4.0. KiCad's source code is GPL-3.0-or-later, and its documentation is GPL-3.0-or-later or CC-BY-3.0-or-later.
- **Uncertain:** the templates repo (`kicad-templates`) and the separate 3D-model *generator* scripts were not checked. They may use different licences. Check each repo's own LICENSE file before relying on it.

### What this means for REDA under ADR-0002

The licence grants everyone the right to reproduce the material and to make Adapted Material (CC-BY-SA 4.0, §2(a)(1)). The attribution and ShareAlike conditions in §3 apply only when someone **Shares** the material, meaning makes it available to the public. Private use and private adaptation trigger neither condition.

**Permitted** (REDA does not distribute anything):

1. **Reading libraries the user installed.** This covers KiCad's own install, a library repo the user cloned, or a path the user configured. REDA can parse, display and place parts from them.
2. **Caching converted copies locally.** A converted symbol or footprint is Adapted Material. Making and keeping it on the user's own machine is within the §2 grant, and ShareAlike does not apply because nothing is Shared. The cache should keep a record of where each item came from and its licence. That way, if the user later shares the cache, the attribution and licence information go with it, and that obligation is the user's.
3. **Importing KiCad projects.** Schematics embed library symbols in `lib_symbols`, and boards embed full footprints. Under the exception, these designs and their generated outputs are free of Article 3 obligations, whether REDA or the user produced them. REDA can import and convert them, and the user can do what they like with the result.
4. **Designs the user makes in REDA with KiCad-derived parts, and their Gerbers, BOMs and other outputs.** These fall within the design exception as written. **Uncertain:** the exception speaks of "electronic designs that use" the material. It is very likely, but not tested, that a design made in another tool from converted KiCad data counts. The wording does not name a tool.
5. **Pointing users at the official repos**, e.g. a "get KiCad libraries" link or instructions. **Uncertain:** whether a built-in feature that downloads directly from KiCad's servers on the user's request counts as REDA redistributing. It probably does not, because the files come from the licensor to the user, but this has not been checked.

**Forbidden, or allowed only under CC-BY-SA 4.0 with attribution, which ADR-0002 rules out:**

1. Bundling KiCad library files, or REDA-converted versions of them, in REDA's repo, release binaries, installers, packages or default "starter" libraries.
2. Committing KiCad library files as **test fixtures**. That is redistribution through a public repo, and the fixture files would have to be CC-BY-SA. Fixtures should be written by the REDA project, or downloaded at test time and never committed.
3. Hosting or syncing converted libraries from infrastructure REDA runs, e.g. a shared parts cloud. Doing so would make REDA the redistributor.
4. Removing the attribution or licence records from converted copies that REDA exports for users to share.
5. Copying KiCad's GPL code, including its parsers. This is the separate licence issue ADR-0002 already covers.

### Note on ADR-0002's wording

ADR-0002 says these licences "would bind REDA's distribution". To be precise, CC-BY-SA's ShareAlike would bind **the redistributed library files**, not REDA's MIT code. Bundling them would not make REDA itself CC-BY-SA, but it would make REDA's releases mixed-licence and add attribution obligations. The decision stands. If the ADR is ever revised, the wording could be tightened to say this.
