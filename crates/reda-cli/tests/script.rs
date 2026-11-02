//! `reda run --script journal.jsonl`: replays an exported Journal.

use std::path::Path;
use std::process::{Command, Output};

use reda_engine::journal;
use reda_testkit::{TempDir, read_tree, test_engine};
use serde_json::{Value, json};

fn reda(args: &[&str]) -> Output {
    let home = TempDir::new("cli-home");
    Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(args)
        .env("REDA_HOME", home.path())
        .output()
        .expect("run reda")
}

fn path(p: &Path) -> &str {
    p.to_str().expect("temp paths are UTF-8")
}

/// Records, on seed 0, a new Project under `folder` and a query on its Sheet, and
/// writes the exported Journal to `folder/journal.jsonl`.
fn record(folder: &Path) -> std::path::PathBuf {
    let mut e = test_engine();
    let sheet = e.run_ok(
        "project.new",
        json!({"path": folder.join("recorded-folder"), "name": "Demo"}),
    )["documents"][0]["uuid"]
        .clone();
    e.run_ok("object.get", json!({ "uuid": sheet }));
    let script = folder.join("journal.jsonl");
    let mut text = journal::export(e.journal());
    // Replay into a different folder than the recording used.
    text = text.replace("recorded-folder", "replay");
    std::fs::write(&script, text).unwrap();
    script
}

#[test]
fn a_script_replays_and_prints_the_new_journal() {
    let dir = TempDir::new("cli-script");
    let script = record(dir.path());
    let out = reda(&["run", "--script", path(&script), "--save"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let ids: Vec<&str> = lines.iter().map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["project.new", "object.get", "project.save"]);
    // The CLI makes random UUIDs, so the query only resolves through remapping.
    let sheet = &lines[0]["result"]["documents"][0]["uuid"];
    assert_ne!(sheet, "00000000-0000-4000-8000-000000000002");
    assert_eq!(&lines[1]["params"]["uuid"], sheet);
    let tree = read_tree(&dir.path().join("replay/Demo"));
    assert!(tree.contains_key("Demo.reda"), "{:?}", tree.keys());
}

#[test]
fn a_replay_mismatch_prints_the_error_json_and_exits_1() {
    let dir = TempDir::new("cli-script-mismatch");
    let script = dir.path().join("journal.jsonl");
    std::fs::write(
        &script,
        "{\"id\":\"project.overview\",\"params\":{},\"result\":{}}\n",
    )
    .unwrap();
    let out = reda(&["run", "--script", path(&script)]);
    assert_eq!(out.status.code(), Some(1));
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr is JSON");
    assert_eq!(err["line"], 1);
    assert_eq!(err["id"], "project.overview");
}

#[test]
fn a_missing_script_exits_1() {
    let dir = TempDir::new("cli-script-missing");
    let out = reda(&["run", "--script", path(&dir.path().join("nope.jsonl"))]);
    assert_eq!(out.status.code(), Some(1));
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr is JSON");
    assert_eq!(err["code"], "not_found");
}

#[test]
fn a_recorded_session_replays_into_another_folder_with_into() {
    let dir = TempDir::new("cli-script-into");
    let mut e = test_engine();
    let recorded = dir.path().join("recorded");
    e.run_ok("project.new", json!({"path": recorded, "name": "Demo"}));
    let script = dir.path().join("journal.jsonl");
    std::fs::write(&script, journal::export(e.journal())).unwrap();

    // The recorded folder still holds the recorded Project.
    let out = reda(&["run", "--script", path(&script)]);
    assert_eq!(out.status.code(), Some(1));
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr is JSON");
    assert!(err["message"].as_str().unwrap().contains("--into"), "{err}");

    let into = dir.path().join("replayed");
    let out = reda(&["run", "--script", path(&script), "--into", path(&into)]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(into.join("Demo/Demo.reda").is_file());
}

#[test]
fn a_reference_project_script_replays_end_to_end_with_random_uuids() {
    let dir = TempDir::new("cli-script-reference");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = repo.join("tests/scenarios/blinky_555.jsonl");
    let out = Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(["run", "--script", path(&script), "--into", path(dir.path())])
        .env("REDA_HOME", dir.path().join("home"))
        .env("REDA_STARTER_LIBRARY", repo.join("library/starter"))
        .output()
        .expect("run reda");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let outputs = read_tree(&dir.path().join("Blinky/outputs/JLCPCB"));
    assert!(outputs.contains_key("Blinky.GTL"), "{:?}", outputs.keys());
    assert!(outputs.contains_key("Blinky-JLCPCB.zip"));
}

#[test]
fn script_usage_errors_exit_2() {
    for args in [
        &["run", "--script"][..],
        &["run", "--script", "a.jsonl", "--params", "{}"],
        &["run", "--script", "a.jsonl", "--name", "x"],
    ] {
        let out = reda(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}
