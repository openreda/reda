//! Laying out text as [`GlyphRun`]s in Document coordinates.

use std::sync::OnceLock;

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::net_name::NameGlyph;
use reda_text::{Font, Mesh, OutlineFont, Segment, layout};

use crate::scene::{Align, GlyphRun, HAlign, VAlign};
use crate::transform::rotate;

/// The bundled Noto Sans, the schematic text font, loaded once.
fn noto_sans() -> &'static OutlineFont {
    static FONT: OnceLock<OutlineFont> = OnceLock::new();
    FONT.get_or_init(OutlineFont::noto_sans)
}

/// How a run of text is placed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextStyle {
    pub height: Nm,
    pub rotation: Millideg,
    pub align: Align,
    pub view_label: bool,
}

/// Lines are this many capital heights apart (the same pitch as PCB text).
pub(crate) const LINE_PITCH: (i64, i64) = (8, 5);

/// Lays out `text` (`\n` starts a new line, below the first) in Noto Sans, aligned on `at`.
///
/// Text never reads upside down: a direction in (90°, 270°] is turned half a turn, with
/// the alignment mirrored so the text stays on the same side of its anchor.
pub(crate) fn glyph_run(text: &str, at: Point, style: TextStyle) -> GlyphRun {
    let (rotation, align) = readable(style.rotation, style.align);
    let font = noto_sans();
    let h = style.height.0;
    let dy = match align.v {
        VAlign::Baseline => 0,
        VAlign::Middle => -h / 2,
        VAlign::Top => -h,
    };
    let mut strokes = Vec::new();
    let mut fill = Mesh::default();
    for (i, line) in text.split('\n').enumerate() {
        let geometry = layout(line, Font::Outline(font), style.height, font);
        let dx = match align.h {
            HAlign::Left => 0,
            HAlign::Center => -geometry.advance.0 / 2,
            HAlign::Right => -geometry.advance.0,
        };
        let line_dy = dy - i as i64 * h * LINE_PITCH.0 / LINE_PITCH.1;
        let place = |p: Point| {
            let (x, y) = rotate(p.x.0 + dx, p.y.0 + line_dy, rotation);
            Point::new(at.x + Nm(x), at.y + Nm(y))
        };
        strokes.extend(
            geometry
                .strokes
                .iter()
                .map(|s| Segment::new(place(s.a), place(s.b))),
        );
        let base = u32::try_from(fill.points.len()).expect("text mesh fits u32 indices");
        fill.points
            .extend(geometry.fill.points.iter().map(|p| place(*p)));
        fill.indices
            .extend(geometry.fill.indices.iter().map(|i| i + base));
    }
    GlyphRun {
        text: text.to_owned(),
        at,
        height: style.height,
        rotation,
        align,
        view_label: style.view_label,
        strokes,
        stroke_width: Nm(0),
        fill,
    }
}

/// The overbars of one line of text laid out as [`glyph_run`] lays out `glyphs`' characters
/// on `at`: one segment over each run of negated characters, a little above the capitals.
pub(crate) fn overbars(glyphs: &[NameGlyph], at: Point, style: TextStyle) -> Vec<(Point, Point)> {
    let (rotation, align) = readable(style.rotation, style.align);
    let h = style.height.0;
    let dy = match align.v {
        VAlign::Baseline => 0,
        VAlign::Middle => -h / 2,
        VAlign::Top => -h,
    };
    let chars: Vec<char> = glyphs.iter().map(|g| g.ch).collect();
    let reach = |n: usize| advance(&chars[..n].iter().collect::<String>(), style.height).0;
    let dx = match align.h {
        HAlign::Left => 0,
        HAlign::Center => -reach(chars.len()) / 2,
        HAlign::Right => -reach(chars.len()),
    };
    let y = dy + h * OVERBAR_LIFT.0 / OVERBAR_LIFT.1;
    let place = |x: i64| {
        let (x, y) = rotate(x + dx, y, rotation);
        Point::new(at.x + Nm(x), at.y + Nm(y))
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < glyphs.len() {
        if !glyphs[i].overbar {
            i += 1;
            continue;
        }
        let start = i;
        while i < glyphs.len() && glyphs[i].overbar {
            i += 1;
        }
        out.push((place(reach(start)), place(reach(i))));
    }
    out
}

/// The box round the characters at byte range `range` of one line of `text`, laid out as
/// [`glyph_run`] lays it out on `at`: from a quarter of a capital height below the
/// baseline to a quarter above the capitals.
pub(crate) fn span(
    text: &str,
    range: std::ops::Range<usize>,
    at: Point,
    style: TextStyle,
) -> (Point, Point) {
    let (rotation, align) = readable(style.rotation, style.align);
    let h = style.height.0;
    let dy = match align.v {
        VAlign::Baseline => 0,
        VAlign::Middle => -h / 2,
        VAlign::Top => -h,
    };
    let reach = |s: &str| advance(s, style.height).0;
    let dx = match align.h {
        HAlign::Left => 0,
        HAlign::Center => -reach(text) / 2,
        HAlign::Right => -reach(text),
    };
    let (x0, x1) = (
        dx + reach(&text[..range.start]),
        dx + reach(&text[..range.end]),
    );
    let (y0, y1) = (dy - h / 4, dy + h + h / 4);
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| {
        let (x, y) = rotate(x, y, rotation);
        Point::new(at.x + Nm(x), at.y + Nm(y))
    });
    let lo = corners
        .iter()
        .fold(corners[0], |a, p| Point::new(a.x.min(p.x), a.y.min(p.y)));
    let hi = corners
        .iter()
        .fold(corners[0], |a, p| Point::new(a.x.max(p.x), a.y.max(p.y)));
    (lo, hi)
}

/// An overbar sits this many capital heights above the baseline.
const OVERBAR_LIFT: (i64, i64) = (23, 20);

/// How far the longest line of `text` reaches at capital height `height`.
pub(crate) fn advance(text: &str, height: Nm) -> Nm {
    let font = noto_sans();
    text.split('\n')
        .map(|line| layout(line, Font::Outline(font), height, font).advance)
        .max()
        .unwrap_or(Nm(0))
}

/// Whether text in direction `r` would read upside down.
fn upside_down(r: Millideg) -> bool {
    let r = r.normalized().0;
    r > 90_000 && r <= 270_000
}

/// The direction text along `r` reads in: `r`, or half a turn on when that reads upside
/// down.
pub(crate) fn reading_direction(r: Millideg) -> Millideg {
    if upside_down(r) {
        (r + Millideg(180_000)).normalized()
    } else {
        r.normalized()
    }
}

fn readable(rotation: Millideg, align: Align) -> (Millideg, Align) {
    let r = rotation.normalized();
    if !upside_down(r) {
        return (r, align);
    }
    let h = match align.h {
        HAlign::Left => HAlign::Right,
        HAlign::Center => HAlign::Center,
        HAlign::Right => HAlign::Left,
    };
    let v = match align.v {
        VAlign::Baseline => VAlign::Top,
        VAlign::Middle => VAlign::Middle,
        VAlign::Top => VAlign::Baseline,
    };
    ((r + Millideg(180_000)).normalized(), Align::new(h, v))
}
