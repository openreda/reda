//! Stored marker positions and involved Layers for PCB checks. A check can supply a
//! more specific location (a contact or dangling end); these defaults use its objects.
use super::Board;
use crate::copper::Item;
use crate::copper::shape::BoardShape as _;
use reda_model::geom::{Bounds, Nm, Point};
use reda_model::{DrcCheck, GraphicShape, LayerKind, PcbDocument, Violation, ViolationArg};

/// The centre of the box around `points`; `None` when there are none.
fn center(points: impl IntoIterator<Item = Point>) -> Option<Point> {
    Bounds::of_points(points).map(Bounds::center)
}

/// Where a Violation about the whole board sits: the Board Outline's centre, or the
/// origin when there is no outline.
pub(crate) fn board_center(pcb: &PcbDocument) -> Point {
    pcb.outline()
        .and_then(|o| center(o.vertices.iter().map(|v| v.point())))
        .unwrap_or(Point::new(Nm::ZERO, Nm::ZERO))
}

pub(super) fn complete(board: &Board<'_>, check: DrcCheck, v: &mut Violation) {
    // Rule-file problems have no canvas location (checks.md#violations).
    if matches!(check, DrcCheck::RuleInvalid | DrcCheck::RuleOutsidePreset) {
        return;
    }
    let pcb = board.pcb;
    v.document.get_or_insert(pcb.uuid);
    if v.at.is_some() && !v.layers.is_empty() {
        return;
    }
    if v.at.is_none()
        && let (Some(ViolationArg::Int(x)), Some(ViolationArg::Int(y))) =
            (v.args.get("x"), v.args.get("y"))
    {
        v.at = Some(Point::new(Nm(*x), Nm(*y)));
    }
    let mut candidates = Vec::new();
    let mut layers = Vec::new();
    // Look up only the named objects. Scanning the whole copper index for every
    // Violation would make marker enrichment quadratic on a board with many results.
    let pads = v
        .objects
        .windows(2)
        .filter(|pair| pcb.footprint(pair[0]).is_some())
        .map(|pair| Item::Pad {
            footprint: pair[0],
            pad: pair[1],
        });
    let stored = v.objects.iter().filter_map(|&uuid| {
        if pcb.track(uuid).is_some() {
            Some(Item::Track { uuid })
        } else if pcb.via(uuid).is_some() {
            Some(Item::Via { uuid })
        } else if pcb.free_pad(uuid).is_some() {
            Some(Item::FreePad { uuid })
        } else {
            None
        }
    });
    for item in pads.chain(stored) {
        for (layer, shape) in board.copper.item_pieces(item) {
            let envelope = shape.envelope();
            candidates.push(Point::from(envelope.lower()).midpoint(envelope.upper().into()));
            layers.push(layer);
        }
    }
    for uuid in &v.objects {
        if let Some(f) = pcb.footprint(*uuid) {
            candidates.push(f.at);
            if matches!(
                check,
                DrcCheck::CourtyardOverlap | DrcCheck::FootprintOutsideBoard
            ) && let Some(l) = pcb.layers().find(LayerKind::Courtyard, Some(f.side))
            {
                layers.push(l.uuid);
            }
        } else if let Some(p) = pcb.free_pad(*uuid) {
            candidates.push(p.at);
            if p.hole.is_some() {
                layers.extend(pcb.copper_layers().map(|l| l.uuid));
            }
        } else if let Some(t) = pcb.text(*uuid) {
            candidates.push(t.position);
            layers.push(t.layer);
        } else if let Some(g) = pcb.graphic(*uuid) {
            let (GraphicShape::Line { from, to }
            | GraphicShape::Arc { from, to, .. }
            | GraphicShape::Rect { from, to, .. }) = g.shape;
            candidates.push(from.midpoint(to));
            layers.push(g.layer);
        } else if let Some(z) = pcb.zone(*uuid) {
            if let Some(p) = center(pcb.zone_vertices(z).iter().map(|v| v.point())) {
                candidates.push(p);
            }
            layers.push(z.layer);
        } else if let Some(k) = pcb.keepout(*uuid) {
            if let Some(p) = center(k.vertices().iter().map(|v| v.point())) {
                candidates.push(p);
            }
            layers.extend(pcb.keepout_layers(k));
        }
    }
    v.at = Some(
        v.at.or_else(|| candidates.first().copied())
            .unwrap_or_else(|| board_center(pcb)),
    );
    if v.layers.is_empty() {
        if layers.is_empty() {
            layers.extend(pcb.copper_layers().map(|l| l.uuid));
        }
        layers.sort();
        layers.dedup();
        v.layers = layers;
    }
}
