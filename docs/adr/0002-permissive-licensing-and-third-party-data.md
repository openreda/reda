# Permissive dependencies only; third-party library data is supported, never shipped

REDA is MIT-licensed. Dependencies must be permissively licensed (MIT, Apache-2.0, BSD, zlib, ISC; MPL-2.0 case by case) and no GPL or LGPL code may be linked or copied. GPL EDA tools (KiCad, LibrePCB, Horizon EDA) are references for ideas only. REDA may read third-party library data and formats (for example KiCad's symbol and footprint libraries) that users obtain themselves, but does not bundle or redistribute that data, because its licences (e.g. CC-BY-SA with exception) would bind REDA's distribution. Enforced with `cargo-deny` once dependencies exist.

## Bundled font data

Font data that REDA bundles may also come under these licences, because no font REDA needs is available under the code licences above:

- **Hershey font terms** for the Hershey Roman Simplex stroke font, REDA's default PCB text font. Any use is allowed, commercial or not, provided the acknowledgements to Dr. A. V. Hershey (U.S. National Bureau of Standards) and James Hurt (Cognition, Inc.) ship with the data, and the data is never converted to the format distributed by the U.S. NTIS. KiCad's newstroke font, a Hershey derivative, is not used: KiCad's copy carries a GPL notice, and the other licence statements for it conflict.
- **OFL-1.1** for the bundled Noto Sans outline font.
- **Ubuntu-font-1.0**, which arrives with eframe's default UI fonts.

The required acknowledgements and licence texts go in a generated NOTICES file and the About box. `cargo-deny` carries matching exceptions. REDA draws its own glyphs (MIT, in the repo) for characters the Hershey data lacks; until then those characters fall back to Noto Sans outlines.
