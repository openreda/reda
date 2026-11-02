//! The canvas widget: lays out the canvas, turns pointer input into camera moves and
//! Tool events, and adds the paint callback plus the overlays drawn over it.
//!
//! A frame is two calls with the editor between them: [`canvas_input`] lays the canvas
//! out and returns its pointer events, the editor turns them into Commands and updates the
//! tab's marks, and [`canvas_paint`] draws what the tab now shows. [`canvas`] does both
//! with no editor.

use std::sync::Arc;

use egui::{Color32, PointerButton, Rect, Role, Sense, Stroke, Ui, WidgetInfo};
use reda_engine::model::Severity;
use reda_engine::model::geom::Point;
use reda_engine::render::gpu::GridSpec;
use reda_engine::render::{ColorKey, Palette, SchematicColor, Theme};

use super::camera::{Camera, CanvasInput, grid_pitch};
use super::gpu::{CanvasFrame, paint};
use super::interact::{BoxMode, Pointer, pointer};
use crate::shell::DocumentTab;
use crate::tr;

/// The grid dots' radius in points.
const GRID_DOT_RADIUS: f32 = 1.0;
/// The canvas a Document is fitted to before it has any size: an A4 sheet.
const EMPTY_SHEET_NM: (i64, i64) = (297_000_000, 210_000_000);
/// A rubber-banded Wire's width: a sheet's Wire width (6 mil), at least this many points.
const MIN_WIRE_POINTS: f32 = 1.5;
const WIRE_NM: f64 = 152_400.0;

/// What a Tool draws over the canvas this frame, besides the marks on the Scene's objects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// A selection box being dragged.
    pub rubber_band: Option<RubberBand>,
    /// Wires a drag in flight redraws, as (from, to).
    pub wires: Arc<[(Point, Point)]>,
    /// Where the pointer snapped to.
    pub cursor: Option<Point>,
}

/// A selection box being dragged out: its corners (nm) and how it picks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RubberBand {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub mode: BoxMode,
}

/// One frame of a canvas's layout and input.
pub struct CanvasInputs {
    pub rect: Rect,
    pub response: egui::Response,
    /// The canvas's camera after this frame's pan and zoom.
    pub camera: Camera,
    /// Primary-button events for the active Tool, in Document coordinates.
    pub events: Vec<Pointer>,
}

/// Shows `tab`'s canvas filling the space left in `ui`, with no Tools.
pub fn canvas(ui: &mut Ui, tab: &mut DocumentTab, theme: Theme) {
    let input = canvas_input(ui, tab);
    canvas_paint(ui, &input, tab, theme);
}

/// Lays out `tab`'s canvas filling the space left in `ui`, moves its camera, and returns
/// the frame's pointer events.
pub fn canvas_input(ui: &mut Ui, tab: &mut DocumentTab) -> CanvasInputs {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let title = tab.title.clone();
    response.widget_info(|| {
        WidgetInfo::labeled(
            Role::Canvas,
            true,
            tr!("canvas-description", document = title.as_str()),
        )
    });

    if let Some((lo, hi)) = tab.zoom_to.take() {
        tab.camera = Some(Camera::fit(lo, hi, rect.size()));
    }
    let camera = tab.camera.get_or_insert_with(|| {
        let (lo, hi) = tab.scene.scene.bounds().unwrap_or_else(|| {
            use reda_engine::model::geom::Nm;
            (
                Point::new(Nm(0), Nm(0)),
                Point::new(Nm(EMPTY_SHEET_NM.0), Nm(EMPTY_SHEET_NM.1)),
            )
        });
        Camera::fit(lo, hi, rect.size())
    });

    let mut input = CanvasInput::default();
    if response.hovered() {
        input.pointer = response.hover_pos();
        ui.input(|i| {
            input.scroll = i.smooth_scroll_delta.y;
            input.zoom = i.zoom_delta();
        });
    }
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        input.pan = response.drag_delta();
    }
    camera.apply(rect, &input);
    let camera = *camera;
    let events = pointer(ui, &response, &camera, rect);
    CanvasInputs {
        rect,
        response,
        camera,
        events,
    }
}

