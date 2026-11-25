# EasyEDA / LCSC part downloads: file formats and model binding

Research for issue #454, part of the v0.3 map (#421). Question: what can a user download for a part from EasyEDA / LCSC, what does each file hold (structure, versioning, symbol, footprint and pins, units and coordinates), how is the 3D model referenced and placed, is the model's STEP inside the download, which formats are documented, and which open-source readers exist. Also: what Farnell / element14 / Newark hand out for a part's CAD download.

Context: #426 (`docs/research/3d-model-sources.md` on branch `research/3d-model-sources`) found that EasyEDA serves OBJ and STEP, that EasyEDA's terms forbid copying and automated access, and that an EasyEDA footprint carries a model UUID and a transform. #432 decided that REDA imports only files the user downloaded, through a new `reda-import-easyeda` crate. This note therefore covers files a user can save by hand. It does not cover EasyEDA's server API except where readers use it.

All sources were read on 2026-11-25. This note records facts and recommends nothing. Markers:

- **Observed** means the fact comes from reading a sample file or source code, not from a statement by its owner.
- **Unverified** means the primary page could not be reached or does not state the fact. The reason is given each time.

Sample files read (all from KiCad's QA data at commit `f445174c`, `qa/data/pcbnew/plugins/easyeda*/`, plus EasyEDA's own V3 example):

