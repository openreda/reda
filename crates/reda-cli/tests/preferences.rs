//! Preferences from the CLI, with `REDA_HOME` choosing the per-user folders.

use std::path::Path;
use std::process::{Command, Output};

use reda_testkit::TempDir;
use serde_json::{Value, json};

fn reda(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(args)
        .env("REDA_HOME", home)
        .output()
        .expect("run reda")
}

fn stdout_json(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "reda failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is JSON")
}

#[test]
fn set_then_get_through_reda_home() {
    let home = TempDir::new("cli-prefs");
    let out = reda(
        home.path(),
        &[
            "run",
            "app.preferences.set",
            "--key",
            "schematic.repeat_placement",
            "--value",
            "false",
        ],
    );
    assert_eq!(
        stdout_json(&out),
        json!({"key": "schematic.repeat_placement", "value": false})
    );
    let file = home.path().join("config").join("preferences.toml");
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        "[schematic]\nrepeat_placement = false\n"
    );

    let out = reda(
        home.path(),
        &[
            "run",
            "app.preferences.get",
            "--key",
            "schematic.repeat_placement",
        ],
    );
    assert_eq!(stdout_json(&out)["value"], false);
}
