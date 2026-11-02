//! The connecting drawing: Wires, Junctions, Net Labels and No-Connect Markers.

use reda_model::geom::{Nm, Point};
use reda_model::{SchematicSheet, Wire};

use super::Builder;
use crate::color::SchematicColor as C;
use crate::scene::{Align, HAlign, Primitive, VAlign};
use crate::text::{TextStyle, reading_direction};
use crate::transform::rotate;

const WIRE_WIDTH: Nm = Nm(152_400);
const JUNCTION_RADIUS: Nm = Nm(508_000);
const LABEL_TEXT: Nm = Nm::from_um(1_270);
/// Net Label text sits this far above its Wire.
const LABEL_GAP: i64 = 254_000;
/// Half the size of a No-Connect Marker's cross.
const NO_CONNECT_HALF: i64 = 635_000;

pub(super) fn wires(b: &mut Builder, sheet: &SchematicSheet) {
    for wire in sheet.wires() {
        b.push(
            wire.uuid,
            C::Wire,
            Primitive::Capsule {
                a: wire.from,
                b: wire.to,
                width: WIRE_WIDTH,
            },
        );
    }
}

/// Junctions are derived, so they have no UUID of their own: each is tagged with the
/// lowest-UUID Wire through it, so picking a Junction picks a Wire of its Net.
pub(super) fn junctions(b: &mut Builder, sheet: &SchematicSheet, junctions: &[Point]) {
    for &at in junctions {
        let object = sheet
            .wires()
            .find(|w| touches(w, at))
            .map_or(sheet.uuid, |w| w.uuid);
        b.push(
            object,
            C::Junction,
            Primitive::Ring {
                center: at,
                outer: JUNCTION_RADIUS,
                inner: Nm(0),
            },
        );
    }
}

fn touches(wire: &Wire, p: Point) -> bool {
    let (a, z) = (wire.from, wire.to);
    let within = |v: Nm, a: Nm, z: Nm| a.min(z) <= v && v <= a.max(z);
    within(p.x, a.x, z.x) && within(p.y, a.y, z.y) && (a.x == z.x || a.y == z.y)
}

pub(super) fn net_labels(b: &mut Builder, sheet: &SchematicSheet) {
    for label in sheet.net_labels() {
        // Above the Wire as read, extending the way the label points.
        let reading = reading_direction(label.rotation);
        let h = if reading == label.rotation.normalized() {
            HAlign::Left
        } else {
            HAlign::Right
        };
        let (dx, dy) = rotate(0, LABEL_GAP, reading);
        b.text(
            label.uuid,
            C::NetLabel,
            &label.name,
            Point::new(label.at.x + Nm(dx), label.at.y + Nm(dy)),
            TextStyle {
                height: LABEL_TEXT,
                rotation: reading,
                align: Align::new(h, VAlign::Baseline),
                view_label: false,
            },
        );
    }
}

pub(super) fn no_connects(b: &mut Builder, sheet: &SchematicSheet) {
    let h = NO_CONNECT_HALF;
    for marker in sheet.no_connects() {
        for (sx, sy) in [(1, 1), (1, -1)] {
            let d = Point::new(Nm(h * sx), Nm(h * sy));
            b.push(
                marker.uuid,
                C::NoConnect,
                Primitive::Capsule {
                    a: marker.at - d,
                    b: marker.at + d,
                    width: WIRE_WIDTH,
                },
            );
        }
    }
}
