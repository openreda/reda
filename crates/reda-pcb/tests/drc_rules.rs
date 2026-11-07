//! DRC checks of the rule limits themselves: Via size, Track and Via maximums, hole size
//! for Pads and Free Pads only, rule records with blocking problems (`rule_invalid`) and
//! rules that allow less than a Fab Preset (`rule_outside_preset`).
//!
//! The board is the Generic preset's: Track width 0.20 mm min, Via 0.70 mm min with a
//! 0.30 mm hole min, Pad holes 0.50 to 6.30 mm.

use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn drc(engine: &mut TestEngine) -> Value {
    engine.run_ok("drc.run", json!({}))
}

/// The Violations of one check.
fn of(run: &Value, check: &str) -> Vec<Value> {
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

fn via(engine: &mut TestEngine, x: &str, diameter: &str, hole: &str) -> Uuid {
    uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": x, "y": 0}, "diameter": diameter, "hole": hole}),
        )["result"]["uuid"],
    )
}

/// `(objects, measured, required, limit, part)` of each Violation.
fn summary(found: &[Value]) -> Vec<(Vec<Uuid>, i64, i64, String, Value)> {
    found
        .iter()
        .map(|v| {
            (
                v["objects"].as_array().unwrap().iter().map(uuid).collect(),
                v["measured"]["length"].as_i64().unwrap(),
                v["required"]["length"].as_i64().unwrap(),
                v["limit"].as_str().unwrap().to_owned(),
                v["args"]["part"].clone(),
            )
        })
        .collect()
}

#[test]
fn via_diameter_and_hole_outside_via_size_are_reported_as_via_size() {
    let dir = TempDir::new("drc-via-size");
    let mut engine = engine(&dir);
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "via_size", "values": {"diameter": {"max": "1mm"}, "hole": {"max": "0.6mm"}}}),
    );
    let ok = via(&mut engine, "0mm", "0.7mm", "0.3mm");
    let small = via(&mut engine, "5mm", "0.6mm", "0.2mm");
    let big = via(&mut engine, "10mm", "1.2mm", "0.8mm");
    let run = drc(&mut engine);
    let found = of(&run, "via_size");
    assert_eq!(
        summary(&found),
        [
            (
                vec![small],
                600_000,
                700_000,
                "min".into(),
                json!("diameter")
            ),
            (vec![small], 200_000, 300_000, "min".into(), json!("hole")),
            (
                vec![big],
                1_200_000,
                1_000_000,
                "max".into(),
                json!("diameter")
            ),
            (vec![big], 800_000, 600_000, "max".into(), json!("hole")),
        ],
        "{run:#}"
    );
    assert!(
        found
            .iter()
            .all(|v| v["severity"] == "error" && v["rule"].is_string())
    );
    assert!(!found.iter().any(|v| v["objects"][0] == json!(ok)));
    assert_eq!(found[2]["args"]["maximum"], 1_000_000);
    assert_eq!(found[0]["args"]["minimum"], 700_000);
    // Via holes are not Pad holes.
    assert_eq!(of(&run, "hole_size"), Vec::<Value>::new(), "{run:#}");
}

#[test]
fn a_track_wider_than_the_maximum_is_reported() {
    let dir = TempDir::new("drc-track-max");
    let mut engine = engine(&dir);
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "track_width", "values": {"max": "1mm"}}),
    );
    let wide = uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "1.5mm",
                   "points": [{"x": "0mm", "y": "0mm"}, {"x": "5mm", "y": "0mm"}]}),
        )["result"]["tracks"][0],
    );
    let run = drc(&mut engine);
    let found = of(&run, "track_width");
    assert_eq!(
        summary(&found),
        [(vec![wide], 1_500_000, 1_000_000, "max".into(), Value::Null)],
        "{run:#}"
    );
    assert_eq!(found[0]["args"]["limit"], "max");
    assert_eq!(found[0]["args"]["maximum"], 1_000_000);
}

#[test]
fn pad_and_free_pad_holes_outside_hole_size_are_reported() {
    let dir = TempDir::new("drc-hole-size");
    let mut engine = engine(&dir);
    let placed = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "10mm", "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1.2mm", "height": "1.2mm", "shape": "circle",
             "kind": "plated_through_hole", "hole": {"width": 400_000, "height": 400_000}},
            {"at": {"x": "10mm", "y": 0}, "width": "1mm", "height": "1mm",
             "kind": "non_plated", "hole": {"width": 800_000, "height": 800_000}},
        ]}),
    )["result"]
        .clone();
    let footprint = uuid(&placed["footprint"]);
    let pth = uuid(&placed["pads"][0]);
    let mounting = uuid(
        &engine.run_ok(
            "pcb.free_pad.add",
            json!({"kind": "non_plated", "shape": "circle", "at": {"x": "40mm", "y": 0},
                   "width": "7mm", "hole": {"width": "7mm"}}),
        )["result"]["uuid"],
    );
    let run = drc(&mut engine);
    assert_eq!(
        summary(&of(&run, "hole_size")),
        [
            (
                vec![footprint, pth],
                400_000,
                500_000,
                "min".into(),
                Value::Null
            ),
            (
                vec![mounting],
                7_000_000,
                6_300_000,
                "max".into(),
                Value::Null
            ),
        ],
        "{run:#}"
    );
}

#[test]
fn a_rule_allowing_less_than_a_job_preset_is_reported_once_per_record_and_preset() {
    let dir = TempDir::new("drc-outside-preset");
    let mut engine = engine(&dir);
    // The new Project's job uses Generic (0.16 mm clearance); add a JLCPCB job (0.10 mm).
    let jlc = uuid(&engine.run_ok("fab_preset.add", json!({"name": "JLCPCB"}))["result"]["uuid"]);
    engine.run_ok("output.job.add", json!({"fab_preset": jlc}));
    let clearance = engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"value": "0.08mm"}}),
    )["result"]["rule"]["uuid"]
        .clone();
    // Below Generic's 0.20 mm and above JLCPCB's 0.10 mm.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "track_width", "values": {"min": "0.15mm"}}),
    );
    let run = drc(&mut engine);
    let found = of(&run, "rule_outside_preset");
    let rows: Vec<(Value, Value, Value)> = found
        .iter()
        .map(|v| {
            (
                v["args"]["rule"].clone(),
                v["args"]["preset"].clone(),
                v["measured"].clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                json!("clearance"),
                json!("Generic"),
                json!({"length": 80_000})
            ),
            (
                json!("clearance"),
                json!("JLCPCB"),
                json!({"length": 80_000})
            ),
            (
                json!("track_width"),
                json!("Generic"),
                json!({"length": 150_000})
            ),
        ],
        "{run:#}"
    );
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(found[0]["rule"], clearance);
    assert_eq!(found[0]["required"], json!({"length": 160_000}));
    assert_eq!(found[0]["limit"], "min");
    assert!(found[0].get("at").is_none(), "{:#}", found[0]);

    engine.run_ok(
        "drc.set_severity",
        json!({"check": "rule_outside_preset", "severity": "off"}),
    );
    assert!(of(&drc(&mut engine), "rule_outside_preset").is_empty());
}
