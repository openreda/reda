//! Derived schematic connectivity: Wires, Net Labels, No-Connect Markers, Junctions and
//! the Nets that follow from them on one sheet.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// A point given in mil, as Command params.
fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// A point given in mil, as stored.
fn point(x: i64, y: i64) -> Value {
    json!({"x": x * 25_400, "y": y * 25_400})
}

/// Adds a Wire run through `points`, given in mil, and returns its Wires' UUIDs.
fn wire(engine: &mut TestEngine, points: &[(i64, i64)]) -> Vec<Value> {
    let points: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
    let out = engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": points}),
    );
    out["result"]["wires"].as_array().unwrap().clone()
}

fn object(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
}

/// Places Part `part` with its origin at (`x`, `y`) mil and returns its Designator.
fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64, rotation: i64) -> String {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({
            "document": SHEET, "part": part, "at": at(x, y), "rotation": format!("{rotation}deg")
        }),
    );
    out["result"]["designator"].as_str().unwrap().to_owned()
}

fn nets(engine: &mut TestEngine) -> Value {
    engine.run_ok("schematic.nets", json!({"document": SHEET}))
}

/// The derived Net holding Pin `number` of `designator`.
fn net_of_pin(engine: &mut TestEngine, designator: &str, number: &str) -> Value {
    let all = nets(engine);
    all["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| {
            n["pins"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["designator"] == designator && p["number"] == number)
        })
        .unwrap_or_else(|| panic!("{designator}-{number} is in no Net: {all}"))
        .clone()
}

/// The `designator-number` of every Pin in a Net.
fn pins(net: &Value) -> Vec<String> {
    net["pins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let designator = p["designator"].as_str().unwrap();
            format!("{designator}-{}", p["number"].as_str().unwrap())
        })
        .collect()
}

#[test]
fn a_wire_run_is_stored_as_one_wire_per_orthogonal_segment() {
    let dir = TempDir::new("wire-add");
    let mut engine = demo(&dir);
    let wires = wire(&mut engine, &[(0, 0), (100, 0), (100, 50)]);
    assert_eq!(wires.len(), 2);
    let first = object(&mut engine, &wires[0]);
    assert_eq!(first["type"], "wire");
    assert_eq!(first["from"], point(0, 0));
    assert_eq!(first["to"], point(100, 0));
    let second = object(&mut engine, &wires[1]);
    assert_eq!(second["from"], point(100, 0));
    assert_eq!(second["to"], point(100, 50));
}

