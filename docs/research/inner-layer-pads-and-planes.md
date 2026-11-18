# Through-hole Pads, Vias and planes on inner Layers

REDA research, ticket #323. Sources read on **2026-11-18**. Research run on the Codex CLI and reviewed by a Claude agent, which spot-checked six claims against their sources. The KiCad comparison uses the 10.0 PCB Editor manual and the 10.0.8 stable release. [KiCad release notes][k-release]

Three decisions affect a plated hole on an inner Layer: whether it connects to the surrounding copper, whether it retains a copper ring when unconnected, and how far unrelated copper stays from the hole. KiCad uses positive Zones for ground and power. The leading suite also has a negative internal plane Layer, where drawing a line removes copper. A second commercial suite supports both representations and can store a separate relief and antipad for each Layer. An unused ring can be removed in the editor, during output, or by the fab. Those operations can produce similar artwork while leaving different design data. [KiCad PCB Editor][k] · [Internal plane Layers][planes] · [Pad stack application note][stacks] · [Fabrication output options][outputs] · [Fab data preparation][euro-cam]

## 1. Relief spokes and direct connections

A relief leaves a gap around a same-Net Pad and bridges it with copper spokes. The spokes limit heat flow during soldering. A solid connection joins the surrounding copper directly. A no-connect setting leaves the object electrically separate from the Zone on that Layer even when the Net matches. [KiCad Zone settings][k] · [Connection rules][rules]

| Tool and copper representation | Through-hole Pads and Vias | Where the choice applies |
|---|---|---|
| The leading suite, positive Zone | Both support relief, solid and no connection. The advanced `PolygonConnect` rule has separate settings for through-hole Pads, surface-mount Pads and Vias. | Rules can target a Pad class or selected named Zones. An individual Pad or Via can override the rule on a selected signal Layer in its pad stack. The same object can therefore be solid on one inner signal Layer and relieved on another. [Connection rules][rules] · [Pad and Via properties][objects] · [Zone manager][polygons] · [Pad-class example][pad-class] |
| The leading suite, negative plane Layer | Both use `PlaneConnect`. Its simple mode treats them alike. Advanced mode separates Pads from Vias. The choices are relief, direct and no connection. | Higher-priority queries can select a particular component Pad, Vias of a given diameter, or all Vias on a named Net. The documented no-connect example disconnects the object from every applicable plane Layer. An independent override on each of two same-Net negative Layers was not verified. [Internal plane Layers][planes] · [Connection rules][rules] |
| KiCad, positive Zone | Through-hole Pads support solid, relief and no connection. A Zone can also relieve only through-hole Pads while connecting surface-mount Pads solidly. An eligible same-Net Via normally joins a solid-filled Zone directly. **Hatched Zones are an exception.** Since 10.0.1, their Via connections respect the Zone setting, including solid and relief. | Separate Zones on two inner Layers can give a Pad different connections. A Pad normally inherits from its Footprint, which inherits from the Zone. A Pad can override style, gap and width. Custom rules can specify `zone_connection`, `thermal_relief_gap` and `thermal_spoke_width` with a Layer clause and object, Net or Net Class conditions. The manual describes these constraints for Pads. It gives no equivalent per-Via relief switch for an ordinary solid fill. [PCB Editor and custom rules][k] · [10.0.1 release notes][k-hatch] · [Zone filler source, behaviour read only, not in the manual][k-fill] |
| A second commercial suite | Positive dynamic copper shapes have separate controls for through-hole Pads, surface-mount Pads and Vias. They support solid contact, spokes or no connection. Negative artwork uses the Layer’s pad stack relief figure for a connected hole and its antipad for an isolated hole. Film output also offers a full-contact option. | On positive shapes, an object override outranks the individual shape’s settings, which outrank board-wide settings. Each shape belongs to a Net and Layer. A version 17.2 distributor guide shows Pad and Via overrides by individual Layer or Layer group. [Layout guide, chapter 3][layout] · [Pad stacks][stacks] · [Film controls][film] · [Layer-specific settings, secondary distributor guide][dynamic-layers] |
| Fusion Electronics and EAGLE | A positive Zone can use reliefs or solid connections. A board setting enables reliefs for Vias. With it off, Vias connect solidly. A Footprint Pad can forbid relief with `NOTHERMALS`. | A Zone on each Layer can have its own setting. The EAGLE manual distinguishes the per-Zone switch, the Via setting and the library Pad flag. Fusion also exposes spoke width as a Zone property. [Multilayer routing][fusion] · [EAGLE manual, Supply and Polygon sections][eagle] · [Fusion Zone properties][fusion-props] |

