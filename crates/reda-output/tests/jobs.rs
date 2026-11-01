//! Output Jobs on the Project stack, through Commands.

use reda_command::ErrorCode;
use reda_model::{OutputJob, Uuid};
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "fab_preset": "JLCPCB"}),
    );
    engine
}

fn jobs(engine: &TestEngine) -> Vec<OutputJob> {
    engine.project().unwrap().output_jobs().cloned().collect()
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

#[test]
fn a_job_can_be_added_edited_duplicated_and_deleted() {
    let dir = TempDir::new("jobs-crud");
    let mut engine = engine(&dir);
    let first = jobs(&engine)[0].clone();

    let added = engine.run_ok(
        "output.job.add",
        json!({"fab_preset": first.fab_preset, "outputs": {"bom": false, "cpl": false}}),
    );
    assert_eq!(added["result"]["name"], "JLCPCB 2");
    let second = uuid(&added["result"]["uuid"]);
    let job = engine
        .project()
        .unwrap()
        .output_job(second)
        .unwrap()
        .clone();
    assert!(job.outputs.gerber && job.outputs.drill);
    assert!(!job.outputs.bom && !job.outputs.cpl);
    assert_eq!(job.pcb, first.pcb);

    let edited = engine.run_ok(
        "output.job.set",
        json!({"job": second, "name": "JLCPCB bare", "destination": "fab/bare/"}),
    );
    assert_eq!(edited["result"]["destination"], "fab/bare");
    let job = engine
        .project()
        .unwrap()
        .output_job(second)
        .unwrap()
        .clone();
    assert_eq!(job.destination(), "fab/bare");
    engine.run_ok("output.job.set", json!({"job": second, "destination": ""}));
    let job = engine
        .project()
        .unwrap()
        .output_job(second)
        .unwrap()
        .clone();
    assert_eq!(job.destination(), "outputs/JLCPCB bare");

    let copy = engine.run_ok("output.job.duplicate", json!({"job": second}));
    assert_eq!(copy["result"]["name"], "JLCPCB bare copy");
    assert_eq!(jobs(&engine).len(), 3);

    engine.run_ok("output.job.delete", json!({"job": first.uuid}));
    let names: Vec<String> = jobs(&engine).into_iter().map(|j| j.name).collect();
    assert_eq!(names.len(), 2);
    assert!(!names.contains(&"JLCPCB".to_owned()), "{names:?}");
}

#[test]
fn job_edits_are_undone_on_the_project_stack() {
    let dir = TempDir::new("jobs-undo");
    let mut engine = engine(&dir);
    let job = jobs(&engine)[0].clone();
    let out = engine.run_ok("output.job.set", json!({"job": job.uuid, "name": "Fab"}));
    assert!(
        out["changes"]["project"]["modified"]
            .as_array()
            .unwrap()
            .contains(&json!(job.uuid))
    );
    assert!(engine.is_dirty());
    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(jobs(&engine)[0].name, "JLCPCB");
    assert!(!engine.is_dirty());
}

#[test]
fn bad_job_params_are_refused() {
    let dir = TempDir::new("jobs-bad");
    let mut engine = engine(&dir);
    let job = jobs(&engine)[0].clone();
    let missing = "00000000-0000-4000-8000-0000000000ff";

    let err = engine.run_err("output.job.add", json!({"fab_preset": missing}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err("output.job.set", json!({"job": missing, "name": "x"}));
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = engine.run_err(
        "output.job.add",
        json!({"fab_preset": job.fab_preset, "name": "jlcpcb"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    for destination in ["../out", "/abs", "a//b", "C:\\x"] {
        let err = engine.run_err(
            "output.job.set",
            json!({"job": job.uuid, "destination": destination}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{destination}");
    }
    let sheet = engine.project().unwrap().documents().next().unwrap().uuid;
    let err = engine.run_err("output.job.set", json!({"job": job.uuid, "pcb": sheet}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn a_job_can_be_pointed_at_another_preset_copy() {
    let dir = TempDir::new("jobs-preset");
    let mut engine = engine(&dir);
    let job = jobs(&engine)[0].clone();
    let added = engine.run_ok("fab_preset.add", json!({"name": "PCBWay"}));
    let pcbway = uuid(&added["result"]["uuid"]);
    engine.run_ok(
        "output.job.set",
        json!({"job": job.uuid, "fab_preset": pcbway}),
    );
    assert_eq!(jobs(&engine)[0].fab_preset, pcbway);
}
