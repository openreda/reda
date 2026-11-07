//! Detached runs: a long-running Command split off the engine onto a copy of the session,
//! so a front end can run it on another thread and keep drawing, then hand the outcome
//! back to be journalled.

use std::sync::{Arc, Mutex};

use reda_command::{CancelToken, ErrorCode, Progress, Signals};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::json;

/// The Output Job `project.new` makes, on seed 0.
const JOB: &str = "00000000-0000-4000-8000-000000000021";

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine
}

#[derive(Default)]
struct Recorder(Mutex<Vec<f64>>);

impl Progress for Recorder {
    fn report(&self, progress: f64, _total: Option<f64>, _message: &str) {
        self.0.lock().unwrap().push(progress);
    }
}

#[test]
fn a_detached_job_run_writes_its_files_on_another_thread_and_is_journalled() {
    let dir = TempDir::new("detached-run");
    let mut engine = board(&dir);
    let before = engine.journal().len();
    let detached = engine
        .detach("output.job.run", json!({"job": JOB}))
        .unwrap();
    let recorder = Arc::new(Recorder::default());
    let signals = Signals::new(CancelToken::new()).with_progress(recorder.clone());
    let done = std::thread::spawn(move || detached.run(&signals))
        .join()
        .unwrap();
    assert_eq!(
        engine.journal().len(),
        before,
        "journalled only when finished"
    );

    let out = engine.finish(done).unwrap();
    let zip = std::path::Path::new(out["zip"].as_str().unwrap());
    assert!(zip.starts_with(dir.path()) && zip.exists(), "{zip:?}");
    assert!(!out["files"].as_array().unwrap().is_empty());
    let last = engine.journal().last().unwrap();
    assert_eq!(last.id, "output.job.run");
    assert_eq!(last.outcome.as_ref().unwrap(), &out);
    assert!(recorder.0.lock().unwrap().len() > 2);
}

#[test]
fn the_zone_fills_a_detached_job_run_computes_are_kept() {
    let dir = TempDir::new("detached-fills");
    let mut engine = board(&dir);
    let gnd = engine.run_ok("test.net.add", json!({"name": "GND"}))["result"]["uuid"].clone();
    let p = |x: i64, y: i64| json!({"x": x * 1_000_000, "y": y * 1_000_000});
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd,
               "vertices": [p(2, 2), p(12, 2), p(12, 12), p(2, 12)]}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap().uuid;
    assert!(engine.project().unwrap().zone_fills(pcb).is_none());

    let detached = engine
        .detach("output.job.run", json!({"job": JOB}))
        .unwrap();
    let done = std::thread::spawn(move || detached.run(&Signals::default()))
        .join()
        .unwrap();
    engine.finish(done).unwrap();
    assert!(
        engine.project().unwrap().zone_fills(pcb).is_some(),
        "the run's fills are stored back, as when it runs on the engine"
    );
    let status = engine.run_ok("pcb.zone.fill_status", json!({}));
    assert_eq!(status["zones"][0]["state"], "filled", "{status}");
}

#[test]
fn a_cancelled_detached_run_is_journalled_as_cancelled() {
    let dir = TempDir::new("detached-cancel");
    let mut engine = board(&dir);
    let detached = engine
        .detach("output.job.run", json!({"job": JOB}))
        .unwrap();
    let token = CancelToken::new();
    token.cancel();
    let err = engine
        .finish(detached.run(&Signals::new(token)))
        .unwrap_err();
    assert!(err.is_cancelled());
    assert!(engine.journal().last().unwrap().outcome.is_err());
    assert!(!dir.path().join("Demo/outputs").exists());
}

#[test]
fn only_long_running_file_commands_can_be_detached() {
    let dir = TempDir::new("detached-refused");
    let engine = board(&dir);
    let err = engine
        .detach("project.properties.set", json!({"title": "X"}))
        .err()
        .unwrap();
    assert_eq!(err.code, ErrorCode::Rejected);

    let err = engine
        .detach("output.job.run", json!({"job": 3}))
        .err()
        .unwrap();
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn nothing_is_detached_without_a_project() {
    let engine = probe_engine();
    let err = engine
        .detach("output.job.run", json!({"job": JOB}))
        .err()
        .unwrap();
    assert_eq!(err.code, ErrorCode::Unavailable);
}
