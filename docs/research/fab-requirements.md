# What fabs require in submitted manufacturing files

Research for issue #4. Researched 2026-10-31 against fab help and capability pages and the Ucamco specifications. Fab pages change often and sometimes contradict each other. Where they do, this note records the contradiction instead of picking a side.

## Answer in brief

- **Gerber dialect:** every fab surveyed accepts Extended Gerber (RS-274X). Gerber X2 is RS-274X plus the `TF/TA/TO/TD` attribute commands. Aisler, Eurocircuits and OSH Park accept or prefer it, and so does JLCPCB according to its current KiCad guide. Attributes do not change the image, so an X1-only reader still renders an X2 file correctly. The spec has revoked Standard Gerber (RS-274-D). Of the fabs surveyed, only Eurocircuits actively uses Gerber X3 (component layers).
- **Drill:** every fab accepts Excellon. Ucamco recommends Gerber for drill data, with XNC (a strict subset of Excellon / IPC-NC-349) as the NC fallback. Write PTH and NPTH as separate files, in metric units, with decimal coordinates (no zero suppression) and an absolute origin that matches the Gerbers.
- **Layer naming:** there is no standard. JLCPCB prefers Protel extensions (`.GTL`, `.GBL`, ...). OSH Park, Aisler and PCBWay recognise KiCad file names, the X2 `.FileFunction` attribute, or both. Protel extensions combined with X2 attributes work at every fab surveyed.
- **Assembly:** the JLCPCB BOM needs Comment, Designator and Footprint. The JLCPCB CPL needs Designator, Mid X, Mid Y, Layer (Top/Bottom) and Rotation, in mm and degrees counter-clockwise. PCBWay wants more BOM fields: quantity, manufacturer, MPN, package and SMT/THT type. For assembly, Aisler and Eurocircuits prefer native KiCad files, ODB++ or Gerber X3.
- **Panelization:** the fab usually does the panelizing. JLCPCB publishes rules for customer-made panels, and Aisler panelizes assembly orders itself. REDA can leave panelization for later.

## Common denominator REDA should target

**Default output set (the "generic" preset):**

1. Gerber X2 (RS-274X plus attributes) with `%MOMM*%` and `%FSLAX46Y46*%`. The spec now requires mm and 6 decimal places. Write one file per layer, use Protel-style extensions and put `.FileFunction` on every file. Each preset gets an "X1 / no attributes" toggle for fabs whose guides still say X2 is not supported.
2. Layers: top and bottom copper, top and bottom mask, top and bottom silkscreen, top and bottom paste (optional), and the board outline (profile). The outline layer contains nothing but the outline.
3. Drill: Excellon, written to the XNC subset:
   - structure: `M48`, `METRIC`, a tool table of `Tnn C<diameter>` lines, `%`, decimal coordinates, then `M30`
   - PTH and NPTH in separate files
   - XNC attribute comments: `; #@! TF.FileFunction,Plated,1,2,PTH` and `; #@! TF.FileFunction,NonPlated,1,2,NPTH`
   - slots as routs: `G00` to the start point, `M15`, `G01`/`G02`/`G03`, then `M16`
4. Everything in one `.zip` with ASCII file names.
5. Assembly files, both CSV:
   - BOM columns: Designator, Quantity, Value/Comment, Footprint, Manufacturer, MPN, and a per-fab part-number column such as LCSC
   - CPL columns: Designator, Mid X, Mid Y, Layer, Rotation, in mm, with rotation in degrees counter-clockwise and coordinates as seen from the top

**Default 2-layer DRC (the strictest standard-service value across JLCPCB, PCBWay, OSH Park and Aisler):**

