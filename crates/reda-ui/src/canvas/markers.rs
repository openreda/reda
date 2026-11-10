//! Violation markers over the canvas (docs/spec/v0.1/checks.md#canvas-markers):
//!
//! - the **detail marker**, a ring at the Violation's point at every zoom; zoomed in far
//!   enough to read, the value against its limit and a line between the closest points
//! - the **overlay**, which colours the whole offending object in the style the Checks
//!   Preferences pick, and what it does on an object too small on screen for its pattern
//!
//! Markers draw only on the Layers involved: one whose Layers are all hidden is not drawn.
//! Waived ones are muted and stale ones dimmed.

use egui::{Color32, Pos2, Rect, Shape, Stroke, Ui, pos2};
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{Severity, Uuid};
use reda_engine::preferences::{OverlayStyle, OverlayZoomedOut};
use reda_engine::render::{Palette, Primitive, Scene, SceneLayer};

use super::widget::CanvasInputs;
use crate::messages::{Marker, OverlayLook};
use crate::shell::{DocumentTab, Shell};
use crate::tr;
use crate::view_state::LayerView;

/// The marker's radius in points.
pub(crate) const MARKER_RADIUS: f32 = 7.0;

/// The zoom, in nm per screen point, at or below which a marker adds its value and the
/// line between the closest points: 1 mm is 50 points or more.
pub const DETAIL_ZOOM: f64 = 20_000.0;

/// The spacing of an overlay pattern, in points.
pub const PATTERN: f32 = 6.0;

/// Whether the canvas is zoomed in far enough for a marker's value and line.
pub fn zoomed_in(nm_per_point: f64) -> bool {
    nm_per_point <= DETAIL_ZOOM
}

/// Whether `marker` draws with `layers` shown: it has no Layers (a sheet), or one of them
/// is not hidden.
pub fn shown_with(marker: &Marker, layers: &LayerView) -> bool {
    marker.layers.is_empty() || marker.layers.iter().any(|l| !layers.hidden.contains(l))
}

/// Draws every marker of `tab` over its canvas, and the "N not shown" count.
pub(super) fn draw(ui: &Ui, input: &CanvasInputs, tab: &DocumentTab, palette: &Palette) {
    let rect = input.rect;
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    let zoomed = zoomed_in(input.camera.nm_per_point);
    for marker in tab.markers.iter() {
        if !shown_with(marker, &tab.layers) {
            continue;
        }
        let mut color = match marker.severity {
            Severity::Error => visuals.error_fg_color,
            _ => visuals.warn_fg_color,
        };
        if marker.waived {
            color = color.gamma_multiply(0.4);
        } else if marker.stale {
            color = color.gamma_multiply(0.6);
        }
        if let Some(look) = marker.overlay {
            let polygons = polygons(&tab.scene.scene, marker, |p| {
                input.screen_point((p.x.0 as f64, p.y.0 as f64))
            });
            for (polygon, layer_color) in polygons {
                let layer = palette.color(layer_color).0;
                let layer = Color32::from_rgba_unmultiplied(layer[0], layer[1], layer[2], 255);
                for shape in overlay_shapes(&polygon, color, layer, look) {
                    painter.add(shape);
                }
            }
        }
        if !marker.details {
            continue;
        }
        let at = input.screen_point((marker.at.x.0 as f64, marker.at.y.0 as f64));
        if zoomed && let Some([a, b]) = marker.points {
            let a = input.screen_point((a.x.0 as f64, a.y.0 as f64));
            let b = input.screen_point((b.x.0 as f64, b.y.0 as f64));
            painter.line_segment([a, b], Stroke::new(1.5, color));
            painter.circle_filled(a, 2.0, color);
            painter.circle_filled(b, 2.0, color);
        }
        if !rect.expand(MARKER_RADIUS).contains(at) {
            continue;
        }
        ring(&painter, at, marker.current, color);
        if zoomed && let Some(value) = &marker.value {
            painter.text(
                at + egui::vec2(MARKER_RADIUS * 1.6, -MARKER_RADIUS),
                egui::Align2::LEFT_BOTTOM,
                value,
                egui::FontId::proportional(12.0),
                color,
            );
        }
    }
    if tab.markers_not_shown > 0 {
        // Above the Status Bar, clear of the Active Bar at the top.
        painter.text(
            rect.left_bottom() + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            tr!("markers-not-shown", count = tab.markers_not_shown),
            egui::FontId::proportional(12.0),
            visuals.warn_fg_color,
        );
    }
}

