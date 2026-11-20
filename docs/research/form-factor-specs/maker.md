# Maker form factors: Raspberry Pi HAT+/HAT/uHAT, Pico, Pico 2, Seeed XIAO

Research for REDA built-in Board Templates. Collected 2026-11-20 from primary sources only. Every PDF and DXF was downloaded and parsed locally. The dimensions in the drawings are vector strokes, not text, so they were rendered to images and read, and positions were checked against the vector geometry where noted.

**Status key**
- **Verified**: the value is printed in the primary source (text or a dimension label).
- **Partial**: the value is derived (arithmetic on verified values, or measured from the vector geometry of a primary drawing), or the source shows the value but does not say what it refers to.
- **Unverified**: no primary source states it. Do not ship it as fact.

**Coordinate convention used in every table.** The origin is the bottom-left corner of the board outline, seen from the top (component side). +X points right and +Y points up. All values are in mm.
- Raspberry Pi add-on boards: the 40-pin header is along the top edge (+Y), the camera/display side is on the left, and the USB/Ethernet/PoE side is on the right. This matches the Pi's own drawings.
- Pico and XIAO: the USB connector is at the top edge (+Y).

---

## Raspberry Pi HAT+ (current spec)

The HAT+ spec does **not** define a fixed outline. It says it "is less prescriptive about HAT physical dimensions" (S1 p3, PDF p4). It gives only minimum mechanical requirements and *example* drawings: Fig 2 is the Sense HAT, Fig 3 the M.2 HAT+ and Fig 4 the PoE+ HAT. A REDA "HAT+" template should therefore be presented as "HAT+ example outline (based on Raspberry Pi examples)", and the legacy HAT drawing (next section) can fill in values the HAT+ spec leaves open.

