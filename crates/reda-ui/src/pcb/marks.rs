//! The marks the board's Tools draw over the painted canvas: the shape a drawing Tool has
//! in progress, measurements, the snap marker, and a Footprint move's live overlap and
//! alignment guides. On-canvas drawing only, in the canvas's own colours, so the source
//! scan leaves this file alone; the chrome over the canvas is in [`super::canvas`].

use egui::{Align2, FontId, Stroke, Ui, vec2};
use reda_engine::model::geom::Point;

use super::view::format_length;
use crate::canvas::CanvasInputs;
use crate::shell::Shell;
use crate::snap::Guide;
use crate::tools::Tool;
use crate::tr;

/// What the drawing Tools show over the canvas: the shape in progress, a measurement,
/// and the Mechanical Part being placed. (The Output Origin is in the Scene.)
pub fn drawing(ui: &Ui, shell: &Shell, shown: &CanvasInputs) {
    let painter = ui.painter_at(shown.rect);
    let accent = ui.visuals().selection.stroke.color;
    let stroke = Stroke::new(1.5, accent);
    let screen = |p: Point| shown.screen_point((p.x.0 as f64, p.y.0 as f64));
    let draw = shell.pcb_draw();
    if let Some(preview) = draw.preview() {
        let mut points: Vec<_> = preview.points.iter().map(|p| screen(*p)).collect();
        if preview.closed
            && let Some(first) = points.first().copied()
        {
            points.push(first);
        }
        if points.len() > 1 {
            painter.add(egui::Shape::line(points.clone(), stroke));
        }
        for p in &points {
            painter.circle_stroke(*p, 3.0, Stroke::new(1.0, accent));
        }
    }
    let units = shell.board_view().units;
    let kept = super::measure::shown(shell);
    for m in kept.iter().chain(draw.measurement().as_ref()) {
        let (a, b) = (screen(m.from), screen(m.to));
        painter.line_segment([a, b], stroke);
        painter.circle_stroke(a, 3.0, Stroke::new(1.0, accent));
        painter.circle_stroke(b, 3.0, Stroke::new(1.0, accent));
        let label = tr!(
            "pcb-measure",
            dx = format_length(m.to.x - m.from.x, units),
            dy = format_length(m.to.y - m.from.y, units),
            distance = format_length(m.distance(), units)
        );
        painter.text(
            b + vec2(8.0, -8.0),
            Align2::LEFT_BOTTOM,
            label,
            FontId::proportional(12.0),
            accent,
        );
    }
}

/// Whether the cursor's snap shows now: over the canvas, or while a move, a polygon edit
/// or a drawing Tool is under way.
pub fn snap_shown(shell: &Shell, shown: &CanvasInputs) -> bool {
    shown.response.hovered()
        || shell.pcb_tool().moving()
        || shell.pcb_tool().polygon_edit().is_some()
        || shell.pcb_draw().preview().is_some()
}

/// Where the cursor snaps now, by the active Tool.
pub fn snapped(shell: &Shell) -> Option<crate::snap::Snapped> {
    match shell.tool() {
        Tool::Route | Tool::Via => shell.router().snap(),
        _ => shell.pcb_snap(),
    }
}

/// The snap marker, while the snap shows.
pub fn snap_marker(ui: &Ui, shell: &Shell, shown: &CanvasInputs) {
    let painter = ui.painter_at(shown.rect);
    let accent = ui.visuals().selection.stroke.color;
    if snap_shown(shell, shown)
        && let Some(snapped) = snapped(shell)
    {
        let at = shown.screen_point((snapped.at.x.0 as f64, snapped.at.y.0 as f64));
        crate::snap::marker(&painter, at, snapped.hit, accent);
    }
}

/// Draws the live overlap and the alignment guides of the Footprint move in flight.
pub fn move_marks(ui: &Ui, shell: &Shell, input: &CanvasInputs) {
    let tool = shell.pcb_tool();
    let (overlaps, outlines) = tool.overlaps();
    let guides = tool.guide_lines();
    if overlaps.is_empty() && guides.is_empty() {
        return;
    }
    let painter = ui.painter_at(input.rect);
    let screen = |p: Point| input.screen_point((p.x.0 as f64, p.y.0 as f64));
    let refused = ui.visuals().error_fg_color;
    for outline in outlines {
        let mut points: Vec<egui::Pos2> = outline.iter().map(|p| screen(*p)).collect();
        if let Some(first) = points.first().copied() {
            points.push(first);
        }
        painter.add(egui::Shape::convex_polygon(
            points[..points.len().saturating_sub(1)].to_vec(),
            refused.gamma_multiply(0.25),
            Stroke::NONE,
        ));
        painter.add(egui::Shape::line(points, Stroke::new(2.0, refused)));
    }
    let accent = ui.visuals().selection.stroke.color.gamma_multiply(0.8);
    let r = input.rect;
    for guide in guides {
        let (a, b) = match *guide {
            Guide::X(x) => {
                let sx = screen(Point::new(x, reda_engine::model::geom::Nm(0))).x;
                (egui::pos2(sx, r.top()), egui::pos2(sx, r.bottom()))
            }
            Guide::Y(y) => {
                let sy = screen(Point::new(reda_engine::model::geom::Nm(0), y)).y;
                (egui::pos2(r.left(), sy), egui::pos2(r.right(), sy))
            }
        };
        painter.add(egui::Shape::dashed_line(
            &[a, b],
            Stroke::new(1.0, accent),
            6.0,
            4.0,
        ));
    }
}
