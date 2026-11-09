//! How a user's Preferences and the Project's derived data draw sheets on the canvas and
//! in outputs.

use reda_engine::preferences;
use reda_engine::schematic_style::{Drawn, schematic_style};
use reda_model::{UnitSuffix, Uuid};
use reda_testkit::{TempDir, TestEngine, test_engine_in};
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn demo(home: &TempDir) -> TestEngine {
    let mut engine = test_engine_in(home.path());
    engine.run_ok("project.new", json!({"path": home.path(), "name": "Demo"}));
    engine
}

fn style(engine: &TestEngine, drawn: Drawn) -> reda_engine::render::SchematicStyle {
    let prefs = preferences::current(engine.user_dirs());
    schematic_style(engine.project().unwrap(), &prefs, drawn)
}

fn set(engine: &mut TestEngine, key: &str, value: serde_json::Value) {
    engine.run_ok("app.preferences.set", json!({"key": key, "value": value}));
}

#[test]
fn the_drawing_preferences_reach_the_canvas_and_outputs() {
    let home = TempDir::new("style-prefs");
    let mut engine = demo(&home);
    let before = style(&engine, Drawn::Canvas);
    assert!(!before.single_negation);
    assert_eq!(before.unit_suffix, UnitSuffix::Alpha);
    set(&mut engine, "schematic.single_negation", json!(true));
    set(&mut engine, "schematic.unit_suffix", json!("numeric"));
    for drawn in [Drawn::Canvas, Drawn::Output] {
        let after = style(&engine, drawn);
        assert!(after.single_negation, "{drawn:?}");
        assert_eq!(after.unit_suffix, UnitSuffix::Numeric, "{drawn:?}");
    }
}

#[test]
fn only_the_canvas_tints_empty_title_fields() {
    let home = TempDir::new("style-canvas");
    let engine = demo(&home);
    assert!(style(&engine, Drawn::Canvas).canvas);
    assert!(!style(&engine, Drawn::Output).canvas);
}

#[test]
fn display_cross_overs_reaches_the_canvas_and_never_an_output() {
    let home = TempDir::new("style-cross-overs");
    let mut engine = demo(&home);
    assert!(!style(&engine, Drawn::Canvas).cross_overs, "off by default");
    set(&mut engine, "schematic.display_cross_overs", json!(true));
    assert!(style(&engine, Drawn::Canvas).cross_overs);
    assert!(!style(&engine, Drawn::Output).cross_overs);
}

#[test]
fn port_cross_references_come_from_the_project_once_it_opts_in() {
    let home = TempDir::new("style-xref");
    let mut engine = demo(&home);
    let two = engine.run_ok("document.add", json!({"kind": "schematic"}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut port = |sheet: &str, x: &str| -> Uuid {
        engine.run_ok(
            "schematic.port.add",
            json!({"document": sheet, "name": "SDA", "at": {"x": x, "y": "10mm"}}),
        )["result"]["port"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    let here = port(SHEET, "10mm");
    let there = port(&two, "10mm");
    assert!(style(&engine, Drawn::Canvas).cross_references.is_empty());
    engine.run_ok("schematic.options.set", json!({"cross_references": true}));
    let notes = style(&engine, Drawn::Output).cross_references;
    // A4: (10, 10) mm is in the bottom row, D, of the first column.
    assert_eq!(notes.get(&here).map(String::as_str), Some("[2:D1]"));
    assert_eq!(notes.get(&there).map(String::as_str), Some("[1:D1]"));

    // Each entry leads to its partner, on its sheet.
    let links = style(&engine, Drawn::Output).cross_reference_links;
    let ten = reda_model::geom::Nm::from_mm(10);
    let link = &links[&here];
    assert_eq!(link.len(), 1);
    assert_eq!(link[0].entry, "2:D1");
    assert_eq!(link[0].sheet.to_string(), two);
    assert_eq!(link[0].at, reda_model::geom::Point::new(ten, ten));
    assert_eq!(links[&there][0].sheet.to_string(), SHEET);
}