**Spoke counts and dimensions**

- **The leading suite.** A negative relief takes two or four spokes, a spoke width, an air gap and a ring expansion measured outwards from the hole. Positive Zones also offer automatic spoke placement based on the Pad outline. The vendor’s historical rule reference gives negative defaults of **four spokes, 10 mil width, 10 mil gap and 20 mil expansion**. It gives four spokes and 10 mil width for positive Zones. The current properties guide illustrates four spokes, 10 mil width and a **15 mil polygon gap** as an example. It does not identify that gap as the current factory default. [Current connection rules][rules] · [Historical vendor rule reference, public mirror][old-rules] · [Current properties example][objects]
- **KiCad.** The illustrated Zone settings use a **0.5 mm relief gap and 0.5 mm spoke width**. The usual Pad pattern has four possible spokes. The illustrated board DRC minimum is **two connected spokes**. These counts answer different questions: the filler can attempt four connections even where surrounding copper allows only two. A custom Pad can also specify individual spoke locations. [Illustrated PCB Editor manual][k-pdf] · [Four-spoke rule example, from the rules help text in source][k-spokes]
- **A second commercial suite.** Positive shapes support minimum and maximum counts, up to four axial or diagonal spokes or eight for the combined pattern. Width can come from the Net’s physical constraint or a fixed value. A historical support answer identifies solid contact as the Via default for positive shapes in the version discussed. [Shape settings, secondary distributor note][shape-note] · [Historical support answer, secondary][via-default]
- **EAGLE.** For a positive polygon, the documented spoke width starts at half the Pad drill diameter and is bounded between one and two polygon line widths. The thermal isolation setting supplies the gap. The older negative supply-Layer manual describes the usual relief as four bridges. [EAGLE polygon dimensions][eagle] · [EAGLE 5 supply-Layer manual][eagle5]

## 2. Removing unused inner-Layer copper

| Tool | Scope and effect |
|---|---|
| The leading suite | A board editor command removes, restores or updates unused shapes for Pads, Vias or both, across the board or selection. It can retain the first and last rings. Repouring Zones recovers the space. A later Track connection to a removed ring can trigger annular-ring DRC. Separately, Gerber and Gerber X2 export can omit unconnected middle-Layer Pads without editing the board. [Editor command][unused] · [Fabrication output options][outputs] |
| KiCad | This is a saved Pad or Via setting. Rings can remain on every Layer, only Layers with a Track or Zone connection, or connected Layers plus the two ends. A bulk tool changes or restores selected objects or the board. A Via’s separate **start and end only** mode also prevents intermediate Zone connections, even where a same-Net Zone is available. [Via properties and Removing unused pads][k] · [Saved Pad properties][k-format] |
| A second commercial suite | Dynamic suppression works in the design database by eligible inner Layer and object kind. It restores a ring as a connection is made. The pad stack must permit suppression, and an exception can preserve a Net, component, Pad or Via. The documented mode excludes outer Layers, negative plane Layers and the end Layers of blind or buried Vias. With dynamic suppression off, individual artwork films can suppress unconnected Pads at output. [HDI guide, Dynamic Unused Pad Suppression][hdi] · [Fabrication tutorial, p. 81][film] |
| Fusion Electronics | The board’s annular-ring design settings can reduce non-functional rings for both Pads and Vias. The vendor describes this as a design rule. A support report also describes removal of inner rings. A per-object exception and the same control in standalone EAGLE were not verified. [Vendor release blog, secondary][fusion-unused] · [Support report, secondary][fusion-report] |

