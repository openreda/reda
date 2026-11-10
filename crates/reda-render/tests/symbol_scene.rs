//! The Symbol editor's Scene: one Unit of a Library Symbol, each graphic and Pin its own
//! object.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{Pin, PinType, Shape, Symbol, SymbolGraphic, Unit, Uuid};
use reda_render::{Primitive, SceneSource, symbol_scene};

fn uuid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn pin(n: u128, unit: Uuid, number: &str, number_visible: bool) -> Pin {
    Pin {
        uuid: uuid(n),
        unit,
        name: "IN".into(),
        number: number.into(),
        electrical_type: PinType::Input,
        at: Point::new(Nm(-2_540_000), Nm(0)),
        rotation: Millideg(0),
        length: Nm(2_540_000),
        name_visible: true,
        number_visible,
        description: String::new(),
        inside_edge: None,
        outside_edge: None,
    }
}

fn op_amp() -> Symbol {
    let (a, b) = (uuid(0xa), uuid(0xb));
    Symbol {
        uuid: uuid(1),
        name: "OpAmp".into(),
        description: String::new(),
        designator_prefix: "U".into(),
        units: vec![
            Unit {
                uuid: a,
                name: "A".into(),
            },
            Unit {
                uuid: b,
                name: "B".into(),
            },
        ],
        pins: vec![pin(0x10, a, "3", true), pin(0x11, b, "5", false)],
        graphics: vec![SymbolGraphic {
            uuid: uuid(0x20),
            unit: a,
            shape: Shape::Rect {
                start: Point::new(Nm(0), Nm(-2_540_000)),
                end: Point::new(Nm(5_080_000), Nm(2_540_000)),
            },
            width: Nm(254_000),
            fill: false,
        }],
        provenance: None,
    }
}

#[test]
fn a_unit_draws_its_own_graphics_and_pins_each_as_its_own_object() {
    let symbol = op_amp();
    let scene = symbol_scene(&symbol, uuid(0xa)).unwrap();
    assert_eq!(scene.source, SceneSource::Symbol(uuid(1), uuid(0xa)));
    let objects: Vec<Uuid> = scene.items.iter().map(|i| i.object).collect();
    assert!(objects.contains(&uuid(0x20)), "the body rectangle");
    assert!(objects.contains(&uuid(0x10)), "Pin 3");
    assert!(!objects.contains(&uuid(0x11)), "Unit B's Pin is not drawn");
    let texts: Vec<&str> = scene
        .items
        .iter()
        .filter_map(|i| match &i.primitive {
            Primitive::Glyphs(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["IN", "3"], "the Pin's name and its own number");
}

#[test]
fn a_hidden_pin_number_is_not_drawn_and_an_unknown_unit_has_no_scene() {
    let symbol = op_amp();
    let scene = symbol_scene(&symbol, uuid(0xb)).unwrap();
    let glyphs = scene
        .items
        .iter()
        .filter(|i| matches!(i.primitive, Primitive::Glyphs(_)))
        .count();
    assert_eq!(glyphs, 1, "only the name");
    assert!(symbol_scene(&symbol, uuid(0xc)).is_none());
}

/// The primitives drawn for the object `object`, other than text.
fn shapes(symbol: &Symbol, unit: Uuid, object: Uuid) -> Vec<Primitive> {
    let scene = symbol_scene(symbol, unit).unwrap();
    scene
        .items
        .iter()
        .filter(|i| i.object == object && !matches!(i.primitive, Primitive::Glyphs(_)))
        .map(|i| i.primitive.clone())
        .collect()
}

#[test]
fn an_inversion_dot_sits_outside_the_body_edge_and_the_pin_line_stops_at_it() {
    use reda_model::OutsideEdge;
    let mut symbol = op_amp();
    // Pin 3 runs from x = -100 mil into the body edge at x = 0.
    symbol.pins[0].outside_edge = Some(OutsideEdge::Dot);
    let drawn = shapes(&symbol, uuid(0xa), uuid(0x10));
    let rings: Vec<_> = drawn
        .iter()
        .filter_map(|p| match p {
            Primitive::Ring { center, outer, .. } => Some((*center, *outer)),
            _ => None,
        })
        .collect();
    assert_eq!(rings.len(), 1, "{drawn:?}");
    let (center, outer) = rings[0];
    assert!(center.x.0 < 0 && center.y.0 == 0, "outside, on the line");
    // The dot touches the body edge.
    assert!(
        (center.x.0 + outer.0).abs() < 160_000,
        "{center:?} {outer:?}"
    );
    let line_end = drawn
        .iter()
        .find_map(|p| match p {
            Primitive::Capsule { a, b, .. } if a.y.0 == 0 && b.y.0 == 0 => Some(a.x.max(b.x)),
            _ => None,
        })
        .unwrap();
    assert!(line_end.0 < center.x.0, "the line stops before the dot");
}

#[test]
fn a_clock_wedge_is_drawn_just_inside_the_body_edge() {
    use reda_model::InsideEdge;
    let mut symbol = op_amp();
    let plain = shapes(&symbol, uuid(0xa), uuid(0x10)).len();
    symbol.pins[0].inside_edge = Some(InsideEdge::Clock);
    let drawn = shapes(&symbol, uuid(0xa), uuid(0x10));
    let inside: Vec<_> = drawn
        .iter()
        .filter(|p| match p {
            Primitive::Capsule { a, b, .. } => a.x.0 >= 0 && b.x.0 >= 0 && (a.x.0 > 0 || b.x.0 > 0),
            _ => false,
        })
        .collect();
    assert_eq!(drawn.len(), plain + 2, "two strokes: {drawn:?}");
    assert_eq!(inside.len(), 2, "both strokes are inside the body");
}

#[test]
fn active_low_flags_are_drawn_outside_the_body_edge() {
    use reda_model::OutsideEdge;
    for flag in [OutsideEdge::ActiveLowInput, OutsideEdge::ActiveLowOutput] {
        let mut symbol = op_amp();
        let plain = shapes(&symbol, uuid(0xa), uuid(0x10)).len();
        symbol.pins[0].outside_edge = Some(flag);
        let drawn = shapes(&symbol, uuid(0xa), uuid(0x10));
        assert!(drawn.len() > plain, "{flag:?} adds strokes");
        let off_line = drawn.iter().any(|p| match p {
            Primitive::Capsule { a, b, .. } => {
                (a.y.0 != 0 || b.y.0 != 0) && a.x.0 <= 0 && b.x.0 <= 0
            }
            _ => false,
        });
        assert!(off_line, "{flag:?} leaves the line, outside the body");
    }
}
