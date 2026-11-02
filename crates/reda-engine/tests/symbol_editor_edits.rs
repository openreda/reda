//! The Symbol editor's selection edits: moving and deleting any mix of Pins and graphics
//! in one Command, so each is one undo step.

use reda_engine::command::ErrorCode;
use reda_engine::model::LibraryElement;
use reda_engine::model::geom::{Nm, Point};
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