**Fab practice differs.** Eurocircuits says its inner-Layer CAM preparation removes non-functional Pads to reduce internal shorts and improve plated-hole reliability. OSH Park describes boards on which suppressing Via rings let copper get too close to the drill. It lists retaining the rings or imposing a larger inner-Layer clearance as ways to meet its service limits, and its changelog records a check, added in June 2024, for Vias that pass through inner Layers without a ring. [Eurocircuits data preparation][euro-cam] · [OSH Park four-Layer service][osh4]

## 3. Inner-Layer annular rings and antipads

For a concentric round hole, radial ring width is half the difference between copper diameter and the specified hole diameter. An antipad is the **opening** in otherwise continuous copper. A clearance of 0.20 mm from the edge of a 0.30 mm hole gives a 0.70 mm opening. A clearance of 0.20 mm from the edge of a retained 0.60 mm Pad gives a 1.00 mm opening. The reference diameter matters in both CAD and fabrication. [Plane clearance definition][rules] · [Fab definition using the production drill][euro-ring]

| Tool | Inner copper geometry and isolation |
|---|---|
| The leading suite | Pads and Vias support a common size, top / middle / bottom groups, or a full stack with individual signal Layers. Positive Zones clear different-Net copper under electrical clearance rules. Removing an unused shape allows clearance to the hole. On a negative Layer, `PlaneClearance` measures radially from the **hole edge**. For a relief, `PlaneConnect` expansion sets the copper ring between the hole and the start of the air gap. [Pad and Via stacks][objects] · [Signal-Layer Zones][polygons] · [Plane rules][rules] · [Unused shapes][unused] |
| KiCad | Normal, front / inner / back and custom modes give Pads and Vias common, grouped or individual Layer geometry. `annular_width` checks the copper ring. Copper clearance measures separation to retained copper. `hole_clearance` checks the drilled edge against different-Net copper even where a ring is absent. Zone fill takes the larger applicable Zone and electrical clearance. [PCB Editor, pad stacks and constraints][k] |
| A second commercial suite | A pad stack can hold regular copper, a thermal-relief figure and an antipad on each conducting Layer. On negative artwork the plotter uses the relief or antipad figure. Positive shapes normally generate voids using spacing constraints and can also use pad stack relief or antipad sizing. The HDI guide distinguishes hole-to-metal checks when the ring is absent from Pad-based checks when copper remains. [Pad stack application note][stacks] · [Layout guide][layout] · [HDI guide][hdi] |
| Fusion Electronics and EAGLE | Inner Pad and Via copper is round. EAGLE’s ring rule uses a fraction of drill diameter bounded by a minimum and maximum. Pads have top, inner and bottom settings. Vias distinguish inner from outer. Positive Zones maintain spacing to other Nets under the clearance rules. [Copper shapes][fusion-shapes] · [EAGLE annular-ring settings][eagle] · [Multilayer clearances][fusion] |

### Published fabrication limits

The table gives design dimensions for the named service. A general multilayer ring value is identified where the fab does not publish a separate inner-Layer minimum. PTH means a plated component hole where the fab distinguishes it from a Via.

