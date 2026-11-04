//! Laying out text as [`GlyphRun`]s in Document coordinates.

use std::sync::OnceLock;

use reda_model::geom::{Millideg, Nm, Point};
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
const LINE_PITCH: (i64, i64) = (8, 5);

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
