# 3D model sources for electronic parts: formats, footprint binding, licences

Research for issue #426, part of the v0.3 map (#421). Question: where would a REDA user get 3D models for parts, in which formats, how is each model tied to a footprint (and does an offset or rotation come with it), under what licence, may REDA ship it in a library, and is there an API or only a manual download? Each source is checked against [ADR-0002](../adr/0002-permissive-licensing-and-third-party-data.md). The note also says which of REDA's existing Import part sources already carry a model that is ignored today.

All sources were read on 2026-11-23 unless a line says otherwise. This is a factual reading of licence and terms pages, not legal advice. It records facts and recommends nothing. Markers used below:

- **Unverified** means the primary page could not be reached or does not state the fact. The reason is given each time.
- **Observed** means the fact comes from reading a file or source code, not from a statement by its owner.
- **From #19** means the fact was established in the earlier note `docs/research/vendor-part-downloads.md` on branch `research/vendor-part-downloads` (read 2026-11-01) and was not re-read here.

## Answer in brief

- **Format.** STEP is the one format every source delivers. KiCad's own library dropped its VRML (`.wrl`) copies at the 10.0.0 tag and is STEP only. EasyEDA / LCSC serves OBJ and STEP. Ultra Librarian also offers STL. The MCAD portals (3D ContentCentral, GrabCAD) carry many native MCAD formats.
- **Binding.** Only two sources hand over a model that is already placed on a footprint with a stored transform: KiCad's library (a `model` entry in the footprint with offset, scale and rotation) and EasyEDA (a model UUID plus a transform on the footprint). SnapMagic and Ultra Librarian put a STEP file beside the footprint and tell the user to attach it by hand. SamacSys's Library Loader writes a model path into the footprint. Manufacturer and MCAD-portal downloads are bare model files with no footprint link at all.
- **Licence.** No source in the list offers models under a licence ADR-0002 allows REDA to bundle. KiCad's library is CC-BY-SA 4.0 with a design exception that does not cover redistributing the library itself. Every vendor service and manufacturer either forbids redistribution or grants no licence to redistribute. Use in the user's own design is the intended use everywhere.
- **Access.** KiCad's library and Würth's library are plain Git repositories. Everything else is a manual, mostly logged-in, download. SnapMagic has an API by request. DigiKey's product API can filter on "has 3D model" and has a media endpoint, but no model download was confirmed in it. EasyEDA's endpoints are undocumented and its terms forbid scripted access.
- **Import part.** All four vendor routes that Import part reads today (SnapMagic, Ultra Librarian, SamacSys, and EasyEDA parts converted by a third-party tool) can carry a model, and a footprint from KiCad's own library always names one. Import part drops all of them.

## Summary table