#[test]
fn a_diagonal_or_empty_segment_is_refused() {
    let dir = TempDir::new("wire-diagonal");
    let mut engine = demo(&dir);
    let err = engine.run_err(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [
            {"x": 0, "y": 0}, {"x": 10, "y": 0}, {"x": 20, "y": 5}
        ]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/points/2");
    let err = engine.run_err(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [{"x": 0, "y": 0}, {"x": 0, "y": 0}]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [{"x": 0, "y": 0}]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

fn label(engine: &mut TestEngine, document: &str, name: &str, x: i64, y: i64) -> Value {
    let out = engine.run_ok(
        "schematic.net_label.add",
        json!({"document": document, "name": name, "at": at(x, y)}),
    );
    out["result"]["net_label"].clone()
}

// The test Resistor's Pin 1 is 150 mil above its origin and Pin 2 150 mil below.

#[test]
fn net_labels_with_the_same_name_join_their_wires_on_one_sheet() {
    let dir = TempDir::new("net-labels");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0, 0);
    place(&mut engine, R_10K, 1000, 0, 0);
    wire(&mut engine, &[(0, 150), (0, 300)]);
    wire(&mut engine, &[(1000, 150), (1000, 300)]);
    // One label at a Wire end, one partway along a Wire.
    let a = label(&mut engine, SHEET, "SDA", 0, 300);
    let b = label(&mut engine, SHEET, "SDA", 1000, 200);
    let net = net_of_pin(&mut engine, "R1", "1");
    assert_eq!(pins(&net), ["R1-1", "R2-1"]);
    assert_eq!(net["name"], "SDA");
    assert_eq!(net["labels"], json!(["SDA"]));
    let mut labels = vec![a, b];
    labels.sort_by_key(|l| l.as_str().unwrap().to_owned());
    assert_eq!(net["net_labels"], json!(labels));
}

#[test]
fn a_net_label_name_wins_over_the_auto_name_and_conflicts_are_listed() {
    let dir = TempDir::new("net-label-names");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0, 0);
    wire(&mut engine, &[(0, 150), (0, 300)]);
    label(&mut engine, SHEET, "VREF", 0, 300);
    label(&mut engine, SHEET, "ADC_IN", 0, 200);
    let net = net_of_pin(&mut engine, "R1", "1");
    assert_eq!(net["labels"], json!(["ADC_IN", "VREF"]));
    assert_eq!(net["name"], "ADC_IN");
}

#[test]
fn net_labels_do_not_connect_across_sheets() {
    let dir = TempDir::new("net-label-sheets");
    let mut engine = demo(&dir);
    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let second = added["result"]["uuid"].as_str().unwrap().to_owned();
    place(&mut engine, R_10K, 0, 0, 0);
    wire(&mut engine, &[(0, 150), (0, 300)]);
    label(&mut engine, SHEET, "SDA", 0, 300);
    label(&mut engine, &second, "SDA", 0, 0);
    let net = net_of_pin(&mut engine, "R1", "1");
    assert_eq!(net["net_labels"].as_array().unwrap().len(), 1);
}

#[test]
fn a_blank_net_label_name_is_refused() {
    let dir = TempDir::new("net-label-blank");
    let mut engine = demo(&dir);
    let err = engine.run_err(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "  ", "at": at(0, 0)}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn the_auto_name_uses_the_lowest_designator_in_natural_order_and_stays_put() {
    let dir = TempDir::new("net-auto-name");
    let mut engine = demo(&dir);
    for i in 0..10 {
        place(&mut engine, R_10K, i * 1000, 0, 0);
    }
    // Join R10 Pin 2 and R2 Pin 2: R2 sorts before R10.
    wire(
        &mut engine,
        &[(9000, -150), (9000, -500), (1000, -500), (1000, -150)],
    );
    assert_eq!(net_of_pin(&mut engine, "R10", "2")["name"], "Net-(R2-2)");
    // Unrelated edits leave the name alone.
    place(&mut engine, R_10K, 20_000, 0, 0);
    wire(&mut engine, &[(20_000, 150), (20_000, 400)]);
    label(&mut engine, SHEET, "OTHER", 20_000, 400);
    assert_eq!(net_of_pin(&mut engine, "R10", "2")["name"], "Net-(R2-2)");
    // Joining R1 Pin 1 to the Net renames it: R1 is now its lowest Designator.
    wire(
        &mut engine,
        &[(0, 150), (0, 200), (1500, 200), (1500, -500)],
    );
    assert_eq!(net_of_pin(&mut engine, "R10", "2")["name"], "Net-(R1-1)");
}

#[test]
fn a_wire_whose_ends_touch_pins_joins_them_in_one_net() {
    let dir = TempDir::new("net-pins");
    let mut engine = demo(&dir);
    assert_eq!(place(&mut engine, R_10K, 0, 0, 0), "R1");
    assert_eq!(place(&mut engine, R_10K, 500, 0, 0), "R2");
    let mut wires = wire(&mut engine, &[(0, 150), (0, 300), (500, 300), (500, 150)]);
    let net = net_of_pin(&mut engine, "R1", "1");
    assert_eq!(pins(&net), ["R1-1", "R2-1"]);
    assert_eq!(net["name"], "Net-(R1-1)");
    wires.sort_by_key(|w| w.as_str().unwrap().to_owned());
    assert_eq!(net["wires"], json!(wires));
    // Every Pin is in exactly one Net; unwired Pins are on their own.
    assert_eq!(pins(&net_of_pin(&mut engine, "R1", "2")), ["R1-2"]);
    assert_eq!(nets(&mut engine)["nets"].as_array().unwrap().len(), 3);
    // Two Wire ends meeting at a corner make no Junction.
    assert_eq!(nets(&mut engine)["junctions"], json!([]));
}

#[test]
fn a_rotated_component_has_its_pins_rotated_about_its_origin() {
    let dir = TempDir::new("net-rotated");
    let mut engine = demo(&dir);
    // Turned 90 degrees counter-clockwise, Pin 1 is 150 mil left of the origin.
    place(&mut engine, R_10K, 1000, 1000, 90);
    wire(&mut engine, &[(850, 1000), (700, 1000)]);
    let net = net_of_pin(&mut engine, "R1", "1");
    assert_eq!(net["wires"].as_array().unwrap().len(), 1);
}

#[test]
fn wires_connect_only_where_an_end_touches() {
    let dir = TempDir::new("net-ends");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0, 0);
    // A Wire running through a Pin's connection point without ending there misses it.
    wire(&mut engine, &[(-100, 150), (100, 150)]);
    assert_eq!(pins(&net_of_pin(&mut engine, "R1", "1")), ["R1-1"]);
    // Two Wires crossing mid-segment do not connect.
    let across = wire(&mut engine, &[(1000, 0), (1000, 200)]);
    let along = wire(&mut engine, &[(900, 100), (1100, 100)]);
    let all = nets(&mut engine);
    let net_of = |w: &Value| {
        all["nets"]
            .as_array()
            .unwrap()
            .iter()
            .position(|n| n["wires"].as_array().unwrap().contains(w))
            .unwrap()
    };
    assert_ne!(net_of(&across[0]), net_of(&along[0]));
    assert_eq!(all["junctions"], json!([]));
}

#[test]
fn a_wire_end_on_another_wire_connects_and_makes_a_junction() {
    let dir = TempDir::new("net-tee");
    let mut engine = demo(&dir);
    let bar = wire(&mut engine, &[(0, 0), (200, 0)]);
    let stem = wire(&mut engine, &[(100, 0), (100, -100)]);
    let all = nets(&mut engine);
    let nets = all["nets"].as_array().unwrap();
    assert_eq!(nets.len(), 1);
    assert!(nets[0]["wires"].as_array().unwrap().contains(&bar[0]));
    assert!(nets[0]["wires"].as_array().unwrap().contains(&stem[0]));
    // A Net with neither a Pin nor a name has no name.
    assert_eq!(nets[0]["name"], Value::Null);
    assert_eq!(all["junctions"], json!([point(100, 0)]));
    assert_eq!(nets[0]["junctions"], json!([point(100, 0)]));
}

#[test]
fn a_no_connect_marker_goes_on_a_pin() {
    let dir = TempDir::new("no-connect");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0, 0);
    let out = engine.run_ok(
        "schematic.no_connect.add",
        json!({"document": SHEET, "at": at(0, -150)}),
    );
    let marker = out["result"]["no_connect"].clone();
    assert_eq!(object(&mut engine, &marker)["type"], "no_connect");
    let net = net_of_pin(&mut engine, "R1", "2");
    assert_eq!(net["pins"][0]["no_connect"], marker);
    // Pin 1 has none.
    let net = net_of_pin(&mut engine, "R1", "1");
    assert_eq!(net["pins"][0].get("no_connect"), None);
    // Off a Pin, or on a Pin that already has one, it is refused.
    let err = engine.run_err(
        "schematic.no_connect.add",
        json!({"document": SHEET, "at": at(0, 0)}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    let err = engine.run_err(
        "schematic.no_connect.add",
        json!({"document": SHEET, "at": at(0, -150)}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn objects_query_can_filter_by_net_name() {
    let dir = TempDir::new("query-net");
    let mut engine = demo(&dir);
    let r1 = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(0, 0)}),
    )["result"]["unit"]
        .clone();
    let r2 = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(1000, 0)}),
    )["result"]["unit"]
        .clone();
    place(&mut engine, R_10K, 5000, 0, 0);
    let wires = wire(&mut engine, &[(0, 150), (0, 300), (1000, 300), (1000, 150)]);
    let sda = label(&mut engine, SHEET, "SDA", 500, 300);
    let out = engine.run_ok("objects.query", json!({"net": "SDA"}));
    let mut found: Vec<Value> = out["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"]["uuid"].clone())
        .collect();
    let mut expected: Vec<Value> = wires.clone();
    expected.extend([sda.clone(), r1.clone(), r2.clone()]);
    let key = |v: &Value| v.as_str().unwrap().to_owned();
    found.sort_by_key(key);
    expected.sort_by_key(key);
    assert_eq!(found, expected);
    // The filters combine.
    let out = engine.run_ok("objects.query", json!({"net": "SDA", "type": "unit"}));
    assert_eq!(out["objects"].as_array().unwrap().len(), 2);
    // Auto-names work too; an unknown name matches nothing.
    let out = engine.run_ok("objects.query", json!({"net": "Net-(R1-2)"}));
    assert_eq!(out["objects"].as_array().unwrap().len(), 1);
    let out = engine.run_ok("objects.query", json!({"net": "NOPE"}));
    assert_eq!(out["objects"], json!([]));
}

