//! The geometry a laid-out string turns into.

use reda_geom::{Nm, Point};

/// A pen stroke from `a` to `b`, drawn as a capsule. The stroke width belongs to the text
/// object, not to the geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub a: Point,
    pub b: Point,
}

impl Segment {
    pub const fn new(a: Point, b: Point) -> Segment {
        Segment { a, b }
    }
}

/// A triangle list: every three `indices` name one triangle in `points`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mesh {
    pub points: Vec<Point>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Twice the summed area of all triangles, in nm². Exact in integer arithmetic.
    pub fn double_area(&self) -> i128 {
        self.indices
            .chunks_exact(3)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| self.points[t[i] as usize]);
                let (abx, aby) = (i128::from((b.x - a.x).0), i128::from((b.y - a.y).0));
                let (acx, acy) = (i128::from((c.x - a.x).0), i128::from((c.y - a.y).0));
                (abx * acy - aby * acx).abs()
            })
            .sum()
    }

    pub(crate) fn append(&mut self, other: Mesh) {
        let base = u32::try_from(self.points.len()).expect("mesh fits u32 indices");
        self.points.extend(other.points);
        self.indices
            .extend(other.indices.into_iter().map(|i| i + base));
    }
}

/// One filled piece of an outline-font glyph, as the glyph's simplified contours: its
/// outer ring, then the holes inside it. A writer that needs whole regions (Gerber) takes
/// these; the canvas takes the [`Mesh`] made from them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Region {
    pub outer: Vec<Point>,
    pub holes: Vec<Vec<Point>>,
}

impl Region {
    /// Every ring of the region, outer first.
    pub fn rings(&self) -> impl Iterator<Item = &Vec<Point>> {
        std::iter::once(&self.outer).chain(&self.holes)
    }

    /// Each point moved by `f`.
    pub fn map(&self, f: impl Fn(Point) -> Point) -> Region {
        Region {
            outer: self.outer.iter().map(|&p| f(p)).collect(),
            holes: self
                .holes
                .iter()
                .map(|h| h.iter().map(|&p| f(p)).collect())
                .collect(),
        }
    }
}

/// A laid-out string: y-up, in nm, with the origin at the left end of the baseline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextGeometry {
    /// Stroke-font glyphs, drawn as capsules at the text object's stroke width.
    pub strokes: Vec<Segment>,
    /// Outline-font glyphs (including Hershey fallbacks), filled.
    pub fill: Mesh,
    /// The same glyphs as regions with holes, one or more per glyph.
    pub regions: Vec<Region>,
    /// Pen advance from the origin to the end of the string.
    pub advance: Nm,
}

impl TextGeometry {
    /// The axis-aligned bounds of every stroke end and fill point, as (min, max). Strokes are
    /// counted without their width. `None` for text with no ink, such as spaces.
    pub fn ink_bounds(&self) -> Option<(Point, Point)> {
        let points = self
            .strokes
            .iter()
            .flat_map(|s| [s.a, s.b])
            .chain(self.fill.points.iter().copied());
        points.fold(None, |acc, p| {
            Some(match acc {
                None => (p, p),
                Some((lo, hi)) => (
                    Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                ),
            })
        })
    }
}
