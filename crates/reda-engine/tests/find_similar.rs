//! `objects.find_similar`: the objects like a template, by attribute rules, on a sheet,
//! across the Project, on the board or in the current Library element.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESC1608: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";
const CAPACITOR: &str = "b4351766-9551-4986-9f92-46fd72d7e385";

fn starter_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&starter_library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// Places a Component and returns its Unit's UUID.
fn place(engine: &mut TestEngine, sheet: &str, part: &str, x: i64, value: &str) -> String {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": part, "at": {"x": format!("{x}mm"), "y": 0},
               "fields": {"value": value}}),
    );
    out["result"]["unit"].as_str().unwrap().to_owned()
}

fn find(engine: &mut TestEngine, params: Value) -> Value {
    engine.run_ok("objects.find_similar", params)
}

fn sorted(mut uuids: Vec<String>) -> Value {
    uuids.sort();
    json!(uuids)
}

#[test]
fn units_with_the_same_value_on_the_sheet() {
    let dir = TempDir::new("similar-sheet");
    let mut engine = engine(&dir);
    let r1 = place(&mut engine, SHEET, RESISTOR, 0, "10k");
    let r2 = place(&mut engine, SHEET, RESISTOR, 20, "10k");
    let _r3 = place(&mut engine, SHEET, RESISTOR, 40, "4k7");
    let c1 = place(&mut engine, SHEET, CAPACITOR, 60, "10k");

    // A Unit of another Part is the same kind.
    let out = find(
        &mut engine,
        json!({"template": r1, "rules": {"fields.value": "same"}, "scope": "sheet"}),
    );
    assert_eq!(
        out["objects"],
        sorted(vec![r1.clone(), r2.clone(), c1.clone()])
    );
    assert_eq!(out["pads"], json!([]));

    // Kind starts at Same: a Wire finds Wires only, unless `type` is `any`.
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [{"x": "0mm", "y": "30mm"}, {"x": "10mm", "y": "30mm"}]}),
    );
    let wire = engine
        .project()
        .unwrap()
        .schematic_sheet(SHEET.parse().unwrap())
        .unwrap()
        .wires()
        .next()
        .unwrap()
        .uuid
        .to_string();
    let out = find(&mut engine, json!({"template": wire, "scope": "sheet"}));
    assert_eq!(out["objects"], json!([wire]));
    let out = find(
        &mut engine,
        json!({"template": wire, "rules": {"type": "any"}, "scope": "sheet"}),
    );
    assert_eq!(out["objects"].as_array().unwrap().len(), 5, "{out}");
}

#[test]
fn different_and_patterns() {
    let dir = TempDir::new("similar-rules");
    let mut engine = engine(&dir);
    let r1 = place(&mut engine, SHEET, RESISTOR, 0, "10k");
    let r2 = place(&mut engine, SHEET, RESISTOR, 20, "10k");
    let r3 = place(&mut engine, SHEET, RESISTOR, 40, "4k7");

    let out = find(
        &mut engine,
        json!({"template": r1, "rules": {"fields.value": "different"}, "scope": "sheet"}),
    );
    assert_eq!(out["objects"], json!([r3]));

    // A `*` pattern, ignoring case.
    let out = find(
        &mut engine,
        json!({"template": r1, "rules": {"fields.value": "*K"}, "scope": "sheet"}),
    );
    assert_eq!(out["objects"], sorted(vec![r1.clone(), r2.clone()]));
    let out = find(
        &mut engine,
        json!({"template": r1, "rules": {"designator": "R*"}, "scope": "sheet"}),
    );
    assert_eq!(out["objects"], sorted(vec![r1, r2, r3]));
}

#[test]
fn project_scope_spans_every_sheet() {
    let dir = TempDir::new("similar-project");
    let mut engine = engine(&dir);
    let out = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let sheet2 = out["result"]["uuid"].as_str().unwrap().to_owned();
    let r1 = place(&mut engine, SHEET, RESISTOR, 0, "10k");
    let r2 = place(&mut engine, &sheet2, RESISTOR, 0, "10k");

    let on_sheet = find(&mut engine, json!({"template": r1, "scope": "sheet"}));
    assert_eq!(on_sheet["objects"], json!([r1]));
    let everywhere = find(&mut engine, json!({"template": r1, "scope": "project"}));
    // Sheet by sheet in Document order.
    assert_eq!(everywhere["objects"], json!([r1, r2]));
}

/// Two synced 0603 resistors and a capacitor on the board; returns R1's Footprint
/// instance.
fn board(engine: &mut TestEngine) -> String {
    place(engine, SHEET, RESISTOR, 0, "10k");
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": RESISTOR, "footprint": RESC1608,
               "at": {"x": "20mm", "y": 0}, "fields": {"value": "10k"}}),
    );
    place(engine, SHEET, CAPACITOR, 40, "100n");
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash": hash}));
    footprint(engine, "R1")
}

fn footprint(engine: &TestEngine, designator: &str) -> String {
    let pcb = engine.project().unwrap().pcb().unwrap();
    let f = pcb
        .footprints()
        .find(|f| f.designator == designator)
        .unwrap();
    f.uuid.to_string()
}

