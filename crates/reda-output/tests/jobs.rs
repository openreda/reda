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

#[test]
fn build_choices_are_editable_saved_and_reserved_keys_are_rejected() {
    let dir = TempDir::new("jobs-build-choices");
    let mut engine = engine(&dir);
    let job = jobs(&engine)[0].clone();
    let defaults = serde_json::to_value(&job).unwrap();
    assert_eq!(defaults["name_template"], "${project}-${job}");
    assert_eq!(defaults["not_fitted_paste"], "keep");
    assert_eq!(defaults["schematic_pdf_colour"], "colour");
    assert_eq!(
        defaults["outputs"],
        json!({"gerber":true,"drill":true,"bom":true,"cpl":true,
        "fab_notes":true,"drill_report":true,"gerber_job":true,"reports":true,"schematic_pdf":true})
    );
    let changed = engine.run_ok(
        "output.job.set",
        json!({
            "job": job.uuid, "name_template": "${project}-${revision}-${date}",
            "not_fitted_paste": "remove", "schematic_pdf_colour": "mono",
            "outputs": {"fab_notes": false, "drill_report": false, "gerber_job": false,
                        "reports": false, "schematic_pdf": false}
        }),
    );
    assert_eq!(
        changed["result"]["name_template"],
        "${project}-${revision}-${date}"
    );
    assert_eq!(changed["result"]["not_fitted_paste"], "remove");
    assert_eq!(changed["result"]["outputs"]["schematic_pdf"], false);
    engine.run_ok("project.save", json!({}));
    let mut reopened = reda_testkit::test_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(jobs(&reopened), jobs(&engine));
    for key in ["checks", "overrides", "variant"] {
        let err = engine.run_err("output.job.set", json!({"job": job.uuid, key: {}}));
        assert_eq!(err.code, ErrorCode::InvalidParams);
    }
}

/// Saves and closes the Project, rewrites its one saved job line with `edit`, and returns
/// the message `project.open` refuses the edited file with.
fn load_error_after(
    dir: &TempDir,
    engine: &mut TestEngine,
    edit: impl Fn(&str) -> String,
) -> String {
    engine.run_ok("project.save", json!({}));
    engine.run_ok("project.close", json!({}));
    let file = dir.path().join("Demo").join("Demo.reda");
    let saved = std::fs::read_to_string(&file).unwrap();
    let job_line = saved
        .lines()
        .find(|l| l.contains("\"name_template\""))
        .expect("the job is saved on one line");
    std::fs::write(&file, saved.replace(job_line, &edit(job_line))).unwrap();
    let mut reopened = test_engine();
    let err = reopened.run_err("project.open", json!({"path": dir.path().join("Demo")}));
    std::fs::write(&file, saved).unwrap();
    engine.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    err.message
}

#[test]
fn reserved_job_keys_are_refused_on_add_and_on_load() {
    let dir = TempDir::new("jobs-reserved");
    let mut engine = engine(&dir);
    let preset = jobs(&engine)[0].fab_preset;
    for key in ["checks", "overrides", "variant"] {
        let err = engine.run_err("output.job.add", json!({"fab_preset": preset, key: {}}));
        assert_eq!(err.code, ErrorCode::InvalidParams, "{key}");
    }
    for key in ["checks", "overrides", "variant"] {
        let message = load_error_after(&dir, &mut engine, |line| {
            line.replacen('{', &format!("{{\"{key}\":{{}},"), 1)
        });
        assert!(message.contains(key), "{key}: {message}");
    }
}

#[test]
fn an_invalid_name_template_is_refused_on_load() {
    let dir = TempDir::new("jobs-name-template-load");
    let mut engine = engine(&dir);
    for (bad, reason) in [("${variant}", "reserved"), ("a:b", "can't hold")] {
        let message = load_error_after(&dir, &mut engine, |line| {
            line.replace("${project}-${job}", bad)
        });
        assert!(message.contains(reason), "{bad}: {message}");
    }
}

#[test]
fn a_name_template_takes_only_known_tokens_and_file_name_characters() {
    let dir = TempDir::new("jobs-name-template");
    let mut engine = engine(&dir);
    let job = jobs(&engine)[0].clone();
    for good in [
        "${project}-${job}",
        "${project}_${revision}_${date}_${preset}",
        "rev $${not a token}",
    ] {
        engine.run_ok(
            "output.job.set",
            json!({"job": job.uuid, "name_template": good}),
        );
    }
    for bad in [
        "",
        "  ",
        "${project}-${board}",
        "${project}-${variant}",
        "${project",
        "a/b",
        "a\\b",
        "a:b",
        "${job}?",
        "<${job}>",
        "a|b*",
        "\"q\"",
    ] {
        let err = engine.run_err(
            "output.job.set",
            json!({"job": job.uuid, "name_template": bad}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{bad:?}");
        let err = engine.run_err(
            "output.job.add",
            json!({"fab_preset": job.fab_preset, "name_template": bad}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{bad:?}");
    }
    assert_eq!(jobs(&engine).len(), 1, "a refused add adds nothing");
}
