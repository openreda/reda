//! Dimensions on the canvas (docs/spec/v0.1/pcb.md#dimensions): which Hotspot a click
//! attaches an end to, and the handles of the one selected Dimension. Handles adjust each
//! end (re-attaching it to the Hotspot it is dropped on, or freeing it), the extension
//! length (`offset`) and the text position (`manual`). A drag previews the Dimension
//! and runs one `pcb.dimension.set` on release.

use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{
    Dimension, DimensionEnd, DimensionTextPosition, Hotspot, HotspotRef, PcbDocument, Uuid,
};
use reda_engine::pcb::dimension::{drawing, hotspot_point};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::snap::{Hit, Snapped};

/// The Hotspot of `object` at `at`, as a Dimension end's `ref`; `None` when none of its
/// Hotspots is exactly there.
pub fn reference_at(pcb: &PcbDocument, object: Uuid, at: Point) -> Option<HotspotRef> {
    let mut candidates: Vec<Hotspot> = Vec::new();
    if let Some(instance) = pcb.footprint(object) {
        candidates.extend(
            reda_engine::pcb::placed::placed_pads(pcb, instance)
                .into_iter()
                .map(|p| Hotspot::Pad(p.pad)),
        );
    }
    candidates.extend([
        Hotspot::Origin,
        Hotspot::Center,
        Hotspot::Start,
        Hotspot::End,
        Hotspot::Anchor,
    ]);
    let found = |hotspot: Hotspot| {
        let r = HotspotRef { object, hotspot };
        (hotspot_point(pcb, &r) == Some(at)).then_some(r)
    };
    candidates.into_iter().find_map(found).or_else(|| {
        (0..)
            .map(Hotspot::Vertex)
            .map(|hotspot| HotspotRef { object, hotspot })
            .map(|r| (r, hotspot_point(pcb, &r)))
            .take_while(|(_, p)| p.is_some())
            .find(|(_, p)| *p == Some(at))
            .map(|(r, _)| r)
    })
}

/// What a Dimension end snaps to: every Hotspot on the board, whatever Layer it is on,
/// since a Dimension on a drawing Layer measures copper and outline alike.
pub fn targets(shell: &Shell) -> Vec<crate::snap::Target> {
    let mut targets = super::tool::board_hotspots(shell);
    for t in &mut targets {
        t.layers = crate::snap::OnLayers::Every;
    }
    targets
}

/// A Dimension end where the cursor snapped: on the Hotspot it caught, else free.
pub fn end_at(pcb: &PcbDocument, snapped: Snapped) -> DimensionEnd {
    let reference = match snapped.hit {
        Hit::Hotspot { object, .. } => reference_at(pcb, object, snapped.at),
        _ => None,
    };
    DimensionEnd {
        point: snapped.at,
        reference,
    }
}

/// `end` as `pcb.dimension.add`/`set` take it: a `ref`, or a free `point`.
pub fn end_param(end: &DimensionEnd) -> Value {
    match &end.reference {
        Some(r) => json!({ "ref": r }),
        None => json!({"point": {"x": end.point.x.0, "y": end.point.y.0}}),
    }
}

/// The signed distance of `c` to the left of the line from `a` along `rotation`.
pub fn offset_of(a: Point, rotation: Millideg, c: Point) -> Nm {
    let (_, n) = axes(rotation);
    let (dx, dy) = ((c.x.0 - a.x.0) as f64, (c.y.0 - a.y.0) as f64);
    Nm((dx * n.0 + dy * n.1).round() as i64)
}

/// The direction along `rotation`, and the normal to its left.
fn axes(rotation: Millideg) -> ((f64, f64), (f64, f64)) {
    let r = (rotation.0 as f64 / 1000.0).to_radians();
    let (s, c) = r.sin_cos();
    ((c, s), (-s, c))
}

/// The direction from `a` to `b`, as a Dimension's `rotation`.
pub fn rotation_of(a: Point, b: Point) -> Millideg {
    let (dx, dy) = ((b.x.0 - a.x.0) as f64, (b.y.0 - a.y.0) as f64);
    Millideg((dy.atan2(dx).to_degrees() * 1000.0).round() as i64)
}

/// The corners of the dimension line `a` to `b` set `offset` to the left: what the
/// Dimension Tool previews before its third click.
pub fn preview_points(a: Point, b: Point, offset: Nm) -> Vec<Point> {
    let (_, n) = axes(rotation_of(a, b));
    let shift = |p: Point| {
        Point::new(
            Nm(p.x.0 + (n.0 * offset.0 as f64).round() as i64),
            Nm(p.y.0 + (n.1 * offset.0 as f64).round() as i64),
        )
    };
    vec![a, shift(a), shift(b), b]
}

/// One handle of a selected Dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    A,
    B,
    /// The middle of the dimension line: the extension length.
    Offset,
    /// The text's anchor: makes the position `manual`.
    Text,
}

