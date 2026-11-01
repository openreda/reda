# Are the name REDA and the .reda, .redasch and .redapcb extensions free to use?

Research for issue #25, checked on 2026-11-01. This is a survey of public
sources, **not legal advice**. Before any commercial launch, or before filing a
trade mark, have a professional run a clearance search.

## Answer in short

| Item | Finding | Risk |
| --- | --- | --- |
| Name "REDA" for EDA software | The only live US REDA registration found that touches class 9/42 belongs to SLB (Schlumberger), for submersible pumps, power cables and pump engineering services. No REDA mark for software or EDA was found. Other REDA software products exist in unrelated markets. | **Low** |
| `reda-*` names on crates.io | Twelve `reda-*` crates already exist, from one author (MoleSir), and they are **IC-EDA** libraries (SPICE, Liberty, GDSII, LEF, Verilog). The bare `reda` crate and the names REDA would likely use are free. | **Medium** (confusion in the same field, not a legal risk) |
| GitHub org `openreda` / domains | Every `openreda` domain checked (.com .org .net .io .dev .app) is unregistered. All the `reda.*` domains checked are taken. | **Low**; register the `openreda` domains soon |
| `.reda`, `.redasch`, `.redapcb` | No listing in the extension databases checked, no MIME glob, and no use as a file extension on GitHub. | **Low** |
| EUIPO / WIPO / national registers | **Not verified.** Their search tools are interactive or behind captchas. Manual search links are below. | Unknown until checked |

## 1. Existing software and EDA tools named REDA

