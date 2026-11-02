//! Placed Units: the Symbol body, the Pins with their names and Pad numbers, and the
//! visible Component fields.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{PartEntry, Pin, SchematicSheet, Shape, SymbolGraphic, UnitInstance, Uuid};

use super::Builder;
use crate::color::SchematicColor as C;
use crate::polygon::triangulate;
use crate::scene::{Align, HAlign, Primitive, VAlign};
use crate::text::{TextStyle, reading_direction};
use crate::transform::{Placement, rotate};

const PIN_WIDTH: Nm = Nm(152_400);
const PIN_TEXT: Nm = Nm::from_um(1_270);
/// Pin names sit this far inside the body end of the Pin.
const NAME_GAP: i64 = 508_000;
/// Pin numbers sit this far off the Pin line.
const NUMBER_GAP: i64 = 254_000;
const FIELD_TEXT: Nm = Nm::from_um(1_270);
/// Fields are stacked this far right of the body, this far apart.
const FIELD_GAP: i64 = 1_270_000;
const FIELD_PITCH: i64 = 2_032_000;
/// Arcs are drawn as capsules at most this many millidegrees long.
const ARC_STEP: i64 = 5_000;

pub(super) fn build(b: &mut Builder, sheet: &SchematicSheet) {
    for unit in sheet.units() {
        let Some(entry) = sheet.part(unit.part) else {
            continue;
        };
        draw_unit(b, unit, entry);
    }
}

fn draw_unit(b: &mut Builder, unit: &UnitInstance, entry: &PartEntry) {
    let place = Placement {
        at: unit.at,
        rotation: unit.rotation,
        mirror: unit.mirror,
    };
    let first = b.scene.items.len();
    for graphic in entry.symbol.graphics.iter().filter(|g| g.unit == unit.unit) {
        draw_graphic(b, unit.uuid, &place, graphic);
    }
    for pin in entry.symbol.pins.iter().filter(|p| p.unit == unit.unit) {
        draw_pin(b, unit.uuid, &place, pin, entry);
    }
    let body = b.scene.items[first..]
        .iter()
        .filter(|i| !matches!(i.primitive, Primitive::Glyphs(_)))
        .filter_map(|i| i.primitive.bounds())
        .reduce(|(lo, hi), (l, h)| {
            (
                Point::new(lo.x.min(l.x), lo.y.min(l.y)),
                Point::new(hi.x.max(h.x), hi.y.max(h.y)),
            )
        })
        .unwrap_or((unit.at, unit.at));
    draw_fields(b, unit, body);
}

fn draw_graphic(b: &mut Builder, object: Uuid, place: &Placement, g: &SymbolGraphic) {
    let w = g.width;
    match &g.shape {
        Shape::Line { start, end } => b.push(object, C::Body, capsule(place, *start, *end, w)),
        Shape::Rect { start, end } => {
            let center = place.apply(Point::new(
                Nm((start.x.0 + end.x.0) / 2),
                Nm((start.y.0 + end.y.0) / 2),
            ));
            let size = (Nm((end.x - start.x).0.abs()), Nm((end.y - start.y).0.abs()));
            let shape = |stroke| Primitive::Rect {
                center,
                size,
                rotation: place.rotation.normalized(),
                corner_radius: Nm(0),
                stroke,
            };
            if g.fill {
                b.push(object, C::BodyFill, shape(None));
            }
            b.push(object, C::Body, shape(Some(w)));
        }
        Shape::Arc {
            center,
            start,
            sweep,
        } => {
            let steps = (sweep.0.abs() + ARC_STEP - 1) / ARC_STEP;
            let r = *start - *center;
            let point = |i: i64| {
                let (x, y) = rotate(r.x.0, r.y.0, Millideg(sweep.0 * i / steps.max(1)));
                Point::new(center.x + Nm(x), center.y + Nm(y))
            };
            for i in 0..steps {
                b.push(object, C::Body, capsule(place, point(i), point(i + 1), w));
            }
        }
        Shape::Circle { center, radius } => {
            let c = place.apply(*center);
            if g.fill {
                b.push(
                    object,
                    C::BodyFill,
                    Primitive::Ring {
                        center: c,
                        outer: *radius,
                        inner: Nm(0),
                    },
                );
            }
            b.push(
                object,
                C::Body,
                Primitive::Ring {
                    center: c,
                    outer: *radius + Nm(w.0 / 2),
                    inner: (*radius - Nm(w.0 / 2)).max(Nm(0)),
                },
            );
        }
        Shape::Polygon { points } => {
            let placed: Vec<Point> = points.iter().map(|p| place.apply(*p)).collect();
            if g.fill {
                b.push(
                    object,
                    C::BodyFill,
                    Primitive::Triangles(triangulate(&placed)),
                );
            }
            for (i, a) in placed.iter().enumerate() {
                let z = placed[(i + 1) % placed.len()];
                b.push(
                    object,
                    C::Body,
                    Primitive::Capsule {
                        a: *a,
                        b: z,
                        width: w,
                    },
                );
            }
        }
        Shape::Text {
            at,
            text,
            size,
            rotation,
        } => b.text(
            object,
            C::Body,
            text,
            place.apply(*at),
            TextStyle {
                height: *size,
                rotation: place.angle(*rotation),
                align: Align::default(),
                view_label: false,
            },
        ),
    }
}