/// The ring with an exclamation mark: thicker for the Violation last gone to.
fn ring(painter: &egui::Painter, at: Pos2, current: bool, color: Color32) {
    let (radius, width) = if current {
        (MARKER_RADIUS * 1.4, 3.0)
    } else {
        (MARKER_RADIUS, 2.0)
    };
    painter.circle(
        at,
        radius,
        Color32::from_black_alpha(96),
        Stroke::new(width, color),
    );
    painter.line_segment(
        [
            at + egui::vec2(0.0, -radius * 0.55),
            at + egui::vec2(0.0, radius * 0.1),
        ],
        Stroke::new(2.0, color),
    );
    painter.circle_filled(at + egui::vec2(0.0, radius * 0.4), 1.2, color);
}

/// A primary click on a marker makes its first shown Layer the active one
/// (`pcb.layer.activate`), as clicking a marker on a board does.
pub fn click(shell: &mut Shell, document: Uuid, input: &CanvasInputs) {
    if !input.response.clicked() {
        return;
    }
    let Some(screen) = input.response.interact_pointer_pos() else {
        return;
    };
    let Some(tab) = shell.document_tab(document) else {
        return;
    };
    let layer = tab.markers.iter().find_map(|m| {
        let at = input.screen_point((m.at.x.0 as f64, m.at.y.0 as f64));
        if !m.details || at.distance(screen) > MARKER_RADIUS * 1.4 {
            return None;
        }
        m.layers
            .iter()
            .find(|l| !tab.layers.hidden.contains(l))
            .copied()
    });
    if let Some(layer) = layer
        && tab.layers.active != Some(layer)
    {
        let _ = shell.run_reporting("pcb.layer.activate", serde_json::json!({ "layer": layer }));
    }
}

/// The convex screen polygons the overlay fills: the drawing of each of the marker's
/// objects (only on its Layers, when it has any), with each item's colour key. Text and
/// hairlines are left out.
pub fn polygons(
    scene: &Scene,
    marker: &Marker,
    to_screen: impl Fn(Point) -> Pos2,
) -> Vec<(Vec<Pos2>, reda_engine::render::ColorKey)> {
    let mut out = Vec::new();
    for object in &marker.objects {
        for item in scene.items_of(*object) {
            if !marker.layers.is_empty()
                && !matches!(item.layer, SceneLayer::Board(l) if marker.layers.contains(&l))
            {
                continue;
            }
            for polygon in primitive_polygons(&item.primitive) {
                let polygon: Vec<Pos2> = polygon.into_iter().map(&to_screen).collect();
                out.push((polygon, item.color));
            }
        }
    }
    out
}

/// A primitive as convex polygons in Document nm (as floats).
fn primitive_polygons(primitive: &Primitive) -> Vec<Vec<Point>> {
    let p = |x: f64, y: f64| Point::new(Nm(x.round() as i64), Nm(y.round() as i64));
    match primitive {
        Primitive::Capsule { a, b, width } => {
            let (ax, ay, bx, by) = (a.x.0 as f64, a.y.0 as f64, b.x.0 as f64, b.y.0 as f64);
            let r = width.0 as f64 / 2.0;
            let angle = (by - ay).atan2(bx - ax);
            let mut out = Vec::new();
            // A half circle about b, then one about a: a convex capsule.
            for i in 0..=8 {
                let t = angle - std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / 8.0;
                out.push(p(bx + r * t.cos(), by + r * t.sin()));
            }
            for i in 0..=8 {
                let t = angle + std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / 8.0;
                out.push(p(ax + r * t.cos(), ay + r * t.sin()));
            }
            vec![out]
        }
        Primitive::Rect {
            center,
            size,
            rotation,
            ..
        } => {
            let (cx, cy) = (center.x.0 as f64, center.y.0 as f64);
            let (hw, hh) = (size.0.0 as f64 / 2.0, size.1.0 as f64 / 2.0);
            let t = (rotation.0 as f64 / 1000.0).to_radians();
            let (s, c) = t.sin_cos();
            let corner = |x: f64, y: f64| p(cx + x * c - y * s, cy + x * s + y * c);
            vec![vec![
                corner(-hw, -hh),
                corner(hw, -hh),
                corner(hw, hh),
                corner(-hw, hh),
            ]]
        }
        Primitive::Ring {
            center,
            outer,
            inner,
        } => {
            let (cx, cy) = (center.x.0 as f64, center.y.0 as f64);
            let (ro, ri) = (outer.0 as f64, inner.0 as f64);
            let at = |r: f64, i: usize| {
                let t = std::f64::consts::TAU * i as f64 / 16.0;
                p(cx + r * t.cos(), cy + r * t.sin())
            };
            if ri <= 0.0 {
                return vec![(0..16).map(|i| at(ro, i)).collect()];
            }
            (0..16)
                .map(|i| vec![at(ri, i), at(ro, i), at(ro, i + 1), at(ri, i + 1)])
                .collect()
        }
        Primitive::Triangles(mesh) => mesh
            .indices
            .chunks_exact(3)
            .map(|t| t.iter().map(|i| mesh.points[*i as usize]).collect())
            .collect(),
        Primitive::Hairline { .. } | Primitive::Glyphs(_) => Vec::new(),
    }
}

