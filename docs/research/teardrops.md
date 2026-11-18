# How PCB editors generate and store teardrops

Sources read on 2026-11-18.

A teardrop adds copper where a narrow Track enters a larger feature. The most consequential difference between editors appears after a routing edit. The leading suite removes the affected teardrop and leaves it to the designer to rerun the command. A second commercial suite can regenerate the affected fillet during routing. KiCad fills teardrops as small Zones, yet applies Track rules when checking their clearance. DipTrace is included because its published XML format shows both the settings that describe a teardrop and the points that describe the copper actually saved. [The leading suite’s teardrop guide][lead] · [Dynamic fillets][dynamic] · [KiCad PCB Editor][ki] · [KiCad 10.0.1 release notes][ki-drc] · [DipTrace PCB XML specification][dip-xml]

## The leading suite

**Attachments and dimensions.** The PCB editor groups round through-hole Pads with Vias and non-round through-hole Pads with surface-mount Pads. It also handles Track width changes and T-junctions. The percentage denominator changes with the attachment, so a length setting of 100% can describe quite different amounts of copper. The **Tools → Teardrops** dialog gives the following defaults. [The leading suite’s teardrop guide][lead]

| Dialog group | Default length | Default width | Reference dimension |
|---|---:|---:|---|
| Via or round through-hole Pad | 30% | 70% | Via or Pad diameter |
| Surface-mount or non-round through-hole Pad | 100% | 200% | Incoming Track width |
| Track | 100% | No separate width is specified. | Attached Track width |
| T-junction | 300% | 100% | Width of the main Track through the junction |

**Storage and edits.** Each teardrop is one solid region with straight or curved edges. `IsTeardrop` identifies those regions in object queries. The PCB Document also saves the current generation settings. Editing the attached Pad, Via, or Track removes the affected region. Rerunning **Tools → Teardrops** restores eligible sites without first removing the other teardrops. Add and Remove can operate on all eligible objects or only the selection, with independent attachment-type filters. The guide documents this selection scope but gives no persistent per-Pad, Via, Footprint, Net, or Net Class generation veto. [Teardrop guide][lead] · [Object query reference][lead-query]

**DRC and output.** The clearance rule treats a region as copper. **Adjust teardrop size** lets the generator shrink the region to satisfy applicable rules. **Force teardrops** permits placement that can still produce a DRC violation. The optional report identifies failed sites by location and Layer. Release notes record a fix for generation overlooking off-screen copper Keepouts. The resulting region contributes its shape to Gerber and ODB++ copper artwork. The X2 exporter documents Pad and Via attributes, but does not specify a teardrop attribute or a fixed Gerber command sequence for these regions. [Teardrop guide][lead] · [Electrical rules][lead-rules] · [Release notes][lead-release] · [Copper region output][lead-region] · [Fabrication output][lead-fab]

The suite also has a separate **CAM editor** command that works on selected Pads on signal Layers. It can add a Pad half the original size or two tangent lines at the Track entry. Its clearance checks require an extracted Net list. Those two constructions describe an edit to the CAM Document. The PCB editor creates the single region described above. [CAM teardrop documentation][lead-cam]

## KiCad

**Attachments and defaults.** KiCad 10 supports Vias, plated through-hole and surface-mount Pads, custom Pads, and transitions between Tracks of different widths. Arc Tracks are documented in the release notes. The manual does not identify an equal-width T-junction as a separate target. **Prefer zone connection** suppresses a teardrop when the object already connects to a Zone. It does not describe generating a flare where a Track enters the edge of a Zone. [PCB Editor][ki] · [KiCad 10.0.7 release notes][ki-arc]

Board Setup has separate defaults for round objects, rectangular objects, and Track-to-Track transitions. The 10.0 manual illustrates all three with a preferred length of **50%**, capped at **1 mm**, and a preferred width of **100%**, capped at **2 mm**. The reference is the receiving object’s width, using the smallest dimension for a Pad. The incoming Track width limit is **90%** of that reference. The illustrated settings allow a teardrop to extend onto a second Track segment and use straight edges. Both Pad-shape groups have **Prefer zone connection** enabled. These are shape defaults applied when teardrops are added. [Board Setup illustration][ki-setup] · [Parameter definitions][ki]

