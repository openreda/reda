# Hobby form factors: primary-source mechanical specs for REDA Board Templates

Researched 2026-11-20. Only primary sources were used: vendor CAD/EAGLE/DXF files, vendor docs and vendor GitHub repos. Most numbers were read straight from the vendors' own design files (EAGLE `.brd` XML, Excellon drill files, Altium Gerbers, DXF), not from rendered drawings. The downloads are in `/data/mij/tmp/claude/ff-dl-hobby/` (untrusted, nothing was executed). The parsing helpers are in `/data/mij/scratch/claude/ff-specs/tools/`.

**Status legend**
- **Verified**: read directly from a primary source, either the design file or explicit vendor text.
- **Partial**: taken from a primary source but needs interpretation, or the sources disagree slightly, or only one example board backs it.
- **Unverified**: not found in any primary source. Do not ship as fact.

**Conventions**: all values are in mm unless stated. "Origin" is given per form factor and is the lower-left corner of the board outline in the vendor file, top view, unless stated otherwise. EAGLE coordinates are the element/pad centres from the `.brd` XML. Layer names follow EAGLE: 20 Dimension, 39/40 t/bKeepout, 41/42 t/bRestrict.

**General licensing note (not legal advice).** Bare dimensions, hole positions and header pitches are largely functional facts. REDA can re-draw a template from them without copying anyone's design file. Even so, add attribution where the source is CC BY-SA (Arduino, Adafruit, SparkFun, Espressif docs). Do not ship the vendors' `.brd`/`.dxf` files themselves. Trademarks are the larger risk: template names should use referential wording ("Uno-R3-compatible shield", "for Arduino"), and should not put vendor names or logos on the generated board silkscreen.

---

## Arduino Uno shield (Uno R3; Uno R4 noted)

Origin: lower-left corner of the Uno PCB in `UNO-TH_Rev3e.brd`, top view, with the USB-B and DC jack on the left edge (x = 0), digital headers along the top edge (y ≈ 50.8) and power/analog headers along the bottom edge (y = 2.54).

