//! `library.where_used`: the Parts across the registered Libraries that use a Symbol or
//! Footprint, and the Components of the open Project that use any element, from the part
//! tables.

use reda_engine::Engine;
use reda_engine::command::{FixedClock, SeededIds, UserDirs};
use reda_testkit::{TempDir, TestEngine, starter_library, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const NPN: &str = "614301c4-68c2-4d22-998f-5c9f4d2dbd19";
const NPN_TO92: &str = "6d84ced2-62ac-42b8-817a-4c2a7c0013f7";
const NPN_SYMBOL: &str = "4b09f27b-e1a9-46d0-af71-1fa9775f2c85";
const SOT23: &str = "43ca8637-7f8d-4597-866a-d91d359dd6bb";
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";

fn place(engine: &mut TestEngine, part: &str, x: i64) {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": {"x": format!("{x}mm"), "y": "0mm"}}),
    );
}

/// A Project with NPN placed twice, NPN_TO-92 once and a resistor, for a user whose
/// folders are in `dir/home`.
fn project(dir: &TempDir) -> TestEngine {
    let mut engine = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
            .starter_library(Some(starter_library()))
            .build(),
    );
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    place(&mut engine, NPN, 0);
    place(&mut engine, NPN, 20);
    place(&mut engine, NPN_TO92, 40);
    place(&mut engine, RESISTOR, 60);
    engine
}

fn part_names(out: &Value) -> Vec<&str> {
    let parts = out["parts"].as_array().unwrap();
    parts.iter().map(|p| p["name"].as_str().unwrap()).collect()
}

fn designators(out: &Value) -> Vec<&str> {
    let components = out["components"].as_array().unwrap();
    components
        .iter()
        .map(|c| c["designator"].as_str().unwrap())
        .collect()
}

#[test]
fn a_symbol_is_used_by_the_parts_that_name_it_and_their_components() {
    let dir = TempDir::new("where-used-symbol");
    let mut engine = project(&dir);
    let out = engine.run_ok("library.where_used", json!({"element": NPN_SYMBOL}));
    assert_eq!(out["kind"], "symbol");
    assert_eq!(out["name"], "NPN");
    assert_eq!(part_names(&out), ["NPN", "NPN_TO-92"]);
    assert_eq!(out["parts"][0]["uuid"], NPN);
    assert_eq!(out["parts"][0]["library"]["origin"], "starter");
    assert_eq!(designators(&out), ["Q1", "Q2", "Q3"]);
    assert_eq!(out["components"][0]["sheet"], SHEET);
    assert_eq!(out["components"][0]["part"], NPN);
    assert_eq!(out["used_in_project"], 3);
}

#[test]
fn a_footprint_is_used_by_every_part_that_lists_it_as_an_alternate() {
    let dir = TempDir::new("where-used-footprint");
    let mut engine = project(&dir);
    let out = engine.run_ok("library.where_used", json!({"element": SOT23}));
    assert_eq!(out["kind"], "footprint");
    assert_eq!(part_names(&out), ["NMOS", "NPN", "PMOS", "PNP"]);
    assert_eq!(designators(&out), ["Q1", "Q2"]);
}

#[test]
fn a_part_is_used_only_by_components_placed_from_it() {
    let dir = TempDir::new("where-used-part");
    let mut engine = project(&dir);
    let out = engine.run_ok("library.where_used", json!({"element": NPN_TO92}));
    assert_eq!(out["kind"], "part");
    assert_eq!(part_names(&out), Vec::<&str>::new());
    assert_eq!(designators(&out), ["Q3"]);
    assert_eq!(out["used_in_project"], 1);
}

#[test]
fn with_no_project_only_library_parts_are_listed() {
    let mut engine = test_engine_with_library(&starter_library());
    let out = engine.run_ok("library.where_used", json!({"element": NPN_SYMBOL}));
    assert_eq!(part_names(&out), ["NPN", "NPN_TO-92"]);
    assert_eq!(out["components"], json!([]));
    assert_eq!(out["used_in_project"], 0);
}

#[test]
fn an_element_no_library_holds_is_still_found_in_the_project() {
    // The Starter Library turned off: NPN's source is missing, but its Components still
    // name it in the part table.
    let dir = TempDir::new("where-used-missing");
    let mut engine = project(&dir);
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.starter_active", "value": false}),
    );
    let out = engine.run_ok("library.where_used", json!({"element": NPN}));
    assert_eq!(out["kind"], "part");
    assert_eq!(out["name"], "NPN");
    assert_eq!(out["library"], Value::Null);
    assert_eq!(designators(&out), ["Q1", "Q2"]);

    let unknown = "00000000-0000-4000-8000-0000000000ff";
    let err = engine.run_err("library.where_used", json!({"element": unknown}));
    assert_eq!(err.code, reda_engine::command::ErrorCode::NotFound);
}
