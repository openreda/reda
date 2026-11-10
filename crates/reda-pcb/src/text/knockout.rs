//! Flatten a knockout box minus its glyphs before any board transform: triangles for the
//! canvas and the same area as regions with holes for Gerber.

use i_triangle::int::triangulatable::IntTriangulatable;
use reda_model::geom::{Bounds, Nm, Point};

use super::{Fill, Region, Stroke};
use crate::copper::shape::{Core, Prim, Shape};
use crate::zone::poly;

pub(super) fn flatten(
    strokes: &[Stroke],
    width: Nm,
    fill: &Fill,
    frame: Option<Bounds>,
    margin: Nm,
) -> (Fill, Vec<Region>) {
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
        let ink = fill.points.iter().map(|&p| Bounds::of_point(p));
        let pen = strokes
            .iter()
            .flat_map(|s| [s.a, s.b])
            .map(|p| Bounds::around(p, Nm(width.0 / 2)));
        ink.chain(pen).reduce(Bounds::union)
    });
    let Some(Bounds { min, max }) = bounds else {
        return (Fill::default(), Vec::new());
    };
    let (left, bottom, right, top) = (min.x.0, min.y.0, max.x.0, max.y.0);
    let m = margin.0;
    let box_area = poly::polygon(vec![
        poly::ip([left - m, bottom - m]),
        poly::ip([right + m, bottom - m]),
        poly::ip([right + m, top + m]),
        poly::ip([left - m, top + m]),
    ]);
    let result = poly::difference(&box_area, &glyphs);
    let mut out = Fill::default();
    let regions = result
        .iter()
        .filter_map(|rings| {
            let (outer, holes) = rings.split_first()?;
            Some(Region {
                outer: outer.iter().map(|&p| poly::point(p)).collect(),
                holes: holes
                    .iter()
                    .map(|h| h.iter().map(|&p| poly::point(p)).collect())
                    .collect(),
            })
        })
        .collect();
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
    (out, regions)
}
