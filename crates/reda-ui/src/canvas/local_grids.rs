//! A board's Local Grids on its canvas (docs/spec/v0.3/grids-guides.md#drawing): each
//! draws its own dots or lines inside a faint boundary, and the everyday grid is not drawn
//! inside it. Where two overlap, the one higher in the list is drawn there. The grids
//! follow the visible-grid switch and coarsen and hide as the everyday grid does; the
//! boundaries follow Show Grids & Guides. They are editor aids, drawn over the canvas and
//! never part of the Scene.

use egui::{Color32, Shape, Stroke, Ui};
use reda_engine::model::{LocalGrid, LocalGridShape};
use reda_engine::render::gpu::{GridQuad, GridSpec, MAX_HOLES};
use reda_engine::render::{BoardColor, ColorKey, Palette};

use super::camera::grid_pitch_xy;
use super::widget::CanvasInputs;
use crate::shell::DocumentTab;

/// The Local Grids whose grid is drawn: shown and enabled (a disabled one does not snap,
/// so the everyday grid stays in force there), while Show Grids & Guides is on.
fn drawn(tab: &DocumentTab) -> impl Iterator<Item = &LocalGrid> {
    let shown = tab.aids_shown;
    (tab.scene.local_grids.iter()).filter(move |g| shown && g.shown && g.enabled)
}

/// A Local Grid's area as the grid shader takes it.
pub(super) fn quad(grid: &LocalGrid) -> GridQuad {
    let LocalGridShape::Cartesian(c) = &grid.shape;
    let (u, v) = c.axes();
    let (w, h) = (c.width.0 as f64, c.height.0 as f64);
    GridQuad {
        corner: (c.origin.x.0 as f64, c.origin.y.0 as f64),
        a: (u.0 * w, u.1 * w),
        b: (v.0 * h, v.1 * h),
    }
}

/// The areas the everyday grid leaves out.
pub(super) fn holes(tab: &DocumentTab) -> Vec<GridQuad> {
    drawn(tab).take(MAX_HOLES).map(quad).collect()
}

/// One grid per drawn Local Grid, inside its area and out of those above it, styled as
/// `everyday` (dots or lines, coarse ones, colour) and coarsened at `nm_per_px`.
pub(super) fn specs(tab: &DocumentTab, everyday: &GridSpec, nm_per_px: f64) -> Vec<GridSpec> {
    let grids: Vec<&LocalGrid> = drawn(tab).collect();
    grids
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            let LocalGridShape::Cartesian(c) = &g.shape;
            let base = (c.step_x.nm().0 as f64, c.step_y.nm().0 as f64);
            let pitch = grid_pitch_xy(base, nm_per_px)?;
            Some(GridSpec {
                origin: (c.origin.x.0 as f64, c.origin.y.0 as f64),
                pitch,
                angle: (c.rotation.0 as f64 / 1000.0).to_radians(),
                inside: Some(quad(g)),
                outside: grids[..i].iter().take(MAX_HOLES).map(|g| quad(g)).collect(),
                paper: None,
                ..everyday.clone()
            })
        })
        .collect()
}

/// Draws the boundary of each shown Local Grid, faint, or stronger when selected, with
/// its name by its first corner; nothing while Show Grids & Guides is off.
pub(super) fn draw(ui: &Ui, input: &CanvasInputs, tab: &DocumentTab, palette: &Palette) {
    if !tab.aids_shown || tab.scene.local_grids.is_empty() {
        return;
    }
    let painter = ui.painter_at(input.rect);
    let [r, g, b, _] = palette.color(ColorKey::Board(BoardColor::Guide)).0;
    for grid in tab.scene.local_grids.iter().filter(|g| g.shown) {
        let selected = tab.selection.objects.contains(&grid.uuid);
        let (alpha, width) = if selected { (255, 2.0) } else { (110, 1.0) };
        let color = Color32::from_rgba_unmultiplied(r, g, b, alpha);
        let corners: Vec<egui::Pos2> = (grid.outline().iter())
            .map(|p| input.screen_point((p.x.0 as f64, p.y.0 as f64)))
            .collect();
        let Some(&first) = corners.first() else {
            continue;
        };
        painter.add(Shape::closed_line(corners, Stroke::new(width, color)));
        painter.text(
            first + egui::vec2(3.0, -3.0),
            egui::Align2::LEFT_BOTTOM,
            &grid.name,
            egui::FontId::proportional(10.0),
            color,
        );
    }
}
