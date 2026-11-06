//! The Symbol editor's edits: moving and deleting any mix of Pins and graphics in one
//! Command, so each is one undo step; a Pin's description and decorations; and setting
//! the origin.

use reda_engine::command::ErrorCode;
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{InsideEdge, LibraryElement, OutsideEdge};
use reda_testkit::{TempDir, TestEngine, test_engine_in};
use serde_json::json;

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

fn new_symbol(home: &TempDir, work: &TempDir) -> (TestEngine, String) {
    let lib = work.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let info = reda_engine::model::LibraryInfo {
        uuid: LIBRARY.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    let mut e = test_engine_in(home.path());
    e.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [lib]}),
    );
    let out = e.run_ok(
        "library.element.new",
        json!({"library": LIBRARY, "kind": "symbol", "name": "Chip"}),
    );
    (e, out["element"].as_str().unwrap().to_owned())
}

fn symbol(e: &TestEngine, uuid: &str) -> reda_engine::model::Symbol {
    match e.element(uuid.parse().unwrap()) {
        Some(LibraryElement::Symbol(s)) => s.clone(),
        _ => panic!("open"),
    }
}

/// Adds a Pin and a line to the first Unit; returns their UUIDs.
fn pin_and_line(e: &mut TestEngine, sym: &str) -> (String, String) {
    let unit = symbol(e, sym).units[0].uuid;
    let pin = e.run_ok(
        "library.symbol.pin.add",
        json!({"symbol": sym, "unit": unit, "number": "1", "name": "A",
               "at": {"x": "0mil", "y": "0mil"}}),
    );
    let line = e.run_ok(
        "library.symbol.graphic.add",
        json!({"symbol": sym, "unit": unit,
               "shape": {"kind": "line", "start": {"x": 0, "y": 0}, "end": {"x": "100mil", "y": 0}}}),
    );
    (
        pin["result"]["pin"].as_str().unwrap().to_owned(),
        line["result"]["graphic"].as_str().unwrap().to_owned(),
    )
}

#[test]
fn moving_pins_and_graphics_is_one_undo_step() {
    let (home, work) = (TempDir::new("syme-home"), TempDir::new("syme-work"));
    let (mut e, sym) = new_symbol(&home, &work);
    let (pin, line) = pin_and_line(&mut e, &sym);

    e.run_ok(
        "library.symbol.move",
        json!({"symbol": sym, "objects": [pin, line], "by": {"x": "50mil", "y": "100mil"}}),
    );
    let s = symbol(&e, &sym);
    assert_eq!(
        s.pins[0].at,
        Point::new(Nm::from_mil(50), Nm::from_mil(100))
    );
    assert_eq!(
        s.graphics[0].shape,
        reda_engine::model::Shape::Line {
            start: Point::new(Nm::from_mil(50), Nm::from_mil(100)),
            end: Point::new(Nm::from_mil(150), Nm::from_mil(100)),
        }
    );

    e.run_ok("edit.undo", json!({"document": sym}));
    assert_eq!(symbol(&e, &sym).pins[0].at, Point::new(Nm(0), Nm(0)));
}