| Fab and service | Radial annular ring | Inner-Layer hole-to-unrelated-copper clearance |
|---|---|---|
| JLCPCB, multilayer FR-4 | For 1 oz multilayer PTH Pads, **0.15 mm absolute minimum**, 0.20 mm preferred. For Vias, the published copper/hole diameter difference of 0.10 mm, preferably 0.15 mm, gives **0.050 mm**, preferably **0.075 mm**, radial by calculation. These are multilayer values. | **0.20 mm from a Via hole and 0.30 mm from a PTH Pad hole.** The capabilities page has separate inner-Layer rows for the two objects. [JLCPCB capabilities][jlc] |
| PCBWay, four and six Layers | The general minimum ring is **0.15 mm** for a centered Via. A separate inner-Layer copper-ring minimum was not verified. | Before compensation, the normal process calls for **7 mil** on four Layers and **8 mil** on six. The high-difficulty band starts at 5 mil and 6 mil respectively. PCBWay’s “isolation ring” measures copper-free space here. [PCBWay capabilities, detailed process item 13][pcbway] |
| OSH Park, four- and six-Layer prototype services | Both list a minimum ring of **4 mil (0.1016 mm)** without a separate inner value. | Both list **10 mil (0.254 mm)** from plated drill to inner copper, including where the Via has no ring. [Four-Layer service][osh4] · [Six-Layer service][osh6] |
| AISLER, four-Layer ENIG | The 35 µm service lists **0.100 mm for Vias** and **0.300 mm for PTH Pads**. The 70 µm service lists **0.150 mm** and **0.300 mm** respectively. Neither table gives a separate inner ring. | The service tables give copper-to-copper and non-plated-hole limits. In a 2024 four-Layer support answer, a fab representative specified **0.265 mm from the outside of the plated Via barrel** after inner ring removal. A current service-table minimum from the plated hole edge was not verified. [35 µm service][aisler35] · [70 µm service][aisler70] · [Fab support clarification, posts 16–19][aisler-support] |
| Eurocircuits, standard pooling pattern class 6 | **0.125 mm** inner ring, measured using the production drill diameter. The fab illustrates a 0.60 mm Pad around a 0.25 mm finished plated hole drilled at 0.35 mm. | **0.200 mm** from the unconnected production-hole edge to copper in this class. Its rule is inner ring plus 0.075 mm, with a 0.200 mm floor. [Class 6 worked example][euro-ring] · [Pattern classification][euro-class] |

## 4. Positive Zones and negative plane Layers

Gerber file polarity describes what the image means to the fab. On positive copper artwork a round flash represents copper. On negative copper artwork the same flash represents a clearance. A negative plane file can therefore consist chiefly of edge pullback, isolation openings and relief patterns. Gerber X2 can declare this with `.FilePolarity,Negative`. Clear features inside an otherwise positive image do not make the whole file negative. Ucamco attributes the historical use of negative planes to vector photoplotter limits and recommends positive copper output today. OSH Park asks for positive inner-Layer files. [Gerber specification, §5.6.4][gerber] · [OSH Park polarity examples][polarity]

- **The leading suite** chooses Signal or Internal Plane in the Stackup editor. A signal Layer can carry Tracks and positive Zones. A negative internal plane Layer starts with copper up to its pullback and defines voids in it. Its documentation favors a signal Layer with a Zone when several other connections need routing on the same Layer. [Stackup editor][stackup] · [Internal plane Layers][planes]
- **KiCad** uses the Zone tool for both a local copper area and a whole-Layer power or ground region. Plotting emits the filled copper. A Layer identified as power can carry the Gerber file function `Plane` while retaining **positive** copper polarity. [PCB Editor][k] · [Gerber plotter source, behaviour read only, not in the manual][k-plot]
- **A second commercial suite** supports positive dynamic shapes and negative plane artwork. Its layout guide describes the trade-off. Positive shapes recalculate clearances around nearby objects. Negative films can be smaller and faster to generate, but a given pad stack’s antipad on that Layer has a fixed size across spacing-rule contexts. The guide favors positive shapes where spacing rules are complex. [Layout guide, chapter 3][layout] · [Copper-plane training, lesson 6][split-training]
- **EAGLE and Fusion Electronics** show a change over time. EAGLE 5 displayed and output a dedicated `$GND`-style supply Layer inverted. EAGLE 6 retired that special meaning and converted old supply Layers into same-Net polygons when loading a board. Fusion documents ordinary inner copper Layers with positive Zones. [EAGLE 5 manual][eagle5] · [EAGLE 6 migration notes][eagle6] · [Fusion multilayer routing][fusion]