| File | Format | Licence (per KiCad's `sources.txt`) |
|---|---|---|
| `PCB_USBMETER-PD QC修改版_2023-09-02.json` | EasyEDA Std PCB JSON, editor 6.5.34 | CC-BY-NC-SA 3.0 |
| `ProProject_Yuzuki Chameleon_2023-09-02.epro` | EasyEDA Pro v2 project | CERN-OHL-S 2.0 |
| `ProProject_LS2K0300Core_2025-11-14.epro2`, `LS2K0300_Footprint_2025-11-14.elibz2` | EasyEDA Pro v3 project and footprint library | "Provided by contributor for QA import compatibility testing" |
| `【泰山派1F】金手指核心板….epro2` inside `lceda-pro-file-format-v3_2025.10.21.zip` | EasyEDA Pro v3 project | EasyEDA's format-spec example; no licence stated |

## Answer in brief

- **Three formats, two products.** EasyEDA Standard ("Std") saves one JSON document per schematic, PCB or library item. EasyEDA Pro has two generations. **v2** (`.epro` project, `.elibz` library) is a zip of one-JSON-array-per-line files. **v3** (`.epro2`, `.elibz2`) is a zip with one append-only log file of `{outer}||{inner}|` lines. Pro cannot export to Std.
- **What the user can save by hand.** Std: **File > EasyEDA File Source…** for one document, or a project zip of these JSON files. Pro: **Save as local** on a library device, or **File > Project Save as (local)**, giving `.elibz`/`.elibz2` or `.epro`/`.epro2`. The LCSC product page has an "EDA Models: Open the EasyEDA Model" button and symbol and footprint pictures, but no download link in its HTML.
- **No model geometry in any download.** Std JSON, Pro v2 and Pro v3 archives hold a model **UUID, title and transform**, never the OBJ or STEP. **Observed** in all four samples, and KiCad's importers assume it: they point the KiCad footprint at `${KIPRJMOD}/EASYEDA_MODELS/<model title>.step`, a file the user must supply.
- **Where the binding sits differs.**
  - In **Std**, the binding is on the **footprint**: an `SVGNODE` on layer 19 (`c_etype: outline3D`) inside the footprint, with `uuid`, `title`, `c_origin`, `z`, `c_rotation`, `c_width`, `c_height`.
  - In **Pro**, it is on the **Device** (symbol + footprint + model + properties): attributes `3D Model` (UUID), `3D Model Title` and `3D Model Transform`. A placed component may override them. A Pro footprint on its own carries no model.
- **Units.** Std stores 10 mil (0.254 mm) per unit, Y down. Pro PCB data is in mil (0.0254 mm) per unit, Y up. Pro schematic and symbol data is in 0.01 inch. The Pro v3 spec says "0.01 inch" for everything, but every PCB sample and KiCad's reader use mil (see §3). Angles are degrees, counter-clockwise positive.
- **Transform semantics.**
  - Pro documents its transform. The fields are `sizeX,sizeY,sizeZ,rotZ,rotX,rotY,offX,offY,offZ`. The matrix is `OFFSET × ROT(Z,X,Y) × SCALE × ORIGIN`, where ORIGIN moves the model's XY midpoint and lowest Z to 0. The model is therefore **scaled to a target size**, not placed by its own origin. The page gives no units; samples show mil.
  - Std's binding is not documented. Readers treat `c_origin` and `z` as the model centre in footprint coordinates, `c_rotation` as X,Y,Z degrees, and `c_width`/`c_height` as the fitted outline size.
- **Documentation.** Pro v2.2 (2022) and v3 (2025-10-21) have published format specs from EasyEDA. Std's format is published (v1.7.5 and a 2020-08-11 revision), but not its 3D binding.
- **Readers.**
  - KiCad imports Std (`.json`, `.zip`) and Pro v2 (`.epro`, `.elibz`, `.zip`) since 2023-09-07. Pro v3 (`.epro2`, `.elibz2`) is on `master` since 2026-07-18 and is in no release up to 10.0.8. KiCad is GPL-2.0-or-later.
  - `easyeda2kicad` (AGPL-3.0) and `JLC2KiCad_lib` (MIT) read the Std JSON that EasyEDA's server returns. `wokwi/easyeda2kicad` (MIT, TypeScript, last push 2023) converts Std designs.
- **STEP availability.** STEP exists on EasyEDA's model server for system-library models: both third-party readers fetch it by UUID from an undocumented URL. No documented way to save a single part's STEP by hand was found. Pro's device menu has **Save 3D model as**, whose result is not described (§5).
- **Farnell / element14 / Newark.** **Unverified.** Every Farnell, element14 and Newark product and help page returned HTTP 403 or timed out for automated clients, and no first-party statement naming SamacSys, Ultra Librarian or SnapMagic was found. The only first-party-sourced statement is from 2017, about TraceParts mechanical CAD files (§8).

## Summary table

| Download | How the user gets it | Container | Symbol, footprint, pins | Units / axes | 3D model reference | Model geometry inside? | Documented? |
|---|---|---|---|---|---|---|---|
| EasyEDA Std source | Std editor: **File > EasyEDA File Source…**; project folder: **Download** (zip) | One JSON object (`head`, `canvas`, `shape[]`, …); a library item may be wrapped in a server envelope with `dataStr` | `shape[]` strings, `~`-separated fields, `#@$` between primitives; footprint = `LIB~…` group; pin = `P~…^^…` segments; pad = `PAD~…` | 10 mil per unit; canvas origin top-left, Y down; origin in `head.x/y` | `SVGNODE` on footprint, layer 19, `c_etype: outline3D`: `uuid`, `title`, `c_origin`, `z`, `c_rotation`, `c_width`, `c_height`, `transform`; also `c_para` key `3DModel` (title) | No | Format yes; 3D node no |
| EasyEDA Pro v2 project / library | **File > Project Save as (local)** → `.epro`; device **Save as local** → `.elibz` | Zip: `project.json` / `device.json` index plus `SYMBOL/*.esym`, `FOOTPRINT/*.efoo`, `SHEET/*/*.esch`, `PCB/*.epcb`, `BLOB/*.eblob`, `POUR/*.ecop`, `FONT/*.efon` | One JSON array per line: `["DOCTYPE","FOOTPRINT","1.3"]`, `["PAD", id, …]`; pin number/name as `NUMBER`/`NAME` attributes | PCB mil, Y up (`["CANVAS",0,0,"mil",…]`); schematic 0.01 in | Device attributes `3D Model`, `3D Model Title`, `3D Model Transform` in the index's `devices` map | No | V2.2 spec (2022-12-15, Chinese PDFs) |
| EasyEDA Pro v3 project / library | Same menus in EasyEDA 3.x → `.epro2` / `.elibz2` | Zip: `project2.json` or `footprint2.json` / `symbol2.json` / `device2.json` index, one `.epru` / `.elibu` log, `IMAGE/*.webp` | Log lines `{"type":…,"id":…,"ticket":n}||{key: value}|`; `DOCHEAD` starts each document; pins carry `Pin Number` / `Pin Name` | Same as v2 (spec says 0.01 in throughout; PCB samples are mil) | Same three Device attributes, in the `DEVICE` document's `META.attributes`; PCB `COMPONENT.attrs` may override | No | V3 spec (2025-10-21, Chinese Markdown and PDF; English web pages) |
| LCSC product page | "EDA Models: Open the EasyEDA Model" button | none found | SVG previews of symbol and footprint from `image.easyeda.com` | n/a | n/a | n/a | n/a |

## 1. What a user can save by hand

- **EasyEDA Std.**
  - "You can create an EasyEDA source file with: File > EasyEDA File Source…". A project can be downloaded via "Project folder > Right Click > Download, this will download a zip file with EasyEDA Source files containing Schematics and PCBs". Source: https://docs.easyeda.com/en/Export/Export-EasyEDA-Source-File/
  - For library items: "We need to open and edit the library file, then export it as EDA source code, or export the AD file." Source: https://docs.easyeda.com/en/FAQ/Library/
  - Std does not export a symbol or footprint in Altium library format. The user places it in a schematic or PCB and exports that as `.schdoc`/`.pcbdoc`. Source: https://docs.easyeda.com/en/Export/Export-Altium/
- **EasyEDA Pro.**
  - Device list right-click menu: "Save as local: After the operation, the selected device will be downloaded to the local, and can be imported later through the top menu-File-Import-Professional Edition." Source: https://prodocs.easyeda.com/en/device/side-panel-bottom-library-device-list/
  - The library FAQ calls the exported file an `elibz`: "Export the components of your personal library … Switch to online mode and import the exported elibz file." Source: https://prodocs.easyeda.com/en/faq/library/
  - Project: "Top Menu - File - Project Save as (local) … the compressed footprint includes the device library and footprint placed in the engineering schematic diagram." And: "Export to EasyEDA Standard format is not supported." Source: https://prodocs.easyeda.com/en/import-export/export-easyeda-pro/
  - Library export to other tools: Pro "supports exporting files in PADS and AD formats", and only "the symbol and footprint separately", not "the entire device". Source: https://prodocs.easyeda.com/en/faq/library/
  - EasyEDA also offers a desktop "Format Converter", whose export formats are ".zip and .epro". Source: https://prodocs.easyeda.com/en/import-export/easyeda-pro-format-converter/
- **LCSC product page.** **Observed** in the HTML of https://www.lcsc.com/product-detail/C25744.html:
  - an "EDA Models" row with a button labelled "Open the EasyEDA Model";
  - a section "C25744 EasyEDA Library" with symbol and footprint SVGs from `image.easyeda.com/component_svgs/prod/…`.

  The static HTML has no "Download", "STEP", "KiCad" or "Altium" text. What the button opens is client-side and was not followed. **Unverified:** whether LCSC offers any file download for a part.
  - LCSC's blog (2026-05-29) says "LCSC product pages link to EasyEDA component entries that include downloadable footprint files in a KiCad-compatible format". It points Altium users to Ultra Librarian or SnapMagic. This is a marketing post and was not confirmed on a product page. Source: https://lcsc.com/blog/smarter-pcb-design-easyeda/
- **Std to Pro migration** loses the 3D binding: "The 3D binding design of the professional version is different from that of the standard version. The 3D library import of PCB and package binding will not be imported together in the professional version, and the 3D model library needs to be re-bound". Source: https://prodocs.easyeda.com/en/import-export/import-easyeda/

## 2. EasyEDA Standard JSON

- **Documents.** EasyEDA publishes the format at https://docs.easyeda.com/en/DocumentFormat/0-EasyEDA-File-Format-Index/ (PCB page says "document version: 1.7.5") and a fuller "Standard Edition Document Format" dated 2020.08.11 at https://docs.easyeda.com/en/DocumentFormat/EasyEDA-Format-Standard/ (mostly Chinese).
- **Structure.** "EasyEDA's file is a JSON file, but we compress all of the shape's attributes to a simple string". The delimiters are `~` between fields, `` ` `` between custom-attribute key and value, `^^` between segments of a pin, pad or net flag, and `#@$` between primitives inside a group. Source: https://docs.easyeda.com/en/DocumentFormat/1-Common-Information/
  - `head` holds `docType`, `editorVersion`, `c_para` (custom attributes), and `x`, `y` ("Document the origin X coordinates"). Source: the 2020 page above.
  - **Observed** in KiCad's `easyeda_parser_structs.h`, the doc types are: 1 schematic sheet, 2 symbol, 3 PCB, 4 PCB component (footprint), 5 schematic list, 14 PCB module.
  - KiCad's `UnwrapDataStr` unwraps a document that "the EasyEDA server wraps in a metadata envelope … under `dataStr`". The 2020 spec's own example of docType 5 shows the same `dataStr` nesting.
- **Units and axes.** "EasyEDA takes 10 mil as a basic factor, when a stroke width is 1, we can take it as 1*10mil = 10mil". Source: https://docs.easyeda.com/en/DocumentFormat/3-EasyEDA-PCB-File-Format/
  - The canvas origin is top-left ("The upper-left corner of the canvas has the coordinate (0,0)").
  - **Observed**: KiCad converts with `ScaleSize(v) = v × 254000 nm` and does not flip Y (`pcb_io_easyeda_parser.h`). The schematic is also in 10 mil (`sch_easyeda_parser.h`, `MilsToIU(v × 10)`).
- **Footprint.** A footprint is a `LIB~x~y~<c_para>~…` header followed by its primitives, joined with `#@$`. The `c_para` keys seen include `package`, `3DModel` (model title), `BOM_Supplier`, `BOM_Supplier Part` (the LCSC number) and `Contributor`. Source: the 2020 page, footprint section, and **observed** in the KiCad Std sample.
- **Pads.** `PAD~shape~x~y~width~height~layerid~net~number~holeR~pointArr~rotation~gId~holeLength~slotPointArr~plated~locked~pasteexpansion~solderexpansion~holeCenter`. Shapes: `ELLIPSE`, `RECT`, `OVAL`, `POLYGON`. Layer 11 is multi-layer (through-hole). Source: the 2020 page, "焊盘PAD".
  - The 2020 page calls `holeR` "孔直径" (hole diameter). The 1.7.5 page says "hole radius: 1.8 (18 mil)". **Observed**: KiCad doubles the value (`holeD = ConvertSize(holeDia) * 2`), so it is a radius.
- **Layers.** **Observed** in KiCad's `LayerToKi`: 1/2 top/bottom copper, 3/4 silk, 5/6 paste, 7/8 mask, 10 board outline, 11 multi-layer, 12 documentation, 13/14 fab, 19 "3D model", 21–52 inner copper.
- **Pins.** `P~show~elecType~number~x~y~rotation~id^^dot^^path^^name^^number^^…`. **Observed**: KiCad takes the electrical type and number from the first segment, the connection point from the second, and the name from the fourth (`sch_easyeda_parser.cpp`).
- **3D binding.** The format spec says only that 3D models are built from `SVGNODE` primitives: "pcb文档中的图形，3D模型等都使用的svgnode图元进行构建". It defines no fields. **Observed** in the KiCad sample (SOT-23-5):

  ```
  SVGNODE~{"nodeType":1,"layerid":"19","attrs":{"c_width":"11.4173","c_height":"10.9636",
    "c_rotation":"0,0,270","z":"0","c_origin":"4095.0135,3478.5863",
    "uuid":"460193f9bf2d42e58cf3c2f675b07dc6","c_etype":"outline3D",
    "title":"SOT-23-5_L2.9-W1.6-H1.1-LS2.8-P0.95","transform":"scale(0.9603) translate(169.293, 143.8091)"},
    "childNodes":[…outline polylines…]}
  ```

  - `c_width` 11.4173 × 0.254 mm = 2.90 mm, the package length in the title, so the field is in 10 mil.
  - `c_origin` is in canvas coordinates, next to the `LIB` origin (4095.1, 3478.5).
  - The node also carries the model's top-view outline as `childNodes`.
- **How readers place it** (**observed**; none of this is documented by EasyEDA):
  - KiCad (`pcb_io_easyeda_parser.cpp`, branch `SVGNODE`, layer `19`) does four things:
    - Offset = −(`c_origin` − footprint origin), scaled to mm and rotated back by the footprint's angle. `z` becomes the Z offset.
    - Rotation = −`c_rotation` per axis, plus the footprint's angle on Z. On the bottom side it mirrors as 180° − Z.
    - It stores the UUID in a hidden field `JLC_3DModel` and `c_width c_height` (mm) in `JLC_3D_Size`. It does not scale.
    - Model path is `${KIPRJMOD}/EASYEDA_MODELS/<title>.step`.
  - `easyeda2kicad` (`easyeda_importer.py`, commit `fff10a3`) computes the offset as `(c_origin − head.x/y) × 0.254`, with Y negated and `z × 0.254`. It replaces that with the outline centre when the two differ by more than 0.1 mm. It bakes the offset into the WRL it writes from the OBJ, and copies the STEP untouched. A code comment says the STEP "is misplaced when model_3d.translation != (0,0,0)".
  - `JLC2KiCad_lib` (`footprint/model3d.py`, `footprint_handlers.py`, commit `e7815b6`) writes `(c_origin − origin)/100` and `z/100` as a KiCad `at` offset in inches, with Y negated, and the rotation negated per axis.
  - Both converters, and EasyEDA's own model handling, assume the model is centred in XY with its lowest point at Z = 0. `easyeda2kicad`'s WRL writer says: "Center XY on (0,0); shift Z so bottom sits at z=0". This matches Pro's documented ORIGIN step (§4).
- **Std's own UI for the binding.** The 3D Model Manager sets "width/height: the width/height of the 2D shape of the 3D model", "X and Y", and rotations about Z, X and Y (all counter-clockwise). Source: https://docs.easyeda.com/en/PCB/3D-Model-Manager/
- **Std's model formats.** Std's import page says "currently only 3D models in "WRL(VRML)" and "obj" are supported". Source: https://docs.easyeda.com/en/PCBLib/PCB-3DLib-Import/
  - Std exports a board as OBJ, which "is not the standard OBJ format, which contains the material data". Source: https://docs.easyeda.com/en/PCB/Export-3D-File/
  - The 3D Model Manager page says "Exporting PCB 3D model files is not supported yet", which contradicts the export page.

## 3. EasyEDA Pro v2 (`.epro`, `.elibz`)

- **Spec.** "V2.2 file format: lceda-pro-file-format-v2.2_2022.12.15.zip. Hint: This is not the latest file format specification, but it contains most of the format details." The zip holds three Chinese PDFs: 原理图文档格式 (schematic), 通用文档格式 (common) and PCB文档格式 (PCB). Source: https://prodocs.easyeda.com/en/format/index/ . The PDFs were listed, not read.
- **Container.** **Observed** in the KiCad sample `.epro`, a zip of 193 entries:
  - `project.json` (keys `schematics`, `pcbs`, `panels`, `symbols`, `footprints`, `devices`, `boards`, `config`);
  - `SYMBOL/<uuid>.esym`, `FOOTPRINT/<uuid>.efoo`, `SHEET/<uuid>/<n>.esch`, `PCB/<uuid>.epcb`, `POUR/*.ecop`, `BLOB/*.eblob`, `FONT/*.efon`.

  No `.step`, `.stp`, `.obj` or `.wrl` entry.
  - **Observed** in KiCad's `EASYEDAPRO::ReadProjectOrDeviceFile`, the index file of a library is `device.json`, `footprint.json` or `symbol.json`.
- **Lines.** Each file holds one JSON array per line. **Observed** in a sample `.efoo`: `["DOCTYPE","FOOTPRINT","1.3"]`, `["CANVAS",0,0,"mil",5,5]`, `["LAYER",1,"TOP","Top Layer",…]`, then `["PAD","e15",0,"",12,"2",124.8,86.8,90,["SLOT",70.9,35.4],["OVAL",59.1,98.4],[],0,0,90,1,…]`.
  - **Observed** in KiCad's `createPAD`, the pad fields are: id, (unknown), net, layer, number, x, y, rotation, hole `[shape,w,h]`, pad shape `[shape,…]`, ….
  - The hole shapes are `ROUND` and `SLOT`, and `["ROUND",0,0]` means no hole.
- **Units.** **Observed** in KiCad: PCB `ScaleSize(v) = v × 25400 nm` (1 mil), and `ScalePos` negates Y (`pcb_io_easyedapro_parser.h`). Schematic and symbol data use `MilsToIU(v × 10)` (0.01 in) (`sch_easyedapro_parser.h`).
- **Pins.** A `PIN` row has id, position, length, rotation and inverted. Number and name are `ATTR` rows on the pin, with keys `NUMBER` and `NAME` (v2) or `Pin Number` and `Pin Name` (v3). **Observed** in `easyedapro_parser.h` and `sch_easyedapro_parser.cpp`.
- **Device and 3D binding.** "Device = Symbol + footprint + 3D model + Properties." Source: https://prodocs.easyeda.com/en/faq/library/
  - **Observed** in the sample's `project.json`, a device entry looks like this: `{"Supplier Part":"C316049","Symbol":"411bab…","Footprint":"61e029…","3D Model":"62a80a18…","3D Model Title":"D0402_L1.0-W0.5-H0.5","3D Model Transform":"39.37,19.685,0,180,0,0,-0.002,-0.0025,0"}`.
  - 28 of the 67 devices name a model.
  - KiCad reads the binding from the Device, and for a placed component lets the component's own attributes override it (`pcb_io_easyedapro_parser.cpp`, around line 1945).

## 4. EasyEDA Pro v3 (`.epro2`, `.elibz2`) and the documented transform

- **Spec.** "V3 uses a brand-new source format, completely different from previous formats … using the concept of logs for incremental storage of project changes; and switched from the array offset design style in 2.0 to a key-value style". Files: `lceda-pro-file-format-v3_2025.10.21.zip`, `.md` and `.pdf`, plus English web pages under `/en/format/`. Source: https://prodocs.easyeda.com/en/format/index/
- **Log format.** "Each line consists of two valid JSON objects concatenated together", split by `||`.
  - The outer object `{type, id, ticket}` is for the "eventual consistency framework". The inner object is the primitive. A larger `ticket` wins.
  - "Deleting data actually means setting the inner layer data to an empty string."
  - Each document starts with `{"type":"DOCHEAD"}||{"docType":…,"uuid":…}`.
  - The doc types are `PROJECT_CONFIG`, `BOARD`, `SCH`, `SCH_PAGE`, `PCB`, `PANEL`, `SYMBOL`, `FOOTPRINT`, `DEVICE`, `BLOB` and `INSTANCE`.
  - "Rotation angle is positive in the counter-clockwise direction, and angles are always in degrees. Unless otherwise specified, all coordinates, lengths, and sizes use 0.01 inch as the unit."

  Source: same page.
- **Units conflict.** The samples contradict the spec's "0.01 inch" for PCB data. **Observed**:
  - In EasyEDA's own `.epro2` example, a `CRYSTAL-SMD_4P-L2.0-W1.6` footprint has pads 34.449 × 30.512 at ±25.12, ±19.21. In mil that is 0.875 × 0.775 mm at a 1.276 × 0.976 mm pitch, which fits a 2.0 × 1.6 mm crystal. In 0.01 in it would be 10× too large.
  - `3D Model Transform` for `R0402_L1.0-W0.5-H0.5` is `39.37,19.685,…`, which is 1.0 × 0.5 mm in mil.
  - KiCad's v3 reader reuses the v2 mil scale.
  - The CANVAS `unit` field is "display unit (does not affect the unit of data in the format)". Source: https://prodocs.easyeda.com/en/format/pcb/common/
- **Container.** **Observed**:
  - EasyEDA's example `.epro2` holds `project2.json`, one `.epru` (18 MB) and `IMAGE/*.webp`.
  - KiCad's `.epro2` sample has the same layout.
  - KiCad's `.elibz2` sample holds `footprint2.json` and one `.elibu`.
  - No model file in any of them.
  - KiCad's detection: "a .elibz2 extension with a ZIP containing symbol2.json, footprint2.json or device2.json and at least one .elibu document file" (`easyedapro_v3_parser.h`).
- **Pads.** `PAD` is key-value: `layerId`, `num`, `centerX`, `centerY`, `padAngle`, `hole`, `defaultPad`, `specialPad` (per layer range), `padOffsetX/Y`, `relativeAngle`, `plated`, and expansions. The hole is `ROUND` or `RECT` with width and height. Source: the V3 Markdown spec, section "PAD 焊盘".
- **Where the binding lives.** **Observed** in the example `.epru`: the three model attributes appear in `DEVICE` documents' `META.attributes` (72 times), in `PCB` `COMPONENT.attrs` (8) and as schematic and PCB `ATTR` rows. They never appear in a `FOOTPRINT` document.
  - KiCad's v3 reader says: "3D models are applied by the caller from the component's Device ATTR … footprints are often shared by devices with different models" (`pcb_io_easyedapro_v3_parser.cpp`).
  - The spec page: "Instances only contain attributes. Attributes with the same Key in the instance will override those in the FOOTPRINT." Source: https://prodocs.easyeda.com/en/format/pcb/component/
- **The transform, as documented.** Source: https://prodocs.easyeda.com/en/format/pcb/component/ and the V3 Markdown spec.
  - "Fixed `3D Model` is the uuid of the 3D model. This uuid represents a record with doctype = 16 in the components table." That table is server-side and is not in the file.
  - "In the device, 3D Model Transform is fixed as the transformation parameters required for the 3D model to match the device when it is [on the top layer], at [coordinates 0,0], and [rotation angle 0]."
  - Fields in order: `sizeX`, `sizeY`, `sizeZ` ("if 0, height is automatically adapted"), `rotZ`, `rotX`, `rotY`, `offX`, `offY`, `offZ`. Example `20,10,0,0,15,45,0,0,20`.
  - Algorithm: `cx, cy` = the model's X and Y midpoint, `bz` = its lowest Z, `wx, wy, wz` = its widths. ORIGIN = translate(−cx, −cy, −bz); SCALE = scale(sizeX/wx, sizeY/wy, sizeZ/wz); ROT = rotateZXY(rotZ, rotX, rotY); OFFSET = translate(offX, offY, offZ); MATRIX = OFFSET × ROT × SCALE × ORIGIN.
  - No units are stated. **Observed** values are in mil (above).
  - **Unverified:** what "automatically adapted" means for `sizeZ = 0`, presumably uniform scale from X and Y. It is not stated.
- **How KiCad uses it** (**observed**, `FillFootprintModelInfo`):
  - rotation = (−rotX, −rotY, −rotZ); offset = (offX, offY, offZ) in mil converted to mm;
  - `sizeX sizeY` are kept in a hidden `JLC_3D_Size` field and the UUID in `JLC_3DModel_Q`;
  - no scale; path `${KIPRJMOD}/EASYEDA_MODELS/<3D Model Title>.step`.
  - KiCad does not apply the ORIGIN step either. It relies on EasyEDA's models already being centred with their base at Z = 0. That is an inference from the code, not a stated rule.

## 5. The model file itself

- **Not in any export** (§2–§4). The UUID points at EasyEDA's server table.
- **Formats EasyEDA holds.**
  - Pro imports "zip, step, stp, wrl, obj" with a unit choice of "cm, m, mil, inch" (default mm). Source: https://prodocs.easyeda.com/en/3d-model/file-new-3d-model/
  - Pro offline mode "only supports importing files in the following formats: (.step/.stp)". Source: https://prodocs.easyeda.com/en/faq/library/
  - Std imports WRL and OBJ only (§2).
- **Both forms on the server.** **Observed** in third-party code:
  - `easyeda2kicad` fetches OBJ text from `https://modules.easyeda.com/3dmodel/{uuid}` and STEP from `https://modules.easyeda.com/qAxj6KHrDKw4blvCG8QJPs7Y/{uuid}`. A comment says "EasyEDA always provides both OBJ (→WRL) and STEP for the same UUID".
  - `JLC2KiCad_lib` uses the same STEP URL. Its comment says the URL was found in EasyEDA's `smt-gl-engine.js` and "points to the bucket containing the step files".
  - EasyEDA documents neither endpoint, and its terms forbid automated access (#426).
  - The OBJ vertices are in mm: `easyeda2kicad` divides by 2.54 to get KiCad's 0.1 in WRL units.
- **Manual routes, by EasyEDA's documentation.**
  - Pro's device menu has "Save 3D model as: … After the operation, the 3D model associated with the selected device will be saved." Source: https://prodocs.easyeda.com/en/device/side-panel-bottom-library-device-list/ . The page does not say whether this downloads a file or saves a copy to the user's library. The neighbouring "Save symbol as" and "Save package as" save a library copy and open it. **Unverified.**
  - Pro exports a whole board as STEP or OBJ, and only parts with a bound STEP appear in the STEP (#426, https://prodocs.easyeda.com/en/pcb/export-3d-file/). A user could export a board with one part on it, but that gives the part placed in board coordinates, not a separate model file. Not tried.

## 6. Open-source readers

| Reader | Reads | 3D handling | Licence | Status |
|---|---|---|---|---|
| KiCad `pcbnew/pcb_io/easyeda`, `eeschema/sch_io/easyeda`, `common/io/easyeda` | Std `.json`, `.zip` (board, footprint, symbol, schematic) | Binding → `model` entry with offset and rotation; UUID and size in hidden fields | GPL-2.0-or-later (file headers) | Added 2023-09-07 (`21ee65aa`); in releases 8.0 to 10.0.8 |
| KiCad `…/easyedapro` (v2) | `.epro`, `.elibz`, `.zip`, single `.efoo` | Device attributes → `model` entry | GPL-2.0-or-later | Same commit; in releases |
| KiCad `…/easyedapro` (`*_v3*`) | `.epro2`, `.elibz2` | Same, reusing the v2 code | GPL-2.0-or-later | Added 2026-07-18 (`56bfdd55`); `master` only, absent from tag 10.0.8 |
| `uPesy/easyeda2kicad.py` | Std JSON from EasyEDA's API by LCSC number | Fetches OBJ and STEP by UUID; offset baked into WRL only | AGPL-3.0 | 1.8k stars; last push 2026-04-08 (`fff10a3`) |
| `TousstNicolas/JLC2KiCad_lib` | Std JSON from EasyEDA's API by LCSC number | Fetches STEP and WRL; writes offset and rotation into the footprint | MIT | 494 stars; last push 2026-09-11 (`e7815b6`) |
| `wokwi/easyeda2kicad` | Std designs (TypeScript) | not checked | MIT (`LICENSE`, "Copyright (c) 2019 Uri Shaked") | last push 2023-12-27 |

- KiCad's documentation lists the import formats: "EasyEDA / JLCEDA Standard Edition (.json or .zip files)" and "EasyEDA / JLCEDA Professional Edition (.elibz, .epro, or .zip files)". It also says "To obtain files for import, export your design from EasyEDA using its Export EasyEDA option". Source: https://docs.kicad.org/10.0/en/pcbnew/pcbnew.html
- KiCad's QA folder `qa/data/pcbnew/plugins/easyeda*/` holds real sample files, with a `sources.txt` giving each file's licence. Some are CC-BY-NC-SA 3.0 or CERN-OHL-S 2.0, and the v3 sample has no licence. These files were read for facts and not copied here.
- Licence notes against REDA's policy (ADR-0002; ADR-0017 takes contributions under MIT): the KiCad and `easyeda2kicad` code can only be read for format facts. `JLC2KiCad_lib` and `wokwi/easyeda2kicad` are MIT. The EasyEDA format specs carry no licence statement on the pages read.

## 7. Versioning

- Std: `head.editorVersion` (for example "6.5.34" in the KiCad sample, "6.3.0" in the 2020 spec) and `docType`.
- Pro v2: the first line of each file is `["DOCTYPE", <type>, <version>]` (for example "1.3" for a footprint). The index has `editorVersion`.
- Pro v3: `DOCHEAD` has `docType`, `uuid`, `client`, and in the `.elibz2` sample `version` and `updateTime`. `project2.json` has `editorVersion`, which is empty in EasyEDA's example.
- Primitive-level versions also appear. **Observed**: a v3 schematic `ATTR` has `"version":"2.0"`.

## 8. Farnell / element14 / Newark CAD downloads

- **Unverified, all of it.** The product pages, `uk.farnell.com/new-web-features` and `community.element14.com` all failed: HTTP 403 to curl, or a timeout through the fetch tool. The pages tried include `uk.farnell.com/…/dp/9589899`, `sg.element14.com`, `au.element14.com`, `export.farnell.com` and `www.newark.com/…/dp/32C1203`. The Internet Archive was offline.
- Search found no first-party statement that Farnell, element14 or Newark uses SamacSys, Ultra Librarian or SnapMagic. SamacSys's partner page (https://componentsearchengine.com/learn-more) names no distributors. Ultra Librarian's partner page names DigiKey among distributors, not Farnell.
- **What was found.**
  - A 2017 announcement, as carried by EE Journal: Premier Farnell partnered with TraceParts for "Downloadable CAD files in multiple formats", starting with 10,000 TE Connectivity products. It "provides CAD files for over 250,000 products on its" farnell.com, newark.com and sg.element14.com sites. Source: https://www.eejournal.com/industry_news/premier-farnell-launches-free-cad-library-with-traceparts/ (2017-07-07). These are mechanical CAD files, not ECAD symbols and footprints.
  - An element14 Community post is titled "Use a LibraryLoader Footprint in KiCad 7". That is SamacSys's tool, but the page could not be read.
- **Open:** which ECAD partner Farnell, element14 and Newark product pages use today, and in which formats. Checking needs a person with a browser on a product page.

## Not verified

- LCSC: what the "Open the EasyEDA Model" button offers, and whether any file can be downloaded from an LCSC product page.
- EasyEDA Pro "Save 3D model as": file download or library copy.
- Pro `sizeZ = 0` "automatically adapted": exact rule.
- Units of the Pro transform: stated nowhere by EasyEDA. Mil is observed in samples and assumed by KiCad.
- Whether a Pro `.elibz` (v2) device export holds the device's model attributes in `device.json`. KiCad's reader expects a `devices` map, but no v2 device `.elibz` sample was available. The v3 `.elibz2` sample is footprint-only.
- Std `SVGNODE` 3D fields: no EasyEDA documentation. Meanings come from three third-party readers, which disagree on offset handling (outline-centre fallback in `easyeda2kicad`, footprint-angle compensation in KiCad).
- The V2.2 PDF specs were listed, not read.
- Farnell / element14 / Newark partner and formats (§8).
