//! Text geometry on the board: a PCB text object laid out by `reda-text` in its font, then
//! split into lines, mirrored, rotated and moved into place. Gerber, PDF and the canvas all
//! draw from this, so they agree glyph for glyph.

mod footprint;

use std::sync::OnceLock;

use reda_command::Fonts;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::text::display;
use reda_model::{HERSHEY_FONT, NOTO_SANS_FONT, PcbDocument, PcbText, Uuid};
use reda_text::{Font, OutlineFont, StrokeFont, layout};
use schemars::JsonSchema;
use serde::Serialize;

pub use footprint::{FOOTPRINT_TEXT_PEN, board_layer, element_texts, footprint_texts};

/// The distance from one baseline to the next, as a multiple of the capital height
/// (`LINE_PITCH.0 / LINE_PITCH.1`).
pub const LINE_PITCH: (i64, i64) = (8, 5);

/// A text object's geometry in board coordinates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PlacedText {
    /// The text object.
    pub uuid: Uuid,
    /// Its Layer.
    pub layer: Uuid,
    /// The font family the text names.
    pub font: String,
    /// The family it was drawn in: the one it names, or Noto Sans when that is missing.
    pub drawn_font: String,
    /// Whether the family the text names is missing on this machine.
    pub missing_font: bool,
    /// The pen width of the strokes.
    pub stroke_width: Nm,
    /// Stroke-font glyphs: capsules of `stroke_width` from `a` to `b`.
    pub strokes: Vec<Stroke>,
    /// Outline-font glyphs (including characters the stroke font lacks), filled.
    pub fill: Fill,
}

/// One pen stroke, drawn as a capsule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Stroke {
    pub a: Point,
    pub b: Point,
}

/// A triangle list: every three `indices` name one triangle in `points`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Fill {
    pub points: Vec<Point>,
    pub indices: Vec<u32>,
}

fn hershey() -> &'static StrokeFont {
    static FONT: OnceLock<StrokeFont> = OnceLock::new();
    FONT.get_or_init(StrokeFont::hershey_roman_simplex)
}

fn noto_sans() -> &'static OutlineFont {
    static FONT: OnceLock<OutlineFont> = OnceLock::new();
    FONT.get_or_init(OutlineFont::noto_sans)
}

/// A font a text object resolved to.
enum Resolved {
    Hershey,
    NotoSans,
    Installed(String, Box<OutlineFont>),
    Missing,
}

fn resolve(family: &str, fonts: &Fonts) -> Resolved {
    if family.eq_ignore_ascii_case(HERSHEY_FONT) {
        return Resolved::Hershey;
    }
    if family.eq_ignore_ascii_case(NOTO_SANS_FONT) {
        return Resolved::NotoSans;
    }
    let Some(face) = fonts.get(family) else {
        return Resolved::Missing;
    };
    match OutlineFont::from_collection(face.bytes.clone(), face.index) {
        Ok(font) => Resolved::Installed(face.family.clone(), Box::new(font)),
        Err(_) => Resolved::Missing,
    }
}

/// Whether the family is missing on this machine: neither built in nor handed to the
/// engine as loadable bytes.
pub fn font_is_missing(family: &str, fonts: &Fonts) -> bool {
    matches!(resolve(family, fonts), Resolved::Missing)
}

/// The font families PCB text names that are missing on this machine, sorted, each once.
/// Their text draws as Noto Sans; Messages and Output Jobs warn about them.
pub fn missing_fonts(pcb: &PcbDocument, fonts: &Fonts) -> Vec<String> {
    let mut missing: Vec<String> = pcb
        .texts()
        .map(|t| t.font.clone())
        .filter(|f| font_is_missing(f, fonts))
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

/// Lays out `text` in its font (or Noto Sans when that is missing) and places it on the
/// board.
pub fn text_geometry(text: &PcbText, fonts: &Fonts) -> PlacedText {
    let resolved = resolve(&text.font, fonts);
    let (font, drawn_font) = match &resolved {
        Resolved::Hershey => (Font::Stroke(hershey()), HERSHEY_FONT.to_owned()),
        Resolved::NotoSans | Resolved::Missing => {
            (Font::Outline(noto_sans()), NOTO_SANS_FONT.to_owned())
        }
        Resolved::Installed(family, font) => (Font::Outline(font), family.clone()),
    };
    let place = Placement::new(text);
    let pitch = text.height.0 * LINE_PITCH.0 / LINE_PITCH.1;
    let mut strokes = Vec::new();
    let mut fill = Fill::default();
    for (i, line) in display(&text.text).split('\n').enumerate() {
        let drop = pitch * i as i64;
        let at = |p: Point| place.apply(Point::new(p.x, p.y - Nm(drop)));
        let geometry = layout(line, font, text.height, noto_sans());
        strokes.extend(geometry.strokes.iter().map(|s| Stroke {
            a: at(s.a),
            b: at(s.b),
        }));
        let base = u32::try_from(fill.points.len()).expect("text fits u32 indices");
        fill.points
            .extend(geometry.fill.points.iter().map(|&p| at(p)));
        fill.indices
            .extend(geometry.fill.indices.iter().map(|i| i + base));
    }
    PlacedText {
        uuid: text.uuid,
        layer: text.layer,
        font: text.font.clone(),
        drawn_font,
        missing_font: matches!(resolved, Resolved::Missing),
        stroke_width: text.thickness,
        strokes,
        fill,
    }
}

/// Mirror, then rotate, then move: text-local coordinates (y-up, origin at the start of the
/// first baseline) to the board.
struct Placement {
    origin: Point,
    mirrored: bool,
    turn: Turn,
}

/// A rotation; quarter turns are exact, others are rounded to the nanometre.
enum Turn {
    Quarter(u8),
    Angle(f64, f64),
}

impl Placement {
    fn new(text: &PcbText) -> Placement {
        let r = text.rotation.normalized().0;
        let turn = if r % 90_000 == 0 {
            Turn::Quarter((r / 90_000) as u8)
        } else {
            let rad = (r as f64 / Millideg::TURN.0 as f64) * std::f64::consts::TAU;
            Turn::Angle(rad.cos(), rad.sin())
        };
        Placement {
            origin: text.position,
            mirrored: text.mirrored,
            turn,
        }
    }

    fn apply(&self, p: Point) -> Point {
        let (x, y) = (if self.mirrored { -p.x.0 } else { p.x.0 }, p.y.0);
        let (x, y) = match self.turn {
            Turn::Quarter(0) => (x, y),
            Turn::Quarter(1) => (-y, x),
            Turn::Quarter(2) => (-x, -y),
            Turn::Quarter(_) => (y, -x),
            Turn::Angle(cos, sin) => {
                let (fx, fy) = (x as f64, y as f64);
                (
                    (fx * cos - fy * sin).round() as i64,
                    (fx * sin + fy * cos).round() as i64,
                )
            }
        };
        Point::new(self.origin.x + Nm(x), self.origin.y + Nm(y))
    }
}