## 5. Split planes and Tracks crossing a split

| Tool | How the split is made | What is checked |
|---|---|---|
| The leading suite | On a negative Layer, draw a closed boundary or a line joining opposite pullbacks. Its width is the copper-free separation. Assign a Net to each resulting region. Nested regions are possible. On a positive Layer, use separate Zones and give the desired one precedence where they overlap. | Batch DRC can report broken planes, isolated copper and starved reliefs. Separately, a Return Path rule checks reference copper above or below a scoped Track along its length. A split or narrow reference region can fail this check on either positive copper or a dedicated plane Layer. [Split construction and batch checks][planes] · [Zone order][polygons] · [Return Path rule][return] |
| KiCad | Draw one positive Zone per Net on the same Layer. The highest-priority Zone fills first. Lower-priority Zones clear its filled copper. | DRC checks connectivity, clearance and inadequate relief spokes. The manual also documents reference Layers for impedance tuning. A built-in DRC for a Track crossing a split on an adjacent reference Layer was not verified. [PCB Editor, Zones, tuning profiles and DRC][k] |
| A second commercial suite | Positive regions can be separate Net-assigned shapes. The split command can also use an anti-etch line across the routing boundary. Its width sets the gap. Select the target Layer and assign each resulting shape a Net. The training covers positive and negative splits. | The negative-plane island check detects copper isolated by antipads or reliefs. Separately, return-path DRC can mark the signal Track where its assigned adjacent Layer lacks the specified reference Net. [Split-plane training][split-training] · [Vendor-hosted DRC example, secondary][second-drc] |
| Fusion Electronics | Draw a positive Zone for each supply Net. Rank 1 has highest priority and rank 6 lowest. A Zone Cutout can reserve a common copper-free region. | DRC compares equal-rank polygons. Recomputing the Zones updates the geometry. An adjacent-reference split check was not verified. [Fusion multilayer routing][fusion] |

## 6. Tracks routed on a plane Layer

**The leading suite.** A Track drawn on a negative internal plane Layer cuts a slot. It cannot carry a Net as an ordinary Track. The documented workaround for a single connection is to enclose a narrow copper region and assign it the Net. A positive Zone on a signal Layer permits ordinary Tracks. This is a consequence of the artwork geometry, regardless of whether the editor displays a warning. [Tracks on internal plane Layers][planes]

**KiCad.** The signal, power plane, mixed and jumper designations guide the designer. All four accept Tracks and Zones. Selecting power does not by itself produce a routing warning, impose a Keepout or change polarity. The designation is stored in the board and can appear in manufacturing metadata. A designer who wants a restriction can apply a Layer-scoped `disallow track` rule. [Board Setup and custom rules][k] · [Board file format][k-board] · [Gerber Layer attributes, from plotter source, not the manual][k-plot]

**A second commercial suite** distinguishes a conducting routing Layer from a plane Layer in the Stackup and selects negative artwork separately. A distributor’s guide to the automatic router states that it skips Layers typed as plane. A universal interactive-routing warning was not verified. **Fusion Electronics** permits ordinary routing on inner copper carrying Zones. In legacy EAGLE, the manual instructed users to exclude an automatically generated negative supply Layer from autorouting. [Stackup quick start, secondary][cross-section] · [Automatic-router inputs, secondary][router] · [Fusion routing][fusion] · [EAGLE 5 manual][eagle5]

## 7. What four- to six-Layer hobby boards actually use

- **Four Layers.** The HomeDeck designer documents a KiCad board with **signal / GND / power / signal**. The power Layer has separate regions for the input supply, USB, peripherals and 3.3 V. Small Vias in staggered rows take signals away from the compute-module connectors. [Designer’s HomeDeck build record][home]
- **Six Layers.** The QMX amateur-radio kit designer kept two Layers entirely for ground. Layer 4 mainly carries power connections with a few signal Tracks. Spare area on all six Layers is ground copper, stitched near signal Vias. The documented Stackup shows through Vias from top to bottom. OSH Park’s four- and six-Layer prototype services likewise support through Vias and exclude blind and buried Vias. These are examples of actual practice, without a population estimate for either Stackup. [Designer’s QMX presentation][qmx] · [Four-Layer service][osh4] · [Six-Layer service][osh6]