**Storage, edits, and scope.** A Pad or Via has its own enable flag and shape settings. The generated copper is a small Zone. KiCad’s board-format specification defines a `zone` record with a Net, Layer, `polygon` outline, and optional `filled_polygon` geometry. Thus the Document format can hold both an outline and its computed copper. This differs from REDA’s separate, rebuildable Zone-fill cache. The public format chapters reviewed do not specify the exact teardrop subtype marker or all serialized parent settings. [PCB Editor][ki] · [Zone file format][ki-format]

KiCad fills a teardrop when it is added. Subsequent unfill and refill operations include teardrops. General Zone refill is manual by default, with an automatic-refill preference. A designer can change or disable one Pad or Via in its properties, or bulk-edit by Net, Net Class, Layer, round Pads, existing teardrops, or selection. The bulk command can remove matching teardrops or remove every teardrop regardless of filters. A Net Class filter selects the objects for that operation. The manual does not describe it as a continuing exclusion for future objects. [PCB Editor, teardrops and Zone filling][ki]

**DRC and output.** Zone refill keeps the copper clear of other Nets. Since 10.0.1, DRC classifies teardrops as **Tracks for rule matching and clearance tests**. Version 10.0.7 also fixed a case where a teardrop could bridge Nets at a Pad with a thermal connection. DRC has an option to refill before checking, and Plot has an option to check and refill stale Zones before output. Disabling either can leave the check or artwork based on old copper. Gerber plotting supports X2 and optional Net attributes. The manual does not specify the primitive or attribute emitted specifically for a teardrop. [PCB Editor, DRC and plotting][ki] · [10.0.1 clearance change][ki-drc] · [10.0.7 connectivity fix][ki-arc]

## A second commercial suite

**Attachments and geometry.** The routing editor calls the added copper at a Pad, Via, or T-junction a *fillet*, and the transition between Track widths a *taper*. Its controls cover circular, square, rectangular, oblong, octagonal, and custom-shaped Pads, including undrilled Pads. An incoming arc can be filleted at a component pin or Via, and that fillet always has curved sides. A Pad covered by another Pad or by a static or dynamic copper shape suppresses the Pad fillet. For stacked Vias and Via-in-Pad connections, the vendor documents sizing from the largest Pad. [17.4 command reference][fillet] · [Vendor routing guide][dynamic]

**Defaults.** The 17.4 command reference uses angles and absolute offsets. For a straight Track entry, the desired and maximum angles both default to **90°**, with **99°** the highest configurable maximum. The default maximum offset from the Pad edge is **25 mil**, and the eligible incoming Track width range is **3–25 mil**. Arc entries have separate minimum and maximum offsets of **1 mil** and **5 mil**. The reference gives a **100 mil** default size limit for the listed standard Pad shapes. For a width-change taper it prints **60°** and a maximum offset of **635 mil**. That last value is as published for 17.4. Its value in a current installation was not verified. [Versioned command reference, pp. 57–60][fillet]

**Storage, edits, and opt-outs.** The editor stores shape-based fillets in the board database. In dynamic mode, changing a connection removes and regenerates the affected geometry. In batch mode, editing a pin, Via, or Track deletes the affected fillet until another batch run. A Net can carry `NO_FILLET`. The designer can also choose Layers in the Stackup, define a gloss Keepout, and fix or unfix individual or all existing fillets. Fixing a fillet preserves its current shape through edits. The vendor also documents older boards containing line-based fillets alongside newer shape-based ones. [Command reference][fillet] · [Dynamic routing, scope, and locking][dynamic] · [Shape-based fillet release note][fillet-drc]

For the binary representation, KiCad’s importer documentation identifies `0x28` shape records in version 17.2 and later boards and reports `0x3001` for classic teardrops and `0x1001` for dynamic Pad/Via fillets. This is an importer’s account of another tool’s file format **(secondary, not verified)**. [Importer format notes][fillet-format]

**DRC and output.** DRC treats a Pad or Via fillet as an extension of that Pad or Via. **Allow DRC** permits a conflicting shape and leaves the violation visible. Dynamic fillets affect the clearance that nearby Zones leave and can displace neighboring Track segments. A missing-fillets report identifies absent or partial fillets. For fabrication, the suite’s Gerber film setup includes the etched copper, pins, and Vias on each selected Layer. Taken together, the routing and film guides indicate that the added etch is part of that Layer’s artwork. They do not specify a per-fillet Gerber primitive or attribute. [Fillet DRC and reporting][fillet-drc] · [Dynamic routing guide][dynamic] · [Gerber film setup][fillet-fab]

