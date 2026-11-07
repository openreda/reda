//! Courtyards drawn as loose lines and arcs, chained into one Polygon per closed loop.
//!
//! KiCad draws most courtyards as separate segments; REDA keeps a courtyard as a closed
//! shape so the Library check and DRC can test it as an area. The pieces may come in any
//! order and direction. When they do not all close into loops, the Layer gets the
//! bounding Rect of its pieces instead, with a warning.

use reda_model::geom::grown::arc_points;
use reda_model::geom::{Nm, Point};
use reda_model::{FootprintLayer, Shape};

use crate::footprint::GraphicSource;

/// How far apart two piece ends may be and still join: KiCad's three-point arcs land
/// their ends to within a few nanometres of the lines they meet.
const JOIN: i64 = 1_000;

/// Replaces the open lines and arcs on each courtyard Layer of `graphics` with Polygons.
pub fn chain(graphics: &mut Vec<GraphicSource>, warnings: &mut Vec<String>) {
    for layer in [
        FootprintLayer::CourtyardTop,
        FootprintLayer::CourtyardBottom,
    ] {
        let is_piece = |g: &GraphicSource| {
            g.layer == layer && matches!(g.shape, Shape::Line { .. } | Shape::Arc { .. })
        };
        let Some(width) = graphics.iter().find(|g| is_piece(g)).map(|g| g.width) else {
            continue;
        };
        let pieces: Vec<Vec<Point>> = graphics
            .iter()
            .filter(|g| is_piece(g))
            .map(|g| points_of(&g.shape))
            .collect();
        graphics.retain(|g| !is_piece(g));
        let shapes: Vec<Shape> = match loops(pieces.clone()) {
            Some(loops) => loops
                .into_iter()
                .map(|points| Shape::Polygon { points })
                .collect(),
            None => {
                let side = if layer == FootprintLayer::CourtyardTop {
                    "top"
                } else {
                    "bottom"
                };
                warnings.push(format!(
                    "the {side} courtyard's lines and arcs do not close, so it was imported \
                     as the rectangle around them"
                ));
                bounding_rect(pieces.iter().flatten()).into_iter().collect()
            }
        };
        graphics.extend(shapes.into_iter().map(|shape| GraphicSource {
            layer,
            shape,
            width,
            fill: false,
        }));
    }
}

/// A line's two ends, or an arc's chord corners from its start to its end.
fn points_of(shape: &Shape) -> Vec<Point> {
    match *shape {
        Shape::Line { start, end } => vec![start, end],
        Shape::Arc {
            center,
            start,
            sweep,
        } => arc_points(start, start.rotated_about(center, sweep), sweep),
        _ => vec![],
    }
}

fn near(a: Point, b: Point) -> bool {
    (a.x.0 - b.x.0).abs() <= JOIN && (a.y.0 - b.y.0).abs() <= JOIN
}

/// The closed loops the pieces make, each without its closing point repeated; `None`
/// when any piece is left in an open chain or a loop has no area.
fn loops(mut pieces: Vec<Vec<Point>>) -> Option<Vec<Vec<Point>>> {
    pieces.retain(|p| p.len() >= 2);
    let mut out = vec![];
    while let Some(mut chain) = pieces.pop() {
        loop {
            let (first, last) = (chain[0], *chain.last()?);
            if chain.len() > 2 && near(first, last) {
                chain.pop();
                break;
            }
            let next = pieces
                .iter()
                .position(|p| near(p[0], last) || near(*p.last().unwrap_or(&p[0]), last))?;
            let mut piece = pieces.swap_remove(next);
            if !near(piece[0], last) {
                piece.reverse();
            }
            chain.extend(piece.into_iter().skip(1));
        }
        chain.dedup();
        if chain.len() < 3 {
            return None;
        }
        out.push(chain);
    }
    Some(out)
}

fn bounding_rect<'a>(points: impl IntoIterator<Item = &'a Point>) -> Option<Shape> {
    let mut points = points.into_iter();
    let first = *points.next()?;
    let (mut lo, mut hi) = (first, first);
    for p in points {
        lo = Point::new(Nm(lo.x.0.min(p.x.0)), Nm(lo.y.0.min(p.y.0)));
        hi = Point::new(Nm(hi.x.0.max(p.x.0)), Nm(hi.y.0.max(p.y.0)));
    }
    Some(Shape::Rect { start: lo, end: hi })
}
