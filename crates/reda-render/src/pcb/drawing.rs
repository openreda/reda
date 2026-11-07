//! Footprint and free drawing, text, the Board Outline and cutouts, and the Ratsnest.

use reda_model::geom::{Nm, Point};
use reda_model::{GraphicShape, LayerKind, Shape, Uuid};

use super::{BoardGraphic, BoardText, Builder, RatsnestLine, capsule};
use crate::arc;
use crate::color::BoardColor;
use crate::polygon::triangulate;
use crate::scene::{GlyphRun, Primitive, SceneLayer};

/// The width of the Board Outline and cutouts.
pub(super) const OUTLINE_WIDTH: Nm = Nm(150_000);
/// The width of a Ratsnest line.
pub(super) const RATSNEST_WIDTH: Nm = Nm(100_000);

/// Footprint drawing, then free lines, arcs and rectangles.
pub(super) fn graphics(b: &mut Builder, graphics: &[BoardGraphic]) {
    for g in graphics {
        shape(b, g.object, g.layer, &g.shape, g.width, g.fill);
    }
    let pcb = b.pcb;
    for g in pcb.graphics() {
        let (shape_, fill) = match g.shape {
            GraphicShape::Line { from, to } => (
                Shape::Line {
                    start: from,
                    end: to,
                },
                false,
            ),
            GraphicShape::Arc { from, to, sweep } => {
                let points = arc::through(from, to, sweep);
                b.polyline(g.uuid, g.layer, &points, g.width, false);
                continue;
            }
            GraphicShape::Rect { from, to, filled } => (
                Shape::Rect {
                    start: from,
                    end: to,
                },
                filled,
            ),
        };
        shape(b, g.uuid, g.layer, &shape_, g.width, fill);
    }
}

fn shape(b: &mut Builder, object: Uuid, layer: Uuid, shape: &Shape, width: Nm, fill: bool) {
    let half = Nm(width.0 / 2);
    match shape {
        Shape::Line { start, end } => b.on_layer(object, layer, capsule(*start, *end, width)),
        Shape::Rect { start, end } => {
            let center = Point::new(Nm((start.x.0 + end.x.0) / 2), Nm((start.y.0 + end.y.0) / 2));
            let size = (Nm((end.x - start.x).0.abs()), Nm((end.y - start.y).0.abs()));
            let rect = if fill {
                Primitive::Rect {
                    center,
                    size: (size.0 + width, size.1 + width),
                    rotation: Default::default(),
                    corner_radius: half,
                    stroke: None,
                }
            } else {
                Primitive::Rect {
                    center,
                    size,
                    rotation: Default::default(),
                    corner_radius: Nm(0),
                    stroke: Some(width),
                }
            };
            b.on_layer(object, layer, rect);
        }
        Shape::Arc {
            center,
            start,
            sweep,
        } => {
            let points = arc::about(*center, *start, *sweep);
            b.polyline(object, layer, &points, width, false);
        }
        Shape::Circle { center, radius } => {
            let inner = if fill {
                Nm(0)
            } else {
                (*radius - half).max(Nm(0))
            };
            let ring = Primitive::Ring {
                center: *center,
                outer: *radius + half,
                inner,
            };
            b.on_layer(object, layer, ring);
        }
        Shape::Polygon { points } => {
            if fill {
                b.on_layer(object, layer, Primitive::Triangles(triangulate(points)));
            }
            b.polyline(object, layer, points, width, true);
        }
        // Text arrives laid out, as a BoardText.
        Shape::Text { .. } => {}
    }
}

pub(super) fn texts(b: &mut Builder, texts: &[BoardText]) {
    for t in texts {
        let run = GlyphRun {
            text: t.text.clone(),
            at: t.at,
            height: t.height,
            rotation: t.rotation,
            align: t.align,
            view_label: false,
            strokes: t.strokes.clone(),
            stroke_width: t.stroke_width,
            fill: t.fill.clone(),
        };
        let before = b.scene.items.len();
        b.on_layer(t.object, t.layer, Primitive::Glyphs(run));
        if let Some(item) = b.scene.items.get_mut(before) {
            item.child = t.child.clone();
        }
    }
}

/// The Board Outline and cutouts, on the Board Outline Layer.
pub(super) fn outline(b: &mut Builder) {
    let Some(layer) = b.find(LayerKind::BoardOutline, None) else {
        return;
    };
    let pcb = b.pcb;
    let contours = pcb
        .outline()
        .map(|o| (o.uuid, &o.vertices))
        .into_iter()
        .chain(pcb.cutouts().map(|c| (c.uuid, &c.vertices)));
    for (object, vertices) in contours {
        let points = arc::contour(vertices);
        b.polyline(object, layer, &points, OUTLINE_WIDTH, true);
    }
    if let Some(origin) = pcb.output_origin() {
        output_origin(b, layer, origin.uuid, origin.at);
    }
}

/// The Output Origin's mark, on the Board Outline Layer: a ring with a cross through
/// it, 2 mm across.
fn output_origin(b: &mut Builder, layer: Uuid, object: Uuid, at: Point) {
    let r = Nm(1_000_000);
    b.on_layer(
        object,
        layer,
        Primitive::Ring {
            center: at,
            outer: Nm(r.0 * 7 / 10 + OUTLINE_WIDTH.0 / 2),
            inner: Nm(r.0 * 7 / 10 - OUTLINE_WIDTH.0 / 2),
        },
    );
    for (dx, dy) in [(r, Nm(0)), (Nm(0), r)] {
        let a = Point::new(at.x - dx, at.y - dy);
        let z = Point::new(at.x + dx, at.y + dy);
        b.on_layer(object, layer, capsule(a, z, OUTLINE_WIDTH));
    }
}

/// The Ratsnest: one line per connection still to be made, tagged with its Net.
pub(super) fn ratsnest(b: &mut Builder, lines: &[RatsnestLine]) {
    for line in lines {
        b.push(
            line.net,
            SceneLayer::Ratsnest,
            BoardColor::Ratsnest,
            capsule(line.from, line.to, RATSNEST_WIDTH),
        );
    }
}
