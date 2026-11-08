//! The health checks: data that passes the other checks but harms outputs. Zero-length
//! Tracks, micro-segments (Tracks no longer than the `micro_segment` length), duplicate
//! Tracks and Vias, and Zones whose outline encloses no area.
//!
//! They read only the PCB Document, never copper or Zone fills, so they are cheap enough
//! to run whenever a board opens. Each finding names the object `drc.fix` removes.

use std::collections::BTreeMap;

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    DrcCheck, Limit, PcbDocument, RuleField, RuleKind, TrackGeometry, Uuid, Violation,
};

use crate::rules::resolve;
use crate::zone::poly;

/// One thing a health check found.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub check: DrcCheck,
    /// The objects involved: for a duplicate, the copy kept and then the duplicate.
    pub objects: Vec<Uuid>,
    /// The object `drc.fix` removes: the last of `objects`.
    pub remove: Uuid,
    at: Point,
    layers: Vec<Uuid>,
    /// A micro-segment's length, its limit and the record that set it.
    length: Option<(Nm, Nm, Uuid)>,
}

impl Finding {
    /// The Violation it is reported as, with `severity`.
    pub fn violation(&self, pcb: &PcbDocument, severity: reda_model::Severity) -> Violation {
        let mut v = Violation::new(self.check.id(), severity)
            .located(pcb.uuid, self.at)
            .on_layers(self.layers.iter().copied())
            .objects(self.objects.iter().copied());
        if let Some((measured, required, rule)) = self.length {
            v = v
                .length(measured, required, Limit::Min, rule)
                .arg("length", measured)
                .arg("max_length", required);
        }
        v
    }
}

/// Everything the health checks find on `pcb`, check by check in table order, each
/// check's in UUID order.
pub fn findings(pcb: &PcbDocument) -> Vec<Finding> {
    let mut out = Vec::new();
    for track in pcb.tracks() {
        if track.geometry.length() == 0.0 {
            out.push(Finding {
                check: DrcCheck::ZeroLengthTrack,
                objects: vec![track.uuid],
                remove: track.uuid,
                at: track.geometry.ends().0,
                layers: vec![track.layer],
                length: None,
            });
        }
    }
    for track in pcb.tracks() {
        let length = track.geometry.length();
        if length == 0.0 {
            continue;
        }
        let Some(limit) = resolve(pcb, track.net, RuleKind::MicroSegment, RuleField::MaxLength)
        else {
            continue;
        };
        let max = limit.value.length().expect("a length");
        if length <= max.0 as f64 {
            let (a, b) = track.geometry.ends();
            out.push(Finding {
                check: DrcCheck::MicroSegment,
                objects: vec![track.uuid],
                remove: track.uuid,
                at: Point::new(
                    Nm(a.x.0 + (b.x.0 - a.x.0) / 2),
                    Nm(a.y.0 + (b.y.0 - a.y.0) / 2),
                ),
                layers: vec![track.layer],
                length: Some((Nm(length.round() as i64), max, limit.rule)),
            });
        }
    }
    out.extend(duplicates(pcb));
    for zone in pcb.active_zones() {
        let vertices = pcb.zone_vertices(zone);
        if vertices.is_empty() {
            continue;
        }
        if poly::polygon(poly::contour(vertices)).is_empty() {
            out.push(Finding {
                check: DrcCheck::ZeroAreaZone,
                objects: vec![zone.uuid],
                remove: zone.uuid,
                at: vertices[0].point(),
                layers: vec![zone.layer],
                length: None,
            });
        }
    }
    out
}

/// Tracks, and Vias, that repeat an earlier one (by UUID) exactly: same Layer or span,
/// Net and size, and the same centreline in either direction.
fn duplicates(pcb: &PcbDocument) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut tracks: BTreeMap<_, Uuid> = BTreeMap::new();
    for track in pcb.tracks() {
        let key = (
            track.layer,
            track.net,
            track.width,
            centreline(track.geometry),
        );
        match tracks.get(&key) {
            Some(&first) => out.push(Finding {
                check: DrcCheck::DuplicateCopper,
                objects: vec![first, track.uuid],
                remove: track.uuid,
                at: track.geometry.ends().0,
                layers: vec![track.layer],
                length: None,
            }),
            None => {
                tracks.insert(key, track.uuid);
            }
        }
    }
    let mut vias: BTreeMap<_, Uuid> = BTreeMap::new();
    for via in pcb.vias() {
        let key = (
            (via.at.x, via.at.y),
            via.net,
            via.diameter,
            via.hole,
            (via.span.from, via.span.to),
        );
        match vias.get(&key) {
            Some(&first) => out.push(Finding {
                check: DrcCheck::DuplicateCopper,
                objects: vec![first, via.uuid],
                remove: via.uuid,
                at: via.at,
                layers: pcb.span_layers(via.span),
                length: None,
            }),
            None => {
                vias.insert(key, via.uuid);
            }
        }
    }
    out
}

/// A Track's centreline in one direction, whichever way it was drawn.
fn centreline(geometry: TrackGeometry) -> ((Nm, Nm), (Nm, Nm), Millideg) {
    let (from, to, sweep) = match geometry {
        TrackGeometry::Line { from, to } => (from, to, Millideg::ZERO),
        TrackGeometry::Arc { from, to, sweep } => (from, to, sweep),
    };
    let (a, b) = ((from.x, from.y), (to.x, to.y));
    if a <= b {
        (a, b, sweep)
    } else {
        (b, a, -sweep)
    }
}