| Item | Value | Status | Source |
|---|---|---|---|
| Mandatory mechanical requirements | (1) Connects to the 40-way GPIO header, including the `ID_*` pins. (2) At least one mounting hole on the HAT+ aligns with one of the four Raspberry Pi mounting holes. | Verified | S1 ch. 7, p13 (PDF p14) |
| Recommendations | Should not foul the PoE headers on Pi 4 and later. Should not interfere with access to the camera, display and PCIe flex connectors. | Verified | S1 ch. 7, p13 |
| Outline (examples) | 65 × 56.5 (Fig 2 Sense HAT, Fig 3 M.2 HAT+) | Verified (as example) | S1 Fig 2 p13, Fig 3 p14 |
| Corner radii | Drawn rounded in the examples but **not dimensioned** in HAT+. The legacy HAT spec requires R3. | Unverified for HAT+ | S1 Figs 2–4. See legacy HAT S2. |
| Mounting hole pattern | 58 (X) × 49 (Y). Holes are 3.5 from the left edge and 3.5 from the bottom edge. | Verified | S1 Fig 2, Fig 3 |
| Mounting hole positions | (3.5, 3.5), (61.5, 3.5), (3.5, 52.5), (61.5, 52.5) | Partial (arithmetic on verified dims) | S1 Fig 2 |
| Top-edge margin above top holes (56.5 board) | 4.0 (= 56.5 − 52.5). The extra 0.5 comes from the SMT-header board height. | Partial (derived) | S1 Fig 2. S2 notes "56.5mm for SMT style GPIO header". |
| Mounting hole finished diameter | **Not stated in HAT+.** Legacy HAT: drilled 2.75 ±0.05 (for M2.5). | Unverified for HAT+ | S2 |
| Hole land / copper keepout | **Not stated in HAT+.** Legacy HAT: land min Ø6.2, isolated copper or bare board, do not connect to GND. | Unverified for HAT+ | S2 |
| 40-pin header centre | X = 3.5 + 29 = 32.5 (29 right of the left hole column). Y = 52.5: drawn on the top-hole row but not dimensioned in Y. | X Verified (29) / Y Partial | S1 Fig 2, Fig 3. The Pi 5 drawing S6 also shows 29. |
| Header pitch / layout | 2×20, 0.1-inch (2.54) pitch | Verified | S1 ch. 1 p3 |
| Pin 1 position | (8.37, 51.23) = header centre − (9.5 × 2.54, 0.5 × 2.54). Pin 1 is the square pad at the **left** end, on the row **away from** the board edge. Pin 2 is at (8.37, 53.77). | Partial (pad shape seen in the S6 vector drawing, coordinates derived) | S6 (square pad at left/inner position), S1 p3 (pitch) |
| Cutout example: display flex notch (Sense HAT) | Left edge, depth 5, height 17, from Y = 19.5 to 36.5 (20 below the top edge). Inner corner radius not dimensioned. | Verified (example) | S1 Fig 2 |
| Cutout example: camera flex slot (Sense HAT) | Obround 2 × 17 (overall), centre at X = 50, Y = 11.5 | Verified (example) | S1 Fig 2 |
| Cutout example: M.2 HAT+ | Left notch depth 5, height 18, Y = 21 to 39 (17.5 below the top edge). Bottom notch 10 wide × 16.5 tall, X = 47.5 to 57.5 (7.5 from the right edge), open to the bottom edge. | Verified (example) | S1 Fig 3 |
| Keepout: PoE header, Pi 4 | Reference point at (61.5, 46.36): the right hole column, 42.86 above the bottom-right hole. The drawing does not say whether this is the header centre or its footprint size. | Verified (distances) / Partial (meaning) | S1 Fig 4 p14 |
| Keepout: PoE header, Pi 5 | Reference point at (61.5, 9.5): 6 above the bottom-right hole. Same caveat. | Verified (distances) / Partial (meaning) | S1 Fig 4 p14 |
| Component height / stacking | Board-to-board spacers **at least 15**, **16 ideal**, to clear the Active Cooler. Use larger spacers if the underside has components. No explicit Z keepout figure. | Verified | S1 ch. 7, p13 |
| Board thickness / edge requirements | Not specified | Unverified (no value in source) | S1 |
| Electrical requirements relevant to layout | ID EEPROM on `ID_SD`/`ID_SC` (GPIO0/1, header pins 27/28) with 3.9 kΩ pull-ups to 3.3 V. Nothing else may connect to these pins. | Verified | S1 ch. 2 p5, ch. 6 p12. Pin numbers from S3 line "pin 27 (ID_SD…) and pin 28 (ID_SC…)". |
| Naming rule | "Only boards that follow the HAT+ specification can be marketed as HAT+ boards. Add-on boards that do not follow the specification can still be used, but cannot use HAT or HAT+ markings." HAT+ boards "should use 'HAT+' in their name". The HAT+ wordmark may only be used on HATs that meet the spec. | Verified | S1 §1.2 p3, §5.1 p11. S4. |
| Licensing | The spec PDF is © Raspberry Pi Ltd, **CC BY-ND 4.0**. Its disclaimer also says "RPL grants users permission to use the RESOURCES solely in conjunction with the Raspberry Pi products." Shipping *numbers* (facts) in our own template does not copy the drawing, so it is low risk. Do **not** redistribute the PDF figures. Trademark rules require referential use ("for", "compatible with") and the acknowledgement "Raspberry Pi is a trademark of Raspberry Pi Ltd". | Verified (licence text). Trademark rules: Partial, read through a summarising fetch tool, not parsed locally. | S1 colophon p1, disclaimer p1. S7. |

**"Mini HAT+":** neither the current HAT+ spec (2024-12-05, the only version published; the datasheets URL and the pip-assets `RP-008281-DS-1` copy are byte-identical) nor the HAT+ wordmark guidelines define a "mini HAT+" form factor. Status: **Unverified / no primary source**. The nearest official small form factor is the legacy **uHAT** (below). If REDA ships a "mini" template, base it on the uHAT drawing and do not label it "HAT+".

## Raspberry Pi HAT (legacy, deprecated) and uHAT

