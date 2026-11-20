# Industry form factors: public primary-source dimensions for REDA Board Templates

Researched 2026-11-20. Scope: PCIe CEM add-in card, M.2, mikroBUS add-on boards, Eurocard, PC/104 (+ PC/104-Plus).

## How to read this

- **Status**
  - **Verified**: read directly in a primary source: the standards body's own published document, or a drawing published by the vendor on its own site. I rendered the vector drawings and read the callouts myself.
  - **Partial**: the source is primary but not the governing spec (for example a connector vendor's drawing of the CEM card edge, a product datasheet, or an informative or draft document). Or the value was derived from a primary drawing by arithmetic or by scaling, and the derivation is stated.
  - **Unverified**: the governing spec is paywalled or member-only and I found no public primary source. **Do not ship these values** until someone with spec access checks them. Where the cell says "not found", I deliberately left the number blank instead of filling it from memory.
- Units are mm unless the source is in inches. In that case the source's inch value comes first and the source's own mm conversion follows in brackets.
- "Derived" means computed from Verified values. The arithmetic is given.
- Local copies of the downloaded sources are in `/data/mij/tmp/claude/ff-dl-industry/` (untrusted downloads, not executed). I read the drawings by rendering their vector content with stdlib-only helpers in `/data/mij/scratch/claude/ff-specs/` (`pdfenc_text.py`, `pdfrender.py`). The PC/104 PDFs are RC4-encrypted with an empty user password and needed decrypting first.

### General licensing note (applies to all sections)

These are observations, not legal advice. Get counsel to review before shipping.

- Spec **text and drawings** are copyrighted: PCI-SIG, SATA-IO, IEC/IEEE, PC/104 Consortium (now RMS Consortium) and MikroElektronika all claim copyright. Do not ship copied figures, tables or prose. Encode only the bare numbers as our own geometry, and cite the source in docs.
- Bare dimensional facts are generally treated as non-copyrightable. Trademarks are a separate question: use a name only to say what a template is compatible with, never as our product's name, and never with a logo.
- Unauthorized copies of the PCI-SIG CEM and M.2 specs turn up in web searches (forum CDNs, personal sites, dokumen.pub). **I did not use them.** They are not primary sources and redistributing their content is a licensing risk.

---

## PCIe CEM add-in card

The governing spec is the PCI Express Card Electromechanical (CEM) Specification from PCI-SIG. It is **member-only, so the outline, bracket and connector-to-bracket datums are Unverified**. pcisig.com could not be fetched from this environment, so PCI-SIG's trademark guideline text is also Unverified.

The best public primary source is **TE Connectivity customer drawing 2364427**, "PCI Express Card Edge Connector Gen5, Std", rev D2. Sheet 3 of 8 has a view titled "ADD-IN CARD EDGE-FINGER DIMENSION", default tolerance ±0.13. This is TE's depiction of the mating card for its Gen5 connector, so it is **Partial**, not the CEM itself. Older CEM revisions (Gen1–4) may differ, for example in finger length and pad setback.

**Origin and orientation in the TE view:** the key notch centreline is datum U. The short side carries pins 1–11 (A1/B1 at the far end from the key). The long side carries pins 12 to F+12. The card edge is at the bottom.

| Item | Value | Status | Source |
|---|---|---|---|
| Card thickness at fingers | 1.57 ± 0.13 "ACROSS PADS" | Partial | [TE-CD] sh.3, side view |
| Edge bevel (chamfer) angle | 20.0°, measured from the card face | Partial | [TE-CD] sh.3, side view |
| Edge bevel length along card | 1.30 ± 0.25 | Partial | [TE-CD] sh.3, side view |
| Card-edge corner chamfers | 0.5 × 45° | Partial | [TE-CD] sh.3, B-side view |
| Finger pitch | 1.00 | Partial | [TE-CD] sh.3 (1.00 between adjacent fingers; 10.00 for pins 1–11) |
| Finger width | 0.60 ± 0.05 (position tolerance ⌖0.10 W V U) | Partial | [TE-CD] sh.3 |
| Finger length | 3.00 | Partial | [TE-CD] sh.3 |
| Card edge to far end of finger | 5.60 | Partial | [TE-CD] sh.3 |
| Card edge to near end of finger (pad setback) | (2.60) reference | Partial | [TE-CD] sh.3. Gen5 pads stop short of the bevel; older generations may differ |
| Key notch width | 1.90 ± 0.06 | Partial | [TE-CD] sh.3 |
| Key notch depth from card edge | 8.40 MIN, full-radius end | Partial | [TE-CD] sh.3 |
| Key centreline to nearest finger centre (each side) | 1.50 | Partial | [TE-CD] sh.3 (pin 11 and pin 12) |
| Key centreline to A1/B1 finger centre | 11.50 | Partial (derived) | 1.50 + 10.00 from [TE-CD] sh.3 |
| Key centreline to short-side card end | 12.15 | Partial | [TE-CD] sh.3 |
| Key centreline to long-side card end (dim H) | x1 8.15 · x4 22.15 · x8 39.15 · x16 72.15 | Partial | [TE-CD] sh.3, table "DIM H" against POS 36/64/98/164 |
| Pin 12 to last-pin span (dim F) | x1 6 · x4 20 · x8 37 · x16 70 (mm, at 1.00 pitch) | Partial | [TE-CD] sh.3, table "DIM F" |
| Total finger-edge length | x1 20.30 · x4 34.30 · x8 51.30 · x16 84.30 | Partial (derived) | H + 12.15 |
| Contact positions (both sides) | x1 36 · x4 64 · x8 98 · x16 164 | Partial | [TE-CD] sh.3 table "POS." |
| Pin naming | A-side and B-side views labelled A1 / A"F+12" and B1 / B"F+12" | Partial | [TE-CD] sh.3. Which face is the component side is **not stated** here (Unverified) |
| Finger plating (gold thickness and type) | not found | Unverified | CEM (member-only) |
| Low-profile card height | 68.90 (bracket not included) | Partial | [AMD-DS1002]: product "compliant with the PCIe CEM rev. 3.0 … single slot, half height, half length" |
| Half-length card length | 167.65 | Partial | [AMD-DS1002] |
| Full-height card height | not found in a primary public source | Unverified | CEM (member-only) |
| Full-length card length | not found in a primary public source | Unverified | CEM (member-only) |
| Card outline corner radii, bracket-side notch, retention-clip cutouts | not found | Unverified | CEM (member-only) |
| Max component height (single slot) | not found in primary source | Unverified | CEM |
| Bracket (full-height and low-profile) geometry, tab, screw slot | not found | Unverified | CEM |
| Finger datum to bracket datum | not found | Unverified | CEM |
| Mounting holes | none on the card itself (bracket-mounted); retainer hole details not found | Unverified | CEM |

**Sources**

- [TE-CD] TE Connectivity, Customer Drawing 2364427, "PCI Express Card Edge Connector Gen5, Std", rev D2, sheet 3 of 8, "Add-in card edge-finger dimension" and the POS/DIM E–H table. Fetched via `https://www.te.com/commerce/DocumentDelivery/DDEController?Action=showdoc&DocId=Customer+Drawing%7F2364427%7FD1%7Fpdf%7FEnglish%7FENG_CD_2364427_D1.pdf%7F4-2364427-7`. Local copy: `te_2364427_cd.pdf`.
- [AMD-DS1002] AMD, *Alveo X3522 Data Sheet* DS1002 v1.4 (2024-10-18), "Dimensions", Table 1: https://docs.amd.com/r/en-US/ds1002-x3522/Dimensions

**Licensing.**
- CEM spec text and figures are PCI-SIG copyright and member-licensed.
- "PCI Express" and "PCIe" are PCI-SIG marks. I could not read PCI-SIG's usage guidelines (fetch blocked), so treat them as Unverified: use the marks only descriptively ("PCI Express x16 add-in card outline"), with no logo, and get PCI-SIG's trademark page checked before release.
- The TE drawing is © TE Connectivity. Use its numbers, not its drawing.
- **Shipping recommendation:** gate the card outline and bracket until someone with CEM access fills in the Unverified rows.

---

## M.2

The governing spec is the PCI Express M.2 Specification from PCI-SIG. It is **member-only**.

The public primary source used here is **SATA-IO TP053v11, "M.2 Card Format for SSDs"**, version 11, 1 May 2013 ("Ratified"), posted on sata-io.org. Its mechanical sections are explicitly **informative** and defer to the PCI-SIG M.2 spec, so everything here is **Partial** at best. It covers Keys B and M only, with SSD lengths 30/42/60/80/110 and 22 mm width.

**Origin in the source:**
- Datum B is the finger edge; LENGTH is measured from it.
- Datum E is the board centreline.
- Datum A is the Key B notch centre; datum D is the Key M notch centre.
- Top view, edge down: pin 1 is at the right end of the top side, pin 75 at the left.

| Item | Value | Status | Source |
|---|---|---|---|
| Board width | 22.00 ± 0.15 | Partial | [SATA-M2] Fig. 7 and Fig. 17 |
| Lengths (datum B to far end) | 2230: 30.0 · 2242: 42.0 · 2260: 60.0 · 2280: 80.0 · 22110: 110.0, all ± 0.15 | Partial | [SATA-M2] Fig. 17 table |
| Board thickness | 0.80 ± 0.08 | Partial | [SATA-M2] Figs. 13–16 (Z-height stack-ups) |
| Finger tab width | 19.85 ± 0.15, centred on datum E | Partial | [SATA-M2] Figs. 7, 9, 11 |
| Finger tab depth (edge to shoulder) | 4 ± 0.15 | Partial | [SATA-M2] Fig. 7 |
| Shoulder corner radii | 2 × R0.50 ± 0.15 | Partial | [SATA-M2] Fig. 8 |
| Contact count and pitch | 75 locations; 0.50 typical pitch per side; odd pins top, even pins bottom | Partial | [SATA-M2] §3, Figs. 7–8 |
| Pin 1 to pin 75 centre span | 18.50 | Partial | [SATA-M2] Fig. 11 (dual-key B+M) |
| Top finger length | 38 × 2 ± 0.15 | Partial | [SATA-M2] Fig. 7 |
| Bottom finger length | 37 × 2.50 ± 0.15 | Partial | [SATA-M2] Fig. 8 |
| Finger width | 0.35 ± 0.04 | Partial | [SATA-M2] Figs. 9–11 |
| Key notch width | 1.20 ± 0.05, full-R end | Partial | [SATA-M2] Figs. 9–11 |
| Key notch depth from edge | 3.50 ± 0.15 | Partial | [SATA-M2] Fig. 9 detail A |
| Key B notch location | pins 12–19 removed; notch centre 5.625 from datum E toward pin 1 | Partial | [SATA-M2] §4 text and Fig. 11 |
| Key M notch location | pins 59–66 removed; notch centre 6.125 from datum E toward pin 75 | Partial | [SATA-M2] §4 text and Fig. 11 |
| Key E notch location | not found | Unverified | PCI-SIG M.2 (member-only); not in [SATA-M2] |
| Key A and other keys | not found (generic letter dims Q/R/S/T/U only, values in the M.2 spec's Table 4) | Unverified | PCI-SIG M.2 |
| Mounting half-moon notch | Ø3.50 ± 0.08 semicircle at the far end, centred on datum E | Partial | [SATA-M2] Figs. 9 and 11 |
| Mounting ground pad, top | Ø5.50 ± 0.10 plated GND pad, "no components allowed" | Partial | [SATA-M2] Figs. 7 and 9 |
| Mounting ground pad, bottom | Ø6 ± 0.10 plated GND pad | Partial | [SATA-M2] Fig. 10 (also has a "1 ± 0.10" callout near the far edge whose datum I could not resolve from the drawing; Unverified) |
| Far-end reserved zone (top) | 3.80 ± 0.15 from far end ("area reserved for RF receptacles or components when no RF receptacles used") | Partial | [SATA-M2] Fig. 7 |
| Bottom component-free zone from finger edge | 5.20 MIN | Partial | [SATA-M2] Fig. 8 |
| Component height limits | per profile S2/D2/D3/D5, Fig. 18. The table did not extract cleanly; read it from the figure before use | Unverified | [SATA-M2] Fig. 18 |
| Finger-edge chamfer or bevel | not found | Unverified | PCI-SIG M.2 |
| Finger plating | not found | Unverified | PCI-SIG M.2 |
| Ground-plane pull-back from fingers | 0.10 / 0.20 / 0.25 / 1.55 callouts (informative) | Partial | [SATA-M2] Fig. 20 |

**Sources**

- [SATA-M2] Serial ATA International Organization, *TPR053v11 SATA 3.1 M.2 Card Format for SSDs*, version 11 (1 May 2013, "Ratified"). Figures 7–11, 13–18 and 20, plus §4 key text: https://sata-io.org/sites/default/files/TP_053v11_SATA31_Mdot2_Card_Format_for_SSDs.pdf. Local copy: `sataio_tp053v11_m2.pdf`.

**Licensing.**
- SATA-IO's notice on this PDF says it is an internal working document, made available for review, and that "Any commercial or for-profit replication or republication is prohibited". So do not ship its figures or text; use numbers only.
- PCI-SIG owns the M.2 spec. Whether "M.2" is a registered mark is **Unverified**, because pcisig.com was not reachable. Use the name only descriptively.
- **Shipping recommendation:** 2230/2242/2280 outlines, half-moon and B/M keys are consistent across Figs. 7–11 and are reasonable to ship as Partial. Key E needs a member-spec check.

---

## mikroBUS add-on boards

The primary source is MikroElektronika's own *mikroBUS Standard Specifications*, rev 2.00 (May 2015), downloaded from download.mikroe.com. This is the governing document and it is free. Board-size figures are given in mils, with mm in the legend.

| Item | Value | Status | Source |
|---|---|---|---|
| Board width (all sizes) | 1000 mil (25.4) | Verified | [MB] p.12 "Add-on board size and shape" |
| Length, size S | 1125 mil (28.6; exact 28.575) | Verified | [MB] p.12 |
| Length, size M | 1690 mil (42.9; exact 42.926) | Verified | [MB] p.12 |
| Length, size L | 2250 mil (57.15) | Verified | [MB] p.12 |
| Shape freedom | free shape as long as it stays within these dimensions, but the bottom-right **notch must be kept** (insertion guide) | Verified (rule) | [MB] p.12 |
| Notch dimensions | not given numerically in text; drawn only | Unverified | [MB] p.12 drawing |
| Corner radii | not specified | Unverified | [MB] |
| Board thickness | h = 1.6 mm | Verified | [MB] p.12 |
| Header holes | ø 35 mil (0.89), 2 × 1×8 headers | Verified (value); Partial (that it means the header pin holes, read from the drawing) | [MB] p.12 |
| Pin pitch | 100 mil (2.54) | Verified | [MB] p.6 and p.12 |
| Header row-to-row spacing | 900 mil (22.86) | Partial (derived) | Scaled from [MB] p.12 drawing: pad columns 64.24 pt apart against 7.12 pt per 100 mil gives ≈ 902 mil. The text and label layer has no explicit "900" |
| Header position from board edges | "100" mil labels near the headers; exact datum unresolved | Unverified | [MB] p.12 |
| Mounting holes | none specified | Verified (absence) | [MB] p.12 |
| Pinout groups | left 1×8 has +3.3 V and GND; right 1×8 has +5 V and GND | Verified | [MB] p.6 text |
| Per-pin order | legend lists AN, RST, CS, SCK, MISO, MOSI, +3.3V, GND / PWM, INT, RX, TX, SCL, SDA, +5V, GND. Positional order is in a vector drawing I did not confirm | Partial | [MB] p.6 |
| Add-on pin naming rules | default names; unused pins "NC"; UART TX/RX criss-crossed on boards; front markings vertical | Verified | [MB] p.11 |
| Silkscreen requirement | must include the mikroBUS logo and default pin names (backside markings required even when the front is shortened) | Verified | [MB] p.7–8 |
| Socket placement on mainboard | 11.87 mm (467.5 mil), 11.43 (450 mil), 3.81 (150 mil) callouts on "Placement requirements"; datums not resolved | Unverified | [MB] p.9 |

**Sources**

- [MB] MikroElektronika, *mikroBUS Standard Specifications*, Revision 2.00, May 2015 (rev history p.2; v1.00 Aug 2011): https://download.mikroe.com/documents/standards/mikrobus/mikrobus-standard-specification-v200.pdf. Local copy: `mikrobus.pdf`.

**Licensing.**
- Per [MB] p.4, "mikroBUS™ is an open standard — anyone can implement mikroBUS™ in their hardware design, as long as the requirements set by this document are being met."
- "mikroBUS" carries ™. [MB] p.7 requires the mikroBUS logo on the silkscreen of compliant boards.
- Per [MB] p.12, "click boards™" is MikroElektronika's trademark: third parties "are not allowed to call their own mikroBUS™ add-on boards click boards™, nor use the word 'click' on the silkscreen." **REDA must not name these templates "Click".** Say "mikroBUS add-on board (S/M/L)".
- A 2012 v1 terms sheet reportedly asked commercial implementers to notify office@mikroe.com. I found it only on a third-party host, so it is Unverified and not cited.

---

## Eurocard 100 × 160 (IEC 60297-3-101 / IEEE 1101.1)

The governing standards are **paywalled**:
- **IEC 60297-3-101:2004**, Ed. 1.0. The official preview shows the board dimensions sit in Figure 7 "Printed board type plug-in unit, 3U" (p.13), Figure 8 (6U), and Tables 3 (heights) and 4 (depths), none of which are in the preview.
- **IEEE 1101.1-1998**, now Inactive-Reserved.

No public primary source gave thickness, guide-rail edge strips or connector datums.

| Item | Value | Status | Source |
|---|---|---|---|
| 3U board size | 100 × 160 ("100 x 160 mm Euroboard") | Partial | [NV-61000] (nVent product page wording). Normative value in IEC 60297-3-101 Fig. 7 / Table 3 (paywalled) |
| 6U board size | 233.35 × 160 | Partial | [NV-61000] product title "PCB Solder Side Cover, 6 U, 233.35 mm x 160 mm" |
| Board depth series | 100, 160, 220, 280, 340, 400 | Verified (as Schroff guide-rail board lengths) | [NV-RAIL] "Dimensions table", column "Board length L" |
| Guide-rail groove width | 2 mm and 2.5 mm variants | Verified (vendor) | [NV-64568] product titles 64568-001 (2 mm) and 64568-096 (2.5 mm) |
| Board thickness | not found in a primary public source | Unverified | IEC 60297-3-101 / IEEE 1101.1 |
| Guide-rail keep-out strips along the long edges | not found | Unverified | IEC 60297-3-101 Fig. 7 (paywalled) |
| Front-panel to board datum, ejector holes | not found | Unverified | IEC 60297-3-101 |
| Rear connector position (IEC 60603-2 / DIN 41612) | standard references IEC 60603-2 for 2.54-grid connectors; position values not found | Unverified (values) | [IEC-PREV] §2 normative references |
| Corner radii, mounting holes | not found | Unverified | IEC 60297-3-101 |
| 1U | 44.45 mm | Verified | [IEC-PREV] §4 "Abbreviation: 1U = 44,45 mm (see Table 3)" |

**Sources**

- [IEC-PREV] IEC 60297-3-101:2004 Ed. 1.0, official publisher preview (TOC, foreword, §1–4): https://www.vde-verlag.de/iec-normen/preview-pdf/info_iec60297-3-101%7Bed1.0%7Db.pdf. Local copy: `iec60297-3-101_vde_preview.pdf`. IEC catalogue entry: https://webstore.iec.ch/publication/1284
- [IEEE] IEEE 1101.1-1998 (Inactive-Reserved), standards listing: https://standards.ieee.org/ieee/1101.1/1645/ (abstract only)
- [NV-RAIL] nVent SCHROFF Product Catalog 03/2021, p.6.42, "Dimension drawings of guide rails", Dimensions table: https://nvent.widen.net/content/uvkh4bx6qv/original/6-1_dimensions_of-guide-rails_en.pdf. Local copy: `schroff_guide_rail_dims_en.pdf`.
- [NV-61000] nVent SCHROFF product page, 61000-012: https://www.nvent.com/en-us/schroff/products/enc61000-012
- [NV-64568] nVent SCHROFF product page, 64568-001 "Guide Rail Standard Type, PC, 160 mm, 2 mm Groove Width": https://www.nvent.com/en-us/schroff/products/enc64568-001 (2.5 mm variant 64568-096).

**Licensing.**
- "Eurocard" is used generically here. I found no trademark claim, but did not search trademark registers.
- IEC and IEEE texts are copyrighted and sold. Ship only the bare sizes.
- **Shipping recommendation:** a plain 100 × 160 rectangle is low-risk. Leave rail strips, thickness and connector placement out until someone with the IEC text checks them.

---

## PC/104 (8-bit and 16-bit)

The primary source is the **PC/104 Specification Version 2.6**, 13 Oct 2008, published free by the PC/104 Embedded Consortium (renamed RMS Consortium on 16 Oct 2025 per pc104.org). Appendix A Figures 2 (8-bit) and 3 (16-bit) are AutoCAD vector drawings. I rendered and read them.

**Origin used by the spec:** the corner nearest J1 pin 1, viewed from the top (component side). X increases to the **left** and Y increases **up**. J1 runs along the right edge, which is the bus edge. Values are inches with the spec's mm in brackets.

| Item | Value | Status | Source |
|---|---|---|---|
| Outline | 3.775 (95.89) in X × 3.550 (90.17) in Y, rectangle | Verified | [PC104] Figs. 2–3 |
| Nominal size (text) | "3.550 by 3.775 inches (90 by 96 mm)" | Verified | [PC104] §1 p.1 |
| Corner radii | none dimensioned (square corners in the drawing) | Verified (absence) | [PC104] Figs. 2–3 |
| Board thickness | .062 (1.57) | Verified | [PC104] Figs. 2–3 side view |
| Mounting holes | 4 × hole .125 (3.18) dia on 8-bit; written ".12 (3.18)" on 16-bit. Pad .250 (6.35) dia | Verified | [PC104] Figs. 2–3 |
| Hole 1 (near J1 pin 1) | X .200 (5.08), Y .200 (5.08) | Verified | [PC104] Fig. 3 |
| Hole 2 | X .200 (5.08), Y 3.350 (85.09) | Verified | [PC104] Fig. 3 |
| Hole 3 | X 3.575 (90.81), Y 3.250 (82.55) | Verified | [PC104] Fig. 3 |
| Hole 4 | X 3.575 (90.81), Y .350 (8.89) | Verified | [PC104] Fig. 3 |
| Standoff height | 0.600" ± 0.005" (15.24); standoff OD < .250 (6.35), thread < .125 (3.175); "Pads must be provided for the standoffs, with the same plating as the pads for the connectors" | Verified | [PC104] App. D, Fig. 7 |
| J1/P1 (rows A, B), 64 pins | row A at X .400 (10.16), row B at X .500 (12.70); pin 1 at Y .250 (6.35); pitch .100 (2.54); pin 32 at Y 3.350 | Verified (pin 32 derived as .250 + 31 × .100) | [PC104] Fig. 3, Fig. 4 |
| J1 pin-1 marking | square pads at A1/B1, labelled "1" | Verified | [PC104] Fig. 3 |
| J2/P2 (rows C, D), 40 pins numbered 0–19 | first row (pin 0) aligned with J1 pin 9, so Y 1.050; square pads and "1" label at J2 pin 1 (Y 1.150); last row Y 2.950 | Partial (derived) | [PC104] Fig. 3 (row count) and Fig. 4 (J2 first pin .800 from J1 first pin: .86 − .06) |
| J2 row X positions | C ≈ .300, D ≈ .200 | Partial (scaled; not dimensioned) | [PC104] Fig. 3 |
| Connector body | J1 3.220 ± .010 (81.79) long × .195 (4.95); J1+J2 .395 ± .010 (10.03) deep; pins .025 (0.64) sq; key at B10 and C19 (omitted pins / plugged holes) | Verified | [PC104] Fig. 4; App. B Table 2 notes |
| Component height | top .435 (11.05) max; bottom .100 (2.54) max | Verified | [PC104] Figs. 2–3 side view |
| I/O overhang zones (allowances, not keepouts) | top and bottom edges: X .325–3.450, extending .500 (12.70) beyond the edge (to Y 4.050 / −.500). Bus edge: X 0 to −.500, Y .325–.95 (24.13) on 16-bit, Y .325–3.225 (81.92) on 8-bit | Verified | [PC104] Figs. 2–3 |
| Stack spacing | .600 (15.24) spacers | Verified | [PC104] Fig. 1 |

### PC/104-Plus (same outline and holes, plus J3 PCI connector)

| Item | Value | Status | Source |
|---|---|---|---|
| Outline, holes, J1/J2 | identical to PC/104 | Verified | [PCP] §6.2.2 and Fig. 6 |
| J3 (PCI), 4 × 30 = 120 pins, 2 mm pitch | rows A B C D; pin-1 row at Y .650 (16.51), at the bottom-left near hole 4 | Verified (Y) | [PCP] §6.2.3 and Fig. 6 |
| J3 column X positions | A ≈ 3.33, D ≈ 3.57 (scaled; outline edge 3.234 (82.14) dimensioned) | Partial (scaled) | [PCP] Fig. 6 |
| Shroud holes | 4 × .075 (2.0) dia; Y .527 (13.39) and 3.057 (77.65); X 3.679 (93.45) on the outer side | Verified (Y, outer X); Partial (inner X) | [PCP] Fig. 6 |
| Component height | unshaded area top .345 (8.76), bottom .190 (4.83); dotted 3-side band top .435 (11.05), bottom .100 (2.54) | Verified | [PCP] §6.2.2 and Fig. 6 |
| Board ID labelling | must label near or on J3: "3.3V only", "5V only" or "Universal" | Verified | [PCP] §6.2.4 |
| PCI-104 | named as a consortium trademark in [PCP]; not researched further | Unverified | [PCP] cover |

**Sources**

- [PC104] PC/104 Embedded Consortium, *PC/104 Specification Version 2.6*, 13 Oct 2008. §1 p.1; App. A Figs. 2–5 (pp. A-2 to A-5); App. B Table 2; App. D Fig. 7: https://pc104.org/wp-content/uploads/2015/02/PC104_Spec_v2_6.pdf (linked from https://pc104.org/hardware-specifications/pc104/). Local copy: `pc104_v2_6.pdf`.
- [PCP] PC/104 Embedded Consortium, *PC/104-Plus Specification Version 2.3*, 13 Oct 2008 (file name says v2_32). §6.2 and App. A Fig. 6 (p. A-2): https://pc104.org/wp-content/uploads/2015/02/PC104_Plus_v2_32.pdf. Local copy: `pc104plus_v2_32.pdf`.

**Licensing.**
- Cover pages: "PC/104 is a trademark of the PC/104 Embedded Consortium" ([PC104]) and "PC/104, PC/104-Plus, and PCI-104 are trademarks of the PC/104 Embedded Consortium" ([PCP]). Both: "Copyright 1992-2008, PC/104 Embedded Consortium".
- pc104.org says the specs are "available at no charge to individuals and companies developing embedded systems".
- Use the names descriptively only.
- The spec defines "Compliant" and "Bus-compatible" conformance levels (§5). Do not claim either in the template name.
- Note that the pc104.org PC/104-Plus page says "3.575 x 3.775 in", but the spec drawings say 3.550 × 3.775. **Use the spec.**