fn capsule(place: &Placement, a: Point, z: Point, width: Nm) -> Primitive {
    Primitive::Capsule {
        a: place.apply(a),
        b: place.apply(z),
        width,
    }
}

/// A Pin: its line from the connection end into the body, its name just inside the body
/// end, and the Pad number(s) it maps to beside the line, as view-only labels.
fn draw_pin(b: &mut Builder, object: Uuid, place: &Placement, pin: &Pin, entry: &PartEntry) {
    let end = place.apply(pin.at);
    let dir = place.angle(pin.rotation);
    let along = |d: i64| {
        let (x, y) = rotate(d, 0, dir);
        Point::new(Nm(x), Nm(y))
    };
    let body_end = end + along(pin.length.0);
    b.push(
        object,
        C::Pin,
        Primitive::Capsule {
            a: end,
            b: body_end,
            width: PIN_WIDTH,
        },
    );
    if pin.name_visible {
        b.text(
            object,
            C::PinName,
            &pin.name,
            body_end + along(NAME_GAP),
            TextStyle {
                height: PIN_TEXT,
                rotation: dir,
                align: Align::new(HAlign::Left, VAlign::Middle),
                view_label: true,
            },
        );
    }
    if pin.number_visible {
        let pads = entry.part.pin_map.pads_of(&pin.number).join(",");
        // Above the line as read, so every Pin's number is on the same side of it.
        let reading = reading_direction(dir);
        let (nx, ny) = rotate(0, NUMBER_GAP, reading);
        let mid = Point::new(
            Nm((end.x.0 + body_end.x.0) / 2 + nx),
            Nm((end.y.0 + body_end.y.0) / 2 + ny),
        );
        b.text(
            object,
            C::PinNumber,
            &pads,
            mid,
            TextStyle {
                height: PIN_TEXT,
                rotation: reading,
                align: Align::new(HAlign::Center, VAlign::Baseline),
                view_label: true,
            },
        );
    }
}

/// The visible fields, stacked right of the body and centred on it: the Designator,
/// the value, then other fields by name.
fn draw_fields(b: &mut Builder, unit: &UnitInstance, (lo, hi): (Point, Point)) {
    let mut lines: Vec<(C, &str)> = Vec::new();
    let shown = |name: &str| unit.visible_fields.contains(name);
    if shown("designator") {
        lines.push((C::Designator, &unit.designator));
    }
    if shown("value") {
        lines.push((C::Value, unit.fields.get("value")));
    }
    for name in &unit.visible_fields {
        if name != "designator" && name != "value" {
            lines.push((C::Field, unit.fields.get(name)));
        }
    }
    lines.retain(|(_, text)| !text.is_empty());
    let n = lines.len() as i64;
    let x = hi.x.0 + FIELD_GAP;
    let top = (lo.y.0 + hi.y.0) / 2 + (n - 1) * FIELD_PITCH / 2;
    for (i, (color, text)) in lines.into_iter().enumerate() {
        b.text(
            unit.uuid,
            color,
            text,
            Point::new(Nm(x), Nm(top - i as i64 * FIELD_PITCH)),
            TextStyle {
                height: FIELD_TEXT,
                rotation: Millideg(0),
                align: Align::new(HAlign::Left, VAlign::Middle),
                view_label: false,
            },
        );
    }
}