- **IC-EDA crates by MoleSir** (`reda-sp`, `reda-lib`, `reda-gds`, `reda-lef`, …). The
  GitHub repo [MoleSir/reda](https://github.com/MoleSir/reda) is described as an
  "EDA development kit in Rust", under the MIT licence, with 1 star. It was created
  2026-09-27. This is the closest overlap with REDA: the same name, language and
  broad field (chips rather than PCBs). See section 3.
- **rueda** ([ccc-eda/rueda](https://github.com/ccc-eda/rueda), "A EDA for IC design. (RUST)",
  created 2026-10-30) ships a binary called **`reda`**. Its crates.io description is "reda: one
  command for rueda's HDL tools (Veryl, Fluxa), synthesis, FPGA / ASIC flows and RISC-V tools"
  ([crates.io API](https://crates.io/api/v1/crates/rueda), `bin_names: ["reda"]`). If both
  tools are installed, a REDA command-line binary named `reda` would clash with it on `PATH`.
  Risk: **low to medium**. The project is a few days old and has 1 star.
- **REDA One** ([reda.one](https://reda.one/)): a Salesforce-based real-estate ERP from
  Princeton, NJ. Its site shows no ™ or ® next to REDA. The market is unrelated. Risk: **low**.
- **ReDA by Infonetica**: a clinical-trial management system on the UK G-Cloud marketplace
  ([listing](https://www.applytosupply.digitalmarketplace.service.gov.uk/g-cloud/services/919689781507125)).
  The market is unrelated. Risk: **low**.
- Other uses of the name: the PyPI package `reda`, "Reproducible Electrical Data Analysis"
  (geophysics), at [pypi.org/pypi/reda](https://pypi.org/pypi/reda/json); the CRAN package
  `reda`, "Recurrent Event Data Analysis", at
  [CRAN](https://cran.r-project.org/web/packages/reda/DESCRIPTION); and an empty npm
  placeholder `reda` from 2022 at [registry.npmjs.org/reda](https://registry.npmjs.org/reda).
  None of them is related to EDA.
- A general web search for an EDA product called REDA turned up nothing besides the two Rust
  IC projects above. The EDA tools that are well known (KiCad, Horizon EDA, LibrePCB and the
  commercial vendors) use no similar name.

## 2. Trade mark registers (classes 9 and 42)

### USPTO: partly verified

The REDA records below were read from the USPTO's own TSDR status pages. I could not run
the USPTO search tool itself from a script, so other REDA records may exist.

| Serial / Reg. | Mark | Owner | Status | Goods/services |
| --- | --- | --- | --- | --- |
| [85119398](https://tsdr.uspto.gov/#caseNumber=85119398&caseType=SERIAL_NO&searchType=statusSearch) / 4054908 | REDA | Schlumberger Technology Corp. | **Live**, renewed 2022 | Class 7: submersible pumps, drives and monitoring machines. Class 9: **power cables**. Class 37: pump repair. Class 42: **technical consulting, "designing, engineering and optimization … in the field of electrical submersible pumps"** |
| [79071623](https://tsdr.uspto.gov/#caseNumber=79071623&caseType=SERIAL_NO&searchType=statusSearch) / 3825285 | 1865 REDA (with a device) | Successori Reda S.p.A. | Live | Classes 24/25: fabrics and clothing |
| [77149027](https://tsdr.uspto.gov/#caseNumber=77149027&caseType=SERIAL_NO&searchType=statusSearch) / 3680234 | REDA | Nextbody LLC | Dead (cancelled 2011) | Food supplements |
| [74127990](https://tsdr.uspto.gov/#caseNumber=74127990&caseType=SERIAL_NO&searchType=statusSearch), [74132205](https://tsdr.uspto.gov/#caseNumber=74132205&caseType=SERIAL_NO&searchType=statusSearch) | REDA | Aldo Shoes | Dead | Jewellery, bags, footwear |
| [73002488](https://tsdr.uspto.gov/#caseNumber=73002488&caseType=SERIAL_NO&searchType=statusSearch) / 1016686 | REDA | TRW Inc. | Dead (expired 2006) | Motors, pumps |

How to read this: SLB's mark is a famous oilfield brand, first used in 1926. It is in
classes 9 and 42, but only for power cables and for engineering services *on submersible
pumps*. Software is not listed. EDA software for circuit boards is a different product sold
to different buyers, so confusion is unlikely. Still, a famous mark gets broader protection,
so REDA should never be presented in an oil, gas or pump context. Risk: **low**.

Canada's register also holds a REDA mark owned by Schlumberger Canada (application 354910,
from 1972), for pump units, switchgear and cable. I saw this only on a third-party mirror
and did not open the CIPO record.

### EUIPO, WIPO and national offices: not verified

EUIPO eSearch, TMview, the WIPO Global Brand Database and the UK IPO journal are interactive
or behind captchas, so I could not query them. One lead is still open. A UK Trade Mark
Journal entry for 27 Feb 2026 (international registration WO0000001901197, EU office of
origin, classes 3, 8, **9**, …, 28) turned up in a search for "Reda", but it was behind a
captcha. Its wording and owner are unknown. Check it by hand.

Manual searches to run. Use the word **REDA**, both exact and fuzzy, filtered to Nice
classes **9 and 42**. Then repeat with **OPENREDA**.

- TMview (EUIPO, the EU national offices and many others in one search):
  <https://www.tmdn.org/tmview/#/tmview/results?page=1&pageSize=30&criteria=C&basicSearch=REDA&niceClass=9,42>
- EUIPO eSearch plus: <https://euipo.europa.eu/eSearch/#basic/1+1+1+1/100+100+100+100/REDA>
- WIPO Global Brand Database (Madrid international registrations):
  <https://branddb.wipo.int/en/similarname?brandName=REDA&niceClass=9,42>
- WIPO Madrid Monitor: <https://www3.wipo.int/madrid/monitor/en/>, search for the mark REDA
- USPTO trademark search (to catch records missed above): <https://tmsearch.uspto.gov/search/search-results?query=REDA>
- UK IPO: <https://trademarks.ipo.gov.uk/ipo-tmtext>, search for REDA and open the WO0000001901197 entry
- Danish DKPTO (the home jurisdiction): <https://onlineweb.dkpto.dk/pvsonline/Varemaerke>, search for REDA

## 3. crates.io

Checked through the crates.io API (`/api/v1/crates/<name>` and `?q=reda`):

- **Free:** `reda`, `openreda`, `openreda-core`, `reda-core`, `reda-app`, `reda-cli`,
  `reda-model`, `reda-ui`, `reda-mcp`, `reda-sch`, `reda-pcb`, `reda-canvas`, `reda-render`,
  `reda-io`, `reda-kicad`.
- **Taken**, all owned by MoleSir and all IC-EDA libraries: `reda-gds`, `reda-geometry`,
  `reda-lef`, `reda-lib`, `reda-lib-macro`, `reda-sp`, `reda-spice`, `reda-spsim`, `reda-tf`,
  `reda-unit`, `reda-unit-macros`, `reda-v`. They were first published Aug to Oct 2025, the
  latest release is Dec 2025, and each has hundreds to under 2,000 downloads.

Risk: **medium**, but practical rather than legal. crates.io has no namespaces, so anyone
browsing `reda-*` will see two unrelated EDA projects mixed together, and REDA cannot use
names such as `reda-geometry` or `reda-lib`. Options: publish under the `reda-` prefix and
avoid the taken names; use an `openreda-` prefix (all free) for published crates; or don't
publish the internal crates at all.

## 4. Domains

RDAP lookups against each registry, run 2026-11-01:

| Domain | Status |
| --- | --- |
| reda.com | Registered 1995 |
| reda.net | Registered 2003 |
| reda.org | Registered 2002 |
| reda.io | Registered 2013 (a personal site) |
| reda.dev | Registered 2021 |
| reda.app | Registered 2024 |
| openreda.com / .org / .net / .io / .dev / .app | **Not registered** (RDAP 404) |

Risk: **low**, as long as the project builds on `openreda`, the name it already uses for
the GitHub org. Registering `openreda.org` and `openreda.dev` (and perhaps `.com`) early
would stop squatting. I did not check `.eu` and `.dk` (no RDAP endpoint, and no `whois`
installed here).

## 5. File extensions

- `fileinfo.com`, `filext.com` and `file-extensions.org` all return 404 / "not found" for
  `.reda`, `.redasch` and `.redapcb`. As a control, `fileinfo.com` and `filext.com` do list
  `.kicad_pcb`.
- freedesktop.org shared-mime-info (the local `/usr/share/mime/packages/freedesktop.org.xml`)
  has no glob for any of the three. IANA's media-type registry has nothing named `reda`.
- GitHub code search for `redasch` and `redapcb` finds them only as parts of other words (for
  example the surname "Redaschi") and in lists of expired domains. Nobody uses them as file
  extensions.
- Nearby extensions that could be confused: `.rda` (old R data) and `.red` (several unrelated
  formats). Neither is an exact match.

Risk: **low**. File extensions are not registered anywhere with legal force. Picking unique
ones mainly avoids desktop file-association clashes, and these three are unique. Once the
formats are stable, REDA could register MIME types for them (e.g.
`application/vnd.openreda.sch+json`) with freedesktop and IANA.

## What was not verified

- The EUIPO, WIPO/Madrid, UK, Danish and other national registers (manual links in §2).
- The full USPTO search. Only known REDA serials were read on TSDR.
- The `.eu` and `.dk` domains.
- Unregistered (common-law) use of REDA in other countries, beyond what web search shows.
