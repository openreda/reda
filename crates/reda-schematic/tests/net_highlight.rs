//! Which objects Net highlighting lights up: the whole Project-wide Net under a picked
//! object, on every sheet, joined by Ports and Power Ports but not by same-named Net
//! Labels on other sheets.

use std::path::{Path, PathBuf};

use reda_model::Uuid;
use reda_model::geom::{Nm, Point};
use reda_schematic::connectivity::{net_at, nets_named};
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

fn pt(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 25_400), Nm(y * 25_400))
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn wire(engine: &mut TestEngine, sheet: &str, points: &[(i64, i64)]) -> Uuid {
    let points: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
    let out = engine.run_ok(
        "schematic.wire.add",
        json!({"document": sheet, "points": points}),
    );
    uuid(&out["result"]["wires"][0])
}

fn add(engine: &mut TestEngine, id: &str, sheet: &str, name: &str, x: i64, y: i64) -> Uuid {
    let out = engine.run_ok(id, json!({"document": sheet, "name": name, "at": at(x, y)}));
    let result = out["result"].as_object().unwrap();
    uuid(result.values().next().unwrap())
}

fn place(engine: &mut TestEngine, x: i64, y: i64) -> Uuid {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(x, y)}),
    );
    uuid(&out["result"]["unit"])
}

#[test]
fn a_picked_wire_lights_up_its_net_on_every_sheet_it_reaches_through_ports() {
    let dir = TempDir::new("highlight-ports");
    let mut engine = demo(&dir);
    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let second = added["result"]["uuid"].as_str().unwrap().to_owned();
    let sheet: Uuid = SHEET.parse().unwrap();

    let r1 = place(&mut engine, 0, 0);
    let a = wire(&mut engine, SHEET, &[(0, 150), (0, 300)]);
    let port_a = add(&mut engine, "schematic.port.add", SHEET, "SDA", 0, 300);
    let b = wire(&mut engine, &second, &[(0, 0), (500, 0)]);
    let port_b = add(&mut engine, "schematic.port.add", &second, "SDA", 500, 0);
    // A same-named Net Label on another sheet is a different Net.
    let c = wire(&mut engine, &second, &[(0, 1000), (500, 1000)]);
    add(
        &mut engine,
        "schematic.net_label.add",
        &second,
        "SDA",
        0,
        1000,
    );
    // An unrelated Wire on the first sheet.
    let d = wire(&mut engine, SHEET, &[(2000, 0), (2500, 0)]);

    let project = engine.project().unwrap();
    let net = net_at(project, sheet, a, pt(0, 200)).expect("a Net under the Wire");
    assert_eq!(net.name.as_deref(), Some("SDA"));
    for object in [a, port_a, b, port_b, r1] {
        assert!(net.objects.contains(&object), "{object} is on the Net");
    }
    assert!(!net.objects.contains(&c));
    assert!(!net.objects.contains(&d));

    // A Unit stands for the Net of its Pin nearest the pick: Pin 2 is 150 mil below.
    let pin_2 = net_at(project, sheet, r1, pt(0, -140)).unwrap();
    assert!(!pin_2.objects.contains(&a));
    assert!(pin_2.objects.contains(&r1));

    // By name: every Net called that, so both SDA Nets.
    let named = nets_named(project, "SDA");
    for object in [a, b, c] {
        assert!(named.contains(&object));
    }
    assert!(!named.contains(&d));
    assert!(nets_named(project, "NOPE").is_empty());
}

#[test]
fn nothing_is_under_an_object_that_carries_no_net() {
    let dir = TempDir::new("highlight-none");
    let mut engine = demo(&dir);
    let sheet: Uuid = SHEET.parse().unwrap();
    let text = engine.run_ok(
        "schematic.text.add",
        json!({"document": SHEET, "text": "Note", "at": at(0, 0)}),
    );
    let text = uuid(&text["result"]["text"]);
    let project = engine.project().unwrap();
    assert_eq!(net_at(project, sheet, text, pt(0, 0)), None);
}
