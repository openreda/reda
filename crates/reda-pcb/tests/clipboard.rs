//! The board clipboard: `pcb.copy` and `pcb.paste`, and Paste Special's options (keep Net
//! names, paste on the active Layer).

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{LayerKind, PcbDocument, Side, TrackGeometry, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

fn layer(engine: &TestEngine, kind: LayerKind, side: Side) -> Uuid {
    pcb(engine).layers().find(kind, Some(side)).unwrap().uuid
}

struct Board {
    engine: TestEngine,
    net: Uuid,
    track: Uuid,
    via: Uuid,
    text: Uuid,
    _dir: TempDir,
}

fn board(label: &str) -> Board {
    let dir = TempDir::new(label);
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let net = uuid(&engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    let track = uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "0.25mm", "net": net,
                   "points": [{"x": "2mm", "y": "1mm"}, {"x": "6mm", "y": "1mm"}]}),
        )["result"]["tracks"][0],
    );
    let via = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": "6mm", "y": "1mm"}, "net": net}),
        )["result"]["uuid"],
    );
    let text = uuid(
        &engine.run_ok(
            "pcb.text.add",
            json!({"layer": "silkscreen_top", "text": "A", "x": "2mm", "y": "3mm"}),
        )["result"]["uuid"],
    );
    Board {
        engine,
        net,
        track,
        via,
        text,
        _dir: dir,
    }
}

fn copy(engine: &mut TestEngine, objects: Vec<Uuid>) -> Value {
    engine.run_ok("pcb.copy", json!({"objects": objects}))
}

fn pasted(out: &Value) -> Vec<Uuid> {
    out["result"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(uuid)
        .collect()
}

#[test]
fn paste_puts_new_copies_at_the_point_on_no_net() {
    let mut b = board("clipboard-paste");
    let clip = copy(&mut b.engine, vec![b.track, b.via]);
    // The extent's lower-left corner is (2, 1) mm: it goes to (12, 11).
    let out = b.engine.run_ok(
        "pcb.paste",
        json!({"clipboard": clip, "at": {"x": "12mm", "y": "11mm"}}),
    );
    let new = pasted(&out);
    assert_eq!(new.len(), 2);
    let pcb = pcb(&b.engine);
    let t = pcb.track(new[0]).unwrap();
    assert_ne!(t.uuid, b.track);
    assert_eq!(
        t.geometry,
        TrackGeometry::Line {
            from: mm(12, 11),
            to: mm(16, 11)
        }
    );
    assert_eq!(t.net, None, "a plain paste drops the Net");
    assert_eq!(t.layer, layer(&b.engine, LayerKind::Copper, Side::Top));
    assert_eq!(pcb.via(new[1]).unwrap().at, mm(16, 11));
    assert_eq!(
        pcb.track(b.track).unwrap().geometry.ends().0,
        mm(2, 1),
        "the original stays"
    );
}

#[test]
fn paste_special_keeps_net_names_and_goes_on_the_active_layer() {
    let mut b = board("clipboard-special");
    b.engine
        .run_ok("pcb.lock", json!({"objects": [b.track], "locked": true}));
    let clip = copy(&mut b.engine, vec![b.track, b.via]);
    let out = b.engine.run_ok(
        "pcb.paste",
        json!({"clipboard": clip, "at": {"x": "2mm", "y": "10mm"},
               "keep_nets": true, "layer": "copper_bottom"}),
    );
    let new = pasted(&out);
    let t = pcb(&b.engine).track(new[0]).unwrap();
    assert_eq!(t.net, Some(b.net), "the Net is found again by name");
    assert_eq!(t.layer, layer(&b.engine, LayerKind::Copper, Side::Bottom));
    assert!(!t.locked, "a paste starts unlocked");
    assert_eq!(pcb(&b.engine).via(new[1]).unwrap().net, Some(b.net));

    // Text goes on a drawing Layer, mirrored on the bottom.
    let clip = copy(&mut b.engine, vec![b.text]);
    let out = b.engine.run_ok(
        "pcb.paste",
        json!({"clipboard": clip, "layer": "silkscreen_bottom"}),
    );
    let t = pcb(&b.engine).text(pasted(&out)[0]).unwrap();
    assert_eq!(
        t.layer,
        layer(&b.engine, LayerKind::Silkscreen, Side::Bottom)
    );
    assert!(t.mirrored);

    // Copper can't go on silkscreen.
    let clip = copy(&mut b.engine, vec![b.track]);
    let err = b.engine.run_err(
        "pcb.paste",
        json!({"clipboard": clip, "layer": "silkscreen_top"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn keep_nets_finds_no_net_of_a_name_the_board_lacks() {
    let mut b = board("clipboard-net-gone");
    let mut clip = copy(&mut b.engine, vec![b.track]);
    clip["nets"][b.net.to_string()] = json!("ELSEWHERE");
    let out = b
        .engine
        .run_ok("pcb.paste", json!({"clipboard": clip, "keep_nets": true}));
    assert_eq!(pcb(&b.engine).track(pasted(&out)[0]).unwrap().net, None);
}

#[test]
fn footprints_are_not_copied() {
    let mut b = board("clipboard-footprint");
    let out = b.engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
    );
    let fp = uuid(&out["result"]["footprint"]);
    let err = b
        .engine
        .run_err("pcb.copy", json!({"objects": [b.track, fp]}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert!(err.message.contains("Sync"), "{}", err.message);
}
