//! Width changes as seen through the fill commands and the copper index.
use reda_model::Uuid;
use reda_model::geom::{Nm, Point};
use reda_pcb::{Copper, copper::Item};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn track(engine: &mut TestEngine, net: Uuid, width: i64, pts: &[(i64, i64)]) -> Vec<Uuid> {
    let points: Vec<_> = pts.iter().map(|(x,y)| json!({"x":x+10_000_000, "y":y+10_000_000})).collect();
    let v = engine.run_ok("pcb.track.add", json!({"layer":"copper_top", "net":net, "width":width, "points":points}));
    v["result"]["tracks"].as_array().unwrap().iter().map(uuid).collect()
}

fn board(dir: &TempDir) -> (TestEngine, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path":dir.path(),"name":"Board"}));
    let net = uuid(&engine.run_ok("test.net.add", json!({"name":"SIG"}))["result"]["uuid"]);
    (engine,net)
}

fn enable(engine: &mut TestEngine, shape: Value) {
    engine.run_ok("pcb.rules.set",json!({"kind":"teardrop","values":{"width_change":shape}}));
}

#[test]
fn width_change_makes_copper_belonging_to_the_narrow_track() {
    let dir = TempDir::new("teardrop-copper");
    let (mut engine, net) = board(&dir);
    track(&mut engine, net, 1_000_000, &[(-2_000_000,0),(0,0)]);
    let narrow = track(&mut engine, net, 100_000, &[(0,0),(3_000_000,0)])[0];
    // At 0.75 mm beyond the junction the wide Track's round cap no longer reaches.
    let point = Point::new(Nm(10_750_000),Nm(10_120_000));
    let pick = |engine: &TestEngine| {
        let project = engine.project().unwrap();
        let layer = project.pcb().unwrap().track(narrow).unwrap().layer;
        Copper::of(project).unwrap().items_at(layer, point)
    };
    assert!(pick(&engine).is_empty());
    enable(&mut engine, json!({"enabled":true,"length":100}));
    engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(pick(&engine), [Item::Track{uuid:narrow}]);
    assert_eq!(engine.run_ok("pcb.teardrop.status",json!({}))["generated"], 1);
}
