//! Zones and Keepouts: their objects and Commands.

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{LayerKind, PadConnection, PcbDocument, Side, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn add_net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn square(x0: &str, y0: &str, x1: &str, y1: &str) -> Value {
    json!([{"x": x0, "y": y0}, {"x": x1, "y": y0}, {"x": x1, "y": y1}, {"x": x0, "y": y1}])
}

fn pointer(err: &reda_command::CommandError) -> &str {
    err.details
        .as_ref()
        .and_then(|d| d["pointer"].as_str())
        .unwrap_or("")
}

#[test]
fn a_zone_takes_its_defaults_from_the_board_rules() {
    let dir = TempDir::new("zone-add");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let out = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_bottom", "net": gnd.to_string(),
               "vertices": square("0mm", "0mm", "10mm", "10mm")}),
    );
    let zone = uuid(&out["result"]["uuid"]);
    let pcb = pcb(&engine);
    let zone = pcb.zone(zone).unwrap();
    let bottom = pcb
        .layers()
        .find(LayerKind::Copper, Some(Side::Bottom))
        .unwrap();
    // The Generic preset's clearance and minimum Track width.
    let (clearance, track_min) = (Nm::from_um(160), Nm::from_um(200));
    assert_eq!(zone.layer, bottom.uuid);
    assert_eq!(zone.net, Some(gnd));
    assert_eq!(zone.vertices.len(), 4);
    assert_eq!(zone.clearance, clearance);
    assert_eq!(zone.min_width, track_min);
    assert_eq!(zone.pad_connection, PadConnection::Thermal);
    assert_eq!(zone.thermal_gap, clearance);
    assert_eq!(zone.thermal_spoke_width, track_min);
    assert!(zone.remove_islands);
    assert_eq!(zone.priority, 0);
}

#[test]
fn zone_settings_can_be_given_and_changed() {
    let dir = TempDir::new("zone-set");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let vcc = add_net(&mut engine, "VCC");
    let out = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd.to_string(),
               "vertices": square("0mm", "0mm", "10mm", "10mm"),
               "clearance": "0.3mm", "min_width": "0.25mm", "pad_connection": "solid",
               "remove_islands": false, "priority": 2}),
    );
    let uuid = uuid(&out["result"]["uuid"]);
    let zone = pcb(&engine).zone(uuid).unwrap().clone();
    assert_eq!(zone.clearance, Nm::parse("0.3mm").unwrap());
    assert_eq!(zone.min_width, Nm::parse("0.25mm").unwrap());
    assert_eq!(zone.pad_connection, PadConnection::Solid);
    assert!(!zone.remove_islands);
    assert_eq!(zone.priority, 2);

    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": uuid.to_string(), "net": vcc.to_string(), "priority": 5,
               "thermal_gap": "0.4mm", "thermal_spoke_width": "0.3mm",
               "pad_connection": "thermal", "layer": "copper_bottom",
               "vertices": square("1mm", "1mm", "5mm", "5mm")}),
    );
    let zone = pcb(&engine).zone(uuid).unwrap();
    assert_eq!(zone.net, Some(vcc));
    assert_eq!(zone.priority, 5);
    assert_eq!(zone.thermal_gap, Nm::parse("0.4mm").unwrap());
    assert_eq!(zone.thermal_spoke_width, Nm::parse("0.3mm").unwrap());
    assert_eq!(zone.pad_connection, PadConnection::Thermal);
    assert_eq!(
        zone.vertices[0].point(),
        Point::new(Nm::from_mm(1), Nm::from_mm(1))
    );

    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": uuid.to_string(), "net": null}),
    );
    assert_eq!(pcb(&engine).zone(uuid).unwrap().net, None);

    engine.run_ok("pcb.zone.delete", json!({"zones": [uuid.to_string()]}));
    assert!(pcb(&engine).zone(uuid).is_none());
}

#[test]
fn a_zone_needs_a_copper_layer_a_closed_contour_and_a_positive_min_width() {
    let dir = TempDir::new("zone-bad");
    let mut engine = engine_with_pcb(&dir);
    let err = engine.run_err(
        "pcb.zone.add",
        json!({"layer": "silkscreen_top", "vertices": square("0mm", "0mm", "1mm", "1mm")}),
    );
    assert_eq!(
        (err.code, pointer(&err)),
        (ErrorCode::InvalidParams, "/layer")
    );
    let err = engine.run_err(
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": [{"x": 0, "y": 0}, {"x": 1, "y": 0}]}),
    );
    assert_eq!(
        (err.code, pointer(&err)),
        (ErrorCode::InvalidParams, "/vertices")
    );
    let err = engine.run_err(
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": square("0mm", "0mm", "1mm", "1mm"),
               "min_width": 0}),
    );
    assert_eq!(
        (err.code, pointer(&err)),
        (ErrorCode::InvalidParams, "/min_width")
    );
    let err = engine.run_err(
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": square("0mm", "0mm", "1mm", "1mm"),
               "net": "00000000-0000-4000-8000-0000000000ff"}),
    );
    assert_eq!(
        (err.code, pointer(&err)),
        (ErrorCode::InvalidParams, "/net")
    );
}