| Item | Value | Status | Source |
|---|---|---|---|
| Outline extents | 68.58 x 53.34 (2.700" x 2.100"). x 0 to 68.58, y 0 to 53.34 | Verified | [A1] layer 20 wires |
| Outline shape | Left edge straight. Top edge to (64.516, 53.34), 45° chamfer to (66.04, 51.816), down to (66.04, 40.386), 45° chamfer out to (68.58, 37.846), straight to (68.58, 5.08), 45° chamfer in to (66.04, 2.54), down to (66.04, 1.0) | Verified | [A1] |
| Corner radii | R1.0 at (0,0), (0,53.34) and (66.04,0) (90° arcs). The other corners are chamfer vertices (sharp) | Verified | [A1] |
| Cutouts / slots | None in the outline | Verified | [A1] |
| Mounting holes | 4x NPTH, drill 3.2 at (13.97, 2.54), (15.24, 50.8), (66.04, 7.62), (66.04, 35.56) | Verified | [A1] `<hole>` in `<plain>`; R4 matches, see below |
| Hole pad / keepout | No pad and no restrict ring around the holes in the Arduino file | Verified (absence) | [A1] |
| Recommended shield keepout around holes | Not published by Arduino | Unverified | none found |
| Digital header D0–D7 ("IOL", 1x8 female) | y = 50.8. D0 at x = 63.5 (EAGLE pad 1), then D1 60.96, … D7 at 45.72. Pitch 2.54 | Verified | [A1] pads + nets IO0..IO7 |
| Digital header D8–D13, GND, AREF, SDA, SCL ("IOH", 1x10) | y = 50.8. D8 at x = 41.656 (pad 1), D9 39.116, D10/SS 36.576, D11/MOSI 34.036, D12/MISO 31.496, D13/SCK 28.956, GND 26.416, AREF 23.876, SDA 21.336, SCL 18.796 (pad 10) | Verified | [A1] pads + nets |
| Off-grid gap | D7 (45.72) to D8 (41.656) = 4.064 = **160 mil**. The whole IOH header sits 40 mil (1.016) off the 100-mil grid used by the other three headers | Verified | [A1], computed from the pad coordinates |
| Power header ("POWER", 1x8) | y = 2.54. Pad 1 (NC) at x = 27.94, IOREF 30.48 (net +5V on Uno), RESET 33.02, 3V3 35.56, 5V 38.1, GND 40.64, GND 43.18, VIN 45.72 | Verified | [A1] pads + nets |
| Analog header ("AD", 1x6) | y = 2.54. A0 at x = 50.8 (pad 1) through A5 at 63.5. Gap from VIN (45.72) to A0 = 5.08 (on grid) | Verified | [A1] |
| ICSP (ATmega328P, 2x3 male) | Pins at x 63.627/66.167, y 30.48/27.94/25.4. Pin 1 (MISO) at (63.627, 30.48), pin 2 (+5V) at (66.167, 30.48). Drill 0.95, pad 1.55 | Verified | [A1] |
| Header drill / pad (Arduino's own footprint) | Drill 0.85, pad 1.4224 long (oblong) | Verified | [A1] |
| USB-B and DC jack overhang | USB-B body (X2) and DC jack (X1) sit at the left edge and protrude past x = 0. The overhang is not dimensioned in the file | Unverified | [A1] (no figure) |
| Board thickness | Not stated in the datasheet text. The EAGLE DRC stackup shows a 1.5 mm core plus 2 x 35 µm copper (≈1.57). Treat 1.6 as convention only | Unverified | [A1] designrules `mtIsolate`/`mtCopper`; [A2] has no figure |
| Edge requirements | None stated | Unverified | none |
| **Uno R4 Minima** | Excellon drill file: 4x NPTH 3.2 at the identical 4 positions. All 32 header holes (0.85) at identical positions, including the 41.656 off-grid row. ICSP holes (0.95) identical. The route/tool-path Gerber (GM3) traces the same outline (1.0 mm offset path) | Verified (holes/headers); Partial (outline, inferred from the route path) | [A3] `PCB-RoundHoles.TXT`, `PCB.DRR` (30/08/2023), `PCB.GM3` |
| Licensing | Design files: CC BY-SA 4.0 (`License.txt` in the CAD zip) | Verified | [A1], [A3] |
| Trademark | "It is not allowed to call your compatible product 'Arduino'", "It is not allowed to print the word Arduino or the Arduino logo on the board", and "Always place the word Arduino last" ("Ladybird Shield for Arduino"). The policy says nothing about board shape or form factor. Whether "Uno" itself is a registered mark was not checked | Verified (quotes); Unverified (status of "Uno") | [A4], [A5] |

Sources:
- [A1] Arduino UNO R3 CAD files, `A000066-cad-files.zip` → `UNO-TH_Rev3e.brd` (EAGLE 9.3.1) and `License.txt`. https://docs.arduino.cc/static/6bb7a3ca51ebee82a252f60c0b418787/A000066-cad-files.zip, linked from https://docs.arduino.cc/hardware/uno-rev3/
- [A2] Arduino UNO R3 datasheet A000066, rev 5 (12/11/2026), "Modified 19/11/2026", 14 pp. https://docs.arduino.cc/resources/datasheets/A000066-datasheet.pdf. §5.3/5.4 (Mechanical, Board Outline & Mounting Holes, p. 12) are images only; their text could not be extracted, so nothing was taken from them.
- [A3] Arduino UNO R4 Minima CAD files, `ABX00080-cad-files.zip` (Altium, Gerber, NC drill; drill report dated 30/08/2023). https://docs.arduino.cc/static/ae97aad5c05de6a565c7f93a28c04717/ABX00080-cad-files.zip. Datasheet ABX00080 rev 6, with "Mechanical drawing update" in rev 5 (10/29/2025).
- [A4] Arduino, "Trademark & Copyright", last revision Nov 25, 2020. https://www.arduino.cc/en/trademark/
- [A5] Arduino, "Trademark guide for compatible products", last revision Oct 31, 2025. https://www.arduino.cc/en/trademark/guides/trademark-guide-for-compatible-products/

---

## Arduino Mega 2560 shield

Origin: EAGLE origin of `MEGA2560_Rev3e.brd`, top view, USB/jack on the left. **Note:** the outline in this file is offset by (-0.0256, -0.0128) from the origin. Holes and headers are on the clean grid, so subtract the outline's lower-left corner if you re-base on the outline.

| Item | Value | Status | Source |
|---|---|---|---|
| Outline extents | 101.6 x 53.34 (4.000" x 2.100"). x -0.0256 to 101.5744, y -0.0128 to 53.3272 | Verified | [M1] layer 20 |
| Outline shape | Same right-hand profile as the Uno, shifted +33.02 in x: chamfer (97.5104, 53.3272)→(99.0344, 51.8032), down to 40.3732, chamfer out to (101.5744, 37.8332), down to 5.0672, chamfer in to (99.0344, 2.5272), down to 0.9872 | Verified | [M1] |
| Corner radii | R1.0 at the lower-left, upper-left and lower-right (99.03, 0) corners | Verified | [M1] |
| Cutouts | None | Verified | [M1] |
| Mounting holes | 6x NPTH drill 3.2: (13.97, 2.54), (15.24, 50.8), (66.04, 7.62), (66.04, 35.56), (90.17, 50.8), (96.52, 2.54) | Verified | [M1] `<hole>` |
| Hole keepout | None in the file | Verified (absence) | [M1] |
| Uno-compatible headers | JP6 (1x10): SCL at x = 18.796 (pad 1) to D8 at 41.656, y = 50.8. PWML (1x8): D0 (PE0) at 63.5 (pad 1) to D7 at 45.72, y = 50.8. POWER (1x8): pad 1 NC at 27.94 to VIN at 45.72, y = 2.54. All identical to the Uno positions, including the 160-mil D7–D8 gap | Verified | [M1] pads + nets |
| COMMUNICATION (1x8) | y = 50.8. D14/TX3 at x = 68.58 (pad 8) to D21/SCL at 86.36 (pad 1) | Verified | [M1] |
| ADCL A0–A7 (1x8) | y = 2.54. A0 at 50.8 (pad 1) to A7 at 68.58 | Verified | [M1] |
| ADCH A8–A15 (1x8) | y = 2.54. A8 at 73.66 (pad 1) to A15 at 91.44 | Verified | [M1] |
| XIO double row (2x18) | Columns x = 93.98 / 96.52, rows y = 50.8 down to 7.62, pitch 2.54. Pad 1 (+5V) at (93.98, 50.8), pad 2 (+5V) at (96.52, 50.8). Pads 35/36 (GND) at y = 7.62. Drill 0.95, pad 1.6764 | Verified | [M1] |
| ICSP (2x3) | Same position as the Uno: pin 1 (MISO) at (63.627, 30.48). Drill 1.016 | Verified | [M1] |
| Thickness / edges | Not stated | Unverified | none |
| Licensing / trademark | CC BY-SA 4.0 (`License.txt`). Trademark as for the Uno [A4][A5] | Verified | [M1] |

Sources:
- [M1] Arduino Mega 2560 Rev3 CAD files, `A000067-cad-files.zip` → `MEGA2560_Rev3e.brd` and `License.txt`. https://docs.arduino.cc/static/00ab83283ad8f7aae17832d0fe1b1d51/A000067-cad-files.zip, linked from https://docs.arduino.cc/hardware/mega-2560/. Datasheet A000067 rev 4 (20/10/2026): §6.1/6.2 Board Outline and Mount Holes are images and were not used.

---

## Arduino Nano (carrier footprint)

Origin: lower-left corner of the Nano PCB in `NanoV3.3.brd`, top view. USB Mini-B at the left (x ≈ 0), pin rows along the long edges.

| Item | Value | Status | Source |
|---|---|---|---|
| PCB outline | 43.18 x 17.78 (1.70" x 0.70"), rectangle | Verified | [N1] layer 20 |
| Corner radii | None (square corners) | Verified | [N1] |
| Overall size incl. USB | 45 x 18 ("45 mm long and 18 mm wide", 7 g). The PCB itself is 43.18 x 17.78, so the extra length is the USB connector overhang | Verified (product page text); Partial (overhang reasoning) | [N2] |
| Nano's own mounting holes | 4x drill 1.651 at (1.27, 1.27), (1.27, 16.51), (41.91, 1.27), (41.91, 16.51) | Verified | [N1] `<hole>` |
| Hole keepout | tRestrict and bRestrict circles r = 1.143 (line width 0.254) around each hole | Verified | [N1] layers 41/42 |
| Header J1 (1x15) | y = 16.51. Pad 1 = D1/TX at x = 39.37 (square pad), pad 2 D0/RX 36.83, RESET, GND, D2…D10, D11/MOSI at 6.35, D12/MISO at 3.81 (pad 15) | Verified | [N1] pads + nets |
| Header J2 (1x15) | y = 1.27. Pad 1 = VIN at x = 39.37 (square), GND, RESET, +5V, A7, A6 … A0, AREF, 3V3 at 6.35, D13/SCK at 3.81 (pad 15) | Verified | [N1] |
| Carrier footprint geometry | 2 x 15 at 2.54 pitch. Row spacing 15.24 (600 mil, y 1.27 ↔ 16.51). Pins run x 3.81 → 39.37 (35.56 span) | Verified | [N1] |
| Header drill / pad | Drill 0.9144, pad 1.778 (pad 1 square) | Verified | [N1] |
| ICSP J4 (2x3) | Pin 1 at (41.8084, 6.35), pins at x 39.2684/41.8084, y 6.35/8.89/11.43. Drill 1.016 | Verified | [N1] |
| Thickness | Not stated | Unverified | none |
| Licensing / trademark | CC BY-SA 4.0. Trademark as for the Uno | Verified | [N1], [A4], [A5] |

Sources:
- [N1] Arduino Nano CAD files, `A000005-cad-files.zip` → `NanoV3.3.brd` (EAGLE 7.7.0), `License.txt`. https://docs.arduino.cc/static/e3c4fe1f9bee7ec63df398422b3257f4/A000005-cad-files.zip
- [N2] https://docs.arduino.cc/hardware/nano/ (product page, dimensions/weight). Datasheet A000005 rev 5 (20/10/2026).

---

## Adafruit Feather (mainboard)

Origin: lower-left corner of the outline, top view, USB end on the left (x = 0). The 16-pin row is at the bottom and the 12-pin row at the top, matching the Feather spec page ("left" = USB side).

| Item | Value | Status | Source |
|---|---|---|---|
| Outline | 50.8 x 22.86. Spec: "The 'classic' Feather and Wing size is 0.9" x 2.0"" | Verified | [F1] "Feather & Wing Sizes"; [F2][F3] layer 20 |
| Corner radii | R2.54 (0.1") on all four corners | Verified | [F2], [F3] (not stated on [F1]) |
| Cutouts | Feather M0 Basic rev C has a 0.508-deep notch at the USB edge, x 0→0.508 for y 8.89–13.97 (45° in/out). Feather RP2040 has none. So this is product-specific, not part of the spec | Verified (per board) | [F2], [F3] |
| Mounting holes | 4 holes at 0.1" (2.54) from both edges: (2.54, 2.54), (2.54, 20.32), (48.26, 2.54), (48.26, 20.32). Spec text: "0.1" holes at each corner" | Verified | [F1]; [F2][F3] |
| Hole finish | USB end: plated, drill 2.5, pad 3.2 (`MOUNTINGHOLE_2.5_PLATED`). Far end: NPTH, drill 2.54 | Verified | [F2], [F3] |
| Hole keepout | USB-end holes: tKeepout/bKeepout/tRestrict/bRestrict circle r = 1, width 2.032, so the keepout disc is ≈ Ø4.03. Far-end NPTH holes: no keepout in the file | Verified | [F2], [F3] package |
| 16-pin header | y = 1.27 (0.05" from the bottom edge), x 6.35 → 44.45, pitch 2.54, centred at x = 25.4. Spec: "16-pin breakout strip on the bottom side, centered 1.0" from the left edge" | Verified | [F1], [F2], [F3] |
| 12-pin header | y = 21.59, x 16.51 → 44.45, centred at x = 30.48. Spec: "12-pin breakout strip on the top side, 1.2" from the left side". Strip spacing 0.8" (20.32) | Verified | [F1], [F2]. [F3] (RP2040) places it at y = 21.6535, a +0.0635 deviation in that product |
| Pin order (spec) | With USB at top, left column top→bottom (= 16-pin, from the USB end): Rst, 3.3V, Aref, GND, A0, A1, A2, A3, A4/D24, A5/D25, SCK, MO, MI, RX/D0, TX/D1, * (free). Right column (12-pin): Bat, En, USB, D13, D12, D11, D10, D9, D6, D5, SCL, SDA | Verified | [F1] "Pin Naming" |
| Pin 1 coordinates | Adafruit's EAGLE number pad 1 at the end away from USB: 16-pin pad 1 at (44.45, 1.27), pad 16 (RESET) at (6.35, 1.27). 12-pin pad 1 (VBAT) at (16.51, 21.59), pad 12 (SDA) at (44.45, 21.59) | Verified | [F2] pads + nets |
| Header drill / pad | Drill 1.0, pad 1.6764 (mainboards) | Verified | [F2], [F3] |
| LiPo JST-PH | Spec: "must be a JST 2-PH … 0.425" from the left edge" → x = 10.795. M0: connector origin (10.795, 19.304), signal pads at (9.795/11.795, 15.604). RP2040 uses x = 10.922 | Verified (spec); Partial (y and the per-board deviation) | [F1], [F2], [F3] |
| USB | Spec: "micro USB connector in the center left of the board". Newer boards use USB-C at y = 11.43 (centre) | Verified | [F1]; [F2] X3 at y = 11.43, [F3] X6 at y = 11.43 |
| Longer variants | Allowed (e.g. the 32u4 FONA Feather, same width, longer), but the USB side must stay the same. Length not specified | Verified (text); Unverified (any length) | [F1] |
| Thickness | Not specified | Unverified | none |
| Licensing | Board repos: CC BY-SA 3.0 Unported (`license.txt`), README asks that its text be kept in redistributions. The spec page says nothing on licence or trademark but invites compatible boards ("Adafruit encourages you to do so!") | Verified | [F1], [F2] |
| Trademark | No Adafruit trademark policy for "Feather" found on adafruit.com. The only statement found was in a third-party report (Hackaday), so it is not used here | Unverified | none primary |

## Adafruit FeatherWing

Same origin and orientation as the Feather.

| Item | Value | Status | Source |
|---|---|---|---|
| Outline | 50.8 x 22.86, R2.54 corners, no notch. Spec: Wings "don't have to keep the same size" | Verified | [F1]; [F4] layer 20 |
| Mounting holes | (2.54, 2.54) and (2.54, 20.32): plated, drill 2.5, pad 3.81. (48.26, 2.54) and (48.26, 20.32): NPTH drill 2.54 | Verified | [F4] `FEATHERWING` package |
| Header positions | `FEATHERWING` package: 16 holes at y = 1.27, x 6.35→44.45, and 12 holes at y = 21.59, x 16.51→44.45. Drill 1.0, pad 1.778. Same as the Feather | Verified | [F4] |
| Keepouts | None in the package | Verified (absence) | [F4] |
| Thickness | Not specified | Unverified | none |
| Licensing | CC BY-SA 3.0 (repo `license.txt`) | Verified | [F4] |

Sources:
- [F1] Adafruit Learn, "Introducing Adafruit Feather" → "Feather Specification" (no revision date shown; the Pin Naming section was added May 26 2021). https://learn.adafruit.com/adafruit-feather/feather-specification. Fab-print image: https://cdn-learn.adafruit.com/assets/assets/000/046/726/original/arduino_adafruit_products_fabprint.png (shows 2.0 / 1.8 / 0.9 / 0.7 / 0.1 in).
- [F2] github.com/adafruit/Adafruit-Feather-M0-Basic-Proto-PCB, `Adafruit Feather M0 Basic rev C.brd`, `license.txt`, README (master)
- [F3] github.com/adafruit/Adafruit-Feather-RP2040-PCB, `Adafruit Feather RP2040.brd` (main, EAGLE 9.6.2)
- [F4] github.com/adafruit/Adafruit-FeatherWing-Proto-Doubler-Tripler-and-Quad, `Proto FeatherWing.brd` (master), `license.txt`

---

## SparkFun MicroMod: Processor Board (M.2 2222, key E)

Origin: card frame of SparkFun's `M.2-CARD-E-22` package. x = 0 is the card centreline, y = 0 is the contact edge, +y runs into the board, top view (pin 1 side at +x).

| Item | Value | Status | Source |
|---|---|---|---|
| Size | 22 x 22 (x -11…+11, y 0…22). "22x22mm MicroMods Processor Boards (note the '2222' text)" | Verified | [S1]; [S2] package |
| Card-edge tongue | x ±9.925 for y 0 → 3.5, then R0.5 fillet out to ±10.425 at y = 4, then R0.575 out to the body width ±11 at y = 4.575 | Verified | [S2] layer 47 |
| Key (E) slot | Slot x 2.025 → 3.225 (1.2 wide), depth 2.9 from the contact edge, semicircular end (R0.6) | Verified | [S2] |
| Corner radii | Far (top) corners R1.0, at (±10, 22)/(±11, 21) | Verified | [S2] |
| Retention notch | Half-round notch in the top edge, centre (4, 22), R1.75 (Ø3.5) for the M2.5 screw. Copper/GND ring to R2.75 (annular polygon on top/bottom, 3 GND vias) | Verified | [S2] |
| Keepouts | t/bKeepout rect x ±10, y 2 → 4 (both sides). Extra bKeepout x ±11, y 4 → 5.2. Tutorial: "Heed the top and bottom keepout areas near the edge … part of the M.2 spec" | Verified | [S2] layers 39/40; [S1] |
| Contacts | 75 positions, 0.5 pitch, 67 pads (key removes 8). Pin 1 (top side) at (9.25, 1.275), pad 0.35 x 1.45. Pin 75 at (-9.25, 1.275). Even pins on the bottom at y = 1.525, 0.35 x 1.95 | Verified | [S2] |
| Thickness / build | "MicroMod PBs are 4-layer boards, 0.8mm thickness, ENIG finish (for the gold fingers) with a 20° edge bevel." SparkFun reports a 45° chamfer works ("45° chamfer works great") | Verified | [S1] |
| Component height under PB (carrier side) | With the 4.2 mm connector, parts under the PB must be "less than 1.9mm +/-0.1mm tall" | Verified | [S1] |

## SparkFun MicroMod: carrier-board connector placement

Origin: SparkFun `M.2-CONNECTOR-E` package origin, which is the line through the two locating pegs. +y is the direction the inserted card extends.

| Item | Value | Status | Source |
|---|---|---|---|
| Connector | TE 2199230-4, M.2 key E, 4.2 mm height | Verified | [S1] |
| Locating holes | NPTH Ø1.1 at (+10, 0) and Ø1.6 at (-10, 0) | Verified | [S2] |
| Pin 1 / anchors | Pin 1 pad at (9.25, -5.275). Mechanical anchor pads A1/A2 at (±10.35, -4.5), 1.2 x 2.75 | Verified | [S2] |
| Card position | The inserted card's contact edge sits at y = -1.75, so a 2222 card's far edge is at y = 20.25 | Verified | [S2] layer 51 card outline |
| Standoff (M2.5 reflowable) | Centre (4.0, 20.25) from the connector origin (the package draws a R3 circle there). Footprint `STAND-OFF-REFLOW-M2.5`: pad Ø6.0, drill 3.8. Tutorial: "It needs to be close but not micron perfect" | Verified | [S2]; [S3] (SparkFun Main Board Single v21 places it at offset (3.959, 20.224)) |
| Other carrier keepouts | None formally specified beyond the component-height rule above | Unverified | none |

## SparkFun MicroMod: Function Board

Origin: card frame of `M.2-CARD-E-22_FUNCTION_STANDARD` (x = 0 at the card centreline, y = 0 at the contact edge).

| Item | Value | Status | Source |
|---|---|---|---|
| Size | "65mm X 38mm (2.56" x 1.5")", "0.8mm thick". Real board: x 0.05 → 38.05 (38.0 wide), y 0 → 65, card centre at x = 19.05 | Verified | [S1] "Physical Dimensions and Layout"; [S4] layer 20 |
| Library vs real board | The library package outline draws the body as ±18.746 (37.49 wide) on layer 51, while the shipped board and the connector package use ±19 / ±19.05 (38.0–38.1). Use 38.0 | Partial (conflict noted) | [S2] vs [S4] |
| Card edge / key | Same tongue and key E slot as the Processor Board. Body starts at y = 4 (square corners in the shipped board) | Verified | [S2], [S4] |
| Retention notches | 2 half-round notches R1.75 centred on the long edges at (±19, 44) (card frame). Copper ring R2.75. "There are and must be at least two standoffs" | Verified | [S2], [S4]; [S1] |
| Carrier standoff positions | (±19.05, 42.245) from the function-connector origin (`M.2-CONNECTOR-E-FUNCTION-STANDARD`, R2.78 circles). Card contact edge at y = -1.75 | Verified | [S2]; [S3] places them at (-19.19, 42.24) and (19.06, 42.27) |
| Connector edge | "The edge opposite the card edge always includes any connectors needed by the Function Board." | Verified | [S1] |
| Keepouts | Same contact-area keepouts as the Processor Board (t/bKeepout y 2–4, bKeepout y 4–5.2) | Verified | [S2] |
| Licensing | SparkFun hardware and the Eagle library: CC BY-SA 4.0. The library readme asks (but does not require) an email notice when selling products that use the footprint | Verified | [S2] readme; [S3] LICENSE.md |
| Trademark | "MicroMod" naming policy not found | Unverified | none |

Sources:
- [S1] SparkFun Learn, "Designing with MicroMod" (all pages). https://learn.sparkfun.com/tutorials/designing-with-micromod/all. Function-board drawing: https://cdn.sparkfun.com/assets/learn_tutorials/1/2/0/6/Function_Board_MM.JPG (65.00 / 61.00 / 40.00 / 38.00 / 13 / 5 mm)
- [S2] github.com/sparkfun/SparkFun-Eagle-Libraries (main), `SparkFun-MicroMod.lbr`: packages `M.2-CARD-E-22`, `M.2-CONNECTOR-E`, `M.2-CARD-E-22_FUNCTION_STANDARD`, `M.2-CONNECTOR-E-FUNCTION-STANDARD`; `readme.md` (licence)
- [S3] github.com/sparkfun/SparkFun_MicroMod_Main_Board_Single, `Hardware/SparkFun_Main_Board_Single_v21.brd`, `LICENSE.md`
- [S4] github.com/sparkfun/MicroMod_Function_Ethernet-W5500, `Hardware/MicroMod-Ethernet-Function-Board-W5500.brd`

---

## SparkFun Qwiic (connector convention and 1"-grid boards)

Origin: lower-left corner of the board, top view.

| Item | Value | Status | Source |
|---|---|---|---|
| Connector | "SM04B-SRSS-TB(LF)(SN) or equivalent" (JST SH, 4-pin, 1.0 mm). "You must use the same style 4-pin connector … you may not use a 5-pin or a different size connector." | Verified | [Q1] FAQ |
| Pinout | "GND / 3.3V / SDA / SCL". Cable colours black/red/blue/yellow. Boards "must be 3.3V" | Verified | [Q1] |
| Connector pin 1 | GND is pad 1 in SparkFun's `JST04_1MM_RA` footprint (checked on the TMAG5273 1x1 board nets) | Verified | [Q2] |
| Name use | "You can use the name Qwiic without royalties or attribution." SparkFun "encourages the use of Qwiic in silk on a PCB" | Verified | [Q1] |
| Standard board size | 1" x 1" (25.4 x 25.4), square corners | Partial: vendor product docs call it the "regular, 1" x 1"" board and two SparkFun board files match, but there is no general Qwiic mechanical standard document | [Q3] text; [Q2], [Q4] layer 20 |
| Mini board size | 1.0" x 0.5" (25.4 x 12.7) in the TMAG5273 Mini file. The doc text says "0.5"x 1" (1.77 x 2.54 cm)", which contradicts itself (0.5" = 1.27 cm) | Partial | [Q2] Mini `.brd`; [Q3] |
| Micro board size | Not found in a primary source | Unverified | none |
| Mounting holes (1x1) | NPTH drill 3.302 (0.13"), "standard 0.13" mounting holes, which are compatible with 4-40 screws", at (2.54, 2.54), (22.86, 2.54), (2.54, 22.86), (22.86, 22.86), i.e. 0.1" in from each edge, 0.8" centres | Verified (TMAG5273 1x1); Partial as a family rule | [Q2] `STAND-OFF`; [Q3] |
| Hole count varies | Qwiic Button (1x1) has only 2 holes, at (2.54, 22.86) and (22.86, 22.86). Do not hard-code 4 holes | Verified | [Q4] |
| Hole keepout | t/bRestrict ring r = 1.8542 (w 0.2032, outer ≈ Ø3.9). tKeepout circle r = 2.794 (Ø5.59) for the screw head | Verified | [Q2] `STAND-OFF` package |
| Connector placement on 1x1 | TMAG5273: right-angle connectors centred at (5.08, 12.7) and (20.32, 12.7), mid-height on the left and right edges. Example only, not a rule | Partial | [Q2] |
| Thickness | Not specified | Unverified | none |
| Licensing | SparkFun hardware CC BY-SA 4.0 | Verified | [Q2] LICENSE.md |

Sources:
- [Q1] https://www.sparkfun.com/qwiic, FAQ section "Can I use Qwiic on my system?", "What are the part numbers?" (fetched 2026-11-20)
- [Q2] github.com/sparkfun/SparkFun_Qwiic_Hall_Effect_Sensor_TMAG5273: `Hardware/1x1/Qwiic_Hall_Effect_Sensor_TMAG5273.brd`, `Hardware/Mini/…_Mini.brd`, `LICENSE.md`
- [Q3] https://docs.sparkfun.com/SparkFun_Qwiic_Hall_Effect_Sensor_TMAG5273/hardware_overview
- [Q4] github.com/sparkfun/Qwiic_Button, `Hardware/Qwiic_Button.brd`

---

## Espressif ESP32-DevKitC (V4), carrier footprint

Origin: lower-left corner of the PCB outline in the official DXF (DXF coordinates minus (500.0, 400.0)), top view. The USB micro-B end is at y = 0 and the module/antenna end at the top.

| Item | Value | Status | Source |
|---|---|---|---|
| PCB outline | 27.94 x 48.26 (DXF linear dims 27.94 and 48.26). The layout PDF labels it 27.9 / 48.2 mm | Verified | [E1] DXF DIMENSION entities and outline; [E2] |
| Corner radii | R1.0 on all four corners (bulge 0.4142 arcs) | Verified | [E1] |
| Module overhang | The module extends 6.04 past the top PCB edge, 18.0 wide (x 5.0 → 23.0) | Verified | [E1] dims `*D59` = 6.04012, `*D57` = 17.99996 |
| Mounting holes | None on the board | Verified (absence) | [E1] |
| Header pitch / rows | 2 x 19 pins, pitch 2.54 (`*D56`). Row spacing 25.4 (`*D58`, 1.0") | Verified | [E1] |
| Column positions | x = 1.2446 and x = 26.6446. Edge-to-pin 1.2446 left, 1.2954 right (`*D60` = 1.2954), so the columns are **0.025 off-centre** | Verified | [E1] |
| Pin y positions | 1.2903 (`*D55` = 1.29032 from the bottom/USB edge) to 47.0103, pitch 2.54 | Verified | [E1] |
| J2 / J3 sides and pin 1 | J2 = left column with pin 1 (3V3) at the top/module end: (1.2446, 47.0103), pin 19 (5V) at (1.2446, 1.2903). J3 = right column with pin 1 (GND) at the top: (26.6446, 47.0103), pin 19 (CLK) at the bottom | Partial: the pin names come from the guide; the left/right side comes from label positions in the PCB layout PDF (J2x1 at x≈92, J3x1 at x≈321 pt, page 1) | [E3], [E2] |
| Header hole | The DXF draws Ø0.7755 circles at the pin positions (likely drill, not confirmed) | Partial | [E1] block `ROUND_370` |
| Keepouts for carrier | None published. The antenna overhang needs clearance on the carrier (no copper under it) per general module guidance, which was not checked for this kit | Unverified | none |
| Thickness | Not stated | Unverified | none |
| Licensing | The esp-dev-kits repo states that documentation (text, diagrams, images) is CC BY-SA 4.0. The DXF/PDF files on dl.espressif.com carry no licence notice | Partial | [E4] |

## Espressif ESP32-S3-DevKitC-1 (v1.1), carrier footprint

Origin: the DXF origin, which is the lower-left corner of the PCB outline, top view. The two USB-C connectors are at the y = 0 end and the module at the top.

| Item | Value | Status | Source |
|---|---|---|---|
| PCB outline | 25.4 x 62.865 (x 0 → 25.4, y 0 → 62.865) | Verified | [E5] layer `BOARD_OUTLINE` |
| Corner radii | R0.5 on all four corners | Verified | [E5] |
| Module overhang | `TOP` layer rectangle x 3.75 → 21.75 (18.0 wide), y 62.84 → 69.15, i.e. ≈6.29 past the top edge. Applies to the WROOM-1 build; WROOM-1U/-2 variants were not checked | Partial | [E5] |
| Mounting holes | None | Verified (absence) | [E5] |
| Header pitch / rows | 2 x 22, pitch 2.54. Columns x = 1.27 and 24.13, row spacing 22.86 (0.9"), symmetric | Verified | [E5] `PADLAYER_TOP` / `TOOL_BOT` |
| Pin y positions | 7.96 (bottom, USB end) to 61.30 (top), pitch 2.54 | Verified | [E5] |
| J1 / J3 and pin 1 | J1 = left column, pin 1 3V3 at (1.27, 61.30), pin 22 G at (1.27, 7.96). J3 = right column, pin 1 G at (24.13, 61.30), pin 22 G at (24.13, 7.96). Side assignment comes from the DXF silkscreen labels ("3V3", "3V3", "RST" … "5V", "G" on the left; "G", "TX" … on the right) matching the guide's pin tables | Verified | [E5] `LAYNR3` text; [E6] |
| Header hole | `TOOL_BOT` circles Ø1.016 at each pin (likely drill) | Partial | [E5] |
| Other drills | USB-C shell slots/holes near y 1.55–5.77 (part of the board, not the carrier) | Verified | [E5] `DRILLHOLE` |
| Thickness | Not stated | Unverified | none |
| Licensing | **The DXF title block reads "ESPRESSIF SYSTEMS … Proprietary", dated 2022.04.29 Fri., Rev 1.1.** The user-guide docs are CC BY-SA 4.0 (esp-dev-kits README). Do not redistribute the DXF; re-derive the dimensions only | Verified | [E5] `LAYER_125` text; [E4] |

Sources:
- [E1] Espressif, "ESP32-DevKitC V4 Dimensions source file (DXF)". https://dl.espressif.com/dl/schematics/esp32_devkitc_v4_dimensions.dxf (also the PDF `…_dimensions.pdf`). No revision in the file.
- [E2] Espressif, "ESP32-DevKitC V4 PCB Layout (PDF)", 6 pp. https://dl.espressif.com/dl/schematics/esp32_devkitc_v4_pcb_layout.pdf (label coordinates on pp. 1 and 5)
- [E3] ESP-IDF esp-dev-kits, "ESP32-DevKitC V4 User Guide" (latest), Header Block J2/J3 tables. https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32/esp32-devkitc/user_guide.html
- [E4] github.com/espressif/esp-dev-kits README, "Open Source Licensing" (code Apache-2.0, docs CC BY-SA 4.0)
- [E5] Espressif, `DXF_ESP32-S3-DevKitC-1_V1.1_20220429.dxf`. https://dl.espressif.com/dl/schematics/esp_idf/DXF_ESP32-S3-DevKitC-1_V1.1_20220429.dxf (title block: Rev 1.1, 2022.04.29, "Proprietary")
- [E6] "ESP32-S3-DevKitC-1 v1.1 User Guide", Header Block J1/J3. https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/user_guide_v1.1.html

---

## Open items / not verified

- **Board thickness:** only MicroMod (0.8) is vendor-stated. Arduino, Feather, Qwiic and Espressif state none. 1.6 is convention, not spec.
- **Arduino shield keepouts:** not published. This covers the USB-B/DC-jack overhang height and clearance and any keepout around the holes. Arduino's datasheet board-outline pages are images; the numbers above come from the CAD files instead.
- **Uno R4 outline:** inferred from the route-path Gerber, not from a Board-layer file. Holes and headers were checked against the Excellon drill file.
- **Feather:** thickness and any Adafruit trademark policy for "Feather" were not found on adafruit.com.
- **Qwiic:** there is no published general mechanical standard. The 1x1 and Mini sizes and the 0.13" holes come from product docs and board files. "Micro" size is unverified.
- **MicroMod:** the library body outline (37.49 wide) conflicts with the shipped board (38.0). Use 38.0.
- **ESP32-DevKitC:** which header is J2 and which is J3 was inferred from PDF label positions. Header drill sizes are inferred from drawing circles.
- **ESP32-S3-DevKitC-1:** module overhang was not checked for the WROOM-1U or WROOM-2 builds.
