//! Locks on every placed kind (docs/spec/v0.1/pcb.md#locks): `pcb.lock {objects, locked}`,
//! geometry changes refused unless `include_locked`, other values still editable, and
//! `pcb.board.move` still moving locked objects.

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, TrackGeometry, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const MM: i64 = 1_000_000;

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn created(out: &Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn first(out: &Value, key: &str) -> Uuid {
    out["result"][key][0].as_str().unwrap().parse().unwrap()
}

fn poly(points: &[(i64, i64)]) -> Value {
    points
        .iter()
        .map(|&(x, y)| json!({"x": x * MM, "y": y * MM}))
        .collect()
}

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm(x * MM), Nm(y * MM))
}

/// One object of every lockable kind, all inside a 50 × 50 mm board.
struct Board {
    engine: TestEngine,
    outline: Uuid,
    cutout: Uuid,
    footprint: Uuid,
    track: Uuid,
    via: Uuid,
    zone: Uuid,
    keepout: Uuid,
    text: Uuid,
    graphic: Uuid,
    free_pad: Uuid,
    dimension: Uuid,
}

impl Board {
    fn new(dir: &TempDir) -> Board {
        let mut engine = probe_engine();
        engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
        let outline = created(&engine.run_ok(
            "pcb.board_outline.add",
            json!({"vertices": poly(&[(0, 0), (50, 0), (50, 50), (0, 50)])}),
        ));
        let cutout = created(&engine.run_ok(
            "pcb.board_cutout.add",
            json!({"vertices": poly(&[(20, 20), (25, 20), (25, 25), (20, 25)])}),
        ));
        let out = engine.run_ok(
            "test.footprint.add",
            json!({"at": {"x": 10 * MM, "y": 10 * MM}, "pads": [
                {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
        );
        let footprint = out["result"]["footprint"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let track = first(
            &engine.run_ok(
                "pcb.track.add",
                json!({"layer": "copper_top", "width": "0.25mm",
                       "points": poly(&[(30, 40), (40, 40)])}),
            ),
            "tracks",
        );
        let via = created(&engine.run_ok("pcb.via.add", json!({"at": {"x": "5mm", "y": "45mm"}})));
        let zone = created(&engine.run_ok(
            "pcb.zone.add",
            json!({"layer": "copper_top", "vertices": poly(&[(30, 5), (40, 5), (40, 15), (30, 15)])}),
        ));
        let keepout = created(&engine.run_ok(
            "pcb.keepout.add",
            json!({"shape": "polygon", "vertices": poly(&[(5, 30), (10, 30), (10, 35), (5, 35)])}),
        ));
        let text = created(&engine.run_ok(
            "pcb.text.add",
            json!({"layer": "silkscreen_top", "text": "REV A", "x": "2mm", "y": "2mm"}),
        ));
        let graphic = created(&engine.run_ok(
            "pcb.graphic.add_line",
            json!({"layer": "mechanical", "width": "0.2mm",
                   "from": {"x": "1mm", "y": "25mm"}, "to": {"x": "10mm", "y": "25mm"}}),
        ));
        let free_pad = created(&engine.run_ok(
            "pcb.free_pad.add",
            json!({"kind": "smd", "at": {"x": "15mm", "y": "35mm"}, "width": "1mm"}),
        ));
        let dimension = created(&engine.run_ok(
            "pcb.dimension.add",
            json!({"layer": "mechanical", "a": {"point": {"x": "5mm", "y": "48mm"}},
                   "b": {"point": {"x": "45mm", "y": "48mm"}}}),
        ));
        Board {
            engine,
            outline,
            cutout,
            footprint,
            track,
            via,
            zone,
            keepout,
            text,
            graphic,
            free_pad,
            dimension,
        }
    }

    fn all(&self) -> Vec<Uuid> {
        vec![
            self.outline,
            self.cutout,
            self.footprint,
            self.track,
            self.via,
            self.zone,
            self.keepout,
            self.text,
            self.graphic,
            self.free_pad,
            self.dimension,
        ]
    }

    fn lock(&mut self, objects: &[Uuid], locked: bool) {
        self.engine
            .run_ok("pcb.lock", json!({"objects": objects, "locked": locked}));
    }
}

/// The `locked` key of `object` as the project file would hold it.
fn stored_lock(pcb: &PcbDocument, object: Uuid) -> Option<Value> {
    let value = if let Some(o) = pcb.outline().filter(|o| o.uuid == object) {
        serde_json::to_value(o)
    } else if let Some(c) = pcb.cutout(object) {
        serde_json::to_value(c)
    } else if let Some(f) = pcb.footprint(object) {
        serde_json::to_value(f)
    } else if let Some(t) = pcb.track(object) {
        serde_json::to_value(t)
    } else if let Some(v) = pcb.via(object) {
        serde_json::to_value(v)
    } else if let Some(z) = pcb.zone(object) {
        serde_json::to_value(z)
    } else if let Some(k) = pcb.keepout(object) {
        serde_json::to_value(k)
    } else if let Some(t) = pcb.text(object) {
        serde_json::to_value(t)
    } else if let Some(g) = pcb.graphic(object) {
        serde_json::to_value(g)
    } else if let Some(p) = pcb.free_pad(object) {
        serde_json::to_value(p)
    } else {
        serde_json::to_value(pcb.dimension(object).unwrap())
    };
    value.unwrap().get("locked").cloned()
}

#[test]
fn every_placed_kind_locks_and_the_file_leaves_out_false() {
    let dir = TempDir::new("locks-kinds");
    let mut b = Board::new(&dir);
    let all = b.all();
    for o in &all {
        assert_eq!(stored_lock(pcb(&b.engine), *o), None, "{o}: false is left out");
    }

    b.lock(&all, true);
    for o in &all {
        assert!(reda_pcb::lock::is_locked(pcb(&b.engine), *o), "{o}");
        assert_eq!(stored_lock(pcb(&b.engine), *o), Some(json!(true)), "{o}");
    }

    b.lock(&all, false);
    for o in &all {
        assert!(!reda_pcb::lock::is_locked(pcb(&b.engine), *o), "{o}");
    }
}

#[test]
fn a_lock_survives_save_and_reopen() {
    let dir = TempDir::new("locks-roundtrip");
    let mut b = Board::new(&dir);
    let all = b.all();
    b.lock(&all, true);
    b.engine.run_ok("project.save", json!({}));
    let text = std::fs::read_to_string(dir.path().join("Demo/Demo.redapcb")).unwrap();
    assert!(text.contains("\"locked\":true"), "{text}");
    let mut reopened = probe_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    for o in &all {
        assert!(reda_pcb::lock::is_locked(pcb(&reopened), *o), "{o}");
    }
}

#[test]
fn pcb_lock_takes_only_placed_objects() {
    let dir = TempDir::new("locks-kinds-refused");
    let mut b = Board::new(&dir);
    let zone_cutout = created(&b.engine.run_ok(
        "pcb.zone_cutout.add",
        json!({"layer": "copper_top", "vertices": poly(&[(31, 6), (33, 6), (33, 8), (31, 8)])}),
    ));
    let err = b.engine.run_err(
        "pcb.lock",
        json!({"objects": [b.track, zone_cutout], "locked": true}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.as_ref().unwrap()["pointer"], "/objects/1", "{err:?}");
    assert!(!reda_pcb::lock::is_locked(pcb(&b.engine), b.track));

    let err = b.engine.run_err("pcb.lock", json!({"footprints": [b.footprint], "locked": true}));
    assert_eq!(err.code, ErrorCode::InvalidParams, "`footprints` is gone");
}

/// Every Command that changes a locked object's geometry, as (id, params).
fn blocked(b: &Board) -> Vec<(&'static str, Value)> {
    let by = json!({"x": "1mm", "y": 0});
    let square = poly(&[(30, 5), (41, 5), (41, 15), (30, 15)]);
    vec![
        ("pcb.move", json!({"objects": [b.footprint], "by": by})),
        ("pcb.move", json!({"objects": [b.free_pad], "by": by})),
        ("pcb.move", json!({"objects": [b.dimension], "by": by})),
        ("pcb.move", json!({"objects": [b.outline], "by": by})),
        ("pcb.move", json!({"objects": [b.cutout], "by": by})),
        ("pcb.rotate", json!({"objects": [b.footprint]})),
        ("pcb.flip", json!({"objects": [b.free_pad]})),
        (
            "pcb.align",
            json!({"objects": [b.footprint, b.free_pad], "to": "left"}),
        ),
        (
            "pcb.distribute",
            json!({"objects": [b.footprint, b.free_pad], "direction": "horizontal"}),
        ),
        (
            "pcb.arrange_in_rect",
            json!({"objects": [b.footprint], "from": {"x": 0, "y": 0},
                   "to": {"x": "40mm", "y": "40mm"}}),
        ),
        ("pcb.track.delete", json!({"tracks": [b.track]})),
        ("pcb.copper.delete", json!({"objects": [b.via]})),
        ("pcb.via.delete", json!({"vias": [b.via]})),
        ("pcb.zone.delete", json!({"zones": [b.zone]})),
        ("pcb.keepout.delete", json!({"keepouts": [b.keepout]})),
        ("pcb.text.delete", json!({"uuid": b.text})),
        ("pcb.graphic.delete", json!({"uuid": b.graphic})),
        ("pcb.free_pad.delete", json!({"pads": [b.free_pad]})),
        ("pcb.dimension.delete", json!({"uuid": b.dimension})),
        ("pcb.dimension.explode", json!({"uuid": b.dimension})),
        ("pcb.board_outline.delete", json!({})),
        ("pcb.board_cutout.delete", json!({"cutout": b.cutout})),
        (
            "pcb.track.set",
            json!({"tracks": [b.track], "end": {"x": "45mm", "y": "40mm"}}),
        ),
        ("pcb.track.set", json!({"tracks": [b.track], "length": "5mm"})),
        ("pcb.track.drag", json!({"track": b.track, "by": {"x": 0, "y": "1mm"}})),
        ("pcb.zone.set", json!({"zone": b.zone, "vertices": square})),
        (
            "pcb.keepout.set",
            json!({"keepout": b.keepout, "vertices": poly(&[(5, 30), (11, 30), (11, 35), (5, 35)])}),
        ),
        ("pcb.text.set", json!({"uuid": b.text, "x": "3mm", "y": "2mm"})),
        ("pcb.text.set", json!({"uuid": b.text, "rotation": "90deg"})),
        (
            "pcb.dimension.set",
            json!({"uuid": b.dimension, "a": {"point": {"x": "6mm", "y": "48mm"}}}),
        ),
        (
            "pcb.polygon.set_vertices",
            json!({"object": b.zone, "vertices": poly(&[(30, 5), (42, 5), (42, 15), (30, 15)])}),
        ),
        ("pcb.polygon.fillet", json!({"object": b.outline, "radius": "1mm"})),
        ("pcb.polygon.chamfer", json!({"object": b.cutout, "distance": "1mm"})),
    ]
}

#[test]
fn a_lock_refuses_every_geometry_change_unless_include_locked() {
    let dir = TempDir::new("locks-blocked");
    let mut b = Board::new(&dir);
    let all = b.all();
    b.lock(&all, true);
    for (id, params) in blocked(&b) {
        let before = b.engine.project().unwrap().clone();
        let err = b.engine.run_err(id, params.clone());
        assert_eq!(err.code, ErrorCode::Rejected, "{id} {params}: {err:?}");
        assert!(err.message.contains("locked"), "{id}: {}", err.message);
        let locked = &err.details.as_ref().expect("details")["locked"];
        assert!(
            locked.as_array().is_some_and(|l| !l.is_empty()),
            "{id}: {locked}"
        );
        assert!(
            *b.engine.project().unwrap() == before,
            "{id}: a refusal changes nothing"
        );
    }
    // Each one runs with `include_locked`, on a fresh board each time (deletes and
    // reshapes would get in each other's way).
    for (i, (id, _)) in blocked(&b).into_iter().enumerate() {
        let dir = TempDir::new(&format!("locks-included-{i}"));
        let mut fresh = Board::new(&dir);
        let all = fresh.all();
        fresh.lock(&all, true);
        let (_, mut params) = blocked(&fresh).swap_remove(i);
        params["include_locked"] = json!(true);
        fresh.engine.run_ok(id, params);
    }
}

#[test]
fn a_lock_still_allows_other_values() {
    let dir = TempDir::new("locks-allowed");
    let mut b = Board::new(&dir);
    let all = b.all();
    b.lock(&all, true);
    let net = created(&b.engine.run_ok("test.net.add", json!({"name": "GND"})));
    for (id, params) in [
        ("pcb.track.set", json!({"tracks": [b.track], "width": "0.5mm", "net": net})),
        ("pcb.zone.set", json!({"zone": b.zone, "net": net, "name": "GND_TOP"})),
        ("pcb.zone.fill", json!({})),
        ("pcb.keepout.set", json!({"keepout": b.keepout, "restrict": ["vias"]})),
        ("pcb.text.set", json!({"uuid": b.text, "text": "REV B", "height": "2mm"})),
        ("pcb.free_pad.set_net", json!({"pads": [b.free_pad], "net": net})),
        ("pcb.dimension.set", json!({"uuid": b.dimension, "precision": 1})),
        (
            "pcb.designator_text.set",
            json!({"footprint": b.footprint, "at": {"x": "12mm", "y": "12mm"}}),
        ),
    ] {
        b.engine.run_ok(id, params);
    }
    let pcb = pcb(&b.engine);
    assert_eq!(pcb.track(b.track).unwrap().net, Some(net));
    assert!(pcb.track(b.track).unwrap().locked, "the lock stays");
}

#[test]
fn the_board_moves_with_its_locked_objects_which_stay_locked() {
    let dir = TempDir::new("locks-board-move");
    let mut b = Board::new(&dir);
    let all = b.all();
    b.lock(&all, true);
    b.engine
        .run_ok("pcb.board.move", json!({"dx": "10mm", "dy": 0}));
    let pcb = pcb(&b.engine);
    assert_eq!(pcb.footprint(b.footprint).unwrap().at, mm(20, 10));
    assert_eq!(
        pcb.track(b.track).unwrap().geometry,
        TrackGeometry::Line {
            from: mm(40, 40),
            to: mm(50, 40)
        }
    );
    assert_eq!(pcb.via(b.via).unwrap().at, mm(15, 45));
    assert_eq!(pcb.free_pad(b.free_pad).unwrap().at, mm(25, 35));
    for o in &all {
        assert!(reda_pcb::lock::is_locked(pcb, *o), "{o} stays locked");
    }
}
