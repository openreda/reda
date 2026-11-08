//! Zones and Keepouts: their objects and Commands.

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{KeepoutShape, LayerKind, PcbDocument, PourOver, Restriction, Side, Uuid};
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
    // Every value left out takes a fixed default: the rules alone for clearance and
    // width, pour over Zones only, unconnected pieces removed, no minimum island area.
    assert_eq!(zone.layer, bottom.uuid);
    assert_eq!(zone.net, Some(gnd));
    assert_eq!(zone.vertices.len(), 4);
    assert_eq!(zone.clearance, None);
    assert_eq!(zone.min_width, None);
    assert_eq!(zone.pour_over, PourOver::ZonesOnly);
    assert!(zone.remove_unconnected);
    assert_eq!(zone.min_island_area, 0);
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
               "clearance": "0.3mm", "min_width": "0.25mm", "pour_over": "all",
               "remove_unconnected": false, "min_island_area": "0.5mm2", "priority": 2}),
    );
    let uuid = uuid(&out["result"]["uuid"]);
    let zone = pcb(&engine).zone(uuid).unwrap().clone();
    assert_eq!(zone.clearance, Some(Nm::parse("0.3mm").unwrap()));
    assert_eq!(zone.min_width, Some(Nm::parse("0.25mm").unwrap()));
    assert_eq!(zone.pour_over, PourOver::All);
    assert!(!zone.remove_unconnected);
    assert_eq!(zone.min_island_area, 500_000_000_000);
    assert_eq!(zone.priority, 2);

    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": uuid.to_string(), "net": vcc.to_string(), "priority": 5,
               "pour_over": "none", "min_island_area": "100sqmil", "clearance": null,
               "layer": "copper_bottom",
               "vertices": square("1mm", "1mm", "5mm", "5mm")}),
    );
    let zone = pcb(&engine).zone(uuid).unwrap();
    assert_eq!(zone.net, Some(vcc));
    assert_eq!(zone.priority, 5);
    assert_eq!(zone.pour_over, PourOver::None);
    assert_eq!(zone.min_island_area, 64_516_000_000, "100 sq mil");
    assert_eq!(zone.clearance, None, "null leaves only the rule");
    assert_eq!(zone.min_width, Some(Nm::parse("0.25mm").unwrap()));
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
fn a_keepout_is_on_one_layer_or_all_a_polygon_or_a_line_and_restricts_chosen_kinds() {
    let dir = TempDir::new("keepout-add");
    let mut engine = engine_with_pcb(&dir);
    let out = engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": square("0mm", "0mm", "3mm", "3mm")}),
    );
    let all = uuid(&out["result"]["uuid"]);
    let out = engine.run_ok(
        "pcb.keepout.add",
        json!({"layer": "copper_top", "shape": "line", "width": "0.5mm",
               "vertices": [{"x": "5mm", "y": "0mm"}, {"x": "8mm", "y": "0mm"},
                            {"x": "8mm", "y": "3mm"}],
               "restrict": ["vias", "footprints"]}),
    );
    let line = uuid(&out["result"]["uuid"]);
    let pcb_doc = pcb(&engine);
    let copper: Vec<Uuid> = pcb_doc.copper_layers().map(|l| l.uuid).collect();

    // Left out, the Layer is every copper Layer and the kinds are all but Footprints.
    let keepout = pcb_doc.keepout(all).unwrap();
    assert_eq!(keepout.layer, None);
    assert_eq!(pcb_doc.keepout_layers(keepout), copper);
    assert!(matches!(keepout.shape, KeepoutShape::Polygon { ref vertices } if vertices.len() == 4));
    assert_eq!(
        keepout.restrict.iter().copied().collect::<Vec<_>>(),
        [
            Restriction::Tracks,
            Restriction::Vias,
            Restriction::Zones,
            Restriction::SmdPads,
            Restriction::ThtPads
        ]
    );
    let keepout = pcb_doc.keepout(line).unwrap();
    assert_eq!(keepout.layer, Some(copper[0]));
    assert_eq!(pcb_doc.keepout_layers(keepout), vec![copper[0]]);
    assert!(matches!(
        keepout.shape,
        KeepoutShape::Line { ref vertices, width } if vertices.len() == 3 && width == Nm(500_000)
    ));
    assert!(keepout.restricts(Restriction::Footprints) && keepout.restricts(Restriction::Vias));
    assert!(!keepout.restricts(Restriction::Tracks));

    // `set` changes what it is told: `null` puts it on every Layer.
    engine.run_ok(
        "pcb.keepout.set",
        json!({"keepout": line.to_string(), "layer": null, "width": "1mm",
               "restrict": ["tracks"]}),
    );
    let keepout = pcb(&engine).keepout(line).unwrap();
    assert_eq!(keepout.layer, None);
    assert!(matches!(keepout.shape, KeepoutShape::Line { width, .. } if width == Nm(1_000_000)));
    assert_eq!(keepout.restrict.len(), 1);
    engine.run_ok(
        "pcb.keepout.set",
        json!({"keepout": line.to_string(), "layer": "copper_bottom", "shape": "polygon",
               "vertices": square("5mm", "0mm", "8mm", "3mm")}),
    );
    let keepout = pcb(&engine).keepout(line).unwrap();
    assert_eq!(keepout.layer, Some(copper[1]));
    assert!(matches!(keepout.shape, KeepoutShape::Polygon { .. }));

    // What a Keepout can't be.
    let refused = [
        (json!({"vertices": square("0mm", "0mm", "1mm", "1mm"), "restrict": []}), "/restrict"),
        (json!({"shape": "line", "vertices": square("0mm", "0mm", "1mm", "1mm")}), "/width"),
        (json!({"vertices": square("0mm", "0mm", "1mm", "1mm"), "width": "1mm"}), "/width"),
        (json!({"shape": "line", "width": "1mm", "vertices": [{"x": 0, "y": 0}]}), "/vertices"),
        (json!({"layer": "silkscreen_top", "vertices": square("0mm", "0mm", "1mm", "1mm")}), "/layer"),
    ];
    for (params, at) in refused {
        let err = engine.run_err("pcb.keepout.add", params.clone());
        assert_eq!((err.code, pointer(&err)), (ErrorCode::InvalidParams, at), "{params}");
    }
    let err = engine.run_err(
        "pcb.keepout.set",
        json!({"keepout": all.to_string(), "shape": "line"}),
    );
    assert_eq!((err.code, pointer(&err)), (ErrorCode::InvalidParams, "/width"));

    engine.run_ok("pcb.keepout.delete", json!({"keepouts": [all.to_string()]}));
    assert!(pcb(&engine).keepout(all).is_none());
}

