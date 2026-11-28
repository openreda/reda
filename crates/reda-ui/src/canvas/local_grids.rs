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

/// The Local Grids of `grids` whose grid is drawn: shown and enabled (a disabled one does
/// not snap, so the everyday grid stays in force there), while Show Grids & Guides
/// (`aids_shown`) is on.
fn drawn(grids: &[LocalGrid], aids_shown: bool) -> impl Iterator<Item = &LocalGrid> {
    (grids.iter()).filter(move |g| aids_shown && g.shown && g.enabled)
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
pub(super) fn holes(grids: &[LocalGrid], aids_shown: bool) -> Vec<GridQuad> {
    drawn(grids, aids_shown).take(MAX_HOLES).map(quad).collect()
}

/// One grid per drawn Local Grid, inside its area and out of those above it, styled as
/// `everyday` (dots or lines, coarse ones, colour) and coarsened at `nm_per_px`.
pub(super) fn specs(
    grids: &[LocalGrid],
    aids_shown: bool,
    everyday: &GridSpec,
    nm_per_px: f64,
) -> Vec<GridSpec> {
    let grids: Vec<&LocalGrid> = drawn(grids, aids_shown).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use reda_engine::model::geom::{Millideg, Nm, Point};
    use reda_engine::model::{CartesianGrid, TypedLength, Uuid};
    use reda_engine::render::Rgba;

    /// A 4 mm square Local Grid from (`x`, 0) mm every 0.3 mm, turned 90°.
    fn grid(n: u128, x: i64) -> LocalGrid {
        let step: TypedLength = "0.3mm".parse().unwrap();
        LocalGrid::new(
            Uuid::from_u128(n),
            format!("G{n}"),
            LocalGridShape::Cartesian(CartesianGrid {
                origin: Point::new(Nm(x * 1_000_000), Nm(0)),
                rotation: Millideg(90_000),
                step_x: step,
                step_y: step,
                width: Nm(4_000_000),
                height: Nm(4_000_000),
            }),
        )
    }

    fn everyday() -> GridSpec {
        let mut spec = GridSpec::new((0.0, 0.0), (500_000.0, 500_000.0), Rgba([1, 2, 3, 4]));
        spec.lines = true;
        spec
    }

    #[test]
    fn each_drawn_local_grid_is_a_hole_in_the_everyday_grid_and_its_own_turned_grid() {
        let grids = [grid(1, 0), grid(2, 2)];
        let holes = holes(&grids, true);
        assert_eq!(holes.len(), 2);
        // Turned 90°: its X runs up the Document, its Y to the left.
        assert_eq!(holes[0].corner, (0.0, 0.0));
        assert_eq!(holes[0].a, (0.0, 4_000_000.0));
        assert_eq!(holes[0].b, (-4_000_000.0, 0.0));
        let specs = specs(&grids, true, &everyday(), 10_000.0);
        assert_eq!(specs.len(), 2);
        let (first, second) = (&specs[0], &specs[1]);
        assert_eq!(first.pitch, (300_000.0, 300_000.0));
        assert_eq!(first.angle, std::f64::consts::FRAC_PI_2);
        assert_eq!(first.inside, Some(holes[0]));
        assert!(
            first.outside.is_empty(),
            "the first in the list is drawn everywhere in it"
        );
        assert_eq!(
            second.outside,
            [holes[0]],
            "the one above it wins where they overlap"
        );
        assert!(first.lines, "styled as the everyday grid");
        assert_eq!(first.paper, None);
    }

    #[test]
    fn a_local_grid_coarsens_and_hides_by_the_everyday_rules() {
        let grids = [grid(1, 0)];
        // 0.3 mm at 0.1 mm a pixel is 3 px: drawn ten times as far apart.
        assert_eq!(
            specs(&grids, true, &everyday(), 100_000.0)[0].pitch,
            (3_000_000.0, 3_000_000.0)
        );
        // Past a hundred times, hidden; its area still has no everyday grid.
        assert!(specs(&grids, true, &everyday(), 10_000_000.0).is_empty());
        assert_eq!(holes(&grids, true).len(), 1);
    }

    #[test]
    fn hidden_disabled_or_aids_off_local_grids_leave_the_everyday_grid_alone() {
        let mut hidden = grid(1, 0);
        hidden.shown = false;
        let mut disabled = grid(2, 0);
        disabled.enabled = false;
        let grids = [hidden, disabled];
        assert!(holes(&grids, true).is_empty());
        assert!(specs(&grids, true, &everyday(), 10_000.0).is_empty());
        let grids = [grid(3, 0)];
        assert!(holes(&grids, false).is_empty());
        assert!(specs(&grids, false, &everyday(), 10_000.0).is_empty());
    }
}