#[test]
fn the_drawing_is_saved_but_nets_and_junctions_are_not() {
    let dir = TempDir::new("save-drawing");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0, 0);
    wire(&mut engine, &[(0, 150), (0, 300), (200, 300)]);
    wire(&mut engine, &[(100, 300), (100, 400)]);
    label(&mut engine, SHEET, "SDA", 200, 300);
    engine.run_ok(
        "schematic.no_connect.add",
        json!({"document": SHEET, "at": at(0, -150)}),
    );
    let before = nets(&mut engine);
    engine.run_ok("project.save", json!({}));
    let (_, bytes) = reda_testkit::read_tree(dir.path())
        .into_iter()
        .find(|(path, _)| path.ends_with(".redasch"))
        .unwrap();
    let file = String::from_utf8(bytes).unwrap();
    assert!(file.contains("\"wires\""));
    assert!(file.contains("\"net_labels\""));
    assert!(file.contains("\"no_connects\""));
    assert!(!file.contains("junction"));
    assert!(!file.contains("Net-("));

    let mut reopened = test_engine_with_library(&library());
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(nets(&mut reopened), before);
}

/// Every Wire on the sheet as `((x1, y1), (x2, y2))` in mil, sorted.
fn wire_segments(engine: &mut TestEngine) -> Vec<((i64, i64), (i64, i64))> {
    let out = engine.run_ok("objects.query", json!({"document": SHEET, "type": "wire"}));
    let mil = |p: &Value| {
        (
            p["x"].as_i64().unwrap() / 25_400,
            p["y"].as_i64().unwrap() / 25_400,
        )
    };
    let mut segments: Vec<_> = out["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| {
            let (a, b) = (mil(&w["object"]["from"]), mil(&w["object"]["to"]));
            (a.min(b), a.max(b))
        })
        .collect();
    segments.sort();
    segments
}

