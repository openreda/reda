//! The canvas widget: lays out the canvas, turns pointer input into camera moves and adds
//! the paint callback.

use egui::{PointerButton, Role, Sense, Ui, WidgetInfo};
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

/// Shows `tab`'s canvas filling the space left in `ui`.
pub fn canvas(ui: &mut Ui, tab: &mut DocumentTab, theme: Theme) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let title = tab.title.clone();
    response.widget_info(|| {
        WidgetInfo::labeled(
            Role::Canvas,
            true,
            tr!("canvas-description", document = title.as_str()),
        )
    });

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

    let ppp = ui.ctx().pixels_per_point();
    let view = camera.view(rect, ppp);
    let palette = Palette::shipped(theme);
    let grid = grid_pitch(SCHEMATIC_GRID_NM, view.nm_per_px).map(|pitch| GridSpec {
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
    };
    paint(ui.painter(), rect, frame);
}

/// The id a tab's canvas keeps its GPU state under.
pub fn canvas_id(tab: &DocumentTab) -> egui::Id {
    egui::Id::NULL.with(("canvas", tab.document))
}
