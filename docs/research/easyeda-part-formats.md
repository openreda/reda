# EasyEDA / LCSC part downloads: formats and how the 3D model binds to the footprint

Research for issue #454. It feeds #455 ("How does Import part read EasyEDA/LCSC files?") and follows the decision in #432: a new `reda-import-easyeda` crate reads EasyEDA/LCSC part data natively (no easyeda2kicad step) and brings the STEP model along as a Model Body placed on the Footprint by an offset and three rotations, with no scale.

Questions: what can a user download for a part from EasyEDA / LCSC, and what does each file hold? For each format: structure and versioning, how the symbol, footprint and pins are described, units and coordinates, how the 3D model is referenced and placed, and whether the STEP is inside the download. Which formats are documented, and which open-source readers exist? Also: what do Farnell, element14 and Newark hand out as a part's CAD download?

Sources were checked on 2026-11-25. Code was read at these commits:

- KiCad `master` at `f445174c` (2026-11-25): `pcbnew/pcb_io/easyeda*`, `eeschema/sch_io/easyeda*`, `common/io/easyeda*` and QA data under `qa/data/*/plugins/easyeda*`.
- easyeda2kicad.py at `fff10a38` (2026-04-08).
- JLC2KiCad_lib at `e7815b61` (2026-09-11).
- Easy-ECAD-Downloader at `86624ad8` (2026-11-21).

**Observed** means seen in real files: KiCad's QA sample archives, which are open-licence projects listed in `sources.txt`. **Unverified** means no primary source was found. EasyEDA's terms forbid scripted access to its site (see `docs/research/vendor-part-downloads.md` on branch `research/vendor-part-downloads`), so no EasyEDA server endpoint was called for this research. Endpoint facts come from the source code of the readers.

## Answer in brief

- **Three families, not one.** EasyEDA ships three unrelated formats:
  1. **EasyEDA Std**: one JSON document, with each primitive written as a `~`-separated string. An LCSC part is a JSON bundle served by EasyEDA's part API: a symbol `dataStr` plus a `packageDetail.dataStr` footprint.
  2. **EasyEDA Pro v2**: a ZIP archive (`.epro` project, `.elibz` library) of JSON-Lines documents (`.esym`, `.efoo`, …) plus a JSON index (`project.json` / `device.json`). A loose `.efoo` or `.esym` can also be downloaded on its own.
  3. **EasyEDA Pro v3**: a ZIP archive (`.epro2` project, `.elibz2` library) holding one change-log document stream (`.epru` / `.elibu`) of `outer||inner` JSON pairs, plus an index (`project2.json`, or `symbol2.json` / `footprint2.json` / `device2.json`). EasyEDA calls v3 "a brand-new source format, completely different from previous formats".

  LCSC product pages offer no file download. They show EasyEDA renders and link to EasyEDA. KiCad files come only from third-party converters.
- **The STEP is never inside the download.** Every format carries only a *reference* to the 3D model: a model UUID, a title and a placement. No sample archive contains a STEP, OBJ or WRL entry (Observed). Readers fetch the STEP separately from EasyEDA's model server, `https://modules.easyeda.com/qAxj6KHrDKw4blvCG8QJPs7Y/{uuid}`; an OBJ comes from `…/3dmodel/{uuid}`. That endpoint is undocumented, and EasyEDA's terms forbid scripted access. KiCad's own importers fetch nothing: they write `${KIPRJMOD}/EASYEDA_MODELS/<title>.step` and leave the file for the user to supply.
- **Where the model binds.**
  - **Std:** the model sits on the **footprint**. It is an `SVGNODE` primitive on layer 19 with `c_etype: "outline3D"` and the attributes `uuid`, `title`, `c_origin` ("x,y" in canvas units), `z`, `c_rotation` ("rx,ry,rz" in degrees), `c_width` and `c_height`. It also carries a 2D outline (`childNodes`) of the model's top view.
  - **Pro v2/v3:** the model sits on the **device**, which is EasyEDA's Part-like record of symbol + footprint + 3D model + properties. The device attributes are `3D Model` (UUID), `3D Model Title` and `3D Model Transform`, and a board `COMPONENT` instance can override them. The Pro `.efoo` footprint itself holds no 3D data.
