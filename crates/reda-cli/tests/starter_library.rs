//! The `reda` binary registers the starter Library (`library/starter/` in the repo) by
//! default; `REDA_STARTER_LIBRARY` points it elsewhere, or turns it off when empty.

use std::process::{Command, Output};

use reda_testkit::TempDir;
use serde_json::Value;

fn search(starter: Option<&str>) -> Value {
    let home = TempDir::new("cli-starter-home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_reda"));
    command
        .args(["run", "library.search", "--text", "Resistor"])
        .env("REDA_HOME", home.path())
        .env_remove("REDA_STARTER_LIBRARY");
    if let Some(starter) = starter {
        command.env("REDA_STARTER_LIBRARY", starter);
    }
    let out: Output = command.output().expect("run reda");
    assert!(
        out.status.success(),
        "reda failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is JSON")
}

#[test]
fn the_starter_library_is_registered_by_default() {
    let out = search(None);
    assert_eq!(out["libraries"][0]["origin"], "starter");
    assert_eq!(out["libraries"][0]["name"], "Starter");
    assert!(
        out["parts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "Resistor"),
        "{out}"
    );
}

#[test]
fn an_empty_reda_starter_library_turns_it_off() {
    let out = search(Some(""));
    assert_eq!(out["libraries"], serde_json::json!([]));
    assert_eq!(out["parts"], serde_json::json!([]));
}