#[test]
fn a_keepout_is_stored_with_its_layer_shape_and_restrictions() {
    let dir = TempDir::new("keepout-file");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": square("0mm", "0mm", "3mm", "3mm")}),
    );
    engine.run_ok(
        "pcb.keepout.add",
        json!({"layer": "copper_top", "shape": "line", "width": "0.5mm",
               "vertices": [{"x": "5mm", "y": "0mm"}, {"x": "8mm", "y": "0mm"}],
               "restrict": ["footprints", "vias"]}),
    );
    engine.run_ok("project.save", json!({}));
    let text = std::fs::read_to_string(dir.path().join("Demo/Demo.redapcb")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let keepouts = file["keepouts"].as_array().unwrap();
    assert_eq!(keepouts.len(), 2);
    let polygon = keepouts.iter().find(|k| k["shape"] == "polygon").unwrap();
    assert!(polygon.get("layer").is_none(), "every Layer is stored as no Layer");
    assert!(polygon.get("width").is_none());
    assert_eq!(
        polygon["restrict"],
        json!(["tracks", "vias", "zones", "smd_pads", "tht_pads"])
    );
    let line = keepouts.iter().find(|k| k["shape"] == "line").unwrap();
    assert_eq!(line["width"], json!(500_000));
    assert_eq!(line["restrict"], json!(["vias", "footprints"]));
    assert_eq!(line["vertices"].as_array().unwrap().len(), 2);
    // One vertex per line, like a Zone's.
    assert!(text.contains("\"vertices\":[\n"));
}

