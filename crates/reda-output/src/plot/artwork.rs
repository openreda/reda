//! Text and drawing on a Layer: the Silkscreen files, and copper text and drawing on the
//! copper files.
//!
//! A Layer's artwork is its Footprints' drawing, Designators and value text, then the free
//! drawing and text on it. Text is plotted from the same geometry the canvas draws
//! ([`reda_pcb::text_geometry`]): stroke-font strokes as round-pen draws, outline-font
//! glyphs and flattened knockout text as filled triangles. Text Frames use the same
//! complete-line omission as the canvas.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{GraphicShape, PcbText, Shape, Side, Uuid};
use reda_pcb::placed::{self, Bounds};
use reda_pcb::{PlacedText, board_layer, footprint_texts, text_geometry};

use super::openings::mask_openings;
use super::{BoardPlot, pad_template};
use crate::gerber::{Gerber, NO_FUNCTION, Template};

/// The Silkscreen file of the Silkscreen Layer `layer` on `side`, cleared over the Solder
/// Mask openings on that side: ink cannot go on bare copper.
pub(super) fn silkscreen(plot: &BoardPlot<'_>, layer: Uuid, side: Side, g: &mut Gerber) {
    draw_layer(g, plot, layer, NO_FUNCTION);
    for opening in mask_openings(plot, side) {
        let (template, at) = pad_template(&opening);
        g.flash_clear(template, at);
    }
}

/// Draws every text and drawing on `layer` with aperture function `function`.
pub(super) fn draw_layer(
    g: &mut Gerber,
    plot: &BoardPlot<'_>,
    layer: Uuid,
    function: &'static str,
) {
    let pcb = plot.pcb;
    for instance in pcb.footprints() {
        for graphic in placed::placed_graphics(pcb, instance) {
            if board_layer(pcb, graphic.layer) == Some(layer) {
                shape(g, &graphic.shape, graphic.width, graphic.fill, function);
            }
        }
        for text in footprint_texts(pcb, instance, plot.fonts) {
            if text.layer == layer {
                draw_text(g, plot, &text, function);
            }
        }
    }
    for graphic in pcb.graphics().filter(|d| d.layer == layer) {
        let w = graphic.width;
        match graphic.shape {
            GraphicShape::Line { from, to } => g.draw(w, function, from, to, Millideg::ZERO, &[]),
            GraphicShape::Arc { from, to, sweep } => g.draw(w, function, from, to, sweep, &[]),
            GraphicShape::Rect { from, to, filled } => {
                let corners = rect_corners(from, to);
                if filled {
                    g.region(&corners, true, function, &[]);
                }
                outline(g, &corners, w, function);
            }
        }
    }
    for text in pcb.texts().filter(|t| t.layer == layer) {
        draw_text(g, plot, text, function);
    }
}

fn draw_text(g: &mut Gerber, plot: &BoardPlot<'_>, text: &PcbText, function: &'static str) {
    let PlacedText {
        strokes,
        stroke_width,
        fill,
        ..
    } = text_geometry(text, plot.fonts);
    for s in strokes {
        g.draw(stroke_width, function, s.a, s.b, Millideg::ZERO, &[]);
    }
    for t in fill.indices.chunks_exact(3) {
        let triangle = [t[0], t[1], t[2]].map(|i| fill.points[i as usize]);
        g.region(&triangle, true, function, &[]);
    }
}

/// A Footprint drawing shape, stroked with `width` and filled when `fill` and closed.
/// Text shapes are drawn by [`footprint_texts`] instead.
fn shape(g: &mut Gerber, shape: &Shape, width: Nm, fill: bool, function: &'static str) {
    match shape {
        Shape::Line { start, end } => g.draw(width, function, *start, *end, Millideg::ZERO, &[]),
        Shape::Rect { start, end } => {
            let corners = rect_corners(*start, *end);
            if fill {
                g.region(&corners, true, function, &[]);
            }
            outline(g, &corners, width, function);
        }
        Shape::Polygon { points } => {
            if fill {
                g.region(points, true, function, &[]);
            }
            outline(g, points, width, function);
        }
        Shape::Arc {
            center,
            start,
            sweep,
        } => {
            if sweep.0.abs() >= Millideg::TURN.0 {
                circle(g, *center, *start, width, function);
            } else if sweep.0 != 0 {
                let end = placed::rotate(*start, *center, *sweep);
                g.arc_about(width, function, *start, end, *center, sweep.0 > 0);
            }
        }
        Shape::Circle { center, radius } => {
            if fill {
                let diameter = Nm(2 * radius.0) + width;
                g.flash(Template::Circle(diameter), function, *center, &[]);
            } else {
                let start = Point::new(center.x + *radius, center.y);
                circle(g, *center, start, width, function);
            }
        }
        Shape::Text { .. } => {}
    }
}

/// A full circle about `centre` through `start`, as two half turns.
fn circle(g: &mut Gerber, centre: Point, start: Point, width: Nm, function: &'static str) {
    let opposite = Point::new(centre.x + centre.x - start.x, centre.y + centre.y - start.y);
    g.arc_about(width, function, start, opposite, centre, true);
    g.arc_about(width, function, opposite, start, centre, true);
}

/// The closed outline through `points`, skipped when the pen has no width.
fn outline(g: &mut Gerber, points: &[Point], width: Nm, function: &'static str) {
    if width <= Nm::ZERO {
        return;
    }
    for (i, &from) in points.iter().enumerate() {
        let to = points[(i + 1) % points.len()];
        g.draw(width, function, from, to, Millideg::ZERO, &[]);
    }
}

/// The corners of the axis-aligned rectangle between `a` and `b`, counter-clockwise.
fn rect_corners(a: Point, b: Point) -> [Point; 4] {
    let Bounds { min, max } = Bounds::of_point(a).union(Bounds::of_point(b));
    [min, Point::new(max.x, min.y), max, Point::new(min.x, max.y)]
}