/// Draws `tab` into the canvas laid out by [`canvas_input`].
pub fn canvas_paint(ui: &mut Ui, input: &CanvasInputs, tab: &DocumentTab, theme: Theme) {
    let (rect, camera) = (input.rect, input.camera);
    let ppp = ui.ctx().pixels_per_point();
    let view = camera.view(rect, ppp);
    let palette = Palette::shipped(theme);
    let grid = grid_pitch(tab.grid.0 as f64, view.nm_per_px).map(|pitch| GridSpec {
        origin: (0.0, 0.0),
        pitch,
        radius_px: GRID_DOT_RADIUS * ppp,
        color: palette.grid(),
    });
    let frame = CanvasFrame {
        canvas: canvas_id(tab),
        scene: tab.scene.clone(),
        theme,
        view,
        grid,
        selection: tab.selection.clone(),
        live: tab.live.clone(),
        moving_offset: tab.moving_offset,
    };
    paint(ui.painter(), rect, frame);
    overlay(ui, rect, &camera, &tab.overlay, &palette);
    markers(ui, rect, &camera, tab);
}

/// Draws a Tool's overlay: rubber-banded Wires, the snap cursor and a selection box (solid
/// when it encloses, dashed when it touches).
fn overlay(ui: &Ui, rect: Rect, camera: &Camera, overlay: &Overlay, palette: &Palette) {
    let painter = ui.painter_at(rect);
    let screen = |p: Point| camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64));
    let wire = palette.color(ColorKey::Schematic(SchematicColor::Wire)).0;
    let wire = Color32::from_rgb(wire[0], wire[1], wire[2]);
    let width = ((WIRE_NM / camera.nm_per_point) as f32).max(MIN_WIRE_POINTS);
    for &(a, b) in overlay.wires.iter() {
        painter.line_segment([screen(a), screen(b)], Stroke::new(width, wire));
    }
    let accent = ui.visuals().selection.stroke.color;
    if let Some(at) = overlay.cursor {
        let c = screen(at);
        let r = 6.0;
        let stroke = Stroke::new(1.0, accent);
        painter.line_segment([c - egui::vec2(r, 0.0), c + egui::vec2(r, 0.0)], stroke);
        painter.line_segment([c - egui::vec2(0.0, r), c + egui::vec2(0.0, r)], stroke);
    }
    if let Some(RubberBand {
        from: a,
        to: b,
        mode,
    }) = overlay.rubber_band
    {
        let band = Rect::from_two_pos(camera.screen_point(rect, a), camera.screen_point(rect, b));
        painter.rect_filled(band, 0.0, accent.gamma_multiply(0.12));
        let stroke = Stroke::new(1.0, accent);
        match mode {
            BoxMode::Enclose => {
                painter.rect_stroke(band, 0.0, stroke, egui::StrokeKind::Inside);
            }
            BoxMode::Touch => {
                let corners = [
                    band.left_top(),
                    band.right_top(),
                    band.right_bottom(),
                    band.left_bottom(),
                    band.left_top(),
                ];
                for pair in corners.windows(2) {
                    painter.extend(egui::Shape::dashed_line(pair, stroke, 4.0, 3.0));
                }
            }
        }
    }
}

/// The marker's radius in points.
const MARKER_RADIUS: f32 = 7.0;

/// Draws a ring with an exclamation mark at each Violation, over the canvas: red for
/// errors, amber for warnings, faded when waived, thicker for the one last gone to.
fn markers(ui: &Ui, rect: Rect, camera: &Camera, tab: &DocumentTab) {
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    for marker in tab.markers.iter() {
        let at = camera.screen_point(rect, (marker.at.x.0 as f64, marker.at.y.0 as f64));
        if !rect.expand(MARKER_RADIUS).contains(at) {
            continue;
        }
        let mut color = match marker.severity {
            Severity::Error => visuals.error_fg_color,
            _ => visuals.warn_fg_color,
        };
        if marker.waived {
            color = color.gamma_multiply(0.4);
        }
        let (radius, width) = if marker.current {
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
}

/// The id a tab's canvas keeps its GPU state under.
pub fn canvas_id(tab: &DocumentTab) -> egui::Id {
    egui::Id::NULL.with(("canvas", tab.document))
}
