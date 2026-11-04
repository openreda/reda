# IPC values for the generators and the current estimate

Research for [issue 143](https://github.com/openreda/reda/issues/143), under [map 123](https://github.com/openreda/reda/issues/123). Facts only. Decisions belong to the later grilling tickets ([135](https://github.com/openreda/reda/issues/135) and [138](https://github.com/openreda/reda/issues/138) are blocked on this one).

Researched 2026-11-04 against `main` at `516cba3`.

## Summary

- **Fillet goals.** The 0.35 / 0.35 / 0.03 style values in `ipc.rs` agree with two independent transcriptions of IPC-7351B, with one exception: the small-chip heel. REDA uses -0.05 mm. Both transcriptions of the B edition say 0.00.
- **F and P.** REDA uses F 0.05 and P 0.025 mm. A generator default and a secondary guide use 0.10 and 0.05. PCB Libraries, whose founder sat on the IPC subcommittee, states ±0.05 and ±0.025, and a poster on its forum says IPC writes the same tolerance as one total value. On that reading REDA's figures are half of what the formula expects. The numeric effect after rounding is small.
- **Rounding.** REDA rounds Z and X up and G down on a 0.05 mm grid for every family. Other sources round to nearest. Small chips use a finer grid in every source that gives one (0.02 or 0.01 mm).
- **Courtyard excess.** 0.50 / 0.25 / 0.10 mm for most families, 0.20 / 0.15 / 0.10 mm for small chips. REDA matches at Nominal.
- **Names.** REDA's SMD names match the 2015-16 PCB Libraries form of the convention. Its through-hole names do not match that company's through-hole form. Sources disagree on the QFN and SON body order and on the thermal-pad part of the name.
- **Current.** `I = k · ΔT^0.44 · A^0.725`, with A in mil², ΔT in °C, k 0.048 external and 0.024 internal. No source seen places this equation in the standard's own text. It is described as a fit to the chart.
- **Via protection.** Seven numbered types and one unnumbered, confirmed from IPC's own contents page.

## What could and could not be verified

IPC-7351B, IPC-7352, IPC-2221 and IPC-4761 are paywalled, and no value below was read from the full text of any of them. The primary material available was IPC's published front matter, which confirms titles, table numbers and dates but not values.

| Source | Kind | Used for |
| --- | --- | --- |
| [IPC-7351B contents](https://www.electronics.org/TOC/IPC-7351B.pdf) | Primary, front matter only | Table numbers and titles, June 2010 date, supersedes 7351A (February 2007) |
| [IPC-4761 contents](https://www.electronics.org/TOC/IPC-4761.pdf) | Primary, front matter only | Via type names |
| [IPC-2221A contents](https://www.electronics.org/TOC/IPC-2221A.pdf), [IPC-2221B contents](https://www.electronics.org/TOC/IPC-2221B.pdf), [IPC-2152 contents](https://www.electronics.org/TOC/IPC-2152.pdf) | Primary, front matter only | Where the current charts sit, editions |
| [Hausherr, "PCB Design Perfection Starts in the CAD Library, Part 1"](https://www.electronics.org/system/files/technical_resource/E6&S17_01.pdf) | Conference paper hosted by IPC | Round-off and small-chip rule changes |
| [footag2, `src/ipc7351b/table.c`](https://github.com/endofexclusive/footag2/blob/master/src/ipc7351b/table.c) | Open-source library that cites table numbers; its author says it reflects their understanding of B | Fillet goals, called **Transcription A** below |
| [Archived generator, `ipc_definitions.yaml`](https://api.github.com/repositories/42412295/contents/scripts/Packages/ipc_definitions.yaml?ref=76fedd51e8680e6cd3b729dcbd6f1a56af53b12c) and [`ipc7351B_smd_two_terminal_chip.yaml`](https://api.github.com/repositories/42412295/contents/scripts/SMD_chip_package_rlc-etc/ipc7351B_smd_two_terminal_chip.yaml?ref=76fedd51e8680e6cd3b729dcbd6f1a56af53b12c) | Open-source generator, commit `76fedd5`. Its header says no open copy of B was available to its authors | Fillet goals, called **Transcription B** below |
| [PCB Libraries forum](https://www.pcblibraries.com/forum/) and [naming documents](https://www.cskl.de/fileadmin/csk/dokumente/produkte/pcbl/ipc_standard_pcb_library_expert_IPC_7351B_Land_Pattern_Naming_Convention.pdf) | Secondary. Forum posts by admin "Tom H" are taken here to be Tom Hausherr, who describes his own work on the IPC draft | F and P, courtyard, naming, standard status |
| [PCBSync guide](https://pcbsync.com/ipc-7351-land-pattern/), [AtlasPCB guide](https://www.atlaspcb.com/blog/pcb-land-pattern-design-ipc-7351b-footprint-standards/) | Secondary, fabricator blogs. AtlasPCB's page has internal errors | Cross-checks and disagreements |
| [AdvancedPCB calculator notes](https://www.advancedpcb.com/en-us/tools/trace-width-calculator/), [mbedded.ninja](https://blog.mbedded.ninja/pcb-design/how-to-calculate-maximum-track-current/) | Secondary | Current formula, constants, limits |

Agreement between the transcriptions is not proof. They may share a reading, and neither can be checked here against the printed tables.

Full-text uploads of IPC-7351 turned up in search. They were not opened and nothing here relies on them.

## IPC-7351B

### Status of the document

- IPC-7351B is dated June 2010 and supersedes 7351A of February 2007 ([contents](https://www.electronics.org/TOC/IPC-7351B.pdf)).
- Secondary sources say IPC lists it as no longer maintained and that IPC-7352 (2023) follows it ([PCBSync](https://pcbsync.com/ipc-7351-land-pattern/), [HilPCB](https://hilpcb.com/en/blog/ipc-7351-land-pattern-standard/)). IPC's own status table was not seen.
- A 7351C was drafted and restarted, and no release was found. Hausherr wrote in 2019 that the committee planned to set F and P to 0.00 in C, shrinking pads by 0.01 to 0.04 mm ([forum thread](https://www.pcblibraries.com/forum/ipc7351c-draft-or-release-date_topic1818_page2.html)).
- Hausherr also says IPC-J-STD-001 governs solder-joint acceptability and has priority over 7351, which he calls a guideline ([forum thread](https://www.pcblibraries.com/forum/ipc7351-smd-pth-reference-calculators_topic785.html)).
- No source describing IPC-7352's fillet goals in full was available. A 2025 forum post by Hausherr gives its courtyard values as in the courtyard section below.

### Density levels

Level A is Most, level B Nominal, level C Least. The name suffixes are M, N and L ([Hausherr paper](https://www.electronics.org/system/files/technical_resource/E6&S17_01.pdf), [naming document](https://www.cskl.de/fileadmin/csk/dokumente/produkte/pcbl/ipc_standard_pcb_library_expert_IPC_7351B_Land_Pattern_Naming_Convention.pdf)).

### Formulas

Z over the outer land edges, G between the inner land edges, X the land width:

- `Z = Lmin + 2·Jt + √(CL² + F² + P²)`
- `G = Smax − 2·Jh − √(CS² + F² + P²)`
- `X = Wmin + 2·Js + √(CW² + F² + P²)`

Sources: [PCBSync](https://pcbsync.com/ipc-7351-land-pattern/), both transcriptions. S uses a root-sum-square tolerance, `CS = √(CL² + 2·CT²)`, with Smax reduced by half the difference from the worst case. Both transcriptions do this, and so does `ipc::span`. Transcription A notes that the formula's combination is not strictly a root mean square, although the standard calls it one.

### Fillet goals by family

Values in mm, listed as level A / B / C, for the families the v0.1 generators use. Table numbers and titles are confirmed from IPC's contents page. The values come from the two transcriptions, which agree on every row here.

| IPC table | Family | Toe | Heel | Side | Round-off | Courtyard excess |
| --- | --- | --- | --- | --- | --- | --- |
| 3-2 | Gull-wing, pitch > 0.625 | 0.55 / 0.35 / 0.15 | 0.45 / 0.35 / 0.25 | 0.05 / 0.03 / 0.01 | 0.05 | 0.50 / 0.25 / 0.10 |
| 3-3 | Gull-wing, pitch ≤ 0.625 | 0.55 / 0.35 / 0.15 | 0.45 / 0.35 / 0.25 | 0.01 / -0.02 / -0.04 | 0.05 | 0.50 / 0.25 / 0.10 |
| 3-5 | Chip, 1608 and larger | 0.55 / 0.35 / 0.15 | 0.00 / 0.00 / 0.00 | 0.05 / 0.00 / -0.05 | 0.05 | 0.50 / 0.25 / 0.10 |
| 3-6 | Chip, smaller than 1608 | 0.30 / 0.20 / 0.10 | 0.00 / 0.00 / 0.00 | 0.05 / 0.00 / -0.05 | 0.02 | 0.20 / 0.15 / 0.10 |
| 3-15, 3-16 | QFN, SON | 0.40 / 0.30 / 0.20 | 0.00 / 0.00 / 0.00 | -0.04 / -0.04 / -0.04 | 0.05 | 0.50 / 0.25 / 0.10 |

Other families (J-lead, MELF, chip arrays, inward L-lead, pull-back no-lead, electrolytic, flat lead) are in Transcription A for when a generator needs them. Its table lists 3-2 to 3-7, 3-9 to 3-11, 3-13, 3-15, 3-16, 3-18, 3-20 and 3-22.

#### Where sources disagree

| Value | Source A | Source B | Note |
| --- | --- | --- | --- |
| Gull-wing heel on short-lead parts | 0.45 / 0.35 / 0.25 (table body, all sources) | 0.25 / 0.15 / 0.05 under Notes 1 and 2 of tables 3-2 and 3-3 (Transcription A) | [footag](https://github.com/endofexclusive/footag/blob/master/src/ipc7351b/ipc7351b.c), by the same author, leaves this unimplemented. It says the condition is roughly `Smin ≤ Amax` and `T1 ≤ 0.5`, that Amax is not defined in 7351B but appears in IPC-SM-782A, and that it does not know what T1 means. |
| Chip heel, Nominal | 0.00 (both transcriptions, PCBSync) | -0.05 for all chip sizes (AtlasPCB) | AtlasPCB's page also has arithmetic and format errors. Transcription B's older table, apparently for an edition before B, has small-chip heel -0.05 at every level with toe 0.2 / 0.1 / 0.0 ([older table](https://api.github.com/repositories/42412295/contents/scripts/SMD_chip_package_rlc-etc/ipc_smd_two_terminal_chip.yaml?ref=76fedd51e8680e6cd3b729dcbd6f1a56af53b12c)). |
| Chip side, Nominal | 0.00 (both transcriptions, PCBSync) | 0.03 (AtlasPCB) | |
| Gull-wing side, fine pitch | -0.02 at B (both transcriptions) | 0.03 for all gull-wing (PCBSync, AtlasPCB) | These two guides do not separate out table 3-3. |
| QFN toe | 0.40 / 0.30 / 0.20 (both transcriptions) | 0.55 / 0.35 / 0.15 (AtlasPCB) | |
| J-lead | toe 0.10 / 0.00 / -0.10, heel 0.55 / 0.35 / 0.15 (Transcription A) | toe 0.10 / 0.00 / 0.00 (PCBSync); toe and heel swapped (AtlasPCB); Transcription B's data has Most heel 0.25 where its own comment says 0.55 | No v0.1 generator uses this. |
| Tool defaults against the standard | Values above | A forum post says PCB Libraries' tool defaults differ from B: a ±0.10 toe spread for all chips, and J-STD-001 based goals ([chip thread](https://www.pcblibraries.com/forum/ipcjstd001-chip-component-solder-joints_topic2588.html)) | Confirms B's Nominal toe as 0.35 above 0603 and 0.20 below it. |

### Fabrication and placement tolerance

| Source | F (mm) | P (mm) | How it is stated |
| --- | --- | --- | --- |
| [PCBSync](https://pcbsync.com/ipc-7351-land-pattern/) | 0.10 | 0.05 | Described as the values in IPC's worked example, adjustable to the fabricator's capability |
| Transcription B ([calculator defaults](https://api.github.com/repositories/42412295/contents/scripts/tools/ipc_pad_size_calculators.py?ref=76fedd51e8680e6cd3b729dcbd6f1a56af53b12c)) | 0.10 | 0.05 | Default parameters, also set in its chip configuration |
| [PCB Libraries forum, Hausherr 2014](https://www.pcblibraries.com/Forum/land-pattern-calculation-of-gull-wing-lead-parts_topic1314.html) | ±0.05 | ±0.025 | Stated as plus-or-minus values |
| [PCB Libraries forum, poster Jeff.M, 2014](https://www.pcblibraries.com/forum/ipc7351-smd-pth-reference-calculators_topic785.html) | | | IPC shows F and P as a single value. The company uses plus-or-minus values and doubles them before squaring. The result is the same. |
| Search-result excerpt of the 2005 edition | 0.1 | 0.2 | In an SO16 example. Seen only as a search excerpt and not read in context, so unverified. |
| [Hausherr 2019](https://www.pcblibraries.com/forum/ipc7351c-draft-or-release-date_topic1818_page2.html) | 0.00 | 0.00 | The plan for the unreleased 7351C |

Read together, 0.10 total and ±0.05 are the same tolerance, and 0.05 total and ±0.025 likewise. No source seen gives the printed text of 7351B on this.

### Round-off

- **Land size and spacing, 1608 and larger:** 0.05 mm. The paper describes land centres on a 0.05 mm snap grid ([Hausherr paper](https://www.electronics.org/system/files/technical_resource/E6&S17_01.pdf)).
- **Chips smaller than 1.6 × 0.8 mm:** the paper lists these changes: land snap grid 0.1 to 0.02 mm, land size round-off 0.05 to 0.01 mm, toe 0.35 to 0.2 mm, corner radius 0.2 to 0.15 mm, courtyard excess 0.25 to 0.15 mm, placement grid 0.5 to 0.1 mm. It also says to enter the nominal terminal dimension as both minimum and maximum. Both transcriptions list table 3-6's round-off as 0.02, so 0.01 and 0.02 disagree unless they refer to size and snap grid respectively.
- **Direction:** PCBSync's worked example rounds to nearest (Z 6.92 to 6.90, G 3.71 to 3.70). Transcription B also rounds to nearest. A search excerpt of the 2005 edition says the tables carry a suggested round-up or round-down, which was not confirmed. No source seen states a rule of Z and X up, G down.
- **Courtyard grid:** Transcription A rounds up to a configurable grid. Transcription B rounds to nearest on a configurable grid. AtlasPCB says outward to 0.05 mm. Hausherr's 1608 example lands at 3.0 × 1.5 mm, which suits a 0.5 mm placement grid. No source seen gives a standard courtyard grid.

### Courtyard excess

- 7351B: Least 0.10, Nominal 0.25, Most 0.50 ([Hausherr 2022](https://www.pcblibraries.com/forum/how-is-a-courtyard-excess-defined_topic3060.html)), with table 3-6 at 0.10 / 0.15 / 0.20 in both transcriptions.
- PCB Libraries' tool defaults differ: Nominal 0.20, Most 0.40, from customer feedback ([2024 thread](https://www.pcblibraries.com/forum/placement-courtyard-excess_topic3372.html)).
- IPC-7352, according to the same thread in 2025: the fillet tables carry 0.10 / 0.25 / 0.50. A forum user also reports a 7352 table with separate distances from pad edge and from body (SMD B: 0.25 and 0.50; through-hole B: 0.50 and 1.00). That post labels level A as Least, the reverse of 7351B's naming. Not confirmed against the document.
- The excess is added to each side of the larger of the body and the lands, per the transcriptions' courtyard code. Parts may be placed so that courtyards touch but do not overlap (Hausherr 2022).

### Naming convention

IPC-7351B table 3-23 is titled "IPC-7351 Land Pattern Naming Convention" ([contents](https://www.electronics.org/TOC/IPC-7351B.pdf)). The table itself was not available. The forms below come from PCB Libraries, whose documents come in two generations.

**Rules** ([2015-16 document](https://www.cskl.de/fileadmin/csk/dokumente/produkte/pcbl/ipc_standard_pcb_library_expert_IPC_7351B_Land_Pattern_Naming_Convention.pdf)):

- metric, in hundredths of a millimetre, with trailing zeros kept and leading zeros dropped
- chip body sizes use one digit on each side of the decimal point
- `X` separates dimensions, `-` comes before the pin count, `_` adds modifiers
- the last character is M, N or L, except for BGA

**Forms, 2015-16:**

| Family | Form |
| --- | --- |
| Chip | `RESC`, `CAPC` or `INDC`, then body length, body width, `X`, height |
| SOT | `SOT`, pitch, `P`, lead span, `X`, height, `-`, pins (example `SOT230P700X180-4`) |
| SOIC | `SOIC127P`, lead span, `X`, height, `-`, pins |
| SOP | `SOP`, pitch, `P`, lead span, `X`, height, `-`, pins |
| QFP | `QFP`, pitch, `P`, span L1, `X`, span L2, `X`, height, `-`, pins |
| QFN, SON | prefix, pitch, `P`, body, `X`, body, `X`, height, `-`, pins, plus thermal pad (example `QFN50P350X350X100-19_15T205X205`) |

**Disagreements:**

- **Body order on QFN and SON:** length then width in the 2015-16 document, width then length on [pcb-3d.com](https://www.pcb-3d.com/wordpress/tutorials/ipc-7351b-naming-convention-for-surface-mount-device-3d-models-and-footprints/).
- **Thermal pad:** the sources show a `T` term with the pad size (PCBSync, the example above). No source says whether the pin count includes the thermal pad.
- **SSOP and TSSOP prefixes:** pcb-3d.com gives them their own prefixes with pitch and height limits (for example SSOP at 0.30 to 0.50 mm pitch above 1.6 mm). At 0.65 mm pitch it uses `SOP`.
- **2017 form:** the [later document](https://www.cskl.de/fileadmin/csk/dokumente/produkte/pcbl/ipc_standard_pcb_library_expert_Land_Pattern_Naming_Convention.pdf) moves the pin count forward and adds lead and thermal dimensions (for example `QFN` + pins + `P` + pitch + `_` + body + `L` + lead + `T` + pad). It is the company's convention, not a stated IPC change.
- **Through-hole:** PCB Libraries names carry lead span, lead width, pitch, body and height, then a complexity level, for example `DIP762W52P254L1905H508Q14B` and `HDRV200W52P200R2L4400T400H900B`.
- **Manufacturer prefix:** PCB Libraries recommends a manufacturer prefix to keep apart packages with the same name but different tolerances.

## Comparison with `reda-library`

Compared with `crates/reda-library/src/generate/` at `516cba3`. The spec calls for IPC-7351 Nominal only (`docs/spec/v0.1/library.md`), so only level B is built. `Goals` has no density parameter.

### Matches

- All three formulas, including the S tolerance (`ipc.rs`, `ipc::span`).
- Nominal goals for tables 3-2, 3-3, 3-5 and 3-15/3-16: toe, heel, side and courtyard.
- Small-chip toe 0.20, side 0.00 and courtyard 0.15.
- The 0.625 mm pitch split between tables 3-2 and 3-3.
- SMD name forms for chip, SOT, SOIC, SOP and QFP, against the 2015-16 document.
- Exposed-pad clearance 0.2 mm, which Transcription B also uses, as a generic rule it does not attribute to a table.

### Differences

| Item | REDA | Sources | Effect |
| --- | --- | --- | --- |
| Small-chip heel | -0.05 (`Goals::SMALL_CHIP`) | 0.00 in both B transcriptions. -0.05 appears in the pre-B table and on AtlasPCB. | 0402 raw G 0.445 rounds to 0.40 today. At heel 0.00, raw G 0.345 rounds to 0.30. Pad length 0.55 becomes 0.60, centres ±0.475 become ±0.45. |
| F and P | 0.05 and 0.025 (`FABRICATION`, `PLACEMENT`), used unscaled in the formula | 0.10 and 0.05 as single values, or ±0.05 and ±0.025 doubled | See the table below. |
| Rounding direction | Z and X up, G down | Nearest in Transcription B and PCBSync. Not confirmed from the standard. | SOIC-8 is 6.95 / 2.95 / 0.60 with REDA's direction. Rounding to nearest gives 6.90 / 3.00 / 0.60. |
| Small-chip round-off | 0.05 (`ROUND_OFF`, for every family) | 0.02 in both transcriptions, 0.01 in the Hausherr paper | 0402 Z is 1.465 raw, 1.50 in REDA. |
| Small-chip test | `length.nominal >= 1.5 mm`, width ignored | Transcription A: length under 1.6 or width under 0.8 is small. Hausherr: smaller than 1.6 × 0.8. | Only custom dimensions can differ, for example a part 1.55 mm long. |
| Short-lead gull-wing | `keep_body_middle_clear`, REDA's own rule, keeping the middle half of the body free of copper | Notes 1 and 2 of tables 3-2 and 3-3 may cover the same case with heel 0.25 / 0.15 / 0.05, condition unclear | The rule is documented in `gull_wing.rs` as a departure. It affects SOT-23. |
| Heel against exposed pad | The exposed pad shrinks to keep its clearance (`exposed_pad_spec`) | Transcription B reduces the lead heel and leaves the exposed pad | No IPC rule found either way. |
| Courtyard grid | Outward to 0.05 mm (`COURTYARD_GRID`) | No standard value found. AtlasPCB gives the same. | |
| Corner radius | A quarter of the shorter side, at most 0.25 mm | The Hausherr paper gives 0.2 mm, and 0.15 mm for small chips | 0402 radius is 0.1375 mm today. |
| Goals as inputs | Fixed constants in `ipc.rs`, not parameters | Issue 135 lists fillet goals and tolerances as possible parameters | |

Effect of F and P on raw values, then with REDA's rounding (mm). The dimensions are the built-in tables, the SOIC-8 row from the existing test, with REDA's current goals including the small-chip heel.

| Part | F 0.05, P 0.025: Z / G / X | F 0.10, P 0.05: Z / G / X | Changes after rounding |
| --- | --- | --- | --- |
| RESC1608 | 2.455 / 0.677 / 0.955 | 2.470 / 0.670 / 0.970 | None |
| CAPC1005 | 1.465 / 0.445 / 0.565 | 1.500 / 0.430 / 0.600 | None |
| RESC0603 | 1.052 / 0.313 / 0.352 | 1.097 / 0.287 / 0.397 | G 0.30 becomes 0.25 |
| SOIC-8 | 6.904 / 2.982 / 0.578 | 6.915 / 2.978 / 0.599 | None |

These figures were calculated for this note from the formulas above and were not run through the generators.

### Naming differences

| Item | REDA | Sources |
| --- | --- | --- |
| QFN and SON body order | `<E>X<D>`, width then length (`no_lead.rs`) | Length first in the 2015-16 document, width first on pcb-3d.com. For square bodies the order has no effect. |
| Exposed pad in the count | Counted: `QFN50P500X500X100-33N` for 32 leads | Not stated by any source seen. The sources use a `T` thermal term that REDA omits. |
| SSOP and TSSOP | `SOP` prefix at 0.65 mm pitch | Consistent with both sources at that pitch. |
| DIP | `DIP254P762X533-8`, no density letter. The doc comment calls this an IPC-7351 name. | PCB Libraries: `DIP762W52P254L1905H508Q14B`. Through-hole patterns belong to IPC-7251 rather than 7351. |
| Headers and sockets | `HDR254P2X5`, `SKT127P2X5` | `HDRV`, `HDRRA` and the long form above. |

### Through-hole, outside this ticket's list

`through_hole.rs` calls hole +0.20 mm and annular allowance 0.25 mm the IPC-2222 / IPC-7251 Nominal values. Transcription A's author's table from an IPC-7251 draft, for multiple-leaded semiconductors, gives Nominal hole +0.20, annular 0.35, antipad 0.70 and courtyard 0.25, with round-off 0.10 ([footag](https://github.com/endofexclusive/footag/blob/master/src/ipc7251draft1/ipc7251draft1.c)). The annular values differ. This is a working draft, not a released standard, and it was not researched further.

## IPC-2221 current capacity

### Formula

```
I = k · ΔT^b · A^c

I   current, A
ΔT  temperature rise above ambient, °C
A   copper cross-section, mil²

b = 0.44, c = 0.725
k = 0.048   external conductors
k = 0.024   internal conductors
```

Solved for width ([AdvancedPCB](https://www.advancedpcb.com/en-us/tools/trace-width-calculator/)):

```
A = (I / (k · ΔT^b))^(1/c)
width [mil] = A / (thickness [oz] · 1.378 [mil/oz])
```

Conversions: 1 mil² = 6.4516 × 10⁻⁴ mm². 1 oz/ft² is taken as 1.378 mil, about 35 µm.

Worked check, calculated for this note: 1 A, 10 °C rise, 1 oz gives 16.3 mil² and 11.8 mil external, 42.4 mil² and 30.8 mil internal. The same current needs 2.6 times the area internally, because `2^(1/0.725) ≈ 2.60`.

### Sources and agreement

All of these agree on the constants: [AdvancedPCB](https://www.advancedpcb.com/en-us/tools/trace-width-calculator/), [mbedded.ninja](https://blog.mbedded.ninja/pcb-design/how-to-calculate-maximum-track-current/), [standardclarity](https://standardclarity.com/guides/ipc-2221-vs-ipc-2152/), [pcb-toolkit](https://docs.rs/pcb-toolkit/0.1.5/src/pcb_toolkit/current.rs.html), [Qorvo](https://qorvo.com/design-hub/design-tools/pcb-trace-power-handling-calculator).

### Limits

The range usually given is current up to 35 A, width up to 400 mil (10.16 mm), rise 10 to 100 °C, copper 0.5 to 3 oz/ft². Outside it the formula extrapolates ([AdvancedPCB](https://www.advancedpcb.com/en-us/tools/trace-width-calculator/), [mbedded.ninja](https://blog.mbedded.ninja/pcb-design/how-to-calculate-maximum-track-current/)). One calculator [says widths under 500 mil](https://www.getzenquery.com/tools/pcb-trace-current-calculator/). A [forum summary](https://www.elektroda.com/qa,pcb-trace-width-current-copper.html) adds 17.5 A internal, which was not confirmed elsewhere.

### Where it comes from, and where sources disagree

- **Origin.** AdvancedPCB calls the constants a curve fit to the IPC-2221 charts. standardclarity calls the equation an empirical fit from MIL-STD-275 and IPC-D-275 data from the 1950s. mbedded.ninja says it is derived from one graph. None shows the equation printed in the standard, and this could not be checked.
- **Where the chart sits.** IPC-2221A's contents list Figure 6-4 as "Conductor Thickness and Width for Internal and External Layers", plus an Appendix B on current-carrying capacity with an "Original Design Chart" and an "IPC 2221A External Conductor Chart" ([contents](https://www.electronics.org/TOC/IPC-2221A.pdf)). In [IPC-2221B's contents](https://www.electronics.org/TOC/IPC-2221B.pdf), Figure 6-4 is a transmission line figure, so that number should not be cited without the edition. The clause number in current editions was not established.
- **Editions.** 2221 (February 1998), 2221A (May 2003) and a 2221B that supersedes both are confirmed from IPC's contents. 2012 for B and 2023 for C come from [standardclarity](https://standardclarity.com/standards/ipc-2221/), which also says the method is unchanged between B and C. IPC-2221C's contents were not available.
- **The internal constant.** [PCBSync](https://pcbsync.com/?p=34352) says the internal chart was made by halving external current, with no test data behind it. [Another guide](https://pcbandassembly.com/?p=9716) explains the gap by FR-4's poor heat conduction. These are different accounts, and neither could be verified.
- **Accuracy.** mbedded.ninja says the formula tends to underestimate capacity. standardclarity says it assumes a bare trace in still air and is not a guaranteed safety baseline. These pull in different directions. Other named assumptions: a long track with no vias, and DC or RMS current.
- **IPC-2152.** Published 2009 by a task group on current-carrying capacity ([contents](https://www.electronics.org/TOC/IPC-2152.pdf)). It uses charts and correction factors, not one formula. It is described as intended to replace the 2221 charts (mbedded.ninja), and also as no longer maintained ([standardclarity](https://standardclarity.com/guides/ipc-2221-vs-ipc-2152/)). A third-party fit (k about 0.089 and 0.063, exponents 0.5 and 0.65) [appears on one calculator](https://salitronic.com/pcb-trace-width-calculator/) and is not from the standard.

### In REDA

Neither the code nor `docs/spec/` has a current estimate today.

## IPC-4761 via protection types

Names are from [IPC's contents page](https://www.electronics.org/TOC/IPC-4761.pdf), section 5, *Design Guide for Protection of Printed Board Via Structures*, published July 2006 and listed as current on [a standards reseller](https://shop.standards.ie/en-ie/standards/ipc-4761-0-574428_saig_ipc_ipc_1314640/).

| Type | Name |
| --- | --- |
| I | Tented |
| II | Tented and covered |
| III | Plugged |
| IV | Plugged and covered |
| V | Filled |
| VI | Filled and covered |
| VII | Filled and capped |
| none (section 5.8) | Partially filled |

- **One-sided and two-sided.** Secondary sources add suffix `a` for one side and `b` for both, on the tented, covered and plugged types ([Leiton](https://www.leiton.de/special-technologies-via-plugging-tenting-filling-ipc-4761.html)). The contents page does not show them. Leiton lists none for V and VII.
- **Disagreement.** Leiton's page numbers some types inconsistently: its prose uses VI for both plugged and filled variants. IPC's contents page settles the names.
- **In REDA.** `crates/reda-output/src/plot/openings.rs` documents that Vias stay tented, with no type name. Neither the spec nor the glossary names a Via protection type.

## Not verified

- Every numeric value in IPC-7351B, IPC-7352 and IPC-2221 against the printed standard.
- The exact condition and meaning of Notes 1 and 2 on tables 3-2 and 3-3.
- The rounding direction, and a standard courtyard grid.
- Whether a QFN or SON pin count includes the thermal pad, and table 3-23's own wording.
- IPC-7352's fillet goals, and its status from IPC directly.
- The clause holding the current chart in IPC-2221B and C.
