# What JLCPCB and PCBWay expect in uploads

Research for [openreda/reda#142](https://github.com/openreda/reda/issues/142). All fab pages were fetched on 2026-11-04. Findings only; a later grilling ticket decides what to do with them.

How to read this file:

- **Documents X**: a fab page states it, with a link.
- **Documents nothing**: the pages listed for that topic were read and do not state it.
- **Could not verify**: see [Not documented or not verified](#not-documented-or-not-verified).
- `J`/`P` tags link to fab pages. `W` tags are fab pages whose URLs name an EDA suite, so their URLs are left out of this file; they are described in [Sources](#sources).
- "`.GTL`-family extensions" means the three-letter layer extensions `.GTL`, `.GBL`, `.GTS` and so on. It is the only `FileNaming` value REDA has (`crates/reda-model/src/output.rs:15-18`).

## Answer summary

- **Gerber zip:** both fabs read RS-274X and `.GTL`-family extensions. JLCPCB's naming table is suggested, not mandatory. JLCPCB and PCBWay both treat the BOM and CPL as separate uploads from the Gerber archive. REDA's default zip holds all of them together.
- **Gerber X2:** JLCPCB's pages disagree. One suite-specific guide says X2 is not supported; another says to turn X2 on. PCBWay recommends RS-274-X and does not mention X2. All shipped presets write X2.
- **Excellon:** JLCPCB expects Excellon. PCBWay requires drill files, but the pages read do not state a format. Separate PTH and NPTH files: JLCPCB's pages disagree, and PCBWay shows both styles depending on the tool. REDA's routed non-plated slots sit in the NPTH drill file, while JLCPCB's capabilities page asks for them as an outline in `.GM1` or `.GKO`.
- **BOM:** the JLCPCB preset matches the required three columns. Its part-number header differs from JLCPCB's own sample, and JLCPCB's pages disagree on what else is required. The PCBWay preset's headers match neither PCBWay's page nor its sample exactly. Both fabs' samples separate Designators with commas; REDA uses comma plus space, which PCBWay's sample also contains.
- **CPL:** the JLCPCB preset's headers match JLCPCB's sample in the same order. Side values differ from that sample (`T`/`B` versus `Top`/`Bottom`), though JLCPCB's help page says Top/Bottom. PCBWay's pages give three different units and its sample has different headers from the preset.
- **Origin:** REDA subtracts the CPL origin from CPL positions only, while Gerber and drill coordinates stay in board coordinates. JLCPCB asks for a common origin across Gerber and CPL.
- **Bottom side:** neither fab documents a mirroring rule. PCBWay documents a top-view convention, in one suite-specific guide. JLCPCB documents nothing on bottom-side coordinates or rotation.
- **Rules:** the JLCPCB preset's values sit on JLCPCB's published 2-layer 1 oz minimums, except the Via diameter, which is larger on purpose. The PCBWay preset's comment says PCBWay publishes no hole spacing or edge clearance, but its Advanced PCB table does. No preset holds mask, silkscreen, slot or board-parameter values.

## Gerber zip

| Topic | JLCPCB | PCBWay | REDA |
|---|---|---|---|
| Format | RS-274X (W1, W2); [J5] names RS-274X as the format to use | RS-274-X recommended [P8]; accepted upload types are Gerber, `.pcb`, `.pcbdoc`, `.cam` [P8] [P1] | `%FSLAX46Y46*%`, `%MOMM*%` (`crates/reda-output/src/gerber.rs:380`) |
| X2 | **Pages disagree.** W2: not supported. W1: turn extended X2 on. [J5] recommends RS-274X throughout, then says X2 in its conclusion. [J6] lists X2 without saying whether JLCPCB accepts it | RS-274-X only; X2 not mentioned [P8] | On in all three presets (`crates/reda-output/src/presets.rs:27-30`, `:54`, `:91`, `:124`) |
| Coordinate format | 4.6 in mm (W1) | Documents nothing | 4.6 in mm (`crates/reda-output/src/gerber.rs:380`) |
| Layer naming | Suggested, not required: `.GTL .GBL .GTS .GBS .GTO .GBO`, `.GKO` or `.GM1`, `.G2L .G3L`, `.XLN` [J2]. Paste is not in that table. W1 says the `.GTL`-family extensions are expected and files without them may need manual layer assignment | Per-tool table; two rows include `gtl gbl gts gbs gto gbo gtp gbp`, outline `gko/gm` or `.GM/.GKO` [P3]. No statement that one naming is required | `<project>.GTL GBL GTS GBS GTP GBP GTO GBO GKO` (`crates/reda-output/src/plot.rs:48-90`) |
| Required layers | Copper, mask, silkscreen, outline, NC drill [J5]; same list plus inner layers (W1). Order held without an outline (W1) | Outline, copper, mask, silkscreen, NC drill. Mask, drill and silkscreen can be declared "none" at order time; the outline cannot [P4] | Top and bottom copper only (`crates/reda-output/src/plot.rs:55-68`), mask and paste always, silkscreen when the layer exists (`crates/reda-output/src/plot.rs:85`). The v0.1 PCB is 2-layer (`docs/spec/v0.1/README.md:3`) |
| Paste | Needed for stencil or SMT orders; harmless on bare-board orders (W1, [J5]) | Assembly minimum is silkscreen, copper and paste layers [P1] [P17] | Always written |
| Archive | One `.zip` or `.rar` [J6] (W1); `.zip` [J4] | One Gerber set per order normally [P9]; the pages read do not state an archive type generically | `<project>-<job>.zip`, every file at top level, deflated (`crates/reda-output/src/archive.rs:1-3`, `crates/reda-engine/src/commands/output_run.rs:186`). Loose copies are also written (`crates/reda-engine/src/commands/output_run.rs:232-237`) |
| File names | English, no special characters (W1) | Documents nothing | `<project>` and job name used as is (`crates/reda-output/src/plot.rs:50-54`, `crates/reda-engine/src/commands/output_run.rs:186`) |
| Assembly files in the zip | BOM and CPL listed as separate uploads, after the Gerbers [J4] [J3b]. Nothing says what happens if they are inside the zip | Gerber, BOM and centroid listed as separate uploads [P7] | All four outputs in one zip by default (`Outputs::ALL`, `crates/reda-model/src/output.rs:155-160`, `crates/reda-engine/src/commands/output_run.rs:173-176`); BOM and CPL can be turned off per job (`crates/reda-output/tests/jobs.rs:33`, `:44`) |
| Other files mentioned | Drill map recommended [J2]; IPC-D-356, README, fab notes optional [J5] | Assembly drawing if silkscreen is unclear [P1] [P6] [P5] | None in v0.1 (`docs/spec/v0.1/outputs.md:15`) |

If a zip holds both PCB design files and Gerbers, JLCPCB uses the Gerbers [J8].

## Excellon

| Topic | JLCPCB | PCBWay | REDA |
|---|---|---|---|
| Format | Excellon [J6] (W1, W2). Mm, absolute, decimal format recommended (W1). [J9] describes the Excellon2 style without a decimal point, and how digit format, zero suppression and units are chosen in its DFM tool | Drill files required [P4]; format not stated on the pages read | `M48`, `FMAT,2`, `METRIC`, `G90`, decimal point, absolute (`crates/reda-output/src/drill.rs:153-168`, `:119-128`) |
| PTH and NPTH | **Pages disagree.** W2: one combined `.XLN`. W1: leave them as separate `-PTH.drl` and `-NPTH.drl`. [J5]: one drill file marks plating. [J2]: multiple drill files can raise a merge warning that can generally be ignored | Per-tool table shows `NPTH.drl, PTH.drl` in one row, single `drl/txt` or `.TXT` in others [P3] | Always two files, even when one is empty: `<project>-PTH.drl`, `<project>-NPTH.drl` (`crates/reda-output/src/drill.rs:18-38`) |
| Extension | `.XLN` suggested; `.TXT` and an older `.tap` name recognised [J2]; `.drl` accepted (W1) | `.drl`, `.txt`, `.TXT` [P3] | `.drl` |
| Non-plated holes | Min 0.50 mm; capabilities page asks to draw them on the mechanical or keep-out layer [J1], while W1 uses an NPTH drill file | No separate statement | In the NPTH file (`crates/reda-output/src/drill.rs:88-93`) |
| Plated slots | Min width 0.5 mm (2-layer), 0.35 mm (multilayer), drawn with a pad, length at least 2x width [J1]. W2 gives 0.65 mm minimum, merged into the outline file, with a remark on the order | Min width 0.5 mm, length to width at least 2 [P2], Advanced PCB table | Routed in the PTH file: `G00`, `M15`, `G01`, `M16`, `G05` with a tool as wide as the slot (`crates/reda-output/src/drill.rs:95-109`, `:174-178`) |
| Non-plated slots | Min width 1.0 mm, outline drawn in `.GM1` or `.GKO` [J1]; W2 also merges into `.GKO` | Min width 0.8 mm [P2]. One tool row uses a separate `.rou` file [P3]; no generic rule | Routed in the NPTH file. Not in the `.GKO`, which holds only the Board Outline and Board Cutouts (`crates/reda-output/src/plot.rs:283-305`) |
| Slot tolerance | Plated +0.13/-0.08 mm; non-plated ±0.2 mm [J1] | Documents nothing for slots | Not modelled |
| X2 in drill files | Documents nothing | Documents nothing | `; #@! TF.` and `; #@! TA.` comments when X2 is on (`crates/reda-output/src/drill.rs:154-165`) |

## BOM

| Topic | JLCPCB | PCBWay | REDA preset output |
|---|---|---|---|
| File type | `.csv`, `.xls`, `.xlsx` [J3a] | `.xls`, `.xlsx`, `.csv` [P1]; PDF not accepted [P5] [P10] | CSV, `\n` line ends (`crates/reda-output/src/assembly.rs:235-247`) |
| Required fields | Comment, Designator, Footprint [J3a]; plus JLCPCB or LCSC part number (W3). **Pages disagree:** [J4] requires Designator, Comment, Footprint, MPN; [J6] requires MPN, Designator, Quantity | Page: `Line#`, `Quantity Per Part Number`, `Reference Designator`, `Part Number`, `Part Description`, `Package`, `Type`; turnkey adds `Manufacturers Name`, `Manufacturers Part Number`, `Distributors Part Number` [P1]. Other lists: MPN, Designator, Quantity, Footprint [P10]; Designators, quantity, package, part numbers [P5] | JLCPCB: `Comment,Designator,Footprint,LCSC Part #`. PCBWay: `Line #,Quantity,Reference Designator,Part Number,Manufacturer,Description,Package,Type` (`crates/reda-output/src/presets.rs:92-97`, `:125-134`, `crates/reda-output/tests/golden/assembly/`) |
| Sample headers | `Comment`, `Designator`, `Footprint`, `JLCPCB Part #（optional）` [J3c] | `Item #`, `*Designator`, `*Qty`, `Manufacturer`, `*Mfg Part #`, `Description / Value`, `*Package/Footprint`, `Type`, `Your Instructions / Notes` [P11] | As above |
| Part-number spelling | `LCSC Part #` and `JLCPCB Part #` both appear in W3; sample has `JLCPCB Part #（optional）` [J3c] | Either manufacturer or distributor part number is acceptable [P12] | `LCSC Part #`, from the `LCSC` field |
| Designator list | No rule stated [J3a]. Sample and [J4] use `R5,R6`; [J4] says comma lists, not ranges. Letter case must not distinguish parts, as all are uppercased [J3a] | Sample uses `U2,U4` and `R16, R31` [P11]. No rule stated. Same part number on one row [P12] | `R1, R2` in natural order, CSV-quoted (`crates/reda-output/src/assembly.rs:37-41`, `:251-257`) |
| Grouping | Documents nothing | Same part number on one row [P12]; Designator count must match quantity [P12] [P13] | Same Footprint and fields (`crates/reda-output/src/assembly.rs:153-156`) |
| `Type` values | No such column | Page: Surface mount, Thru-hole or Hybrid [P1]. Sample: `SMD`, `DNS` [P11]. PTH or SMD labelling recommended [P5] | `SMT` or `THT`; THT when any Pad is plated through-hole (`crates/reda-output/src/assembly.rs:163-171`) |
| Not-fitted parts | The BOM and CPL pages document nothing [J3a] [J3]. [J10] says the order flow marks unselected parts DNP. Only selected or matched parts are placed [J3b] [J11] | May be marked DNP or DNS [P12]; note goes in Instructions/Notes [P10]; sample shows `DNS` in `Type` with the part still listed [P11]. Assembly follows the BOM; check whether listed Designators should not be soldered [P13] [P14] | Not Fitted and Mechanical Parts left out of the BOM and CPL entirely (`crates/reda-output/src/assembly.rs:198-214`). No mark is written |

## CPL

| Topic | JLCPCB | PCBWay | REDA preset output |
|---|---|---|---|
| Name | CPL, centroid, pick and place [J3] | Centroid, Insertion, Pick-N-Place, XY Data [P1] | `<project>-CPL.csv` |
| File type | `.csv`, `.xls`, `.xlsx` [J3] | Excel, `.txt`, `.pos` [P6]; `.csv` in W5 | CSV |
| Columns | Fields Designator, Mid X/Mid Y, Layer, Rotation [J3]; sample: `Designator`, `Mid X`, `Mid Y`, `Layer`, `Rotation` [J3d] | Designator, X, Y, rotation, side named without exact headers [P1]. Sample: `Designator`, `Footprint`, `Mid X`, `Mid Y`, `Ref X`, `Ref Y`, `Pad X`, `Pad Y`, `TB`, `Rotation`, `Comment` [P15] | JLCPCB: `Designator,Mid X,Mid Y,Layer,Rotation`. PCBWay: `Designator,X,Y,Rotation,Side` (`crates/reda-output/src/presets.rs:98-104`, `:135-141`) |
| Side values | **Pages disagree.** `Top` / `Bottom` [J3]; `T` / `B` in sample [J3d] and table [J4] | `Top or Bottom` [P1]; `T` in sample, no bottom row [P15]; component side and solder side (W4). Top and bottom in separate files [P13], while [P1] and [P15] describe one file with a side field | `Top` / `Bottom` (`crates/reda-output/src/assembly.rs:75-78`), one file |
| Units | Mm [J3]; sample values carry an `mm` suffix [J3d] [J4] | **Pages disagree.** Mm [P6]; mil suffix in sample [P15]; inches (W4) | Mm, six decimals, no suffix (`crates/reda-output/src/assembly.rs:73-74`, `crates/reda-output/src/gerber.rs:107-111`) |
| Position | Component centroid [J3] | Component centre (W4) | The Footprint's own position, not a computed centroid (`crates/reda-output/src/assembly.rs:66`, `:73-74`) |
| Origin | Common origin for Gerber and CPL; design-space origin or lower-left of the outline [J4]. [J3] documents nothing | Lower-left of the board viewed from the top, on both sides (W4). Other pages document nothing | Placement Origin, else lower-left of the Board Outline bounding box, else board zero (`crates/reda-output/src/assembly.rs:91-116`). Gerber and drill stay in board coordinates (`crates/reda-output/src/gerber.rs:246-249`, `crates/reda-output/src/drill.rs:137-139`) |
| Bottom-side coordinates | Documents nothing [J3] [J4] | Top view, no mirroring mentioned (W4) | X not mirrored (`crates/reda-output/tests/assembly.rs:341-347`) |
| Rotation | Degrees, positive counter-clockwise [J3]. Zero reference not defined [J3]; a library's zero may not match the machine's expectation [J4] | 0 to 360, referenced from the top view for both sides; sign and zero not defined (W4). Parts should be drawn in one consistent orientation [P16] | Degrees in `[0, 360)`, three decimals, counter-clockwise (`crates/reda-output/src/assembly.rs:79`, `crates/reda-output/src/gerber.rs:126-129`) from the Footprint's zero, stored value unchanged |
| Bottom-side rotation | Documents nothing | Top view (W4) | The Footprint frame mirrors left-right, then turns counter-clockwise (`crates/reda-pcb/src/placed.rs:3-5`, `:50-56`). Flipping a Footprint at 30° gives 330.000 (`crates/reda-output/tests/assembly.rs:340-347`) |
| Which parts | SMT and through-hole both placed [J3] | SMT only [P1] [P17]; THT Designators may be left out [P5] | Every fitted part, THT included (`crates/reda-output/tests/assembly.rs:393-400`) |
| BOM link | Designators must match between files (W3) [J4] | Designators must match [P5] [P12] [P13] | Both files read the same fitted set (`crates/reda-output/src/assembly.rs:200-220`) |

## Board parameters and design rules

### Board parameters the fabs ask for

No Fab Preset holds any of these (`crates/reda-model/src/output.rs:81-91`).

| Parameter | JLCPCB | PCBWay |
|---|---|---|
| Layers | 1 to 32 [J1] | 1 to 14 Standard [P2]; order form 1 to 14 [P18] |
| Thickness | 0.4 to 4.5 mm; FR-4 options 0.4, 0.6, 0.8, 1.0, 1.2, 1.6, 2.0 mm; FAQ on the same page gives 0.4 to 3.2 mm [J1] | 0.2 to 3.2 mm: 0.2, 0.4, 0.6, 0.8, 1.0, 1.2, 1.6, 2.0, 2.4, 2.6, 2.8, 3.0, 3.2 [P2] |
| Finish | HASL (leaded, lead-free), ENIG, OSP, with exclusions by board type [J1] | HASL with lead, HASL lead free, Immersion Gold, OSP, Hard Gold, Immersion Silver, Immersion Tin, ENEPIG [P2]; selective combinations on the form [P18] |
| Mask colour | Green, Purple, Red, Yellow, Blue, White, Black [J1] | Green, Red, Yellow, Blue, White, Black, Matt green, Matte black, Purple, none [P2]; Transparent and Translucent also on the form [P18] |
| Silkscreen colour | Not on the capabilities page [J1] | White, Black, Yellow, None [P2] |
| Outer copper | 2-layer 1, 2, 2.5, 3.5, 4.5 oz; multilayer 1, 2 oz [J1] | 1 to 8 oz [P2]; form lists higher values [P18] |
| Other fields on the form | Base material, dimensions, quantity, delivery format, mark on PCB, electrical test, gold fingers, castellated holes, remark [J12] | Min track/spacing, min hole size, via process [P18] |

[J12] shows options that change with other selections, so its option lists are not used above.

### Minimum design rules

| Limit | JLCPCB | PCBWay |
|---|---|---|
| Track / clearance | 1 oz: 1 to 2 layers 0.10/0.10 mm; multilayer 0.09/0.09 mm. 2 oz: 0.16/0.16 mm on 2 layers [J1] | **Pages disagree.** 0.1 mm / 4 mil [P2] Standard; 3 mil minimum, 6 mil recommended [P19]; form offers 3/3 to 8/8 mil [P18]. Advanced outer 1 oz: 5/6 mil normal, 5/5 medium, below 4/4 high difficulty [P2] |
| Drill | 0.15 to 6.3 mm on 2 layers; 0.1 mm only at 1 mm or thinner with ENIG/OSP [J1] | 0.15 to 6.0 mm; under 0.2 mm extra charge [P2]. Advanced 0.20 to 6.5 mm [P2] |
| Via | 0.15/0.25 mm hole/diameter; diameter 0.1 mm larger than hole, 0.15 mm preferred; preferred min hole 0.2 mm; 0.15 mm costs more [J1] | No separate Via diameter |
| Annular ring | Headline ≥0.20 mm. 2-layer 1 oz: 0.25 recommended, 0.18 absolute minimum. Multilayer 1 oz: 0.20 recommended, 0.15 absolute. 2 oz: 0.254 [J1]. NPTH pad 0.45 recommended [J1] | 0.15 mm / 6 mil [P2] |
| Non-plated hole | 0.50 mm [J1] | Finished hole 0.15 to 6.0 mm; no separate minimum [P2] |
| Hole to hole | Via 0.2 mm; pad 0.45 mm [J1] | Advanced only: component hole ≥16 mil normal, 14 to 16 medium, 13 to 14 high; Via up to 0.45 mm ≥11 mil [P2]. Standard table: nothing |
| Copper to edge | Routed ≥0.2 mm; V-cut ≥0.4 mm [J1] | Advanced only, CNC milling: 0.25 mm normal, 0.20 mm medium [P2]. Standard table: nothing |
| Hole to edge | Castellated only [J1] | ≥0.5 mm; V-score panels 1.0 mm [P20] (2019 page) |
| Mask expansion | 1:1; keep 0.09 mm from openings to neighbouring traces [J1] | Not on the capabilities page [P2] |
| Mask bridge | 1 oz: 0.10 mm (green, red, yellow, blue, purple), 0.13 mm (black, white); 2 oz: 0.20 mm [J1] | **Pages disagree.** Advanced: under 2 oz 4 mil green, ≥4.5 mil other; ≥2 oz 5 mil normal, 4 mil medium [P2]. IC pad spacing: ≥0.2 mm green, ≥0.22 mm other colours, 1 to 3 oz [P21] |
| Mask opening | See expansion | Advanced: ≥2 mil normal, 1.5 medium, 1 high (BGA only) [P2] |
| Silkscreen | Line ≥0.15 mm; text height ≥1.0 mm; ratio 1:6; pad to silkscreen 0.15 mm [J1] | Character width 0.15 mm; height 0.8 mm; ratio 1:5 [P2] |
| Slots | See [Excellon](#excellon) | See [Excellon](#excellon) |

## REDA presets versus findings

### Rules

| Rule (`crates/reda-model/src/pcb.rs:100-115`) | JLCPCB preset (`crates/reda-output/src/presets.rs:78-90`) | JLCPCB published [J1] | PCBWay preset (`crates/reda-output/src/presets.rs:114-123`) | PCBWay published [P2] |
|---|---|---|---|---|
| `clearance` | 0.10 mm | 0.10 mm, same | 0.10 mm | 0.1 mm Standard; 3 mil on [P19] |
| `track_width` | 0.10 mm | 0.10 mm, same | 0.10 mm | 0.1 mm Standard; 3 mil on [P19] |
| `via_hole` | 0.15 mm | 0.15 mm, same | 0.15 mm | 0.15 mm, same; under 0.2 mm costs extra |
| `via_diameter` | 0.51 mm | 0.25 mm; preset larger | 0.45 mm | Not published as such; preset is drill plus two rings |
| `annular_ring` | 0.18 mm | 0.18 mm absolute minimum (2-layer 1 oz); below the ≥0.20 headline | 0.15 mm | 0.15 mm, same |
| `hole_to_hole` | 0.45 mm | Pad 0.45 mm, same; Via 0.2 mm, preset larger | 0.50 mm | Advanced: ≥16 mil (0.406 mm) component, ≥11 mil (0.279 mm) Via; preset larger |
| `edge_clearance` | 0.20 mm | Routed 0.2 mm, same; V-cut 0.4 mm, preset smaller | 0.40 mm | Advanced: 0.25 mm normal; preset larger |

A board rule below the preset's value warns at run time (`crates/reda-engine/src/commands/output_run/checks.rs:160-172`). So a JLCPCB-preset board with a 0.25 mm Via diameter warns, although JLCPCB publishes 0.25 mm. The preset comment gives the reason for 0.51 mm: a 0.15 mm drill plus the 0.18 mm ring the preset's `annular_ring` asks of every hole (`crates/reda-output/src/presets.rs:81-83`). The test pins only track, clearance and Via hole for JLCPCB (`crates/reda-engine/tests/fab_presets.rs:33-36`).

### Each difference

Applies to both presets:

1. X2 is on (`crates/reda-output/src/presets.rs:27-30`). JLCPCB pages disagree on X2 (W1, W2); PCBWay recommends RS-274-X and does not mention X2 [P8].
2. The default zip includes the BOM and CPL. Both fabs list them as separate uploads [J4] [P7].
3. CPL positions are measured from the CPL origin while Gerber and drill stay in board coordinates (`crates/reda-output/src/assembly.rs:61`, `:73-74`, `crates/reda-output/src/gerber.rs:246-249`, `crates/reda-output/src/drill.rs:137-139`). The two match only when the origin is board zero. JLCPCB asks for a common origin [J4]; PCBWay (W4) asks for the board's lower-left corner, which REDA's default origin is.
4. Routed non-plated slots are in the NPTH file, not the `.GKO` (`crates/reda-output/src/drill.rs:174-178`, `crates/reda-output/src/plot.rs:283-305`). JLCPCB's capabilities page asks for `.GM1` or `.GKO` [J1].
5. Not Fitted parts are absent from the BOM. PCBWay's sample lists them with a `DNS` mark [P11]. JLCPCB's order flow marks unselected parts itself [J10].
6. Designators use comma plus space (`crates/reda-output/src/assembly.rs:37-41`). JLCPCB's sample uses a comma alone [J3c]. PCBWay's sample has both [P11].
7. No preset holds mask, silkscreen, slot, non-plated-hole or board-parameter values (`crates/reda-model/src/pcb.rs:100-115`, `crates/reda-model/src/output.rs:81-91`).
8. Paste files are always written, also on bare-board jobs. JLCPCB says this is harmless (W1).

JLCPCB preset:

9. Header `LCSC Part #` versus `JLCPCB Part #（optional）` in the sample [J3c]. W3 uses both spellings.
10. No Quantity or MPN column. [J3a] does not require them; [J4] requires MPN and [J6] requires MPN and Quantity.
11. `Layer` values `Top`/`Bottom` match [J3] but not the `T`/`B` sample [J3d].
12. `Mid X` and `Mid Y` carry no `mm` suffix; the sample has one [J3d]. [J3] gives the unit in the column description.
13. `.drl` extension versus suggested `.XLN` [J2]; W1 accepts `.drl`. Separate PTH and NPTH files versus W2's single file.
14. Paste extensions `.GTP`, `.GBP` are not in the suggested table [J2].
15. `edge_clearance` 0.20 mm is the routed value; V-cut is 0.40 mm [J1]. The preset comment says so (`crates/reda-output/src/presets.rs:88`).
16. `annular_ring` 0.18 mm is the 2-layer 1 oz absolute minimum, below the page's ≥0.20 headline and 0.25 recommended value [J1].
17. Spec line `docs/spec/v0.1/outputs.md:71` says the preset follows JLCPCB's BOM template; the template's fourth header differs (item 9).

PCBWay preset:

18. BOM headers versus page [P1] and sample [P11]:

    | Preset (`crates/reda-output/src/presets.rs:125-134`) | Page [P1] | Sample [P11] |
    |---|---|---|
    | `Line #` | `Line#` | `Item #` |
    | `Quantity` | `Quantity Per Part Number` | `*Qty` |
    | `Reference Designator` | `Reference Designator` | `*Designator` |
    | `Part Number` (from `MPN`) | `Part Number`; turnkey also `Manufacturers Part Number` | `*Mfg Part #` |
    | `Manufacturer` | `Manufacturers Name` (turnkey) | `Manufacturer` |
    | `Description` (from value) | `Part Description` | `Description / Value` |
    | `Package` (Footprint name) | `Package` | `*Package/Footprint` |
    | `Type` (`SMT`, `THT`) | `Type` (Surface mount, Thru-hole, Hybrid) | `Type` (`SMD`, `DNS`) |
    | Not present | `Distributors Part Number` (turnkey) | `Your Instructions / Notes` |

19. CPL headers `X`, `Y`, `Side` versus sample `Mid X`, `Mid Y`, `TB` [P15]. The preset writes `Top`/`Bottom`; the page says the same [P1] and the sample shows `T`.
20. THT parts are in the CPL; PCBWay lists SMT only, and THT may be left out [P1] [P5].
21. Mm units match [P6] but not the sample's mil [P15] or W4's inches.
22. One CPL file versus separate top and bottom files on [P13].
23. The comment at `crates/reda-output/src/presets.rs:108-109` says PCBWay publishes no hole spacing or edge clearance. Its Advanced PCB table publishes both [P2]; the Standard table has neither. `hole_to_hole` and `edge_clearance` keep the Generic values (0.50, 0.40 mm).
24. [P19] gives 3 mil track and clearance, versus 0.1 mm on [P2]. The preset uses 0.1 mm.

Generic preset (`crates/reda-output/src/presets.rs:40-71`):

25. Rule values are at or above both fabs' published minimums used above. Its CPL headers match JLCPCB's. Its BOM (`Designator,Quantity,Value,Footprint,Manufacturer,MPN`) matches neither fab's required headers.

### Spec versus code

No contradiction on an emitted value. Smaller gaps:

- `docs/spec/v0.1/outputs.md:9` lists silkscreen files without a condition; the code writes them only when the layer exists (`crates/reda-output/src/plot.rs:85`).
- `docs/spec/v0.1/outputs.md:69` groups by Part, Footprint and value fields; the code compares the Footprint and all instance fields (`crates/reda-output/src/assembly.rs:153-156`), which the comment there says hold the Part's own.
- `docs/spec/v0.1/outputs.md:78` says rotation uses the IPC-7351 zero orientation; the writer outputs the stored rotation and cannot check what zero the Footprint was drawn in (`crates/reda-output/src/assembly.rs:79`).
- `docs/spec/v0.1/outputs.md:72` omits one more exclusion: a Footprint with no Component is also left out (`crates/reda-output/src/assembly.rs:205-209`).

`crates/reda-engine/src/commands/fab_preset.rs` only copies and compares presets; it holds no fab values.

## Not documented or not verified

**Fab documents nothing, on the pages read:**

- JLCPCB: bottom-side coordinate or rotation rules; rotation zero reference; Designator separator as a stated rule; whether assembly files inside the Gerber zip are accepted; whether X2 attributes cause a problem in practice beyond the conflicting pages; a `Quantity` rule in its help article [J3a].
- PCBWay: generic statement on X2; drill format; whether PTH and NPTH must be split; generic slot layer; archive type; a required Designator separator; rotation sign and zero; a required header spelling; `DNS` definition in the sample [P11]; hole-to-hole and copper-to-edge on the Standard table [P2].

**Could not verify:**

- A search index snippet attributed to JLCPCB said to remove unwanted parts from the BOM or mark them `DNP` in a `Customer Supply` column. The page it seemed to come from [J11] does not say so, and neither do the other JLCPCB assembly pages searched in raw text. Treated as unverified.
- Search snippets gave JLCPCB silkscreen text height of 0.8 mm on a blog and a note that rotation values are only a reference. Neither page was read, so neither is used.
- W5's settings appear only in screenshots, so its Gerber and drill settings were not read.
- Whether each fab's upload tool accepts REDA's actual files. Nothing was uploaded, by design.
- Which of PCBWay's Standard and Advanced tables applies to a given order. The tiers are reported as labelled on the page.
- Page text was mostly read through a summarising fetch. The capabilities pages, the blog origin passage, the X2 passages of W1 and W2 and the PTH/NPTH passage of W1 were checked against raw page text; the sample files were read directly. W2's combined `.XLN` and slot statements and [J3]'s fields were confirmed only by a second summarising fetch.
- REDA references are to `origin/main` at 516cba3. The compared paths do not differ from the `v0.1` branch.

## Sources

Fetched 2026-11-04. Dates are the pages' own "last updated" where shown.

JLCPCB:

- [J1]: Rigid PCB capabilities.
- [J2]: Suggested naming patterns, 2026-09-09.
- [J3]: Pick and place file, 2026-09-09.
- [J3a]: BOM file, 2026-09-09.
- [J3b]: PCB assembly FAQs.
- [J3c]: Sample BOM, linked from [J3a], hosted off the fab's domain.
- [J3d]: Sample CPL, linked from [J3], hosted off the fab's domain.
- [J4]: Files needed for assembly, blog, 2026-02-05.
- [J5]: Preparing Gerber files, blog, 2026-06-27.
- [J6]: PCB file formats, blog, 2026-09-09.
- [J8]: Ordering guide, 2026-09-09.
- [J9]: DFM drill layer format, 2025-04-24.
- [J10]: Assembly workflow, blog.
- [J11]: Using your own parts, 2026-09-09.
- [J12]: Quote form.

PCBWay:

- [P1]: Assembly file requirements.
- [P2]: PCB capabilities.
- [P3]: Gerber file extensions per tool, 2024-09-05.
- [P4]: Gerber files not complete, 2025-03-13.
- [P5]: Files and info for assembly orders.
- [P6]: Files requested for assembly, 2020-05-12.
- [P7]: Uploading an assembly inquiry, 2022-02-25.
- [P8]: Non-Gerber uploads, 2024-05-31.
- [P9]: Several Gerber sets, 2024-03-12.
- [P10]: PCBA ordering Q&A, 2020-09-24.
- [P11]: Sample BOM.
- [P12]: Factors affecting assembly, 2020-05-12.
- [P13]: Assembly file problems, 2020-05-14.
- [P14]: Parts in the centroid but not the BOM, 2020-05-14.
- [P15]: Sample centroid file.
- [P16]: Assembly process.
- [P17]: Assembly FAQ.
- [P18]: Order form.
- [P19]: Design rule check.
- [P20]: Hole to board edge, 2019-05-28.
- [P21]: Soldermask bridge, 2024-03-12.

Fab pages whose URLs name an EDA suite. The URLs are left out of this file and were given separately to the ticket owner:

- **W1**: JLCPCB's Gerber and drill export guide for one suite, 2026-09-24.
- **W2**: JLCPCB's Gerber and drill export guide for another suite, 2026-09-09.
- **W3**: JLCPCB's BOM and CPL export guide for one suite, 2026-09-09.
- **W4**: PCBWay's centroid file guide covering several suites, 2024-02-26. Its conventions were stated for centroid files generally.
- **W5**: PCBWay's Gerber, BOM and placement export guide for one suite, 2025-09-11.

[J1]: https://jlcpcb.com/capabilities/pcb-capabilities
[J2]: https://jlcpcb.com/help/article/suggested-naming-patterns
[J3]: https://jlcpcb.com/help/article/pick-place-file-for-pcb-assembly
[J3a]: https://jlcpcb.com/help/article/bill-of-materials-for-pcb-assembly
[J3b]: https://jlcpcb.com/help/article/pcb-assembly-faqs
[J3c]: https://s3.amazonaws.com/helpscout.net/docs/assets/59f1de7804286313cffbb22c/attachments/60ee4acd9e87cb3d0124d284/Sample-BOM_JLCSMT.xlsx
[J3d]: https://s3.amazonaws.com/helpscout.net/docs/assets/59f1de7804286313cffbb22c/attachments/5d83250e04286364bc8f4c5e/Sample-CPL_JLCSMT.xlsx
[J4]: https://jlcpcb.com/blog/files-needed-for-pcb-assembly
[J5]: https://jlcpcb.com/blog/prepare-perfect-gerber-files
[J6]: https://jlcpcb.com/blog/pcb-file-formats
[J8]: https://jlcpcb.com/help/article/instructions-for-ordering
[J9]: https://jlcpcb.com/help/article/dfm-drill-layer-format-adjustment
[J10]: https://jlcpcb.com/blog/pcb-assembly-workflow-beginners
[J11]: https://jlcpcb.com/help/article/how-to-use-my-own-parts-for-pcb-assembly-order
[J12]: https://cart.jlcpcb.com/quote
[P1]: https://www.pcbway.com/assembly-file-requirements.html
[P2]: https://www.pcbway.com/capabilities.html
[P3]: https://www.pcbway.com/helpcenter/generate_gerber/Gerber_File_Extension_from_Different_Software.html
[P4]: https://www.pcbway.com/helpcenter/file_issues/Gerber_files_are_not_completed.html
[P5]: https://www.pcbway.com/smt_ordering_guide.html
[P6]: https://www.pcbway.com/helpcenter/pcb_assembly_ordering/What_files_are_requested_for_assembly_production_.html
[P7]: https://www.pcbway.com/helpcenter/pcb_assembly_ordering/How_to_Upload_PCB_Assembly_PCBA__Inquiry.html
[P8]: https://www.pcbway.com/helpcenter/shoppingcart/Can_I_upload_file_information_to_upload_non_gerber_format_.html
[P9]: https://www.pcbway.com/helpcenter/file_issues/Several_gerber_files_have_in_your_order.html
[P10]: https://www.pcbway.com/blog/PCB_Basic_Information/PCBWay_Q_A_003___Common_Questions_for_PCBA_Ordering_01.html
[P11]: https://www.pcbway.com/img/images/pcbway/Sample_BOM_PCBWay.xlsx?v=1.1
[P12]: https://www.pcbway.com/helpcenter/pcb_assembly_ordering/Common_factors_that_affect_the_efficiency_of_Assembly_service_.html
[P13]: https://www.pcbway.com/helpcenter/Assembly_files_problems/Top_13_suggestions_for_solving_PCB_Assembly_files_problems.html
[P14]: https://www.pcbway.com/helpcenter/SMT_Engineering_Questions/How_to_avoid_the_problem_that_missing_to_solder_the_parts_on_centroid_file_but_not_show_on_BOM_.html
[P15]: https://www.pcbway.com/img/images/pcbway/a-sample-of-PCBWay-Centroid-File.txt
[P16]: https://www.pcbway.com/assembly-process.html
[P17]: https://www.pcbway.com/assembly-faq.html
[P18]: https://www.pcbway.com/orderonline.aspx
[P19]: https://www.pcbway.com/pcb_prototype/PCB_Design_Rule_Check.html
[P20]: https://www.pcbway.com/helpcenter/Engineering_Questions/the_spacing_requirement_from_hole_to_the_edge_of_board.html
[P21]: https://www.pcbway.com/helpcenter/soldermask_issues/Soldermask_bridge.html
