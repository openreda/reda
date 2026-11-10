//! Paste Array's `batch` (library.md#placing-pins-and-pads): Pins step their numbers
//! (letters too) and names, keep their other fields, and leading zeroes go on request.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::LibraryElement;
use reda_ui::shell::Shell;
use serde_json::json;

const MINE: &str = "000000aa-0000-4000-8000-000000000000";

#[test]
fn pins_step_numbers_and_names_and_the_batch_is_one_undo_step() {
    let home = TempDir::new("paste-array-pins");
    let lib = home.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(&home.path().join("home"))))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    let symbol = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": "symbol", "name": "Bus"}),
        )
        .unwrap()["element"]
        .clone();
    let Some(LibraryElement::Symbol(s)) = shell
        .engine()
        .element(symbol.as_str().unwrap().parse().unwrap())
        .cloned()
    else {
        panic!("open");
    };
    shell
        .run(
            "library.symbol.pin.add",
            json!({"symbol": symbol, "unit": s.units[0].uuid, "name": "D08", "number": "A",
                   "at": {"x": 0, "y": 0}, "electrical_type": "bidirectional",
                   "description": "data"}),
        )
        .unwrap();
    let pin = match shell
        .engine()
        .element(symbol.as_str().unwrap().parse().unwrap())
    {
        Some(LibraryElement::Symbol(s)) => s.pins[0].uuid,
        _ => unreachable!(),
    };
    let out = shell
        .run(
            "library.paste_array",
            json!({"element": symbol, "objects": [pin], "count": 2, "dx": 0, "dy": "-2.54mm",
                   "remove_leading_zeroes": true}),
        )
        .unwrap();
    let batch = out["batch"].clone();
    let steps = batch["commands"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["id"], "library.symbol.pin.add");
    assert_eq!(steps[0]["params"]["number"], "B");
    assert_eq!(steps[1]["params"]["number"], "C");
    assert_eq!(steps[0]["params"]["name"], "D9");
    assert_eq!(steps[1]["params"]["name"], "D10");
    assert_eq!(steps[1]["params"]["at"], json!({"x": 0, "y": -5_080_000}));
    assert_eq!(steps[1]["params"]["description"], "data");
    shell.run("batch", batch).unwrap();
    let Some(LibraryElement::Symbol(s)) = shell
        .engine()
        .element(symbol.as_str().unwrap().parse().unwrap())
        .cloned()
    else {
        panic!("open");
    };
    assert_eq!(s.pins.len(), 3);
    assert!(
        s.pins
            .iter()
            .all(|p| p.electrical_type == s.pins[0].electrical_type)
    );
    // Nothing selected, nothing to paste.
    let refused = shell.run(
        "library.paste_array",
        json!({"element": symbol, "objects": [], "count": 2, "dx": 0, "dy": 0}),
    );
    assert!(refused.is_err());
}
