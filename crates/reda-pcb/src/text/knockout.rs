//! Flatten a knockout box minus its glyphs before any board transform.

use i_triangle::int::triangulatable::IntTriangulatable;
use reda_model::geom::{Nm, Point};

use super::{Fill, Stroke};
use crate::copper::shape::{Core, Prim, Shape};
use crate::zone::poly;

pub(super) fn flatten(
    strokes: &[Stroke],
    width: Nm,
    fill: &Fill,
    frame: Option<(i64, i64, i64, i64)>,
    margin: Nm,
) -> Fill {
    let prims = strokes
        .iter()
        .map(|s| Prim {
            core: Core::Seg([s.a.x.0, s.a.y.0], [s.b.x.0, s.b.y.0]),
            radius: width.0 / 2,
        })
        .collect();
    let mut glyphs = poly::grown(&Shape { prims }, 0);
    for tri in fill.indices.chunks_exact(3) {
        glyphs.push(vec![
            tri.iter()
                .map(|&i| {
                    let p = fill.points[i as usize];
                    poly::ip([p.x.0, p.y.0])
                })
                .collect(),
        ]);
    }
    let glyphs = poly::union(glyphs);
    let bounds = frame.or_else(|| {
        let mut points = fill.points.iter().map(|p| (p.x.0, p.y.0, 0)).chain(
            strokes
                .iter()
                .flat_map(|s| [s.a, s.b])
                .map(|p| (p.x.0, p.y.0, width.0 / 2)),
        );
        let (x, y, r) = points.next()?;
        let mut b = (x - r, y - r, x + r, y + r);
        for (x, y, r) in points {
            b = (
                b.0.min(x - r),
                b.1.min(y - r),
                b.2.max(x + r),
                b.3.max(y + r),
            );
        }
        Some(b)
    });
    let Some((left, bottom, right, top)) = bounds else {
        return Fill::default();
    };
    let m = margin.0;
    let box_area = poly::polygon(vec![
        poly::ip([left - m, bottom - m]),
        poly::ip([right + m, bottom - m]),
        poly::ip([right + m, top + m]),
        poly::ip([left - m, top + m]),
    ]);
    let result = poly::difference(&box_area, &glyphs);
    let mut out = Fill::default();
    for rings in result {
        let mesh = rings.triangulate().into_triangulation::<u32>();
        let base = out.points.len() as u32;
        out.points.extend(
            mesh.points
                .into_iter()
                .map(|p| Point::new(Nm(p.x), Nm(p.y))),
        );
        out.indices
            .extend(mesh.indices.into_iter().map(|i| i + base));
    }
    out
}