## Sources

- **KiCad.** [10.0 PCB Editor manual][k], [illustrated manual][k-pdf] and [board format][k-board]. Three claims rest on KiCad's GPL source as browsed on its Doxygen site ([Zone filler][k-fill], [Gerber plotter][k-plot], [rules help text][k-spokes]); they describe behaviour only and copy no code, and they are weaker than the manual.
- **The leading suite.** [Plane Layers and splits][planes], [signal-Layer Zones][polygons], [plane and Zone Connection rules][rules], [Pad and Via properties][objects], [unused Pad removal][unused] and [output options][outputs].
- **A second commercial suite.** [Vendor pad stack application note][stacks], [vendor fabrication tutorial][film], [layout guide][layout], [HDI guide][hdi] and [split-plane training][split-training]. The last three are vendor-authored documents read on public mirrors. Distributor guides and support discussions are identified where used.
- **Fusion Electronics and EAGLE.** [Multilayer editor guide][fusion], [EAGLE manual][eagle] and [version 6 migration notes][eagle6]. The older manuals and migration notes are vendor documents, including copies hosted by distributors.
- **Manufacturing and boards.** [Gerber specification][gerber], [JLCPCB][jlc], [PCBWay][pcbway], [OSH Park][osh4], [AISLER][aisler35], [Eurocircuits][euro-class], [HomeDeck][home] and [QMX][qmx].

## Not verified / open

- For the leading suite, the public manual establishes per-Layer **positive** Zone Connection overrides. An equivalent for one multilayer object on two same-Net **negative** plane Layers remains unverified. Current factory spoke widths and gaps were not specified numerically in the readable rule text. Historical defaults and illustrated values are identified separately above. [Connection rules][rules] · [Pad and Via properties][objects]
- For a second commercial suite, a current universal factory spoke count, width and gap, and a manual Track-placement warning across editor editions, remain unverified. For Fusion, a current factory spoke count and gap and an individual exemption from board-wide unused-ring removal remain unverified. [Layout guide][layout] · [Fusion multilayer guide][fusion] · [Fusion release blog, secondary][fusion-unused]
- PCBWay’s inner-only copper-ring minimum and AISLER’s current service-table clearance from a plated **hole edge** remain unverified. AISLER’s support number uses the outer barrel surface. [PCBWay capabilities][pcbway] · [AISLER service rules][aisler35] · [AISLER support clarification][aisler-support]