| Rule | Default | Driven by |
|---|---|---|
| Min track width | 0.20 mm | Aisler (8 mil) |
| Min copper-to-copper clearance | 0.16 mm | OSH Park (6 mil); Aisler 0.15 |
| Min via drill | 0.30 mm | Aisler |
| Min via annular ring | 0.20 mm | Aisler (JLCPCB 0.18 absolute) |
| Min PTH drill | 0.50 mm | Aisler; JLCPCB recommends >= 0.5 |
| Min PTH annular ring | 0.30 mm | Aisler (JLCPCB recommends 0.25) |
| Min NPTH drill | 0.50 mm | JLCPCB, Aisler |
| Hole-to-hole spacing | 0.50 mm | JLCPCB pad holes 0.45 |
| Copper to board edge | 0.40 mm | OSH Park 0.381; JLCPCB 0.4 at V-cut edges |
| Silkscreen line width | 0.15 mm | JLCPCB, PCBWay, Aisler |
| Silkscreen text height | 1.0 mm | JLCPCB |
| Silkscreen-to-pad clearance | 0.15 mm | JLCPCB |
| Solder mask web (dam) | 0.10 mm (0.13 for black/white) | JLCPCB, Aisler |
| Min board dimension | > 10 mm | Aisler |

**Per-fab presets** relax these defaults to each fab's published minimum. For example, JLCPCB allows 0.10/0.10 mm track/space and a 0.15 mm drill. OSH Park allows 0.1524/0.1524 mm and a 0.254 mm drill.

## Specifications

### Gerber (Ucamco Gerber Layer Format Specification, rev. 2024.05)

Source: <https://www.ucamco.com/en/gerber/downloads>. The rev. 2024.05 PDF was read. Rev. 2026.05 is now listed there too.

- Files that contain attribute commands (TF, TA, TO, TD) are Gerber X2, and files without them are X1. "A Gerber reader will generate the correct image even if it ignores some or all attributes." (attributes overview)
- Standard Gerber (RS-274-D) "is revoked and therefore non-conforming". It was revoked in revision I1, December 2012 (sect. 8.5).
- `MO`: "Use metric. Imperial is historic and will be deprecated at a future date."
- `FS`: the number of decimal digits "must now be set to 6". The integer digits range from 1 to 6, and 3 is typical (sect. 4.2).
- `.FileFunction` values (sect. 5.6.3) include:
  - `Copper,Ln,(Top|Inr|Bot)`
  - `Soldermask,Top|Bot`, `Legend,Top|Bot`, `Paste,Top|Bot`
  - `Profile,NP`
  - `Plated,i,j,PTH` and `NonPlated,i,j,NPTH`

  `.FileFunction` and `.FilePolarity` are the essential attributes.
- Drill/rout files (sect. 6.6): "PTH and NPTH must be split into two separate files". The drill image shows the finished diameter, after plating. On format choice the spec says: "Use Gerber rather than an NC format for drill and rout layers ... If for whatever reason you need to output the drill files in an NC format, then use the XNC format."
- X3 (rev. 2020.09) adds component layers (`.FileFunction,Component,L1,Top`). They carry each component's reference designator, location, rotation, MPN and so on. They are separate files, so legacy readers can ignore them.

### XNC (Ucamco, rev. 2021.11)

Source: <https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf>

- The spec calls XNC "A subset of IPC-NC-349, aka Excellon". KiCad's Jean-Pierre Charras is one of its co-authors. Legacy NC readers read XNC files "without a hitch".
- File structure:
  - `M48`
  - `METRIC` or `INCH` (metric recommended)
  - a tool table of `Tnn C<diameter>` lines, with 2-digit tool numbers and finished diameters
  - `%`
  - the body
  - `M30`
- Coordinates are decimal numbers in the file unit, which removes Excellon's ambiguity about where the implied decimal point sits.
- The drill file must use the same coordinate system as the Gerbers: "no offset, rotation, mirroring."
- Attributes are written as standard comments, such as `; #@! TF.FileFunction,Plated,1,4,PTH`. Legacy readers ignore them safely.
- Routs: `G00` to the start point, `M15` (tool down), `G01` for lines or `G02`/`G03` for arcs, then `M16` (tool up).

## Fabs

### JLCPCB

