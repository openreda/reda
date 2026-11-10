//! Showing objects to the user: open their Document, select them and zoom the canvas to
//! them. Find uses it for a match; cross-selecting between the schematic and the board can
//! use it for the other side.

use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};
use reda_engine::render::Scene;
use serde_json::json;

use crate::shell::Shell;

/// The smallest box a reveal frames, so one small object is not zoomed in on absurdly
/// far: 20 mm.
const MIN_FRAME: i64 = 20_000_000;

impl Shell {
    /// Opens `document` (`document.open`), selects `objects` (`selection.set`) and zooms
    /// its canvas to them. Objects the Document does not draw are still selected.
    pub fn reveal(&mut self, document: Uuid, objects: &[Uuid]) {
        if self
            .run_reporting("document.open", json!({ "document": document }))
            .is_err()
        {
            return;
        }
        // A mask already up (Find's Mask Matching) stays.
        let dim = !self.highlight().mask;
        self.show_result(document, objects, None, dim);
    }

    /// Shows a result as the shared highlight Preference says
    /// (docs/spec/v0.1/editing.md#highlighting): selects `objects` (those the Project has),
    /// zooms `document`'s canvas to `frame` (else to what it draws of them) as close as
    /// its closeness says, and with `dim` dims everything else. Messages, Find and Show
    /// use it.
    pub fn show_result(
        &mut self,
        document: Uuid,
        objects: &[Uuid],
        frame: Option<(Point, Point)>,
        dim: bool,
    ) {
        let prefs = self.preferences().editing.highlight.clone();
        let existing: Vec<Uuid> = match self.engine().project() {
            Some(project) => (objects.iter().copied())
                .filter(|o| project.object(*o).is_some())
                .collect(),
            None => Vec::new(),
        };
        if prefs.select {
            let _ = self.run_reporting("selection.set", json!({ "objects": existing }));
        }
        if prefs.zoom
            && let Some(tab) = self.document_tab_mut(document)
            && let Some(frame) = frame.or_else(|| self::frame(&tab.scene.scene, objects))
        {
            tab.zoom_to = Some(closer(frame, prefs.closeness));
        }
        if dim && prefs.dim && !existing.is_empty() {
            let _ = self.run_reporting("view.highlight", json!({ "objects": existing }));
        }
    }
}

impl Shell {
    /// Zooms `document`'s canvas to what it draws of `objects`, as close as `closeness`
    /// says (the shared highlight Preference's: 0 leaves room around them, 1 is tight);
    /// false when it draws none of them.
    pub fn zoom_closely(&mut self, document: Uuid, objects: &[Uuid], closeness: f64) -> bool {
        let Some(tab) = self.document_tab_mut(document) else {
            return false;
        };
        let Some(frame) = frame(&tab.scene.scene, objects) else {
            return false;
        };
        tab.zoom_to = Some(closer(frame, closeness));
        true
    }
}

/// `frame` grown about its centre so the objects fill less of the view the less close
/// `closeness` (0..1) is: tight at 1, four times the size at 0.
pub fn closer((lo, hi): (Point, Point), closeness: f64) -> (Point, Point) {
    let scale = 1.0 / (0.25 + 0.75 * closeness.clamp(0.0, 1.0));
    let grow = |a: Nm, b: Nm| {
        let (c, half) = ((a.0 + b.0) as f64 / 2.0, (b.0 - a.0) as f64 / 2.0 * scale);
        (Nm((c - half).round() as i64), Nm((c + half).round() as i64))
    };
    let (x0, x1) = grow(lo.x, hi.x);
    let (y0, y1) = grow(lo.y, hi.y);
    (Point::new(x0, y0), Point::new(x1, y1))
}

impl Shell {
    /// Zooms `document`'s canvas to what it draws of the selection (Pads by their
    /// Footprint) when it next lays out; false when it draws none of it.
    pub fn zoom_to_selection(&mut self, document: Uuid) -> bool {
        let selection = self.selection();
        let objects: Vec<Uuid> = (selection.objects.iter().copied())
            .chain(selection.pads.iter().map(|p| p.footprint))
            .collect();
        self.zoom_to_objects(document, &objects)
    }

    /// Zooms `document`'s canvas to what it draws of `objects` when it next lays out;
    /// false when it draws none of them.
    pub fn zoom_to_objects(&mut self, document: Uuid, objects: &[Uuid]) -> bool {
        let Some(tab) = self.document_tab_mut(document) else {
            return false;
        };
        let Some(frame) = frame(&tab.scene.scene, objects) else {
            return false;
        };
        tab.zoom_to = Some(frame);
        true
    }
}

/// The box around what `scene` draws of `objects`, grown to at least [`MIN_FRAME`] across.
pub fn frame(scene: &Scene, objects: &[Uuid]) -> Option<(Point, Point)> {
    let (lo, hi) = objects
        .iter()
        .flat_map(|o| scene.items_of(*o).filter_map(|i| i.primitive.bounds()))
        .reduce(|(alo, ahi), (blo, bhi)| {
            (
                Point::new(alo.x.min(blo.x), alo.y.min(blo.y)),
                Point::new(ahi.x.max(bhi.x), ahi.y.max(bhi.y)),
            )
        })?;
    let grow = |a: Nm, b: Nm| {
        let short = (MIN_FRAME - (b.0 - a.0)).max(0);
        (Nm(a.0 - short / 2), Nm(b.0 + short - short / 2))
    };
    let (x0, x1) = grow(lo.x, hi.x);
    let (y0, y1) = grow(lo.y, hi.y);
    Some((Point::new(x0, y0), Point::new(x1, y1)))
}
