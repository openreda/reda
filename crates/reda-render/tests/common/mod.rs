//! A sample board shared by the PCB Scene, `document.render` and PNG snapshot tests.

#![allow(dead_code)]

use reda_model::{LayerKind, Side, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

pub fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

pub fn mm(v: f64) -> i64 {
    (v * 1e6).round() as i64
}

pub fn xy(x: f64, y: f64) -> Value {
    json!({"x": mm(x), "y": mm(y)})
}

pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!([xy(x0, y0), xy(x1, y0), xy(x1, y1), xy(x0, y1)])
}

/// An engine with a new Project (and its empty PCB) in `dir`.
pub fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

pub fn layer(engine: &TestEngine, kind: LayerKind, side: Option<Side>) -> Uuid {
    let pcb = engine.project().unwrap().pcb().unwrap();
    pcb.layers().find(kind, side).unwrap().uuid
}

/// The ids a sample board's tests look at.
pub struct Board {
    pub pcb: Uuid,
    pub fp1: Uuid,
    pub track: Uuid,
    pub zone: Uuid,
}

/// A 30 × 20 mm board: two Footprints (one on the bottom) whose SIG Pads are not yet
/// routed, a GND Track and Via, a plated mounting hole, a bottom GND Zone (filled),
/// silkscreen text and a line.
pub fn sample_board(engine: &mut TestEngine) -> Board {
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "30mm", "height": "20mm"}),
    );
    let gnd = uuid(&engine.run_ok("test.net.add", json!({"name": "GND"}))["result"]["uuid"]);
    let sig = uuid(&engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    let fp1 = engine.run_ok(
        "test.footprint.add",
        json!({"at": xy(8.0, 10.0), "pads": [
            {"at": xy(-1.0, 0.0), "width": mm(1.0), "height": mm(1.4), "net": sig.to_string()},
            {"at": xy(1.0, 0.0), "width": mm(1.0), "height": mm(1.4), "net": gnd.to_string(),
             "shape": "rounded_rect", "corner_ratio": 40_000}]}),
    );
    let fp1 = uuid(&fp1["result"]["footprint"]);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": xy(22.0, 10.0), "side": "bottom", "pads": [
            {"at": xy(0.0, 0.0), "width": mm(1.2), "height": mm(1.2), "shape": "circle",
             "net": sig.to_string()}]}),
    );
    let track = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": gnd.to_string(), "width": "0.5mm",
               "points": [xy(9.0, 10.0), xy(15.0, 10.0)]}),
    );
    let track = uuid(&track["result"]["tracks"][0]);
    engine.run_ok(
        "pcb.via.add",
        json!({"at": xy(15.0, 10.0), "net": gnd.to_string()}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "circle", "at": xy(3.0, 3.0),
               "width": mm(3.0), "hole": {"width": mm(1.6)}, "net": gnd.to_string()}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "REDA", "x": "4mm", "y": "16mm",
               "height": "1.5mm"}),
    );
    engine.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "silkscreen_top", "from": xy(4.0, 15.0), "to": xy(10.0, 15.0),
               "width": "0.2mm"}),
    );
    let zone = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_bottom", "net": gnd.to_string(),
               "vertices": rect(1.0, 1.0, 29.0, 19.0), "clearance": "0.5mm"}),
    );
    let zone = uuid(&zone["result"]["uuid"]);
    engine.run_ok("pcb.zone.fill", json!({}));
    let pcb = engine.project().unwrap().pcb().unwrap().uuid;
    Board {
        pcb,
        fp1,
        track,
        zone,
    }
}
