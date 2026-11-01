# Vendor part downloads: formats and terms

Research for issue #19. Question: what do the part-download services that distributors link to (SnapMagic/SnapEDA, Ultra Librarian, SamacSys / Component Search Engine, EasyEDA/LCSC) deliver, in which KiCad format versions, how can the files be fetched, and may a user import them into their own Library and commit them to a public Project repository? The answer decides the input formats of the **Import part** Command (#13, ADR-0008) and the licence warning it shows.

Sources were checked on 2026-11-01. This is a factual reading of terms pages, not legal advice. Points that could not be confirmed from a primary source are marked **Unconfirmed**. Claims marked **Observed** come from vendor-generated files that users have committed to public GitHub repositories. Those files may have been re-saved by KiCad after download, so they show what such files typically look like, not exactly what a service outputs today.

## Answer in brief

- **Formats.** SnapMagic, Ultra Librarian and SamacSys all offer a KiCad 6+ download with a `.kicad_sym` symbol and `.kicad_mod` footprint, plus older KiCad 4/5 (`.lib`/`.mod`) options. EasyEDA/LCSC offers **no** KiCad download. Its library data is EasyEDA's own JSON, served from an undocumented web API. KiCad files come only from third-party converters such as `easyeda2kicad` (AGPL-3.0).
- **Version stamps vary.** The "KiCad 6+" symbol files seen in public repositories carry `(version 20211014)` (KiCad 6) or older and claim `generator kicad_symbol_editor`. Many "6+" footprints still use the **pre-6 `(module …)` syntax with no version token**. The importer should therefore accept the KiCad 6 through 10 `.kicad_sym` grammar and both the `(module …)` and `(footprint …)` forms of `.kicad_mod`. It should not branch on `generator`.
- **Bundle contents.** A download contains one symbol, one footprint and usually a STEP model. Datasheet URL, manufacturer and MPN go into symbol properties, but each service uses its own property names (`MF`, `MP`, `MANUFACTURER_PART_NUMBER`, `Manufacturer_Part_Number`, `Mouser Part Number`, `LCSC Part`, …). The footprint's 3D-model path is often missing, or is an absolute path from the author's machine.
- **Pins and pads.** All of them follow KiCad's rule that a symbol pin's *number* matches the footprint pad's *number*. The services add no mapping table, so the importer can bind pins to pads by number.
- **Fetching.** In practice every service is manual download. SnapMagic has an API, but access is by request and aimed at partners. Ultra Librarian's terms forbid robots and automated access. SamacSys works through its Library Loader desktop helper, and Supplyframe's terms forbid automated data mining. EasyEDA has the undocumented JSON endpoints that `easyeda2kicad` uses, and its terms forbid scrapers and scripts without written permission. None of them gives a supplier-search module a sanctioned, open fetch path.
- **Terms.** All four allow using parts in your own designs. **None clearly allows committing the library files to a public repository.** SnapMagic's terms are the most explicit. Files are CC-BY-SA 4.0 with a design exception, but no more than 10 Design Files may be distributed "to or through any one location" without permission. Ultra Librarian grants no explicit licence ("all right, title and interest" stay with Cadence and its suppliers). Supplyframe/SamacSys grants view, download and print rights for "informational and internal purposes" and forbids making the content available to others. EasyEDA forbids copying, distributing or publishing site Content.
- **Recommendation for Import part.** Show a one-time licence notice. It should say that imported vendor parts stay under the vendor's terms, that the user should check those terms before committing the imported Library to a public repository, and that SnapMagic sets a 10-file limit per location. Record the source service and its terms URL with each imported element, so the notice can be repeated when the Project is exported or shared. Under ADR-0002, REDA itself must never ship, mirror or host such files, including as test fixtures.

## 1. Formats offered and KiCad versions

