//! The canvas widget: lays out the canvas, turns pointer input into camera moves and adds
//! the paint callback.

use egui::{Color32, PointerButton, Rect, Role, Sense, Stroke, Ui, WidgetInfo};
use reda_engine::model::Severity;
use reda_engine::render::gpu::GridSpec;
use reda_engine::render::{Palette, Theme};

use super::camera::{Camera, CanvasInput, grid_pitch};
use super::gpu::{CanvasFrame, paint};
use crate::shell::DocumentTab;
use crate::tr;

/// The schematic snap grid in v0.1: 50 mil.
const SCHEMATIC_GRID_NM: f64 = 1_270_000.0;
/// The grid dots' radius in points.
const GRID_DOT_RADIUS: f32 = 1.0;
/// The canvas a Document is fitted to before it has any size: an A4 sheet.
const EMPTY_SHEET_NM: (i64, i64) = (297_000_000, 210_000_000);

/// What a canvas showed this frame, for a Tool to map pointer input onto the Document.
#[derive(Clone, Debug)]
pub struct CanvasShown {
    pub response: egui::Response,
    pub rect: Rect,
    pub camera: Camera,
    /// Seen from the bottom: x runs right to left on screen.
    pub mirrored: bool,
}

impl CanvasShown {
    /// The Document point (nm) under the screen position `pos`.
    pub fn document_point(&self, pos: egui::Pos2) -> (f64, f64) {
        self.camera
            .document_point(self.rect, mirror(self.rect, pos, self.mirrored))
    }

    /// Where the Document point `p` shows on screen.
    pub fn screen_point(&self, p: (f64, f64)) -> egui::Pos2 {
        mirror(
            self.rect,
            self.camera.screen_point(self.rect, p),
            self.mirrored,
        )
    }
}

/// `pos` reflected about the vertical centre line of `rect` when `mirrored`.
fn mirror(rect: Rect, pos: egui::Pos2, mirrored: bool) -> egui::Pos2 {
    if mirrored {
        egui::pos2(2.0 * rect.center().x - pos.x, pos.y)
    } else {
        pos
    }
}

/// Shows `tab`'s canvas filling the space left in `ui`, with a snap grid of `grid_nm`
/// (the schematic's 50 mil when `None`).
pub fn canvas(
    ui: &mut Ui,
    tab: &mut DocumentTab,
    theme: Theme,
    grid_nm: Option<f64>,
) -> CanvasShown {
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
            use reda_engine::model::geom::{Nm, Point};
            (
                Point::new(Nm(0), Nm(0)),
                Point::new(Nm(EMPTY_SHEET_NM.0), Nm(EMPTY_SHEET_NM.1)),
            )
        });
        Camera::fit(lo, hi, rect.size())
    });

    let mirrored = tab.layers.from_bottom && tab.scene.layers.is_some();
    let mut input = CanvasInput::default();
    if response.hovered() {
        input.pointer = response.hover_pos().map(|p| mirror(rect, p, mirrored));
        ui.input(|i| {
            input.scroll = i.smooth_scroll_delta.y;
            input.zoom = i.zoom_delta();
        });
    }
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        input.pan = response.drag_delta();
        if mirrored {
            input.pan.x = -input.pan.x;
        }
    }
    camera.apply(rect, &input);
    let camera = *camera;

    let ppp = ui.ctx().pixels_per_point();
    let mut view = camera.view(rect, ppp);
    view.mirrored = mirrored;
    let palette = Palette::shipped(theme);
    let grid =
        grid_pitch(grid_nm.unwrap_or(SCHEMATIC_GRID_NM), view.nm_per_px).map(|pitch| GridSpec {
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
        palette: tab.palette.clone(),
    };
    paint(ui.painter(), rect, frame);
    let shown = CanvasShown {
        response,
        rect,
        camera,
        mirrored,
    };
    markers(ui, &shown, tab);
    shown
}

/// The marker's radius in points.
const MARKER_RADIUS: f32 = 7.0;

/// Draws a ring with an exclamation mark at each Violation, over the canvas: red for
/// errors, amber for warnings, faded when waived, thicker for the one last gone to.
fn markers(ui: &Ui, shown: &CanvasShown, tab: &DocumentTab) {
    let rect = shown.rect;
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    for marker in tab.markers.iter() {
        let at = shown.screen_point((marker.at.x.0 as f64, marker.at.y.0 as f64));
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