The HAT+ spec says the original HAT spec "is now deprecated. All new HATs should follow the HAT+ specification" (S1 §1.1 p3). The repo is marked deprecated (S3 README). The legacy drawing is fully dimensioned, so it is the best-defined template.

### Legacy HAT (65 × 56)

| Item | Value | Status | Source |
|---|---|---|---|
| Outline | 65 × 56 (through-hole GPIO header) **or** 65 × 56.5 (SMT GPIO header) | Verified | S2 notes and side note |
| Corner radii | R3 on all four outer corners ("BOARD MUST HAVE 3mm RADIUS CORNERS") | Verified | S2 |
| Mounting holes | 4 × M2.5, drilled **2.75 ±0.05**. Ideally non-plated. If plated, hole and land must be electrically isolated (do not connect to GND). | Verified | S2 mounting-hole notes |
| Hole land / copper keepout | Land **min Ø6.2**, either isolated copper or bare board (open solder mask) | Verified | S2 |
| Hole positions | 3.5 from the left and bottom edges, pattern 58 × 49 → (3.5, 3.5), (61.5, 3.5), (3.5, 52.5), (61.5, 52.5) | Verified (dims) / Partial (coordinates) | S2 |
| Display flex cutout | Left edge, depth 5, height 17, bottom of the cutout at Y = 19.5 (so Y = 19.5 to 36.5). Inner corners **R1**. | Verified | S2 |
| Camera flex slot | Width 2, length 17 (overall, round ends). Centreline X = 45 from the left edge. The 11.5 dimension runs from the bottom edge to the slot centre, so the slot spans Y ≈ 3.0 to 20.0. | Verified (2, 17, 45, 11.5) / Partial (11.5 is to the slot centre, measured from geometry) | S2 |
| Cutouts optional | Slots and cutouts are "recommended". A HAT that omits them still conforms. | Verified | S2 notes. S3 README FAQ. |
| 40-pin header centre | 29 right of the left hole column → X = 32.5. Drawn on the top-hole row. | Verified (29) / Partial (Y) | S2 |
| Pin 1 | (8.37, 51.23) on a 56.5 board, or (8.37, 50.73) on a 56 board, with the top hole row at Y = 52.5 in both cases. Left end, inner row. | Partial (derived, as in HAT+) | S2, S6 |
| PoE header (Pi 3B+/4) | Reference "+" at 58 right of the left hole column (X = 61.5) and 42.86 above the bottom-right hole (Y = 46.36). "Newly designed HATs that do not provide a connector for this header must avoid fouling it." | Verified | S2. S3 README note. |
| Spacing / height | GPIO connector must space the HAT **≥ 8** from the Pi (spacers 8 or larger) | Verified | S3 README "HAT requirements" item 5 |
| Board thickness | Not specified | Unverified | S2, S3 |
| HAT naming | Can be called a HAT only if it has a valid ID EEPROM, a full 40-way connector, follows the mechanical spec, uses ≥ 8 spacing, and (when back-powering) can supply ≥ 1.3 A | Verified | S3 README |
| Licensing | Repo `raspberrypi/hats` is **BSD-3-Clause** (LICENCE: "Copyright (c) 2014-2017, Raspberry Pi (Trading) Ltd"), and that covers `hat-board-mechanical.pdf`. Fine for an open-source tool. Keep the BSD notice if any file is redistributed. | Verified | S3 LICENCE @348dbf88 |

### uHAT (micro-HAT, 2018)

