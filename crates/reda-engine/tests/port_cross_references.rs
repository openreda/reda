//! Port Cross-References and jump to partner: `schematic.port.partners`, the Project's
//! cross-reference options and the Preferences a Project can follow instead.

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_in};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

/// A new Project with three sheets; returns the engine and the three sheets' UUIDs.
fn demo(home: &TempDir) -> (TestEngine, [String; 3]) {
    let mut engine = test_engine_in(home.path());
    engine.run_ok("project.new", json!({"path": home.path(), "name": "Demo"}));
    let mut add = || {
        let out = engine.run_ok("document.add", json!({"kind": "schematic"}));
        out["result"]["uuid"].as_str().unwrap().to_owned()
    };
    let (two, three) = (add(), add());
    (engine, [SHEET.to_owned(), two, three])
}

/// A point given in millimetres, as Command params.
fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mm"), "y": format!("{y}mm")})
}

fn port(engine: &mut TestEngine, sheet: &str, name: &str, x: i64, y: i64) -> String {
    let out = engine.run_ok(
        "schematic.port.add",
        json!({"document": sheet, "name": name, "at": at(x, y)}),
    );
    out["result"]["port"].as_str().unwrap().to_owned()
}

fn partners(engine: &mut TestEngine, port: &str) -> Value {
    engine.run_ok("schematic.port.partners", json!({"port": port}))
}

fn note(engine: &mut TestEngine, port: &str) -> Value {
    partners(engine, port)["note"].clone()
}

#[test]
fn partners_are_same_named_ports_on_other_sheets_in_sheet_order() {
    let home = TempDir::new("xref-partners");
    let (mut engine, [one, two, three]) = demo(&home);
    let here = port(&mut engine, &one, "SDA", 10, 10);
    port(&mut engine, &one, "SDA", 200, 10); // same sheet: not a partner
    let on_three = port(&mut engine, &three, "sda", 120, 150); // case is ignored
    let on_two = port(&mut engine, &two, "SDA", 10, 200);
    port(&mut engine, &two, "SCL", 10, 100);
    engine.run_ok(
        "document.rename",
        json!({"document": two, "title": "Power"}),
    );

    let out = partners(&mut engine, &here);
    let list = out["partners"].as_array().unwrap();
    assert_eq!(list.len(), 2, "{out}");
    assert_eq!(list[0]["port"], on_two.as_str());
    assert_eq!(list[0]["document"], two.as_str());
    assert_eq!(list[0]["sheet_number"], 2);
    assert_eq!(list[0]["sheet_title"], "Power");
    // A4 landscape: six columns and four rows, row A at the top.
    assert_eq!(list[0]["zone"], "A1");
    assert_eq!(list[1]["port"], on_three.as_str());
    assert_eq!(list[1]["zone"], "B3");
    // The Project shows no cross-references until it opts in.
    assert_eq!(out["note"], json!(null));

    let err = engine.run_err("schematic.port.partners", json!({"port": SHEET}));
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn the_note_follows_the_projects_sheet_and_location_styles() {
    let home = TempDir::new("xref-styles");
    let (mut engine, [one, two, three]) = demo(&home);
    let here = port(&mut engine, &one, "EN", 10, 10);
    port(&mut engine, &two, "EN", 10, 200);
    port(&mut engine, &three, "EN", 120, 150);
    engine.run_ok(
        "document.rename",
        json!({"document": two, "title": "Power"}),
    );
    let alone = port(&mut engine, &one, "NC", 10, 50);

    let out = engine.run_ok("schematic.options.set", json!({"cross_references": true}));
    assert_eq!(out["result"]["cross_references"], true);
    // Sheet number and Border Zone by default.
    assert_eq!(note(&mut engine, &here), "[2:A1, 3:B3]");
    // A Port with no partners shows nothing.
    assert_eq!(note(&mut engine, &alone), json!(null));

    let styles = |engine: &mut TestEngine, sheet: &str, location: &str| {
        engine.run_ok(
            "schematic.options.set",
            json!({"cross_reference_sheet_style": sheet,
                   "cross_reference_location_style": location}),
        );
    };
    styles(&mut engine, "name", "xy");
    assert_eq!(
        note(&mut engine, &here),
        "[Power:(10,200), Sheet 3:(120,150)]"
    );
    styles(&mut engine, "none", "zone");
    assert_eq!(note(&mut engine, &here), "[A1, B3]");
    styles(&mut engine, "none", "none");
    assert_eq!(note(&mut engine, &here), json!(null));

    // On a sheet with Border Zones off, `zone` gives only the sheet.
    styles(&mut engine, "number", "zone");
    engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": three, "border_zones": false}),
    );
    assert_eq!(note(&mut engine, &here), "[2:A1, 3]");
}

#[test]
fn a_port_can_hide_its_note() {
    let home = TempDir::new("xref-hide");
    let (mut engine, [one, two, _]) = demo(&home);
    let here = port(&mut engine, &one, "EN", 10, 10);
    port(&mut engine, &two, "EN", 10, 200);
    engine.run_ok("schematic.options.set", json!({"cross_references": true}));
    engine.run_ok(
        "schematic.port.set",
        json!({"uuid": here, "hide_cross_reference": true}),
    );
    let out = partners(&mut engine, &here);
    assert_eq!(out["note"], json!(null));
    // Jump to partner still finds it.
    assert_eq!(out["partners"].as_array().unwrap().len(), 1);
}

#[test]
fn a_project_that_follows_the_preferences_takes_their_styles() {
    let home = TempDir::new("xref-prefs");
    let (mut engine, [one, two, _]) = demo(&home);
    let here = port(&mut engine, &one, "EN", 10, 10);
    port(&mut engine, &two, "EN", 10, 200);
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.cross_reference_sheet_style", "value": "name"}),
    );
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.cross_reference_location_style", "value": "none"}),
    );
    engine.run_ok("schematic.options.set", json!({"cross_references": true}));
    assert_eq!(note(&mut engine, &here), "[2:A1]");
    engine.run_ok(
        "schematic.options.set",
        json!({"cross_references_follow_preferences": true}),
    );
    assert_eq!(note(&mut engine, &here), "[Sheet 2]");
}

#[test]
fn the_cross_reference_options_are_saved_in_the_project_file() {
    let home = TempDir::new("xref-saved");
    let (mut engine, _) = demo(&home);
    engine.run_ok(
        "schematic.options.set",
        json!({"cross_references": true, "cross_reference_location_style": "xy"}),
    );
    engine.run_ok("project.save", json!({}));
    let folder = home.path().join("Demo");
    let file = std::fs::read_to_string(folder.join("Demo.reda")).unwrap();
    assert!(file.contains(r#""cross_references":true"#), "{file}");
    assert!(file.contains(r#""location":"xy""#), "{file}");

    let mut reopened = test_engine_in(home.path());
    reopened.run_ok("project.open", json!({"path": folder}));
    let out = reopened.run_ok("schematic.options.set", json!({}));
    assert_eq!(out["result"]["cross_reference_styles"]["location"], "xy");
    assert_eq!(out["result"]["cross_references_follow_preferences"], false);
}