| Service | KiCad options | Symbol | Footprint | 3D | Source |
|---|---|---|---|---|---|
| SnapMagic (SnapEDA) | "KiCad (V6 and later)", "KiCad (V4 and V5)", "KiCad (pre V4)" | `.kicad_sym` (V6+); `.lib` (older) | `.kicad_mod` (V6+, V5); `.mod` (V4 and earlier) | STEP, where the part has one (separate download) | https://www.snapeda.com/about/import/ ; https://changelog.snapmagic.com/snapeda-now-supports-kicad-v6-native-format-251075 ; https://support.snapmagic.com/en/articles/5995733-how-to-import-into-kicad-v6-and-later |
| Ultra Librarian | "KiCAD 5.0" and "KiCAD 6.0+" | `.kicad_sym` (6.0+) | `.kicad_mod` in a `.pretty` folder | STEP | https://app.ultralibrarian.com/content/help/kicad.htm ; https://app.ultralibrarian.com/content/help/kicad-6_0.htm |
| SamacSys / Component Search Engine | KiCad, through Library Loader (V2.50 or later for `.kicad_sym`). Older guides use `.lib`/`.mod` | `.kicad_sym` (appended to a `SamacSys_Parts.kicad_sym` library) | `.kicad_mod` in `SamacSys_Parts.pretty` | STEP (`SamacSys_Parts.3dshapes`) | https://componentsearchengine.com/learn-more ; samacsys.com KiCad guide (now redirects, see below) |
| EasyEDA / LCSC | **None.** EasyEDA library parts are EasyEDA JSON | n/a | n/a | OBJ/STEP from EasyEDA's model server | https://github.com/uPesy/easyeda2kicad.py (endpoints in `easyeda2kicad/easyeda/easyeda_api.py`) |

Notes:

- **SnapMagic.** The import guide lists every target tool. For KiCad: "KiCad (V6 and later)" uses `.kicad_sym` and `.kicad_mod`. "KiCad (V4 and V5)" uses `.lib` and `.mod` (V4) or `.lib`, `.kicad_mod` and `.mod` (V5). The download is a zip, and the guide warns that "Renaming the folder can lead to footprint import issues in KiCad." The changelog post introducing V6 says "you can download native `*.kicad_sym` and `*.kicad_mod` files" (the post is undated).
- **Ultra Librarian.** The help index links separate KiCad 5.0 and KiCad 6.0+ guides. **Unconfirmed:** the "6.0+" guide still tells users to pick "the [date].lib file", which looks stale. A third-party guide (https://docs.tscircuit.com/guides/importing-modules-and-chips/importing-from-ultra-librarian) describes the current flow as "Choose the KiCad v5 or KiCad v6+ format" and lists `.kicad_sym`, `.kicad_mod` and `.stp` in the zip.
- **SamacSys.** samacsys.com pages now redirect (HTTP 301) to a Supplyframe marketing page at https://supplyframe.com/samacsys, which says only that symbols and footprints download "directly into … KiCad, Cadence, PADS, Zuken, and more". The Library Loader V2.50 requirement for `.kicad_sym` comes from the former samacsys.com KiCad guide, as indexed by search engines. **Unconfirmed** on a live primary page. The Component Search Engine "learn more" page confirms that Library Loader is "a helper application that streamlines the process of loading models from Component Search Engine into your CAD system" and that "Footprints, symbols, and 3D model downloads are all FREE."
- **EasyEDA/LCSC.** No official KiCad export of library parts was found in EasyEDA docs. **Unconfirmed:** whether EasyEDA Pro can export a single part to KiCad. Community converters (`easyeda2kicad`, AGPL-3.0, "supports KiCad v6 and newer"; `JLC2KiCadLib`) read `https://easyeda.com/api/products/{lcsc_id}/components` and write `.kicad_sym`, `.pretty/*.kicad_mod` and `.3dshapes/*.wrl|.step`. REDA must not reuse their code (AGPL). Reading EasyEDA JSON directly would be a separate importer and is out of scope for a KiCad-pair Import part.

### Version stamps actually found (Observed)

| Origin (as committed) | Header seen |
|---|---|
| SamacSys `SamacSys_Parts.kicad_sym` (deltabeard/DBGC) | `(kicad_symbol_lib (version 20210619) (generator kicad_symbol_editor)` |
| Ultra Librarian `ultralibrarian.kicad_sym` (alvarop/kicad6) | `(kicad_symbol_lib (version 20211014) (generator kicad_symbol_editor)` |
| SnapMagic `KEYSTONE_8730.kicad_sym` (osy/XG_Mobile_Station) | `(kicad_symbol_lib (version 20211014) (generator kicad_symbol_editor)` |
| easyeda2kicad (ClemensElflein/xESC) | `(kicad_symbol_lib (version 20211014) (generator https://github.com/uPesy/easyeda2kicad.py)` |
| Ultra Librarian footprint (peej/lumberjack-keyboard) | `(module CONN_S4B-ZRLFSN_JST (layer F.Cu)`, legacy syntax, no version |
| SamacSys footprint (akuker/adbuino) | `(module SOIC127P600X175-8N (layer F.Cu)`, legacy syntax, no version |
| easyeda2kicad footprint (cnlohr/cnhardware) | `(module easyeda2kicad:SMD_BD5.6-D4.1 (layer F.Cu) (tedit …)`, legacy syntax |
| SnapMagic footprint (49bitcat/nileswan) | `(footprint "…" (version 20221018) (generator pcbnew)`, possibly re-saved |

KiCad's own format docs (https://dev-docs.kicad.org/en/file-formats/sexpr-symbol-lib/index.html, https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/index.html) say "Prior to version 6, the footprint token was referred to as module", and ask third-party tools not to write KiCad's own generator names. Vendors ignore that request, so `generator` cannot be used to tell where a file came from. KiCad reads both footprint forms. See `docs/research/kicad-formats-and-licences.md` on branch `research/kicad-formats-and-licences` for the grammar.

**Implication.** The importer's minimum input set is a `.kicad_sym` with version 20210619 or later and a `.kicad_mod` in either legacy `module` or modern `footprint` syntax. Accepting `.lib`/`.mod` (KiCad 5 and earlier) is not needed, because every service offers a 6+ option.

## 2. What a download bundles

- **Symbol, footprint, 3D.** One symbol and one footprint per part. A STEP model where the service has one. SnapMagic's KiCad guide points to a separate article for 3D models (https://support.snapmagic.com/en/articles/5995733-how-to-import-into-kicad-v6-and-later). Ultra Librarian's 6.0+ guide has the user attach the STEP by hand through Footprint Properties → 3D Models (https://app.ultralibrarian.com/content/help/kicad-6_0.htm). So the footprint's `(model …)` reference is often missing. **Observed:** SamacSys footprints contain an absolute Windows path to `SamacSys_Parts.3dshapes\<part>.stp`. The importer should look for a STEP/WRL file next to the footprint in the zip, not trust the path.
- **Symbol-to-footprint link.** The symbol's `Footprint` property names the footprint, in the form `<nickname>:<name>` or just `<name>`. **Observed:** SnapMagic writes `8730:KEYSTONE_8730`, with the part number as the library nickname. SamacSys writes just `CLXC8T245QRHLRQ1`. Ultra Librarian writes just `SK9822_ADA`. The importer should pair the symbol with the footprint in the same download, not resolve the property.
- **Part data fields** (Observed, all as symbol `property` entries):
  - SnapMagic: `MF`, `MP`, `MANUFACTURER_PART_NUMBER`, `Package`, `Description`, `Check_prices`, `SnapEDA_Link`, `Purchase-URL`, `PARTREV`, `STANDARD`, `MAXIMUM_PACKAGE_HEIGHT`, `SNAPEDA_PN`. Parts that SnapMagic sourced from Ultra Librarian also carry `COPYRIGHT "Copyright (C) 2018 Accelerated Designs. All rights reserved"`.
  - SamacSys: `Datasheet`, `Description`, `Height`, `Mouser Part Number`, `Mouser Price/Stock`, `Manufacturer_Name`, `Manufacturer_Part_Number`.
  - Ultra Librarian: minimal fields, plus `ki_fp_filters`. Its footprints carry `(fp_text user "Copyright 2021 Accelerated Designs. All rights reserved." … (layer Cmts.User))`.
  - easyeda2kicad: `Datasheet` (LCSC PDF URL), `Manufacturer`, `MPN`, `LCSC Part`.
  - The importer should map known aliases to REDA Part fields (manufacturer, MPN, datasheet, supplier part numbers) and keep the rest as free-form properties. It must **preserve copyright fields**, which also helps the licence notice.
- **Pins and pads.** KiCad's rule: "Symbol pins are matched to footprint pads by number. The pin number in the symbol must match the corresponding pad number in the footprint." (KiCad 9 Schematic Editor manual, https://docs.kicad.org/9.0/en/eeschema/eeschema.html). None of the services adds a separate pin map. **Observed:** pin `(number "1")` matches `(pad 1 …)` (or `(pad "1" …)`). The same pad number can repeat for exposed or thermal pads, as in the easyeda2kicad footprint. Numbers are strings (BGA balls such as `A1`). The importer should bind by string equality and report pins with no pad and numbered pads with no pin.

## 3. How files can be fetched

| Service | Manual | Programmatic | Source |
|---|---|---|---|
| SnapMagic | Part page → Download Symbol and Footprint → KiCad (V6 and later), login required | "SnapMagic Search API": free and Premium tiers, **access on request**, aimed at manufacturers, distributors and plugins. No public docs | https://www.snapeda.com/get-api ; ToS §5.1(a) forbids uploading Design Files elsewhere |
| Ultra Librarian | Part page → choose format → zip, login required | **No public API found.** Terms: "You may not use any robot or other automated means to access or gather content from the Website." | https://www.ultralibrarian.com/legal |
| SamacSys / CSE | componentsearchengine.com or partner site → zip; Library Loader desktop helper watches downloads and adds them to the KiCad library | **No public API found.** Supplyframe ToS §4.2(e) forbids automated means to "strip or mine data" | https://componentsearchengine.com/learn-more ; https://intelligence.supplyframe.com/terms/ |
| EasyEDA / LCSC | Only inside EasyEDA (no KiCad file) | Undocumented JSON endpoints (`easyeda.com/api/products/{lcsc}/components`) used by community tools. Terms forbid "any robot, spider, scraper, crawler, script or other automated means to access, scrape, download" without written permission | https://easyeda.com/legal ; easyeda2kicad source |

**Implication for a later supplier-search module.** No service offers an open, sanctioned URL scheme. A supplier-search module can at most deep-link the user to the part page, then hand the downloaded zip to Import part. Automated fetching would need a partner agreement (SnapMagic API) or written permission (EasyEDA).

**Unconfirmed:** distributor-specific routes, for example DigiKey or Mouser embedding these services with their own terms. Distributor pages were not checked. When a distributor hosts the download, its terms may also apply.

## 4. Terms of use: own Library and public repository

| Service | Use in own designs | Commit library files to a public repo | Source |
|---|---|---|---|
| SnapMagic | Yes. §4.1: Design Files licensed under **CC-BY-SA 4.0** plus "Design Exception 1.0" (§4.2), "for commercial and personal purposes free of charge". Boards and combinations may be conveyed "under terms of your choice" without attribution | **Limited.** §4.2: "any files shared publicly containing Design Files are still subject to the Site License restriction of 5.1.(g)". §5.1(g): may not "distribute more than 10 Design Files to or through any one location" without written permission. §5.1(a) forbids distributing "by uploading Design Files to another website or application" | https://www.snapeda.com/about/terms/ (undated; © 2013–2026) |
| Ultra Librarian (Cadence) | Implied only. The site offers free downloads, but the legal page **grants no explicit licence** | **Not granted.** "The software and any content on the Website are proprietary property of Cadence or third parties … [who] are the sole and exclusive owners of all right, title and interest". Footprints embed "Copyright … Accelerated Designs. All rights reserved." | https://www.ultralibrarian.com/legal (undated; © 2026 Cadence) |
| SamacSys / CSE (Supplyframe, Siemens) | §4.3: "non-exclusive, non-transferable license to view, download and print Service Content", "solely for informational and internal purposes". Use in designs is implied by the service's purpose, not stated | **Not granted.** §4.3: "you may not distribute or sell, rent, lease, license or otherwise make the Service Content available to others", nor "modify or alter the Service Content". §4.7: all rights reserved. The licence is revocable | https://intelligence.supplyframe.com/terms/ (linked as "Terms of Service" from the componentsearchengine.com footer; undated) |
| EasyEDA / LCSC | §V: software for "internal business or personal purposes" | **Not granted** for site Content. §VII: may not "copy, distribute, transmit, display, perform, reproduce, publish" Content. Downloads are for "personal, informational, and noncommercial purposes only". Also forbids using Content for ML/AI training. User-posted public projects are a separate case (§VI), and "Nothing in this clause affects your rights in respect of an individual project under its applicable open-source license" | https://easyeda.com/legal (effective 2026-09-21) |

Reading:

- **Private use is fine everywhere.** Importing into a user's own Library on their own machine, and making boards, Gerbers and BOMs from it, is the intended use of every service. Only SnapMagic says so explicitly.
- **Public repositories are the risk.** A KiCad or REDA project commits its Library or cache alongside the design. That is distribution of the library files, not just of a design.
  - **SnapMagic:** OK up to 10 Design Files per repository. **Uncertain:** whether a symbol, a footprint and a 3D model count as one Design File or three. Also uncertain whether CC-BY-SA attribution is then required for the files themselves; §4.2 waives it only for the combinations.
  - **Ultra Librarian, SamacSys, EasyEDA:** no permission to redistribute. Committing such parts publicly is at the user's own risk. **Uncertain:** whether a footprint embedded in a design file (as KiCad boards embed footprints) is a "design" rather than redistributed content. Only SnapMagic's design exception addresses this.
- **Manufacturer-hosted downloads** (for example on a manufacturer site using Ultra Librarian) may carry the manufacturer's own extra licence. This was not checked.
- **Disclaimers.** All four disclaim accuracy ("AS IS"; Ultra Librarian: "IT IS YOUR RESPONSIBILITY TO VERIFY ANY INFORMATION BEFORE RELYING ON IT"). The import notice can repeat this.

## 5. Consequences for Import part

1. **Inputs.** A `.kicad_sym` (KiCad 6+, `version` ≥ 20210619, any `generator`) and a `.kicad_mod` (legacy `module` or modern `footprint`), ideally taken from the vendor zip together with any `.step`/`.stp`/`.wrl` in it. If the `.kicad_sym` holds several symbols, as SamacSys's accumulating library does, the user picks one.
2. **Pairing.** Bind symbol pins to footprint pads by number string. Warn about unmatched pins and pads.
3. **Part fields.** Map known MPN, manufacturer, datasheet and supplier aliases. Keep the remaining properties, including any copyright line.
4. **Provenance.** Store the source service (from field names, copyright text or the zip name) and its terms URL on the imported elements.
5. **Licence notice.** One-time text, roughly: "Parts from vendor download services remain under that service's terms. Most do not allow redistributing their files; SnapMagic allows at most 10 per location. Check the terms before committing this Library to a public repository." Repeat it when a Project containing such elements is exported or shared.
6. **ADR-0002.** No vendor-downloaded file, raw or converted, in REDA's repo, test fixtures, releases or any REDA-run service. Test fixtures must be hand-written.
