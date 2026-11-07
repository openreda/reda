//! Board instances are the common geometry boundary of the manufacturing writers.

use reda_model::geom::{Millideg, Nm, Point, contour_bounds};
use reda_model::{FileOrigin, PcbDocument, ZoneFills};

/// A rigid transform: rotate about design zero, then translate in output coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstanceTransform {
    pub translation: Point,
    pub rotation: Millideg,
}

impl InstanceTransform {
    /// `point` in output coordinates.
    pub fn point(self, point: Point) -> Point {
        let rotated = point.rotated_about(Point::default(), self.rotation);
        Point::new(
            rotated.x + self.translation.x,
            rotated.y + self.translation.y,
        )
    }

    /// An object's design-space rotation `angle` in output coordinates.
    pub fn angle(self, angle: Millideg) -> Millideg {
        angle + self.rotation
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

/// The design-space point used as zero by Gerber, drill and CPL together: zero for
/// `absolute`; for `relative` the Output Origin, or else the lower-left of the Board
/// Outline's bounding box, arcs included.
pub fn resolve_origin(pcb: &PcbDocument, mode: FileOrigin) -> Point {
    if mode == FileOrigin::Absolute {
        return Point::default();
    }
    if let Some(origin) = pcb.output_origin() {
        return origin.at;
    }
    pcb.outline()
        .and_then(|outline| contour_bounds(&outline.vertices))
        .map_or_else(Point::default, |bounds| bounds.min)
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
