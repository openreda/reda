//! The data-driven scenario harness, run on scratch folders.

use std::path::Path;

use reda_testkit::{TempDir, read_tree, scenario_failures};

const NEW: &str =
    "{\"id\":\"project.new\",\"params\":{\"path\":\"/nonexistent\",\"name\":\"Demo\"}}\n";

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn blessing_writes_goldens_and_records_outcomes_then_the_check_passes() {
    let dir = TempDir::new("scenarios-bless");
    write(&dir.path().join("demo.jsonl"), NEW);
    assert_eq!(scenario_failures(dir.path(), true), Vec::<String>::new());

    let goldens: Vec<String> = read_tree(&dir.path().join("demo")).into_keys().collect();
    assert_eq!(goldens, ["Demo.reda", "Demo.redapcb", "Demo.redasch"]);
    let script = std::fs::read_to_string(dir.path().join("demo.jsonl")).unwrap();
    assert!(script.contains("\"result\":"), "{script}");
    // The temporary folder the replay wrote to is recorded as the script's placeholder.
    assert!(
        script.contains("\"path\":\"/nonexistent") && !script.contains("reda-test-scenario-"),
        "{script}"
    );
    assert_eq!(scenario_failures(dir.path(), false), Vec::<String>::new());
}

#[test]
fn a_missing_golden_fails() {
    let dir = TempDir::new("scenarios-missing");
    write(&dir.path().join("demo.jsonl"), NEW);
    let failures = scenario_failures(dir.path(), false);
    assert_eq!(failures.len(), 3, "{failures:?}");
    assert!(failures[0].contains("demo") && failures[0].contains("Demo.reda"));
}

#[test]
fn a_changed_or_stale_golden_fails() {
    let dir = TempDir::new("scenarios-changed");
    write(&dir.path().join("demo.jsonl"), NEW);
    scenario_failures(dir.path(), true);
    write(&dir.path().join("demo/Demo.redasch"), "{}\n");
    write(&dir.path().join("demo/Old.redasch"), "{}\n");
    let failures = scenario_failures(dir.path(), false);
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(
        failures
            .iter()
            .any(|f| f.contains("Demo.redasch") && f.contains("differs"))
    );
    assert!(
        failures
            .iter()
            .any(|f| f.contains("Old.redasch") && f.contains("stale"))
    );

    // Blessing removes the stale file.
    scenario_failures(dir.path(), true);
    assert!(!dir.path().join("demo/Old.redasch").exists());
}

#[test]
fn a_replay_failure_names_the_script_and_line() {
    let dir = TempDir::new("scenarios-replay");
    write(
        &dir.path().join("bad.jsonl"),
        &format!(
            "{NEW}{{\"id\":\"object.get\",\"params\":{{\"uuid\":\"00000000-0000-4000-8000-0000000000ff\"}}}}\n"
        ),
    );
    let failures = scenario_failures(dir.path(), false);
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].contains("bad.jsonl") && failures[0].contains("line 2"));
}

#[test]
fn a_folder_without_scripts_fails() {
    let dir = TempDir::new("scenarios-empty");
    assert_eq!(scenario_failures(dir.path(), false).len(), 1);
}
