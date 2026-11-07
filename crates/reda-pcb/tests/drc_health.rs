//! The health checks (`zero_length_track`, `micro_segment`, `duplicate_copper`,
//! `zero_area_zone`) and `drc.fix`, on a board whose file holds what only a hand edit, a
//! merge or an import leaves behind.

use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const PCB: &str = "00000000-0000-4000-8000-000000000003";

const HEALTH: [&str; 4] = [
    "zero_length_track",
    "micro_segment",
    "duplicate_copper",
    "zero_area_zone",
];

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-0000000f{n:04x}")
}

/// What the board file holds beside its first, ordinary Track, Via and Zone.
struct Board {
    engine: TestEngine,
    track: Uuid,
    via: Uuid,
    zero: Uuid,
    micro: Uuid,
    duplicate_track: Uuid,
    duplicate_via: Uuid,
    flat_zone: Uuid,
}

/// A saved board, its file edited to hold one finding of each health check, opened again.
fn board(dir: &TempDir) -> Board {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let net = uuid(&engine.run_ok("test.net.add", json!({"name": "A"}))["result"]["uuid"]);
    let track = uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "0.2mm", "net": net,
                   "points": [{"x": 0, "y": 0}, {"x": "5mm", "y": 0}]}),
        )["result"]["tracks"][0],
    );
    let via = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": "5mm", "y": 0}, "net": net}),
        )["result"]["uuid"],
    );
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_bottom", "net": net, "vertices": [
            {"x": "10mm", "y": 0}, {"x": "20mm", "y": 0}, {"x": "20mm", "y": "10mm"}]}),
    );
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let path = folder.join("Demo.redapcb");
    let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let tracks = file["tracks"].as_array_mut().unwrap();
    let extra = |n: u32, from: Value, to: Value| {
        let mut t = tracks[0].clone();
        t["uuid"] = json!(id(n));
        t["geometry"] = json!({"kind": "line", "from": from, "to": to});
        t
    };
    let zero = extra(
        1,
        json!({"x": 0, "y": 1_000_000}),
        json!({"x": 0, "y": 1_000_000}),
    );
    let micro = extra(
        2,
        json!({"x": 0, "y": 2_000_000}),
        json!({"x": 5_000, "y": 2_000_000}),
    );
    // The first Track again, drawn the other way.
    let duplicate = extra(3, json!({"x": 5_000_000, "y": 0}), json!({"x": 0, "y": 0}));
    tracks.extend([zero, micro, duplicate]);
    let mut via_copy = file["vias"][0].clone();
    via_copy["uuid"] = json!(id(4));
    file["vias"].as_array_mut().unwrap().push(via_copy);
    // Three corners in a line: no area.
    let mut flat = file["zones"][0].clone();
    flat["uuid"] = json!(id(5));
    flat["vertices"] = json!([{"x": 30_000_000, "y": 0}, {"x": 35_000_000, "y": 0},
                              {"x": 40_000_000, "y": 0}]);
    file["zones"].as_array_mut().unwrap().push(flat);
    std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap()).unwrap();
    let mut engine = probe_engine();
    engine.run_ok("project.open", json!({"path": folder}));
    let ids = |n| id(n).parse().unwrap();
    Board {
        engine,
        track,
        via,
        zero: ids(1),
        micro: ids(2),
        duplicate_track: ids(3),
        duplicate_via: ids(4),
        flat_zone: ids(5),
    }
}

fn of(run: &Value, check: &str) -> Vec<Value> {
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

fn objects(violation: &Value) -> Vec<Uuid> {
    violation["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(uuid)
        .collect()
}

fn found(engine: &mut TestEngine) -> Vec<(String, Vec<Uuid>)> {
    let run = engine.run_ok("drc.run", json!({"checks": HEALTH}));
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["check"].as_str().unwrap().to_owned(), objects(v)))
        .collect()
}