[k-release]: https://www.kicad.org/blog/2026/10/KiCad-10.0.8-Release/
[k]: https://docs.kicad.org/10.0/en/pcbnew/pcbnew.html
[k-pdf]: https://docs.kicad.org/10.0/en/pcbnew/pcbnew.pdf
[k-hatch]: https://www.kicad.org/blog/2026/04/KiCad-10.0.1-Release/
[k-fill]: https://docs.kicad.org/doxygen/zone__filler_8cpp_source.html
[k-spokes]: https://docs.kicad.org/doxygen/panel__setup__rules__help__9more_examples_8h_source.html
[k-format]: https://dev-docs.kicad.org/en/file-formats/sexpr-intro/
[k-board]: https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/
[k-plot]: https://docs.kicad.org/doxygen/pcbplot_8cpp_source.html
[planes]: https://www.altium.com/documentation/altium-designer/pcb/internal-power-split-planes
[rules]: https://www.altium.com/documentation/altium-designer/pcb/design-rule-types/plane
[objects]: https://www.altium.com/documentation/altium-designer/pcb/pads-vias
[polygons]: https://www.altium.com/documentation/altium-designer/pcb/signal-layer-polygons
[pad-class]: https://www.altium.com/documentation/knowledge-base/altium-designer/applying-polygon-connect-style-rule-to-component-pads-by-pad-class
[old-rules]: https://www.scribd.com/document/324057125/Altium-TR0116-Design-Rules-Reference
[unused]: https://www.altium.com/documentation/altium-designer/pcb/removing-unused-pads-adding-teardrops
[outputs]: https://www.altium.com/documentation/altium-designer/preparing-for-manufacture/output-jobs/fabrication-data
[stackup]: https://www.altium.com/documentation/altium-designer/pcb/defining-layer-stack
[return]: https://www.altium.com/documentation/altium-designer/pcb/design-rule-types/high-speed
[stacks]: https://community.cadence.com/cfs-file/__key/communityserver-discussions-components-files/137/MakePadstackInAllegroSkill.pdf
[layout]: https://www.scribd.com/document/484488891/algrolay
[film]: https://www.cadence.com/content/dam/cadence-www/global/en_US/documents/solutions/cadence-cloud/oncloud/orcad-tutorial.pdf
[dynamic-layers]: https://www.graser.com.tw/img/news/news_product/news_product_v172/Cadence_Allegro_172_2016.pdf
[hdi]: https://www.scribd.com/document/765690123/high-density-interconnect-hdi
[shape-note]: https://www.parallel-systems.co.uk/wp-content/uploads/2020/02/Shape_Settings.pdf
[via-default]: https://community.cadence.com/cadence_technology_forums/pcb-design/f/pcb-design/17286/help-with-thermal-reliefs
[split-training]: https://www.jotrin.com/userfiles/downloadfile/201812111754279229.pdf
[second-drc]: https://community.cadence.com/cadence_technology_forums/pcb-design/f/allegro-x-pcb-editor/59231/return-path-and-its-importance-in-pcb-design
[cross-section]: https://www.flowcad.de/AN/PCB_Editor_Quickstart_EN.pdf
[router]: https://www.ema-eda.com/ema-resources/product-guide/x-ai/setup-required-design-inputs-routing/
[fusion]: https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-ROUTE-MULTILAYER-BOARD-CPT.htm
[fusion-props]: https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/electron_PolyPour.htm
[fusion-unused]: https://www.autodesk.com/products/fusion-360/blog/march-2024-product-update-whats-new/
[fusion-report]: https://forums.autodesk.com/t5/fusion-electronics-forum/ecad-fixes-in-the-upcoming-july-update/td-p/12892675
[fusion-shapes]: https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-DESIGN-PREFERENCES-SHAPES.htm
[eagle]: https://forums.autodesk.com/autodesk/attachments/autodesk/fusion-electronics-forum-en/8948/1/manual_en.pdf
[eagle5]: https://www.multi-circuit-boards.eu/fileadmin/user_upload/downloads/EAGLE-Manual_5.pdf
[eagle6]: https://cdn.sparkfun.com/Homepage/Release_notes_V6.pdf
[gerber]: https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf
[polarity]: https://docs.oshpark.com/troubleshooting/inverted-gerbers/
[osh4]: https://docs.oshpark.com/services/four-layer/
[osh6]: https://docs.oshpark.com/services/six-layer/
[jlc]: https://jlcpcb.com/capabilities/pcb-capabilities
[pcbway]: https://www.pcbway.com/capabilities.html
[aisler35]: https://community.aisler.net/t/4-layer-35-m-enig-design-rules/3733
[aisler70]: https://community.aisler.net/t/4-layer-70-m-enig-design-rules/5772
[aisler-support]: https://community.aisler.net/t/inner-annular-rings-required/2084
[euro-class]: https://www.eurocircuits.com/technical-guidelines/pcb-design-guidelines/classification/
[euro-ring]: https://www.eurocircuits.com/iar-inner-annular-ring/
[euro-cam]: https://www.eurocircuits.com/frontend-data-preparation/
[home]: https://kilanrou.com/projects/homedeck.html
[qmx]: https://www.qrp-labs.com/images/qmx/docs/fdim2023.pdf