/// The one Dimension selected, if that is the whole selection.
pub fn selected(shell: &Shell) -> Option<Dimension> {
    let [object] = shell.selection().objects[..] else {
        return None;
    };
    shell.engine().project()?.pcb()?.dimension(object).cloned()
}

/// Where `dimension`'s handles are.
pub fn handles(shell: &Shell, dimension: &Dimension) -> Vec<(Handle, Point)> {
    let (u, n) = axes(dimension.rotation);
    let (a, b) = (dimension.a.point, dimension.b.point);
    let along = (b.x.0 - a.x.0) as f64 * u.0 + (b.y.0 - a.y.0) as f64 * u.1;
    let off = dimension.offset.0 as f64;
    let middle = Point::new(
        Nm((a.x.0 as f64 + n.0 * off + u.0 * along / 2.0).round() as i64),
        Nm((a.y.0 as f64 + n.1 * off + u.1 * along / 2.0).round() as i64),
    );
    let mut out = vec![(Handle::A, a), (Handle::B, b), (Handle::Offset, middle)];
    if let Some(text) = drawing(dimension, shell.engine().fonts()).text {
        out.push((Handle::Text, text.position));
    }
    out
}

/// The handle of the selected Dimension within `tolerance` of `at`, nearest first.
pub fn grab_at(shell: &Shell, at: Point, tolerance: Nm) -> Option<DimensionEdit> {
    let dimension = selected(shell)?;
    let document = shell.engine().project()?.pcb()?.uuid;
    let d = |p: Point| ((p.x.0 - at.x.0) as f64).hypot((p.y.0 - at.y.0) as f64);
    let (handle, _) = handles(shell, &dimension)
        .into_iter()
        .filter(|(_, p)| d(*p) <= tolerance.0 as f64)
        .min_by(|x, y| d(x.1).total_cmp(&d(y.1)))?;
    Some(DimensionEdit {
        document,
        original: dimension.clone(),
        edited: dimension,
        handle,
        moved: false,
    })
}

/// A handle drag in flight.
#[derive(Clone, Debug)]
pub struct DimensionEdit {
    pub document: Uuid,
    original: Dimension,
    /// The Dimension as the drag leaves it, for the preview.
    edited: Dimension,
    handle: Handle,
    moved: bool,
}

impl DimensionEdit {
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// The Dimension as the drag would leave it.
    pub fn edited(&self) -> &Dimension {
        &self.edited
    }

    /// The pointer is at `to`: ends snap to Hotspots (and attach to them) or the grid.
    pub fn follow(&mut self, shell: &Shell, to: Point) -> Snapped {
        let snapped = super::tool::snap_board(shell, to, &targets(shell));
        let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
            return snapped;
        };
        self.moved = true;
        let d = &mut self.edited;
        match self.handle {
            Handle::A => d.a = end_at(pcb, snapped),
            Handle::B => d.b = end_at(pcb, snapped),
            Handle::Offset => d.offset = offset_of(d.a.point, d.rotation, snapped.at),
            Handle::Text => d.text_position = DimensionTextPosition::Manual(snapped.at),
        }
        snapped
    }

    /// The one Command the drag runs; `None` when nothing changed.
    pub fn command(&self) -> Option<(&'static str, Value)> {
        let d = &self.edited;
        if !self.moved || *d == self.original {
            return None;
        }
        let mut params = json!({"uuid": d.uuid});
        match self.handle {
            Handle::A => params["a"] = end_param(&d.a),
            Handle::B => params["b"] = end_param(&d.b),
            Handle::Offset => params["offset"] = json!(d.offset.0),
            Handle::Text => params["text_position"] = json!(d.text_position),
        }
        Some(("pcb.dimension.set", params))
    }
}

/// Draws the selected Dimension's handles, and while one is dragged the Dimension as it
/// would be.
pub fn draw(ui: &egui::Ui, shell: &Shell, input: &crate::canvas::CanvasInputs) {
    if shell.tool() != crate::tools::Tool::Select {
        return;
    }
    let edit = shell.pcb_tool().dimension_edit();
    let dimension = match edit {
        Some(e) => e.edited().clone(),
        None => match selected(shell) {
            Some(d) => d,
            None => return,
        },
    };
    let painter = ui.painter_at(input.rect);
    let screen = |p: Point| input.screen_point((p.x.0 as f64, p.y.0 as f64));
    let accent = ui.visuals().selection.stroke.color;
    let stroke = egui::Stroke::new(1.5, accent);
    if edit.is_some() {
        for line in drawing(&dimension, shell.engine().fonts()).lines {
            painter.line_segment([screen(line.from), screen(line.to)], stroke);
        }
    }
    let fill = ui.visuals().extreme_bg_color;
    for (handle, p) in handles(shell, &dimension) {
        match handle {
            Handle::A | Handle::B => {
                let rect = egui::Rect::from_center_size(screen(p), egui::vec2(8.0, 8.0));
                painter.rect(rect, 0.0, fill, stroke, egui::StrokeKind::Middle);
            }
            Handle::Offset | Handle::Text => {
                painter.circle(screen(p), 3.5, fill, stroke);
            }
        }
    }
}
