//! `reda run <id> [--params json | --key value…] [--project p] [--save]`, in-process.

use std::path::Path;
use std::process::{Command, Output};

use reda_testkit::{TempDir, read_tree};
use serde_json::Value;

fn reda(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(args)
        .output()
        .expect("run reda")
}

fn path(p: &Path) -> &str {
    p.to_str().expect("temp paths are UTF-8")
}

fn stdout_json(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "reda failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is JSON")
}

fn new_params(dir: &Path) -> String {
    serde_json::json!({"path": dir, "name": "Demo"}).to_string()
}

#[test]
fn project_new_with_save_writes_the_project_folder() {
    let dir = TempDir::new("cli-new-save");
    let params = new_params(dir.path());
    let out = reda(&["run", "project.new", "--params", &params, "--save"]);
    let result = stdout_json(&out);
    assert_eq!(result["name"], "Demo");
    assert_eq!(result["documents"][0]["title"], "Sheet 1");

    let tree = read_tree(&dir.path().join("Demo"));
    let names: Vec<&str> = tree.keys().map(String::as_str).collect();
    assert_eq!(names, [".reda/.gitignore", "Demo.reda", "Demo.redasch"]);
}

#[test]
fn nothing_is_written_without_save() {
    let dir = TempDir::new("cli-new-nosave");
    let params = new_params(dir.path());
    let out = reda(&["run", "project.new", "--params", &params]);
    stdout_json(&out);
    assert!(read_tree(dir.path()).is_empty());
}

#[test]
fn params_can_be_given_as_key_value_flags() {
    let dir = TempDir::new("cli-new-flags");
    let out = reda(&[
        "run",
        "project.new",
        "--path",
        path(dir.path()),
        "--name",
        "Demo",
        "--save",
    ]);
    stdout_json(&out);
    assert!(dir.path().join("Demo").join("Demo.reda").is_file());
}

#[test]
fn open_and_save_through_project_reproduces_the_same_bytes() {
    let dir = TempDir::new("cli-roundtrip");
    let params = new_params(dir.path());
    stdout_json(&reda(&[
        "run",
        "project.new",
        "--params",
        &params,
        "--save",
    ]));
    let folder = dir.path().join("Demo");
    let before = read_tree(&folder);

    let reda_file = folder.join("Demo.reda");
    let out = reda(&["run", "project.save", "--project", path(&reda_file)]);
    stdout_json(&out);
    assert_eq!(read_tree(&folder), before);
}

#[test]
fn a_failed_command_prints_the_error_json_and_exits_1() {
    let out = reda(&["run", "no.such.command"]);
    assert_eq!(out.status.code(), Some(1));
    let err: Value = serde_json::from_slice(&out.stderr).expect("stderr is JSON");
    assert_eq!(err["code"], "not_found");
}

#[test]
fn usage_errors_exit_2() {
    for args in [
        &["run"][..],
        &["run", "project.new", "--params"],
        &["run", "project.new", "--params", "{not json"],
        &["run", "project.new", "--params", "{}", "--name", "x"],
        &["run", "project.new", "stray"],
    ] {
        let out = reda(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("Usage"), "{args:?}: {stderr}");
    }
}
