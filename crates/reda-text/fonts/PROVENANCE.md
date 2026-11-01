# Vendored fonts

Bundled under the font licences allowed by [ADR-0002](../../../docs/adr/0002-permissive-licensing-and-third-party-data.md). Each font's licence text sits next to it and must ship with REDA, in the NOTICES file and the About box. Retrieved 2026-11-01. The files are unmodified.

## Hershey Roman Simplex: `hershey/rowmans.jhf`

- **What:** Dr. A. V. Hershey's Roman Simplex, in James Hurt's `.jhf` format from the Usenet distribution. The records keep their original Hershey glyph numbers (501–526 for capitals, 601–626 for lower case) and map in order to ASCII 32–127.
- **From:** <https://github.com/kamalmostafa/hershey-fonts>, file `hershey-fonts/rowmans.jhf` at commit `1356bf2f83d380fcef68c887e88675eb9d445d86`. Only the `.jhf` data is taken; that repository's own code is GPL and none of it is used.
- **SHA-256:** `8718fb129c0f6bce89c84fe41bc467e39534d215a6f7c3220cc5789a8a7d8618`
- **Licence:** the Hershey font terms in `hershey/LICENSE.txt`, copied verbatim from `hershey.txt` of the original distribution. Any use is allowed provided the acknowledgements to Dr. A. V. Hershey and James Hurt ship with the data, and the data is never converted to the NTIS format.

## Noto Sans Regular: `noto-sans/NotoSans-Regular.ttf`

- **What:** Noto Sans Regular, version 2.015, unhinted static TTF (Latin, Greek and Cyrillic).
- **From:** <https://github.com/notofonts/notofonts.github.io>, file `fonts/NotoSans/unhinted/ttf/NotoSans-Regular.ttf` at commit `e2ec4a11da38a23b1ecebfcb233833a153ecb841`.
- **SHA-256:** `f3961a9cde016d41a4879aecda1474d3a36d6bf54fa0e4643de029cc2248b0e8`
- **Licence:** SIL Open Font License 1.1, in `noto-sans/OFL.txt`, from <https://github.com/notofonts/latin-greek-cyrillic> at commit `0ef94805d19e98ed658543a7c48c0a4f19cc0ac0`. The header declares no Reserved Font Name.