/// The shapes an overlay paints over one convex screen polygon, in the severity `color`
/// and the object's `layer` colour. A polygon less than two pattern periods across is
/// "zoomed out": the pattern shrinks with it, or the Layer or severity colour fills it,
/// as `look.zoomed_out` says.
pub fn overlay_shapes(
    polygon: &[Pos2],
    color: Color32,
    layer: Color32,
    look: OverlayLook,
) -> Vec<Shape> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    let bounds = Rect::from_points(polygon);
    let extent = bounds.width().max(bounds.height());
    let fill = |c: Color32| Shape::convex_polygon(polygon.to_vec(), c, Stroke::NONE);
    let outline = Shape::closed_line(polygon.to_vec(), Stroke::new(1.0, color));
    let layer_bright = layer.lerp_to_gamma(Color32::WHITE, 0.35);
    let patterned = matches!(
        look.style,
        OverlayStyle::WarningPattern | OverlayStyle::CrossPattern
    );
    let mut period = PATTERN;
    if patterned && extent < 2.0 * PATTERN {
        match look.zoomed_out {
            OverlayZoomedOut::PatternScales => period = (extent / 3.0).max(1.0),
            OverlayZoomedOut::LayerColorDominates => return vec![fill(layer_bright)],
            OverlayZoomedOut::OverrideColorDominates => {
                return vec![fill(color.gamma_multiply(0.8))];
            }
        }
    }
    match look.style {
        OverlayStyle::Solid => vec![fill(color.gamma_multiply(0.6))],
        OverlayStyle::LayerColor => vec![fill(layer_bright), outline],
        OverlayStyle::WarningPattern => {
            let mut out = vec![fill(color.gamma_multiply(0.15))];
            out.extend(
                hatch(polygon, bounds, period, 1.0)
                    .into_iter()
                    .map(|l| Shape::line_segment(l, Stroke::new(period * 0.45, color))),
            );
            out.push(outline);
            out
        }
        OverlayStyle::CrossPattern => {
            let mut out = vec![fill(color.gamma_multiply(0.1))];
            for slope in [1.0, -1.0] {
                out.extend(
                    hatch(polygon, bounds, period, slope)
                        .into_iter()
                        .map(|l| Shape::line_segment(l, Stroke::new(1.0, color))),
                );
            }
            out.push(outline);
            out
        }
    }
}

/// Diagonal lines `period` apart across `bounds`, rising (`slope` 1) or falling (-1),
/// each clipped to the convex `polygon`.
fn hatch(polygon: &[Pos2], bounds: Rect, period: f32, slope: f32) -> Vec<[Pos2; 2]> {
    let span = bounds.width() + bounds.height();
    let mut out = Vec::new();
    let mut offset = 0.0;
    while offset <= span {
        // Lines x - slope * y = c, through the box.
        let (a, b) = if slope > 0.0 {
            let start = pos2(bounds.min.x + offset - bounds.height(), bounds.max.y);
            (start, start + egui::vec2(span, -span))
        } else {
            let start = pos2(bounds.min.x + offset - bounds.height(), bounds.min.y);
            (start, start + egui::vec2(span, span))
        };
        if let Some(line) = clip(polygon, a, b) {
            out.push(line);
        }
        offset += period;
    }
    out
}