#[test]
fn placing_a_component_with_a_pin_on_a_wire_splits_the_wire_and_connects() {
    let dir = TempDir::new("split-one");
    let mut engine = demo(&dir);
    let original = wire(&mut engine, &[(-500, 150), (500, 150)]);
    place(&mut engine, R_10K, 0, 0, 0);
    assert_eq!(
        wire_segments(&mut engine),
        [((-500, 150), (0, 150)), ((0, 150), (500, 150))]
    );
    let net = net_of_pin(&mut engine, "R1", "1");
    assert_eq!(net["wires"].as_array().unwrap().len(), 2);
    // The original Wire keeps its UUID for one of the halves.
    assert!(net["wires"].as_array().unwrap().contains(&original[0]));
    assert_eq!(net["junctions"], json!([point(0, 150)]));
    // Undo restores the whole Wire in one step.
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(wire_segments(&mut engine), [((-500, 150), (500, 150))]);
}

#[test]
fn dropping_a_component_inline_on_a_wire_removes_the_piece_between_its_pins() {
    let dir = TempDir::new("split-inline");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, -500), (0, 500)]);
    place(&mut engine, R_10K, 0, 0, 0);
    assert_eq!(
        wire_segments(&mut engine),
        [((0, -500), (0, -150)), ((0, 150), (0, 500))]
    );
    let top = net_of_pin(&mut engine, "R1", "1");
    let bottom = net_of_pin(&mut engine, "R1", "2");
    assert_eq!(pins(&top), ["R1-1"]);
    assert_eq!(pins(&bottom), ["R1-2"]);
    assert_eq!(top["wires"].as_array().unwrap().len(), 1);
    assert_eq!(bottom["wires"].as_array().unwrap().len(), 1);
}

