//! The extent of selected objects on their sheet, for aligning, distributing and choosing a
//! centre to turn about.

pub(crate) use reda_model::geom::Bounds;
use reda_model::geom::{Nm, Point};
use reda_model::{Object, SchematicSheet, Shape, UnitInstance};

use crate::connectivity::rotate;

/// Where `object` sits: a Unit's origin, a label's, marker's, Port's, Power Port's or text's
/// point, or the middle of a Wire or graphic. A lone object turns and flips about it.
pub(crate) fn anchor(body: &SchematicSheet, object: &Object) -> Point {
    match object {
        Object::Unit(u) => u.at,
        Object::NetLabel(l) => l.at,
        Object::NoConnect(m) => m.at,
        Object::Port(p) => p.at,
        Object::PowerPort(p) => p.at,
        Object::SheetText(t) => t.at,
        _ => bounds(body, object).center(),
    }
}

/// The box `object` covers on `body`: a Unit's Pins and body drawing, a Wire's or graphic's
/// ends, or a label's, marker's, Port's, Power Port's or text's point.
pub(crate) fn bounds(body: &SchematicSheet, object: &Object) -> Bounds {
    let points: Vec<Point> = match object {
        Object::Unit(u) => unit_points(body, u),
        Object::Wire(w) => vec![w.from, w.to],
        Object::NetLabel(l) => vec![l.at],
        Object::NoConnect(m) => vec![m.at],
        Object::Port(p) => vec![p.at],
        Object::PowerPort(p) => vec![p.at],
        Object::SheetText(t) => vec![t.at],
        Object::SheetGraphic(g) => {
            let (a, b) = g.shape.ends();
            vec![a, b]
        }
        _ => vec![],
    };
    Bounds::of_points(points).expect("a selectable object has at least one point")
}

/// The union of the boxes of `objects`.
pub(crate) fn union(body: &SchematicSheet, objects: &[Object]) -> Bounds {
    objects
        .iter()
        .map(|o| bounds(body, o))
        .reduce(Bounds::union)
        .expect("a selection is never empty")
}

/// The Unit's origin, Pins and body drawing on the sheet.
fn unit_points(body: &SchematicSheet, unit: &UnitInstance) -> Vec<Point> {
    let entry = body.part(unit.part).expect("a Unit's part entry exists");
    let symbol = &entry.symbol;
    let mut local = vec![Point::new(Nm(0), Nm(0))];
    local.extend(symbol.unit_pins(unit.unit).map(|p| p.at));
    for graphic in symbol.graphics.iter().filter(|g| g.unit == unit.unit) {
        match &graphic.shape {
            Shape::Line { start, end } | Shape::Rect { start, end } => {
                local.extend([*start, *end]);
            }
            Shape::Arc { center, start, .. } => {
                let r = Nm((start.x - center.x).0.abs() + (start.y - center.y).0.abs());
                local.extend(around(*center, r));
            }
            Shape::Circle { center, radius } => local.extend(around(*center, *radius)),
            Shape::Polygon { points } => local.extend(points.iter().copied()),
            Shape::Text { at, .. } => local.push(*at),
        }
    }
    local.into_iter().map(|p| on_sheet(unit, p)).collect()
}

/// Two corners of the box of radius `r` about `center`.
fn around(center: Point, r: Nm) -> [Point; 2] {
    [
        Point::new(center.x - r, center.y - r),
        Point::new(center.x + r, center.y + r),
    ]
}

/// A point of the Unit's Symbol, placed on the sheet the way its Pins are.
fn on_sheet(unit: &UnitInstance, p: Point) -> Point {
    let x = if unit.mirror { -p.x.0 } else { p.x.0 };
    let (x, y) = rotate(x, p.y.0, unit.rotation);
    Point::new(unit.at.x + Nm(x), unit.at.y + Nm(y))
}
