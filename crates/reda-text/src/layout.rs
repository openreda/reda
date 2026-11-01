//! Turning a string, a font and a height into [`TextGeometry`].

use i_triangle::i_overlay::core::fill_rule::FillRule;
use i_triangle::i_overlay::core::simplify::Simplify;
use i_triangle::i_overlay::i_float::int::point::IntPoint;
use i_triangle::int::unchecked::IntUncheckedTriangulatable;
use reda_geom::{Nm, Point};

use crate::geometry::{Mesh, Segment, TextGeometry};
use crate::outline::OutlineFont;
use crate::stroke::{StrokeFont, StrokeGlyph};

/// The font a string is laid out in.
#[derive(Clone, Copy, Debug)]
pub enum Font<'a> {
    /// A Hershey stroke font, drawn as capsules.
    Stroke(&'a StrokeFont),
    /// A TTF/OTF font, shaped and filled.
    Outline(&'a OutlineFont),
}

/// Lays out one line of `text` with its capital height at `height`.
///
/// Characters a stroke font lacks are shaped in `fallback` (normally Noto Sans), glyph by
/// glyph. Characters an outline font lacks draw as its `.notdef` glyph.
pub fn layout(text: &str, font: Font<'_>, height: Nm, fallback: &OutlineFont) -> TextGeometry {
    let mut out = TextGeometry::default();
    match font {
        Font::Stroke(stroke) => layout_stroke(&mut out, text, stroke, height, fallback),
        Font::Outline(outline) => {
            out.advance = Nm(push_outline_run(&mut out.fill, text, outline, height, 0));
        }
    }
    out
}

fn layout_stroke(
    out: &mut TextGeometry,
    text: &str,
    font: &StrokeFont,
    height: Nm,
    fallback: &OutlineFont,
) {
    let scale = Scale::new(height.0, i64::from(font.cap_height()));
    // The pen is kept as Hershey units plus nm from fallback runs, so stroke positions are
    // rounded once rather than accumulating per-glyph rounding.
    let mut units: i64 = 0;
    let mut fallback_nm: i64 = 0;
    let mut rest = text;
    while !rest.is_empty() {
        let missing = rest
            .char_indices()
            .find(|&(_, c)| font.glyph(c).is_some())
            .map_or(rest.len(), |(i, _)| i);
        if missing > 0 {
            let origin = scale.apply(units) + fallback_nm;
            fallback_nm +=
                push_outline_run(&mut out.fill, &rest[..missing], fallback, height, origin);
            rest = &rest[missing..];
            continue;
        }
        let c = rest.chars().next().expect("rest is not empty");
        let glyph = font.glyph(c).expect("checked above");
        push_stroke_glyph(out, glyph, font.baseline(), units, fallback_nm, scale);
        units += i64::from(glyph.advance());
        rest = &rest[c.len_utf8()..];
    }
    out.advance = Nm(scale.apply(units) + fallback_nm);
}

fn push_stroke_glyph(
    out: &mut TextGeometry,
    glyph: &StrokeGlyph,
    baseline: i32,
    units: i64,
    offset_nm: i64,
    scale: Scale,
) {
    let to_nm = |(x, y): (i32, i32)| {
        let ux = units + i64::from(x - glyph.left);
        let uy = i64::from(baseline - y);
        Point::new(Nm(scale.apply(ux) + offset_nm), Nm(scale.apply(uy)))
    };
    for stroke in &glyph.strokes {
        out.strokes.extend(
            stroke
                .windows(2)
                .map(|w| Segment::new(to_nm(w[0]), to_nm(w[1]))),
        );
    }
}

/// Shapes `text` in `font`, fills each glyph into `mesh` starting at `origin_x` nm, and
/// returns the run's advance in nm.
fn push_outline_run(
    mesh: &mut Mesh,
    text: &str,
    font: &OutlineFont,
    height: Nm,
    origin_x: i64,
) -> i64 {
    let scale = Scale::new(height.0, i64::from(font.cap_height()));
    let shaped = font.shape(text);
    for glyph in &shaped.glyphs {
        let contours = font.contours(glyph.id);
        if contours.is_empty() {
            continue;
        }
        let to_nm = |(x, y): (f32, f32)| {
            let px = scale.apply_f(glyph.x as f64 + f64::from(x)) + origin_x;
            let py = scale.apply_f(glyph.y as f64 + f64::from(y));
            IntPoint::new(px, py)
        };
        let shape: Vec<Vec<IntPoint<i64>>> = contours
            .iter()
            .map(|c| c.iter().copied().map(to_nm).collect())
            .collect();
        mesh.append(triangulate(&shape));
    }
    scale.apply(shaped.advance)
}

/// Resolves overlaps and winding with a non-zero union, then triangulates.
fn triangulate(contours: &[Vec<IntPoint<i64>>]) -> Mesh {
    let shapes = contours.simplify(FillRule::NonZero, Default::default());
    let triangulation = shapes.uncheck_triangulate().into_triangulation::<u32>();
    Mesh {
        points: triangulation
            .points
            .iter()
            .map(|p| Point::new(Nm(p.x), Nm(p.y)))
            .collect(),
        indices: triangulation.indices,
    }
}

/// `v * num / den`, rounded half away from zero, in i128 so large heights cannot overflow.
#[derive(Clone, Copy, Debug)]
struct Scale {
    num: i64,
    den: i64,
}

impl Scale {
    fn new(num: i64, den: i64) -> Scale {
        assert!(den > 0, "scale denominator must be positive");
        Scale { num, den }
    }

    fn apply(self, v: i64) -> i64 {
        let p = i128::from(v) * i128::from(self.num);
        let d = i128::from(self.den);
        let q = if p >= 0 {
            (p + d / 2) / d
        } else {
            (p - d / 2) / d
        };
        q as i64
    }

    /// The same for fractional font units from flattened curves.
    fn apply_f(self, v: f64) -> i64 {
        (v * self.num as f64 / self.den as f64).round() as i64
    }
}
