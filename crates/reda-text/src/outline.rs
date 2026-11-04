//! TTF/OTF fonts loaded from bytes.
//!
//! This is the only module that touches `skrifa` and `harfrust`, whose APIs change between
//! minor versions, so an upgrade stays in one place.

use std::fmt;
use std::sync::Arc;

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

const NOTO_SANS: &[u8] = include_bytes!("../fonts/noto-sans/NotoSans-Regular.ttf");

/// A TTF or OTF font. Holds the bytes plus the shaping tables built from them.
pub struct OutlineFont {
    data: Arc<[u8]>,
    index: u32,
    shaper_data: harfrust::ShaperData,
    units_per_em: u16,
    cap_height: u16,
}

/// Why font bytes could not be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontError {
    /// The bytes are not a font this crate can read (or the collection index is out of range).
    Unreadable,
    /// The font has no usable `head` table.
    MissingHead,
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::Unreadable => f.write_str("not a readable TTF/OTF font"),
            FontError::MissingHead => f.write_str("font has no usable head table"),
        }
    }
}

impl std::error::Error for FontError {}

impl fmt::Debug for OutlineFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutlineFont")
            .field("bytes", &self.data.len())
            .field("index", &self.index)
            .field("units_per_em", &self.units_per_em)
            .field("cap_height", &self.cap_height)
            .finish_non_exhaustive()
    }
}

impl OutlineFont {
    /// The bundled Noto Sans Regular, the schematic default and the Hershey fallback.
    pub fn noto_sans() -> OutlineFont {
        OutlineFont::from_bytes(NOTO_SANS).expect("the bundled Noto Sans loads")
    }

    /// Loads a single font (TTF/OTF), or the first face of a collection.
    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Result<OutlineFont, FontError> {
        OutlineFont::from_collection(bytes, 0)
    }

    /// Loads face `index` of a font collection (TTC/OTC); index 0 also reads a single font.
    pub fn from_collection(
        bytes: impl Into<Arc<[u8]>>,
        index: u32,
    ) -> Result<OutlineFont, FontError> {
        let data: Arc<[u8]> = bytes.into();
        let font = FontRef::from_index(&data, index).map_err(|_| FontError::Unreadable)?;
        let units_per_em = font
            .head()
            .map_err(|_| FontError::MissingHead)?
            .units_per_em();
        if units_per_em == 0 {
            return Err(FontError::MissingHead);
        }
        let cap_height = font
            .os2()
            .ok()
            .and_then(|os2| os2.s_cap_height())
            .and_then(|h| u16::try_from(h).ok())
            .filter(|&h| h > 0)
            // Without an OS/2 cap height, assume the common 0.7 em.
            .unwrap_or_else(|| (u32::from(units_per_em) * 7 / 10) as u16);
        let shaper_data = harfrust::ShaperData::new(&font);
        Ok(OutlineFont {
            data,
            index,
            shaper_data,
            units_per_em,
            cap_height,
        })
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Capital height in font units: the text height maps to this.
    pub fn cap_height(&self) -> u16 {
        self.cap_height
    }

    fn font_ref(&self) -> FontRef<'_> {
        FontRef::from_index(&self.data, self.index).expect("checked when loaded")
    }

    /// Shapes one run of text. Positions are in font units, y-up, from the run's origin.
    pub(crate) fn shape(&self, text: &str) -> Shaped {
        let font = self.font_ref();
        let shaper = self.shaper_data.shaper(&font).build();
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let shaped = shaper.shape(buffer, harfrust::ShapeOptions::new());
        let mut pen = (0i64, 0i64);
        let mut glyphs = Vec::with_capacity(shaped.len());
        for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            glyphs.push(PlacedGlyph {
                id: info.glyph_id,
                x: pen.0 + i64::from(pos.x_offset),
                y: pen.1 + i64::from(pos.y_offset),
            });
            pen.0 += i64::from(pos.x_advance);
            pen.1 += i64::from(pos.y_advance);
        }
        Shaped {
            glyphs,
            advance: pen.0,
        }
    }

    /// The glyph's outline flattened to closed polygons, in font units, y-up. Empty for blank
    /// glyphs and for ids the font does not have.
    pub(crate) fn contours(&self, glyph_id: u32) -> Vec<Vec<(f32, f32)>> {
        let font = self.font_ref();
        let Some(glyph) = font.outline_glyphs().get(GlyphId::new(glyph_id)) else {
            return Vec::new();
        };
        let mut pen = Flattener::new(f32::from(self.units_per_em) / FLATNESS_PER_EM);
        let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
        if glyph.draw(settings, &mut pen).is_err() {
            return Vec::new();
        }
        pen.finish()
    }
}

/// The flattening tolerance is the em divided by this: 0.5 µm at a 1 mm em.
const FLATNESS_PER_EM: f32 = 2000.0;

/// A shaped run: glyph ids and their origins in font units.
pub(crate) struct Shaped {
    pub glyphs: Vec<PlacedGlyph>,
    pub advance: i64,
}

pub(crate) struct PlacedGlyph {
    pub id: u32,
    pub x: i64,
    pub y: i64,
}

/// An outline pen that flattens quadratic and cubic curves into polylines within a tolerance.
struct Flattener {
    tolerance: f32,
    contours: Vec<Vec<(f32, f32)>>,
    current: Vec<(f32, f32)>,
}

impl Flattener {
    fn new(tolerance: f32) -> Flattener {
        Flattener {
            tolerance,
            contours: Vec::new(),
            current: Vec::new(),
        }
    }

    fn last(&self) -> (f32, f32) {
        self.current.last().copied().unwrap_or((0.0, 0.0))
    }

    /// Segments needed so a curve whose control polygon bends by `bend` stays within tolerance.
    fn segments(&self, bend: f32) -> u32 {
        let n = (bend / self.tolerance).sqrt().ceil();
        (n as u32).clamp(1, 64)
    }

    fn finish(mut self) -> Vec<Vec<(f32, f32)>> {
        self.close();
        self.contours
    }
}

fn len(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

impl OutlinePen for Flattener {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.current.push((x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push((x, y));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        // A chord's error is |p0 - 2c + p1| / 4, shrinking with the square of the segments.
        let n = self.segments(len(x0 - 2.0 * cx + x, y0 - 2.0 * cy + y) / 4.0);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            self.current.push((
                u * u * x0 + 2.0 * u * t * cx + t * t * x,
                u * u * y0 + 2.0 * u * t * cy + t * t * y,
            ));
        }
    }

    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        let bend = len(x0 - 2.0 * c0x + c1x, y0 - 2.0 * c0y + c1y)
            .max(len(c0x - 2.0 * c1x + x, c0y - 2.0 * c1y + y));
        let n = self.segments(bend * 3.0 / 4.0);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.current.push((
                a * x0 + b * c0x + c * c1x + d * x,
                a * y0 + b * c0y + c * c1y + d * y,
            ));
        }
    }

    fn close(&mut self) {
        if self.current.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }
}