#[test]
fn moving_a_component_onto_a_wire_splits_it_and_connects() {
    let dir = TempDir::new("split-move");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(2000, 0)}),
    );
    let unit = out["result"]["unit"].clone();
    wire(&mut engine, &[(-500, 150), (500, 150)]);
    assert_eq!(pins(&net_of_pin(&mut engine, "R1", "1")), ["R1-1"]);
    engine.run_ok("schematic.unit.move", json!({"unit": unit, "at": at(0, 0)}));
    assert_eq!(object(&mut engine, &unit)["at"], point(0, 0));
    assert_eq!(wire_segments(&mut engine).len(), 2);
    let net = net_of_pin(&mut engine, "R1", "1");
    assert_eq!(net["wires"].as_array().unwrap().len(), 2);
    // Rotation and mirroring can change with the move.
    engine.run_ok(
        "schematic.unit.move",
        json!({"unit": unit, "at": at(0, 0), "rotation": "180deg", "mirror": true}),
    );
    let moved = object(&mut engine, &unit);
    assert_eq!(moved["rotation"], 180_000);
    assert_eq!(moved["mirror"], true);
}

#[test]
fn find_by_net_name_returns_the_objects_on_the_derived_net() {
    let dir = TempDir::new("find-net");
    let mut engine = demo(&dir);
    let r1 = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(0, 0)}),
    )["result"]["unit"]
        .clone();
    let wires = wire(&mut engine, &[(0, 150), (0, 300)]);
    let sda = label(&mut engine, SHEET, "I2C_SDA", 0, 300);
    let out = engine.run_ok("schematic.find", json!({"text": "sda", "fields": ["net"]}));
    let matches = out["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["field"], "net");
    assert_eq!(matches[0]["text"], "I2C_SDA");
    assert_eq!(matches[0]["document"], SHEET);
    let mut expected = vec![r1, wires[0].clone(), sda];
    expected.sort_by_key(|v| v.as_str().unwrap().to_owned());
    assert_eq!(matches[0]["objects"], json!(expected));
    // A lone unwired Pin is not worth finding as a Net.
    let out = engine.run_ok(
        "schematic.find",
        json!({"text": "Net-(R1-2)", "fields": ["net"], "exact": true}),
    );
    assert_eq!(out["matches"], json!([]));
    // Auto-names of wired Nets are found.
    wire(&mut engine, &[(0, -150), (0, -300)]);
    let out = engine.run_ok(
        "schematic.find",
        json!({"text": "Net-(R1-2)", "fields": ["net"], "exact": true}),
    );
    assert_eq!(out["matches"].as_array().unwrap().len(), 1);
}