- **EasyEDA's placement includes a fit-to-size scale and a bounding-box origin.** The official Pro v3 specification defines `3D Model Transform` as `sizeX,sizeY,sizeZ,rotZ,rotX,rotY,offX,offY,offZ` and gives the matrix as `OFFSET × rotateZXY(rotZ,rotX,rotY) × scale(sizeX/wx, sizeY/wy, sizeZ/wz) × translate(-cx,-cy,-bz)`. Here `cx` and `cy` are the model's X/Y midpoints, `bz` its lowest Z, and `wx`, `wy`, `wz` its extents; `sizeZ = 0` means "auto-fit height". The transform holds for the device on the top layer, at (0,0), with rotation 0. So EasyEDA (1) moves the model's bottom-centre to the origin, then (2) stretches it to the given XY size. The Std `c_width`/`c_height` with `c_origin` express the same idea: the Std 3D Model Manager fits "the 2D shape of the 3D model". **Consequence for #432:** the "offset + three rotations, no scale" Model Body can only reproduce EasyEDA when the importer (a) computes the STEP's bounding box and folds `(-cx, -cy, -bz)` into the offset, and (b) accepts that `size/extent ≈ 1`. For LCSC library parts the size usually matches the model's nominal size; for example `C0402_L1.0-W0.5-H0.5` has size `39.37,19.685` mil = 1.0 × 0.5 mm (Observed). Where it does not match, the importer must warn or reject. KiCad and both Python converters skip step (a) or (b), and they disagree with each other (see §4).
- **Units.**
  - **Std:** one unit is 10 mil (0.254 mm) in both schematic and PCB. Y points down, as in SVG. Coordinates are absolute on a canvas, and the footprint origin is `head.x/y`, or canvas fields 16–17 in the PCB canvas string.
  - **Pro PCB and footprint data:** one unit is **1 mil**, per KiCad's Pro PCB scale factor of 25 400 nm. The transform values observed in samples are also mil. The v3 specification's general rule of "0.01 inch unless stated otherwise" matches the schematic but not the PCB values seen.
  - **Pro schematic:** one unit is 0.01 inch.
  - **All Pro:** Y points down. Angles are in degrees, counter-clockwise positive.
- **Pins and pads.** Every family binds a symbol pin to a footprint pad by **pin number = pad number** (strings), as KiCad does.
  - Std pin: segment 4 of the `P~…` record holds the number. Std pad: the `number` field of `PAD~…`.
  - Pro pin: `PIN` plus a child `ATTR … "NUMBER"`. Pro pad: the `num` field of `PAD`.
  - The v3 board adds a `PAD_NET` record (`padNum` → net). No separate pin map exists.
- **Documentation.**
  - Std has an official (Chinese/English) format reference, dated 2020-08-11 and stamped document version 1.7.5. It does **not** document the 3D `SVGNODE` attributes.
  - Pro has official specifications: V2.2 (2022-12-15, "not the latest… contains most of the format details") and **V3 (2025-10-21)**, which defines the 3D transform. Neither specification states a licence.
  - The LCSC part API (`easyeda.com/api/products/{lcsc}/components`) is undocumented.
- **Readers to learn from.**
  - KiCad has Std, Pro v2 and Pro v3 importers. Its EasyEDA code is **GPL-2.0-or-later**.
  - JLC2KiCad_lib (**MIT**) reads Std from the LCSC API.
  - easyeda2kicad.py (**AGPL-3.0**) reads Std and has partial Pro-API code.
  - Easy-ECAD-Downloader (**AGPL-3.0**) is a browser extension.

  Only the MIT reader's code could be reused without licence friction, given REDA's "GPL now, MIT later" plan. The others should be read for facts only.
