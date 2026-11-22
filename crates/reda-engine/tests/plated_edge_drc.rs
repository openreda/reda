//! The DRC cases of docs/spec/v0.2.5/testing.md#drc-cases on one board, as a golden:
//! `edge_clearance` must fire for copper of another Net beside a Plated Edge and for a
//! normal Pad at the outline, and must not fire for an Edge Connector Pad; Plated Edges
//! that meet with different Nets are refused with an error.

use reda_testkit::{TempDir, TestEngine, assert_golden, probe_engine};
use serde_json::{Value, json};

fn mm(v: f64) -> i64 {
    (v * 1_000_000.0).round() as i64
}

fn pt(x: f64, y: f64) -> Value {
    json!({"x": mm(x), "y": mm(y)})
}

fn uuid(value: &Value) -> String {
    value.as_str().unwrap().to_owned()
}

fn net(engine: &mut TestEngine, name: &str) -> String {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

/// A 40 × 30 mm board: the left edge (3) plated on GND and the top edge (2) on VCC, so
/// they meet at (0, 30) with different Nets; a VCC Track 0.3 mm from the left edge; a
/// normal SMD Pad and an Edge Connector Pad each touching the unplated bottom edge.
fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let (gnd, vcc) = (net(&mut engine, "GND"), net(&mut engine, "VCC"));
    let outline = uuid(
        &engine.run_ok(
            "pcb.board_outline.add",
            json!({"vertices": [pt(0.0, 0.0), pt(40.0, 0.0), pt(40.0, 30.0), pt(0.0, 30.0)]}),
        )["result"]["uuid"],
    );
    engine.run_ok(
        "pcb.contour.edge.set",
        json!({"contour": outline, "edge": 2, "plated": {"net": vcc}}),
    );
    engine.run_ok(
        "pcb.contour.edge.set",
        json!({"contour": outline, "edge": 3, "plated": {"net": gnd}}),
    );
    // GND copper touching its own plating keeps it from the unconnected warning.
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.2mm", "net": gnd,
               "points": [pt(5.0, 10.0), pt(0.1, 10.0)]}),
    );
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.2mm", "net": vcc,
               "points": [pt(5.0, 20.0), pt(0.4, 20.0)]}),
    );
    // The bottom edge is y = 0: each Pad's copper runs from y = 0 to y = 2 mm.
    let pads = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(20.0, 1.0), "designator": "J1", "pads": [
            {"at": pt(-5.0, 0.0), "width": "1mm", "height": "2mm"},
            {"at": pt(5.0, 0.0), "width": "1mm", "height": "2mm"}]}),
    )["result"]
        .clone();
    engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": pads["footprint"], "pad": pads["pads"][1],
               "role": "edge_connector"}),
    );
    engine
}

#[test]
fn the_plated_edge_drc_cases_match_their_golden() {
    let dir = TempDir::new("plated-edge-drc");
    let mut engine = board(&dir);
    let mut report = engine.run_ok(
        "drc.run",
        json!({"checks": ["edge_clearance", "plated_edge_nets_differ",
                          "edge_connector_on_plated_edge", "plated_edge_without_net",
                          "plated_edge_unconnected"]}),
    );
    let checks: Vec<&str> = report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["check"].as_str().unwrap())
        .collect();
    // The VCC Track beside the GND plating and the normal Pad at the outline fire
    // `edge_clearance`; the Edge Connector Pad does not; the corner is an error. The
    // VCC plating's only copper is the Track beside the GND edge, which never touches it.
    assert_eq!(
        checks,
        [
            "edge_clearance",
            "edge_clearance",
            "plated_edge_nets_differ",
            "plated_edge_unconnected"
        ],
        "{report:#}"
    );
    report.as_object_mut().unwrap().remove("design");
    let text = serde_json::to_string_pretty(&report).unwrap() + "\n";
    assert_golden(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/plated_edges/drc.json"),
        text.as_bytes(),
    );
}
