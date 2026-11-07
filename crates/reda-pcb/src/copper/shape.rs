//! Copper shapes on the board: the [grown shapes](reda_model::geom::grown) of Pads,
//! Tracks and Vias, placed by their Footprint instance and boxed for the R-tree index.

use reda_model::geom::Millideg;
use reda_model::{FootprintInstance, FreePad, Pad, Side, TrackGeometry};
use rstar::AABB;

pub(crate) use reda_model::geom::grown::{Core, P, Prim, Shape, arc_points};

/// The board's ways to make and box a [`Shape`].
pub(crate) trait BoardShape {
    /// A Track's copper: its centreline grown by half the width; an arc as a chain of
    /// chords within 1 µm of it.
    fn track(geometry: &TrackGeometry, width: reda_model::geom::Nm) -> Shape;
    /// A Pad's copper as placed on the board by `instance`.
    fn pad(instance: &FootprintInstance, pad: &Pad) -> Shape;
    /// A Free Pad's copper: it is drawn in board coordinates.
    fn free_pad(pad: &FreePad) -> Shape;
    /// The box the R-tree indexes it by.
    fn envelope(&self) -> AABB<P>;
}

impl BoardShape for Shape {
    fn track(geometry: &TrackGeometry, width: reda_model::geom::Nm) -> Shape {
        let points = match *geometry {
            TrackGeometry::Line { from, to } => vec![from, to],
            TrackGeometry::Arc { from, to, sweep } => arc_points(from, to, sweep),
        };
        Shape::chain(&points, width.0 / 2)
    }

    fn pad(instance: &FootprintInstance, pad: &Pad) -> Shape {
        let local = reda_model::footprint_copper::pad_shape(pad);
        Shape {
            prims: local
                .prims
                .into_iter()
                .map(|prim| Prim {
                    core: match prim.core {
                        Core::Seg(a, b) => {
                            Core::Seg(board_point(instance, a), board_point(instance, b))
                        }
                        Core::Poly(points) => Core::Poly(
                            points
                                .into_iter()
                                .map(|p| board_point(instance, p))
                                .collect(),
                        ),
                    },
                    radius: prim.radius,
                })
                .collect(),
        }
    }

    fn free_pad(pad: &FreePad) -> Shape {
        Shape::outline(&pad.outline(), |p| {
            let p = rotate(p, pad.rotation);
            [p[0] + pad.at.x.0, p[1] + pad.at.y.0]
        })
    }

    fn envelope(&self) -> AABB<P> {
        let (lo, hi) = self.bounds();
        AABB::from_corners(lo, hi)
    }
}

/// A point of a Footprint (relative to its origin) on the board: flipped for the bottom
/// side, then turned, then moved to the instance's position.
pub(crate) fn board_point(instance: &FootprintInstance, p: P) -> P {
    let p = match instance.side {
        Side::Top => p,
        Side::Bottom => [-p[0], p[1]],
    };
    let p = rotate(p, instance.rotation);
    [p[0] + instance.at.x.0, p[1] + instance.at.y.0]
}

/// `p` turned counter-clockwise about the origin; exact for quarter turns.
pub(crate) fn rotate(p: P, angle: Millideg) -> P {
    let (x, y) = reda_model::geom::rotate(p[0], p[1], angle);
    [x, y]
}