- **Farnell / element14 / Newark → SamacSys.** Each runs a SamacSys-operated "Component Search Engine" partner site: `farnell.`, `element14.` and `newark.componentsearchengine.com` (footer: SamacSys Ltd, Supplyframe terms). That site lists KiCad among about 25 target tools, and the part pages link to it as the "Supply Frame Models Link" in their ECAD/MCAD section. The KiCad download is the ordinary SamacSys zip (`.kicad_sym` + `.kicad_mod` + STEP), already covered by Import part's KiCad path. It is not Ultra Librarian or SnapMagic. Older Premier Farnell press releases mention Ultra Librarian; that arrangement appears to be superseded (**Unverified**).

## 1. What a user can get

| Route | What comes out | Format family | 3D inside? | Source |
|---|---|---|---|---|
| LCSC product page (`lcsc.com/product-detail/C….html`) | Symbol and footprint **SVG renders** from `image.easyeda.com/component_svgs/…`, and an "EDA Models / EasyEDA" link. No file download | n/a | No | Fetched page C2040, 2026-11-25. LCSC blog 2026-05-29 ("product pages link to EasyEDA component entries") |
| LCSC part API (`easyeda.com/api/products/{lcsc}/components`) used by the converters | JSON: `result.dataStr` (symbol, `head.c_para` with `name`, `pre`, `package`, `Manufacturer`, …), `result.subparts[]` (multi-unit), `result.packageDetail.{title,dataStr}` (footprint), `result.lcsc.{number,url}`, `SMT` | Std | Reference only (`SVGNODE`) | easyeda2kicad `easyeda_api.py`, `easyeda_importer.py` |
| EasyEDA Std editor: File → EasyEDA Source | One `.json` document (schematic, symbol, PCB or footprint); project folder → Download gives a `.zip` of them | Std | Reference only | https://docs.easyeda.com/en/Export/Export-EasyEDA-Source-File/ |
| EasyEDA Pro (v2): Library → save as local library file; Project → Save as (local) | `.elibz` (library zip) / `.epro` (project zip). Loose `.efoo` / `.esym` documents also circulate | Pro v2 | Reference only (device attrs) | https://prodocs.easyeda.com/en/faq/library/ ; https://prodocs.easyeda.com/en/import-export/export-easyeda-pro/ ; KiCad `pcb_io_easyedapro.h` (`elibz`, `efoo`, `epro`, `zip`) |
| EasyEDA Pro (v3) | `.elibz2` (library zip) / `.epro2` (project zip) | Pro v3 | Reference only | KiCad `easyedapro_v3_parser.h`; samples `LS2K0300_*.elibz2`, `ProProject_LS2K0300Core_2025-11-14.epro2` |
| EasyEDA Pro: Export → 3D file | Whole-board STEP (assembly or part) or OBJ. Not per component | STEP/OBJ | Yes, but board-level | https://prodocs.easyeda.com/en/pcb/export-3d-file/index.html |
| EasyEDA Pro: Export Altium / PADS | Symbol and footprint exported **separately** ("does not support exporting the entire device") | other tools | No | https://prodocs.easyeda.com/en/faq/library/ |

**Unverified:** whether EasyEDA Pro currently offers a KiCad *library* export. None was found in the Pro docs; the LCSC blog's "KiCad-compatible format" claim is vague. Also **Unverified**: whether the LCSC product page has a JavaScript-only download control that a static fetch does not show.

## 2. EasyEDA Std (JSON)

Official reference: https://docs.easyeda.com/en/DocumentFormat/EasyEDA-Format-Standard/ (overview dated 2020-08-11) and https://docs.easyeda.com/en/DocumentFormat/3-EasyEDA-PCB-File-Format/index.html.

