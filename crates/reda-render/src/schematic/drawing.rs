//! The connecting drawing (Wires, Junctions, Net Labels and No-Connect Markers) and the
//! free drawing (text, lines and rectangles).

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::text::display;
use reda_model::{JustifyAxis, SchematicSheet, SheetShape};

use super::Builder;
use crate::color::SchematicColor as C;
use crate::scene::{Align, HAlign, Primitive, VAlign};
use crate::text::{LINE_PITCH, TextStyle, reading_direction};
use crate::transform::rotate;

const WIRE_WIDTH: Nm = Nm(152_400);
/// Free lines and rectangle outlines have no width of their own.
const GRAPHIC_WIDTH: Nm = Nm(254_000);
const JUNCTION_RADIUS: Nm = Nm(508_000);
const LABEL_TEXT: Nm = Nm::from_um(1_270);
/// Net Label text sits this far above its Wire.
const LABEL_GAP: i64 = 254_000;
/// Half the size of a No-Connect Marker's cross.
const NO_CONNECT_HALF: i64 = 635_000;

pub(super) fn wires(b: &mut Builder, sheet: &SchematicSheet) {
    for wire in sheet.wires() {
        for (a, z) in wire.segments() {
            b.push(
                wire.uuid,
                C::Wire,
                Primitive::Capsule {
                    a,
                    b: z,
                    width: WIRE_WIDTH,
                },
            );
        }
    }
}

/// Junctions are derived, so they have no UUID of their own: each is tagged with the
/// lowest-UUID Wire through it, so picking a Junction picks a Wire of its Net.
pub(super) fn junctions(b: &mut Builder, sheet: &SchematicSheet, junctions: &[Point]) {
    for &at in junctions {
        let object = sheet
            .wires()
            .find(|w| w.contains(at))
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

pub(super) fn graphics(b: &mut Builder, sheet: &SchematicSheet) {
    for graphic in sheet.graphics() {
        let primitive = match graphic.shape {
            SheetShape::Line { from, to } => Primitive::Capsule {
                a: from,
                b: to,
                width: GRAPHIC_WIDTH,
            },
            SheetShape::Rect { from, to, filled } => Primitive::Rect {
                center: Point::new(Nm((from.x.0 + to.x.0) / 2), Nm((from.y.0 + to.y.0) / 2)),
                size: (Nm((to.x - from.x).0.abs()), Nm((to.y - from.y).0.abs())),
                rotation: Millideg(0),
                corner_radius: Nm(0),
                stroke: (!filled).then_some(GRAPHIC_WIDTH),
            },
        };
        b.push(graphic.uuid, C::Graphic, primitive);
    }
}

pub(super) fn texts(b: &mut Builder, sheet: &SchematicSheet) {
    for text in sheet.texts() {
        // Laid out in the direction it reads, so text turned upside down is drawn the
        // right way up with its justification point mirrored to match.
        let reading = reading_direction(text.rotation);
        let flipped = reading != text.rotation.normalized();
        let turn = |axis: JustifyAxis| match (flipped, axis) {
            (true, JustifyAxis::Start) => JustifyAxis::End,
            (true, JustifyAxis::End) => JustifyAxis::Start,
            (_, axis) => axis,
        };
        let h = match turn(text.justify.horizontal()) {
            JustifyAxis::Start => HAlign::Left,
            JustifyAxis::Middle => HAlign::Center,
            JustifyAxis::End => HAlign::Right,
        };
        // Lines run down from the first; the block's bottom is the last line's baseline.
        let lines = text.text.split('\n').count() as i64;
        let below = (lines - 1) * text.height.0 * LINE_PITCH.0 / LINE_PITCH.1;
        let (v, lift) = match turn(text.justify.vertical()) {
            JustifyAxis::Start => (VAlign::Baseline, below),
            JustifyAxis::Middle => (VAlign::Middle, below / 2),
            JustifyAxis::End => (VAlign::Top, 0),
        };
        let (dx, dy) = rotate(0, lift, reading);
        b.text(
            text.uuid,
            C::Text,
            &display(&text.text),
            Point::new(text.at.x + Nm(dx), text.at.y + Nm(dy)),
            TextStyle {
                height: text.height,
                rotation: reading,
                align: Align::new(h, v),
                view_label: false,
            },
        );
    }
}
