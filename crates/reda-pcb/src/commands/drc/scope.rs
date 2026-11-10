//! The scope of a narrowed DRC run: which Violations it lists. A Violation is in scope
//! when any of its objects is; the whole board is still checked first.

use reda_command::CommandError;
use reda_model::geom::{Bounds, Nm, Point};
use reda_model::{PcbDocument, Uuid, Violation};
use schemars::JsonSchema;
use serde::Deserialize;

use super::super::outline::PointParam;
use super::DrcRunParams;
use crate::placed::{footprint_bounds, free_pad_bounds};

/// A box on the board, by two opposite corners in either order. Its edges count as
/// inside.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AreaParam {
    pub from: PointParam,
    pub to: PointParam,
}

impl std::fmt::Debug for AreaParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.bounds())
    }
}

impl AreaParam {
    fn bounds(&self) -> Bounds {
        let (a, b) = (self.from.point(), self.to.point());
        Bounds {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }
}

/// The scope `drc.run`'s params ask for, with its Layers resolved.
pub(super) struct Scope {
    objects: Option<Vec<Uuid>>,
    area: Option<Bounds>,
    layers: Option<Vec<Uuid>>,
}

impl Scope {
    pub(super) fn resolve(pcb: &PcbDocument, p: &DrcRunParams) -> Result<Scope, CommandError> {
        let layers = match &p.layers {
            None => None,
            Some(layers) => Some(
                (layers.iter().enumerate())
                    .map(|(i, l)| Ok(l.resolve(pcb, &format!("/layers/{i}"))?.uuid))
                    .collect::<Result<Vec<Uuid>, CommandError>>()?,
            ),
        };
        Ok(Scope {
            objects: p.objects.clone(),
            area: p.area.as_ref().map(AreaParam::bounds),
            layers,
        })
    }

    /// Whether the run lists less than the whole board.
    pub(super) fn is_narrowed(&self) -> bool {
        self.objects.is_some() || self.area.is_some() || self.layers.is_some()
    }

    /// Whether `v` is in every part of the scope that was given.
    pub(super) fn contains(&self, pcb: &PcbDocument, v: &Violation) -> bool {
        let of_objects = (self.objects.as_ref())
            .is_none_or(|objects| v.objects.iter().any(|o| objects.contains(o)));
        let on_layers =
            (self.layers.as_ref()).is_none_or(|layers| v.layers.iter().any(|l| layers.contains(l)));
        of_objects && on_layers && self.area.is_none_or(|area| in_area(pcb, v, area))
    }
}

/// Whether one of `v`'s objects touches `area`. A Violation none of whose objects has a
/// box (such as text, or an object deleted since) goes by its marker and closest points.
fn in_area(pcb: &PcbDocument, v: &Violation, area: Bounds) -> bool {
    let boxes: Vec<Bounds> = v.objects.iter().filter_map(|o| bounds(pcb, *o)).collect();
    if boxes.is_empty() {
        return (v.at.iter())
            .chain(v.points.iter().flatten())
            .any(|p| touches(Bounds::of_point(*p), area));
    }
    boxes.into_iter().any(|b| touches(b, area))
}

fn touches(a: Bounds, b: Bounds) -> bool {
    a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
}

/// A box around a board object's copper or outline, for the objects that have one.
fn bounds(pcb: &PcbDocument, uuid: Uuid) -> Option<Bounds> {
    if let Some(track) = pcb.track(uuid) {
        let (from, to) = track.geometry.ends();
        let half = Nm(track.width.0 / 2);
        return Some(Bounds::around(from, half).union(Bounds::around(to, half)));
    }
    if let Some(via) = pcb.via(uuid) {
        return Some(Bounds::around(via.at, Nm(via.diameter.0 / 2)));
    }
    if let Some(instance) = pcb.footprint(uuid) {
        return Some(footprint_bounds(pcb, instance));
    }
    if let Some(pad) = pcb.free_pad(uuid) {
        return Some(free_pad_bounds(pad));
    }
    let vertices = if let Some(zone) = pcb.zone(uuid) {
        pcb.zone_vertices(zone)
    } else if let Some(cutout) = pcb.zone_cutout(uuid) {
        &cutout.vertices
    } else if let Some(keepout) = pcb.keepout(uuid) {
        if let reda_model::KeepoutShape::Line { vertices, width } = &keepout.shape {
            let r = Nm(width.0 / 2);
            return vertices
                .iter()
                .map(|v| Bounds::around(v.point(), r))
                .reduce(Bounds::union);
        }
        keepout.vertices()
    } else if let Some(cutout) = pcb.cutout(uuid) {
        &cutout.vertices
    } else {
        return None;
    };
    Bounds::of_points(vertices.iter().map(|v| v.point()))
}
