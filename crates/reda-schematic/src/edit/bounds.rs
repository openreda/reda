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

/// The box `object` covers on `body`: a Unit's Pins and body drawing, a Wire's vertices, a graphic's
/// ends, or a label's, marker's, Port's, Power Port's or text's point.
pub(crate) fn bounds(body: &SchematicSheet, object: &Object) -> Bounds {
    let points: Vec<Point> = match object {
        Object::Unit(u) => unit_points(body, u),
        Object::Wire(w) => w.vertices.clone(),
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
    local.extend(drawing_points(symbol, unit.unit, true));
    local.into_iter().map(|p| on_sheet(unit, p)).collect()
}

/// The box a placed Unit's body covers on the sheet: its drawing other than text, and each
/// Pin from its connection end to its root at the body. `None` for a Unit with neither.
pub(crate) fn unit_body(body: &SchematicSheet, unit: &UnitInstance) -> Option<Bounds> {
    let entry = body.part(unit.part).expect("a Unit's part entry exists");
    let symbol = &entry.symbol;
    let mut local: Vec<Point> = drawing_points(symbol, unit.unit, false).collect();
    for pin in symbol.unit_pins(unit.unit) {
        let (dx, dy) = rotate(pin.length.0, 0, pin.rotation);
        local.extend([pin.at, Point::new(pin.at.x + Nm(dx), pin.at.y + Nm(dy))]);
    }
    Bounds::of_points(local.into_iter().map(|p| on_sheet(unit, p)))
}

/// Points spanning the Unit's own Symbol drawing, in Symbol coordinates; text anchors only
/// `with_text`.
fn drawing_points(
    symbol: &reda_model::Symbol,
    unit: reda_model::Uuid,
    with_text: bool,
) -> impl Iterator<Item = Point> + '_ {
    symbol
        .graphics
        .iter()
        .filter(move |g| g.unit == unit)
        .flat_map(move |graphic| -> Vec<Point> {
            match &graphic.shape {
                Shape::Line { start, end } | Shape::Rect { start, end } => vec![*start, *end],
                Shape::Arc { center, start, .. } => {
                    let r = Nm((start.x - center.x).0.abs() + (start.y - center.y).0.abs());
                    around(*center, r).to_vec()
                }
                Shape::Circle { center, radius } => around(*center, *radius).to_vec(),
                Shape::Polygon { points } => points.clone(),
                Shape::Text { at, .. } if with_text => vec![*at],
                Shape::Text { .. } => vec![],
            }
        })
}

/// The box everything on `body` covers that a new object could overlap; `None` for an
/// empty sheet.
pub(crate) fn content(body: &SchematicSheet) -> Option<Bounds> {
    body.objects()
        .iter()
        .filter(|o| {
            matches!(
                o,
                Object::Unit(_)
                    | Object::Wire(_)
                    | Object::NetLabel(_)
                    | Object::NoConnect(_)
                    | Object::Port(_)
                    | Object::PowerPort(_)
                    | Object::SheetText(_)
                    | Object::SheetGraphic(_)
            )
        })
        .map(|o| bounds(body, o))
        .reduce(Bounds::union)
}

/// The box Unit `unit` of `symbol` covers about its own origin, unturned: its origin, Pins
/// and drawing.
pub(crate) fn symbol_extent(symbol: &reda_model::Symbol, unit: reda_model::Uuid) -> Bounds {
    let mut local = vec![Point::new(Nm(0), Nm(0))];
    local.extend(symbol.unit_pins(unit).map(|p| p.at));
    local.extend(drawing_points(symbol, unit, true));
    Bounds::of_points(local).expect("the origin is a point")
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