/// The part of segment `a`–`b` inside the convex `polygon` (Cyrus–Beck), if any.
fn clip(polygon: &[Pos2], a: Pos2, b: Pos2) -> Option<[Pos2; 2]> {
    let area: f32 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(p, q)| p.x * q.y - q.x * p.y)
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let d = b - a;
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    for (p, q) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        let edge = *q - *p;
        // Inside is to the left of each edge of a counter-clockwise polygon.
        let normal = egui::vec2(-edge.y, edge.x) * sign;
        let num = normal.dot(a - *p);
        let den = normal.dot(d);
        if den.abs() < 1e-9 {
            if num < 0.0 {
                return None;
            }
            continue;
        }
        let t = -num / den;
        if den > 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some([a + d * t0, a + d * t1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(side: f32) -> Vec<Pos2> {
        vec![
            pos2(0.0, 0.0),
            pos2(side, 0.0),
            pos2(side, side),
            pos2(0.0, side),
        ]
    }

    fn look(style: OverlayStyle, zoomed_out: OverlayZoomedOut) -> OverlayLook {
        OverlayLook { style, zoomed_out }
    }

    fn lines(shapes: &[Shape]) -> Vec<[Pos2; 2]> {
        shapes
            .iter()
            .filter_map(|s| match s {
                Shape::LineSegment { points, .. } => Some(*points),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_cross_pattern_hatches_both_ways_inside_the_object_with_an_outline() {
        let shapes = overlay_shapes(
            &square(60.0),
            Color32::RED,
            Color32::GREEN,
            look(
                OverlayStyle::CrossPattern,
                OverlayZoomedOut::OverrideColorDominates,
            ),
        );
        let lines = lines(&shapes);
        assert!(lines.len() >= 20, "{}", lines.len());
        let inside = |p: Pos2| (-0.01..=60.01).contains(&p.x) && (-0.01..=60.01).contains(&p.y);
        assert!(lines.iter().all(|[a, b]| inside(*a) && inside(*b)));
        let rising = lines
            .iter()
            .filter(|[a, b]| (b.y - a.y) * (b.x - a.x) < 0.0)
            .count();
        let falling = lines
            .iter()
            .filter(|[a, b]| (b.y - a.y) * (b.x - a.x) > 0.0)
            .count();
        assert!(rising > 5 && falling > 5, "{rising} {falling}");
        assert!(matches!(shapes.last(), Some(Shape::Path(_))));
    }

    #[test]
    fn zoomed_out_the_override_colour_or_the_layer_colour_fills_or_the_pattern_shrinks() {
        let tiny = square(5.0);
        let red = Color32::RED;
        let green = Color32::from_rgb(0, 128, 0);
        let filled = |shapes: Vec<Shape>| match shapes.as_slice() {
            [Shape::Path(p)] => Some(p.fill),
            _ => None,
        };
        let over = overlay_shapes(
            &tiny,
            red,
            green,
            look(
                OverlayStyle::CrossPattern,
                OverlayZoomedOut::OverrideColorDominates,
            ),
        );
        assert_eq!(filled(over), Some(red.gamma_multiply(0.8)));
        let layer = overlay_shapes(
            &tiny,
            red,
            green,
            look(
                OverlayStyle::WarningPattern,
                OverlayZoomedOut::LayerColorDominates,
            ),
        );
        assert_eq!(
            filled(layer),
            Some(green.lerp_to_gamma(Color32::WHITE, 0.35))
        );
        let scaled = overlay_shapes(
            &tiny,
            red,
            green,
            look(OverlayStyle::CrossPattern, OverlayZoomedOut::PatternScales),
        );
        assert!(lines(&scaled).len() >= 4, "{}", lines(&scaled).len());
        // Big enough on screen, the pattern stays whatever the zoomed-out choice.
        let big = overlay_shapes(
            &square(60.0),
            red,
            green,
            look(
                OverlayStyle::WarningPattern,
                OverlayZoomedOut::LayerColorDominates,
            ),
        );
        assert!(!lines(&big).is_empty());
    }

    #[test]
    fn solid_and_layer_colour_styles_fill_the_object() {
        let red = Color32::RED;
        let green = Color32::from_rgb(0, 128, 0);
        let solid = overlay_shapes(
            &square(60.0),
            red,
            green,
            look(OverlayStyle::Solid, OverlayZoomedOut::PatternScales),
        );
        assert!(matches!(solid.as_slice(), [Shape::Path(p)] if p.fill == red.gamma_multiply(0.6)));
        let layer = overlay_shapes(
            &square(60.0),
            red,
            green,
            look(OverlayStyle::LayerColor, OverlayZoomedOut::PatternScales),
        );
        assert!(
            matches!(layer.first(), Some(Shape::Path(p)) if p.fill == green.lerp_to_gamma(Color32::WHITE, 0.35))
        );
    }

    #[test]
    fn a_track_capsule_is_one_convex_polygon_around_its_ends() {
        let polygons = primitive_polygons(&Primitive::Capsule {
            a: Point::new(Nm(0), Nm(0)),
            b: Point::new(Nm(10_000), Nm(0)),
            width: Nm(2_000),
        });
        assert_eq!(polygons.len(), 1);
        let xs: Vec<i64> = polygons[0].iter().map(|p| p.x.0).collect();
        assert_eq!(xs.iter().min(), Some(&-1_000));
        assert_eq!(xs.iter().max(), Some(&11_000));
    }
}
