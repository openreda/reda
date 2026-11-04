//! `pcb.zone.fill`, the `.reda/` fill cache, fill staleness and fills in the Ratsnest.

use std::sync::{Arc, Mutex};

use reda_command::{CancelToken, Progress, Signals};
use reda_engine::model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn mm(v: i64) -> i64 {
    v * 1_000_000
}

fn rect(x0: i64, y0: i64, x1: i64, y1: i64) -> Value {
    let p = |x: i64, y: i64| json!({"x": mm(x), "y": mm(y)});
    json!([p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)])
}

/// A board with two GND Pads under a GND Zone, and the Zone's UUID.
fn board(dir: &TempDir) -> (TestEngine, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let gnd = engine.run_ok("test.net.add", json!({"name": "GND"}))["result"]["uuid"].clone();
    for (x, y) in [(2, 2), (8, 8)] {
        engine.run_ok(
            "test.footprint.add",
            json!({"at": {"x": mm(x), "y": mm(y)},
                   "pads": [{"at": {"x": 0, "y": 0}, "width": mm(1), "height": mm(1),
                             "net": gnd}]}),
        );
    }
    let zone = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd, "vertices": rect(0, 0, 10, 10)}),
    );
    (engine, uuid(&zone["result"]["uuid"]))
}

fn status(engine: &mut TestEngine) -> Value {
    engine.run_ok("pcb.zone.fill_status", json!({}))
}

fn cache_file(engine: &TestEngine) -> std::path::PathBuf {
    let pcb = engine.project().unwrap().pcb().unwrap().uuid;
    engine
        .location()
        .unwrap()
        .join(".reda")
        .join(format!("{pcb}.zonefill"))
}

#[test]
fn filling_stores_the_fills_and_writes_the_cache() {
    let dir = TempDir::new("zone-fill-cmd");
    let (mut engine, zone) = board(&dir);
    let st = status(&mut engine);
    assert_eq!(st["needs_refill"], true);
    assert_eq!(st["zones"][0]["state"], "unfilled");

    engine.run_ok("project.save", json!({}));
    let out = engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(out["cached"], true);
    assert_eq!(out["zones"][0]["zone"], zone.to_string());
    assert_eq!(out["zones"][0]["islands"], 1);
    assert!(!engine.is_dirty(), "filling is not an edit");
    let st = status(&mut engine);
    assert_eq!(st["needs_refill"], false);
    assert_eq!(st["zones"][0]["state"], "filled");
    let bytes = std::fs::read(cache_file(&engine)).unwrap();
    let cached = reda_pcb::zone::decode(&bytes).unwrap();
    let pcb = engine.project().unwrap().pcb().unwrap();
    assert_eq!(
        Some(&cached),
        engine.project().unwrap().zone_fills(pcb.uuid)
    );

    let fills = engine.run_ok("pcb.zone.fills", json!({}));
    assert_eq!(fills["zones"][0]["zone"], zone.to_string());
    assert_eq!(fills["zones"][0]["state"], "filled");
    assert_eq!(fills["zones"][0]["shapes"].as_array().unwrap().len(), 1);
}

#[test]
fn an_edit_makes_the_fill_stale_and_undo_makes_it_current_again() {
    let dir = TempDir::new("zone-fill-stale");
    let (mut engine, _) = board(&dir);
    engine.run_ok("pcb.zone.fill", json!({}));
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "points": [{"x": mm(20), "y": 0}, {"x": mm(30), "y": 0}]}),
    );
    assert!(out["result"]["tracks"].is_array());
    let st = status(&mut engine);
    assert_eq!(st["needs_refill"], true);
    assert_eq!(st["zones"][0]["state"], "stale");
    let doc = engine.project().unwrap().pcb().unwrap().uuid;
    engine.run_ok("edit.undo", json!({"document": doc.to_string()}));
    assert_eq!(status(&mut engine)["needs_refill"], false);

    // Free text does not touch the fill.
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "X", "x": 0, "y": 0}),
    );
    assert_eq!(status(&mut engine)["needs_refill"], false);
}

#[test]
fn a_reopened_project_reads_its_fills_from_the_cache() {
    let dir = TempDir::new("zone-fill-reopen");
    let (mut engine, _) = board(&dir);
    engine.run_ok("pcb.zone.fill", json!({}));
    engine.run_ok("project.save", json!({}));
    let folder = engine.location().unwrap().to_owned();
    let fills = engine.project().unwrap().pcb().map(|p| p.uuid).unwrap();
    let fills = engine.project().unwrap().zone_fills(fills).cloned();

    let mut other = probe_engine();
    other.run_ok("project.open", json!({"path": folder}));
    let doc = other.project().unwrap().pcb().unwrap().uuid;
    assert_eq!(other.project().unwrap().zone_fills(doc).cloned(), fills);
    assert_eq!(status(&mut other)["needs_refill"], false);

    // A cache that does not decode is no cache.
    std::fs::write(cache_file(&other), b"junk").unwrap();
    let mut third = probe_engine();
    third.run_ok("project.open", json!({"path": folder}));
    assert_eq!(status(&mut third)["zones"][0]["state"], "unfilled");
}

#[test]
fn a_filled_zone_joins_its_pads_in_the_ratsnest() {
    let dir = TempDir::new("zone-fill-ratsnest");
    let (mut engine, zone) = board(&dir);
    let lines = |e: &mut TestEngine| e.run_ok("pcb.ratsnest", json!({}))["lines"].clone();
    assert_eq!(lines(&mut engine).as_array().unwrap().len(), 1);
    engine.run_ok("pcb.zone.fill", json!({}));
    assert_eq!(lines(&mut engine), json!([]));
    let footprint = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .footprints()
        .next()
        .unwrap()
        .uuid;
    let connected = engine.run_ok(
        "pcb.copper.connected",
        json!({"object": footprint.to_string()}),
    );
    assert!(
        connected["items"]
            .as_array()
            .unwrap()
            .contains(&json!({"kind": "zone", "zone": zone.to_string(), "island": 0}))
    );
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(f64, Option<f64>)>>);

impl Progress for Recorder {
    fn report(&self, progress: f64, total: Option<f64>, _: &str) {
        self.0.lock().unwrap().push((progress, total));
    }
}

#[test]
fn filling_reports_progress_and_a_cancelled_fill_changes_nothing() {
    let dir = TempDir::new("zone-fill-cancel");
    let (mut engine, _) = board(&dir);
    let recorder = Arc::new(Recorder::default());
    let signals = Signals::new(CancelToken::new()).with_progress(recorder.clone());
    engine
        .run_with("pcb.zone.fill", json!({}), &signals)
        .unwrap();
    let seen = recorder.0.lock().unwrap().clone();
    assert_eq!(seen.first(), Some(&(0.0, Some(1.0))));
    assert_eq!(seen.last(), Some(&(1.0, Some(1.0))));

    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_bottom", "vertices": rect(0, 0, 5, 5)}),
    );
    let before = engine.project().unwrap().clone();
    let cancel = CancelToken::new();
    cancel.cancel();
    let err = engine
        .run_with("pcb.zone.fill", json!({}), &Signals::new(cancel))
        .unwrap_err();
    assert!(err.is_cancelled());
    assert_eq!(engine.project().unwrap(), &before);
}
