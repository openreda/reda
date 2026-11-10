//! Net settings, Track values and Net reassignment, through Commands: `pcb.net.set_color`,
//! `pcb.net.set_routing`, `pcb.track.set` and `pcb.copper.update_nets`.

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, TrackGeometry, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const PCB: &str = "00000000-0000-4000-8000-000000000003";

fn engine(dir: &TempDir) -> TestEngine {
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

fn uuids(value: &Value) -> Vec<Uuid> {
    value.as_array().unwrap().iter().map(uuid).collect()
}

fn add_net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

/// A route on `layer` through `points` (in mm), on `net` (none for `Value::Null`).
fn route(engine: &mut TestEngine, layer: &str, net: Value, points: &[(f64, f64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let mut params = json!({"layer": layer, "points": points});
    if !net.is_null() {
        params["net"] = net;
    }
    uuids(&engine.run_ok("pcb.track.add", params)["result"]["tracks"])
}

/// A one-Pad Footprint at `(x, y)` mm with its SMD Pad on `net`; returns (footprint, pad).
fn pad(engine: &mut TestEngine, x: f64, y: f64, net: Uuid) -> (Uuid, Uuid) {
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")},
               "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm",
                         "net": net.to_string()}]}),
    )["result"]
        .clone();
    (uuid(&out["footprint"]), uuid(&out["pads"][0]))
}

fn saved_board(engine: &mut TestEngine, dir: &TempDir) -> String {
    engine.run_ok("project.save", json!({}));
    std::fs::read_to_string(dir.path().join("Demo/Demo.redapcb")).unwrap()
}

