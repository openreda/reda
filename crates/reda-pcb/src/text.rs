//! Text geometry on the board: a PCB text object laid out by `reda-text` in its font, then
//! split into lines, mirrored, rotated and moved into place. Gerber, PDF and the canvas all
//! draw from this, so they agree glyph for glyph.

mod footprint;
mod knockout;

use std::sync::OnceLock;

use reda_command::Fonts;
use reda_model::geom::{Bounds, Millideg, Nm, Point};
use reda_model::text::display;
use reda_model::{
    HERSHEY_FONT, NOTO_SANS_FONT, PcbDocument, PcbText, TextHJustify, TextVJustify, Uuid,
};
use reda_text::{Font, OutlineFont, StrokeFont, layout};
use schemars::JsonSchema;
use serde::Serialize;

pub use footprint::{
    FOOTPRINT_TEXT_PEN, board_layer, element_texts, footprint_texts, placed_field_texts,
};

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
    /// Wrapped lines omitted because their whole capital height or width did not fit.
    pub dropped_lines: usize,
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
        .chain(
            pcb.footprints()
                .flat_map(|instance| footprint_texts(pcb, instance, fonts))
                .map(|t| t.font.clone()),
        )
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
    let displayed = display(&text.text);
    let mut lines: Vec<String> = displayed.split('\n').map(str::to_owned).collect();
    let mut dropped_lines = 0;
    if let Some(frame) = text.frame {
        lines = lines
            .into_iter()
            .flat_map(|line| wrap_line(&line, frame.width, font, text.height))
            .collect();
        // Only a single glyph wider than the frame is left too wide; its line is dropped
        // without leaving a gap.
        let wrapped = lines.len();
        lines.retain(|line| layout(line, font, text.height, noto_sans()).advance <= frame.width);
        dropped_lines = wrapped - lines.len();
        let capacity = if frame.height < text.height {
            0
        } else {
            1 + (frame.height.0 - text.height.0) / pitch
        } as usize;
        dropped_lines += lines.len().saturating_sub(capacity);
        lines.truncate(capacity);
    }
    let block_height = if lines.is_empty() {
        0
    } else {
        text.height.0 + pitch * (lines.len() as i64 - 1)
    };
    let (left, bottom, width, box_height) = text.frame.map_or((0, 0, 0, 0), |frame| {
        let left = match text.justify.h {
            TextHJustify::Left => 0,
            TextHJustify::Center => -frame.width.0 / 2,
            TextHJustify::Right => -frame.width.0,
        };
        let bottom = match text.justify.v {
            TextVJustify::Bottom => 0,
            TextVJustify::Center => -frame.height.0 / 2,
            TextVJustify::Top => -frame.height.0,
        };
        (left, bottom, frame.width.0, frame.height.0)
    });
    let top = if text.frame.is_some() {
        bottom
            + match text.justify.v {
                TextVJustify::Top => box_height,
                TextVJustify::Center => (box_height + block_height) / 2,
                TextVJustify::Bottom => block_height,
            }
    } else {
        match text.justify.v {
            TextVJustify::Top => 0,
            TextVJustify::Center => block_height / 2,
            TextVJustify::Bottom => block_height,
        }
    };
    for (i, line) in lines.iter().enumerate() {
        let geometry = layout(line, font, text.height, noto_sans());
        let dx = left
            + match text.justify.h {
                TextHJustify::Left => 0,
                TextHJustify::Center => (width - geometry.advance.0) / 2,
                TextHJustify::Right => width - geometry.advance.0,
            };
        let baseline = top - text.height.0 - pitch * i as i64;
        let at = |p: Point| Point::new(p.x + Nm(dx), p.y + Nm(baseline));
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
    if let Some(knockout) = text.knockout {
        let frame = text.frame.map(|frame| Bounds {
            min: Point::new(Nm(left), Nm(bottom)),
            max: Point::new(Nm(left + frame.width.0), Nm(bottom + frame.height.0)),
        });
        fill = knockout::flatten(&strokes, text.thickness, &fill, frame, knockout.margin);
        strokes.clear();
    }
    for stroke in &mut strokes {
        stroke.a = place.apply(stroke.a);
        stroke.b = place.apply(stroke.b);
    }
    for point in &mut fill.points {
        *point = place.apply(*point);
    }
    PlacedText {
        uuid: text.uuid,
        layer: text.layer,
        font: text.font.clone(),
        drawn_font,
        missing_font: matches!(resolved, Resolved::Missing),
        dropped_lines,
        stroke_width: text.thickness,
        strokes,
        fill,
    }
}

/// Wrap at whitespace where possible, otherwise between complete Unicode characters.
fn wrap_line(line: &str, width: Nm, font: Font<'_>, height: Nm) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for ch in line.chars() {
        current.push(ch);
        while layout(&current, font, height, noto_sans()).advance > width {
            let Some((last, _)) = current.char_indices().next_back().filter(|(at, _)| *at > 0)
            else {
                break;
            };
            if let Some((at, _)) = current
                .char_indices()
                .rev()
                .find(|(at, c)| *at > 0 && c.is_whitespace())
            {
                lines.push(current[..at].trim_end().to_owned());
                current = current[at..].trim_start().to_owned();
            } else {
                lines.push(current[..last].to_owned());
                current = current[last..].to_owned();
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
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
