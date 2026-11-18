# Characteristic impedance formulas for a Stackup readout

[REDA issue 329](https://github.com/openreda/reda/issues/329) · [Stackup issue map](https://github.com/openreda/reda/issues/319)

Researched 2026-11-18. This is the access date for all external sources below. The research was run on the Codex CLI and reviewed by a Claude agent. Every number in the worked-values table was computed in review, with a short script kept outside the repo. The review's own cross-checks are in [Review cross-checks](#review-cross-checks).

## Summary

- **Surface microstrip has the strongest practical comparison.** For bare rectangular copper over one plane, Hammerstad and Jensen account for the fraction of the field in air and for copper thickness. Wheeler’s 1977 formula provides an independent calculation. In a published method-of-moments comparison near 50 Ω, the numerical reference was **50.63 Ω**, the expression attributed to Wadell gave **50.50 Ω**, and the older IPC logarithm gave **49.46 Ω**. For a wider Track on the same substrate, the older logarithm was almost 30% low. [Original comparison, table 1](https://www.polarinstruments.com/support/cits/IPC1999.pdf)
- **The number of reference planes determines the inner-Layer model.** Embedded microstrip has one reference plane and a finite Dielectric cover. Stripline has two reference planes. Cohn gives an exact reference for ideal centered stripline with zero-thickness copper. Wheeler’s 1978 expression includes finite copper thickness. Offset stripline needs both plane distances. On JLCPCB’s four-Layer 7628 build, an L2 signal between L1 and L3 sees **0.2104 mm of prepreg on one side and 1.065 mm of core on the other**, with different Dk values. A centered, single-material calculation does not describe that cross-section. [Cohn](https://doi.org/10.1109/TMTT.1954.1124875) · [Wheeler](https://doi.org/10.1109/TMTT.1978.1129505) · [JLCPCB Stackups and material parameters](https://jlcpcb.com/impedance)
- **Two historical JLCPCB outputs give useful four-Layer reference points.** A saved calculator screen shows **0.3493 mm for 50 Ω** on `JLC04161H-7628`, L1 referenced to L2, single-ended and non-coplanar. A separate hardware project reports **0.1565 mm for 50 Ω** on `JLC04161H-3313`. These are widths synthesized for the fab’s constructions. They are not impedance measurements at a user-entered width. [7628 calculator screenshot](https://digilib.unila.ac.id/91900/6/FILE%20SKRIPSI%20FULL%20TANPA%20BAB%20PEMBAHASAN.pdf) · [3313 project report, §4.5.2](https://dspace.ut.ee/server/api/core/bitstreams/524199a9-3a25-4d7d-a3e1-159889ead108/content)
- **Mask and finished copper matter at this scale.** PCBWay reports a typical reduction of **2 Ω single-ended** after one mask printing pass. JLCPCB’s calculator uses 40.64 µm finished outer copper for nominal 1 oz, different mask thicknesses above copper and substrate, and a narrower etched Track top. REDA’s 35 µm convention, a bare-line equation and the same nominal Fab Preset can therefore give different results for identifiable reasons. [PCBWay process notes](https://www.pcbway.com/pcb_prototype/Impedance_Control.html) · [JLCPCB calculator guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator)
- **The corrected IPC-2141A no longer uses the short logarithms for surface microstrip or offset stripline.** Its errata replaces surface equation 4-4 with a Wheeler-type expression, replaces asymmetric stripline equations 4-17 and 4-19 with a combination of air-filled stripline terms, and replaces coupled microstrip equation 4-24 with the Kirschning–Jansen odd-mode form. The familiar `87/sqrt(Er + 1.41)` logarithm survives in IPC-2221A and in vendor notes. [IPC-2141A errata, pp. 1–3](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf)
- **Worked values (computed in review).** On 0.2 mm Dielectric with Dk 4.4 and 35 µm copper, Hammerstad–Jensen and Wheeler 1977 agree within 0.5 Ω, and both give about 50 Ω at 0.35 mm. The IPC logarithm is 1 to 2 Ω lower. On 0.1 mm Dielectric with Dk 4.1, the gap widens to 4 to 5 Ω, because 35 µm copper is a large fraction of the height. The formulas then put 50 Ω at about 0.15 mm (IPC) or about 0.18 mm (Hammerstad–Jensen), against 0.1565 mm from JLCPCB's calculator. See [Worked values](#worked-values-for-review).
- **For later, edge-coupled differential impedance is twice the per-Track odd-mode impedance.** Kirschning and Jansen give a numerical error envelope for coupled microstrip. Cohn gives an exact zero-thickness solution for centered, edge-coupled stripline. The familiar exponential shortcuts are much shorter, but their constants and height conventions differ between publications. [Kirschning and Jansen](https://doi.org/10.1109/TMTT.1984.1132616) · [Cohn](https://doi.org/10.1109/TMTT.1955.1124973) · [TI LVDS guide, figure 5-1](https://www.ti.com/lit/ug/snla187/snla187.pdf)

## What could and could not be verified

| Source | Kind | What was verified |
| --- | --- | --- |
| E. Hammerstad and Ø. Jensen, “Accurate Models for Microstrip Computer-Aided Design,” IEEE MTT-S International Microwave Symposium Digest, 1980, pp. 407–409 | Paywalled or inaccessible full paper; secondary equation source | The publication was identified. The equations, thickness correction and component accuracy figures below were checked in published technical documentation. Their attribution to the original text is **not verified (secondary)**. [IEEE record](https://doi.org/10.1109/MWSYM.1980.1124303) · [technical equations](https://qucs.sourceforge.net/tech/node75.html) |
| H. A. Wheeler, IEEE Transactions on Microwave Theory and Techniques, 1965, 1977 and 1978 | Primary papers for 1965 and 1977; secondary for the 1978 coefficients | The 1965 and 1977 works address a strip and dielectric sheet. The 1978 paper treats stripline between planes. For 1978, the complete forward calculation below follows the published engineering-book transcription. [1965 paper](https://davemcglone.com/wp-content/uploads/2023/02/IEEE-Wheeler-Transmission-Line-Properties-of-Parallel-Strips-01125962-1.pdf) · [1977 paper](https://doi.org/10.1109/TMTT.1977.1129179) · [1978 paper](https://doi.org/10.1109/TMTT.1978.1129505) · [Gupta, Garg and Chadha, §3.3](https://djvu.online/file/ug2O5U7pooVFZ) |
| S. B. Cohn, “Characteristic Impedance of the Shielded-Strip Transmission Line,” 1954, and “Shielded Coupled-Strip Transmission Line,” 1955 | Primary abstract and paper; subsequent published equation check | Cohn distinguishes zero-thickness results from finite-thickness approximations. The single and odd-mode elliptic expressions can also be checked side by side in the Polar paper. [1954 abstract](https://scixplorer.org/abs/1954ITMTT...2...52C/abstract) · [1955 paper](https://doi.org/10.1109/TMTT.1955.1124973) · [equations 4–8](https://www.polarinstruments.com/support/cits/IPC1999.pdf) |
| M. Kirschning and R. H. Jansen, “Accurate Wide-Range Design Equations for the Frequency-Dependent Characteristic of Parallel Coupled Microstrip Lines,” IEEE Transactions on Microwave Theory and Techniques 32, 83–90, 1984 | Primary paper on a mirror; secondary transcription | The authors identify their numerical reference and give the static modal impedance error. The technical documentation resolves typography in the coefficient equations. [Paper record](https://doi.org/10.1109/TMTT.1984.1132616) · [paper text](https://id.scribd.com/document/477366661/Accurate-wide-range-design-equations-for-microstrip) · [equation transcription](https://qucs.sourceforge.net/tech/node77.html) |
| B. C. Wadell, *Transmission Line Design Handbook*, Artech House, 1991 | Publisher information only; secondary for equations and accuracy | The publisher confirms the book. Burkhardt, Gregg and Staniforth identify and test a handbook expression. A claim attributed specifically to the book’s own text or validation is **not verified (secondary)**. [Publisher](https://uk.artechhouse.com/Transmission-Line-Design-Handbook-P684.aspx) · [comparator study](https://www.polarinstruments.com/support/cits/IPC1999.pdf) |
| IPC-2141, IPC-2141A and the revision-A errata | Paywalled standards; primary front matter and primary errata on a mirror | Revision A supersedes the April 1996 guide. Its errata replaces surface, embedded, asymmetric and coupled equations and withdraws several accuracy statements. The 1996 formula attribution comes from the authors who compared it. [IPC-2141A contents](https://www.ipc.org/TOC/IPC-2141A.pdf) · [revision-A errata](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf) · [1996-form comparison](https://www.polarinstruments.com/support/cits/IPC1999.pdf) |
| IPC-2221A, IPC-2221B and IPC-2251 | Primary exhibit for 2221A; front matter only for 2221B and 2251 | A public exhibit reproduces IPC-2221A §§6.4.1–6.4.4, including the logarithms and width limits. This establishes more for that edition than its contents page alone. The later 2221B and the high-speed packaging guide 2251 were checked only at contents level. [2221A exhibit, PDF pp. 54–56](https://s3-us-west-1.amazonaws.com/ptab-filings%2FIPR2024-01071%2F1011) · [2221B contents](https://www.electronics.org/TOC/IPC-2221B.pdf) · [2251 contents](https://www.electronics.org/TOC/IPC-2251.pdf) |
| Fab guides, calculator documentation, project reports and solver comparisons | Primary for each publisher’s own process, tool, calculation or observation | A saved fab screen establishes what the calculator displayed. A fab Stackup establishes its published dimensions. A solver benchmark establishes error against its stated reference. None of these alone establishes measured impedance on the worked boards. [JLCPCB guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator) · [PCBWay calculator](https://www.pcbway.com/pcb_prototype/impedance_calculator.html) · [Polar study](https://www.polarinstruments.com/support/cits/IPC1999.pdf) |

## Geometry, units and what the Stackup describes

In REDA, a **Stackup** is the ordered physical build of a PCB Document, with its copper and dielectric entries, materials and thicknesses. A **Layer** is a plane of the PCB Document on which objects sit, such as Top Copper. A **Dielectric** is an insulating Stackup entry between copper and holds no objects. The v0.1 Stackup stores copper entries as `{layer, thickness}` in nm, with 1 oz exactly 35 µm, and Dielectric entries as `{uuid, thickness?}`. Solder mask is outside this Stackup. The v0.2 ticket concerns a readout only. Differential pairs belong to v0.4. [PCB specification](../spec/v0.1/pcb.md#stackup) · [REDA glossary](../../GLOSSARY.md) · [issue 329](https://github.com/openreda/reda/issues/329)

| Symbol | Definition |
| --- | --- |
| `w`, `t` | Physical width and finished copper thickness of a rectangular Track. For an etched trapezoid, top and base widths must be distinguished. |
| `h` | Clear Dielectric gap from the facing surface of a Track to the facing surface of its reference plane. In centered stripline this gap is the same on both sides. |
| `hn`, `hf`, `b` | Nearer and farther clear gaps for stripline, with `0 < hn ≤ hf`. `b = hn + t + hf` is the full inside-face separation of the two planes. For centered stripline, `b = 2*h + t`. |
| `H`, `c` | For embedded microstrip, `H` is the height from the plane to the outer surface of the covering Dielectric. If `c` is the cover above the copper top, `H = h + t + c`. |
| `s` | Edge-to-edge gap between two identical Tracks, measured on the same Layer. Center spacing is `w + s`. |
| `Er`, `Eeff` | `Er` or Dk is the material’s relative permittivity. `Eeff` is the mode’s effective relative permittivity. Both are dimensionless. An air-exposed microstrip has fields in two media. Homogeneous stripline has `Eeff = Er`. |
| `Z0`, `Zodd`, `Zdiff`, `eta` | Isolated characteristic impedance, per-Track odd-mode impedance, differential port impedance, and vacuum wave impedance. All are in Ω. Use `eta = 376.730313668 Ω` where the formula calls for it. Constants such as `87`, `60`, `30` and `377` are the published rounded impedance coefficients. |

Use one common length unit throughout each calculation. The worked inputs use mm. `ln` is the natural logarithm, `exp` is its inverse, `^` denotes exponentiation, and angular arguments are in radians. Intermediate ratios and fit coefficients are dimensionless unless identified as lengths or impedances. The equations describe a uniform straight cross-section in the quasi-static limit. Width, reference-plane continuity and a nearby same-Layer Zone can change the answer even with identical Stackup entries. A Pad or Via transition is another geometry. [Polar, equations 9–13](https://www.polarinstruments.com/support/cits/IPC1999.pdf) · [JLCPCB geometry guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator) · [TI guide to interconnect discontinuities](https://www.ti.com/lit/ug/sllu319/sllu319.pdf?ts=1778298828496)

## Surface microstrip

### Hammerstad and Jensen, including finite copper thickness

This model describes one rectangular Track above one plane, with a homogeneous substrate and air above. `A(x)` is the impedance in Ω of the corresponding zero-thickness line in air. `E(x)` is the effective relative permittivity for a zero-thickness normalized width `x`. `u1` and `ur` are different corrected widths divided by `h`. The impedance and propagation permittivity use them differently. [Hammerstad and Jensen, 1980](https://doi.org/10.1109/MWSYM.1980.1124303) · [technical equations 11.4–6, 11.15–17 and 11.22–25](https://qucs.sourceforge.net/tech/node75.html)

```text
u   = w/h
tau = t/h

F(x) = 6 + (2*pi - 6)*exp(-(30.666/x)^0.7528)
A(x) = eta/(2*pi) * ln(F(x)/x + sqrt(1 + (2/x)^2))

a(x) = 1 + ln((x^4 + (x/52)^2)/(x^4 + 0.432))/49
         + ln(1 + (x/18.1)^3)/18.7
beta = 0.564 * ((Er - 0.9)/(Er + 3))^0.053
E(x) = (Er + 1)/2 + (Er - 1)/2 * (1 + 10/x)^(-a(x)*beta)

if t == 0:
    du1 = 0
else:
    du1 = tau/pi * ln(1 + (4*exp(1)/tau)*tanh(sqrt(6.517*u))^2)

dur = du1/2 * (1 + 1/cosh(sqrt(Er - 1)))
u1  = u + du1
ur  = u + dur

Z0   = A(ur)/sqrt(E(ur))
Eeff = E(ur)*(A(u1)/A(ur))^2
```

`beta` is the coefficient often written `b(Er)` in this paper. The equivalent widths are `h*u1` in air and `h*ur` with the Dielectric correction. For fixed physical width, thickness increases the equivalent width and generally lowers impedance. The final line for `Eeff` includes a correction already accounted for in `Z0`. Substituting that final `Eeff` back into the denominator of `Z0` would apply the correction twice. [Technical documentation, equations 11.22–25](https://qucs.sourceforge.net/tech/node75.html)

**Range and accuracy.** The technical documentation reports better than 0.01% for the homogeneous impedance fit through `w/h = 1` and 0.03% through `w/h = 1000`. It gives better than 0.2% for the effective-permittivity fit over `0.01 ≤ w/h ≤ 100`, `Er < 128`. These are component-fit figures. A maximum `t/h` and a measured-board accuracy guarantee for the assembled finite-thickness model were **not verified (secondary)**. This matters particularly for 35 µm copper over 100 µm of prepreg. [Technical documentation](https://qucs.sourceforge.net/tech/node75.html)

### Wheeler 1977 and the comparison with Wadell

Wheeler’s 1965 analysis used separate narrow- and wide-strip treatments. The 1977 paper gives a continuous expression for a strip over a plane and accounts for metal thickness through an equivalent width. `dw` and `wp` below are lengths. [Wheeler, 1965](https://doi.org/10.1109/TMTT.1965.1125962) · [Wheeler, 1977](https://doi.org/10.1109/TMTT.1977.1129179) · [1977 paper text](https://www.scribd.com/document/573778229/Transmission-Line-Properties-of-a-Strip-on-a-Dielectric-Sheet-on-a-Plane)

```text
if t == 0:
    wp = w
else:
    dw = t*(1 + 1/Er)/(2*pi) *
         ln(4*exp(1)/sqrt((t/h)^2 + (1/(pi*(w/t + 1.1)))^2))
    wp = w + dw

x = 4*h/wp
p = x*(14 + 8/Er)/11
Z0 = eta/(2*pi*sqrt(2*(Er + 1))) *
     ln(1 + x*(p + sqrt(p^2 + pi^2*(1 + 1/Er)/2)))
```

Burkhardt, Gregg and Staniforth quote a 2% accuracy figure across width and Er for the Wheeler-type surface equation they attribute to Wadell. Their table 1 is a direct numerical test at `h = 0.794 mm`, `t = 0.035 mm`, `Er = 4.2`. At `w = 1.5 mm`, their handbook implementation was 0.26% below their numerical reference. At `w = 3.3 mm`, it was 0.66% below. The 2% attribution is **not verified (secondary)** against Wadell’s text. The two tabulated comparisons are the study authors’ own results. [Comparison and inputs](https://www.polarinstruments.com/support/cits/IPC1999.pdf) · [Wadell publisher record](https://uk.artechhouse.com/Transmission-Line-Design-Handbook-P684.aspx)

### Older IPC logarithm

The surface expression below is visible in IPC-2221A §6.4.1 and TI’s LVDS guide. Burkhardt and colleagues also attribute it to IPC-2141 **of 1996**. The copper thickness is already in the denominator. [IPC-2221A exhibit](https://s3-us-west-1.amazonaws.com/ptab-filings%2FIPR2024-01071%2F1011) · [TI, figure 5-1](https://www.ti.com/lit/ug/snla187/snla187.pdf) · [IPC-2141 comparison](https://www.polarinstruments.com/support/cits/IPC1999.pdf)

```text
Z0 = 87/sqrt(Er + 1.41) * ln(5.98*h/(0.8*w + t))
```

IPC-2221A prints `w/h < 1` beneath its microstrip formula set. Flux gives its public calculator a wider interval of `0.1 < w/h < 3` and `1 < Er < 15`. These are different published limits. In the preceding numerical study, the IPC result was 2.31% low at `w = 1.5 mm` and 29.94% low at `w = 3.3 mm`. Both widths exceed IPC-2221A’s printed ratio at that substrate height. The study shows how extrapolation behaves. It does not establish an error bound inside the printed range. Revision A of IPC-2141 has an erratum replacing equation 4-4 and removing a 2% accuracy statement. The replacement is a Wheeler 1977-type expression: `eta/(2*pi*sqrt(2)*sqrt(Eeff + 1)) * ln(1 + 4*(h/w')*[4*(h/w')*(14*Eeff + 8)/(11*Eeff) + sqrt(16*(h/w')^2*((14*Eeff + 8)/(11*Eeff))^2 + (Eeff + 1)/(2*Eeff)*pi^2)])`. It is printed with the effective permittivity `Eeff` and an effective width `w'` where Wheeler's 1977 formula above uses `Er` and `wp`. The review read this from the errata itself. The definitions of `Eeff` (equation 4-6) and `w'` are in the paywalled standard and were not verified. [IPC-2221A §6.4.1](https://s3-us-west-1.amazonaws.com/ptab-filings%2FIPR2024-01071%2F1011) · [Flux](https://www.flux.ai/p/tools/pcb-impedance-calculator) · [comparator, table 1](https://www.polarinstruments.com/support/cits/IPC1999.pdf) · [IPC-2141A errata](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf)

## Embedded microstrip

The geometry has one reference plane, Dielectric below the Track, and a cover of finite thickness above it. Doug Brooks, in “Embedded Microstrip Impedance Formula,” *Printed Circuit Design*, February 2000, traces the common approximation to the older IPC-D-317A derivation and explains a substitution error. Atmel application note 6386A independently prints the resulting equation. The model assumes one material Er throughout the substrate and cover, with air beyond the cover. [Brooks, equations DGB3–4](https://www-eng.lbl.gov/~shuman/NEXT/CURRENT_DESIGN/TP/MATERIALS/impedance_formula_derivations.pdf) · [Atmel geometry table](https://kr.element14.com/wcsstore/ExtendedSitesCatalogAssetStore/cms/asset/pdf/apac/common/applications/medical/appnotes/atmel/Atmel_SAM9XE128_Application_Notes_15.pdf)

```text
H      = h + t + c
Eembed = Er*(1 - exp(-1.55*H/h))
Z0     = 60/sqrt(Eembed) * ln(5.98*h/(0.8*w + t))
```

`Eembed` is the model’s effective relative permittivity. The exponent uses the plane-to-outer-surface height `H`, **not just the cover thickness** `c`. For deep embedding `Eembed` approaches `Er`. At zero cover the fit need not reproduce the surface formula exactly. Brooks explains that substituting `Eembed` for `Er` in the **87-based** surface formula applies the dielectric adjustment twice. [Brooks](https://www-eng.lbl.gov/~shuman/NEXT/CURRENT_DESIGN/TP/MATERIALS/impedance_formula_derivations.pdf)

In review on 2026-11-18, the Brooks PDF link returned HTTP 404 to a direct fetch, although search engines still index it. The `1 - exp(-1.55*H/h)` form was confirmed only from search-result summaries of Brooks's article and of calculators that implement it. The errata's replacement embedded equation 4-7 is a different form, `Z0_embedded = Z0_surf / sqrt(exp(-2*b/h1) + (Er/Eeff)*(1 - exp(-2*b/h1)))`, with its variables defined in the paywalled text. [IPC-2141A errata, p. 1](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf)

**Range and accuracy.** PCBWay’s form warns that the total-height to Track-height ratio must exceed 1.2 for its embedded calculation. Brooks calls the original test conditions uncertain and gives no percentage against a field solver. Neither source validates this expression for a thin solder mask with Dk different from the substrate. Attribution of this particular exponential to corrected IPC-2141A is **not verified (secondary)**. The errata explicitly changes that standard’s embedded equation 4-7. [PCBWay calculator](https://www.pcbway.com/pcb_prototype/impedance_calculator.html) · [Brooks](https://www-eng.lbl.gov/~shuman/NEXT/CURRENT_DESIGN/TP/MATERIALS/impedance_formula_derivations.pdf) · [IPC errata](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf)

## Symmetric stripline

### Cohn’s exact zero-thickness reference

For a centered Track between two parallel planes in one uniform Dielectric, Cohn’s conformal-mapping result is exact in the ideal TEM model for positive `w` and `b` and `t = 0`. `K` below takes an elliptic **modulus**. A library defining its complete elliptic integral in terms of the parameter `m` takes the square of the modulus. This distinction also applies to the differential formula below. [Cohn, 1954](https://doi.org/10.1109/TMTT.1954.1124875) · [Polar, equations 4–5](https://www.polarinstruments.com/support/cits/IPC1999.pdf)

```text
K(k) = integral(theta = 0 to pi/2) of
       1/sqrt(1 - k^2*sin(theta)^2) dtheta

x  = pi*w/(2*b)
k  = tanh(x)
kp = 1/cosh(x)
Z0 = eta/(4*sqrt(Er)) * K(kp)/K(k)
```

Cohn reports that his simpler design approximations agree within 1.2% with the exact zero-thickness solution. He expects similar accuracy for finite thickness through one-quarter of the plane spacing. That finite-thickness statement is an expectation. The exact expression above has no thickness correction. [Cohn’s abstract](https://scixplorer.org/abs/1954ITMTT...2...52C/abstract)

### Wheeler 1978, including finite copper thickness

H. A. Wheeler, “Transmission-Line Properties of a Strip Line Between Parallel Planes,” IEEE Transactions on Microwave Theory and Techniques 26, 866–876 (1978), treats **centered stripline**. In the forward calculation below, `T` is normalized thickness, `m` is a fitted exponent, `dw` is a length and `v` is effective width divided by the available Dielectric thickness. This is the normalized form given by Gupta, Garg and Chadha in *Computer-Aided Design of Microwave Circuits*, equations 3.39–3.41. [Wheeler publication](https://doi.org/10.1109/TMTT.1978.1129505) · [Gupta, Garg and Chadha](https://djvu.online/file/ug2O5U7pooVFZ) · [independent handbook transcription](https://uodiyala.edu.iq/uploads/PDF%20ELIBRARY%20UODIYALA/EL33/Electronic%20Packaging%20Handbook.pdf)

```text
T = t/b
if t == 0:
    dw = 0
else:
    m  = 6*(1 - T)/(3 - T)
    q  = (T/(2 - T))^2 + (0.0796*T/(w/b + 1.1*T))^m
    dw = t/pi * (1 - 0.5*ln(q))

wp = w + dw
v  = wp/(b - t)
x  = 8/(pi*v)
Z0 = 30/sqrt(Er) * ln(1 + (x/2)*(x + sqrt(x^2 + 6.27)))
```

The geometry requires `w > 0` and `0 ≤ t < b`. Gupta, Garg and Chadha report accuracy within 0.5% for `wp/(b−t) < 10`. The original reference sweep and its precise error definition were **not verified (secondary)**. Keysight separately documents using Cohn for a centered line of negligible thickness and Wheeler for its finite-thickness centered model. It describes a different model for offset conductors. [Gupta, Garg and Chadha](https://djvu.online/file/ug2O5U7pooVFZ) · [Keysight distributed components manual, stripline models](https://edadownload.software.keysight.com/eedl/ads/2011_01/pdf/ccdist.pdf)

### IPC-style centered logarithm

IPC-2221A §6.4.3, Atmel and TI give this simpler form. The full plane spacing includes the copper thickness. IPC-2221A gives `w/h < 2`, where `h` is the clear gap to **one** plane. No solver or measurement percentage was verified for this logarithm. The revision-A errata removes separate accuracy statements for both of its symmetric stripline formula sets. [IPC-2221A exhibit](https://s3-us-west-1.amazonaws.com/ptab-filings%2FIPR2024-01071%2F1011) · [TI figure 5-1](https://www.ti.com/lit/ug/snla187/snla187.pdf) · [IPC-2141A errata](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf)

```text
b = 2*h + t
Zcenter_log = 60/sqrt(Er) * ln(1.9*b/(0.8*w + t))
```

## Offset or asymmetric stripline

Atmel gives the following engineering approximation for a rectangular Track between unequal plane gaps. The same family appears in IPC-2221A §6.4.4 for dual asymmetric stripline, with the far distance expressed using the separation between signal Layers. Use facing-surface distances consistently. The formula below assumes continuous reference planes, negligible coupling to other signals and one common Er. [Atmel application note 6386A](https://kr.element14.com/wcsstore/ExtendedSitesCatalogAssetStore/cms/asset/pdf/apac/common/applications/medical/appnotes/atmel/Atmel_SAM9XE128_Application_Notes_15.pdf) · [IPC-2221A §6.4.4](https://s3-us-west-1.amazonaws.com/ptab-filings%2FIPR2024-01071%2F1011)

```text
hn = min(clear_gap_above, clear_gap_below)
hf = max(clear_gap_above, clear_gap_below)

Zoffset_log = 80/sqrt(Er) *
              ln(1.9*(2*hn + t)/(0.8*w + t)) *
              (1 - hn/(4*hf))
```

Setting `hn = hf` recovers the centered logarithm exactly, since `80*(1−1/4) = 60`. Finite copper enters the numerator and denominator. No primary source located gives a maximum gap ratio or a solver-validated error bound for this particular fit. IPC-2221A observes that assuming a centered line causes relatively little error for a conductor in roughly the middle third of the plane interval. That observation is not a validation of the fit at large offsets. The attribution of this simple expression to **IPC-2141A** is **not verified (secondary)**. The errata replaces IPC-2141A's asymmetric equations 4-17 and 4-19 with a different construction. In that construction, `Z0 = (1/sqrt(Er)) * {Z_SS(Er = 1, w', h = (h1 + h2)/2) - dZ_air}`, where `Z_SS` is the standard's symmetric stripline equation 4-9 evaluated in air. `dZ_air` is built from `Z_SS` evaluated at `h1` and at `h2`. Equation 4-9 itself is in the paywalled text and was not verified.

**A second source prints a different correction term.** The leading suite's article attributes an `80/sqrt(Er)` stripline equation to Digital Equipment Corporation in the mid-1980s. Its correction factor is `1 - B/(4*B + 4*C + 4*T)` instead of `1 - hn/(4*hf)`, and the article does not define `B` and `C`. If `B` is read as `hn` and `C` as `hf`, the two factors differ: at equal gaps the article's form gives about `70/sqrt(Er)`, not the centered `60/sqrt(Er)`. On worked case C the two forms differ by about 0.4 Ω, because there the near gap is much smaller than the far gap. The review could not settle which form the original sources print. The offset logarithm is therefore **not cross-checked**. [The leading suite's article, equation 4](https://resources.altium.com/p/methods-calculating-and-measuring-impedance-part-1) [IPC-2221A exhibit](https://s3-us-west-1.amazonaws.com/ptab-filings%2FIPR2024-01071%2F1011) · [revision-A errata](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf)

A physical distinction matters on four-Layer boards. JLCPCB specifies Dk 4.4 for 7628 prepreg and 4.6 for its core. The formula accepts only one Er. The worked section therefore identifies both materials and specifies two homogeneous sensitivity calculations. A matched prediction for the manufactured geometry would need a model accepting both Dielectrics and both gaps. Exterior solder mask is outside the field region of a stripline bounded by continuous, ideal planes. [JLCPCB parameters](https://jlcpcb.com/impedance) · [Polar stripline structures](https://www.polarinstruments.com/support/cits/AP123.html)

## Edge-coupled differential forms — for later, v0.4

These forms assume two identical parallel Tracks with opposite excitation and a uniform gap. `Zodd` is the per-Track impedance under that excitation. As spacing becomes large, each Track approaches its isolated impedance. [Polar, equations 6–8](https://www.polarinstruments.com/support/cits/IPC1999.pdf)

```text
Zdiff = 2*Zodd
```

### Edge-coupled surface microstrip

**Short engineering approximation.** TI’s LVDS guide gives the following fit for an air-exposed pair. Thickness enters through the isolated Track model. The spacing factor has no independent copper-sidewall or mask parameter. No solver error bound accompanying this specific fit was verified. [TI LVDS guide, figure 5-1](https://www.ti.com/lit/ug/snla187/snla187.pdf)

```text
Zsingle = 87/sqrt(Er + 1.41) * ln(5.98*h/(0.8*w + t))
Zdiff   = 2*Zsingle*(1 - 0.48*exp(-0.96*s/h))
```

**Kirschning–Jansen quasi-static odd mode.** The following is the zero-thickness branch needed for differential impedance. `E(u)` and `A(u)` are the complete single-line functions in the Hammerstad–Jensen section above. `E0` is the isolated thin-line effective permittivity. `Eo` is the coupled odd-mode effective permittivity. `Zs` is the isolated thin-line impedance in Ω. All `Q` coefficients are dimensionless. [Original paper, equations 1–10](https://id.scribd.com/document/477366661/Accurate-wide-range-design-equations-for-microstrip) · [technical transcription, equations 11.95–99 and 11.119–129](https://qucs.sourceforge.net/tech/node77.html)

```text
u = w/h
g = s/h
E0 = E(u)
Zs = A(u)/sqrt(E0)

ao = 0.7287*(E0 - (Er + 1)/2)*(1 - exp(-0.179*u))
bo = 0.747*Er/(0.15 + Er)
co = bo - (bo - 0.207)*exp(-0.414*u)
d  = 0.593 + 0.694*exp(-0.562*u)
Eo = E0 + ((Er + 1)/2 + ao - E0)*exp(-co*g^d)

Q1  = 0.8695*u^0.194
Q2  = 1 + 0.7519*g + 0.189*g^2.31
Q3  = 0.1975 + (16.6 + (8.4/g)^6)^(-0.387)
             + ln(g^10/(1 + (g/3.4)^10))/241
Q4  = (2*Q1/Q2)/(exp(-g)*u^Q3 + (2 - exp(-g))*u^(-Q3))
Q5  = 1.794 + 1.14*ln(1 + 0.638/(g + 0.517*g^2.43))
Q6  = 0.2305 + ln(g^10/(1 + (g/5.8)^10))/281.3
             + ln(1 + 0.598*g^1.154)/5.1
Q7  = (10 + 190*g^2)/(1 + 82.3*g^3)
Q8  = exp(-6.5 - 0.95*ln(g) - (g/0.15)^5)
Q9  = ln(Q7)*(Q8 + 1/16.5)
Q10 = Q4 - (Q5/Q2)*exp(ln(u)*Q6*u^(-Q9))

Zodd  = sqrt(E0/Eo)*Zs/(1 - (Zs/377)*sqrt(E0)*Q10)
Zdiff = 2*Zodd
```

Kirschning and Jansen give `0.1 ≤ w/h ≤ 10`, `0.1 ≤ s/h ≤ 10` and `1 ≤ Er ≤ 18`. They report better than 0.6% for the **static modal impedances**, against converged spectral-domain hybrid-mode numerical data. That percentage does not extend to a solder-masked, etched pair on woven FR-4. Their paper separately models frequency dispersion. [Original paper](https://id.scribd.com/document/477366661/Accurate-wide-range-design-equations-for-microstrip) · [parameter ranges](https://qucs.sourceforge.net/tech/node77.html)

A separate R. H. Jansen finite-metal correction uses different equivalent widths for the two modes. The published technical transcription gives the following width terms, in length units. For the odd mode, use `w_odd` as the trial strip width in the thin odd-mode model, with the physical gap `s`. This approximation requires `s` much larger than `2*t` and one of the stated width branches. Its finite-thickness accuracy relative to a solver was **not verified (secondary)**. [Technical equations 11.180–183](https://qucs.sourceforge.net/tech/node77.html) · [KiCad manual’s original Jansen citation](https://docs.kicad.org/10.0/en/pcb_calculator/pcb_calculator.html)

```text
if t == 0:
    w_even = w
    w_odd  = w
else if s >> 2*t and w > h/(2*pi) and h/(2*pi) > 2*t:
    dw = (t/pi)*(1 + ln(2*h/t))
else if s >> 2*t and h/(2*pi) >= w and w > 2*t:
    dw = (t/pi)*(1 + ln(4*pi*w/t))
else:
    the published approximation supplies no valid branch

for either valid nonzero-t branch:
    dt     = 2*t*h/(s*Er)
    w_even = w + dw*(1 - 0.5*exp(-0.69*dw/dt))
    w_odd  = w_even + dt
```

The source leaves “much larger” qualitative. It supplies no numerical cutoff for treating a tightly spaced hobby-board pair as thin metal. [Technical documentation](https://qucs.sourceforge.net/tech/node77.html)

### Edge-coupled centered stripline

**Cohn’s zero-thickness solution.** The two strips lie side by side, halfway between the same two planes, in a homogeneous Dielectric. Use the modulus convention for `K` defined in the symmetric-stripline section. The expression is exact for `t = 0` and positive `w`, `s` and `b`. It recovers twice the isolated centered-stripline impedance as spacing grows. Cohn treats finite metal with separate approximations. [Cohn, 1955](https://doi.org/10.1109/TMTT.1955.1124973) · [Polar, equations 7–8](https://www.polarinstruments.com/support/cits/IPC1999.pdf)

```text
q     = tanh(pi*w/(2*b))/tanh(pi*(w + s)/(2*b))
Zodd  = eta/(4*sqrt(Er)) * K(sqrt(1 - q^2))/K(q)
Zdiff = 2*Zodd
```

**Finite-copper engineering shortcut.** The following is the explicit full-plane-spacing convention published by Flux. Thickness enters the isolated impedance and the plane separation. Its exact IPC edition and a numerical solver error envelope are **not verified (secondary)**. TI’s LVDS manual also prints a `0.347` coefficient. A different TI evaluation-module guide discusses `0.374` and a larger coefficient for close spacing. The geometry definitions and coefficient must travel together. [Flux equations](https://www.flux.ai/p/tools/pcb-impedance-calculator) · [TI LVDS guide](https://www.ti.com/lit/ug/snla187/snla187.pdf) · [TI evaluation-module guide, p. 17](https://www.ti.com/lit/ug/sllu319/sllu319.pdf?ts=1778298828496)

```text
b       = 2*h + t
Zsingle = 60/sqrt(Er) * ln(1.9*b/(0.8*w + t))
Zdiff   = 2*Zsingle*(1 - 0.347*exp(-2.9*s/b))
```

The centered Cohn solution and this centered shortcut do not provide an offset edge-coupled solution. For a manufactured pair, gap and sidewall shape affect mutual capacitance as well as each Track’s isolated impedance. Polar’s experimental study also identifies the glass/resin distribution as a source of discrepancy for differential lines on FR-4. [Cohn](https://doi.org/10.1109/TMTT.1955.1124973) · [Polar differential measurements](https://www.polarinstruments.com/support/cits/IPC-Paper.pdf)

## Solder mask, copper thickness and accuracy on a finished board

| Effect | Published evidence | Consequence for comparison |
| --- | --- | --- |
| Solder mask on an outer Track | Polar describes a decrease of up to a few ohms depending on mask material and thickness. PCBWay gives a typical one-pass reduction of 2 Ω single-ended and 8 Ω differential. [Polar coated microstrip](https://www.polarinstruments.com/products/si/help/coatedmicrostrip.htm) · [PCBWay process notes](https://www.pcbway.com/pcb_prototype/Impedance_Control.html) | The PCBWay decrement supplies a reproducible estimate for the mask rows. Its source gives no universal mask Dk, film thickness, Track-width range or accuracy for the decrement. |
| Nonuniform mask | JLCPCB specifies Dk 3.8, 0.6 mil above the Track, 1.2 mil above the substrate and 1.2 mil between Tracks. Polar shows a 2.4 Ω difference between two coating models on a nominal 75 Ω line. [JLCPCB guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator) · [Polar AP176](https://www.polarinstruments.com/support/cits/AP176.html) | Polar’s 2.4 Ω is a **model-to-model difference**. Adding that number to a bare-line prediction would misread the experiment. |
| Finished copper and etch | REDA represents 1 oz by 35 µm. JLCPCB’s calculator guide uses 1.6 mil finished outer copper and a top width 0.7 mil smaller than the base width. OSH Park publishes 0.0432 mm plated outer copper for its four-Layer service. [REDA specification](../spec/v0.1/pcb.md#stackup) · [JLCPCB guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator) · [OSH Park four-Layer service](https://docs.oshpark.com/services/four-layer/) | Greater thickness at fixed rectangular width generally lowers impedance. Narrowing the etched top changes the cross-section again. Neither change is captured by the label “1 oz” alone. |
| Numerical model versus measurement | Burkhardt and colleagues measured manufactured differential Tracks by time-domain reflectometry and obtained dimensions by microsection. Their embedded-microstrip distribution reports mean difference 0.05% and standard deviation 1.88%. [Study, figures 8–9](https://www.polarinstruments.com/support/cits/IPC1999.pdf) | This is a comparison of their numerical model with manufactured samples. It is not a measurement accuracy figure for the Brooks, IPC or Hammerstad–Jensen equations. |

For the PCBWay one-pass comparison, apply the decrement once, after evaluating a bare surface formula. It is independent of the separate JLCPCB coating model. [PCBWay process notes](https://www.pcbway.com/pcb_prototype/Impedance_Control.html)

```text
Zcoated_single_estimate = Zbare_single - 2 Ω
Zcoated_diff_estimate   = Zbare_diff   - 8 Ω
```

## What tools and fabs document

| Tool or fab | Documented behaviour |
| --- | --- |
| **KiCad Calculator Tools 10.0** | The transmission-line calculator is based on Transcalc. The manual cites H. A. Atwater’s 1989 paper under single microstrip. For coupled microstrip it lists Atwater, Kirschning and Jansen, R. Jansen, and S. March. It lists stripline without naming a stripline author. The documentation supports these statements without inspecting calculator code. [KiCad manual](https://docs.kicad.org/10.0/en/pcb_calculator/pcb_calculator.html) |
| **the leading suite** | Its documentation identifies a two-dimensional quasi-static method-of-moments field solver in its Stackup manager. It names solder mask, surface finish and the reference plane as settings that can change a comparison. [The leading suite’s documentation](https://www.altium.com/documentation/knowledge-base/altium-designer/validate-transmission-impedance-computed-in-layer-stack) |
| **a second commercial suite** | Its cross-section editor displays single-ended and differential impedance for microstrip and stripline, supports edge or broadside coupling, and accepts an etch angle. The cited product page does not identify the underlying numerical algorithm. [A second commercial suite’s documentation](https://community.cadence.com/cadence_blogs_8/b/pcb/posts/specifying-layer-info-multilayer-rigid-flex-stackups) |
| **JLCPCB** | The calculator synthesizes width for a target impedance, selected Stackup, signal Layer, reference plane or planes, and optional same-Layer copper. JLCPCB documents its copper, etch and coating parameters. Its design article suggests Si9000 as an **alternative** calculation using JLCPCB lamination data. I found no first-party statement identifying the online backend as Si9000 or establishing its field-solver method. [Calculator guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator) · [JLCPCB design article](https://jlcpcb.com/blog/understanding-impedance-matching-for-high-speed-pcb-designs/) |
| **PCBWay** | PCBWay calls its online results approximate. It omits surface finish and says its outer microstrip, edge-coupled and coplanar calculations exclude solder mask. It recommends Polar for coated calculations. Its separate manufacturing note describes first-article inspection and production impedance checks. [Calculator](https://www.pcbway.com/pcb_prototype/impedance_calculator.html) · [Manufacturing process](https://www.pcbway.com/pcb_prototype/Impedance_Control.html) |
| **Polar** | Polar explicitly identifies Si9000e as a boundary-element field solver supporting multiple Dielectrics and separate mask coverage adjacent to, above and between Tracks. This establishes the algorithm for Polar’s product. [Si9000e documentation](https://www.polarinstruments.com/products/si/Si9000.html) |
| **AISLER** | For its nominal 1.6 mm, 35 µm, four-Layer build, AISLER publishes **0.295 mm for a 50 Ω Top-to-IN1 Track**. It describes the published values as basic orientation. This is a useful fab-owned comparison on a different construction. [AISLER four-Layer guidance](https://community.aisler.net/t/4-layers-1-6mm-35-m-stackup/5457) |
| **OSH Park and Eurocircuits** | OSH Park publishes its plated copper, Dielectric thicknesses and Dk. Its four-Layer service has 0.1999 mm outer prepreg with Dk 3.61 at 1 GHz. Its six-Layer service has 0.1107 mm outer prepreg with Dk 3.23 at 1 GHz. Eurocircuits offers buildup-specific width and gap calculation on its four-, six- and eight-Layer impedance pool. [OSH Park four-Layer](https://docs.oshpark.com/services/four-layer/) · [OSH Park six-Layer](https://docs.oshpark.com/services/six-layer/) · [Eurocircuits service](https://www.eurocircuits.com/services/defined-impedance-pool/) |

**What prototype builders actually use.** A four-Layer telemetry project used JLCPCB’s non-coplanar 50 Ω result for an RF Track. A keyboard builder instead calculated a USB pair from the selected prepreg height and Dk, reported 91 Ω against a 90 Ω target, and reported successful USB operation. A third builder compared a 2116 Stackup with and without adjacent ground and obtained different widths from the fab tool. These examples document both fab-calculator and hand-calculation workflows. Successful USB operation alone does not measure characteristic impedance. For six Layers, JLCPCB describes a signal/ground/power/signal/ground/signal arrangement, which provides an inner signal Layer between references. Eurocircuits distinguishes pooled constructions from non-pooled blind or buried Via options. No source established the fraction of hobbyists using any one formula. [Telemetry project](https://digilib.unila.ac.id/91900/6/FILE%20SKRIPSI%20FULL%20TANPA%20BAB%20PEMBAHASAN.pdf) · [Keyboard project](https://khushaanvirk.com/projects/keyboard-pcb) · [Builder’s Stackup comparison](https://blog.jessfraz.com/post/home-hardware-part-two/) · [JLCPCB six-Layer service](https://jlcpcb.com/6-layer-pcb/) · [Eurocircuits](https://www.eurocircuits.com/services/defined-impedance-pool/)

## Worked values for review

**Every formula value in this section was computed in review** with a short script kept outside the repo. The script implements the equations exactly as written in this file: Hammerstad–Jensen with the thickness correction, Wheeler 1977, the IPC logarithms and the offset logarithm. It also implements Wheeler 1978 and Cohn, used as a check. No interactive fab calculation was run. All inputs below are lengths in **mm**, except dimensionless Dk. Copper is rectangular, planes are continuous, and there is no adjacent same-Layer Zone. Bare-line calculations use fixed Dk in the quasi-static limit.

The script was checked in two ways before use:
- It reproduces the published Burkhardt, Gregg and Staniforth comparison at `h = 0.794`, `t = 0.035`, `Er = 4.2`. At `w = 1.5` it gives Hammerstad–Jensen 50.63 Ω (numerical reference 50.63), Wheeler 1977 50.56 Ω (their handbook form 50.50) and IPC 49.47 Ω (theirs 49.46). At `w = 3.3`, IPC is 30% below Hammerstad–Jensen, matching their 29.94% figure. The review could not read that PDF directly, so these study numbers are taken from the Codex draft. The close match with an independent recomputation supports them.
- Its Wheeler 1978 and Cohn stripline functions agree within 0.3% at `t = 0`.

| Input set | Exact construction to use |
| --- | --- |
| **A — nominal 7628-style surface microstrip** | Use a nominal 1.6 mm four-Layer board, L1 signal and L2 reference. Use `h = 0.2000`, `t = 0.0350`, `Er = 4.4`. For context, the corresponding JLCPCB construction drawing lists 0.2104 mm prepreg on each side of a 1.065 mm core, 0.0152 mm inner copper and 0.035 mm outer copper. The outer-line formula uses the specified 0.2000 mm gap. [JLCPCB drawing](https://jlcpcb.com/impedance) |
| **B — nominal 3313-style surface microstrip** | Use the same Layer assignment. Use `h = 0.1000`, `t = 0.0350`, `Er = 4.1`. The corresponding JLCPCB drawing lists 0.0994 mm 3313 on each side of a 1.265 mm core, 0.0152 mm inner copper and 0.035 mm outer copper. [JLCPCB drawing](https://jlcpcb.com/impedance) |
| **C — actual 7628 drawing, inner signal** | Use `JLC04161H-7628`, L2 signal, continuous L1 and L3 reference planes, `hn = 0.2104` prepreg, `hf = 1.0650` core, `t = 0.0152`, `w = 0.2500`. The real Dk values are 4.4 near and 4.6 far. For the single-Er offset logarithm, evaluate once with **all Er = 4.4** and once with **all Er = 4.6**. These are sensitivity cases. The production two-material impedance remains unknown. Exterior mask has no role between the ideal reference planes. [JLCPCB drawing and Dk](https://jlcpcb.com/impedance) |
| **M — masked versions of A** | Keep every A dimension and Dk. Apply **one PCBWay mask pass** using the published 2 Ω single-ended decrement. That empirical rule has no film-thickness or mask-Dk input. For a separately identified JLCPCB-style solver comparison, the documented mask parameters are Dk 3.8, 0.6 mil above copper and 1.2 mil above substrate. Do not apply the decrement to a solver output that already includes mask. [PCBWay rule](https://www.pcbway.com/pcb_prototype/Impedance_Control.html) · [JLCPCB guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator) |

There are **three distinct input conventions** in the comparisons. A and B use rounded nominal dielectric height and REDA’s 35 µm copper. JLCPCB’s drawing gives physical construction dimensions, while its separate impedance notes specify 7.1 mil and 3.5 mil for outer controlled Tracks on 7628 and 3313, and 8.1 mil for an inner Track on 7628. Its current calculator guide uses 1.6 mil finished outer copper. The historical screenshots do not disclose every internal solver setting. The table therefore preserves the published inputs rather than treating these conventions as interchangeable. [JLCPCB construction notes](https://cart.jlcpcb.com/client/template/placeOrder/impedance.html) · [current Stackup drawing](https://jlcpcb.com/impedance) · [calculator guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator)

**Calculator access for every row.** **J** means JLCPCB’s JavaScript form could not be driven. Its older public template exposed unresolved result placeholders. **P** means PCBWay’s form was readable, but the web reader could not submit inputs or obtain a result. Neither failure established a login requirement. **I** denotes an independently published IndieRF preset result. IndieRF’s interactive form remained at its loading screen, but its preset pages expose numerical results. The published 1 GHz presets use rectangular 40.64 µm copper and outer gaps of 7.1 or 3.5 mil. They omit JLCPCB’s documented etch taper. [JLCPCB form](https://jlcpcb.com/pcb-impedance-calculator) · [older template](https://cart.jlcpcb.com/client/template/placeOrder/impedanceCalculation.html) · [PCBWay form](https://www.pcbway.com/pcb_prototype/impedance_calculator.html) · [IndieRF 7628 preset](https://www.indierf.com/tools/stackups/jlc04161h-7628) · [IndieRF 3313 preset](https://www.indierf.com/tools/stackups/jlc04161h-3313)

| Case | Track width | Mask | IPC-style result, Ω | Hammerstad–Jensen, Ω | Wheeler / handbook comparison, Ω | Fab or other published comparison |
| --- | ---: | --- | --- | --- | --- | --- |
| A1 | 0.2000 | None | 65.46 | 66.28 | 66.56 | **J / P:** no fixed-width result. The published JLCPCB 7628 point is 50 Ω at **0.3493 mm**. **I:** its bare 50 Ω width is **0.313 mm**. Neither gives the impedance at 0.2000 mm. [Fab screen](https://digilib.unila.ac.id/91900/6/FILE%20SKRIPSI%20FULL%20TANPA%20BAB%20PEMBAHASAN.pdf) · [I](https://www.indierf.com/tools/stackups/jlc04161h-7628) |
| A2 | 0.3000 | None | 53.06 | 54.53 | 54.65 | **J / P:** no fixed-width result. **I:** bare 50 Ω at **0.313 mm** on its preset. AISLER publishes 50 Ω at **0.295 mm** on its own four-Layer board. The AISLER page does not establish a bare-line match. [I](https://www.indierf.com/tools/stackups/jlc04161h-7628) · [AISLER](https://community.aisler.net/t/4-layers-1-6mm-35-m-stackup/5457) |
| A3 | 0.3500 | None | 48.15 | 50.20 | 50.26 | **J / P:** no live result. The historical JLCPCB screen gives **50 Ω at 0.3493 mm**, L1 to L2, with no coplanar ground. This is the closest width match in A; the calculator’s construction still differs from the specified bare rectangle. [Fab screen](https://digilib.unila.ac.id/91900/6/FILE%20SKRIPSI%20FULL%20TANPA%20BAB%20PEMBAHASAN.pdf) |
| B1 | 0.1000 | None | 61.10 | 65.35 | 65.84 | **J / P:** no fixed-width result. A builder reports JLCPCB **50 Ω at 0.1565 mm** on 3313. **I:** bare 50 Ω at **0.152 mm** on its preset. Neither is a result for 0.1000 mm. [Project report](https://dspace.ut.ee/server/api/core/bitstreams/524199a9-3a25-4d7d-a3e1-159889ead108/content) · [I](https://www.indierf.com/tools/stackups/jlc04161h-3313) |
| B2 | 0.1500 | None | 50.04 | 54.28 | 54.54 | **J / P:** no live result. The nearby published points are **0.1565 mm** from the JLCPCB user and **0.152 mm** from the bare independent preset, both targeting 50 Ω. [Project report](https://dspace.ut.ee/server/api/core/bitstreams/524199a9-3a25-4d7d-a3e1-159889ead108/content) · [I](https://www.indierf.com/tools/stackups/jlc04161h-3313) |
| B3 | 0.2000 | None | 41.53 | 46.63 | 46.72 | **J / P:** no result at this width. The same-build JLCPCB reference remains **0.1565 mm for 50 Ω**. On a different, thicker 2116 build, a user reports **0.1814 mm** without side ground and **0.1778 mm** with side ground. [3313 report](https://dspace.ut.ee/server/api/core/bitstreams/524199a9-3a25-4d7d-a3e1-159889ead108/content) · [2116 comparison](https://blog.jessfraz.com/post/home-hardware-part-two/) |
| C | 0.2500 | Outside planes | Offset logarithm: 48.87 at all Er 4.4, 47.79 at all Er 4.6. The variant in the leading suite's article gives 49.31 and 48.23. | Inapplicable (one-plane model) | Inapplicable (centered model) | **J / P:** no matched L2 result or fab-published substitute found. As a separate offset example, RF Tools publishes **50 Ω at 0.1654 mm** for gaps **0.15/0.45 mm**, `t = 0.0175 mm`, uniform `Er = 4.4`. It is not a JLCPCB prediction. [JLCPCB physical build](https://jlcpcb.com/impedance) · [independent worked example](https://rftools.io/calculators/pcb/asymmetric-stripline/) |
| M1 | 0.2000 | One pass | 63.46 | 64.28 | 64.56 | **J / P:** no coated fixed-width result. PCBWay’s published substitute is **A1 minus about 2 Ω**. **I:** the coated 7628 preset gives **50 Ω at 0.296 mm**. [PCBWay](https://www.pcbway.com/pcb_prototype/Impedance_Control.html) · [I](https://www.indierf.com/tools/stackups/jlc04161h-7628) |
| M2 | 0.3000 | One pass | 51.06 | 52.53 | 52.65 | **J / P:** no coated fixed-width result. PCBWay’s substitute is **A2 minus about 2 Ω**. **I:** **0.296 mm for 50 Ω** is close in width, under a different dielectric height, copper thickness and mask model. [PCBWay](https://www.pcbway.com/pcb_prototype/Impedance_Control.html) · [I](https://www.indierf.com/tools/stackups/jlc04161h-7628) |
| M3 | 0.3500 | One pass | 46.15 | 48.20 | 48.26 | **J / P:** no coated fixed-width result. PCBWay’s substitute is **A3 minus about 2 Ω**. The historical JLCPCB **0.3493 mm for 50 Ω** is a fab-process comparison. Its screen does not establish equivalence to one PCBWay mask pass. [PCBWay](https://www.pcbway.com/pcb_prototype/Impedance_Control.html) · [fab screen](https://digilib.unila.ac.id/91900/6/FILE%20SKRIPSI%20FULL%20TANPA%20BAB%20PEMBAHASAN.pdf) |

**How to read the formula columns.** In A and B the third formula column uses Wheeler **1977**. In M each column applies the PCBWay decrement to its own bare result. In C only the **offset** logarithm applies directly to the two single-Er sensitivity cases. For that row, Hammerstad–Jensen is a one-plane microstrip model and Wheeler **1978** is a centered stripline model. Their cells are therefore recorded as *inapplicable to the actual geometry*. Substituting the mean plane distance would create a different test case. [Wheeler 1977](https://doi.org/10.1109/TMTT.1977.1129179) · [Wheeler 1978](https://doi.org/10.1109/TMTT.1978.1129505) · [Keysight’s distinction between centered and offset models](https://edadownload.software.keysight.com/eedl/ads/2011_01/pdf/ccdist.pdf)

Codex had computed A3 in an earlier session as 48.15 Ω (IPC), 50.20 Ω (Hammerstad–Jensen) and 50.26 Ω (Wheeler 1977), with 52.72 Ω for Hammerstad–Jensen at `t = 0`. The review's script reproduces all four to 0.01 Ω. At A and B, the `t = 0` limit is 3 to 8 Ω above the thick-copper value: 71.03 against 66.28 at A1, and 73.26 against 65.35 at B1. Copper thickness matters most on thin prepreg.

**Width for 50 Ω, computed in review, set against published fab widths.** The table below solves each formula for the bare 50 Ω width. The last column is the published fab or tool width for comparison.

| Inputs | IPC, mm | Hammerstad–Jensen, mm | Wheeler 1977, mm | Published 50 Ω width |
| --- | ---: | ---: | ---: | --- |
| A: `h = 0.2000`, `t = 0.035`, Er 4.4 | 0.330 | 0.353 | 0.353 | — |
| JLCPCB 7628 drawing: `h = 0.2104`, `t = 0.035`, Er 4.4 | 0.350 | 0.372 | 0.373 | JLCPCB calculator, historical screen, with its mask and etch model: 0.3493 |
| IndieRF 7628 preset: `h = 0.1803` (7.1 mil), `t = 0.04064`, Er 4.4 | 0.287 | 0.312 | 0.313 | IndieRF bare: 0.313 |
| B: `h = 0.1000`, `t = 0.035`, Er 4.1 | 0.150 | 0.176 | 0.177 | — |
| JLCPCB 3313 drawing: `h = 0.0994`, `t = 0.035`, Er 4.1 | 0.149 | 0.175 | 0.176 | JLCPCB calculator, user report: 0.1565 |
| IndieRF 3313 preset: `h = 0.0889` (3.5 mil), `t = 0.04064`, Er 4.1 | 0.122 | 0.152 | 0.153 | IndieRF bare: 0.152 |

IndieRF's bare preset widths match Hammerstad–Jensen and Wheeler 1977 within 0.001 mm on IndieRF's own inputs. That suggests, but does not prove, that IndieRF uses a closed form of this family. JLCPCB's widths are narrower than the bare Hammerstad–Jensen widths on the drawing's prepreg heights: by 0.023 mm on 7628 and 0.018 mm on 3313. That is the direction solder mask and thicker finished copper push the result. The JLCPCB inputs behind those historical results are not fully disclosed, so the size of each contribution was not verified. The IPC logarithm happens to land near JLCPCB's widths, but it does so as a bare-line formula that is low against Hammerstad–Jensen. This is a coincidence of offsetting effects, not agreement.

## Review cross-checks

The review on 2026-11-18 recomputed every worked value independently and checked each formula against a second source where one could be read. Large PDFs (the Polar study, the IPC-2221A exhibit and TI's LVDS guide) could not be text-extracted in review. Claims resting only on those were not re-read.

| Formula | Source used in this file | Second-source check in review | Status |
| --- | --- | --- | --- |
| Hammerstad–Jensen microstrip with thickness correction | Qucs technical documentation | The Qucs page was re-read and confirms `Z0 = Z_L1(Wr)/sqrt(Eeff(Wr))`, with `W1` used only in the `Eeff` correction. No second written source of the equation text was read. Numerically, it reproduces the Polar study's method-of-moments reference of 50.63 Ω exactly. | Equation text from one source; **numerically cross-checked** |
| Wheeler 1977 microstrip | Wheeler 1977, via the Codex draft | The IPC-2141A errata prints the same `eta/(2*pi*sqrt(2*(E + 1)))` and `(14*E + 8)/(11*E)` structure, with `Eeff` in place of `Er`. It reproduces the Polar study's handbook value within 0.06 Ω. The thickness term `dw` was not cross-checked. | **Cross-checked** (impedance); thickness term **not cross-checked** |
| IPC `87/sqrt(Er + 1.41)` logarithm | IPC-2221A exhibit, TI | Not re-read in review, because the PDFs could not be extracted. It reproduces the Polar study's IPC value of 49.46 Ω within 0.01 Ω. The leading suite's article prints a different variant, `79*ln(4.98*H/(0.8*W + T))/sqrt(Er + 1.41)`, with no origin. [The leading suite's article](https://resources.altium.com/p/methods-calculating-and-measuring-impedance-part-1) | **Numerically cross-checked**; variants exist |
| Brooks embedded microstrip | Brooks PDF (HTTP 404 in review), Atmel (timed out in review) | Search-result summaries of Brooks's article and of calculators that implement it agree on the `1 - exp(-1.55*H/h)` factor. The IPC-2141A errata uses a different form. | **Partly cross-checked** (secondary only) |
| Cohn exact stripline | Cohn 1954, Polar | An independent implementation agrees with Wheeler 1978 at `t = 0` within 0.3%. | **Numerically cross-checked** |
| Wheeler 1978 stripline | Gupta, Garg and Chadha | The `m`, `dw` and `Z0` expressions match the reviewer's independent transcription of the same equations. It agrees with Cohn at `t = 0`. | **Cross-checked** |
| IPC centered stripline logarithm | IPC-2221A exhibit, TI | Not re-read. Its `4/(0.67*pi) = 1.90` form is widely quoted, but it was not re-read in a primary text. | **Not cross-checked** |
| Offset stripline logarithm | Atmel, IPC-2221A | The leading suite's article prints a different correction term, as explained in the offset section. IPC-2141A replaces both with an air-filled construction. | **Not cross-checked**; sources disagree |
| Kirschning–Jansen coupled microstrip | The paper (mirror), Qucs | The Qucs page was re-read. It confirms the range `0.1 ≤ u, g ≤ 10`, `1 ≤ Er ≤ 18`, better than 0.6% for the static impedances, and the `Q1`–`Q10`, `ao`, `bo`, `co`, `d` coefficients as written here. The IPC-2141A errata's coupled microstrip equation 4-24 has the same `Q10` structure. | **Cross-checked** |
| Jansen finite-thickness coupled correction | Qucs | None. | **Not cross-checked** |
| Cohn coupled stripline | Cohn 1955, Polar | None. Polar could not be read in review. | **Not cross-checked** |
| TI differential shortcuts (`0.48*exp(-0.96*s/h)`, `0.347*exp(-2.9*s/b)`) | TI, Flux | None. TI could not be read in review. | **Not cross-checked** |

Other claims were spot-checked against their sources in review:
- PCBWay's 2 Ω single-ended and 8 Ω differential reduction per mask pass is confirmed. The same page adds that two passes double the reduction and that three or more passes make no further change. [PCBWay](https://www.pcbway.com/pcb_prototype/Impedance_Control.html)
- JLCPCB's guide is confirmed on several points: 1.6 mil outer copper for 1 oz; mask Dk 3.8 at 0.6 mil over copper, 1.2 mil over the base and 1.2 mil between Tracks; a top width 0.7 mil narrower than the base; and no named solver. [JLCPCB](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator)
- AISLER's 295 µm for 50 Ω on Top referenced to IN1 is confirmed, with the "basic orientation" caveat. The prepreg thickness and Dk are only in an image, so they were not read. [AISLER](https://community.aisler.net/t/4-layers-1-6mm-35-m-stackup/5457)
- The IPC-2141A errata was read in full. It confirms the replaced equations 4-4, 4-7, 4-17, 4-19, 4-24, 4-42, 4-43 and 4-44. It also confirms the removed accuracy statements: 2% for surface microstrip, 1.5% and 1.3% for the symmetric stripline sets, 1% for wire microstrip, and the broadside-coupled "nearly exact" remark. [Errata](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf)

## Reuse in an MIT-licensed tool

The copyright distinction is the same for all six geometries. Under U.S. copyright law, an underlying mathematical method, principle or process is outside copyright protection. Its original written explanation, illustrations and software implementation can be protected. REDA can independently implement the published mathematics and license its own code under MIT. Access to an IPC standard does not grant permission to reproduce its text or figures. An existing GPL implementation has its own licence terms. The equations here are presented from papers, standards evidence and technical documentation. No calculator source code is needed. This addresses copyright, without making a patent-clearance determination. [U.S. Copyright Office, Circular 33](https://www.copyright.gov/circs/circ33.pdf) · [GNU GPL FAQ](https://www.gnu.org/licenses/gpl-faq.html) · [IPC publication notice in the public exhibit](https://s3-us-west-1.amazonaws.com/ptab-filings%2FIPR2024-01071%2F1011)

## Not verified

- **Not verified (secondary):** The exact printed coefficients and error definitions in the original Hammerstad–Jensen paper and Wheeler’s 1978 paper, and the detailed validation claimed in Wadell’s handbook. The accessible engineering transcriptions and independently published comparison are identified above. [Hammerstad–Jensen](https://doi.org/10.1109/MWSYM.1980.1124303) · [Wheeler 1978](https://doi.org/10.1109/TMTT.1978.1129505) · [Wadell](https://uk.artechhouse.com/Transmission-Line-Design-Handbook-P684.aspx)
- **Not verified (secondary):** Attribution of every short logarithmic or exponential expression to the corrected text of IPC-2141A. The IPC-2221A exhibit, the 1996 comparator and the revision-A errata resolve different parts of that question. No corresponding equation was verified in the full text of IPC-2221B or IPC-2251. [IPC-2141A errata](https://davemcglone.com/wp-content/uploads/2023/02/IPC-2141A_Errata.pdf) · [IPC-2221B contents](https://www.electronics.org/TOC/IPC-2221B.pdf) · [IPC-2251 contents](https://www.electronics.org/TOC/IPC-2251.pdf)
- No matched solver or measurement error envelope was found for the Brooks embedded logarithm, the simple offset logarithm, the one-pass mask decrement or the differential exponential shortcuts across the worked geometries. [Brooks](https://www-eng.lbl.gov/~shuman/NEXT/CURRENT_DESIGN/TP/MATERIALS/impedance_formula_derivations.pdf) · [Atmel](https://kr.element14.com/wcsstore/ExtendedSitesCatalogAssetStore/cms/asset/pdf/apac/common/applications/medical/appnotes/atmel/Atmel_SAM9XE128_Application_Notes_15.pdf) · [PCBWay](https://www.pcbway.com/pcb_prototype/Impedance_Control.html) · [TI](https://www.ti.com/lit/ug/snla187/snla187.pdf)
- No fresh JLCPCB or PCBWay result, named JLCPCB web-solver engine, matched two-Dk inner-Layer result, or impedance coupon measurement for a worked row was verified. The differing 50 Ω widths remain attributable to their particular sources and inputs. [JLCPCB calculator guide](https://jlcpcb.com/help/article/user-guide-to-the-jlcpcb-impedance-calculator) · [PCBWay calculator](https://www.pcbway.com/pcb_prototype/impedance_calculator.html) · [historical JLCPCB screen](https://digilib.unila.ac.id/91900/6/FILE%20SKRIPSI%20FULL%20TANPA%20BAB%20PEMBAHASAN.pdf)- **Added in review:** the Brooks embedded-microstrip PDF returned HTTP 404 on 2026-11-18, and the Atmel note timed out, so the embedded logarithm rests on search-result summaries. The offset-stripline correction term differs between the Atmel/IPC-2221A form and the leading suite's article, and neither could be checked against an original paper. The Polar study's table values, the IPC-2221A exhibit and TI's LVDS guide could not be text-extracted in review. Their numbers are supported only by the review's recomputation, which matches the quoted Polar values. [Review cross-checks](#review-cross-checks)