- **Capabilities** (<https://jlcpcb.com/capabilities/pcb-capabilities>), 2-layer:
  - track/space 0.10/0.10 mm (1 oz)
  - min drill 0.15 mm; via 0.15 mm hole / 0.25 mm diameter
  - PTH annular ring 0.25 mm recommended, 0.18 mm minimum
  - NPTH min 0.50 mm
  - hole-to-hole 0.2 mm (vias) / 0.45 mm (pads)
  - copper to edge 0.2 mm (routed) / 0.4 mm (V-cut)
  - silkscreen line 0.15 mm, text 1.0 mm, silkscreen-to-pad 0.15 mm
  - mask dam 0.10 mm (0.13 mm for black/white)
  - board 3x3 mm minimum; thickness 0.4 to 2.0 mm
  - The page gives two different maximum board sizes (670x600 mm and 1020x600 mm).
- **Panelization** (the same page and <https://jlcpcb.com/capabilities/smt-capabilities>):
  - V-cut: zero spacing; panel 70x70 to 475x475 mm; board at least 0.6 mm thick
  - board spacing 2 mm (1.6 or 2 mm with mouse bites)
  - tooling edge 3 mm minimum, 5 mm for SMT
  - breakaway tab 4 mm (5 mm with mouse bites)
  - SMT: 2 mm tooling holes and 1 mm fiducials, centred 3.85 mm from the panel edge
- **Files**: the KiCad 9 guide (<https://jlcpcb.com/help/article/how-to-generate-gerber-and-drill-files-in-kicad-9>) says:
  - Gerbers: Protel extensions, X2 ticked, 4.6 mm coordinate format, netlist attributes ticked
  - drill: Excellon in mm with decimal zeros; KiCad's separate `-PTH.drl` and `-NPTH.drl` files "are accepted as they are"
  - upload: a single .zip or .rar with English file names

  **Contradiction:** JLCPCB's Eagle guide (<https://jlcpcb.com/help/article/how-to-generate-gerber-and-drill-files-in-autodesk-eagle>) says "JLCPCB uses the Gerber format of RS-274X, Gerber X2 is not supported."
- **Naming** (<https://jlcpcb.com/help/article/suggested-naming-patterns>): GTL, GBL, GTS, GBS, GTO, GBO, GKO (or GM1), G2L and G3L, with drills as `.XLN`.
- **BOM** (<https://jlcpcb.com/help/article/bill-of-materials-for-pcb-assembly>): the required columns are Comment, Designator and Footprint. Files can be .csv, .xls or .xlsx. JLCPCB uppercases all designators.
- **CPL** (<https://jlcpcb.com/help/article/pick-place-file-for-pcb-assembly>): Designator, Mid X and Mid Y (mm), Layer (Top/Bottom) and Rotation (degrees, positive is counter-clockwise). Files can be .csv, .xls or .xlsx.

### PCBWay

- **Capabilities** (<https://www.pcbway.com/capabilities.html>), standard service:
  - track/space 0.1/0.1 mm
  - min drill 0.15 mm; annular ring 0.15 mm; PTH 0.15 to 6.0 mm
  - silkscreen line 0.15 mm, text 0.8 mm
  - board 3x3 mm minimum, 600x1200 mm maximum (1-2 layers); thickness 0.2 to 3.2 mm
  - Hole-to-hole, copper-to-edge and mask dam values appear only in the Advanced table.
- **Panelization**: V-scoring (zero spacing), tab-routing, and tab-routing with stamp holes (at least 1.6 mm between boards).
- **Files**:
  - Gerber is preferred, in RS-274X (<https://www.pcbway.com/blog/Engineering_Technical/What_s_the_Gerber_.html>).
  - Drill files are Excellon, and an outline layer is required.
  - Some native project files from commercial EDA tools are also accepted.
  - KiCad file names (`F_Cu.gbr`, ..., `PTH.drl`, `NPTH.drl`) are recognised (<https://www.pcbway.com/helpcenter/generate_gerber/Gerber_File_Extension_from_Different_Software.html>).
- **Assembly** (<https://www.pcbway.com/assembly-file-requirements.html>):
  - BOM: Line#, Quantity, Reference Designator, Part Number, Description, Package, and Type (SMT/THT/hybrid). Turnkey orders add Manufacturer, MPN and Distributor PN.
  - Centroid file: designator, X, Y, rotation and side, for SMT parts only. PCBWay gives no fixed header names.

### OSH Park

- **2-layer service** (<https://docs.oshpark.com/services/two-layer/>):
  - track/space 6/6 mil (0.1524 mm)
  - drill 10 mil (0.254 mm); annular ring 5 mil (0.127 mm)
  - edge keepout 15 mil (0.381 mm)
  - silkscreen 5 mil (3 mil for short features)
  - 1.6 mm board, 1 oz copper, ENIG
- **Files**:
  - Native `.kicad_pcb` upload is preferred, and KiCad file names are detected automatically (<https://docs.oshpark.com/design-tools/kicad/>).
  - The Gerber guide (<https://docs.oshpark.com/design-tools/kicad/generating-kicad-gerbers/>) recommends X2. Required layers are F/B.Cu, F/B.SilkScreen, F/B.Mask and Edge.Cuts, with paste optional.
  - Drill: Excellon with "alternate drill mode" (slots written as routs), absolute origin, mm or inch, decimal format.
- **Panelization**: none. OSH Park panelizes its shared orders itself.

### Aisler

- **2-layer 1.6 mm 35 µm HASL rules** (<https://community.aisler.net/t/2-layer-1-6-mm-35-m-hasl-design-rules/3735>):
  - track 0.20 mm, clearance 0.15 mm
  - via drill 0.3 to 0.45 mm; PTH and NPTH 0.5 to 5.6 mm
  - annular ring 0.3 mm (PTH) / 0.2 mm (via)
  - copper to edge 0.3 mm; drill spacing 0.3 mm
  - silkscreen line 0.15 mm, text 0.8 mm, 0.125 mm to pads
  - mask dam 0.1 mm, mask expansion 0.05 mm
  - board larger than 10 mm, at most 500x500 mm
- **Files**:
  - Aisler accepts native KiCad and other tool files, ODB++, or "traditional Gerber". Assembly is not possible from Gerber (<https://community.aisler.net/t/pcb-design-tool-support-matrix/103>).
  - Traditional Gerbers use Aisler's own names (`.toplayer.ger`, ..., `drills_pth.xln`, `holes_npth.xln`) (<https://community.aisler.net/t/naming-convention-for-traditional-gerber-files/99>).
  - X2 attributes are detected automatically. Excellon without attributes must use "2:4 precision, inches".
- **Panelization** (<https://community.aisler.net/t/aisler-production-panel-specification/2779>): Aisler panelizes assembly orders itself, with mouse-bite bridges, a 12.4 mm frame, and 3 global plus 3 local 1.0 mm fiducials. It does not use fiducials the customer adds.

### Eurocircuits

- **Formats** (<https://www.eurocircuits.com/technical-guidelines/pcb-design-guidelines/input-data-formats/>):
  - Native KiCad and Eagle files are the most preferred.
  - Next come Gerber X3 (with assembly data) or X2+ (PCB data only), and ODB++.
  - Drill: XNC, Excellon 2 or 1, or Sieb&Meyer.
  - Assembly needs a separate BOM.
  - Eurocircuits also asks for no zero-size apertures or tools.
- **DRC numbers**: not collected. Eurocircuits uses pattern and drill classes; collecting them is follow-up work if a preset is wanted.

## Open points

- JLCPCB's guides contradict each other on X2. Uploading an X2 set and an X1 set would settle the question. Keep the X1 toggle until then.
- JLCPCB CPL rotations for parts from its library are often offset from the footprint's zero orientation. Users often report that each part needs a rotation correction, but the pages read here do not document this.
- Gerber spec rev. 2026.05 is listed but was not read. Check it for changes before implementing the writer.
