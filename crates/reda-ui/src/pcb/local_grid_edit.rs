//! The handles of the one selected Local Grid (docs/spec/v0.3/grids-guides.md#local-grids):
//! the middle of its far X edge sets its width, the middle of its far Y edge its height,
//! and the far corner both. Its origin, the grid points' phase, stays put. A drag snaps
//! as the cursor does, previews the area and runs one `pcb.local_grid.set` on release.
//! A locked Local Grid shows no handles.

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{CartesianGrid, LocalGrid, LocalGridShape, Uuid};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::snap::Snapped;

/// One handle of a selected Local Grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Width,
    Height,
    Corner,
}

/// The one Local Grid selected and not locked, if that is the whole selection.
pub fn selected(shell: &Shell) -> Option<LocalGrid> {
    let [object] = shell.selection().objects[..] else {
        return None;
    };
    let grid = shell.engine().project()?.pcb()?.local_grid(object)?;
    (!grid.locked).then(|| grid.clone())
}

/// Where `grid`'s handles are.
pub fn handles(grid: &LocalGrid) -> Vec<(Handle, Point)> {
    let LocalGridShape::Cartesian(c) = &grid.shape;
    let (w, h) = (c.width.0 as f64, c.height.0 as f64);
    vec![
        (Handle::Width, c.point((w, h / 2.0))),
        (Handle::Height, c.point((w / 2.0, h))),
        (Handle::Corner, c.point((w, h))),
    ]
}

/// The handle of the selected Local Grid within `tolerance` of `at`, nearest first.
pub fn grab_at(shell: &Shell, at: Point, tolerance: Nm) -> Option<LocalGridEdit> {
    let grid = selected(shell)?;
    let document = shell.engine().project()?.pcb()?.uuid;
    let d = |p: Point| ((p.x.0 - at.x.0) as f64).hypot((p.y.0 - at.y.0) as f64);
    let (handle, _) = handles(&grid)
        .into_iter()
        .filter(|(_, p)| d(*p) <= tolerance.0 as f64)
        .min_by(|x, y| d(x.1).total_cmp(&d(y.1)))?;
    Some(LocalGridEdit {
        document,
        original: grid.clone(),
        edited: grid,
        handle,
    })
}

/// A handle drag in flight.
#[derive(Clone, Debug)]
pub struct LocalGridEdit {
    pub document: Uuid,
    original: LocalGrid,
    /// The Local Grid as the drag leaves it, for the preview.
    edited: LocalGrid,
    handle: Handle,
}

impl LocalGridEdit {
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// The Local Grid as the drag would leave it.
    pub fn edited(&self) -> &LocalGrid {
        &self.edited
    }

    /// The pointer is at `to`: the dragged edge follows where it snaps, never past the
    /// origin.
    pub fn follow(&mut self, shell: &Shell, to: Point) -> Snapped {
        let snapped = super::tool::snap_board(shell, to, &super::tool::board_hotspots(shell));
        let LocalGridShape::Cartesian(c) = &mut self.edited.shape;
        let (x, y) = c.local((snapped.at.x.0 as f64, snapped.at.y.0 as f64));
        let size = |v: f64, was: Nm| {
            if v.round() >= 1.0 {
                Nm(v.round() as i64)
            } else {
                was
            }
        };
        if matches!(self.handle, Handle::Width | Handle::Corner) {
            c.width = size(x, c.width);
        }
        if matches!(self.handle, Handle::Height | Handle::Corner) {
            c.height = size(y, c.height);
        }
        snapped
    }

    /// The one Command the drag runs; `None` when nothing changed.
    pub fn command(&self) -> Option<(&'static str, Value)> {
        if self.edited.shape == self.original.shape {
            return None;
        }
        let LocalGridShape::Cartesian(c) = &self.edited.shape;
        Some((
            "pcb.local_grid.set",
            json!({"uuid": self.edited.uuid, "shape": shape_param(c)}),
        ))
    }
}

/// `c` as `pcb.local_grid.*` take a shape.
fn shape_param(c: &CartesianGrid) -> Value {
    crate::panels::properties::local_grids::shape_param(c)
}

/// Draws the selected Local Grid's handles, and while one is dragged the area as it
/// would be.
pub fn draw(ui: &egui::Ui, shell: &Shell, input: &crate::canvas::CanvasInputs) {
    if shell.tool() != crate::tools::Tool::Select {
        return;
    }
    let edit = shell.pcb_tool().local_grid_edit();
    let grid = match edit {
        Some(e) => e.edited().clone(),
        None => match selected(shell) {
            Some(g) => g,
            None => return,
        },
    };
    let painter = ui.painter_at(input.rect);
    let screen = |p: Point| input.screen_point((p.x.0 as f64, p.y.0 as f64));
    let accent = ui.visuals().selection.stroke.color;
    let stroke = egui::Stroke::new(1.5, accent);
    if edit.is_some() {
        let corners: Vec<egui::Pos2> = grid.outline().into_iter().map(screen).collect();
        painter.add(egui::Shape::closed_line(corners, stroke));
    }
    let fill = ui.visuals().extreme_bg_color;
    for (_, p) in handles(&grid) {
        let rect = egui::Rect::from_center_size(screen(p), egui::vec2(8.0, 8.0));
        painter.rect(rect, 0.0, fill, stroke, egui::StrokeKind::Middle);
    }
}
