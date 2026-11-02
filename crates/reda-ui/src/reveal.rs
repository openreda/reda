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
        let _ = self.run_reporting("selection.set", json!({ "objects": objects }));
        if let Some(tab) = self.document_tab_mut(document)
            && let Some(frame) = frame(&tab.scene.scene, objects)
        {
            tab.zoom_to = Some(frame);
        }
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
