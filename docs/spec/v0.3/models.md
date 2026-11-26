# v0.3 Models: shipped, attached and imported

Source: [Which models does REDA ship, and how does the user bring in others?](https://github.com/openreda/reda/issues/432), after [Where do 3D models for electronic parts come from, and under what licences?](https://github.com/openreda/reda/issues/426) ([findings](https://github.com/openreda/reda/blob/research/3d-model-sources/docs/research/3d-model-sources.md)).

This file says which Models REDA ships and how a user brings in a Model they downloaded: by hand in the Footprint editor, or along with a part through Import part. What a Model and a Model Body are, how a Model Body is placed and how Models are stored is in [Bodies](bodies.md). How the new EasyEDA/LCSC reader maps a part is in [EasyEDA import](easyeda-import.md). Commands and file-format details are in [Commands and file formats](commands.md).

## What REDA ships

- The Starter Library ships **no Model files**. No third-party source licenses its Models in a way [ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md) allows REDA to bundle.
- Every Starter Library Footprint has generated Extruded Bodies instead ([Bodies](bodies.md#generated-bodies)).
- REDA does not download parts or Models on the user's behalf. The user downloads them from whatever source they use (LCSC/EasyEDA, DigiKey, Mouser, Farnell, SnapMagic, Ultra Librarian, SamacSys, a manufacturer, KiCad's library), for their own use, and REDA imports the files.

## Formats

- STEP only: `.step`, `.stp` and `.stpz` ([STEP](step.md)). Mesh formats are not accepted.

## What is stored

- A Model is stored **as given**: the original STEP text, gzip-compressed as `models/<hash>.stpz`, with `<hash>` over that text ([Bodies](bodies.md#models-on-disk)). A `.stpz` input is unpacked before hashing.
- It is not converted to millimetres or to another protocol when it is attached. The conversion happens when a STEP export is written ([STEP](step.md)).
- A Model Body keeps the **original file name** for display. No source URL, licence or other provenance is stored, and attaching asks nothing about licences. Using a downloaded Model in the user's own design is the intended use at every source.

## Checks when a file is attached

| The file | Result |
|---|---|
| is not STEP Part 21 at all | refused, with the reason |
| parses but nothing in it can be drawn | accepted; `model_unreadable` reports it and the Stand-in Body is shown |
| is larger than 20 MB uncompressed | accepted, with a warning |

There is no hard size limit.

## Attaching a Model by hand

In the Footprint editor:

- **Add Model Body…** in the Bodies list opens a file picker.
- Dropping a `.step`, `.stp` or `.stpz` file on the canvas or on the Bodies list does the same.
- The new Model Body lands at **zero offset and zero rotation**: the file's origin on the Footprint origin. No aid runs on its own, because most vendor Models already share the footprint's origin. **Seat on board** and **Centre on pads** ([Bodies](bodies.md#in-the-footprint-editor)) are one click away.
- **Replace Model…** swaps the file and keeps the Body's placement and Material.
- **Save Model As…** writes the embedded Model back out as `.step` or `.stpz`, so an embedded Model never traps the user's file.

## Import part

Import part stops ignoring 3D models. It brings each model along as a Model Body.

### Inputs

- As in v0.1, a `.kicad_sym` and a `.kicad_mod`.
- **New:** a vendor **zip**, or a **folder**, as downloaded from SnapMagic, Ultra Librarian or SamacSys (which is also what DigiKey, Mouser and Farnell hand out). Import part finds the KiCad symbol and footprint inside. The `zip` crate is already in the workspace.
- **New:** EasyEDA/LCSC downloads, read natively by a new **`reda-import-easyeda`** crate, beside `reda-import-kicad` and `reda-import-dxf`. No `easyeda2kicad` step is needed. Which formats it reads and how they map is in [EasyEDA import](easyeda-import.md). It shares Import part's dialog, report and choice of Library.

### Finding a KiCad footprint's model

- Each `model` entry of the footprint becomes a Model Body, with its offset and rotation. A scale other than 1 is applied once on import, or else reported ([Bodies](bodies.md#placement)).
- The file is looked for in this order:
  1. the path as written, if absolute, with KiCad path variables resolved (below);
  2. the path relative to the `.kicad_mod`;
  3. a file of the same name anywhere in the zip or folder;
  4. a sibling `*.3dshapes` folder.
- A `.wrl` reference is swapped for a `.step` or `.stp` of the same name, following KiCad's own convention.
- A STEP file in the zip or folder that no footprint refers to is attached at zero offset, with a note in the report. Ultra Librarian's Models share the footprint's origin and rotation.
- A model that cannot be found, or is refused by the checks above, is listed in the report. That Footprint gets no Model Body for it, and shows the Stand-in Body if it has no other Body.

### KiCad 3D models folder

- A Preference holds the folder of the user's KiCad 3D model library. It is filled in by auto-detection from KiCad's standard install paths, and the user can change it.
- Import part uses it only to resolve `${KICAD6_3DMODEL_DIR}` through `${KICAD10_3DMODEL_DIR}` and `${KISYS3DMOD}`. The Model is embedded on import, so the path is never stored in a Library or Project.
- REDA does not download KiCad's library.

## Out of v0.3

- In-app part or Model search and fetching. Supplier part search stays in v0.6 ([modules](../modules.md#libraries--import-v06)), with deep links only.
- Matching Models to a whole Library's Footprints from a folder by name. It belongs with full KiCad library import in v0.6.
- Per-Component Model overrides on the PCB. Bodies belong to the Footprint ([Bodies](bodies.md)).