- **Structure.** A JSON object with `head`, `canvas`, `shape[]` and `BBox`. Each `shape` entry is a string of fields separated by `~`. Sub-records are separated by `^^`, and shapes are joined by `#@$` in raw files. `head` begins `docType~version`: PCB `3~1.7.5`, footprint `4~1.7.5`. `editorVersion` is seen as `6.3.0`. A `head.c_para` map holds part properties.
- **Units.** "EasyEDA takes 10 mil as a basic factor". KiCad scales Std PCB by 254 000 nm and Std schematic by value × 10 mil. Coordinates are absolute canvas positions. Subtract the origin, which is `head.x/y`, or PCB canvas fields 16/17 as easyeda2kicad prefers. Y points down; readers negate it for a Y-up model.
- **Symbol pins.** The `P~show~type~spice~x~y~rot~id~locked^^dot^^path^^name^^number^^dot^^clock` record. The pin number is field 4 of segment 4. Electrical type: 0 unspecified, 1 input, 2 output, 3 bidirectional, 4 power/passive. The readers disagree on 4: easyeda2kicad says power, KiCad dev-docs say passive. Schematic LIB headers carry `uuid` for the component and `puuid` for the bound footprint.
- **Footprint.** The `PAD~shape~cx~cy~w~h~layer~net~number~holeR~points~rot~id~holeLen~holePts~plated~locked` record. Shapes are ELLIPSE, RECT, OVAL and POLYGON. Polygon pads have their rotation already applied to the points. Other primitives are TRACK, ARC, CIRCLE, RECT, HOLE, VIA, TEXT and SOLIDREGION. Layers: 1 top, 2 bottom, 11 multi-layer (THT), paste 5/6, mask 7/8. Std swaps these numbers compared with Pro.
- **3D reference.** An `SVGNODE~{json}` primitive. The official doc says only that "3D models … use the svgnode primitive", with no fields given. The fields below come from KiCad `pcb_io_easyeda_parser.cpp`, the easyeda2kicad docs and JLC2KiCad_lib:
  - `nodeType: 1`, `layerid: "19"`, `attrs.c_etype: "outline3D"`
  - `attrs.uuid`: model id (download key); `attrs.title`: model name, used as the file name
  - `attrs.c_origin: "x,y"`: absolute canvas position of the model's XY centre
  - `attrs.z`: Z offset in Std units
  - `attrs.c_rotation: "rx,ry,rz"`: degrees
  - `attrs.c_width`, `attrs.c_height`: fitted XY size in Std units
  - `attrs.transform`: present, unused by KiCad
  - `childNodes[].attrs.points`: the model's top-view outline polygon
  - The official Std 3D Model Manager page defines "width/height: the width/height of the 2D shape of the 3D model", "X and Y: the X and Y coordinates of the 2D shape", and the rotations: Z anticlockwise in top view, X anticlockwise in side view, Y anticlockwise facing (https://docs.easyeda.com/en/PCB/3D-Model-Manager).
  - Std's own 3D import accepted only WRL and OBJ (https://docs.easyeda.com/en/PCBLib/PCB-3DLib-Import/). STEP for library parts comes from the model server.

## 3. EasyEDA Pro v2 (`.epro`, `.elibz`, `.efoo`, `.esym`)

Official spec: V2.2, 2022-12-15, https://image.lceda.cn/files/lceda-pro-file-format-v2.2_2022.12.15.zip, linked from https://prodocs.easyeda.com/en/format/index. It could not be downloaded from here (the CDN timed out), so this section rests on KiCad's reader and Observed samples.

- **Archive.** A ZIP with folders `SHEET/<uuid>/<n>.esch`, `PCB/<uuid>.epcb`, `SYMBOL/<uuid>.esym`, `FOOTPRINT/<uuid>.efoo`, `POUR/*.ecop`, `BLOB/*.eblob` (base64 images), `FONT/*.efon`, `INSTANCE/` and `PANEL/`. The index is `project.json` (`schematics`, `pcbs`, `symbols`, `footprints`, `devices`, `boards`, `config`). A `.elibz` carries `device.json` / `footprint.json` / `symbol.json` instead (KiCad `ReadProjectOrDeviceFile`). Observed: `ProProject_Yuzuki Chameleon_2023-09-02.epro` has 193 entries (66 `.esym`, 82 `.efoo`) and **no 3D file of any kind**.
- **Documents** are JSON Lines, one array per line, UTF-8, sometimes with a BOM.
  - Header: `["DOCTYPE","FOOTPRINT","1.4"]` or `"1.8"`, `["DOCTYPE","SYMBOL","1.0"]`, then `["HEAD",{…"editorVersion":"2.2.36.7"}]`.
  - `["CANVAS",0,0,"mil",…]`. The canvas unit is display-only; v3 spec: "Canvas unit … does not change stored data".
  - **Versioning:** the DOCTYPE version per document type (FOOTPRINT 1.4, 1.8; SYMBOL 1.0 seen) plus `editorVersion`.
- **Symbol (`.esym`).** `["PART","<name>.1",{BBOX}]` per unit, then `["PIN",id,…,x,y,length,rotation,…]`. Child `["ATTR",id,pinId,"NAME"|"NUMBER"|"Pin Type",value,…]` records carry the name, number and type. Units are 0.01 inch (Observed: pin length 10 = 100 mil).
- **Footprint (`.efoo`).**
  - Pad: `["PAD",id,net?,…,layer,"<num>",x,y,rot,hole|null,[shape,w,h,r],…]`. Observed: `["PAD","e9",0,"",1,"2",16.4,0,0,null,["RECT",19.889,21.685,0],…]` for a 0402 pad 2.
  - Other records: `FILL`, `POLY`, `ATTR` (`Footprint` name, `Designator`).
  - Units are mil.
  - 3D-related layers exist (48 COMPONENT_SHAPE, 53–55 3D shell), but **no model reference**.
- **Device = Part.** `project.json` → `devices[uuid].attributes` holds `Symbol` (uuid), `Footprint` (uuid), `Designator`, `Supplier Part` (LCSC `C…`), `Manufacturer`, `Manufacturer Part`, `Datasheet`, `Supplier`, `JLCPCB Part Class`, plus the 3D attributes. Observed (device `0402ESDA-05N`):

  ```
  "3D Model": "62a80a189f1b46df8ba79817b730b378",
  "3D Model Title": "D0402_L1.0-W0.5-H0.5",
  "3D Model Transform": "39.37,19.685,0,180,0,0,-0.002,-0.0025,0"
  ```

  Board `COMPONENT` instances may override the same attributes (KiCad `pcb_io_easyedapro_parser.cpp` reads `localCompAttribs` first). The Pro 3D Model Manager binds to the **device**: "the selected device will be bound when binding the 3D model". Its Auto button "automatically adjust[s] the size and position of the model" from the top-view size (https://prodocs.easyeda.com/en/pcb/tools-3d-model-manager/).

## 4. EasyEDA Pro v3 (`.epro2`, `.elibz2`) and the 3D transform

Official spec: V3, 2025-10-21, https://image.lceda.cn/files/lceda-pro-file-format-v3_2025.10.21.md (also as PDF, and as a zip with examples). Written in Chinese. Quotes are translated.

- **Archive.**
  - `.epro2`: `project2.json` (title and tags only, Observed) + `<name>.epru` + `IMAGE/*.webp`.
  - `.elibz2`: `symbol2.json` / `footprint2.json` / `device2.json` (index: `uuid`, `title`, `display_title`, `owner`, `updateTime`, …) + `<name>.elibu`.
  - KiCad detects v3 by these names (`V3_DOC_PARSER::IsV3Archive`, `IsV3Library`).
- **Stream.** One line per record: `{"type":…,"id":…,"ticket":n}||{…atomic object…}|`. A `DOCHEAD` row (`docType`, `uuid`, `client`, `version` = source update time) starts each document; doc types include SYMBOL, FOOTPRINT and DEVICE. `ticket` is a logical clock: the last write wins, and readers must de-duplicate by `id` (KiCad `ParseEpruStream`). **There is no format-version field**; the spec gives none, and versioning is implicit in the `2` suffixes.
- **Device.** A `META` row: `{"title":…,"source":…,"attributes":{"Symbol":…,"Footprint":…,"3D Model":uuid,"3D Model Title":…,"3D Model Transform":…,"Supplier Part":"C…",…}}`. Observed in the `.epro2` sample. The sample `.elibz2` files hold only symbols or only footprints, with no 3D mentions.
- **3D transform (normative).** From the spec section "3D Model Transform 的特殊说明":
  - "In the device, `3D Model Transform` is the transform the 3D model needs to match this device [on the top layer] [at 0,0] [at rotation 0]."
  - Fields in order: `sizeX`, `sizeY`, `sizeZ` ("if 0, auto-fit height"), `rotZ`, `rotX`, `rotY`, `offX`, `offY`, `offZ`.
  - The matrix:

    ```
    ORIGIN = translate(-cx, -cy, -bz)      # model X/Y midpoint, lowest Z
    SCALE  = scale(sizeX/wx, sizeY/wy, sizeZ/wz)
    ROT    = rotateZXY(rotZ, rotX, rotY)
    OFFSET = translate(offX, offY, offZ)
    MATRIX = OFFSET × ROT × SCALE × ORIGIN
    ```

  - `3D Model` is "the uuid of a record with doctype = 16 in the components table", a server-side table.
  - Units are not stated for the transform. The Observed values are mil: `39.37` = 1.0 mm for a 1.0 mm-long 0402 model. KiCad converts them as mil.
  - **Unverified:** what "auto-fit height" means exactly. It may mean a uniform scale taken from XY, or the native height.
- **How the readers place the model (they disagree):**

  | Reader | Offset | Rotation | Size / scale | Bounding-box origin |
  |---|---|---|---|---|
  | KiCad Std (`pcb_io_easyeda_parser.cpp`) | `-(c_origin − fp origin)`, Y flipped; Z = `z` | `(-rx, -ry, -rz + fp angle)`; bottom side: `rz = 180 − rz` | **Ignored**; size kept as hidden field `JLC_3D_Size` | Not applied |
  | KiCad Pro (`FillFootprintModelInfo`) | `(offX, offY, offZ)` as mil | `(-rotX, -rotY, -rotZ)` | **Ignored**; kept as `JLC_3D_Size`, UUID as `JLC_3DModel_Q` | Not applied |
  | easyeda2kicad (AGPL) | WRL: bbox-centre and min-Z folded into vertices, footprint offset 0. **STEP: copied unchanged, offset 0.** A TODO says the STEP is misplaced when `c_origin` ≠ origin | `(360 − r) mod 360` | Ignored | Applied to WRL only |
  | JLC2KiCad_lib (MIT) | `(c_origin − fp origin)/100` in inches, Y negated; Z = `z/100` | negated | Ignored | Not applied |

  Every reader drops the scale, and only easyeda2kicad's WRL path applies the bounding-box origin. **Unverified:** whether the STEP files on the model server are already stored bottom-centred. If they are, the ORIGIN term is a no-op for STEP and the KiCad/JLC2KiCad approach is correct. This needs an empirical check against a handful of parts, made by hand in EasyEDA Pro rather than scripted. #455 should decide whether `reda-import-easyeda` computes the STEP bounding box itself, which is what the spec implies.

## 5. Where the STEP comes from

- Std and LCSC parts: `https://modules.easyeda.com/qAxj6KHrDKw4blvCG8QJPs7Y/{uuid}` returns the STEP; `https://modules.easyeda.com/3dmodel/{uuid}` returns an OBJ in mm. JLC2KiCad_lib notes that the bucket constant comes from EasyEDA's `smt-gl-engine.js`. Older code used `easyeda.com/analyzer/api/3dmodel/{uuid}`. Sources: easyeda2kicad `easyeda_api.py`, JLC2KiCad_lib `footprint/model3d.py`, Easy-ECAD-Downloader `src/sources/easyeda_common.js`.
- Pro device UUIDs (`3D Model`) point into a server "components" table (doctype 16). easyeda2kicad lists `pro.easyeda.com/api/v2/components/{uuid}` and marks it "requires auth". **Unverified:** whether Pro model UUIDs work on the `modules.easyeda.com` STEP bucket.
- No downloadable archive carries the model. A whole-board STEP export from Pro does carry bound models, but only for the board as a whole.
- Fetching from these endpoints is scripted access to undocumented URLs, and the EasyEDA terms forbid it without written permission (see the earlier vendor research). **Implication for #455:** the importer should accept a user-supplied STEP, either dropped next to the JSON/archive or picked by hand, and match it by `uuid` or `title`. Any built-in fetch needs EasyEDA's permission.

## 6. Open-source readers

| Reader | Reads | Licence | Notes |
|---|---|---|---|
| KiCad (`pcbnew/pcb_io/easyeda`, `easyedapro`, `eeschema/sch_io/…`, `common/io/…`) | Std `.json`/`.zip`; Pro v2 `.epro`/`.elibz`/`.efoo`/`.zip`; Pro v3 `.epro2`/`.elibz2` | **GPL-2.0-or-later** (file headers) | Most complete; Pro v3 support is brand new (2025–26 samples). Developer notes: https://dev-docs.kicad.org/en/import-formats/easyeda/index.html |
| easyeda2kicad.py (uPesy) | Std via the LCSC API; partial Pro API | **AGPL-3.0** | Good field docs in `docs/CMD_*.md` |
| JLC2KiCad_lib (TousstNicolas) | Std via the LCSC/JLC API | **MIT** | Only permissive reader found |
| Easy-ECAD-Downloader (JoeShade) | Std via the API; SamacSys zips from Mouser/Farnell | **AGPL-3.0** | Browser extension |
| KiCAD-EasyEDA-Parts (Yanndroid) | Std | MIT | KiCad plugin; inactive since 2024 |
| wokwi/easyeda2kicad | Std *projects* | No licence asserted | Inactive since 2023 |

REDA should learn the format from the official specs and these readers, but write its own code (AGENTS.md; licensing plan "GPL now, MIT later"). Copying from the GPL or AGPL readers is out.

## 7. Farnell / element14 / Newark

- Partner sites operated by SamacSys: `https://farnell.componentsearchengine.com/`, `https://element14.componentsearchengine.com/` and `https://newark.componentsearchengine.com/`. All three returned HTTP 200 on 2026-11-25. The Farnell one is branded "Farnell Component Search", its footer names SamacSys Ltd, and it links Supplyframe terms. It lists "KiCad EDA" among about 25 tools and links Library Loader.
- The product pages link to it from the ECAD/MCAD section via an image link titled "Supply Frame Models Link". This is Observed in Easy-ECAD-Downloader `src/content_script.js` and `systemDesign.md`. Farnell and Newark pages answered automated fetches with 403, so this was not seen first-hand.
- Format: the standard SamacSys KiCad zip, as described in `docs/research/vendor-part-downloads.md` (§1–2). It holds `.kicad_sym` + `.kicad_mod` (often legacy `module` syntax) + STEP, and the STEP path in the footprint is often absolute and must be ignored. Import part's KiCad-zip path covers it.
- History: Premier Farnell once offered CAD models built with Ultra Librarian (press releases at news.thomasnet.com and newelectronics.co.uk, undated or old). **Unverified:** whether any Farnell-family page still serves Ultra Librarian files.

## 8. Consequences for #455

1. Recognise four inputs: Std JSON (an LCSC API bundle or an editor source file), Pro v2 zip or loose `.efoo`/`.esym`, and Pro v3 zip. Sniff the contents, not the extension: `.zip` is used for Std, v2 and v3 alike.
2. The Part comes from the Std bundle (`dataStr` + `packageDetail`) or from a Pro device (`Symbol` + `Footprint` + 3D attributes). Bind pins to pads by number string. Map `Supplier Part`, `Manufacturer`, `Manufacturer Part` and `Datasheet` to Part fields.
3. The Model Body needs a STEP the user supplies (none is in the download). Compute the offset from `c_origin`/`z` (Std) or `offX/Y/Z` (Pro) in each family's unit with Y flipped, and the rotation from `c_rotation` or `rotZ,rotX,rotY`. Note the Pro ZXY rotation order. Then either add the bounding-box origin term or confirm the STEP is stored bottom-centred. Compare the size against the STEP's extents: warn when the ratio is not 1, since REDA's Model Body has no scale.
4. Keep the source `uuid` and `title` on the Model Body, so a later fetch or match is possible.
