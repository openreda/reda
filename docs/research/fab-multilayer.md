# Multilayer Stackups and Via options at prototype PCB fabs

## Answer in brief

- **Through Vias are the documented prototype option at all five fabs.** OSH Park explicitly excludes blind, buried and filled-and-plated Vias from its 4- and 6-Layer services. Aisler confirms through Vias for its 4-Layer prototype service. PCBWay’s numbered standard Stackups expressly apply to through-hole boards. JLCPCB, PCBWay and NextPCB offer blind, buried and laser-drilled Vias through HDI services with different construction rules and pricing. JLCPCB also offers free Via-in-Pad on its ordinary 6-Layer service. A filled through Via can therefore be available at a price that does not include a blind Via. [OSH Park 4-Layer service](https://docs.oshpark.com/services/four-layer/), [OSH Park 6-Layer service](https://docs.oshpark.com/services/six-layer/), [Aisler’s fabrication reply](https://community.aisler.net/t/vias-and-4-layer-pcbs/3337), [PCBWay standard Stackups](https://www.pcbway.com/multi-layer-laminated-structure.html), [PCBWay HDI](https://www.pcbway.com/hdi-pcb.html), [JLCPCB 6-Layer service](https://jlcpcb.com/6-layer-pcb), [JLCPCB HDI](https://jlcpcb.com/pcb-fabrication/hdi-pcb), [NextPCB Advanced](https://www.nextpcb.com/advanced-pcb-manufacturing-capabilities).
- **Finished board thickness and copper weight alone do not determine impedance.** On two JLCPCB 1.6 mm, 4-Layer boards with identical copper weights, the L1–L2 Dielectric is 0.2104 mm for `JLC04161H-7628` and 0.0764 mm for `JLC04161H-1080`. PCBWay also varies the pressed thickness of the same prepreg with the proportion of inner copper remaining. OSH Park publishes a distinct Stackup for each prototype service. Those differences matter for an outer USB differential pair referenced to a ground Zone on L2. [JLCPCB Stackup catalogue](https://jlcpcb.com/impedance), [PCBWay numbered constructions](https://www.pcbway.com/multi-layer-laminated-structure.html), [OSH Park 4-Layer Stackup](https://docs.oshpark.com/services/four-layer/), [OSH Park 6-Layer Stackup](https://docs.oshpark.com/services/six-layer/).
- **Copper artwork order and drill span are separate manufacturing inputs.** The first inner copper Layer is physically L2. JLCPCB recognizes first-inner filenames such as `.G1` as well as physical-Layer filenames such as `.G2L`. PCBWay asks for the actual filenames in top-to-bottom order. For HDI, JLCPCB asks for both endpoints in each drill filename, and NextPCB explicitly asks for a separate drill program for each Layer pair. Gerber X2 and XNC can encode the endpoints, but the fab instructions still apply. [JLCPCB naming guide](https://jlcpcb.com/help/article/suggested-naming-patterns), [PCBWay Layer-order field](https://www.pcbway.com/helpcenter/layers_issues/No_layer_orders_for_multi_layers_boards.html), [JLCPCB HDI file requirements](https://jlcpcb.com/pcb-fabrication/hdi-pcb), [NextPCB drill-file guide](https://www.nextpcb.com/blog/pcb-drill-file), [Ucamco Gerber specification](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf).

Research for REDA ticket #328. **All linked sources and all tables were read on 2026-11-18.** Prices are published offers or examples. No purchase or private quotation was made. The comparison covers rigid multilayer PCB fabrication. It extends the earlier 2-layer research (`research/fab-requirements` branch).

The Stackup tables run from top to bottom. `L2` is copper. `L2–L3` is the insulating interval between two copper Layers and can contain more than one Dielectric entry. Copper weight is in oz/ft². Dk is relative permittivity. A dash denotes an unverified value. A material-specific Dk frequency is given only where the fab supplies one.

## Comparison of inner-Layer rules and Via options

The table gives millimetres unless it explicitly says mil. Width and clearance are separate minima, in that order. Annular ring is the radial width of copper around a hole. Drill-to-copper clearance is the distance from the hole edge to unrelated copper on an inner Layer. A clearance around a hole in a Zone cannot be used as the annular ring of a connected Pad.

| Fab and product | Inner Track width / clearance | Outer Track width / clearance | Through drill and Via annular ring | Copper to board edge and hole to unrelated copper | Partial-span Vias | Source tables, read 2026-11-18 |
|---|---|---|---|---|---|---|
| JLCPCB rigid multilayer | 0.09/0.09 at 1 oz, 0.15/0.15 at 2 oz. No separate 0.5 oz inner rule was verified. | Same published multilayer values. The 1 oz row permits 3 mil locally in BGA fan-out. | 0.15 drill with 0.25 Pad implies 0.05 ring. Preferred drill 0.20. Preferred Pad diameter is 0.15 larger than the drill. Component PTH at 1 oz has 0.15 absolute ring and 0.20 recommended. | 0.20 routed edge, 0.40 scored edge. Inner copper clearance is 0.20 from a Via hole and 0.30 from a component PTH hole. | Separate HDI order. Laser blind 0.075–0.15. Mechanical buried normally 0.15–0.55. Blind/buried ring 0.075. | [Rigid capabilities, Drilling, Tracks and board edges](https://jlcpcb.com/capabilities/pcb-capabilities), [HDI parameters A–H](https://jlcpcb.com/help/article/hdi-pcb-capabilities-faq) |
| PCBWay Standard and Advanced | Standard summary 0.10/0.10. Advanced normal-process column, before compensation, gives **4/4 mil at 18 µm, 4/5 at 35 µm, 6/8 at 70 µm and 8/11 at 105 µm**. | Advanced normal-process values at those same thicknesses are 4/5, 5/6, 7/8 and 10/12 mil. | Standard CNC minimum 0.15, with extra charge below 0.20. Standard ring 0.15. Advanced normal **outer** Via ring is 5 mil at 18 or 35 µm. A separate inner-only connected ring was unverified. | Advanced normal routed edge 0.25. Inner hole isolation is **7 mil on 4 Layers, 8 mil on 6, 9 mil on 8**, before compensation. | Advanced HDI. Standard laser 4 mil, 3 mil by evaluation. | [Standard capabilities and Advanced rows 5, 13–17 and 19](https://www.pcbway.com/capabilities.html), [HDI drill specifications](https://www.pcbway.com/hdi-pcb.html) |
| OSH Park 4 / 6 Layers | 0.127/0.127 for both | Same | Minimum drill 0.254 on 4 Layers and 0.2032 on 6. Ring 0.1016 for both. | 0.381 from nominal board edge. Plated drill to unrelated inner copper 0.254. | Through only. No blind, buried or filled-and-plated Via-in-Pad. | [4-Layer copper and drill specifications](https://docs.oshpark.com/services/four-layer/), [6-Layer specifications](https://docs.oshpark.com/services/six-layer/) |
| Aisler 4 Layers, 35 µm or 70 µm outer | 0.125/0.125 in both cases | 0.125/0.125 at 35 µm, 0.175/0.175 at 70 µm | Minimum Via drill 0.25. Via ring 0.10 at 35 µm and 0.15 for the heavy-outer product. | 0.30 edge. NPTH to copper 0.25. Separate inner plated-hole clearance unverified. | Fab confirms through Vias on its 4-Layer prototype service. | [35 µm ENIG](https://community.aisler.net/t/4-layer-35-m-enig-design-rules/3733), [70 µm ENIG](https://community.aisler.net/t/4-layer-70-m-enig-design-rules/5772), [70 µm HASL](https://community.aisler.net/t/4-layer-70-m-hasl-design-rules/5773), [fab reply](https://community.aisler.net/t/vias-and-4-layer-pcbs/3337) |
| Aisler 6 / 8 Layers, 35 µm ENIG | 0.10/0.10 | Same | Via drill 0.20, ring 0.10. Component PTH drill 0.50, ring 0.30. | 0.30 edge. NPTH to copper 0.25. Separate inner plated-hole clearance unverified. | Through drilling is documented. No laser or buried option verified for these products. | [6+ Layer ENIG rules](https://community.aisler.net/t/6-layer-8-layer-and-more-35-m-enig-design-rules/5467) |
| NextPCB | 2.5/3 mil at 0.5 oz inner | 3/3 mil at 1 oz, 5.5/5.5 mil at 2 oz | Mechanical minimum 0.15 for boards ≤1.2 thick. Ring 3.5 mil at 1 oz, 4.5 mil at 2 oz. See the differing 4-Layer article below. | General limits are 0.20 to routed edge, 0.40 to V-score. Via/PTH/NPTH to Track 7/9/8 mil respectively. No separate inner-edge value was identified. | Advanced HDI includes stacked microvias, laser drilling down to 0.075 and any-Layer interconnection. | [Drill, clearance, copper and outline tables](https://www.nextpcb.com/pcb-capabilities), [Advanced HDI](https://www.nextpcb.com/advanced-pcb-manufacturing-capabilities) |

Three changes from the earlier 2-Layer values are directly verifiable. OSH Park’s Track width/clearance goes from 6/6 to 5/5 mil, and its ring goes from 5 to 4 mil. Aisler’s 2-Layer HASL width/clearance of 0.20/0.15 mm becomes 0.125/0.125 mm on 4-Layer 35 µm ENIG and 0.10/0.10 mm on 6+ Layers. JLCPCB’s 1 oz width/clearance goes from 0.10/0.10 to 0.09/0.09 mm. [OSH Park 2-Layer rules](https://docs.oshpark.com/services/two-layer/), [OSH Park 4-Layer rules](https://docs.oshpark.com/services/four-layer/), [Aisler 2-Layer HASL rules](https://community.aisler.net/t/2-layer-1-6-mm-35-m-hasl-design-rules/3735), [Aisler 4-Layer rules](https://community.aisler.net/t/4-layer-35-m-enig-design-rules/3733), [Aisler 6+ rules](https://community.aisler.net/t/6-layer-8-layer-and-more-35-m-enig-design-rules/5467), [JLCPCB Track rules](https://jlcpcb.com/capabilities/pcb-capabilities).

## JLCPCB

### Materials and copper

The readable catalogue selection is **1.6 mm nominal, 1 oz finished outer copper and 0.5 oz inner copper**. Its copper dimensions are 0.035 mm outer and 0.0152 mm inner. JLCPCB explains that processing reduces the inner copper from the supplier’s nominal 0.0175 mm. The prepreg dimensions below are **after pressing**. The core dimensions exclude the adjacent copper. JLCPCB specifies ±10% finished thickness at 1.6 mm. [Stackup catalogue, controlled-impedance parameters and individual constructions](https://jlcpcb.com/impedance), [lamination definitions](https://jlcpcb.com/help/article/multi-layer-pcb-standard-laminated-structures), [finished-thickness tolerance](https://jlcpcb.com/capabilities/pcb-capabilities).

| Dielectric material in the following tables | Catalogue Dk | Qualification |
|---|---:|---|
| 7628 prepreg | 4.4 | Pressed-construction value. |
| 3313 prepreg | 4.1 | The fab says that 3313 replaced 2313 with the same thickness and Dk. One named construction still uses the 2313 designation. |
| 1080 prepreg | 3.91 | Pressed-construction value. |
| 2116 prepreg | 4.16 | Pressed-construction value. |
| Core | 4.6 | The calculator guide models 4–8 Layers using Nan Ya NP-155F. The general fabrication page lists several FR-4 suppliers. |

Sources for the material table, read 2026-11-18: [JLCPCB prepreg and core Dk tables](https://jlcpcb.com/impedance), [calculator material assumptions](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator), [fabrication materials](https://jlcpcb.com/capabilities/pcb-capabilities). The catalogue does not give a frequency for these Dk values.

### Four copper Layers at 1.6 mm

The following two tables together specify every Layer and Dielectric of the 18 named constructions in the displayed selection, plus the default. In a prepreg entry, `7628/0.218` means one 7628 ply with pressed thickness 0.218 mm. A plus sign preserves physical order. For example, variant `7628A` has 7628 followed by 1080 above L2, and 1080 followed by 7628 below L3. Source for both tables, read 2026-11-18: [JLCPCB 4-Layer Impedance Control Stackup, individual named tables](https://jlcpcb.com/impedance).

| Physical position | Material | Thickness and weight |
|---|---|---|
| L1 | Finished outer copper | 0.035 mm, 1 oz |
| L1–L2 | Prepreg | Use the top gap for the selected code below. |
| L2 | Inner copper | 0.0152 mm, 0.5 oz |
| L2–L3 | FR-4 core, Dk 4.6 | Use the core thickness below. |
| L3 | Inner copper | 0.0152 mm, 0.5 oz |
| L3–L4 | Prepreg | Reverse the selected top gap, retaining each ply’s thickness. |
| L4 | Finished outer copper | 0.035 mm, 1 oz |

| JLCPCB order code | L1–L2 prepreg, top to bottom, mm | L2–L3 core, mm |
|---|---|---:|
| No requirement | 7628/0.2104 | 1.065 |
| JLC04161H-7628 | 7628/0.2104 | 1.065 |
| JLC04161H-3313 | 3313/0.0994 | 1.265 |
| JLC04161H-1080 | 1080/0.0764 | 1.265 |
| JLC04161H-7628A | 7628/0.218 + 1080/0.0764 | 0.865 |
| JLC04161H-3313A | 3313/0.107 + 3313/0.0994 | 1.065 |
| JLC04161H-1080A | 1080/0.084 + 1080/0.0764 | 1.065 |
| JLC04161H-7628B | 7628/0.218 + 7628/0.218 + 2116/0.1164 | 0.400 |
| JLC04161H-2116A | 2116/0.124 + 7628/0.218 + 2116/0.1164 | 0.600 |
| JLC04161H-2116B | 2116/0.124 + 7628/0.2104 | 0.865 |
| JLC04161H-2116C | 2116/0.124 + 1080/0.0764 | 1.065 |
| JLC04161H-1080B | 1080/0.084 + 1080/0.084 + 1080/0.0764 | 1.065 |
| JLC04161H-7628G | 7628/0.218 + 7628/0.218 + 1080/0.0764 | 0.500 |
| JLC04161H-2116 | 2116/0.1164 | 1.265 |
| JLC04161H-7628E | 7628/0.218 + 7628/0.2104 | 0.600 |
| JLC04161H-2116D | 2116/0.124 + 7628/0.2104 | 0.700 |
| JLC04161H-7628F | 7628/0.218 + 7628/0.218 + 7628/0.2104 | 0.250 |
| JLC04161H-2116E | 2116/0.124 + 2116/0.1164 | 0.865 |
| JLC04161H-7628C | 7628/0.218 + 7628/0.218 + 7628/0.2104 | 0.150 |

### Six copper Layers at 1.6 mm

The centre gap can consist of a prepreg, several prepregs, or a core between prepregs. That last construction still has six copper Layers. The **No requirement** default and `JLC06161H-3313` have different centre gaps. Source for both tables, read 2026-11-18: [JLCPCB 6-Layer Impedance Control Stackup, individual named tables](https://jlcpcb.com/impedance).

| Physical position | Material | Thickness and weight |
|---|---|---|
| L1 | Finished outer copper | 0.035 mm, 1 oz |
| L1–L2 | Prepreg | Top gap in the selected row below. |
| L2 | Inner copper | 0.0152 mm, 0.5 oz |
| L2–L3 | FR-4 core | Core column below. |
| L3 | Inner copper | 0.0152 mm, 0.5 oz |
| L3–L4 | Centre Dielectric entries | Centre column below, in physical order. |
| L4 | Inner copper | 0.0152 mm, 0.5 oz |
| L4–L5 | FR-4 core | Same thickness as L2–L3. |
| L5 | Inner copper | 0.0152 mm, 0.5 oz |
| L5–L6 | Prepreg | Reverse the top gap, except for the `3313C` value identified below. |
| L6 | Finished outer copper | 0.035 mm, 1 oz |

| JLCPCB order code | L1–L2 prepreg, mm | Each of L2–L3 and L4–L5 cores, mm | L3–L4, top to bottom, mm |
|---|---|---:|---|
| No requirement | 3313/0.0994 | 0.550 | 2116/0.1164 |
| JLC06161H-3313 | 3313/0.0994 | 0.550 | 2116/0.1088 |
| JLC06161H-7628 | 7628/0.2104 | 0.400 | 7628/0.2028 |
| JLC06161H-7628D | 7628/0.218 + 7628/0.2104 | 0.200 | 7628/0.2028 |
| JLC06161H-1080 | 1080/0.0764 | 0.550 | 7628/0.2104 |
| JLC06161H-2116A | 2116/0.1164 | 0.130 | 2116/0.1164 + core/0.700 + 2116/0.1164 |
| JLC06161H-1080A | 1080/0.0764 | 0.600 | 3313/0.0994 |
| JLC06161H-3313C | 3313/0.107 + 1080/0.0764 | 0.400 | 1080/0.0764 + 1080/0.0764 |
| JLC06161H-1080B | 1080/0.0764 | 0.100 | 7628/0.2104 + core/0.700 + 7628/0.2104 |
| JLC06161H-3313B | 3313/0.0994 | 0.400 | 7628/0.2104 + 7628/0.2104 |
| JLC06161H-3313E | 3313/0.0994 | 0.100 | 7628/0.2104 + core/0.700 + 7628/0.2104 |
| JLC06161H-1080C | 1080/0.0764 | 0.075 | 7628/0.2104 + core/0.700 + 7628/0.2104 |
| JLC06161H-3313A | 3313/0.0994 | 0.550 | 2116/0.1088 |
| JLC06161H-2116B | 2116/0.1164 | 0.500 | 1080/0.0764 + 1080/0.0764 |
| JLC06161H-2116 | 2116/0.127 + 2313/0.0964 | 0.300 | 7628/0.2084 + 7628/0.2084 |
| JLC06161H-7628B | 7628/0.218 + 1080/0.0764 | 0.350 | 1080/0.0764 + 1080/0.0764 |
| JLC06161H-3313D | 3313/0.107 + 1080/0.0764 | 0.250 | 7628/0.2104 + 2116/0.124 + 7628/0.2104 |
| JLC06161H-2116C | 2116/0.127 + 2116/0.1194 | 0.300 | 2116/0.1194 + 2116/0.127 + 2116/0.1194 |
| JLC06161H-7628A | 7628/0.218 + 7628/0.218 + 1080/0.0764 | 0.100 | 2116/0.1164 + 2116/0.1164 |

For `JLC06161H-3313C`, JLCPCB prints L5–L6 as **1080/0.0784 + 3313/0.107 mm**, although the corresponding upper 1080 ply is 0.0764 mm. The unequal values are preserved here. [JLCPCB, `JLC06161H-3313C` table](https://jlcpcb.com/impedance).

### Eight copper Layers and other selections

JLCPCB names **`JLC08161H-3313`, 1.6 mm**, in its 8-Layer example. That table identifies each copper Layer but omits the Dielectric dimensions. The suffix alone does not establish a material or a thickness for each interval. Source for the table, read 2026-11-18: [JLCPCB 8-Layer High-Performance Stackup](https://jlcpcb.com/blog/multilayer-pcb-design-guidelines).

| Position | Material and thickness | Weight or Dk |
|---|---|---|
| L1 | Copper, 0.035 mm | 1 oz finished |
| L1–L2 | Dielectric — | — |
| L2 | Copper, 0.0152 mm | 0.5 oz |
| L2–L3 | Dielectric — | — |
| L3 | Copper, 0.0152 mm | 0.5 oz |
| L3–L4 | Dielectric — | — |
| L4 | Copper, 0.0152 mm | 0.5 oz |
| L4–L5 | Dielectric — | — |
| L5 | Copper, 0.0152 mm | 0.5 oz |
| L5–L6 | Dielectric — | — |
| L6 | Copper, 0.0152 mm | 0.5 oz |
| L6–L7 | Dielectric — | — |
| L7 | Copper, 0.0152 mm | 0.5 oz |
| L7–L8 | Dielectric — | — |
| L8 | Copper, 0.035 mm | 1 oz finished |

The 4-Layer catalogue also exposes 0.8, 1.0, 1.2 and 2.0 mm thickness filters. The 6-Layer catalogue also exposes 1.2 and 2.0 mm. Both show 1/2 oz outer and 0.5/1/2 oz inner weight choices. Codes and dimensions behind other filter combinations could not be read. The order form exposes separate **Layer Stackup** and **Layer Sequence** fields. [JLCPCB catalogue selectors](https://jlcpcb.com/impedance), [order form](https://cart.jlcpcb.com/quote).

### Hobby pricing and Via options

JLCPCB advertises eligible 4-Layer prototypes up to 100 × 100 mm from **US $2**, and five 6-Layer boards at 50 × 50 mm with ENIG from **US $2**. Its promotion page also advertises selected boards up to 8 Layers from $2. The available evidence does not connect every named construction to that price. The calculator guide identifies the first recommended Stackup as usually the lowest-cost, quickest option. [Prototype offer](https://jlcpcb.com/ads/pcb-prototype), [6-Layer size, quantity and finish](https://jlcpcb.com/resources/pcb-layers), [multilayer promotion](https://jlcpcb.com/features/pcb-prototyping-promotion), [calculator guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator).

For an ordinary through Via, the preferred minimum drill is 0.20 mm. A 0.10 or 0.15 mm drill costs extra at any Pad size. A 0.20 or 0.25 mm drill also costs extra with a Pad smaller than 0.45 mm. The 0.10 mm drill / 0.20 mm Pad option requires a board no thicker than 1 mm and ENIG or OSP. The catalogue overview separately lists a 0.20 mm minimum Via. Its summary and the detailed, surcharge-qualified drill minimum are different figures. The 6-Layer product advertises free Via-in-Pad, and the capabilities table describes filling and plating over as the default for 6+ Layers. [Rigid drilling and Via-in-Pad capabilities](https://jlcpcb.com/capabilities/pcb-capabilities), [catalogue overview](https://jlcpcb.com/impedance), [6-Layer product](https://jlcpcb.com/6-layer-pcb).

For HDI, choose the HDI material/service. Blind/buried drilling and lamination charges are confirmed after engineering review. The fab lists first-, second- and third-order structures on 4–24 Layers and names Nan Ya NP-175F or Shengyi S1000-2M, Tg 170 °C. It can manufacture a requested skip such as L1–L3 or L6–L4 as stacked segments. [HDI service, order steps and structures](https://jlcpcb.com/pcb-fabrication/hdi-pcb).

| JLCPCB HDI feature | Published capability |
|---|---|
| Blind laser Via | Diameter 0.075–0.15 mm, default 0.10 mm. Capture and target Pads require about 0.075 mm radial ring. The Via is copper-filled and planarized. |
| Buried mechanical Via | Normally 0.15–0.55 mm diameter with 0.075 mm ring. An exceptional 0.10 mm diameter is possible when the drilled Dielectric span is ≤1.0 mm. The buried core is ≥0.30 mm. Resin plugging and copper capping are part of this construction. |
| Through mechanical Via on HDI | Minimum diameter 0.15 mm. The FAQ calls for a Pad diameter at least 0.10 mm larger than the hole. |
| Depth and aspect ratio | The FAQ gives a 0.05–0.127 mm blind-Via Dielectric. Its table says the diameter must exceed the Dielectric thickness, but its worked example permits equality at 1:1. The product page also permits ≤1:1. Mechanical buried and through Vias have a ≤16:1 limit. |
| Copper clearance | Hole edge to Track ≥0.15 mm. Blind-hole edge to routed board edge ≥0.35 mm. Blind Via to another Via requires ≥0.13 mm on the same Net or ≥0.24 mm on different Nets. |
| HDI Track width / clearance | Standard 3/3 mil on inner and outer Layers. The 2.7/2.7 mil limit increases cost and process difficulty. |
| Finished thickness | The FAQ gives 0.5–2.4 mm. The product page gives 0.5–3.0 mm. Both restrict the 0.5 mm minimum to 4 Layers. |

Sources for the HDI table, read 2026-11-18: [JLCPCB HDI parameters and FAQ](https://jlcpcb.com/help/article/hdi-pcb-capabilities-faq), [HDI fabrication specifications](https://jlcpcb.com/pcb-fabrication/hdi-pcb), [extreme buried-Via capability](https://jlcpcb.com/capabilities/pcb-capabilities).

### Impedance and manufacturing files

The public calculator accepts board thickness, Layer count, copper weight, target impedance, differential gap and coplanar copper gap. It returns candidate Stackups and Track widths. The form was readable without login. The guide describes support for 1 oz outer and 0.5/1 oz inner, while the form displays 2 oz choices for both. Its calculation parameters use **1.6 mil** for 1 oz finished outer copper, versus **0.035 mm** in the named Stackup tables. The older cart template gives Dk **4.6/4.05/4.25** for 7628/3313/2116, versus **4.4/4.1/4.16** in the public catalogue. Both give core Dk 4.6. [Calculator](https://jlcpcb.com/pcb-impedance-calculator), [calculator parameters](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator), [public catalogue](https://jlcpcb.com/impedance), [older cart template](https://cart.jlcpcb.com/client/template/placeOrder/impedance.html).

The lamination article offers **free standard impedance testing at ±20%** and paid precision testing. The rigid capabilities page calls **±10%** standard and offers **±5%** on special request. The HDI service gives **±10% at ≥50 Ω and ±5 Ω below 50 Ω**. These pages do not establish one price and tolerance for every order. [Lamination article and test tiers](https://jlcpcb.com/help/article/multi-layer-pcb-standard-laminated-structures), [rigid capabilities FAQ](https://jlcpcb.com/capabilities/pcb-capabilities), [HDI impedance specification](https://jlcpcb.com/pcb-fabrication/hdi-pcb).

For a 4-Layer board the naming guide recognizes `.G1/.G2` and suggests `.G2L/.G3L`. The preparation guide continues the latter numbering for further inner Layers, so the first inner copper Layer (physically L2) is **`.G2L`**. JLCPCB warns that negative `.GP1/.GP2` artwork may be skipped. The KiCad guide enables X2 and accepts separate PTH and NPTH drill files, while another JLCPCB export guide says X2 is unsupported. [Naming guide](https://jlcpcb.com/help/article/suggested-naming-patterns), [artwork preparation](https://jlcpcb.com/help/article/gerber-files-preparation), [KiCad export guide](https://jlcpcb.com/help/article/how-to-generate-gerber-and-drill-files-in-kicad-9), [other export guide](https://jlcpcb.com/help/article/how-to-generate-gerber-and-drill-files-in-autodesk-eagle).

For HDI, provide the Gerbers, drill programs, finished thickness, copper weights and impedance requirements. Identify **both physical endpoints in each blind/buried drill filename**, for example `1_2` or `3_4`. The older naming guide still says blind/buried Vias are unsupported. The dedicated HDI page describes how to order and label them. Acceptance of an XNC attribute in place of that filename was unverified. [HDI file FAQ](https://jlcpcb.com/pcb-fabrication/hdi-pcb), [older naming guide](https://jlcpcb.com/help/article/suggested-naming-patterns).

## PCBWay

### Through-hole Stackups

PCBWay assigns each construction a number **within a Layer count**. Thus 4-Layer `#1` and 8-Layer `#1` identify different constructions. Its catalogue expressly applies to through-hole boards. The order form describes Tg 150 FR-4 as the usual free multilayer upgrade, with exceptions including heavy copper, greater thickness, custom Stackups and Via-in-Pad. The table does not guarantee a laminate manufacturer or grade for each number. [PCBWay numbered catalogue](https://www.pcbway.com/multi-layer-laminated-structure.html), [order-form material conditions](https://www.pcbway.com/orderonline.aspx).

The next tables specify all the catalogue’s **nominal 1.6 mm** constructions for 4, 6 and 8 Layers. The final-thickness column gives PCBWay’s separate **finished estimate, ±10%**, for each entry. The copper coverage column is the proportion remaining on an inner Layer after etching. In its 70/50/30% families the fab maps coverage above 60% to the 70% construction, above 40% through 60% to the 50% construction, and 40% or less to the 30% construction. [PCBWay numbered material, lamination and finished-thickness tables](https://www.pcbway.com/multi-layer-laminated-structure.html).

| Material | Dk in the numbered tables | How thickness is recorded below |
|---|---:|---|
| 7628 prepreg, 46% resin | 4.74 | Pressed gap in mm |
| 2116 prepreg, 58% resin | 4.45 | Pressed gap in mm |
| 3313 prepreg, 58% resin | 4.45 | Pressed gap in mm |
| 1080 prepreg, 68% resin | 4.21 | Pressed gap in mm |
| FR-4 core | 4.6 | Dielectric thickness excluding both copper faces |

Source for the material table, read 2026-11-18: [PCBWay numbered constructions](https://www.pcbway.com/multi-layer-laminated-structure.html). The tables do not quote a Dk frequency. For stacked prepregs, `3313+2116/0.219` means **3313 then 2116, with a combined pressed thickness of 0.219 mm**. Assigning that pressed value to each individual ply would double the gap.

| Finished copper weight | Outer base foil used in these tables | Nominal finished outer thickness from PCBWay’s copper-weight table | Inner thickness in these numbered constructions |
|---:|---:|---:|---:|
| 1 oz | 0.0175 mm | 0.035 mm | 0.0350 mm |
| 1.5 oz | — | — | 0.0525 mm |
| 2 oz | 0.0525 mm | 0.070 mm | 0.0700 mm |
| 3 oz | 0.0875 mm | 0.105 mm | 0.1050 mm |
| 4 oz | 0.1225 mm | 0.140 mm | 0.1400 mm |
| 5 oz | 0.1225 mm for the listed 5 oz builds | 0.175 mm | 0.1750 mm |

Sources for the copper table, read 2026-11-18: [numbered constructions, base foil and plating entries](https://www.pcbway.com/multi-layer-laminated-structure.html), [capabilities, outer copper weight equivalents](https://www.pcbway.com/capabilities.html). The capabilities summary rounds 1.5 oz inner to 50 µm, while the numbered constructions give 52.5 µm. It also lists inner weights through 4 oz, while the construction catalogue includes 5 oz inner.

The following physical sequence gives the position of **every copper Layer and Dielectric** for each numbered variant below. “Reverse” reverses ply order across the board centre. Each gap retains the pressed thickness in its numbered row. Source, read 2026-11-18: [PCBWay 4-, 6- and 8-Layer lamination tables](https://www.pcbway.com/multi-layer-laminated-structure.html).

| Physical entry | 4 Layers | 6 Layers | 8 Layers |
|---|---|---|---|
| L1 | Outer Cu | Outer Cu | Outer Cu |
| L1–L2 | Outer prepreg gap | Outer prepreg gap | Outer prepreg gap |
| L2 | Inner Cu | Inner Cu | Inner Cu |
| L2–L3 | Core | Core | Core |
| L3 | Inner Cu | Inner Cu | Inner Cu |
| L3–L4 | Reverse outer prepreg | Centre prepreg gap | Interior prepreg gap |
| L4 | Outer Cu | Inner Cu | Inner Cu |
| L4–L5 | n/a | Core | Core |
| L5 | n/a | Inner Cu | Inner Cu |
| L5–L6 | n/a | Reverse outer prepreg | Reverse interior prepreg |
| L6 | n/a | Outer Cu | Inner Cu |
| L6–L7 | n/a | n/a | Core |
| L7 | n/a | n/a | Inner Cu |
| L7–L8 | n/a | n/a | Reverse outer prepreg |
| L8 | n/a | n/a | Outer Cu |

#### Four copper Layers, nominal 1.6 mm

Source for every row, read 2026-11-18: [PCBWay 4-Layer numbered tables](https://www.pcbway.com/multi-layer-laminated-structure.html). PCBWay marks `#1–7` **Regular**. Other numbered constructions include additional copper weights. Inclusion in the catalogue alone does not establish a base-tier price.

| No. | Finished Cu outer/inner, oz | Inner coverage | L1–L2 prepreg / pressed mm | L2–L3 core, mm | Finished board, mm |
|---:|---|---:|---|---:|---:|
| 1 | 1/1 | 70% | 7628/0.1855 | 1.030 | 1.61 |
| 2 | 1/1 | 50% | 7628/0.1785 | 1.030 | 1.59 |
| 3 | 1/1 | 30% | 7628/0.1715 | 1.030 | 1.58 |
| 4 | 2/1 | 70% | 7628/0.1855 | 0.930 | 1.58 |
| 5 | 2/1 | 50% | 7628/0.1785 | 0.930 | 1.56 |
| 6 | 2/1 | 30% | 7628/0.1715 | 0.930 | 1.55 |
| 7 | 2/1.5 | 70% | 7628/0.1803 | 0.895 | 1.57 |
| 44 | 2/1.5 | 50% | 2116+2116/0.2338 | 0.795 | 1.57 |
| 45 | 2/1.5 | 30% | 1080+7628/0.2403 | 0.795 | 1.59 |
| 46 | 2/2 | 70% | 2116+7628/0.3050 | 0.630 | 1.59 |
| 47 | 2/2 | 50% | 2116+7628/0.2910 | 0.630 | 1.56 |
| 48 | 2/2 | 30% | 7628+7628/0.3430 | 0.630 | 1.66 |
| 49 | 3/3 | 90% | 2116+2116/0.2495 | 0.590 | 1.57 |
| 50 | 3/3 | 90% | 7628+7628/0.3815 | 0.340 | 1.59 |
| 51 | 3/3 | 90% | 2116+7628/0.3155 | 0.470 | 1.59 |
| 52 | 4/4 | 90% | 7628+7628/0.3780 | 0.230 | 1.61 |
| 53 | 4/4 | 90% | 3313+2116/0.2190 | 0.520 | 1.58 |
| 54 | 4/4 | 90% | 2116+7628/0.3120 | 0.320 | 1.57 |
| 55 | 5/5 | 90% | 3313+7628/0.2855 | 0.320 | 1.59 |
| 56 | 5/5 | 90% | 3313+2116/0.2265 | 0.450 | 1.60 |
| 57 | 5/5 | 90% | 3313+7628/0.2855 | 0.320 | 1.59 |

#### Six copper Layers, nominal 1.6 mm

Source for every row, read 2026-11-18: [PCBWay 6-Layer numbered tables](https://www.pcbway.com/multi-layer-laminated-structure.html). PCBWay marks `#1–4` **Regular**. The core column applies to L2–L3 and L4–L5. The centre prepregs are in top-to-bottom order.

| No. | Finished Cu outer/inner, oz | Inner coverage | L1–L2 prepreg / mm | Each core, mm | L3–L4 prepreg / mm | Finished board, mm |
|---:|---|---:|---|---:|---|---:|
| 1 | 1/1 | 70% | 2116/0.1195 | 0.430 | 7628/0.1750 | 1.55 |
| 2 | 1/1 | 50% | 7628/0.1785 | 0.430 | 7628/0.1610 | 1.65 |
| 3 | 2/1 | 70% | 2116/0.1195 | 0.430 | 2116/0.1090 | 1.55 |
| 4 | 2/1 | 50% | 2116/0.1125 | 0.430 | 2116+2116/0.2250 | 1.66 |
| 24 | 1/1 | 30% | 2116/0.1055 | 0.430 | 2116+2116/0.2110 | 1.56 |
| 25 | 2/1 | 30% | 2116/0.1055 | 0.430 | 2116+2116/0.2110 | 1.63 |
| 26 | 2/1.5 | 70% | 2116+2116/0.2443 | 0.261 | 2116+2116/0.2285 | 1.65 |
| 27 | 2/1.5 | 50% | 2116+2116/0.2338 | 0.261 | 2116+2116/0.2075 | 1.61 |
| 28 | 2/1.5 | 30% | 2116+2116/0.2233 | 0.261 | 2116+2116/0.1865 | 1.57 |
| 29 | 2/2 | 70% | 2116+2116/0.2390 | 0.206 | 2116+2116/0.2180 | 1.59 |
| 30 | 2/2 | 50% | 2116+2116/0.2250 | 0.206 | 2116+1080+2116/0.2710 | 1.62 |
| 31 | 2/2 | 30% | 3313+7628/0.2500 | 0.206 | 3313+7628/0.2010 | 1.60 |
| 32 | 3/3 | 90% | 2116+2116/0.2495 | 0.103 | 2116+2116/0.2390 | 1.64 |
| 33 | 3/3 | 90% | 3313+2116/0.2225 | 0.130 | 3313+2116/0.2120 | 1.61 |

#### Eight copper Layers, nominal 1.6 mm

Source for every row, read 2026-11-18: [PCBWay 8-Layer numbered tables](https://www.pcbway.com/multi-layer-laminated-structure.html). PCBWay marks `#1–4` **Regular**. The core column applies to **all three** core intervals. L5–L6 reverses the listed L3–L4 ply order.

| No. | Finished Cu outer/inner, oz | Inner coverage | L1–L2 prepreg / mm | Each core, mm | L3–L4 prepreg / mm | Finished board, mm |
|---:|---|---:|---|---:|---|---:|
| 1 | 1/1 | 70% | 2116/0.1195 | 0.230 | 7628/0.1750 | 1.62 |
| 2 | 1/1 | 50% | 2116/0.1125 | 0.230 | 7628/0.1610 | 1.58 |
| 3 | 2/1 | 70% | 2116/0.1195 | 0.230 | 2116/0.1090 | 1.56 |
| 4 | 2/1 | 50% | 2116/0.1125 | 0.230 | 7628/0.1610 | 1.65 |
| 19 | 1/1 | 30% | 7628/0.1715 | 0.130 | 7628+3313/0.2500 | 1.58 |
| 20 | 2/1 | 30% | 2116/0.1055 | 0.130 | 2116+7628/0.2770 | 1.57 |
| 21 | 2/1.5 | 70% | 3313+2116/0.2173 | 0.082 | 3313+2116/0.2015 | 1.60 |
| 22 | 2/1.5 | 50% | 2116+2116/0.2338 | 0.082 | 2116+2116/0.2075 | 1.65 |
| 23 | 2/1.5 | 30% | 2116+2116/0.2233 | 0.082 | 2116+2116/0.1865 | 1.59 |
| 24 | 2/2 | 70% | 1080+2116/0.1900 | 0.081 | 1080+2116/0.1690 | 1.59 |
| 25 | 2/2 | 50% | 3313+2116/0.1980 | 0.081 | 3313+2116/0.1630 | 1.59 |
| 26 | 2/2 | 30% | 2116+2116/0.2110 | 0.081 | 2116+2116/0.1620 | 1.61 |

#### Other PCBWay thicknesses

Source for all catalogue numbers and nominal thicknesses, read 2026-11-18: [PCBWay through-hole lamination catalogue](https://www.pcbway.com/multi-layer-laminated-structure.html). Numbers restart at each Layer count.

| Nominal thickness | 4-Layer catalogue numbers | 6-Layer catalogue numbers | 8-Layer catalogue numbers |
|---:|---|---|---|
| 0.4 mm | 40–42 | — | — |
| 0.6 mm | 35–39, 112 | 21–23 | — |
| 0.8 mm | 29–34, 106–111 | 18–20 | — |
| 1.0 mm | 23–28, 100–105 | 16–17, 74 | 16–18 |
| 1.2 mm | 17–22, 85–99 | 12–15, 69–73 | 13–15 |
| 2.0 mm | 11–16, 73–84 | 8–11, 55–68 | 9–12, 42–49 |
| 2.4 mm | 8–10, 43, 58–72 | 5–7, 34–54 | 5–8, 27–41 |

PCBWay still hosts an older, struck-through 1.6 mm 4-Layer table with 0.11/1.13/0.11 mm Dielectrics and Dk 4.29/3.96/4.29. Those are different values from the numbered catalogue. The older page labels them as the old version. [Superseded PCBWay tables](https://www.pcbway.com/blog/Engineering_Technical/stackup___pcbway.html), [numbered catalogue](https://www.pcbway.com/multi-layer-laminated-structure.html).

### Prototype prices, HDI and impedance

PCBWay’s public prototype pages display **US $48 for 4 Layers and $208 for 6 Layers at 100 × 100 mm**, and $116/$282 respectively at 150 × 100 mm. The fetched forms did not identify the selected quantity. These are displayed configuration prices, with no verified per-board denominator. Custom Stackup, impedance control, filled Vias and Via-in-Pad are separate quote options. Standard CNC holes below 0.20 mm incur extra cost. [4-Layer 100 × 100 example](https://www.pcbway.com/pcb_prototype/4_Layer_pcb/100x100mm.html), [6-Layer 100 × 100 example](https://www.pcbway.com/pcb_prototype/6_Layer_pcb/100x100mm.html), [150 × 100 price list](https://www.pcbway.com/pcb_prototype/4_Layer_pcb/150x100mm.html), [order form](https://www.pcbway.com/orderonline.aspx), [standard drilling charges](https://www.pcbway.com/capabilities.html).

The blind/buried guide describes a core-drilling process in which a span includes an even number of copper Layers. It prohibits a Via ending on the top of a core or starting on its bottom. Completely enclosed spans are possible with extra processing, while partially overlapping spans are restricted. The HDI service separately offers sequential laser constructions. Extra drill, press, plate and verification operations raise the price. A fixed premium per Via or per span was unverified. [Blind/buried construction guide](https://www.pcbway.com/pcb_prototype/Blind_vias_and_Buried_Vias.html), [HDI product](https://www.pcbway.com/hdi-pcb.html).

The HDI product lists **0.15 mm mechanical**, **4 mil laser** as standard, **3 mil laser by evaluation** on a single 106 prepreg, and a maximum laser diameter of **8 mil** with Dielectric thickness ≤0.15 mm. It lists 0.254–6.5 mm for resin-filled Vias. The separate Advanced roadmap specifies a 0.076 mm minimum finished laser hole and a retained Via ring of 3 mil. Its summary on the same page gives 0.10 mm for laser blind/buried holes. Published depth/diameter figures are ≤8:1 in the normal mechanical process column, 12:1 in the Advanced roadmap, 14:1 on the HDI page and 20:1 in the Advanced summary. These figures have different or unspecified process scopes. No single laser aspect-ratio limit was verified. [Normal drilling table](https://www.pcbway.com/capabilities.html), [Advanced roadmap and capabilities summary](https://www.pcbway.com/advanced-pcb-capabilities.html), [HDI laser and fill limits](https://www.pcbway.com/hdi-pcb.html).

The public calculator solves for impedance or geometry and exposes microstrip, embedded microstrip, stripline, asymmetric stripline, edge-coupled and broadside-coupled structures, and coplanar waveguide. It omits surface finish and omits solder mask from the outer microstrip, edge-coupled and coplanar models. It was readable without login. The Advanced roadmap gives **±10%, or ±5 Ω at 50 Ω and below**. The order form offers impedance control, and the fab confirms the construction after reviewing the requirements. A fixed control or coupon fee was unverified. [PCBWay impedance calculator](https://www.pcbway.com/pcb_prototype/impedance_calculator.html), [Advanced impedance tolerance](https://www.pcbway.com/advanced-pcb-capabilities.html), [Stackup review process](https://www.pcbway.com/multi-layer-laminated-structure.html), [order options](https://www.pcbway.com/orderonline.aspx).

### Manufacturing files

Enter each copper filename in **physical top-to-bottom order** in PCBWay’s order field. The fab illustrates a failed review involving `.G1` through `.G4` and `.GP1/.GP2`: labels such as “ground” and “power” did not tell CAM which submitted files belonged at those positions. For more than four Layers with blind/buried Vias, its export guide requires **all Excellon outputs generated from the Stackup**. A mandatory filename spelling for each drill span or confirmation that XNC attributes alone determine the span was unverified. [Layer-order instructions](https://www.pcbway.com/helpcenter/layers_issues/No_layer_orders_for_multi_layers_boards.html), [failed-review example](https://www.pcbway.com/helpcenter/layers_issues/The_layer_order_information_you_filled_online_doesn_t_match_the_name_suffix_in_gerber_file.html), [multilayer drill export](https://www.pcbway.com/helpcenter/generate_gerber/Notes_for_Gerber_files_Generated_from_Eagle_9_20.html).

## OSH Park

### Standard prototype Stackups

The **4 Layer Prototype** and **6 Layer Prototype** services have a nominal finished thickness of **63 mil (1.6 mm)**. No 8-Layer prototype was listed in the service index. The two service tables give material-specific Dk at **1 GHz** for the entries identified below. Both leave the central core’s Dk blank. Sources for the table, read 2026-11-18: [4-Layer Stackup](https://docs.oshpark.com/services/four-layer/), [6-Layer Stackup](https://docs.oshpark.com/services/six-layer/), [fabrication service index](https://docs.oshpark.com/services/).

| Physical position | 4 Layer Prototype | 6 Layer Prototype |
|---|---|---|
| L1 | Cu 0.0432 mm, 1 oz clad and plated | Cu 0.0432 mm, 1 oz clad and plated |
| L1–L2 | FR408HR 2113 prepreg, 0.1999 mm, Dk 3.61 | FR408HR 106 prepreg, 75% resin, 0.1107 mm, Dk 3.23 |
| L2 | Cu 0.0175 mm, 0.5 oz | Cu 0.0175 mm, 0.5 oz |
| L2–L3 | FR408HR core, 0.9906 mm, Dk — | FR408HR 2113 core, 0.1016 mm, Dk 3.66 |
| L3 | Cu 0.0175 mm, 0.5 oz | Cu 0.0175 mm, 0.5 oz |
| L3–L4 | FR408HR 2113 prepreg, 0.1999 mm, Dk 3.61 | FR408HR core, 0.9256 mm, Dk — |
| L4 | Cu 0.0432 mm, 1 oz clad and plated | Cu 0.0175 mm, 0.5 oz |
| L4–L5 | n/a | FR408HR 2113 core, 0.1016 mm, Dk 3.66 |
| L5 | n/a | Cu 0.0175 mm, 0.5 oz |
| L5–L6 | n/a | FR408HR 106 prepreg, 75% resin, 0.1107 mm, Dk 3.23 |
| L6 | n/a | Cu 0.0432 mm, 1 oz clad and plated |

Both tables also list 0.0152 mm solder mask outside each outer copper Layer. The 4-Layer general material table says Dk 3.61 at 1 GHz, while its detailed Stackup assigns that value specifically to the outer prepregs and leaves the core entry blank. [4-Layer material and Stackup tables](https://docs.oshpark.com/services/four-layer/), [6-Layer Stackup table](https://docs.oshpark.com/services/six-layer/).

OSH Park documents stock-substitution constructions separately. Its all-EM528 4-Layer option retains the 1/0.5/0.5/1 oz copper sequence, with two 7.67 mil gaps of EM528 two-ply 3313 prepreg, Dk 3.76, and a 39 mil five-ply 7628 core, Dk 4.11. The mixed 4-Layer construction retains FR408HR prepregs and changes the central core to EM528. The mixed 6-Layer table lists a **21 mil** EM528 central core, while the ordinary table lists **36.44 mil** FR408HR, despite the notice describing the change as core material only. These are shortage-period notices. Current checkout availability of the substitutes was unverified. [Alternate material notice and Stackup](https://docs.oshpark.com/troubleshooting/alternate-4-layer-stackup/), [mixed-material constructions](https://docs.oshpark.com/troubleshooting/mixed-material-4-layer-stackup/), [ordinary 6-Layer construction](https://docs.oshpark.com/services/six-layer/).

### Price, Vias, impedance and files

Prototype pricing is **$10/in² of design area for three copies at 4 Layers** and **$15/in² for three copies at 6 Layers**. The expedited 4-Layer Super Swift service is $20/in² for three copies. Its 4-Layer Medium Run service uses a different denominator: $2 per square inch of **total ordered board area**, with a 100 in² minimum and each design ordered in multiples of three. [4-Layer Prototype](https://docs.oshpark.com/services/four-layer/), [6-Layer Prototype](https://docs.oshpark.com/services/six-layer/), [Super Swift](https://docs.oshpark.com/services/four-layer-swift/), [Medium Run and worked price example](https://docs.oshpark.com/services/four-layer-medium-run/).

A plated Via spans L1–L4 or L1–L6 according to the service. Both pages explicitly exclude blind, buried and filled-and-plated Vias. A Via may be Tented, without a guaranteed filled hole or flat surface. Neither page supplies a separate maximum aspect ratio. Both publish impedance constraint charts. The alternate 4-Layer chart, specifically, lists a 50 Ω target with ±5 Ω and a 90 Ω target with ±9 Ω. A fab-operated interactive calculator, individually contracted tolerance, production coupon report and separate fee were unverified. [4-Layer drill specifications and chart](https://docs.oshpark.com/services/four-layer/), [6-Layer drill specifications and chart](https://docs.oshpark.com/services/six-layer/), [alternate impedance chart](https://docs.oshpark.com/troubleshooting/alternate-4-layer-stackup/).

OSH Park detects common filenames and Gerber X2. Its suggested 4-Layer sequence is `.GTL, .G2L, .G3L, .GBL`. Its 6-Layer sequence is `.GTL, .G2L, .G3L, .G4L, .G5L, .GBL`. Inner copper artwork must have positive polarity: drawn copper is retained. Partial-span drill programs are incompatible with these prototype services. [4-Layer file conventions](https://docs.oshpark.com/services/four-layer/), [6-Layer file conventions](https://docs.oshpark.com/services/six-layer/), [drill-file troubleshooting](https://docs.oshpark.com/troubleshooting/drill-issues/).

## Aisler

### Standard prototype Stackups

Aisler lists four 4-Layer products, crossing two board thicknesses with two outer copper thicknesses, plus 6- and 8-Layer ENIG products. The 35 µm products use FR-4, Tg 150 °C. The 70 µm outer products use 35 µm inner copper and **explicitly allow the core and prepreg stock to vary**. The 35 µm product pages publish their geometries as image attachments. Their surrounding text and impedance tables were readable, but the dimension labels in those images could not be verified. Aisler confirms that the 6-Layer product has fixed Dk values. [PCB Portfolio](https://community.aisler.net/t/pcb-portfolio/101), [6-Layer Dk clarification](https://community.aisler.net/t/6l-board/5504), [heavy-outer construction policy](https://community.aisler.net/t/4-layers-1-6mm-70-m-stackup/5523).

Sources for the physical table, read 2026-11-18: [4 Layers, 1.6 mm, 35 µm](https://community.aisler.net/t/4-layers-1-6mm-35-m-stackup/5457), [4 Layers, 0.8 mm, 35 µm](https://community.aisler.net/t/4-layers-0-8mm-35-m-stackup/5460), [4 Layers, 1.6 mm, 70 µm](https://community.aisler.net/t/4-layers-1-6mm-70-m-stackup/5523), [4 Layers, 0.8 mm, 70 µm](https://community.aisler.net/t/4-layers-0-8mm-70-m-stackup/5524), [6 Layers](https://community.aisler.net/t/6-layers-1-6mm-35-m-stackup/5458), [8 Layers](https://community.aisler.net/t/8-layers-1-6mm-35-m-stackup/5459), [material and copper matrix](https://community.aisler.net/t/pcb-portfolio/101). Copper is stated in the fab’s finished µm. A dash in a 35 µm column means unread geometry. “Variable” in a heavy-outer column is a property of the service.

| Physical position | 4 Layers, 35 µm, 1.6 or 0.8 mm | 4 Layers, 70 µm, 1.6 or 0.8 mm | 6 Layers, 35 µm, 1.6 mm | 8 Layers, 35 µm, 1.6 mm |
|---|---|---|---|---|
| L1 | Cu 35 µm | Cu 70 µm | Cu 35 µm | Cu 35 µm |
| L1–L2 | — | Variable prepreg | — | — |
| L2 | Cu 35 µm | Cu 35 µm | Cu 35 µm | Cu 35 µm |
| L2–L3 | — | Variable FR-4 core | — | — |
| L3 | Cu 35 µm | Cu 35 µm | Cu 35 µm | Cu 35 µm |
| L3–L4 | — | Variable prepreg | — | — |
| L4 | Cu 35 µm | Cu 70 µm | Cu 35 µm | Cu 35 µm |
| L4–L5 | n/a | n/a | — | — |
| L5 | n/a | n/a | Cu 35 µm | Cu 35 µm |
| L5–L6 | n/a | n/a | — | — |
| L6 | n/a | n/a | Cu 35 µm | Cu 35 µm |
| L6–L7 | n/a | n/a | n/a | — |
| L7 | n/a | n/a | n/a | Cu 35 µm |
| L7–L8 | n/a | n/a | n/a | — |
| L8 | n/a | n/a | n/a | Cu 35 µm |

The fixed 35 µm pages specify ±10% total thickness. For the heavy-outer 4-Layer product, the portfolio lists HASL or ENIG at 1.6 mm and ENIG at 0.8 mm. An older fab reply identifies Panasonic 1080 R-1551(W) prepreg on its then-current 4-Layer product. A later staff answer directs customers to the newer, thickness-specific drawings. The historical material identification does not verify a current Dielectric dimension or Dk. The 6-Layer page gives **Df about 0.02**, a loss factor distinct from Dk. [PCB Portfolio](https://community.aisler.net/t/pcb-portfolio/101), [historical material identification](https://community.aisler.net/t/option-for-board-suitable-for-rf/3581), [updated fab guidance](https://community.aisler.net/t/4-layers-pcb-stackup/4866), [6-Layer construction](https://community.aisler.net/t/6-layers-1-6mm-35-m-stackup/5458).

### Price, Vias, impedance and files

Aisler’s Budget pricing table lists **€14 plus €0.13 per cm² of total ordered area** for 4-Layer, 1.6 mm ENIG, and **€12 plus €0.156 per cm²** for 4-Layer, 0.8 mm ENIG. No fixed 6- or 8-Layer price was verified. Its 4-Layer fabrication reply confirms through Vias only. The multilayer rules classify drills up to 0.45 mm as Vias and enlarge component PTH drills to compensate for plating. A laser drill size, HDI span, aspect-ratio limit, filled-and-capped Via-in-Pad option and related surcharge were unverified for the newer prototype products. [Pricing formulas](https://community.aisler.net/t/our-simple-pricing/102/), [4-Layer Via answer](https://community.aisler.net/t/vias-and-4-layer-pcbs/3337), [4-Layer rule sheet](https://community.aisler.net/t/4-layer-35-m-enig-design-rules/3733), [6+ Layer rule sheet](https://community.aisler.net/t/6-layer-8-layer-and-more-35-m-enig-design-rules/5467).

Aisler publishes reference geometries for the fixed constructions. For a **90 Ω differential pair on top referenced to L2**, its 1.6 mm products give these different Track geometries. The fab describes these as design guidance. An **impedance check** is available by quotation: specify the required tolerance, and the fab measures a coupon per production run and records each requested impedance by Layer. No default contracted tolerance, fixed fee or public interactive impedance calculator was verified. [Aisler’s coupon-service instructions](https://community.aisler.net/t/coplaner-waveguide-impedance-testing/5456/2).

| Fixed 1.6 mm product | Track width | Gap within pair | Source impedance table, read 2026-11-18 |
|---|---:|---:|---|
| 4 Layers, 35 µm | 260 µm | 135 µm | [4-Layer geometries](https://community.aisler.net/t/4-layers-1-6mm-35-m-stackup/5457) |
| 6 Layers, 35 µm | 230 µm | 120 µm | [6-Layer geometries](https://community.aisler.net/t/6-layers-1-6mm-35-m-stackup/5458) |
| 8 Layers, 35 µm | 180 µm | 120 µm | [8-Layer geometries](https://community.aisler.net/t/8-layers-1-6mm-35-m-stackup/5459) |

Aisler accepts native KiCad boards and recognizes Gerber X2. In a traditional Gerber set it uses `project.toplayer.ger`, `project.internalplane1.ger` for the first inner copper Layer, consecutive inner numbers towards the bottom, and `project.bottomlayer.ger`. The numbering continues beyond `internalplane4` where required. Its separate drill names are `project.drills_pth.xln` and `project.holes_npth.xln`. It may merge multiple files with identical extensions. No blind/buried filename convention was verified. [Input support matrix](https://community.aisler.net/t/pcb-design-tool-support-matrix/103), [traditional Gerber and drill naming](https://community.aisler.net/t/naming-convention-for-traditional-gerber-files/99).

## NextPCB

### Standard Stackups

NextPCB identifies three 4-Layer, 1.6 mm constructions as **`04161H01-2116`, `04161H02-1080` and `04161H03-7628`**. Its common-Stackup guide gives prepreg heights of 0.125, 0.077 and 0.20 mm respectively. The standard 6-Layer family uses two cores and a centre prepreg gap. Its “pseudo 8” family still has six copper Layers, with an additional stripped core and insulating entries between L3 and L4. Its 8-Layer article supplies a dimensioned example without an order code. [Common Stackups and order selections](https://www.nextpcb.com/tech-center/pcb-layer-stack-up), [8-Layer Dielectric example](https://www.nextpcb.com/blog/8-layer-pcb-stackup-and-prototype).

Sources for the table, read 2026-11-18: [4-/6-Layer families and prepreg heights](https://www.nextpcb.com/tech-center/pcb-layer-stack-up), [4-Layer engineering examples](https://www.nextpcb.com/blog/4-layer-pcb-stackup-and-impedance-control), [8-Layer sample construction](https://www.nextpcb.com/blog/8-layer-pcb-stackup-and-prototype), [general material and copper capabilities](https://www.nextpcb.com/pcb-capabilities). The three 4-Layer columns are named constructions. The 6-Layer entries identify a family and the 8-Layer column is the fab’s example. Exact construction-specific Dk and frequency were unverified.

| Entry | 04161H01-2116, 1.6 mm | 04161H02-1080, 1.6 mm | 04161H03-7628, 1.6 mm | Standard 6 Layers, 1.6 mm | Sample 8 Layers, 1.6 mm |
|---|---|---|---|---|---|
| L1 | Cu — | Cu — | Cu — | Cu — | Cu — |
| L1–L2 | 2116, 0.125 mm | 1080, 0.077 mm | 7628, 0.20 mm | 1080, 0.077 or 2116, 0.125 mm | 1080, 0.077 mm |
| L2 | Cu — | Cu — | Cu — | Cu — | Cu — |
| L2–L3 | Core — | Core — | Core — | Core — | Core, 0.135 mm |
| L3 | Cu — | Cu — | Cu — | Cu — | Cu — |
| L3–L4 | 2116, 0.125 mm | 1080, 0.077 mm | 7628, 0.20 mm | Prepreg, 0.103 mm | Two 7628H plies, 0.23 mm each |
| L4 | Cu — | Cu — | Cu — | Cu — | Cu — |
| L4–L5 | n/a | n/a | n/a | Core — | Core, 0.135 mm |
| L5 | n/a | n/a | n/a | Cu — | Cu — |
| L5–L6 | n/a | n/a | n/a | Same selection as L1–L2 | Two 7628H plies, 0.23 mm each |
| L6 | n/a | n/a | n/a | Cu — | Cu — |
| L6–L7 | n/a | n/a | n/a | n/a | Core, 0.135 mm |
| L7 | n/a | n/a | n/a | n/a | Cu — |
| L7–L8 | n/a | n/a | n/a | n/a | 1080, 0.077 mm |
| L8 | n/a | n/a | n/a | n/a | Cu — |

NextPCB’s general capabilities give FR-4 Tg options of 130/150/170 °C, a general material Dk of 4.2, 1 or 2 oz finished outer copper, and 0.5/1/2 oz inner copper. Its 8-Layer article separately describes 0.5 or 1 oz outer. The 4-Layer article gives an **approximate** 0.8–1.2 mm core range for a 1.6 mm example with 1080. It does not identify exact core values for all three order codes. The 1.0 mm 8-Layer example replaces both pairs of 0.23 mm 7628H plies with single 0.16 mm 1506 plies and retains the other listed gap dimensions. [General capabilities](https://www.nextpcb.com/pcb-capabilities), [4-Layer example](https://www.nextpcb.com/blog/4-layer-pcb-stackup-and-impedance-control), [8-Layer examples](https://www.nextpcb.com/blog/8-layer-pcb-stackup-and-prototype).

| Other standard choices | Published selections |
|---|---|
| 4 Layers | At 0.6 mm the common table offers 2116 and 1080. At 0.8, 1.0, 1.2, 1.6 and 2.0 mm it offers 2116, 1080 and 7628. |
| 6 Layers | The ordinary 1080 and 2116 families are listed at 0.8, 1.0, 1.2, 1.6, 2.0 and 2.5 mm. Both pseudo-8 variants are listed at 1.2, 1.6, 2.0 and 2.5 mm. |
| 8 Layers | The common table lists the 1080 family at 1.0, 1.2, 1.6, 2.0 and 2.5 mm. |

Source for the availability table, read 2026-11-18: [NextPCB common-Stackup guide, available 4-, 6- and 8-Layer Stackups](https://www.nextpcb.com/tech-center/pcb-layer-stack-up). The order form selects the construction under **Special Process → Stack up**. NextPCB states that common selections carry no additional selection fee, but that its pseudo-8 construction costs about as much to process as an ordinary 8-Layer board.

### HDI, price and rules

NextPCB’s standard 8-Layer boards default to through Vias. The Advanced service offers sequential and any-Layer HDI. On four Layers it documents three choices: laser blind L1–L2 and L3–L4, those same blind Vias plus mechanical buried L2–L3, or mechanical buried L2–L3 together with ordinary L1–L4 through holes. Minimum laser diameter is **0.075 mm**. The published maximum microvia aspect ratio is **0.8:1 to 1:1**. The general Advanced drilling overview lists **20:1**, without assigning that value to laser holes. Copper-filled microvias and resin filling with overplating are available. A span-specific ring or fixed HDI surcharge was unverified. [Advanced capability and 4-Layer HDI choices](https://www.nextpcb.com/advanced-pcb-manufacturing-capabilities), [8-Layer service description](https://www.nextpcb.com/blog/8-layer-pcb-stackup-and-prototype).

Its separate 6-Layer HDI example uses 1 oz finished copper on L1–L6, blind L1–L2 and L6–L5, and mechanical buried L2–L5. The example table gives 1080 outer gaps of 0.076 mm, two FR-4 cores with approximate Dk 4.2 and thickness shown as 0.1/0.8 mm depending on construction, and 2116 at 0.11 mm between L3 and L4. The same article’s 1.6 mm illustration identifies a 1.2 mm core and 0.0770 mm prepreg. Those descriptions do not establish one unambiguous 1.6 mm HDI Stackup. [NextPCB 6-Layer HDI table and illustration](https://www.nextpcb.com/blog/6-layer-pcb-stackup-design).

The fab compares its general Standard service at **$5 for five pieces or less** with Advanced jobs ranging from under $100 to thousands. It does not identify a particular multilayer configuration at the $5 price. Its separate 4-Layer design article gives **0.075/0.075 mm inner, 0.10/0.10 mm outer, 0.25 mm drill, 0.20 mm plated hole, and 0.127 mm Via ring**. The capabilities table gives the different copper-weight- and board-thickness-dependent minima in the cross-fab comparison. The 4-Layer article does not identify matching copper weights for its Track rules. [Service and cost comparison](https://www.nextpcb.com/advanced-pcb-manufacturing-capabilities), [4-Layer fabrication capability table](https://www.nextpcb.com/blog/4-layer-pcb-stackup-and-impedance-control), [general drill and copper capabilities](https://www.nextpcb.com/pcb-capabilities).

### Impedance and manufacturing files

The public matched-Stackup calculator describes itself as single-ended and links separate geometry calculators, including differential microstrip and stripline. The Stackup-query guide describes single-ended and differential tables accounting for solder mask, with mask Dk 3.5. Its inputs were public. The main query endpoint timed out and a localized version returned selectors without populated constructions. The Advanced service specifies TDR with **±10% above 50 Ω and ±5 Ω at or below 50 Ω**. Its separate 8-Layer article describes **±5% with reports**. The commercial conditions that distinguish these tolerances, and any separate coupon charge, were unverified. [Matched-Stackup calculator](https://www.nextpcb.com/pcb-impedance-calculator), [differential calculator](https://www.nextpcb.com/differential-microstrip-impedance-calculator), [query-system methodology](https://www.nextpcb.com/blog/pcb-impedance-control-stackup-query-guide), [localized query form](https://jp.nextpcb.com/impedance-control-stackups), [Advanced tolerances](https://www.nextpcb.com/advanced-pcb-manufacturing-capabilities), [8-Layer tolerance](https://www.nextpcb.com/blog/8-layer-pcb-stackup-and-prototype).

The KiCad guide asks for each actual inner copper artwork file in the archive. A separate export guide gives `.G1` and `.GP2` as examples. NextPCB’s orientation guide recommends consecutive physical Layer labels in copper and describes the bottom label as mirrored when viewed from above. Selecting a construction in the order form communicates its physical materials, while the artwork identifies what goes on each Layer. [KiCad export guide](https://www.nextpcb.com/helpcenter/nextpcb-gerber-from-kicad), [other export guide](https://www.nextpcb.com/helpcenter/export-gerber-files-from-easyeda), [Layer orientation](https://www.nextpcb.com/pcb-vocabulary/layer-orientation), [Stackup ordering](https://www.nextpcb.com/tech-center/pcb-layer-stack-up).

For HDI, **supply an independent drill file for each Layer pair**. NextPCB illustrates an 8-Layer job with blind L1–L2, blind L8–L7 and buried L3–L6 programs separate from ordinary through drilling. For plating classes its instructions differ: the KiCad guide recommends merging PTH and NPTH to avoid omitting one. The drill guide asks for independent PTH/NPTH files or a drill map explicitly assigning plating status to tool numbers. Neither guide says that a merged through-drill file may also contain unidentified blind/buried spans. Mandatory span filename syntax and reliance on XNC metadata alone were unverified. [Drill-file guide, HDI handling and delivery checklist](https://www.nextpcb.com/blog/pcb-drill-file), [KiCad drill-export recommendation](https://www.nextpcb.com/helpcenter/nextpcb-gerber-from-kicad).

## Copper order and drill spans in Gerber X2 and XNC

Gerber `.FileFunction` numbers physical copper Layers from the top. For example, `%TF.FileFunction,Copper,L2,Inr*%` identifies the first inner copper artwork. The optional copper role can describe whether its artwork is signal, plane, mixed or hatched. The role does not supply the physical index. A plated drill/rout file has one pair of endpoints and one of the functions `PTH`, `Blind` or `Buried`. XNC encodes the corresponding file attribute in a comment. Sources for the syntax and the derived six-Layer examples below, read 2026-11-18: [Ucamco Gerber specification, revision 2026.05, `.FileFunction` table](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf), [Ucamco XNC specification, revision 2021.11, attributes and example](https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf).

| Manufacturing operation | Illustrative XNC filename | File attribute if delivered as Gerber | File attribute if delivered as XNC |
|---|---|---|---|
| L1–L6 plated through | `board-1_6.drl` | `%TF.FileFunction,Plated,1,6,PTH*%` | `; #@! TF.FileFunction,Plated,1,6,PTH` |
| L1–L2 blind | `board-1_2.drl` | `%TF.FileFunction,Plated,1,2,Blind*%` | `; #@! TF.FileFunction,Plated,1,2,Blind` |
| L2–L5 buried | `board-2_5.drl` | `%TF.FileFunction,Plated,2,5,Buried*%` | `; #@! TF.FileFunction,Plated,2,5,Buried` |

The filenames in the table are examples, not a universal convention. Each additional span needs its own attributed file. Even at identical XY coordinates, two stacked microvias can correspond to separate drilling operations. The file attribute identifies endpoints and plating. The ordered Stackup and fabrication instructions establish the drilling and lamination process. JLCPCB explicitly asks for endpoints in filenames, PCBWay asks for all Stackup-derived drill outputs, and NextPCB explicitly asks for independent span programs. No source read demonstrated that all three fabs infer everything from XNC attributes alone. [Gerber file functions](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf), [JLCPCB HDI file FAQ](https://jlcpcb.com/pcb-fabrication/hdi-pcb), [PCBWay drill export](https://www.pcbway.com/helpcenter/generate_gerber/Notes_for_Gerber_files_Generated_from_Eagle_9_20.html), [NextPCB HDI file checklist](https://www.nextpcb.com/blog/pcb-drill-file).

## Open points / not verified

- **JLCPCB:** The public 4- and 6-Layer catalogue tables were readable. The complete 8-Layer Dielectric schedule, other copper-weight combinations and the codes behind alternate thickness filters **could not be read: script-rendered**. The older cart template, help article, calculator and order form did not expose the missing schedules. The cart’s impedance-template endpoint was also inaccessible to the reader. Eligibility of a particular named Stackup for the $2 offer remains unverified. [Catalogue](https://jlcpcb.com/impedance), [alternate template](https://cart.jlcpcb.com/client/template/placeOrder/impedance.html), [lamination help](https://jlcpcb.com/help/article/multi-layer-pcb-standard-laminated-structures), [order form](https://cart.jlcpcb.com/quote), [template endpoint](https://cart.jlcpcb.com/api/overseas-shop-cart/v1/shoppingCart/getImpedanceTemplateSettings).
- **PCBWay:** The standard tables establish through-hole constructions. They do not establish an HDI Stackup for the same catalogue number. The prototype price pages left the selected quantity unresolved. An inner-only connected Via ring, a single laser-specific aspect ratio, mandatory span filenames and a fixed impedance-control surcharge were unverified. [Through-hole catalogue](https://www.pcbway.com/multi-layer-laminated-structure.html), [capabilities](https://www.pcbway.com/capabilities.html), [HDI service](https://www.pcbway.com/hdi-pcb.html), [prototype price example](https://www.pcbway.com/pcb_prototype/4_Layer_pcb/100x100mm.html).
- **OSH Park:** No 8-Layer prototype was listed. Central-core Dk in the ordinary detailed Stackups, an interactive fab calculator, a board-specific coupon service or fee, and a service-specific maximum drill aspect ratio were unverified. Shortage-period material notices do not confirm present availability. [Service portfolio](https://docs.oshpark.com/services/), [4-Layer service](https://docs.oshpark.com/services/four-layer/), [6-Layer service](https://docs.oshpark.com/services/six-layer/), [alternate material notice](https://docs.oshpark.com/troubleshooting/alternate-4-layer-stackup/).
- **Aisler:** The current 35 µm construction dimensions **could not be read: image-only attachments**. Direct requests returned image references without readable labels, a cache miss or a rate limit. Alternative topic and JSON views did not supply numeric entries. The heavy-outer product explicitly has variable Dielectrics. The fab’s older named prepreg and a customer’s estimates were not used to fill the current gaps. Laser/buried Via availability on the newer services, fixed 6-/8-Layer prices and a default coupon tolerance remain unverified. [4-Layer drawing](https://community.aisler.net/t/4-layers-1-6mm-35-m-stackup/5457), [6-Layer drawing](https://community.aisler.net/t/6-layers-1-6mm-35-m-stackup/5458), [8-Layer drawing](https://community.aisler.net/t/8-layers-1-6mm-35-m-stackup/5459), [variable construction](https://community.aisler.net/t/4-layers-1-6mm-70-m-stackup/5523), [coupon quote](https://community.aisler.net/t/coplaner-waveguide-impedance-testing/5456/2).
- **NextPCB:** The main construction-query endpoint timed out. Alternate fab-hosted forms loaded, but selected results **could not be read: script-rendered**. The standard-family and example tables therefore leave exact 6-/8-Layer order codes, several core dimensions, variant-specific copper, and Dk/frequency unverified. The fab does verify that HDI requires separate drill files per Layer pair. Its preferred spelling for those filenames and a separate coupon fee remain open. [Common Stackup guide](https://www.nextpcb.com/tech-center/pcb-layer-stack-up), [main query](https://www.nextpcb.com/impedance-control-stackups), [localized query](https://jp.nextpcb.com/impedance-control-stackups), [8-Layer engineering example](https://www.nextpcb.com/blog/8-layer-pcb-stackup-and-prototype), [drill-file guide](https://www.nextpcb.com/blog/pcb-drill-file).