#[test]
fn footprints_on_the_board() {
    let dir = TempDir::new("similar-board");
    let mut engine = engine(&dir);
    let r1 = board(&mut engine);
    let r2 = footprint(&engine, "R2");
    let c1 = footprint(&engine, "C1");

    let out = find(&mut engine, json!({"template": r1, "scope": "board"}));
    assert_eq!(out["objects"], sorted(vec![r1.clone(), r2.clone(), c1]));
    // The two resistors use different Footprint copies.
    let out = find(
        &mut engine,
        json!({"template": r1, "rules": {"footprint": "same"}, "scope": "board"}),
    );
    assert_eq!(out["objects"], json!([r1]));
    let out = find(
        &mut engine,
        json!({"template": r1, "rules": {"fields.value": "same"}, "scope": "board"}),
    );
    assert_eq!(out["objects"], sorted(vec![r1, r2]));
}

#[test]
fn pads_can_be_found() {
    let dir = TempDir::new("similar-pads");
    let mut engine = engine(&dir);
    let r1 = board(&mut engine);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let instance = pcb.footprint(r1.parse().unwrap()).unwrap();
    let pads: Vec<String> = instance.pads.iter().map(|p| p.pad.to_string()).collect();
    let copy = pcb.footprint_copy(instance.footprint).unwrap();
    let number_of = |pad: &str| {
        let pad = copy
            .footprint
            .pads
            .iter()
            .find(|p| p.uuid.to_string() == pad);
        pad.unwrap().number.clone()
    };
    let first = pads.iter().find(|p| number_of(p) == "1").unwrap().clone();

    let template = json!({"footprint": r1, "pad": first});
    let out = find(
        &mut engine,
        json!({"template": template, "rules": {"number": "same"}, "scope": "board"}),
    );
    assert_eq!(out["objects"], json!([]));
    // Pin 1 of each of the three Footprints.
    let found = out["pads"].as_array().unwrap();
    assert_eq!(found.len(), 3, "{out}");
    assert!(found.contains(&template), "{out}");

    // A Pad in Board scope finds only Pads, whatever the other rules say.
    let out = find(
        &mut engine,
        json!({"template": template, "rules": {"footprint": "same"}, "scope": "board"}),
    );
    let found: Vec<&Value> = out["pads"].as_array().unwrap().iter().collect();
    assert_eq!(found.len(), 2, "{out}");
    assert!(found.iter().all(|p| p["footprint"] == json!(r1)));
}

#[test]
fn element_scope_finds_the_parts_of_an_open_library_element() {
    let dir = TempDir::new("similar-element");
    let mut engine = engine(&dir);
    engine.run_ok("library.element.open", json!({"element": RESC1608}));
    let footprint = match engine.project().unwrap().element(RESC1608.parse().unwrap()) {
        Some(reda_engine::model::LibraryElement::Footprint(f)) => f.clone(),
        other => panic!("{other:?}"),
    };
    let pad = footprint.pads[0].uuid.to_string();
    let out = find(&mut engine, json!({"template": pad, "scope": "element"}));
    let mut pads: Vec<String> = footprint.pads.iter().map(|p| p.uuid.to_string()).collect();
    pads.sort();
    assert_eq!(out["objects"], json!(pads));
    // A text is another kind.
    let text = footprint.texts[0].uuid.to_string();
    let out = find(&mut engine, json!({"template": text, "scope": "element"}));
    assert_eq!(
        out["objects"].as_array().unwrap().len(),
        footprint.texts.len()
    );
    assert!(out["objects"].as_array().unwrap().contains(&json!(text)));
}

#[test]
fn bad_requests_are_refused_with_a_pointer() {
    let dir = TempDir::new("similar-bad");
    let mut engine = engine(&dir);
    let r1 = place(&mut engine, SHEET, RESISTOR, 0, "10k");

    let err = engine.run_err(
        "objects.find_similar",
        json!({"template": r1, "scope": "board"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{}", err.message);
    assert_eq!(
        err.details,
        Some(json!({"pointer": "/scope"})),
        "{}",
        err.message
    );

    let err = engine.run_err(
        "objects.find_similar",
        json!({"template": r1, "rules": {"colour": "same"}, "scope": "sheet"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(
        err.details,
        Some(json!({"pointer": "/rules/colour"})),
        "{}",
        err.message
    );

    let err = engine.run_err(
        "objects.find_similar",
        json!({"template": PCB, "scope": "board"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);

    let err = engine.run_err(
        "objects.find_similar",
        json!({"template": "ffffffff-0000-4000-8000-000000000000", "scope": "sheet"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn element_scope_works_without_a_project() {
    let mut engine = test_engine_with_library(&starter_library());
    engine.run_ok("library.element.open", json!({"element": RESC1608}));
    let pad = match engine
        .elements()
        .unwrap()
        .element(RESC1608.parse().unwrap())
    {
        Some(reda_engine::model::LibraryElement::Footprint(f)) => f.pads[0].uuid.to_string(),
        other => panic!("{other:?}"),
    };
    let out = find(&mut engine, json!({"template": pad, "scope": "element"}));
    assert_eq!(out["objects"].as_array().unwrap().len(), 2, "{out}");
    let err = engine.run_err(
        "objects.find_similar",
        json!({"template": pad, "scope": "sheet"}),
    );
    assert_eq!(
        err.details,
        Some(json!({"pointer": "/scope"})),
        "{}",
        err.message
    );
}
