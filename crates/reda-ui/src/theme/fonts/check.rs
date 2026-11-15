//! Looking inside a font file before egui gets it: whether it can be drawn at all, and
//! whether its digits are one width.
//!
//! egui parses a font lazily and panics during layout on a file it cannot read, so every
//! installed font passes [`validate`] first (app.md#preferences).

use std::fmt;

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::{FontRef, TableProvider as _};
use skrifa::{GlyphId, MetadataProvider as _};

/// Why a font file is not handed to egui.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Invalid {
    /// Not an OpenType font, or it has no face at that index.
    NotAFont,
    /// A table every font needs is missing or cut short.
    BrokenTables,
    /// It has no glyph for basic Latin letters and digits.
    NoLatin,
    /// It has no outlines to draw (a bitmap-only font), or they cannot be read.
    NoOutlines,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Invalid::NotAFont => "not an OpenType font",
            Invalid::BrokenTables => "a required table is missing or damaged",
            Invalid::NoLatin => "no basic Latin letters and digits",
            Invalid::NoOutlines => "no glyph outlines",
        })
    }
}

/// The characters a chrome font must have, and whose outlines are drawn as a check.
const PROBE: &str = "Aag0189 ";

/// Whether face `index` of `bytes` is safe and useful to hand to egui: it parses with the
/// call egui makes, has sane metrics, and maps and draws basic Latin.
pub fn validate(bytes: &[u8], index: u32) -> Result<(), Invalid> {
    let font = FontRef::from_index(bytes, index).map_err(|_| Invalid::NotAFont)?;
    // The tables layout reads: all must be there and whole.
    let (Ok(head), Ok(hhea), Ok(_), Ok(maxp), Ok(_)) = (
        font.head(),
        font.hhea(),
        font.hmtx(),
        font.maxp(),
        font.cmap(),
    ) else {
        return Err(Invalid::BrokenTables);
    };
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let height = metrics.ascent - metrics.descent;
    if head.units_per_em() == 0
        || maxp.num_glyphs() == 0
        || hhea.number_of_h_metrics() == 0
        || !height.is_finite()
        || height <= 0.0
    {
        return Err(Invalid::BrokenTables);
    }

    let charmap = font.charmap();
    let outlines = font.outline_glyphs();
    let advances = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    for ch in PROBE.chars() {
        let glyph = charmap
            .map(ch)
            .filter(|g| *g != GlyphId::NOTDEF)
            .ok_or(Invalid::NoLatin)?;
        let advance = advances.advance_width(glyph).ok_or(Invalid::BrokenTables)?;
        if !advance.is_finite() {
            return Err(Invalid::BrokenTables);
        }
        let outline = outlines.get(glyph).ok_or(Invalid::NoOutlines)?;
        let mut pen = Ink(false);
        outline
            .draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                &mut pen,
            )
            .map_err(|_| Invalid::NoOutlines)?;
        if !pen.0 && !ch.is_whitespace() {
            return Err(Invalid::NoOutlines);
        }
    }
    Ok(())
}

/// Whether the digits of face `index` all advance by one width, so numbers line up in
/// columns. False for a font that cannot be read.
pub fn tabular_digits(bytes: &[u8], index: u32) -> bool {
    let Ok(font) = FontRef::from_index(bytes, index) else {
        return false;
    };
    let charmap = font.charmap();
    let advances = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let mut widths = ('0'..='9').map(|ch| advances.advance_width(charmap.map(ch)?));
    match widths.next() {
        Some(Some(first)) => widths.all(|w| w == Some(first)),
        _ => false,
    }
}

/// Records whether an outline drew anything.
struct Ink(bool);

impl OutlinePen for Ink {
    fn move_to(&mut self, _: f32, _: f32) {}
    fn line_to(&mut self, _: f32, _: f32) {
        self.0 = true;
    }
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
        self.0 = true;
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        self.0 = true;
    }
    fn close(&mut self) {}
}