#[test]
fn a_net_colour_is_set_undone_and_kept_in_the_board_file() {
    let dir = TempDir::new("net-color");
    let mut engine = engine(&dir);
    let gnd = add_net(&mut engine, "GND");
    let vcc = add_net(&mut engine, "VCC");
    assert!(
        !saved_board(&mut engine, &dir).contains("\"color\""),
        "left out when unset"
    );

    let out = engine.run_ok(
        "pcb.net.set_color",
        json!({"nets": [gnd, vcc], "color": "#FF8800"}),
    );
    assert_eq!(uuids(&out["result"]["nets"]).len(), 2);
    assert_eq!(
        pcb(&engine).net(gnd).unwrap().color.as_deref(),
        Some("#ff8800")
    );
    assert_eq!(
        pcb(&engine).net(vcc).unwrap().color.as_deref(),
        Some("#ff8800")
    );
    assert!(saved_board(&mut engine, &dir).contains("\"color\":\"#ff8800\""));

    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(pcb(&engine).net(gnd).unwrap().color, None, "undo clears it");
    engine.run_ok("edit.redo", json!({"document": PCB}));
    engine.run_ok("pcb.net.set_color", json!({"nets": [gnd]}));
    assert_eq!(
        pcb(&engine).net(gnd).unwrap().color,
        None,
        "no colour unsets it"
    );
    assert_eq!(
        pcb(&engine).net(vcc).unwrap().color.as_deref(),
        Some("#ff8800")
    );

    let err = engine.run_err(
        "pcb.net.set_color",
        json!({"nets": [gnd], "color": "orange"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err(
        "pcb.net.set_color",
        json!({"nets": [Uuid::from_u128(7)], "color": "#000000"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn loop_removal_is_on_by_default_and_stored_only_when_off() {
    let dir = TempDir::new("net-routing");
    let mut engine = engine(&dir);
    let gnd = add_net(&mut engine, "GND");
    assert!(pcb(&engine).net(gnd).unwrap().loop_removal);
    assert!(!saved_board(&mut engine, &dir).contains("loop_removal"));

    engine.run_ok(
        "pcb.net.set_routing",
        json!({"nets": [gnd], "loop_removal": false}),
    );
    assert!(!pcb(&engine).net(gnd).unwrap().loop_removal);
    let text = saved_board(&mut engine, &dir);
    assert!(text.contains("\"loop_removal\":false"), "{text}");

    let mut reopened = reda_testkit::test_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert!(!pcb(&reopened).net(gnd).unwrap().loop_removal);

    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert!(pcb(&engine).net(gnd).unwrap().loop_removal);
}

fn geometry(engine: &TestEngine, track: Uuid) -> TrackGeometry {
    pcb(engine).track(track).unwrap().geometry
}

#[test]
fn track_set_moves_ends_keeps_start_for_a_new_length_and_sets_width_layer_and_net() {
    let dir = TempDir::new("track-set");
    let mut engine = engine(&dir);
    let sig = add_net(&mut engine, "SIG");
    let t = route(
        &mut engine,
        "copper_top",
        Value::Null,
        &[(0.0, 0.0), (3.0, 4.0)],
    )[0];

    engine.run_ok(
        "pcb.track.set",
        json!({"tracks": [t], "start": {"x": "1mm", "y": "0mm"}, "end": {"x": "4mm", "y": "4mm"}}),
    );
    assert_eq!(
        geometry(&engine, t),
        TrackGeometry::Line {
            from: mm(1.0, 0.0),
            to: mm(4.0, 4.0)
        }
    );

    // Length 10 mm keeps Start and moves End along the line (a 3-4-5 direction).
    engine.run_ok("pcb.track.set", json!({"tracks": [t], "length": "10mm"}));
    assert_eq!(
        geometry(&engine, t),
        TrackGeometry::Line {
            from: mm(1.0, 0.0),
            to: mm(7.0, 8.0)
        }
    );

    let out = engine.run_ok(
        "pcb.track.set",
        json!({"tracks": [t], "width": "0.4mm", "layer": "copper_bottom", "net": sig}),
    );
    assert_eq!(uuids(&out["result"]["tracks"]), [t]);
    let track = pcb(&engine).track(t).unwrap().clone();
    assert_eq!(track.width, Nm::from_um(400));
    assert_eq!(track.net, Some(sig));
    let bottom = pcb(&engine)
        .copper_layers()
        .find(|l| l.side == Some(reda_model::Side::Bottom))
        .unwrap()
        .uuid;
    assert_eq!(track.layer, bottom);

    engine.run_ok("pcb.track.set", json!({"tracks": [t], "net": null}));
    assert_eq!(
        pcb(&engine).track(t).unwrap().net,
        None,
        "null takes it off its Net"
    );
    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(pcb(&engine).track(t).unwrap().net, Some(sig));
}

#[test]
fn track_set_refuses_what_makes_no_track() {
    let dir = TempDir::new("track-set-refuse");
    let mut engine = engine(&dir);
    let t = route(
        &mut engine,
        "copper_top",
        Value::Null,
        &[(0.0, 0.0), (3.0, 0.0)],
    )[0];
    let cases = [
        (json!({"tracks": [t], "length": "0mm"}), "/length"),
        (json!({"tracks": [t], "width": "0mm"}), "/width"),
        (json!({"tracks": [t], "end": {"x": 0, "y": 0}}), "/end"),
        (
            json!({"tracks": [t], "length": "2mm", "end": {"x": "1mm", "y": 0}}),
            "/length",
        ),
        (json!({"tracks": [t], "layer": "silkscreen_top"}), "/layer"),
        (json!({"tracks": [t], "net": Uuid::from_u128(9)}), "/net"),
        (json!({"tracks": [Uuid::from_u128(9)]}), "/tracks/0"),
    ];
    for (params, pointer) in cases {
        let err = engine.run_err("pcb.track.set", params.clone());
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
        assert_eq!(
            err.details.as_ref().unwrap()["pointer"],
            pointer,
            "{params}"
        );
    }
    assert_eq!(
        geometry(&engine, t),
        TrackGeometry::Line {
            from: mm(0.0, 0.0),
            to: mm(3.0, 0.0)
        }
    );
}

#[test]
fn update_nets_gives_copper_the_net_of_the_pads_it_touches_and_reports_shorts() {
    let dir = TempDir::new("update-nets");
    let mut engine = engine(&dir);
    let gnd = add_net(&mut engine, "GND");
    let vcc = add_net(&mut engine, "VCC");
    let sig = add_net(&mut engine, "SIG");
    pad(&mut engine, 0.0, 0.0, gnd);
    pad(&mut engine, 0.0, 10.0, gnd);
    pad(&mut engine, 20.0, 0.0, vcc);
    pad(&mut engine, 30.0, 0.0, sig);
    // On the wrong Net, from a GND Pad through a Via to the bottom copper.
    let top = route(
        &mut engine,
        "copper_top",
        json!(sig),
        &[(0.0, 0.0), (5.0, 0.0)],
    );
    let via = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": "5mm", "y": "0mm"}, "net": sig}),
        )["result"]["uuid"],
    );
    let bottom = route(
        &mut engine,
        "copper_bottom",
        json!(sig),
        &[(5.0, 0.0), (5.0, 5.0)],
    );
    // On the wrong Net, from the other GND Pad to nothing.
    let wrong = route(
        &mut engine,
        "copper_top",
        json!(sig),
        &[(0.0, 10.0), (3.0, 10.0)],
    );
    // Joining VCC and SIG: a short, left as it is (on VCC, where it starts).
    let short = route(
        &mut engine,
        "copper_top",
        Value::Null,
        &[(20.0, 0.0), (30.0, 0.0)],
    );
    // Touching no Pad.
    let lone = route(
        &mut engine,
        "copper_top",
        json!(sig),
        &[(40.0, 0.0), (45.0, 0.0)],
    );

    let out = engine.run_ok("pcb.copper.update_nets", json!({"nets": [vcc]}))["result"].clone();
    assert_eq!(uuids(&out["changed"]), Vec::<Uuid>::new(), "{out}");
    assert_eq!(out["conflicts"].as_array().unwrap().len(), 1, "{out}");

    let out = engine.run_ok("pcb.copper.update_nets", json!({}))["result"].clone();
    let mut expected = vec![top[0], via, bottom[0], wrong[0]];
    expected.sort();
    assert_eq!(uuids(&out["changed"]), expected);
    let pcb = pcb(&engine);
    for t in [top[0], bottom[0], wrong[0]] {
        assert_eq!(pcb.track(t).unwrap().net, Some(gnd));
    }
    assert_eq!(pcb.via(via).unwrap().net, Some(gnd));
    assert_eq!(
        pcb.track(short[0]).unwrap().net,
        Some(vcc),
        "touches two Nets"
    );
    assert_eq!(pcb.track(lone[0]).unwrap().net, Some(sig), "touches no Pad");
    let conflicts = out["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{out}");
    assert_eq!(uuids(&conflicts[0]["objects"]), short);
    let mut nets = vec![vcc, sig];
    nets.sort();
    assert_eq!(uuids(&conflicts[0]["nets"]), nets);

    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(pcb_net(&engine, wrong[0]), Some(sig), "one undo step");
}

fn pcb_net(engine: &TestEngine, track: Uuid) -> Option<Uuid> {
    pcb(engine).track(track).unwrap().net
}