#[test]
fn a_keepout_restricts_copper_and_vias_on_its_layers() {
    let dir = TempDir::new("keepout-add");
    let mut engine = engine_with_pcb(&dir);
    let out = engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": square("0mm", "0mm", "3mm", "3mm")}),
    );
    let all = uuid(&out["result"]["uuid"]);
    let out = engine.run_ok(
        "pcb.keepout.add",
        json!({"layers": ["copper_top"], "vertices": square("5mm", "0mm", "8mm", "3mm"),
               "vias": false}),
    );
    let top_only = uuid(&out["result"]["uuid"]);
    let pcb_doc = pcb(&engine);
    let copper: Vec<Uuid> = pcb_doc.copper_layers().map(|l| l.uuid).collect();
    let keepout = pcb_doc.keepout(all).unwrap();
    assert_eq!(keepout.layers, copper);
    assert!(keepout.copper && keepout.vias);
    let keepout = pcb_doc.keepout(top_only).unwrap();
    assert_eq!(keepout.layers, vec![copper[0]]);
    assert!(keepout.copper && !keepout.vias);

    engine.run_ok(
        "pcb.keepout.set",
        json!({"keepout": top_only.to_string(), "copper": false, "vias": true}),
    );
    let keepout = pcb(&engine).keepout(top_only).unwrap();
    assert!(!keepout.copper && keepout.vias);

    let err = engine.run_err(
        "pcb.keepout.set",
        json!({"keepout": top_only.to_string(), "vias": false}),
    );
    assert_eq!(
        (err.code, pointer(&err)),
        (ErrorCode::InvalidParams, "/vias")
    );

    engine.run_ok("pcb.keepout.delete", json!({"keepouts": [all.to_string()]}));
    assert!(pcb(&engine).keepout(all).is_none());
}

#[test]
fn the_clearance_queries_report_the_keepouts_copper_would_enter() {
    let dir = TempDir::new("keepout-clearance");
    let mut engine = engine_with_pcb(&dir);
    let copper = engine.run_ok(
        "pcb.keepout.add",
        json!({"layers": ["copper_top"], "vertices": square("0mm", "0mm", "2mm", "2mm"),
               "vias": false}),
    )["result"]["uuid"]
        .clone();
    let vias = engine.run_ok(
        "pcb.keepout.add",
        json!({"layers": ["copper_bottom"], "vertices": square("10mm", "0mm", "12mm", "2mm"),
               "copper": false}),
    )["result"]["uuid"]
        .clone();
    let track = |e: &mut TestEngine, layer: &str, x: &str| {
        e.run_ok(
            "pcb.clearance.track",
            json!({"layer": layer, "from": {"x": x, "y": "-1mm"}, "to": {"x": x, "y": "3mm"}}),
        )
    };
    let out = track(&mut engine, "copper_top", "1mm");
    assert_eq!(out["clear"], false);
    assert_eq!(out["keepouts"], json!([copper]));
    let out = track(&mut engine, "copper_bottom", "1mm");
    assert_eq!(out["clear"], true, "the Keepout is on the top only");
    assert!(out.get("keepouts").is_none());
    let out = track(&mut engine, "copper_bottom", "11mm");
    assert_eq!(out["clear"], true, "Tracks may cross a Via Keepout");

    let via = |e: &mut TestEngine, x: &str| {
        e.run_ok("pcb.clearance.via", json!({"at": {"x": x, "y": "1mm"}}))
    };
    assert_eq!(via(&mut engine, "11mm")["keepouts"], json!([vias]));
    assert_eq!(
        via(&mut engine, "1mm")["keepouts"],
        json!([copper]),
        "a Via is copper too"
    );
    assert_eq!(via(&mut engine, "6mm")["clear"], true);
}

#[test]
fn zones_and_keepouts_survive_save_and_open() {
    let dir = TempDir::new("zone-file");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd.to_string(),
               "vertices": square("0mm", "0mm", "10mm", "10mm")}),
    );
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": square("1mm", "1mm", "2mm", "2mm")}),
    );
    engine.run_ok("project.save", json!({}));
    let before = pcb(&engine).clone();
    let mut other = probe_engine();
    other.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(other.project().unwrap().pcb().unwrap(), &before);
}
