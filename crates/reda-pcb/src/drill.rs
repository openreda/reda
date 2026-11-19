//! Physical drilled holes, independent of the lands present on each Layer.
use crate::copper::{
    Item,
    shape::{Core, Prim, Shape},
};
use crate::placed::{self, PlacedPad};
use reda_model::geom::{CornerRect, Nm, Point};
use reda_model::{ConnectionTarget, Hole, PadKind, PcbDocument, RuleField, Uuid, ZoneConnection};

/// One drilled hole on the board.
pub(crate) struct DrillHole {
    /// The Via, or the Footprint instance then the Pad, or the Free Pad.
    pub objects: Vec<Uuid>,
    pub size: Hole,
    /// Its Net, which picks its rules.
    pub net: Option<Uuid>,
    /// Whether it is a Via's, which `via_size` limits rather than `hole_size`.
    pub via: bool,
    pub item: Item,
    pub own: Option<ZoneConnection>,
    /// The copper ring around a plated hole; `None` for a bare (non-plated) hole.
    pub(crate) ring: Option<Rings>,
    pub shape: Shape,
}

/// A Via follows its drilling span. Pad rings are measured in board-side order.
#[derive(Clone, Copy)]
pub(crate) enum Rings {
    Via,
    Pad { top: Nm, middle: Nm, bottom: Nm },
}

/// Every hole, Vias first, then Footprint Pads, then Free Pads, each in UUID order.
pub(crate) fn holes(pcb: &PcbDocument) -> Vec<DrillHole> {
    let mut out = Vec::new();
    for via in pcb.vias() {
        let size = Hole::new(via.hole, via.hole);
        out.push(DrillHole {
            objects: vec![via.uuid],
            size,
            net: via.net,
            via: true,
            item: Item::Via { uuid: via.uuid },
            own: via.zone_connection,
            ring: Some(Rings::Via),
            shape: hole_shape((via.at, via.at), size),
        });
    }
    let pad_hole = |item, objects: Vec<Uuid>, pad: &PlacedPad| {
        let size = pad.hole?;
        let ring = (pad.kind == PadKind::PlatedThroughHole).then(|| {
            let top = annular_ring(&pad.outline, size);
            Rings::Pad {
                top,
                middle: pad.middle.map_or(top, |(_, o)| annular_ring(&o, size)),
                bottom: pad.bottom.map_or(top, |(_, o)| annular_ring(&o, size)),
            }
        });
        Some(DrillHole {
            objects,
            size,
            net: pad.net,
            via: false,
            item,
            own: pad.zone_connection,
            ring,
            shape: hole_shape(pad.hole_ends()?, size),
        })
    };
    for instance in pcb.footprints() {
        for pad in placed::placed_pads(pcb, instance) {
            out.extend(pad_hole(
                Item::Pad {
                    footprint: instance.uuid,
                    pad: pad.pad,
                },
                vec![instance.uuid, pad.pad],
                &pad,
            ));
        }
    }
    for pad in pcb.free_pads() {
        out.extend(pad_hole(
            Item::FreePad { uuid: pad.uuid },
            vec![pad.uuid],
            &placed::placed_free_pad(pad),
        ));
    }
    out
}

/// The narrowest copper between a Pad's hole and the edge of its copper, negative when
/// the hole reaches outside it. The copper is convex, so the narrowest point is at an end
/// of the slot's centre line.
fn annular_ring(outline: &CornerRect, hole: Hole) -> Nm {
    let (a, b) = hole.ends();
    let inside = outline.edge_distance(a).min(outline.edge_distance(b));
    Nm((inside - hole.diameter().0 as f64 / 2.0).floor() as i64)
}

/// A hole's outline: a disc, or a slot between the `ends` of its centre line.
fn hole_shape(ends: (Point, Point), size: Hole) -> Shape {
    let end = |p: Point| [p.x.0, p.y.0];
    Shape {
        prims: vec![Prim {
            core: Core::Seg(end(ends.0), end(ends.1)),
            radius: size.diameter().0 / 2,
        }],
    }
}

impl DrillHole {
    pub fn layers(&self, pcb: &PcbDocument) -> Vec<Uuid> {
        if let Item::Via { uuid } = self.item {
            pcb.span_layers(pcb.via(uuid).expect("Via hole").span)
        } else {
            pcb.copper_layers().map(|l| l.uuid).collect()
        }
    }

    pub fn field(&self) -> RuleField {
        if self.via {
            RuleField::HoleVia
        } else {
            RuleField::HolePad
        }
    }

    pub fn connection(&self, pcb: &PcbDocument, layer: Uuid) -> crate::rules::Connection {
        let target = if self.via {
            ConnectionTarget::Via
        } else {
            ConnectionTarget::ThroughHole
        };
        crate::rules::zone_connection_on(pcb, self.net, Some(layer), target, self.own.as_ref())
    }
}
