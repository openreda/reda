# v0.3 Import part from EasyEDA/LCSC

Source: [How does Import part read EasyEDA/LCSC files?](https://github.com/openreda/reda/issues/455), after [What do EasyEDA/LCSC part downloads contain, and how is the model bound to the footprint?](https://github.com/openreda/reda/issues/454) ([findings](https://github.com/openreda/reda/blob/research/easyeda-part-formats/docs/research/easyeda-part-formats.md)).

This file says what the new `reda-import-easyeda` crate reads and how an EasyEDA/LCSC part becomes a Symbol, a Footprint, a Part and Model Bodies. Import part's inputs, the KiCad path and the attach checks are in [Models](models.md). What a Model Body is and how it is placed is in [Bodies](bodies.md).

The format facts come from EasyEDA's published specifications (Std 1.7.5, Pro V2.2, Pro V3 of 2025-10-21) and from reading other readers for facts only. No code is copied from the GPL or AGPL readers.

## Inputs

- All three EasyEDA families, told apart by **content**, never by extension (`.zip` is used for all three):

  | Family | Files |
  |---|---|
  | **Std** | an editor source `.json` or `.zip`, or an LCSC part bundle (symbol `dataStr` plus `packageDetail`) the user saved |
  | **Pro v2** | `.epro`, `.elibz`, and loose `.esym` / `.efoo` |
  | **Pro v3** | `.epro2`, `.elibz2` |

- **Several devices per file.** A library or project that holds more than one device shows a list (title, LCSC number, footprint name). The user ticks one or more, and each becomes its own Part.
- A project is read only for the devices it holds. A board `COMPONENT`'s overrides of the device's 3D attributes are ignored, because Bodies belong to the Footprint.
- **Loose files** work like the KiCad path: a Std symbol and footprint, or an `.esym` and an `.efoo`, are imported as a pair.
- Pins bind to Pads by number (pin number = pad number, as strings), so no pin-map overrides are needed.

## Units and coordinates

- Std: one unit is 10 mil, in both symbols and footprints. Coordinates are absolute on a canvas, and the origin (`head.x/y`) is subtracted.
- Pro footprints and the 3D transform: 1 mil. Pro symbols: 0.01 inch.
- Y points down in every family and is flipped. Angles are degrees, counter-clockwise positive.

## Part fields

| EasyEDA attribute | REDA |
|---|---|
| `Manufacturer Part` | MPN |
| `Manufacturer` | Manufacturer |
| `Datasheet` | Datasheet |
| `Value`, else `Manufacturer Part` | Value |
| designator prefix (Std `pre`, Pro `Designator`) | the Symbol's designator prefix |
| `Supplier Part` (the C-number) | a free-form field named after `Supplier`, normally `LCSC` |
| `JLCPCB Part Class` (Basic or Extended) | a free-form field of that name |
| any other attribute | a free-form field of the same name |

The 3D attributes (`3D Model`, `3D Model Title`, `3D Model Transform`) are not kept as fields (see [Model](#model)).

## Symbol

- Multi-unit parts (Std `subparts`, one Pro `PART` per unit) become Units.
- Pin types map to REDA's [Pin electrical types](../v0.1/library.md#elements). Std type 4, which readers disagree on, becomes **Passive**.
- Lines, rectangles, arcs, circles, polylines and text map across. Pin features REDA lacks are treated as in [the KiCad path](../v0.1/library.md#import-part): kept visible, or dropped, with a warning.

## Footprint

| In EasyEDA | In REDA |
|---|---|
| Pads on top, bottom or multi-layer | Pads; an oval hole with a length becomes a slot |
| POLYGON Pads | the anchor Pad with a warning, as in the KiCad path, until Polygon Pads arrive |
| paste and mask | each Pad's paste and mask, by the KiCad path's rules |
| silkscreen | the Silkscreen Layer |
| Component Shape and Assembly graphics | the Assembly Layer |
| Document and Mechanical graphics | dropped and counted, because Footprints do not draw on Mechanical Layers ([Mechanical Layers](https://github.com/openreda/reda/issues/435)) |
| other layers | dropped and counted |

- **Courtyard.** EasyEDA has no dependable courtyard layer. The Courtyard is a Rect around the Pads and the Component Shape outline, grown by 0.25 mm (IPC-7351 nominal), with a note in the report.
- **`height`**, in this order, with the source named in the report:
  1. the top of the placed Model Body, when a STEP was supplied;
  2. the `-H<mm>` part of the 3D Model Title, LCSC's naming convention (for example `D0402_L1.0-W0.5-H0.5`);
  3. left unset, so the Stand-in Body is 1 mm high.

## Model

### No STEP in the download

No EasyEDA or LCSC download contains the STEP. Each holds only a model UUID, a title and a placement, and REDA never fetches from EasyEDA ([Models](models.md#what-reda-ships)). The user supplies the file:

- **Auto-match:** Import part looks beside the input file, and inside a zip, for `<3D Model Title>` or `<uuid>` with `.step`, `.stp` or `.stpz`.
- **Model row:** the dialog shows one row per imported part: the model title and either the matched file or "not found" with **Choose file…**. A file picked there is placed like a matched one.
- **No file:** the Footprint gets no Model Body, and the report names the missing model by its title. The Stand-in Body is shown. Re-running Import part with the file is the way to get the EasyEDA placement. A Model added later with **Add Model Body…** lands at zero offset, as any hand-added Model does.
- Nothing about the EasyEDA model (UUID, title, transform) is stored in the Library, as in [Models](models.md#what-is-stored). The Model Body's name is the supplied file's name.
- The attach checks in [Models](models.md#checks-when-a-file-is-attached) apply.

### Placement

EasyEDA places a model with `OFFSET × rotateZXY(rotZ, rotX, rotY) × scale(size / extent) × translate(−cx, −cy, −bz)`, where `cx`, `cy` are the model's X/Y midpoints, `bz` its lowest Z and `extent` its size per axis. A Model Body has offset and rotation only ([Bodies](bodies.md#placement)), so the importer:

1. reads the supplied STEP's bounding box through `reda-step`;
2. folds `−(cx, cy, bz)` through the rotation into the offset (when a Model is already stored bottom-centred, this term is zero);
3. converts the ZXY Euler angles to the Model Body's rotation, and flips Y;
4. compares `size / extent` on each axis, skipping an axis whose size is 0 (Pro's "auto-fit height"), and reports any ratio more than 2 % from 1, naming the axis. The scale itself is dropped.

- **Pro:** from the device's `3D Model Transform` (`sizeX, sizeY, sizeZ, rotZ, rotX, rotY, offX, offY, offZ`, in mil).
- **Std:** from the footprint's 3D `SVGNODE`: `c_origin` less the footprint origin for X/Y, `z` for Z, `c_rotation` for the rotations, and `c_width` / `c_height` for the X/Y size. The same procedure applies.

## Dialog, report and provenance

- It is the same Import part Command, dialog, report and choice of Library as the KiCad path. The file picker also accepts the EasyEDA extensions.
- Provenance records the source service as EasyEDA/LCSC with its terms URL. The one-time vendor-terms notice covers it.
- The report lists dropped layers and primitives with counts, anchor-Pad substitutions, the Courtyard note, the `height` source, missing Models by title, and scale mismatches.
- The Library check runs after the import.
- No EasyEDA or LCSC files go into REDA's repo or test fixtures ([ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md)). Tests use files REDA writes itself in each format.

## Out of v0.3

- Fetching parts or Models from EasyEDA's servers, including by LCSC number.
- Taking a part's Model out of a whole-board STEP exported from EasyEDA Pro. It can be added later if Pro's export proves to name parts reliably.
- Importing EasyEDA schematics or boards as Documents.
