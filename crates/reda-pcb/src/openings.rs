//! One opening resolver for manufacturing, silkscreen clearing and DRC. Intentional
//! tenting/absence returns no entry; an opening shrunk to nothing returns a collapsed entry.

use reda_model::geom::{CornerCut, CornerRect, Millideg, Nm, Point};
use reda_model::{
    MaskSide, PadKind, PadShape, Paste, PasteExpansion, PcbDocument, RuleField, RuleKind, Side,
    Uuid,
};

use crate::placed::{PlacedPad, placed_free_pad, placed_pads};
use crate::rules::{limit, paste_expansion, resolve as rule_value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mask,
    Paste,
}

#[derive(Clone, Debug)]
pub struct Opening {
    pub objects: Vec<Uuid>,
    pub side: Side,
    pub kind: Kind,
    /// `None` means a negative expansion left nothing, which DRC must report.
    pub geometry: Option<PlacedPad>,
    /// The copper the opening is over: the Pad or Via itself.
    pub copper: PlacedPad,
    pub at: Point,
}

/// All openings in board coordinates. The board's rule records, resolved per Net, are
/// authoritative; a manufacturing job never substitutes its preset's values.
pub fn resolve(pcb: &PcbDocument) -> Vec<Opening> {
    let mut out = Vec::new();
    for instance in pcb.footprints() {
        for pad in placed_pads(pcb, instance) {
            pad_openings(pcb, &pad, vec![instance.uuid, pad.pad], None, &mut out);
        }
    }
    for pad in pcb.free_pads().map(placed_free_pad) {
        pad_openings(pcb, &pad, vec![pad.pad], None, &mut out);
    }
    for via in pcb.vias() {
        let pad = PlacedPad {
            pad: via.uuid,
            number: String::new(),
            kind: PadKind::PlatedThroughHole,
            shape: PadShape::Circle,
            at: via.at,
            rotation: Millideg::ZERO,
            outline: CornerRect::uniform(
                via.diameter.start,
                via.diameter.start,
                Point::default(),
                CornerCut::Round(Nm(via.diameter.start.0 / 2)),
            ),
            hole: Some(reda_model::Hole::new(via.hole, via.hole)),
            side: None,
            net: via.net,
            mask: via.mask,
            paste: None,
            zone_connection: None,
        };
        pad_openings(pcb, &pad, vec![via.uuid], Some(via), &mut out);
    }
    out
}

/// The openings of a Library Footprint's Pads, unplaced at its own origin (top side up),
/// from `pcb`'s rules: the Footprint editor's mask and paste preview.
pub fn element_openings(pcb: &PcbDocument, footprint: &reda_model::Footprint) -> Vec<Opening> {
    let mut out = Vec::new();
    for pad in &footprint.pads {
        let placed = PlacedPad {
            pad: pad.uuid,
            number: pad.number.clone(),
            kind: pad.kind,
            shape: pad.shape,
            at: pad.at,
            rotation: pad.rotation,
            outline: pad.outline(),
            hole: pad.hole,
            side: pad.copper_side(),
            net: None,
            mask: pad.mask,
            paste: pad.paste.clone(),
            zone_connection: pad.zone_connection,
        };
        pad_openings(pcb, &placed, vec![pad.uuid], None, &mut out);
    }
    out
}

fn flag(pcb: &PcbDocument, net: Option<Uuid>, field: RuleField) -> bool {
    rule_value(pcb, net, RuleKind::MaskExpansion, field)
        .and_then(|r| r.value.flag())
        .unwrap_or(false)
}

fn pad_openings(
    pcb: &PcbDocument,
    pad: &PlacedPad,
    objects: Vec<Uuid>,
    via: Option<&reda_model::Via>,
    out: &mut Vec<Opening>,
) {
    let mask = pad.mask.unwrap_or_default();
    for side in [Side::Top, Side::Bottom] {
        if pad.side.is_some_and(|s| s != side) || via.is_some_and(|v| !v.span.reaches(pcb, side)) {
            continue;
        }
        let (own, field, tent) = match side {
            Side::Top => (mask.top, RuleField::Top, RuleField::TentViasTop),
            Side::Bottom => (mask.bottom, RuleField::Bottom, RuleField::TentViasBottom),
        };
        let expansion = match own {
            Some(MaskSide::Mode(_)) => continue,
            Some(MaskSide::Expansion { expansion }) => expansion,
            None if via.is_some() && flag(pcb, pad.net, tent) => continue,
            None => limit(pcb, pad.net, RuleKind::MaskExpansion, field).unwrap_or(Nm::ZERO),
        };
        let from_hole = mask
            .from_hole_edge
            .unwrap_or_else(|| flag(pcb, pad.net, RuleField::FromHoleEdge));
        let mut copper = pad.clone();
        if let Some(via) = via
            && let Some(end) = via.diameter.end
            && pcb
                .layers()
                .layer(via.span.to)
                .is_some_and(|l| l.side == Some(side))
        {
            copper.outline =
                CornerRect::uniform(end, end, Point::default(), CornerCut::Round(Nm(end.0 / 2)));
        }
        let mut opening = copper.clone();
        if from_hole && let Some(hole) = pad.hole {
            opening.outline = CornerRect::uniform(
                hole.width,
                hole.height,
                Point::default(),
                CornerCut::Round(Nm(hole.width.0.min(hole.height.0) / 2)),
            );
            opening.rotation = (pad.rotation + hole.rotation).normalized();
        }
        let geometry = opening
            .outline
            .grown(expansion)
            .map(|outline| PlacedPad { outline, ..opening });
        out.push(Opening {
            objects: objects.clone(),
            side,
            kind: Kind::Mask,
            geometry,
            copper,
            at: pad.at,
        });
    }
    if pad.kind != PadKind::Smd {
        return;
    }
    let paste = pad
        .paste
        .clone()
        .unwrap_or_else(|| match paste_expansion(pcb, pad.net) {
            PasteExpansion::Expansion { expansion } => Paste::Expansion { expansion },
            PasteExpansion::Percent { percent } => Paste::Percent { percent },
        });
    let outlines = match paste {
        Paste::Mode(_) => return,
        Paste::Expansion { expansion } => vec![pad.outline.grown(expansion)],
        Paste::Percent { percent } => vec![scaled(pad.outline, percent)],
        Paste::Windows { windows } => windows.into_iter().map(|w| Some(w.outline())).collect(),
    };
    for outline in outlines {
        out.push(Opening {
            objects: objects.clone(),
            side: pad.side.unwrap_or(Side::Top),
            kind: Kind::Paste,
            geometry: outline.map(|outline| PlacedPad {
                outline,
                ..pad.clone()
            }),
            copper: pad.clone(),
            at: pad.at,
        });
    }
}

fn scaled(mut outline: CornerRect, percent: i32) -> Option<CornerRect> {
    let factor = ((100.0 + f64::from(percent)) / 100.0).sqrt();
    let scale = |n: Nm| Nm((n.0 as f64 * factor).round() as i64);
    outline.width = scale(outline.width);
    outline.height = scale(outline.height);
    outline.corners = outline.corners.map(|c| match c {
        CornerCut::Square => c,
        CornerCut::Round(r) => CornerCut::Round(scale(r)),
        CornerCut::Chamfer(r) => CornerCut::Chamfer(scale(r)),
    });
    (outline.width > Nm::ZERO && outline.height > Nm::ZERO).then_some(outline)
}
