//! Filling polygons as triangle lists.

use i_triangle::i_overlay::core::fill_rule::FillRule;
use i_triangle::i_overlay::core::simplify::Simplify;
use i_triangle::i_overlay::i_float::int::point::IntPoint;
use i_triangle::int::unchecked::IntUncheckedTriangulatable;
use reda_model::geom::{Nm, Point};
use reda_text::Mesh;

/// Triangulates the closed polygon through `points`, either winding, resolving
/// self-overlaps with the non-zero rule.
pub(crate) fn triangulate(points: &[Point]) -> Mesh {
    fill(&[points], FillRule::NonZero)
}

/// Triangulates an outer contour with holes (a Zone fill's shape), either winding: the
/// area inside an odd number of contours is filled.
pub(crate) fn triangulate_with_holes(outer: &[Point], holes: &[Vec<Point>]) -> Mesh {
    let mut contours: Vec<&[Point]> = vec![outer];
    contours.extend(holes.iter().map(Vec::as_slice));
    fill(&contours, FillRule::EvenOdd)
}

fn fill(contours: &[&[Point]], rule: FillRule) -> Mesh {
    let contours: Vec<Vec<IntPoint<i64>>> = contours
        .iter()
        .map(|c| c.iter().map(|p| IntPoint::new(p.x.0, p.y.0)).collect())
        .collect();
    let shapes = contours.simplify(rule, Default::default());
    let t = shapes.uncheck_triangulate().into_triangulation::<u32>();
    Mesh {
        points: t
            .points
            .iter()
            .map(|p| Point::new(Nm(p.x), Nm(p.y)))
            .collect(),
        indices: t.indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_triangle_fills_its_own_area() {
        let mm = Nm::from_mm;
        let mesh = triangulate(&[
            Point::new(mm(0), mm(0)),
            Point::new(mm(4), mm(0)),
            Point::new(mm(0), mm(3)),
        ]);
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.double_area(), 12 * 1_000_000_000_000);
    }
}
