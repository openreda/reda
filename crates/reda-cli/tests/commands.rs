//! `reda commands [--json]`: the Command catalogue.

use std::path::Path;
use std::process::{Command, Output};

use reda_testkit::TempDir;

fn reda(args: &[&str]) -> Output {
    let home = TempDir::new("cli-home");
    Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(args)
        .env("REDA_HOME", home.path())
        .output()
        .expect("run reda")
}

#[test]
fn commands_json_prints_the_checked_in_catalogue() {
    let out = reda(&["commands", "--json"]);
    assert!(out.status.success());
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/commands.json");
    let expected = std::fs::read_to_string(docs).unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
}

#[test]
fn commands_lists_one_id_and_title_per_line() {
    let out = reda(&["commands"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<Vec<&str>> = text
        .lines()
        .map(|l| l.splitn(2, "  ").map(str::trim).collect())
        .collect();
    assert!(lines.contains(&vec!["project.save", "Save All"]), "{text}");
    assert!(lines.contains(&vec!["batch", "Batch"]), "{text}");
    let ids: Vec<&str> = lines.iter().map(|l| l[0]).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
}

#[test]
fn commands_refuses_unknown_flags() {
    let out = reda(&["commands", "--yaml"]);
    assert_eq!(out.status.code(), Some(2));
}