#[test]
fn the_clearance_queries_report_the_keepouts_copper_would_enter() {
    let dir = TempDir::new("keepout-clearance");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"value": "0.3mm"}}),
    );
    let tracks = engine.run_ok(
        "pcb.keepout.add",
        json!({"layer": "copper_top", "vertices": square("0mm", "0mm", "2mm", "2mm"),
               "restrict": ["tracks"]}),
    )["result"]["uuid"]
        .clone();
    let vias = engine.run_ok(
        "pcb.keepout.add",
        json!({"layer": "copper_bottom", "vertices": square("10mm", "0mm", "12mm", "2mm"),
               "restrict": ["vias"]}),
    )["result"]["uuid"]
        .clone();
    // A 0.2 mm Track whose edge is `gap` from the Keepout's right side at x = 2 mm.
    let track = |e: &mut TestEngine, layer: &str, x: &str| {
        e.run_ok(
            "pcb.clearance.track",
            json!({"layer": layer, "width": "0.2mm",
                   "from": {"x": x, "y": "-1mm"}, "to": {"x": x, "y": "3mm"}}),
        )
    };
    let out = track(&mut engine, "copper_top", "1mm");
    assert_eq!(out["clear"], false);
    assert_eq!(out["keepouts"], json!([tracks]));
    let out = track(&mut engine, "copper_bottom", "1mm");
    assert_eq!(out["clear"], true, "the Keepout is on the top only");
    assert!(out.get("keepouts").is_none());
    let out = track(&mut engine, "copper_bottom", "11mm");
    assert_eq!(out["clear"], true, "Tracks may cross a Via Keepout");
    // Outside, but closer than the 0.3 mm clearance: edge 0.2 mm away.
    let out = track(&mut engine, "copper_top", "2.3mm");
    assert_eq!(out["keepouts"], json!([tracks]), "copper keeps the clearance");
    assert_eq!(track(&mut engine, "copper_top", "2.45mm")["clear"], true);

    let via = |e: &mut TestEngine, x: &str| {
        e.run_ok("pcb.clearance.via", json!({"at": {"x": x, "y": "1mm"}}))
    };
    assert_eq!(via(&mut engine, "11mm")["keepouts"], json!([vias]));
    assert_eq!(
        via(&mut engine, "1mm")["clear"],
        true,
        "a Keepout restricting Tracks lets a Via in"
    );
    assert_eq!(via(&mut engine, "6mm")["clear"], true);

    // Zero Keepout clearance makes the edge exact.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"zero_keepout_clearance": true}}),
    );
    assert_eq!(track(&mut engine, "copper_top", "2.3mm")["clear"], true);
    assert_eq!(
        track(&mut engine, "copper_top", "2.1mm")["clear"],
        true,
        "touching an edge-exact Keepout is allowed"
    );
    assert_eq!(track(&mut engine, "copper_top", "2.05mm")["clear"], false);
}

#[test]
fn zero_keepout_clearance_is_set_on_the_board_wide_clearance_record_only() {
    let dir = TempDir::new("keepout-zero-rule");
    let mut engine = engine_with_pcb(&dir);
    let out = engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"zero_keepout_clearance": true}}),
    );
    assert_eq!(out["result"]["rule"]["values"]["zero_keepout_clearance"], true);
    let class = engine.run_ok("pcb.net_class.add", json!({"name": "Power"}))["result"]["uuid"]
        .clone();
    let err = engine.run_err(
        "pcb.rules.set",
        json!({"kind": "clearance", "scope": {"net_class": class},
               "values": {"zero_keepout_clearance": true}}),
    );
    assert_eq!(
        (err.code, pointer(&err)),
        (ErrorCode::InvalidParams, "/values/zero_keepout_clearance")
    );
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
