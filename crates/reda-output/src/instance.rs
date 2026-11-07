//! Board instances are the common geometry boundary of the manufacturing writers.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{FileOrigin, PcbDocument, ZoneFills};

use crate::gerber::arc_centre;

/// A rigid transform: rotate about design zero, then translate in output coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstanceTransform {
    pub translation: Point,
    pub rotation: Millideg,
}

impl InstanceTransform {
    pub fn point(self, point: Point) -> Point {
        let rotated = point.rotated_about(Point::default(), self.rotation);
        Point::new(
            rotated.x + self.translation.x,
            rotated.y + self.translation.y,
        )
    }
}

/// One use of a board in an output set. v0.1 runs pass exactly one, with the resolved
/// origin expressed as a translation. A future Panel can supply several rigid instances.
#[derive(Clone, Copy)]
pub struct BoardInstance<'a> {
    pub pcb: &'a PcbDocument,
    pub fills: &'a ZoneFills,
    pub transform: InstanceTransform,
}

impl<'a> BoardInstance<'a> {
    pub fn single(pcb: &'a PcbDocument, fills: &'a ZoneFills, origin: FileOrigin) -> Self {
        let origin = resolve_origin(pcb, origin);
        Self {
            pcb,
            fills,
            transform: InstanceTransform {
                translation: Point::new(Nm(-origin.x.0), Nm(-origin.y.0)),
                rotation: Millideg::ZERO,
            },
        }
    }
}

/// The design-space point used as zero by Gerber, drill and CPL together.
pub fn resolve_origin(pcb: &PcbDocument, mode: FileOrigin) -> Point {
    if mode == FileOrigin::Absolute {
        return Point::default();
    }
    if let Some(origin) = pcb.output_origin() {
        return origin.at;
    }
    let Some(outline) = pcb.outline().filter(|o| !o.vertices.is_empty()) else {
        return Point::default();
    };
    let vertices = &outline.vertices;
    let mut min = Point::new(Nm(i64::MAX), Nm(i64::MAX));
    let mut take = |p: Point| {
        min = Point::new(min.x.min(p.x), min.y.min(p.y));
    };
    for (i, v) in vertices.iter().enumerate() {
        take(v.point());
        if v.sweep != Millideg::ZERO {
            let to = vertices[(i + 1) % vertices.len()].point();
            for p in arc_extremes(v.point(), to, v.sweep) {
                take(p);
            }
        }
    }
    min
}

fn arc_extremes(from: Point, to: Point, sweep: Millideg) -> Vec<Point> {
    let c = arc_centre(from, to, sweep);
    let (dx, dy) = ((from.x.0 - c.x.0) as f64, (from.y.0 - c.y.0) as f64);
    let r = Nm(dx.hypot(dy).round() as i64);
    let start = dy.atan2(dx).to_degrees();
    let s = sweep.0 as f64 / 1000.0;
    [
        (0.0, Point::new(c.x + r, c.y)),
        (90.0, Point::new(c.x, c.y + r)),
        (180.0, Point::new(c.x - r, c.y)),
        (270.0, Point::new(c.x, c.y - r)),
    ]
    .into_iter()
    .filter(|(axis, _)| {
        if s > 0.0 {
            (axis - start).rem_euclid(360.0) <= s
        } else {
            (start - axis).rem_euclid(360.0) <= -s
        }
    })
    .map(|(_, p)| p)
    .collect()
}

/// Stable identity of the complete coordinate arrangement. This is an alignment tag,
/// not a content hash: editing copper keeps it, moving the resolved origin changes it.
pub(crate) fn coordinates_id(instances: &[BoardInstance<'_>]) -> reda_model::Uuid {
    let mut hash = 0x6c62272e07bb014262b821756295c58du128;
    let mut take = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u128::from(*byte);
            hash = hash.wrapping_mul(0x0000000001000000000000000000013b);
        }
    };
    take(b"REDA SameCoordinates 1\0");
    for instance in instances {
        take(&instance.pcb.uuid.as_u128().to_be_bytes());
        take(&instance.transform.translation.x.0.to_be_bytes());
        take(&instance.transform.translation.y.0.to_be_bytes());
        take(
            &instance
                .transform
                .rotation
                .0
                .rem_euclid(360_000)
                .to_be_bytes(),
        );
    }
    reda_model::Uuid::v8_from_bits(hash)
}