## DipTrace

**Attachments and settings.** DipTrace has four independently configurable cases: through-hole Pads and Vias, surface-mount Pads, Track width changes, and T-junctions. Each can use no teardrop, straight sides, or curved sides. For through-hole Pads and Vias, length and connection width scale with Pad size. For surface-mount Pads, both scale with Track width, with connection width limited by the Pad. Track width changes and T-junctions have a length relative to Track width. The tutorial asks the reader to enter **50% length and 90% connection width** for one Net. These are exercise values. Neither the tutorial nor the XML example establishes factory defaults. [PCB Layout help][dip-help] · [PCB XML specification, §4.20.1.4][dip-xml] · [Worked tutorial][dip-tutorial]

**Storage and scope.** The XML Document stores both intent and geometry. A `<Net>` can own `<TeardropParams>` and a `<Teardrops>` list. Each `<Teardrop>` has an ID, a copper Layer, a starting coordinate, and a `<Points>` list of outline coordinates. A routed Track can identify its associated teardrops by ID in `<ConnectedTeardrops>`. This explicitly records which Track uses the saved copper. The editor can apply settings to the board, a Net, selected objects, an individual Pad or Via, or the Pads of selected Footprints. Individual Pad and Via settings take precedence over Net settings. [PCB XML specification, §§4.20.1.4–4.20.1.6][dip-xml] · [Pad, Via, and Net controls][dip-help]

**Edits, DRC, and output.** **Route → Teardrops** applies or changes settings on existing junctions. The saved Track-to-teardrop reference establishes an association, but the documentation reviewed does not say when moving a Footprint or rerouting a Track rebuilds those points. DipTrace documents object clearance checks and a separate connectivity check for broken or merged Nets. It also documents RS-274X and X2 Layer export. This review could not verify a teardrop-specific DRC classification, connectivity exemption, export switch, Gerber primitive, or X2 attribute. [Layout checks][dip-check] · [Editing and Gerber export][dip-help] · [PCB XML specification][dip-xml]

## Comparison

| Question | The leading suite | KiCad 10 | A second commercial suite | DipTrace |
|---|---|---|---|---|
| Attaches to | Pads, Vias, width changes, and T-junctions. [Guide][lead] | Pads, Vias, width changes, and arc Tracks. A Zone connection can suppress generation. [Manual][ki] · [Release notes][ki-arc] | Pads, Vias, T-junctions, and width changes. Arc fillets apply to pins and Vias. [Reference][fillet] | Through-hole Pads and Vias, surface-mount Pads, width changes, and T-junctions. [XML][dip-xml] |
| Parameters and defaults | Length and width use different references by attachment. A round Pad starts at 30% and 70% of diameter. [Guide][lead] | Preferred length and width are 50% and 100%, with 1 mm and 2 mm caps. [Board Setup][ki-setup] | Fillets use angles and offsets. The 17.4 straight-entry defaults are 90° and a 25 mil maximum offset. [Reference][fillet] | Ratios depend on the junction. Factory values are unverified. [Help][dip-help] · [Tutorial][dip-tutorial] |
| Storage | One identifiable solid region per teardrop. [Query reference][lead-query] | A small Zone. The board format supports saved fill geometry. [Manual][ki] · [Format][ki-format] | A board-database shape that can be individually fixed. [Routing guide][dynamic] | Net settings, Layer-specific outline points, and references from Tracks. [XML][dip-xml] |
| Follows edits | An attachment edit removes it. Rerun Add. [Guide][lead] | It participates in Zone refill. Automatic refill is optional. [Manual][ki] | Dynamic mode regenerates it. Batch mode requires another run. [Reference][fillet] | The command updates existing junctions. Move-time behavior is unverified. [Help][dip-help] |
| Opt-outs | Selection and attachment type scope each run. [Guide][lead] | Per-Pad and per-Via controls, plus bulk filters including Net Class. [Manual][ki] | Net property, Layer controls, spatial Keepouts, and individual locks. [Routing guide][dynamic] | An individual Pad or Via overrides its Net. [Help][dip-help] |
| DRC | Region clearance applies. Placement can shrink or be forced. [Guide][lead] · [Rules][lead-rules] | It fills as a Zone and matches Track clearance rules. [Manual][ki] · [Release notes][ki-drc] | A Pad/Via fillet extends its parent for DRC. Conflicts can be allowed and reported. [Release note][fillet-drc] · [Routing guide][dynamic] | General clearance and connectivity checks are documented. Fillet classification is unverified. [Checks][dip-check] |
| Gerber | The region contributes to Layer artwork. A separate CAM command can add a Pad or two lines. [Regions][lead-region] · [CAM][lead-cam] | The copper plot uses the Zone fill. Plot can refresh it first. [Manual][ki] | The configured copper film includes the added etch. [Reference][fillet] · [Film setup][fillet-fab] | RS-274X and X2 Layer export are documented. The teardrop encoding is unverified. [Export help][dip-help] |