| Item | Value | Status | Source |
|---|---|---|---|
| Outline | 65 × 30 (through-hole connector) **or** 65 × 30.5 (SMT connector) | Verified | S5 |
| Corner radii | R3 | Verified | S5 |
| Mounting holes | Min. 3: the 2 holes next to the GPIO header are mandatory, plus at least one more in another corner | Verified | S5 |
| Hole size / land | Drilled 2.75 ±0.05. Land min Ø6.2, isolated copper or bare board. Ideally non-plated. | Verified | S5 |
| Hole positions | 3.5 from the left and bottom edges, 58 (X) × 23 (Y) → (3.5, 3.5), (61.5, 3.5), (3.5, 26.5), (61.5, 26.5) | Verified (dims) / Partial (coordinates) | S5 |
| Header centre / pin 1 | Header centre X = 32.5 (29 from the left hole), on the top-hole row Y = 26.5. Pin 1 ≈ (8.37, 25.23). | Verified (29) / Partial | S5 |
| PoE header (Pi 3B+/4) | Reference "+" at X = 61.5, 16.86 above the bottom-right hole (Y = 20.36). "BOARD MUST MAKE SURE PoE HEADER DOES NOT FOUL." | Verified | S5 |
| Cutouts | None defined | Verified (absence) | S5 |
| Licensing | Same BSD-3-Clause repo as the legacy HAT | Verified | S3 |

### Legacy "B+ add-on board" (non-HAT)

| Item | Value | Status | Source |
|---|---|---|---|
| Mechanical spec | **None.** The repo defines only *electrical* "basic requirements" for add-on boards that use the 40-way header (Model B+ onwards): ID pins used only for an ID EEPROM, safe back-powering, and protection of GPIO 6/14/16 at boot. Such boards "can't be called a HAT". | Verified | S3 README "New HATs / add-on boards basic requirements", FAQ |
| Host hole pattern (for any Pi-mounted board) | 58 × 49, 3.5 from the board edges, on the Pi 5 drawing (dimension texts 58, 49, 3.5, 29 are present). The drawing is "approximate and for reference purposes only". | Partial | S6 |
| Recommendation | Treat it as the legacy HAT outline without the EEPROM/naming claims. Do not name the template "HAT". | — | S1 §1.2, S3 |

**Sources (Raspberry Pi add-on boards)**
- S1: *Raspberry Pi HAT+ Specification*, build-date 2024-12-05, build-version 6df06cc-clean (doc ID RP-008281-DS-1). https://datasheets.raspberrypi.com/hat/hat-plus-specification.pdf redirects to https://pip-assets.raspberrypi.com/categories/1215-raspberry-pi-hat/documents/RP-008281-DS-1-hat-plus-specification.pdf. Page numbers are printed page numbers; PDF page = printed + 1.
- S2: *Raspberry Pi HAT Board Mechanical Specification* "(c) Raspberry Pi 2014/2018", `hat-board-mechanical.pdf`, github.com/raspberrypi/hats @ commit 348dbf88 (2023-12-19), 1 page.
- S3: github.com/raspberrypi/hats @ 348dbf88: `README.md`, `designguide.md`, `LICENCE`.
- S4: *HAT+ wordmark* guidelines, https://datasheets.raspberrypi.com/hat/hat-plus-wordmark-guidelines.pdf (pip-assets RP-008287-DS-1), 1 page, undated.
- S5: *Raspberry Pi Micro-HAT (uHAT) Board Mechanical Specification* "(c) Raspberry Pi 2018", `uhat-board-mechanical.pdf`, raspberrypi/hats @ 348dbf88.
- S6: *Raspberry Pi 5 mechanical drawing*, https://datasheets.raspberrypi.com/rpi5/raspberry-pi-5-mechanical-drawing.pdf (pip-assets RP-008347-DS-1), 1 page, undated.
- S7: Raspberry Pi trademark rules, https://www.raspberrypi.com/trademark-rules/ ("Last updated 17 June 2026"), sections 1, 3, 4.

---

## Raspberry Pi Pico (RP2040) as a carrier footprint

The Pico datasheet figures cover the Rev3 board. Figure 3 is the board dimensions (PDF p7, printed p6), Fig 4 the pin numbering (PDF p8), Fig 5 the SMT footprint (PDF p10, printed p9) and Fig 6 the paste stencil (PDF p11).