#[test]
fn the_health_checks_find_what_harms_outputs() {
    let dir = TempDir::new("drc-health-find");
    let mut b = board(&dir);
    let run = b.engine.run_ok("drc.run", json!({}));
    let zero = &of(&run, "zero_length_track");
    assert_eq!(zero.len(), 1, "{run:#}");
    assert_eq!(objects(&zero[0]), [b.zero]);
    assert_eq!(zero[0]["severity"], "warning");
    assert_eq!(zero[0]["at"], json!({"x": 0, "y": 1_000_000}));
    assert_eq!(zero[0]["layers"].as_array().unwrap().len(), 1);

    let micro = &of(&run, "micro_segment");
    assert_eq!(micro.len(), 1, "{run:#}");
    assert_eq!(objects(&micro[0]), [b.micro]);
    assert_eq!(micro[0]["measured"], json!({"length": 5_000}));
    assert_eq!(micro[0]["required"], json!({"length": 10_000}));
    assert_eq!(micro[0]["limit"], "min");
    let pcb = b.engine.project().unwrap().pcb().unwrap();
    let rule = pcb
        .rules()
        .find(|r| r.kind.id() == "micro_segment")
        .unwrap()
        .uuid;
    assert_eq!(micro[0]["rule"], rule.to_string());

    let duplicates: Vec<Vec<Uuid>> = of(&run, "duplicate_copper").iter().map(objects).collect();
    assert_eq!(
        duplicates,
        [
            vec![b.track, b.duplicate_track],
            vec![b.via, b.duplicate_via]
        ],
        "{run:#}"
    );
    let flat: Vec<Vec<Uuid>> = of(&run, "zero_area_zone").iter().map(objects).collect();
    assert_eq!(flat, [vec![b.flat_zone]], "{run:#}");

    // A run of the health checks alone lists the same, and nothing else.
    let health = b.engine.run_ok("drc.run", json!({"checks": HEALTH}));
    let all: Vec<&Value> = run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| HEALTH.contains(&v["check"].as_str().unwrap()))
        .collect();
    assert_eq!(
        health["violations"]
            .as_array()
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        all
    );
    assert_eq!(health["refilled"], false);

    // A longer micro-segment limit catches more; Off reports nothing.
    b.engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "micro_segment", "values": {"max_length": "0.001mm"}}),
    );
    assert_eq!(
        of(&b.engine.run_ok("drc.run", json!({})), "micro_segment").len(),
        0
    );
    b.engine.run_ok(
        "drc.set_severity",
        json!({"check": "duplicate_copper", "severity": "off"}),
    );
    assert_eq!(
        of(&b.engine.run_ok("drc.run", json!({})), "duplicate_copper").len(),
        0
    );
}

#[test]
fn drc_fix_removes_what_it_is_given_or_everything_as_one_undo_step() {
    let dir = TempDir::new("drc-health-fix");
    let mut b = board(&dir);
    let before = found(&mut b.engine);
    assert_eq!(before.len(), 5, "{before:?}");

    // Only the duplicate Via, though the check also finds a duplicate Track.
    let out = b.engine.run_ok(
        "drc.fix",
        json!({"check": "duplicate_copper", "objects": [b.duplicate_via]}),
    );
    assert_eq!(out["result"]["removed"], json!([b.duplicate_via]));
    let pcb = b.engine.project().unwrap().pcb().unwrap();
    assert!(pcb.via(b.duplicate_via).is_none());
    assert!(pcb.via(b.via).is_some(), "the copy kept stays");
    assert!(pcb.track(b.duplicate_track).is_some());

    // Everything one check finds, in one undo step.
    let out = b
        .engine
        .run_ok("drc.fix", json!({"check": "zero_length_track"}));
    assert_eq!(out["result"]["removed"], json!([b.zero]));
    b.engine
        .run_ok("drc.fix", json!({"check": "zero_area_zone"}));
    b.engine
        .run_ok("drc.fix", json!({"check": "micro_segment"}));
    b.engine
        .run_ok("drc.fix", json!({"check": "duplicate_copper"}));
    assert_eq!(found(&mut b.engine), []);
    let pcb = b.engine.project().unwrap().pcb().unwrap();
    assert!(pcb.track(b.track).is_some() && pcb.track(b.micro).is_none());
    assert!(pcb.zone(b.flat_zone).is_none());

    b.engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(
        found(&mut b.engine),
        [(
            "duplicate_copper".to_owned(),
            vec![b.track, b.duplicate_track]
        )]
    );
    for _ in 0..4 {
        b.engine.run_ok("edit.undo", json!({"document": PCB}));
    }
    assert_eq!(found(&mut b.engine), before);
}

#[test]
fn drc_fix_refuses_other_checks_and_objects_the_check_does_not_find() {
    let dir = TempDir::new("drc-health-refuse");
    let mut b = board(&dir);
    let err = b.engine.run_err("drc.fix", json!({"check": "clearance"}));
    assert!(
        err.message.contains("not a health check"),
        "{}",
        err.message
    );
    let err = b.engine.run_err(
        "drc.fix",
        json!({"check": "zero_length_track", "objects": [b.track]}),
    );
    assert!(
        err.message.contains("finds nothing to fix"),
        "{}",
        err.message
    );
    assert_eq!(found(&mut b.engine).len(), 5);
}
