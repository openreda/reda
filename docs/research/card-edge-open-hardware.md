# Card-edge form factors from open hardware: PCIe CEM add-in cards and M.2

Researched 2026-11-20. This follows up `docs/research/form-factor-specs/industry.md` on branch `research/form-factor-specs`. That file found that the governing PCI-SIG specs (CEM and M.2) are member-only. Here the missing numbers are cross-checked against **open hardware design files and open libraries**. I did not use pirated copies of the PCI-SIG specs.

## Method

- I downloaded KiCad design files (`.kicad_pcb`, `.kicad_mod`) into an untrusted directory, `/data/mij/tmp/claude/cardedge-dl/`. The URLs and commits are listed under [Sources](#sources).
- The files were parsed as data only, by stdlib-only scripts run with `python3 -I`. The scripts are in `/data/mij/scratch/claude/cardedge/`:
  - `kmod.py` reads the S-expressions and transforms footprint geometry into board coordinates.
  - `norm.py` finds the PCIe key notch and re-expresses the `Edge.Cuts` outline relative to it.
  - `zones.py` dumps keepout zones and user-layer drawings.
- One vendor PDF (a Keystone bracket drawing) was rendered with the stdlib PDF helpers from the earlier research.
- Each value is in mm and was read from the geometry itself, not from a README.

### Status meanings

| Status | Meaning |
|---|---|
| **Verified-by-agreement** | At least two sources from different authors give the same value. Where the earlier research's TE or SATA-IO numbers also agree, that is noted. |
| **Single-source** | Only one open source gives it, or the other sources derive from it. |
| **Conflicting** | The sources disagree. The values and the likely reason are given. |
| **Not found** | No open source gives it. |

### Independence caveat (read before relying on "agreement")

Open projects copy each other. Where I could see a lineage, I counted the copies as one source:

- **eggsampler/PCIeDMA** says in its README that its PCIe outline "is based on lucanastasio/PCIexpress-KiCad". It is therefore not counted as independent of lucanastasio.
- **KCORES/OCP2PCIe** and **KCORES/KCORES-FlexibleLOM-Adapter** have the same author, so they count as one source.
- **KiCad's official M.2 footprints** and **adryzz/M.2-Key-E-Footprints** both cite, in their own description or README, a `read.pudn.com` copy of the "PCIe M.2 Electromechanical Spec Rev1.0". That is an unauthorized copy of the PCI-SIG spec. Their numbers are bare facts, but their provenance is that copy. Treat them as one lineage.
- **lucanastasio/PCIexpress-KiCad** says it was "made using guidelines from PCI Express Card Electromechanical Specification Rev. 1.2". It does not say how the author got that spec.
- **KiCad's official `BUS_PCIexpress_*` footprints** cite a Ritrontek (connector vendor) catalogue.
- The other projects (ZoidTechnology/PCIce, Supercookiegaming, yutotnh, Antmicro, gralco, septentrio G5Dot) give no source. Some may have started from lucanastasio or the KiCad library. When several of them agree to the 0.001 mm, a shared ancestor is possible but cannot be proved.

### Coordinate conventions

- **PCIe.** `u` runs along the finger edge from the key-notch centreline, positive toward the **bracket side** (the side with pins A1/B1). `v` is the height above the finger (insertion) edge.
- **M.2.** `x` is measured from the board centreline, positive toward **pin 1**. `y` is the distance from the finger edge.

---

## PCIe CEM add-in card

### Card edge (fingers, key, finger tab)

| Item | Value | Sources | Agree? | Status |
|---|---|---|---|---|
| Key notch width | 1.90 | KiCad `BUS_PCIexpress_x1/4/8/16`; lucanastasio; KCORES; PCIce; Supercookie; yutotnh; TE 2364427 | yes (7, incl. TE) | **Verified-by-agreement** |
| Key notch depth (tip from finger edge) | 8.40, full radius R0.95 | KiCad; lucanastasio; KCORES; Supercookie; yutotnh; TE "8.40 MIN" | yes. PCIce has 8.45 (its notch has R1 fillets) | **Verified-by-agreement** |
| Key centreline to short (bracket-side) end of finger tab | 12.15 | KiCad; lucanastasio; KCORES; PCIce; Supercookie; yutotnh; TE | yes | **Verified-by-agreement** |
| Key centreline to long end of finger tab | x1 8.15 · x4 22.15 · x8 39.15 · x16 72.15 | KiCad (all four widths); lucanastasio (x1, x16); KCORES OCP (x4); KCORES FlexibleLOM (x8); yutotnh (x16); TE "DIM H" | yes | **Verified-by-agreement** |
| Finger-tab corner chamfers | 0.5 × 45° | KiCad; lucanastasio; KCORES; yutotnh; TE | yes. PCIce uses R1 fillets instead | **Verified-by-agreement** |
| Finger pitch | 1.00 | KiCad; lucanastasio; TE | yes | **Verified-by-agreement** |
| Key centreline to A1/B1 centre | 11.50 | KiCad; lucanastasio (pads at `u` = 11.5); TE (derived) | yes | **Verified-by-agreement** |
| Finger pad width | 0.70 | KiCad; lucanastasio | yes, but TE Gen5 gives **0.60 ± 0.05** | **Conflicting** (open libraries 0.70 vs TE Gen5 0.60) |
| Finger far end (from finger edge) | 5.60 | KiCad; lucanastasio; TE | yes | **Verified-by-agreement** |
| Finger near end, standard pins | KiCad 1.30 · lucanastasio 1.40 | KiCad; lucanastasio | no. TE Gen5 gives 2.60 (ref) | **Conflicting**. Open libraries follow the older generations' long pads; TE's Gen5 drawing pulls the pads back |
| Finger near end, short pins (A1, B17 on x1 / PRSNT) | 2.40 | KiCad; lucanastasio | yes | **Verified-by-agreement** (open sources only) |
| Which face carries B-side fingers | B-side on F.Cu (component side), A-side on B.Cu | KiCad; lucanastasio | yes | **Verified-by-agreement** |
| Edge bevel | 20°, 1.30 long (TE only). Open boards only set KiCad's `(edge_connector bevelled)` flag (Supercookie, eggsampler) with no angle | TE | n/a | **Single-source** (TE) |
| Finger plating | "Hard gold" finish set in Supercookie; ENIG in eggsampler. No thickness anywhere | — | no | **Not found** (no normative value) |
| Card thickness | 1.57 (TE) | TE only; open boards' stack-ups vary | n/a | **Single-source** |

### Card body and bracket-side outline

All values are in (`u`, `v`). The bracket side is +`u`. Several sources agree on the full bracket-side profile, from the finger tab out to the bracket edge:

| Item | Value | Sources | Status |
|---|---|---|---|
| Bracket-side card edge | `u` = 57.15 from the key centreline (= 45.00 from the end of the finger tab) | lucanastasio (x1/x16, full/half/LP); KCORES OCP2PCIe; KCORES FlexibleLOM; PCIce; Supercookie; eggsampler (derived) | **Verified-by-agreement**. This is the **finger-to-bracket datum** the earlier research could not find |
| Card main bottom edge (beside the fingers) | `v` = 12.75 | lucanastasio; KCORES; PCIce; Supercookie; eggsampler | **Verified-by-agreement** |
| Bracket-side retention notch, next to the finger tab | `u` 12.15 → 15.80 (3.65 wide), full-radius R1.825 end, tip at `v` 12.75 | lucanastasio; KCORES; PCIce; Supercookie | **Verified-by-agreement** |
| Tab between the notch and the cutout | `u` 15.80 → 23.80, bottom at `v` 4.50 | lucanastasio; KCORES; PCIce; Supercookie | **Verified-by-agreement** |
| Cutout | `u` 23.80 → 42.15, up to `v` 12.75 | lucanastasio; KCORES; PCIce; Supercookie | **Verified-by-agreement** |
| Bracket-end extension | `u` 42.15 → 57.15, bottom at `v` 4.50 | lucanastasio; KCORES; PCIce; Supercookie | **Verified-by-agreement**. Corner treatment varies: square (lucanastasio, Supercookie), R1 (KCORES, PCIce) |
| Lower bracket-screw hole | Ø3.20 at `u` 49.65 (7.50 in from the bracket-side edge), `v` 9.35 | lucanastasio; Supercookie; eggsampler (derived) | **Verified-by-agreement** (2 independent) |
| Upper bracket-screw hole, low profile | Ø3.20 at `u` 49.65, `v` 63.25 | lucanastasio; Supercookie | **Verified-by-agreement** |
| Upper bracket-screw hole, full height | Ø3.20 at `u` 49.65, `v` 94.75 | lucanastasio only | **Single-source** |
| Low-profile card height (finger edge to top) | 68.90 | lucanastasio; Supercookie; eggsampler; AMD DS1002 (earlier research); NVIDIA ConnectX-7 manual "68.90mm" | **Verified-by-agreement** |
| Full-height card height | 111.15 | lucanastasio (`v` max 111.15); NVIDIA ConnectX-7 user manual, MCX713114TC-GEAT: "PCIe Full Height, Half Length 4.37 in. x 6.6 in. (111.15mm x 167.65 mm)" | **Verified-by-agreement** (one open source plus one vendor primary). AMD DS962 gives "111.28 mm maximum" for the U200/U250, consistent with tolerance |
| Half-length card length (bracket-side edge to far end) | 167.65 | lucanastasio (232.125 − 64.475); NVIDIA ConnectX-7 manual; AMD DS1002 | **Verified-by-agreement** |
| Full-length card length | 312.00 | lucanastasio (376.475 − 64.475). Mouser listings of BittWare cards say "312 mm x 111.15 mm" (retailer text, not a datasheet) | **Single-source** in open hardware (weak retail corroboration) |
| Full-length far-end holes | Ø3.20, 5.08 in from the far end, at `v` 16.60 and 106.13 | lucanastasio only | **Single-source** |
| Corner radii of the card's top corners | none (square) in lucanastasio and Supercookie; R1 in KCORES and PCIce | — | **Conflicting**. These look like designer choice, not spec |
| x16 retention ("hockey-stick") feature, past the long end of the fingers | notch `u` −72.15 → −74.90, R1.375 end, tip at `v` 9.325. Then a 0.5 chamfer down to `v` 2.95 from `u` −75.40 to −86.20, a slope up to `v` 5.50 at `u` −87.60, up to `v` 7.95, back to `u` −83.90, and an R1.75 S-curve to the body edge `v` 12.75 at `u` −83.90 (`u` negative = long side) | lucanastasio only. KiCad's x16 footprint and yutotnh do not have it | **Single-source** |
| Keepout bands (copper/track keepouts drawn as zones) | `v` 0 → 13.75 strip along the bottom. Bracket-edge strip 5.08 wide (`u` 52.07 → 57.15, `v` 13.75 → 89.70, B.Cu). Top-corner block `u` 44.45 → 57.15, `v` 89.70 → 111.15. 1.0 strip along the top edge (`v` 110.15 → 111.15). Full-length far-end band 10.2 deep | lucanastasio only | **Single-source** |
| Max component height (primary/secondary side) | not given in any open file (no courtyard heights, no notes) | — | **Not found** |
| Bracket-side relief | KCORES OCP2PCIe insets the bracket-side edge by 1.80 between `v` ≈ 11.8 and 61.6 | KCORES only; no one else has it | **Single-source** (probably product-specific) |

### Bracket

| Item | Value | Sources | Status |
|---|---|---|---|
| Bracket overall length, full height | 120.0 (top of fold to bottom tip) | lucanastasio `PCIexpress_bracket_full` (−104.86 → +15.14 = 120.00); Keystone blank-bracket drawing "4.725 [120.0]" | **Verified-by-agreement** (open library plus vendor drawing) |
| Bracket overall length, low profile | 79.2 | lucanastasio `PCIexpress_bracket_low` (−68.09 → +11.11 = 79.20); Keystone "3.118 [79.2]" | **Verified-by-agreement** |
| Bracket bottom tip below the finger edge | full height 15.14, low profile 11.11 | lucanastasio only | **Single-source** |
| Bracket inner face relative to the card's bracket-side edge | 1.03 outboard (inner face at `u` 58.18). Sheet thickness 0.86 | lucanastasio only | **Single-source** |
| Top fold (screw-tab) width | 11.43 (full height), 11.84 (low profile) | lucanastasio. Keystone shows .450 [11.5] (full height) and .466 [11.9] (low profile) | **Verified-by-agreement within ≈0.1** |
| Screw-slot position on the top fold, screw size | not cross-checked. Keystone's dimensions are for its own blank bracket; its datums do not map cleanly onto the card | — | **Not found** (cross-checked) |

### PCIe verdict

**What can ship with numbers from at least two independent sources:**
- the card edge for x1, x4, x8 and x16: key notch, finger tab, chamfers, finger pitch and positions;
- the bracket-side outline: card edge 57.15 from the key, the notch, tab and cutout, and the 12.75 body edge;
- the lower bracket hole;
- low-profile height 68.90 with its upper hole;
- full-height height 111.15;
- half-length length 167.65;
- the bracket's outer length, as a Keepout or drawing outline only.

**What stays single-source:**
- full-length 312;
- the full-height upper hole at `v` 94.75;
- the bracket's position in Z/U relative to the card;
- the x16 retention feature;
- keepout bands.

**What is open or conflicting:**
- finger pad width (0.70 vs TE 0.60) and pad near-end (1.30/1.40 vs TE 2.60). Choose per PCIe generation;
- bevel angle and length (TE only);
- plating;
- max component height (not found).

---

## M.2

### Outline, finger tab, keys

| Item | Value | Sources | Agree? | Status |
|---|---|---|---|---|
| Width | 22.00 | KiCad (all); Antmicro oculink; gralco; G5Dot; adryzz; SATA-IO | yes | **Verified-by-agreement** |
| Lengths (finger edge to far end) | 2230 30.0 · 2242 42.0 · 2260 60.0 · 2280 80.0 · 22110 110.0 | KiCad (all five); SATA-IO; 2230 also gralco, G5Dot, adryzz; 2280 also Antmicro oculink | yes | **Verified-by-agreement**. 2242/2260/22110 rely on KiCad plus SATA-IO only |
| Finger-tab width | 19.85 | KiCad; Antmicro oculink; gralco; G5Dot; SATA-IO | yes. adryzz 19.80, Antmicro iot 19.95 | **Verified-by-agreement** |
| Finger-tab depth (edge to shoulder) | 4.00 | KiCad; Antmicro oculink; gralco; G5Dot; SATA-IO | yes | **Verified-by-agreement** |
| Shoulder corner radius | R0.50 | KiCad; gralco; G5Dot; adryzz; Antmicro oculink (≈0.48); SATA-IO | yes | **Verified-by-agreement** |
| Key notch width | 1.20, full-radius end | KiCad; Antmicro oculink; gralco; G5Dot; adryzz; SATA-IO | yes. Antmicro iot 1.25 | **Verified-by-agreement** |
| Key notch depth (tip from finger edge) | **3.50** | Antmicro oculink; Antmicro iot (3.46); gralco; G5Dot; adryzz; SATA-IO "3.50 ± 0.15" | no: **KiCad official = 4.10** (notch arc tip 0.1 past the shoulder) | **Conflicting**. The majority (5 projects plus SATA-IO) say 3.50. KiCad's library footprints cut 0.6 deeper. Use 3.50 |
| Key A centre | +6.625 (toward pin 1; pins 8–15 removed) | KiCad `M.2_2230-xx-A`; G5Dot (A+E) | yes | **Verified-by-agreement** (both A+E-style; G5Dot is a 2230 A+E card) |
| Key B centre | +5.625 (pins 12–19) | KiCad; SATA-IO | yes | **Verified-by-agreement** |
| Key E centre | +2.625 (pins 24–31) | KiCad `M.2_2230-xx-E`; gralco (E only); G5Dot (A+E); adryzz (+2.65 from the board centre, but 6.625 from pin 1, so the whole footprint sits 0.025 off-centre) | yes | **Verified-by-agreement** |
| Key M centre | −6.125 (pins 59–66) | KiCad; Antmicro oculink (6.127); SATA-IO; Antmicro iot (spacing B–M 11.75) | yes | **Verified-by-agreement** |
| Pin 1 position | +9.25. Pitch 0.25 alternating top/bottom (0.50 per side); pin 75 at −9.25 | KiCad; gralco; G5Dot; Antmicro oculink (9.243); SATA-IO | yes | **Verified-by-agreement** |
| Odd pins on top (component side), even on bottom | yes | KiCad; gralco; G5Dot; Antmicro; SATA-IO | yes | **Verified-by-agreement** |
| Pad width | 0.35 | KiCad; gralco; G5Dot; adryzz; Antmicro; SATA-IO | yes | **Verified-by-agreement** |
| Top pad, far end from edge | 2.00 | KiCad; gralco; G5Dot; adryzz; SATA-IO "2" | yes. Antmicro 2.70 | **Verified-by-agreement** (Antmicro outlier) |
| Bottom pad, far end from edge | 2.50 | KiCad; gralco; G5Dot; SATA-IO "2.50" | yes. Antmicro 3.20; adryzz 2.00 | **Verified-by-agreement** (outliers noted) |
| Pad setback from edge | KiCad 0.50 · G5Dot 0.55 · adryzz 0.55 · gralco 0.30 · Antmicro 0.20 | — | no | **Conflicting**. No normative value found; 0.50 is the most common |
| Half-moon mounting notch | R1.75 (Ø3.5), centred on the far end | KiCad; G5Dot; Antmicro oculink; SATA-IO. gralco uses a Ø3.5 PTH on the edge instead | yes | **Verified-by-agreement**. Antmicro oculink sits 0.13 off-centre |
| Mounting ground pad | gralco: Ø6.15 PTH pad. SATA-IO: Ø5.5 top / Ø6 bottom. G5Dot: custom polygon (not parsed) | — | no | **Single-source** (SATA-IO) |
| Board thickness | 0.80 | gralco README "should be 0.8mm thick"; SATA-IO 0.80 ± 0.08. KiCad stack-ups in the projects range 0.72–0.88 | yes | **Verified-by-agreement** |
| Finger finish | gralco README: select "Gold fingers"; gralco stack-up ENIG | — | n/a | **Not found** (no thickness or type) |
| Edge bevel or chamfer | not set in any M.2 project (gralco sets `(edge_connector yes)`, not `bevelled`) | — | — | **Not found** |
| Component height (S1/S2/D… profiles) | not in any open file | — | — | **Not found** |
| Other widths | KiCad also ships 3030 Key A and Key E and 3042 Key B footprints (30 mm wide) | KiCad only | — | out of scope; **Single-source** |

### M.2 verdict

**What can ship with numbers from at least two independent sources:**
- the 22 mm width;
- all five lengths, 2230/2242/2260/2280/22110. The 42/60/110 lengths are KiCad plus SATA-IO only;
- the 19.85 × 4.00 finger tab with R0.5 shoulders;
- 0.35 pads at 0.25 alternating pitch, with pin 1 at +9.25;
- the 1.20 notch to a 3.50 depth;
- key positions A (+6.625), B (+5.625), E (+2.625) and M (−6.125), so A, B, E, M and A+E/B+M combinations;
- the R1.75 half-moon;
- 0.8 mm thickness.

**Do not copy KiCad's notch depth (4.10).** It disagrees with every other source.

**What remains:**
- the edge bevel;
- plating;
- per-profile component heights;
- the normative pad setback;
- the ground-pad size (SATA-IO only).

---

## Disagreements with the earlier TE and SATA-IO numbers

- **PCIe pad width.** Open libraries use 0.70; TE Gen5 uses 0.60 ± 0.05.
- **PCIe pad near end.** Open libraries put it 1.30–1.40 from the edge; TE Gen5 gives 2.60 (ref). Long-pin geometry differs by generation, and both open libraries predate Gen5.
- **M.2 notch depth.** SATA-IO's 3.50 is confirmed as the depth to the notch **tip**. KiCad's official footprints disagree at 4.10. The earlier research's question, whether 3.5 meant the tip or the arc centre, is answered: it is the tip.
- **M.2 pad lengths.** SATA-IO's "2" (top) and "2.50" (bottom) match the open designs' pad **far end measured from the card edge**. They are not pad lengths: KiCad pads are 1.5/2.0 long, starting 0.5 in from the edge.
- Every other TE/SATA-IO value checked here agrees with the open sources.

## Licensing notes (not legal advice)

| Source | Licence as stated |
|---|---|
| KiCad footprint library | CC-BY-SA 4.0, with an exception for designs that use the library (`LICENSE.md`) |
| lucanastasio/PCIexpress-KiCad | CERN OHL v1.2. Stated in the README; there is no LICENSE file and GitHub shows none |
| KCORES/OCP2PCIe, KCORES/KCORES-FlexibleLOM-Adapter | GPL-3.0 |
| ZoidTechnology/PCIce | GPL-3.0 |
| Supercookiegaming/PCIe-Aux-Signal-Breakout | **no licence declared** |
| eggsampler/PCIeDMA | MIT |
| yutotnh/PCIeAdapter | MIT |
| antmicro/m2-oculink-adapter, m2-smart-iot-module, m2-pcie-adapter | Apache-2.0 |
| gralco/m2-sd-card | GPL-3.0+ (README) |
| septentrio-gnss/G5Dot | "CERN-OHL" (version not stated in the README) |
| adryzz/M.2-Key-E-Footprints | MIT |
| Keystone blank-bracket drawing | © Keystone Electronics (vendor datasheet) |
| NVIDIA ConnectX-7 user manual | © NVIDIA (vendor documentation) |

- REDA would take **bare dimensions** only, not geometry files or drawings.
- Dimensional facts are generally not copyrightable, so copyleft licences (GPL, CC-BY-SA, CERN-OHL) should not attach to a template built from numbers that several sources agree on. Still, cite the sources.
- Do not copy any project's `.kicad_mod` or `.kicad_pcb` into REDA.
- Where the only source is a copyleft or unlicensed file, or a file whose lineage runs to an unauthorized spec copy (KiCad M.2, adryzz), that is a provenance risk for counsel to weigh.

## Sources

All repositories were fetched on 2026-11-20 at the commit shown.

- [KICAD-FP] KiCad footprint library, `Connector_PCBEdge.pretty`: `BUS_PCIexpress_x1/x4/x8/x16`, `M.2_2230-xx-{A,B,E,M}`, `M.2_2242-xx-M`, `M.2_2280-xx-M`, `M.2_22110-xx-M`, `M.2_3030-xx-{A,E}`. https://gitlab.com/kicad/libraries/kicad-footprints, commit a0dcd398e7 (2026-11-02).
- [LUCA] lucanastasio/PCIexpress-KiCad: `PCIexpress_x{1,16}_{full,half,low}.kicad_pcb`, `PCIexpress_bracket_{full,low,adapter}.kicad_mod`, `PCIexpress_x1/x16.kicad_mod`. https://github.com/lucanastasio/PCIexpress-KiCad @ b5704546ce
- [KCORES-OCP] KCORES/OCP2PCIe: `OCP2PCIe-H-F-4/OCP2PCIe-H-F-4-V2.0Beta/V1.kicad_pcb` (x4). https://github.com/KCORES/OCP2PCIe @ 4d2b9eb8e4
- [KCORES-LOM] KCORES/KCORES-FlexibleLOM-Adapter: `Electricals/KCORES-FlexibleLOM-Adapter/KCORES-FlexibleLOM-Adapter.kicad_pcb` (x8). https://github.com/KCORES/KCORES-FlexibleLOM-Adapter @ 2401923550
- [PCICE] ZoidTechnology/PCIce: `hardware/pcice.kicad_pcb`. https://github.com/ZoidTechnology/PCIce @ d605e736a5
- [SUPERCOOKIE] Supercookiegaming/PCIe-Aux-Signal-Breakout: `KiCad/PCIe-Aux-Signal-Breakout/PCIe-Aux-Signal-Breakout.kicad_pcb` (x1 low profile). https://github.com/Supercookiegaming/PCIe-Aux-Signal-Breakout @ 2575d2802b
- [EGG] eggsampler/PCIeDMA: `PCIeDMA.kicad_pcb` (x4 low profile, derived from [LUCA]). https://github.com/eggsampler/PCIeDMA @ e48e8e5572
- [YUTO] yutotnh/PCIeAdapter: `PCIeAdapter.kicad_pcb` (x16 finger tab). https://github.com/yutotnh/PCIeAdapter @ 582f84286d
- [AM-OCU] antmicro/m2-oculink-adapter: `antmicro-m2-oculink-adapter-hw.kicad_pcb` (2280 M). https://github.com/antmicro/m2-oculink-adapter @ 067f607270
- [AM-IOT] antmicro/m2-smart-iot-module: `m2-smart-iot-module.kicad_pcb` (2280 B+M). https://github.com/antmicro/m2-smart-iot-module @ c986781e69
- [AM-PCIE] antmicro/m2-pcie-adapter: `m2-pcie-adapter.kicad_pcb`. This is a non-standard-width M-key card with an imprecise outline; it was inspected but not used for any value. https://github.com/antmicro/m2-pcie-adapter @ 11844f18ce
- [GRALCO] gralco/m2-sd-card: `m2-sd-card.kicad_pcb` and README (2230 E). https://github.com/gralco/m2-sd-card @ d48d5e87dd
- [G5DOT] septentrio-gnss/G5Dot: `G5Dot/G5Dot Kicad/G5Dot.kicad_pcb` (2230 A+E). https://github.com/septentrio-gnss/G5Dot @ f8ba155d2f
- [ADRYZZ] adryzz/M.2-Key-E-Footprints: `2230-key-E.kicad_mod`. https://github.com/adryzz/M.2-Key-E-Footprints @ 1b8806e49a
- [KEYSTONE] Keystone Electronics, standard and low-profile blank PC bracket drawing (Farnell-hosted datasheet). The vector page was rendered; I read "4.725 [120.0]", "3.118 [79.2]", ".450 [11.5]" and ".466 [11.9]". https://www.farnell.com/datasheets/1843967.pdf
- [NV-CX7] NVIDIA ConnectX-7 Adapter Cards User Manual, Specifications: "PCIe Full Height, Half Length 4.37 in. x 6.6 in. (111.15mm x 167.65 mm)" and "PCIe Half Height, Half Length 2.71 in. x 6.6 in. (68.90mm x 167.65 mm)". https://networking-docs.nvidia.com/connectx7hw/specifications
- [AMD-DS962] AMD Alveo U200/U250 data sheet, Dimensions: "4.381 inch (111.28 mm) maximum", standard height. https://docs.amd.com/r/en-US/ds962-u200-u250/Dimensions
- Earlier research: `docs/research/form-factor-specs/industry.md` on `research/form-factor-specs` ([TE-CD] TE 2364427 rev D2; [AMD-DS1002]; [SATA-M2] SATA-IO TP053v11).

Not found as open design files:
- Antmicro's `virtex-ultrascale-pcie` repo holds only FPGA designs.
- RHS NiteFury/LiteFury publishes only PDFs (schematic and block diagram), no PCB.
- KiCad's official template repo has no PCIe or M.2 templates.
