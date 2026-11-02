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