| Source | Model formats | Tied to a footprint? | Offset / rotation delivered? | Licence, and may REDA ship it? | Access |
|---|---|---|---|---|---|
| KiCad `kicad-packages3D` | STEP (VRML too up to 9.x) | Yes: `(model …)` in each library footprint | Yes: `offset`, `scale`, `rotate` | CC-BY-SA 4.0 with design exception. A redistributed collection stays CC-BY-SA. Not shippable under ADR-0002 | Public Git repository and KiCad's installers. No API needed |
| Manufacturer sites (sampled: TI, Würth, Samtec) | STEP mostly; varies | Only when the manufacturer publishes an EDA library (Würth's KiCad library does) | Würth: yes, in its KiCad footprints. Bare downloads: no | Each manufacturer's own terms. TI: non-commercial or personal, no redistribution. Würth: use, modify, share modified; property stays Würth's. Not shippable | Manual download. Würth: public Git repository |
| DigiKey | Whatever the partner delivers (STEP) | Through the partner's download | As the partner | Partner's terms, plus DigiKey's site terms (not read, see Unverified) | Product page link to Ultra Librarian or SnapMagic. API has `Has3DModel` filter and a media endpoint |
| Mouser | As SamacSys (STEP) | Through SamacSys | As SamacSys | Supplyframe terms | Product page ECAD link and Library Loader. No model in the API confirmed |
| LCSC / EasyEDA | OBJ and STEP from EasyEDA's model server | Yes: model UUID on the footprint | Yes: offset and rotation on the footprint | EasyEDA terms forbid copying and publishing Content. Not shippable | Inside EasyEDA. Undocumented endpoints; terms forbid scripts |
| SnapMagic (SnapEDA) | STEP | File in the same zip; attached by hand | No: the user rotates it to fit | CC-BY-SA 4.0 plus design exception, at most 10 Design Files per location, none inside other software. Not shippable | Manual, logged in. API by request |
| Ultra Librarian | STEP, STL | File in the same zip; attached by hand | None in the file, but the vendor says the model shares the footprint's origin and rotation | No licence granted; all rights reserved by Cadence and suppliers. Not shippable | Manual, logged in. Robots forbidden. No public API found |
| SamacSys / Component Search Engine | STEP | Library Loader writes a path into the footprint | Not confirmed | "Informational and internal purposes"; may not make content available to others. Not shippable | Manual plus Library Loader. Automated queries forbidden |
| 3D ContentCentral | Many MCAD formats | No | No | **Unverified** (terms page unreachable) | Manual, logged in |
| GrabCAD | Whatever the uploader posted | No | No | Personal, non-commercial use per GrabCAD's help pages; terms page unreachable. Not shippable | Manual, logged in |

## 1. KiCad's 3D model library

- **What it is.** The repository `kicad-packages3D` "contains 3D models for rendering and MCAD integration". Models are generated by script (FreeCAD with CadQuery) or modelled by hand, with sources in a second repository, `kicad-packages3D-source`. Source: https://gitlab.com/kicad/libraries/kicad-packages3D/-/blob/master/README.md
- **Formats.** The README still says the library "supports two file formats" but lists only STEP. **Observed** through the GitLab API on one folder, `Resistor_SMD.3dshapes`: tags `8.0.0` and `9.0.0` hold 40 `.step` and 40 `.wrl` files, tag `10.0.0` and `master` hold 40 `.step` files and no `.wrl`. Only that one folder was counted. Source: `https://gitlab.com/api/v4/projects/kicad%2Flibraries%2Fkicad-packages3D/repository/tree?path=Resistor_SMD.3dshapes&ref=<tag>`
- **What KiCad itself can attach.** "Footprint models can be in STEP, VRML, or IDF format." A model can also be embedded in the footprint or board file and referred to by a `kicad-embed://` reference. Source: https://docs.kicad.org/10.0/en/pcbnew/pcbnew.html (sections "3D models" and "Embedding files")
- **Binding.** Each library footprint names its model. **Observed** in `Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod` on `master`:

  ```
  (model "${KICAD10_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0603_1608Metric.step"
    (offset (xyz 0 0 0))
    (scale (xyz 1 1 1))
    (rotate (xyz 0 0 0))
  )
  ```

  Source: https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/master/Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod
  - The path starts with a versioned path variable (`KICAD10_3DMODEL_DIR` today). KiCad resolves a variable from an older version to the current one when the old one is not defined. `${KIPRJMOD}` (the project folder) is the other common prefix. Source: the KiCad manual page above.
  - The folder name mirrors the footprint library: `Resistor_SMD.pretty` pairs with `Resistor_SMD.3dshapes`.
  - "Many footprints in KiCad's standard library do not yet have model files created for them. However, these footprints may contain a path to a 3D model that does not yet exist". Source: the KiCad manual page above. So a `model` entry is not proof that a file exists.
- **Transform.** A footprint can hold several `model` entries. Each has an offset, a scale and a rotation per axis, plus optional `hide` and `opacity`.
  - The published format page documents the position token as `at`, and gives no units. Source: https://dev-docs.kicad.org/en/file-formats/sexpr-intro/index.html (section "Footprint 3D Model")
  - **Observed** in KiCad's source, the writer emits `offset`, and the reader accepts both: a comment says "Prior to KiCad v5, model offset was designated by "at", and the units were in inches. Now we use mm, but support reading of legacy files", and it multiplies `at` values by 25.4. Source: https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr_parser.cpp (`parse3DModel`) and `pcb_io_kicad_sexpr.cpp` in the same folder. KiCad is GPL, so this is read for the file format only (ADR-0002).
  - The library's README says STEP models "must be 1:1 in mm" and that "the 3D model scaling parameter must always be set to 1:1".
  - **Observed** in the same writer on `master`: a second form, `(model (type extruded) … (overall_height …) (body_pcb_gap …) (layer …) (material …) (color …))`, describes a body extruded from footprint geometry with no model file. Which release first ships it was not checked.
- **Licence.** "The KiCad libraries are licensed under the Creative Commons CC-BY-SA 4.0 License, with the following exception: To the extent that the creation of electronic designs that use 'Licensed Material' can be considered to be 'Adapted Material', then the copyright holder waives article 3 of the license with respect to these designs and any generated files which use data provided as part of the 'Licensed Material'." And: "if you wish to redistribute the KiCad libraries, or parts thereof (including in modified form) as a collection then the exception above does not apply. Redistributed library collections must be shared under the same license agreement." Sources: https://gitlab.com/kicad/libraries/kicad-packages3D/-/blob/master/LICENSE.md and https://www.kicad.org/libraries/license/
  - Individual folders carry a `CREDITS.md` (seen in `Resistor_SMD.3dshapes`); its contents were not read.
  - The library's contributing notes say of manufacturer models: "In most cases these models are licensed under an incompatible license", and that accepting one needs "the explicit consent by the current license holder". Source: https://github.com/KiCad/packages3D/wiki/Contributing (the old GitHub wiki; no dated revision shown)
- **Against ADR-0002.** This is the case the ADR names: KiCad library data may be read when the user has it, and is not bundled, because CC-BY-SA with the exception would bind REDA's distribution. A STEP export of a user's board that contains these models is a "generated file" under the exception.
- **Access.** A public Git repository, also installed with KiCad's packages. No login, no API needed.

## 2. Manufacturer downloads

There is no common pattern, so three manufacturers were sampled. Molex and TE Connectivity terms pages could not be reached (timeout and HTTP 404), so they are **Unverified**.

- **Texas Instruments.**
  - TI's product pages offer "CAD/CAE symbols" per package through Ultra Librarian. Source: https://e2e.ti.com/support/logic-group/logic/f/logic-forum/1027781/faq-where-can-i-get-a-cad-symbol-soldering-footprint-or-3d-model-for-my-device (a TI forum FAQ, seen only as a search result summary, not opened). So TI's models carry the Ultra Librarian traits in section 6.
  - Terms: TI grants permission to "download, reproduce, display, and distribute TI Services" "solely for non-commercial or personal use", provided they are not modified and notices are kept. Otherwise nothing "may be reproduced, duplicated, mirrored, modified" without TI's written consent. Effective 2025-03-27. The page does not name CAD models; service-specific terms take precedence where they exist. Source: https://www.ti.com/legal/terms-conditions/terms-of-use.html
- **Würth Elektronik.**
  - Würth publishes its own KiCad library on GitHub: `symbols`, `footprints` (`*_Wurth.pretty`) and `3dmodels` (`*_Wurth.3dshapes`). **Observed:** the one model folder listed, `Crystal_SMD_Wurth.3dshapes`, holds 35 `.step` files and nothing else. Sister repositories exist for Altium, Eagle and Cadence. Last push 2026-11-18. Source: https://github.com/WurthElektronik/KiCad-Library
  - **Observed** binding, in `footprints/Crystal_SMD_Wurth.pretty/Y_Wurth_WE-SPXO-CFPS-102.kicad_mod`: `(model "${WE_3DMODEL_DIR}/Crystal_SMD_Wurth.3dshapes/Y_Wurth_WE-SPXO-CFPS-102.step" (offset (xyz 0 0 0)) (scale (xyz 1 1 1)) (rotate (xyz 0 0 0)))`. The same KiCad mechanism, with Würth's own path variable.
  - Licence (`License_Terms_WE_KiCad_library.pdf`, undated): a "limited, non-exclusive, non-transferable, non- sublicensable, royalty-free license". "You shall be allowed to download and save the Models to your device. You shall be allowed to create designs with the Models and share those designs. You shall be allowed to modify the Models. You shall be allowed to share the modified Models." "No more extensive rights to use and exploit the Models are granted to you." Modified models must be marked as modified and attribute Würth. "The Models created and provided by WE eiSos are and will remain the exclusive property of WE eiSos." Source: https://github.com/WurthElektronik/KiCad-Library/blob/master/License_Terms_WE_KiCad_library.pdf
  - Sharing an unmodified model is not among the allowed uses, and the licence cannot be sublicensed, so it cannot be passed on under REDA's MIT licence.
- **Samtec.** Models are downloaded from the product page after accepting a disclaimer: "Samtec provides the model(s) "AS IS" and does not guarantee or warrant that it is completely accurate", "By downloading these files, you agree that the use of these files and their content IS AT YOUR OWN RISK". The page fetched does not list formats or state a redistribution rule. Source: https://www.samtec.com/products/2sn
- **Binding.** A model downloaded from a manufacturer's product page is a bare file with no footprint and no transform. It gets a footprint link only when the manufacturer publishes a full EDA library (Würth) or delegates to a part-data service (TI).
- **Against ADR-0002.** Third-party data under terms that are not permissive licences. Readable when the user has it, not bundled.
- **Access.** Manual download, apart from manufacturers with a public repository.

## 3. DigiKey

- **What it delivers.** DigiKey hosts no model library of its own for this. A product page has an "EDA / CAD Models" row that links to a partner download:
  - Ultra Librarian: the link "will take you to the Ultra Librarian for DigiKey page". "Formats for download include Altium, Cadence, Eagle, KiCAD, Mentor, Pulsonix, Quadcept, Zuken and 3D STEP Model." "Currently DigiKey is offering 15 free downloads per month to anyone who has a DigiKey login." Source: https://forum.digikey.com/t/eda-cad-model-downloads/586 (DigiKey staff post dated 2017-09-27, so the limit may be out of date)
  - SnapMagic: "over 1.5 million verified and high-quality symbols, footprints, and 3D models for Digi-Key parts", reached from the "EDA / CAD Models" row. Source: https://www.digikey.com/en/news/press-releases/2020/nov/digi-key-releases-1-5-million-snapeda-cad-models (2020-11-18)
- **DigiKey's own KiCad library.** `Digi-Key/digikey-kicad-library` holds `digikey-symbols` and `digikey-footprints.pretty` and no 3D model folder. It "is licensed under a CC-BY-SA 4.0 license (with an exception)". Last push 2024-03-16. Source: https://github.com/Digi-Key/digikey-kicad-library
- **API.** **Observed** in the Product Information V4 OpenAPI file:
  - The keyword search accepts the search options `Has3DModel` and `HasCadModel`.
  - `GET /search/{productNumber}/media` ("Retrieve all media for a given product") returns `MediaLinks` entries with `MediaType`, `Title`, `SmallPhoto`, `Thumbnail` and `Url`. `MediaType` is a free string with no listed values.
  - The `Product` object has `DatasheetUrl`, `PhotoUrl` and `PrimaryVideoUrl` and no model field.
  - Source: https://developer.digikey.com/products/product-information-v4/productsearch (Swagger download at `https://developer.digikey.com/node/2387/oas-download`). The API needs an OAuth client.
  - **Unverified:** whether the media endpoint returns model links, and whether such a link is a file or a partner page. This needs a call with real credentials.
- **Licence.** The partner's terms apply to the files (sections 5 and 6). **Unverified:** DigiKey's own site terms (https://www.digikey.com/en/terms-and-conditions returned HTTP 403), which the OpenAPI file names as the API's terms of service.
- **Against ADR-0002.** Same as the partner behind the link.

## 4. Mouser

- **What it delivers.** Mouser's ECAD downloads come from SamacSys: "PCB footprints, schematic symbols, and 3D models", installed into the user's ECAD tool by Library Loader. This rests on search result summaries of https://www.mouser.com/en/electronic-cad-symbols-models/ and of Mouser's 2018 press release. **Unverified** directly: every mouser.com page tried returned HTTP 403 or timed out.
- **Formats, binding, licence.** As SamacSys, section 7.
- **API.** **Unverified.** Mouser's Search API page (https://www.mouser.com/api-search/) could not be reached, so whether it returns any model field is not known.

## 5. LCSC / EasyEDA

- **What it is.** LCSC parts carry EasyEDA library data. In EasyEDA a 3D model is its own library element, bound to a footprint.
- **Formats.**
  - EasyEDA Pro imports models as "zip, step, stp, wrl, obj", with a unit choice. Source: https://prodocs.easyeda.com/en/3d-model/file-new-3d-model/
  - EasyEDA Pro exports a board in STEP and OBJ only, and "The exporting Step format must be the 3D model binding of the PCB device and is also STEP format", otherwise the part is missing from the STEP. Source: https://prodocs.easyeda.com/en/pcb/export-3d-file/
  - **Observed** in the third-party converter `easyeda2kicad` (commit `fff10a3`): the model is fetched by UUID from `https://modules.easyeda.com/3dmodel/{uuid}` as OBJ text and from a second `modules.easyeda.com` path as STEP. A code comment says "EasyEDA always provides both OBJ (→WRL) and STEP for the same UUID". Source: https://github.com/uPesy/easyeda2kicad.py/blob/master/easyeda2kicad/easyeda/easyeda_api.py . EasyEDA does not document these endpoints.
- **Binding and transform.**
  - EasyEDA Pro's file format puts two attributes on the footprint: `3D Model`, a UUID, and `3D Model Transform`, "the transformation parameters required for the 3D model to match the device" when placed on the top layer at 0,0 with rotation 0. The fields are `sizeX`, `sizeY`, `sizeZ`, `rotZ`, `rotX`, `rotY`, `offX`, `offY`, `offZ`, and the matrix is `OFFSET × ROT × SCALE × ORIGIN`: move the model's X/Y midpoint and lowest Z to the origin, scale it to the target size, rotate in Z-X-Y order, then offset. The page gives no units. Source: https://prodocs.easyeda.com/en/format/pcb/component/
  - EasyEDA Standard's 3D Model Manager binds a model to a footprint with width and height, X and Y, and rotation about each axis. It mentions "user-contributed 3D models". Source: https://docs.easyeda.com/en/PCB/3D-Model-Manager/
  - **Observed** in `easyeda2kicad`: in the Standard-format data the binding is an `SVGNODE` entry with `uuid`, `title`, `c_origin`, `z` and `c_rotation`. The converter bakes the offset into the WRL vertices, writes the rotation into the KiCad footprint with a zero offset, and copies the STEP untouched; a comment notes the STEP "is misplaced when model_3d.translation != (0,0,0)". Source: `easyeda2kicad/easyeda/easyeda_importer.py`, `kicad/export_kicad_3d_model.py` and `kicad/export_kicad_footprint.py` in the repository above. The converter is AGPL-3.0, so it is a reference for facts only.
- **Licence.** EasyEDA's terms (effective 2026-09-21): "You may not modify any of the materials and you may not copy, distribute, transmit, display, perform, reproduce, publish" Content; downloads are "for personal, informational, and noncommercial purposes only"; no "robot, spider, scraper, crawler, script or other automated means to access, scrape, download". The terms do not mention libraries or 3D models by name. User-posted public projects keep their own licence. Source: https://easyeda.com/legal
  - **Unverified:** LCSC's own terms page (https://www.lcsc.com/agreement/terms-and-conditions returned navigation text only).
- **Against ADR-0002.** Not bundled. Automated fetching is also against the service's terms, which is a separate matter from the ADR.
- **Access.** Inside EasyEDA. No documented public API.

## 6. SnapMagic and Ultra Librarian

### SnapMagic (SnapEDA)

- **Format.** STEP. "the step file will be included in the zip folder if the 3D model is available" when the symbol and footprint are downloaded. Source: https://support.snapmagic.com/en/articles/3758108-how-to-import-a-3d-model-in-kicad
- **Binding.** By hand. The same article has the user open the footprint's properties, pick the `.stp` file, and rotate the model until it fits the footprint. It mentions no offset or scale. SnapMagic's general import page gives 3D steps only for PADS ("Adjust alignments as necessary"). Source: https://www.snapeda.com/about/import/
- **Licence.** "Design Files" are "all digital models, schematics, layouts, blueprints, and other computer-aided design ("CAD") files distributed to or by SnapMagic"; the terms never say "3D model". They are licensed under CC-BY-SA 4.0 with a design exception for boards made with them. The user may not "distribute or share any parts of the Site, Design Files, or Site Materials within any other software" (5.1(f)) nor "distribute more than 10 Design Files to or through any one location" (5.1(g)) without written permission. Automated access is forbidden (5.1(j)). Undated. Source: https://www.snapeda.com/about/terms/
- **Against ADR-0002.** Clause 5.1(f) rules out bundling in REDA directly, before the ADR is even consulted.
- **Access.** Manual download, logged in. The API "provide[s] free and Premium versions" on request and is aimed at distributors, manufacturers and assemblers. The page describes it as "giving instant access to symbols & footprints"; whether it serves 3D models is **Unverified**. Source: https://www.snapeda.com/get-api/

### Ultra Librarian

- **Formats.** "3D step models download as .stp or .stl format." Source: https://app.ultralibrarian.com/content/help/3d_step_model.htm
- **Binding.** By hand in KiCad: the 6.0+ guide has the user open Footprint Properties, the 3D Models tab, and browse to the downloaded STEP file. It gives no offset or rotation step. Source: https://app.ultralibrarian.com/content/help/kicad-6_0.htm
- **Transform.** Ultra Librarian says none is needed: "Since the 3D models are dynamically created from footprint information, the rotation, origin, and pins are going to be the same as the original footprint", with "no need to mate the two". The models are generated from footprints built with its template system, "which means about 85% of footprints are covered by 3D models". Source: https://www.ultralibrarian.com/about/standards
- **Licence.** No licence is granted. "The software and any content on the Website are proprietary property of Cadence or third parties", who are "the sole and exclusive owners". "You may not use any robot or other automated means to access or gather content from the Website." Undated, © 2026. Source: https://www.ultralibrarian.com/legal
- **Against ADR-0002.** Not bundled.
- **Access.** Manual download, logged in, directly or through DigiKey and manufacturer sites. No public API was found.

## 7. SamacSys / Component Search Engine

- **Format.** STEP (**From #19**). The service's own page says only "Footprints, symbols, and 3D model downloads are all FREE" and supports "over 24 CAD systems". Source: https://componentsearchengine.com/learn-more
- **Binding.** Library Loader is "a helper application that streamlines the process of loading models from Component Search Engine into your CAD system". **From #19** (Observed): for KiCad it keeps models in `SamacSys_Parts.3dshapes` and writes an absolute Windows path to the `.stp` file into the footprint.
- **Transform.** **Unverified.** Whether the written `model` entry carries a non-zero offset or rotation was not checked on a real file.
- **Licence.** Supplyframe's terms grant "a non-exclusive, non-transferable license to view, download and print Service Content", "solely for informational and internal purposes"; "you may not distribute or sell, rent, lease, license or otherwise make the Service Content available to others"; "you may not modify or alter the Service Content". Section 4.2(e) forbids scripts that "generate automated searches, requests and queries, or to strip or mine data". Undated, © 2026. Source: https://intelligence.supplyframe.com/terms/
- **Against ADR-0002.** Not bundled.
- **Access.** Manual download or Library Loader, directly or from Mouser and other partner sites. No public API was found.

## 8. 3D ContentCentral and GrabCAD

These are general MCAD model portals, not part-data services. KiCad's library README lists both, with TraceParts and the FreeCAD library, as "External Model Libraries" and adds: "Ensure that the LICENSE of any 3D data used is compatible with the intended use-case of the project." A model from either is a bare file with no footprint link and no transform.

- **3D ContentCentral (Dassault Systèmes).**
  - **Unverified** throughout. The site's terms page (https://www.3dcontentcentral.com/Terms-of-use.aspx) timed out on every attempt, and the SolidWorks help page about it returned only navigation. A search result summary says the terms limit Content to "internal computer-aided design use" and that the site separates supplier catalogues from a user library; neither statement was read at its source.
  - A separate contributor agreement exists: https://www.3dcontentcentral.com/includes/Attachments/Terms_and_Conditions_for_Contributions.pdf (not read).
- **GrabCAD (Stratasys).**
  - The terms page (https://grabcad.com/terms) returned HTTP 403, so the licence text is **Unverified**. A search result summary describes a "Cross License" giving each user a non-exclusive licence to use and modify submissions for "non-commercial, internal use", not sublicensable or transferable.
  - GrabCAD's help pages were readable: "all models on GrabCAD are free for personal use"; "charging for GrabCAD models is not allowed"; "Please do not upload the exact same model that someone else has already posted." Source: https://help.grabcad.com/article/149-community-values-and-guidelines
  - Models are "generally for private use"; public non-commercial showing needs attribution; commercial use needs the designer's explicit permission. Source: https://help.grabcad.com/article/246-how-can-models-be-used-and-shared (as summarised by the fetch tool, not quoted in full)
  - Formats are whatever the uploader posted. Whether GrabCAD or 3D ContentCentral offers a public API was not checked, so that is **Unverified** too.
- **Against ADR-0002.** On what could be read, neither is a permissive licence, and each model has a different owner. Not bundled.

## 9. Check against ADR-0002

ADR-0002 says REDA "may read third-party library data and formats … that users obtain themselves, but does not bundle or redistribute that data", and that bundled material must be under MIT, Apache-2.0, BSD, zlib or ISC (MPL-2.0 case by case).

| Source | Licence or terms found | Permissive under ADR-0002? | Stricter than the ADR needs? |
|---|---|---|---|
| KiCad library | CC-BY-SA 4.0 + design exception | No (share-alike on the collection) | No: reading and using is free |
| Würth KiCad library | Own licence, non-sublicensable | No | No |
| TI site | Non-commercial or personal use, no modification | No | The non-commercial wording is the manufacturer's, and binds the user, not REDA |
| Samtec | "AS IS" disclaimer; no grant read | No grant found | n/a |
| SnapMagic | CC-BY-SA 4.0 + exception, plus site limits | No | Yes: forbids sharing Design Files "within any other software" |
| Ultra Librarian | No licence; all rights reserved | No | Forbids automated access |
| SamacSys | Internal use; no making available to others | No | Forbids automated queries |
| EasyEDA / LCSC | No copying or publishing of Content | No | Forbids scripted access |
| 3D ContentCentral | Unverified | Unverified | Unverified |
| GrabCAD | Personal, non-commercial (help pages) | No | Unverified |

No source in the ticket's list offers models that ADR-0002 would let REDA bundle. Every one of them can be read from files the user obtained, which is what the ADR allows.

## 10. Import part sources that carry a model ignored today

Import part (`reda-import-kicad`, [library spec](../spec/v0.1/library.md#import-part)) takes one `.kicad_sym` and one `.kicad_mod`. The spec says "3D models are ignored", and the crate's module comment repeats it; the footprint's `model` entry is used only as a hint for recognising the source service (`crates/reda-import-kicad/src/provenance.rs`, at commit `965ee047`). The spec lists manual downloads from SnapMagic, Ultra Librarian and SamacSys as the inputs, and the crate also recognises EasyEDA parts converted by a third-party tool.

| Input to Import part | Model that comes with it | Where it is | Transform that comes with it |
|---|---|---|---|
| SnapMagic zip | STEP, when the part has one | A `.stp` file in the zip. The footprint's `model` entry is often missing (**From #19**) | None. SnapMagic's guide has the user rotate it to fit |
| Ultra Librarian zip | STEP (or STL) | A STEP file in the zip. The guide has the user attach it by hand | None stored. The vendor states the model shares the footprint's origin and rotation |
| SamacSys via Library Loader | STEP | `SamacSys_Parts.3dshapes`, named by an absolute Windows path in the footprint (**From #19**) | Not confirmed |
| EasyEDA / LCSC part converted by `easyeda2kicad` | WRL and STEP | A `.3dshapes` folder beside the `.pretty` folder, named in the footprint's `model` entry | Rotation in the `model` entry, offset zero. The offset is baked into the WRL only |
| A footprint from KiCad's own library, or Würth's | STEP | Named in the `model` entry through a path variable (`${KICAD10_3DMODEL_DIR}`, `${WE_3DMODEL_DIR}`) | `offset`, `scale`, `rotate` in the `model` entry |

In every row the `.kicad_mod` that Import part already parses either names a model or sits next to one, and nothing of it reaches the REDA Footprint. The reserved Footprint field `bodies` (v0.3) and the existing `height` are the only model-related fields in REDA today ([reserved names](../spec/v0.1/architecture.md#reserved-names)).

## Unverified, in one place

- **3D ContentCentral:** terms of use, formats, and the supplier versus user-library split. The site timed out.
- **GrabCAD:** the terms text itself (HTTP 403). Only help pages were read. Whether it, or 3D ContentCentral, has a public API.
- **Mouser:** every mouser.com page, including the Search API description (HTTP 403 or timeout).
- **DigiKey:** site terms (HTTP 403); whether the media endpoint returns model links (needs credentials); the current free-download limit for Ultra Librarian files.
- **LCSC:** its own terms page. EasyEDA's terms were read instead.
- **Molex, TE Connectivity:** terms pages not reached. **Samtec:** formats and any redistribution rule.
- **TI:** the product-page CAD download flow, known only from a search summary of a TI forum FAQ.
- **SnapMagic:** whether its API serves 3D models; whether a symbol, footprint and model count as one Design File or three (open since #19).
- **SamacSys:** whether Library Loader writes an offset or rotation; the STEP format statement on a live first-party page.
- **KiCad:** the Library Convention rule on 3D models (klc.kicad.org returned HTTP 403); the `.wrl` removal was counted in one folder only; the release that first ships extruded `model` bodies; the units of `rotate` (degrees is assumed from the dialog, not read in a source).
- **EasyEDA:** units of the `3D Model Transform` fields; whether every system-library model has a STEP form (stated only in a third-party code comment).