#[test]
fn deleting_pins_and_graphics_is_one_undo_step_and_strangers_are_refused() {
    let (home, work) = (TempDir::new("symd-home"), TempDir::new("symd-work"));
    let (mut e, sym) = new_symbol(&home, &work);
    let (pin, line) = pin_and_line(&mut e, &sym);

    let err = e.run_err(
        "library.symbol.delete",
        json!({"symbol": sym, "objects": [pin, "00000000-0000-4000-8000-0000000000ff"]}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(symbol(&e, &sym).pins.len(), 1, "nothing half-done");

    e.run_ok(
        "library.symbol.delete",
        json!({"symbol": sym, "objects": [pin, line]}),
    );
    let s = symbol(&e, &sym);
    assert!(s.pins.is_empty() && s.graphics.is_empty());
    e.run_ok("edit.undo", json!({"document": sym}));
    let s = symbol(&e, &sym);
    assert_eq!((s.pins.len(), s.graphics.len()), (1, 1));
}

#[test]
fn a_pin_has_a_description_and_edge_decorations() {
    let (home, work) = (TempDir::new("sym-pin-home"), TempDir::new("sym-pin-work"));
    let (mut e, sym) = new_symbol(&home, &work);
    let pin = e.run_ok(
        "library.symbol.pin.add",
        json!({"symbol": sym, "number": "3", "name": "CLK", "at": {"x": 0, "y": 0},
               "description": "Shift clock", "inside_edge": "clock",
               "outside_edge": "active_low_input"}),
    )["result"]["pin"]
        .as_str()
        .unwrap()
        .to_owned();
    let get = |e: &TestEngine| symbol(e, &sym).pins[0].clone();
    let p = get(&e);
    assert_eq!(p.description, "Shift clock");
    assert_eq!(p.inside_edge, Some(InsideEdge::Clock));
    assert_eq!(p.outside_edge, Some(OutsideEdge::ActiveLowInput));

    e.run_ok(
        "library.symbol.pin.edit",
        json!({"symbol": sym, "pin": pin, "outside_edge": "dot"}),
    );
    let p = get(&e);
    assert_eq!(p.outside_edge, Some(OutsideEdge::Dot));
    assert_eq!(p.inside_edge, Some(InsideEdge::Clock), "omitted, kept");
    e.run_ok(
        "library.symbol.pin.edit",
        json!({"symbol": sym, "pin": pin, "inside_edge": null, "description": ""}),
    );
    let p = get(&e);
    assert_eq!(p.inside_edge, None, "null removes it");
    assert_eq!(p.outside_edge, Some(OutsideEdge::Dot));
    assert_eq!(p.description, "");

    let refused = e.run_err(
        "library.symbol.pin.edit",
        json!({"symbol": sym, "pin": pin, "inside_edge": "dot"}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams, "a dot sits outside");
}

#[test]
fn the_origin_moves_to_pin_1_the_centre_or_a_picked_point() {
    let (home, work) = (TempDir::new("sym-org-home"), TempDir::new("sym-org-work"));
    let (mut e, sym) = new_symbol(&home, &work);
    let unit = symbol(&e, &sym).units[0].uuid;
    for (number, x, y) in [("1", "-300mil", "100mil"), ("2", "300mil", "-200mil")] {
        e.run_ok(
            "library.symbol.pin.add",
            json!({"symbol": sym, "unit": unit, "number": number, "name": "P",
                   "at": {"x": x, "y": y}, "length": 0}),
        );
    }
    let rect = e.run_ok(
        "library.symbol.graphic.add",
        json!({"symbol": sym, "shape": {"kind": "rect", "start": {"x": "-200mil", "y": "-200mil"},
                                        "end": {"x": "200mil", "y": "100mil"}}}),
    )["result"]["graphic"]
        .as_str()
        .unwrap()
        .to_owned();
    let pin_at = |e: &TestEngine, number: &str| {
        symbol(e, &sym)
            .pins
            .iter()
            .find(|p| p.number == number)
            .unwrap()
            .at
    };
    let mil = |x: i64, y: i64| Point::new(Nm::from_mil(x), Nm::from_mil(y));

    e.run_ok(
        "library.symbol.origin.set",
        json!({"symbol": sym, "to": {"kind": "pin1"}}),
    );
    assert_eq!(pin_at(&e, "1"), Point::default());
    assert_eq!(pin_at(&e, "2"), mil(600, -300));
    let s = symbol(&e, &sym);
    let g = s
        .graphics
        .iter()
        .find(|g| g.uuid.to_string() == rect)
        .unwrap();
    assert_eq!(
        g.shape,
        reda_engine::model::Shape::Rect {
            start: mil(100, -300),
            end: mil(500, 0)
        },
        "graphics move with the Pins"
    );

    // Everything spans x 0…600 and y -300…0 mil: the centre (300, -150) rounds to the
    // 50 mil grid, so the Pins stay on it.
    e.run_ok(
        "library.symbol.origin.set",
        json!({"symbol": sym, "to": {"kind": "centre"}}),
    );
    assert_eq!(pin_at(&e, "1"), mil(-300, 150));
    assert_eq!(pin_at(&e, "2"), mil(300, -150));

    e.run_ok(
        "library.symbol.origin.set",
        json!({"symbol": sym, "to": {"kind": "pick", "at": {"x": "300mil", "y": "-150mil"}}}),
    );
    assert_eq!(pin_at(&e, "2"), Point::default());
    e.run_ok("edit.undo", json!({"document": sym}));
    assert_eq!(pin_at(&e, "2"), mil(300, -150), "each is one undo step");
}

#[test]
fn setting_the_origin_to_pin_1_needs_a_pin_1() {
    let (home, work) = (TempDir::new("sym-org1-home"), TempDir::new("sym-org1-work"));
    let (mut e, sym) = new_symbol(&home, &work);
    let refused = e.run_err(
        "library.symbol.origin.set",
        json!({"symbol": sym, "to": {"kind": "pin1"}}),
    );
    assert_eq!(refused.code, ErrorCode::Rejected);
    let refused = e.run_err(
        "library.symbol.origin.set",
        json!({"symbol": sym, "to": {"kind": "centre"}}),
    );
    assert_eq!(refused.code, ErrorCode::Rejected, "nothing to centre on");
}
