//! `schematic.wire.route`: Auto Wire's query, over a real sheet.

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

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// Places a 10k resistor (Pins at 0, ±150 mil, body between) and returns its Designator.
fn place(engine: &mut TestEngine, x: i64, y: i64) -> String {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(x, y)}),
    );
    out["result"]["designator"].as_str().unwrap().to_owned()
}

fn route(engine: &mut TestEngine, from: (i64, i64), to: (i64, i64)) -> Value {
    engine.run_ok(
        "schematic.wire.route",
        json!({"document": SHEET, "from": at(from.0, from.1), "to": at(to.0, to.1)}),
    )
}

/// The route's vertices in mil.
fn mil(route: &Value) -> Vec<(i64, i64)> {
    route["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["x"].as_i64().unwrap() / 25_400,
                p["y"].as_i64().unwrap() / 25_400,
            )
        })
        .collect()
}

/// The `designator-number` of every Pin in the Net holding `designator`'s Pin `number`.
fn net_pins(engine: &mut TestEngine, designator: &str, number: &str) -> Vec<String> {
    let all = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    let net = all["nets"]
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
        .unwrap()
        .clone();
    let mut pins: Vec<String> = net["pins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            format!(
                "{}-{}",
                p["designator"].as_str().unwrap(),
                p["number"].as_str().unwrap()
            )
        })
        .collect();
    pins.sort();
    pins
}

#[test]
fn a_routed_wire_goes_round_a_resistor_and_joins_only_its_two_pins() {
    let dir = TempDir::new("route");
    let mut engine = demo(&dir);
    let left = place(&mut engine, 0, 0);
    let middle = place(&mut engine, 500, 0);
    let right = place(&mut engine, 1000, 0);
    // The straight line from Pin 1 to Pin 1 would pass over the middle resistor's Pin 1.
    let found = route(&mut engine, (0, 150), (1000, 150));
    let points = mil(&found);
    assert_eq!(points.first(), Some(&(0, 150)));
    assert_eq!(points.last(), Some(&(1000, 150)));
    assert_eq!(found["crossings"], 0);
    assert!(points.len() > 2, "{points:?}");

    let params: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": params}),
    );
    assert_eq!(
        net_pins(&mut engine, &left, "1"),
        [format!("{left}-1"), format!("{right}-1")]
    );
    assert_eq!(net_pins(&mut engine, &middle, "1"), [format!("{middle}-1")]);
}

#[test]
fn routing_twice_gives_the_same_path() {
    let dir = TempDir::new("route");
    let mut engine = demo(&dir);
    place(&mut engine, 0, 0);
    place(&mut engine, 300, 200);
    place(&mut engine, 700, -100);
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(-500, 400), at(1500, 400)]}),
    );
    let first = route(&mut engine, (0, -150), (700, 50));
    assert_eq!(route(&mut engine, (0, -150), (700, 50)), first);
}

#[test]
fn a_route_avoids_cutting_wires_as_asked() {
    let dir = TempDir::new("route");
    let mut engine = demo(&dir);
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(250, -100), at(250, 100)]}),
    );
    let ask = |engine: &mut TestEngine, avoid: &str| {
        engine.run_ok(
            "schematic.wire.route",
            json!({
                "document": SHEET, "from": at(0, 0), "to": at(500, 0),
                "avoid_cutting_wires": avoid
            }),
        )
    };
    assert_eq!(ask(&mut engine, "low")["crossings"], 1);
    assert_eq!(ask(&mut engine, "high")["crossings"], 0);
}

#[test]
fn bad_params_are_refused() {
    let dir = TempDir::new("route");
    let mut engine = demo(&dir);
    let refused = |engine: &mut TestEngine, extra: Value| {
        let mut params = json!({"document": SHEET, "from": at(0, 0), "to": at(500, 0)});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        engine.run_err("schematic.wire.route", params).code
    };
    assert_eq!(
        refused(&mut engine, json!({"to": at(0, 0)})),
        ErrorCode::InvalidParams
    );
    assert_eq!(
        refused(&mut engine, json!({"grid": "0mil"})),
        ErrorCode::InvalidParams
    );
    assert_eq!(
        refused(&mut engine, json!({"timeout_ms": 5})),
        ErrorCode::InvalidParams
    );
}

#[test]
fn routing_draws_nothing() {
    let dir = TempDir::new("route");
    let mut engine = demo(&dir);
    route(&mut engine, (0, 0), (500, 300));
    let wires = engine.run_ok("objects.query", json!({"document": SHEET, "type": "wire"}));
    assert_eq!(wires["objects"], json!([]));
}