| Item | Value | Status | Source |
|---|---|---|---|
| Outline | 21 × 51. Single-sided PCB. Micro-USB overhangs the top (short) edge. | Verified | P1 §2 p6 text, Fig 3 |
| Board thickness | 1 mm | Verified | P1 §2 p6 |
| Corner radii | Corners drawn slightly rounded, **not dimensioned** | Unverified | P1 Fig 3 |
| USB overhang | 1.3 (typ) beyond the top edge. The USB receptacle is 8 wide and centred (the "8" dimension). | Verified (1.3) / Partial (meaning of 8) | P1 Fig 3 |
| Edge pins | 40 pins on a 2.54 (0.1") grid with Ø1 holes, castellated and through-hole ("DIP" style) | Verified | P1 §2 p6, Fig 3 (Ø1) |
| Pin columns | 17.78 apart, 1.61 from each long edge → X = 1.61 and 19.39 | Verified | P1 Fig 3 |
| Pin rows | First-to-last span 48.26 (19 × 2.54), centred → Y = 1.37 to 49.63. Vector geometry gives 1.36 / 49.63. | Verified (48.26) / Partial (1.37 offset) | P1 Fig 3 |
| Castellation pad width (along edge) | 1.6 | Verified | P1 Fig 3 |
| Pin 1 | **Top-left** next to the USB, at (1.61, 49.63). Numbering runs counter-clockwise: 1–20 down the left side, 21–40 up the right (20 bottom-left, 21 bottom-right, 40 top-right). | Verified (location/numbering) / Partial (coordinate) | P1 Fig 4 p7 |
| Mounting holes | 4 × **Ø2.1 ±0.05** drilled | Verified | P1 §2 p6, Fig 3 |
| Mounting hole positions | X = 4.8 and 16.2 (4.8 from the left edge, 11.4 apart). Y = 2.0 (bottom) and 49.0 (top, "2" from the top edge). → (4.8, 2.0), (16.2, 2.0), (4.8, 49.0), (16.2, 49.0) | Verified (4.8, 11.4, 2 top) / Partial (bottom Y = 2.0, measured from geometry, 2.003) | P1 Fig 3 |
| Mounting hole land | Ø3.8. The "3.8" dimension spans the copper ring around the bottom-right hole (measured ring Ø3.77). | Partial | P1 Fig 3 |
| SWD pins (D1–D3) | 3 through-holes on the bottom edge, 2.54 pitch, centred on X = 10.5 (4.8 + 5.7). Hole centres at Y = 1.6. Holes ≈ Ø0.95 measured. | Verified (5.7, 1.6) / Partial (diameter) | P1 Fig 3, Fig 4 |
| SMT footprint origin | "ORIGIN BOTTOM LEFT AS INDICATED" = board corner | Verified | P1 Fig 5 |
| SMT edge pads | Outer pad extents 22.58 apart, inner ends 16.18 apart → pad length 3.2, extending **0.79 beyond the board edge**. Pad width 1.6. Signal pads have a rounded inner end **R0.8**. GND pads are rectangular with **R0.2** corners. | Verified (22.58, 16.18, 0.79, R0.8, R0.2) / Partial (3.2 derived) | P1 Fig 5 |
| SMT pad vertical refs | Bottom of pad 20 is 0.57 above the board edge. 49.86 = bottom of pad 20 to top of pad 1. 46.66 and 44.78 are inner pad-edge spans. | Verified | P1 Fig 5 |
| SMT SWD pads | 3 pads spanning 6.68 overall, extending **0.8 below the board edge** | Verified | P1 Fig 5 |
| Test points (bottom side of the Pico) | 1.5 × 1.5 with 0.3 corner radius. Centres (8, 43), (8, 40.5), (8, 38), (9.5, 49.8), (11.5, 49.8), (10.5, 46.5). From the Fig 4 layout these are TP4, TP5, TP6, TP3, TP2, TP1 respectively. | Verified (coordinates) / Partial (TP mapping) | P1 Fig 5, Fig 4 |
| USB shell pads | A (7.775, 49.5), B (13.225, 49.5): 1.1 × 1.8, R0.55. C (8.075, 46.47), D (12.925, 46.47): 1.05 × 1.45, R0.525. The figure labels the second set "B, C", which is a typo for C, D. | Verified | P1 Fig 5 |
| Paste | Paste zones 163% larger than the footprint | Verified | P1 §2.2 p9 |
| Carrier keepouts | Unused test points: "acceptable to void any copper under these (with suitable clearance)". No other keepout is specified for Pico (the antenna keepout applies to Pico W only and is out of scope). | Verified | P1 §2.2 p9 |
| Licensing | Datasheet © 2022-2026 Raspberry Pi Ltd, **CC BY-ND 4.0**, with the same "solely in conjunction with the Raspberry Pi products" disclaimer. A carrier footprint is used with the product, and dimensions are facts, so ship our own geometry and do not copy the figures. "Raspberry Pi"/"Pico" naming: use referential phrasing ("for Raspberry Pi Pico") plus the trademark acknowledgement (S7). | Verified (licence) / Partial (trademark, S7) | P1 colophon PDF p2 |

## Raspberry Pi Pico 2 (RP2350) as a carrier footprint

The mechanics are the same as Pico. The vector geometry of Fig 3 was measured independently and matches to < 0.01 mm. The only footprint difference is **TP7**.

| Item | Value | Status | Source |
|---|---|---|---|
| Outline / thickness | 21 × 51, 1 mm thick, micro-USB overhanging the top edge | Verified | P2 §3 p7 (PDF p8) |
| Corner radii | Not dimensioned | Unverified | P2 Fig 3 |
| USB overhang | 1.3 (typ). USB width 8. | Verified / Partial | P2 Fig 3 |
| Edge pins | 40 pins, 2.54 grid, Ø1 holes, castellated. Columns at X = 1.61 / 19.39 (17.78 apart). Rows Y = 1.37 to 49.63 (48.26 span, measured 1.364 / 49.634). | Verified (dims) / Partial (offsets) | P2 Fig 3 |
| Pin 1 | Top-left by the USB, (1.61, 49.63), counter-clockwise numbering as on Pico | Verified (location) / Partial (coordinate) | P2 Fig 4 (PDF p9) |
| Mounting holes | 4 × Ø2.1 ±0.05 at (4.8, 2.0), (16.2, 2.0), (4.8, 49.0), (16.2, 49.0). Land Ø3.8 (measured 3.77). | Verified (2.1, 4.8, 11.4, 2) / Partial (bottom Y, land) | P2 §3 p7, Fig 3 |
| SWD pins | 3 at X ≈ 7.95, 10.51, 13.05, Y = 1.6 (measured) | Partial | P2 Fig 3 |
| SMT footprint | Identical dimension set to Pico: 22.58 / 16.18 / 0.79 / 0.57 / 0.8 / 6.68 / 49.86 / 46.66 / 44.78, R0.8, R0.2, same TP1–TP6 and A–D shell pads | Verified | P2 Fig 5 (PDF p11) |
| Extra test point | **TP7 at (11.5, 30.5)**: 1V1, "do not use". Void or clear copper under it on the carrier. | Verified | P2 Fig 5, §3.1 p9 |
| Paste / keepouts | 163% paste. Unused TPs may have copper voided. | Verified | P2 §3.2 p10 |
| Licensing | Same as Pico: © 2022-2026 Raspberry Pi Ltd, CC BY-ND 4.0 | Verified | P2 colophon PDF p2 |

**Sources (Pico)**
- P1: *Raspberry Pi Pico Datasheet*, Release 21, build date 03/07/2026, build version d4e0f1799616. https://datasheets.raspberrypi.com/pico/pico-datasheet.pdf redirects to pip-assets `RP-008307-DS-2-pico-datasheet.pdf`. Figures 3–6 are on printed pp. 6–10.
- P2: *Raspberry Pi Pico 2 Datasheet*, Release 5, build date 03/07/2026, build version d4e0f1799616. https://datasheets.raspberrypi.com/pico/pico-2-datasheet.pdf redirects to pip-assets `RP-008299-DS-3-pico-2-datasheet.pdf`. Figures 3–6 are on printed pp. 7–11.

---

## Seeed Studio XIAO (common 14-pin footprint)

All standard XIAO boards share the 14-pin outline and pinning. The board-specific parts are the bottom SMD pads, the "Plus" boards' extra pins, castellation pad sizes, and occasional internal slots. Seeed's own documents disagree on the outline by up to 0.05 mm, and one datasheet is badly off. The table below gives the values from the actual CAD files.

| Item | Value | Status | Source |
|---|---|---|---|
| Outline (nominal, marketing) | 21 × 17.8 | Verified | X1 spec table ("Dimensions 21 x 17.8mm") |
| Outline (CAD) | **20.955 × 17.78** (0.825" × 0.7"). Edge.Cuts in the XIAO RP2350 DXF, with dimension entities measuring 20.955 and 17.78. Seeed KiCad footprints use 17.78 wide × ~20.955–21.0 long, depending on the file. | Verified | X3 Edge.Cuts + DIMENSION entities. X4 footprints. |
| Conflicting value | SOM datasheet says "only 21x17.5mm" and "20×17.5×3.5mm". Treat as an error. | Verified (that it conflicts) | X5 text |
| Corner radii | **R1.905** (0.075") on all four corners | Verified | X3 Edge.Cuts arcs (bulge 0.4142 = 90°, 1.905 legs). X4 silk arcs. |
| Mounting holes | **None** | Verified (absence) | X3 Edge.Cuts, X4 footprints |
| Pins | 14 (7 per long side), 2.54 pitch | Verified | X4 all `*-DIP` footprints. X1 "common 14-pin XIAO footprint". |
| Pin columns | 15.24 apart, centred → X = 1.27 and 16.51 (1.27 from each long edge) | Verified (±7.62 hole positions vs ±8.89 outline in X4) | X4 `XIAO-RP2040-DIP` and others |
| Pin rows | 7 rows, first/last 2.88 from the short edges → Y = 2.88 to 18.12 (on a 21 board) | Verified | X4 `XIAO-RP2040-SMD` (corner origin, pads at y = 2.88 to 18.12) |
| Pin 1 | **Top-left, at the USB end**: (1.27, 18.12). Pins 1–7 run down the left side, 8–14 up the right (8 bottom-right, 14 top-right). | Verified | X4: in the DIP footprint, pad 1 is at (+7.62, −7.62) with the USB at +X. In the SMD footprint, pad 1 is at (0.835, 18.12) with the USB overhang at the top. |
| Through-hole pins (DIP footprint) | Drill 0.889, pad Ø1.524 | Verified | X4 `*-DIP` |
| Castellation SMD pad (varies) | Older boards (SAMD21, RP2040, ESP32-C3/C6/S3, nRF52840, nRF54L15 DIP): 2.432 × 1.524 centred 8.455 from the board centreline, so the pad extends 0.78 past the edge. Newer boards (RP2350, MG24, R4, nRF54L15 SMD): 2.75 × 2.0, centred 8.0825 from centre. "Plus" boards: 2.032 × 0.95. Older corner-origin SMD footprints: 2.75 × 2.0 at X = 0.835 / 17.0. | Verified (per file) | X4 |
| USB connector overhang | ≈ 1.41 beyond the top edge, ≈ 9.0 wide (from silkscreen) | Partial (silk, not dimensioned) | X4 `XIAO-RP2040-SMD` silk |
| Board-specific cutouts | XIAO RP2350 only: two small obround internal slots, 1.27 × 0.508, near one short edge | Verified (that board) | X3 Edge.Cuts |
| Bottom pads (varies by board) | Example, RP2040: 4 × Ø1.7 pads at (7.747, 18.878), (10.287, 18.878), (7.747, 16.338), (10.287, 16.338), near the USB end, and 2 × 2.5 × 1.1 pads at (7.747, 1.86) and (10.287, 1.86), at the far end. Origin is the bottom-left corner with the USB at the top. **Different on every model.** Do not put these in a generic template. | Verified (RP2040 file) | X4 `XIAO-RP2040-SMD` |
| Board thickness | **Not found.** The only PCB file found (XIAO ESP32-C6 KiCad project) uses KiCad's default 1.6 mm 4-layer stackup, which may not be the fabricated value. | Unverified | X6 |
| Keepouts (antenna etc.) | Not researched in Seeed primary sources. The wireless variants have on-board or u.FL antennas. | Unverified | — |
| Licensing | The Seeed footprint ZIPs (X4) contain **no licence file**. Seeed's KiCad library repo `Seeed-Studio/OPL_Kicad_Library` is **CC-BY-SA-4.0**. The wiki source repo `Seeed-Studio/wiki-documents` is **GPL-3.0**. Shipping our own geometry built from the dimensions is safest. Do not bundle Seeed's .kicad_mod/.dxf files. Whether "XIAO" is a registered trademark was **not verified**: use referential naming ("XIAO-compatible footprint, Seeed Studio XIAO") until checked. | Verified (licences) / Unverified (trademark) | GitHub API licence fields, X4 |

**Sources (XIAO)**
- X1: Seeed wiki *Getting Started with Seeed Studio XIAO RP2040*, source `sites/en/docs/Sensor/SeeedStudio_XIAO/SeeedStudio_XIAO_RP2040/XIAO-RP2040.md` in github.com/Seeed-Studio/wiki-documents (last commit 11a2865d, 2026-10-12), lines 54, 74, 184. Published at https://wiki.seeedstudio.com/XIAO-RP2040/.
- X2: Seeed wiki *XIAO Introduction*, https://wiki.seeedstudio.com/SeeedStudio_XIAO_Series_Introduction/ (source `sites/en/.../SeeedStudio_XIAO_Series_Introduction.md`). Lists the DXF and footprint downloads.
- X3: `XIAO-RP2350-dimension-v1.0.dxf`, https://files.seeedstudio.com/wiki/XIAO-RP2350/res/XIAO-RP2350-dimension-v1.0.dxf (AutoCAD 2007 DXF, layer `XIAORP2350-EDGE_CUTS_GBR`).
- X4: Seeed XIAO KiCad footprints: https://files.seeedstudio.com/wiki/XIAO-KiCad-Library/New_XIAO_Series_Footprints.zip (KiCad 9 files) and https://files.seeedstudio.com/wiki/XIAO/Seeed_Studio_XIAO_Series-20240814.pretty.zip.
- X5: *Seeed Studio XIAO Series SOM Datasheet*, https://files.seeedstudio.com/wiki/XIAO/Seeed-Studio-XIAO-Series-SOM-Datasheet.pdf (34 pp, undated).
- X6: `XIAO-ESP32-C6_v1.0_SCH&PCB_24028.zip`, https://files.seeedstudio.com/wiki/SeeedStudio-XIAO-ESP32C6/XIAO-ESP32-C6_v1.0_SCH&PCB_24028.zip (`XIAO ESP32 C6.kicad_pcb`).

---

## Open items before shipping

1. HAT+: pick hole size and land for the template. The legacy values 2.75 ±0.05 / Ø6.2 are the only primary numbers. The Pi 5 drawing also carries a Ø2.7 label, but the rasterised drawing could not show which feature it belongs to.
2. HAT+/HAT: the header Y position (on the top-hole row) and pin 1 coordinates are derived. Check them against a Raspberry Pi KiCad/STEP model if one is published.
3. PoE header keepout size (Pi 4 and Pi 5) is not dimensioned anywhere read here. Only the reference point is.
4. Pico/Pico 2 corner radius is not dimensioned.
5. XIAO board thickness, antenna keepouts and trademark status are unverified.
6. "Mini HAT+" does not exist in any primary source found. Confirm with the product owner what was meant (likely the uHAT).