## What hobbyists use

- **Four Layers.** The NEO Chess builder recorded adding teardrops to every Pad on the four-Layer controller and antenna-matrix boards. That is a first-person account of an all-Pad workflow on a specific hobby project. The log gives no shape ratios. [NEO Chess build journal][hobby-four]
- **Six Layers.** The Meko V2 builder recorded choosing a six-Layer Stackup and enabling teardrops during routing. The same entry mentions experimenting with curved Tracks, but does not identify the teardrop side style, dimensions, or enabled junction types. [Meko V2 build journal][hobby-six]
- **Fab guidance and practitioner opinion.** JLCPCB identifies narrow Tracks entering through-holes and Vias with small annular rings as use cases. In a practitioner discussion of a six-Layer board, contributors described teardrops as optional and pointed to the available annular ring when assessing drill breakout **(secondary, not verified)**. The project logs establish actual use at both Layer counts. They do not establish an adoption rate, a forum consensus, or that hobbyists usually retain KiCad’s illustrated defaults. [JLCPCB guidance][fab] · [Practitioner discussion][forum] · [KiCad defaults][ki-setup]

## Not verified and gaps

- **Track-to-Zone and arc support.** None of the four generation guides reviewed documents a standalone teardrop at a Track-to-Zone boundary. KiCad’s Zone preference and the second commercial suite’s covered-Pad behavior are both suppression cases. Arc-entry support is documented for those two tools. It remains unverified for the leading suite and DipTrace. A curved teardrop outline alone does not establish support for an incoming arc. [Teardrop guide][lead] · [KiCad manual][ki] · [Arc release note][ki-arc] · [Fillet reference][fillet] · [DipTrace help][dip-help]
- **Persistence and regeneration.** The leading suite documents its region query but not the corresponding native file field. KiCad’s public Zone grammar omits the teardrop subtype. DipTrace publishes XML geometry, but the reviewed sources do not establish an equivalent native binary or separate ASCII field layout, a remove-all command, or regeneration timing after a move. [Region query][lead-query] · [KiCad format][ki-format] · [DipTrace XML][dip-xml] · [DipTrace help][dip-help]
- **Defaults and access.** Factory side style and absolute minimum dimensions were not verified for the leading suite. Current-edition numeric defaults and license-tier coverage were not verified for the second commercial suite. Its public 17.4 command reference was readable. The linked 17.2 support-portal release notes returned **HTTP 403** and were unreadable without further access. DipTrace factory dimensions remain unverified. [Teardrop guide][lead] · [17.4 command reference][fillet] · [Restricted release notes][restricted] · [DipTrace tutorial][dip-tutorial]
- **Review re-check.** A second reader re-checked these claims against their sources on 2026-11-18 and found them supported: the leading suite's dialog defaults, removal on edit, one region per teardrop, the Adjust and Force options and the `IsTeardrop` query; KiCad's Board Setup values (from the manual's screenshot), the small-Zone and refill text, the Edit Teardrops filters and Remove All, and the 10.0.1 change to Track rules; and the JLCPCB guidance. The second commercial suite's vendor blog and community PDF returned **HTTP 403** to the second reader, and the DipTrace XML PDF could not be turned into searchable text. Those claims rest on the first reading only.
- **Artwork encoding.** Gerber defines filled regions (`G36`/`G37`), aperture flashes (`D03`), and draws or arcs. Attributes can describe conductors, including regions. An editor calling an object a “region,” “Zone,” or “fillet” is insufficient evidence of the Gerber commands or attributes its exporter emits. The reviewed exporter documentation did not establish a fixed per-teardrop sequence or dedicated X2 attribute for any of the four tools. [Gerber Layer Format Specification, §§4.8, 4.10, 5][gerber] · [Leading suite output][lead-fab] · [KiCad plotting][ki] · [Second commercial suite film setup][fillet-fab] · [DipTrace export][dip-help]

## Sources

- The leading suite — [Teardrop generation and dialog defaults][lead], [object query reference][lead-query], [electrical clearance rules][lead-rules], [public release notes][lead-release], [copper regions][lead-region], [fabrication outputs][lead-fab], and [CAM teardrop tools][lead-cam].
- KiCad — [10.0 PCB Editor manual][ki], [Board Setup illustration][ki-setup], [board-format Zone specification][ki-format], [10.0.1 release notes][ki-drc], and [10.0.7 release notes][ki-arc].
- A second commercial suite — [17.4 fillet command reference][fillet], [dynamic fillets, tapers, and edit scope][dynamic], [shape-based fillets and DRC][fillet-drc], [Gerber artwork guide][fillet-fab], and [17.2 support-portal notes, access restricted][restricted]. [KiCad’s importer documentation][fillet-format] is secondary for this suite’s binary format.
- DipTrace — [PCB Layout help][dip-help], [PCB XML specification][dip-xml], [teardrop tutorial][dip-tutorial], and [DRC and Net connectivity overview][dip-check].
- Fabrication and practice — [Ucamco Gerber Layer Format Specification][gerber], [JLCPCB teardrop guidance][fab], [NEO Chess build journal][hobby-four], [Meko V2 build journal][hobby-six], and [BeagleBoard practitioner discussion][forum].

[lead]: https://www.altium.com/documentation/altium-designer/pcb/removing-unused-pads-adding-teardrops
[lead-query]: https://www.altium.com/documentation/altium-designer/pcb/query-functions/object-type-checks
[lead-rules]: https://www.altium.com/documentation/altium-designer/pcb/design-rule-types/electrical
[lead-release]: https://www.altium.com/documentation/altium-designer/public-release-notes
[lead-region]: https://www.altium.com/documentation/altium-designer/pcb/defining-polygons-copper-regions
[lead-fab]: https://www.altium.com/documentation/altium-designer/preparing-for-manufacture/output-jobs/fabrication-data
[lead-cam]: https://www.altium.com/documentation/altium-designer/preparing-for-manufacture/cam/additional-tools
[ki]: https://docs.kicad.org/10.0/en/pcbnew/pcbnew.html
[ki-setup]: https://docs.kicad.org/10.0/en/pcbnew/images/board_setup_teardrops.png
[ki-format]: https://dev-docs.kicad.org/en/file-formats/sexpr-intro/#_zone
[ki-drc]: https://www.kicad.org/blog/2026/04/KiCad-10.0.1-Release/
[ki-arc]: https://www.kicad.org/blog/2026/10/KiCad-10.0.7-Release/
[fillet]: https://community.cadence.com/cfs-file/__key/communityserver-discussions-components-files/27/0640.GLOSS_5F00_PARAM_5F00_FILLET.pdf
[dynamic]: https://community.cadence.com/cadence_blogs_8/b/pcb/posts/teardrops-and-tapers
[fillet-drc]: https://community.cadence.com/cadence_blogs_8/b/pcb/posts/what-s-good-about-dynamic-fillets-in-allegro-pcb-editor-check-out-the-spb16-2-release
[fillet-fab]: https://resources.academic.cadence.com/pcb-design-and-analysis-resources/2024-how-to-generate-gerber-files-in-orcad-x-cadence
[fillet-format]: https://dev-docs.kicad.org/en/import-formats/allegro/index.html
[restricted]: https://support.cadence.com/apex/techpubDocViewerPage?c_version=17.2-2016&path=algPN%2FalgPN17.2-2016%2FalgPNTOC.html&xmlName=algpn.xml
[dip-help]: https://www.diptrace.com/support/tutorials/help_html/
[dip-xml]: https://www.diptrace.com/books/DipTraceXML_Pcb_En.pdf
[dip-tutorial]: https://diptrace.com/support/tutorials/tutorial_html/
[dip-check]: https://diptrace.com/diptrace-software/pcb-layout/
[gerber]: https://www.ucamco.com/files/downloads/file_en/456/gerber-layer-format-specification-revision-2024-05_en.pdf
[fab]: https://jlcpcb.com/blog/pcb-teardrop-you-should-know
[hobby-four]: https://blueprint.hackclub.com/projects/11460
[hobby-six]: https://blueprint.hackclub.com/projects/21
[forum]: https://forum.beagleboard.org/t/pcb-layout-questions-for-bbb/24084