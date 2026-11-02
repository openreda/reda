//! `app.preferences.get` and `app.preferences.set` against a `preferences.toml` under a
//! test `REDA_HOME`.

use std::path::{Path, PathBuf};

use reda_engine::command::{ErrorCode, UserDirs};
use reda_testkit::{TempDir, test_engine, test_engine_in};
use serde_json::{Value, json};

fn prefs_file(home: &Path) -> PathBuf {
    UserDirs::under(home).preferences_file()
}

fn write_prefs(home: &Path, text: &str) {
    let path = prefs_file(home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read_prefs(home: &Path) -> String {
    std::fs::read_to_string(prefs_file(home)).unwrap()
}

fn get(home: &Path, key: &str) -> Value {
    test_engine_in(home).run_ok("app.preferences.get", json!({"key": key}))
}

#[test]
fn defaults_apply_when_there_is_no_file() {
    let home = TempDir::new("prefs-defaults");
    let mut engine = test_engine_in(home.path());
    let out = engine.run_ok("app.preferences.get", json!({}));
    assert_eq!(out["values"]["schematic"]["repeat_placement"], true);
    assert_eq!(out["values"]["general"]["theme"], "follow_os");
    assert_eq!(out["values"]["general"]["undo_depth"], Value::Null);
    assert_eq!(out["values"]["pcb"]["default_units"], "mm");
    assert_eq!(out["warnings"], json!([]));
    assert!(!prefs_file(home.path()).exists(), "get never writes");
}

#[test]
fn one_key_reports_its_value_default_and_description() {
    let home = TempDir::new("prefs-one");
    write_prefs(home.path(), "[schematic]\nrepeat_placement = false\n");
    let out = get(home.path(), "schematic.repeat_placement");
    assert_eq!(out["key"], "schematic.repeat_placement");
    assert_eq!(out["value"], false);
    assert_eq!(out["default"], true);
    let description = out["description"]
        .as_str()
        .expect("described by the schema");
    assert!(description.contains("placing"), "{description}");
}

#[test]
fn set_writes_only_the_non_default_value() {
    let home = TempDir::new("prefs-set");
    let mut engine = test_engine_in(home.path());
    let out = engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.repeat_placement", "value": false}),
    );
    assert_eq!(
        out,
        json!({"key": "schematic.repeat_placement", "value": false})
    );
    assert_eq!(
        read_prefs(home.path()),
        "[schematic]\nrepeat_placement = false\n"
    );
    assert_eq!(
        get(home.path(), "schematic.repeat_placement")["value"],
        false
    );
}

#[test]
fn setting_the_default_value_removes_the_key() {
    let home = TempDir::new("prefs-reset");
    write_prefs(
        home.path(),
        "[schematic]\nrepeat_placement = false\nauto_pan = false\n",
    );
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.repeat_placement", "value": true}),
    );
    assert_eq!(read_prefs(home.path()), "[schematic]\nauto_pan = false\n");
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.auto_pan", "value": null}),
    );
    assert!(!read_prefs(home.path()).contains("auto_pan"));
}

#[test]
fn comments_and_other_keys_survive_a_set() {
    let home = TempDir::new("prefs-comments");
    let before = "\
# My REDA settings.

[general]
# Dark at night.
theme = \"dark\" # keep this

[schematic]
auto_pan = false   # it jumps around
";
    write_prefs(home.path(), before);
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "general.theme", "value": "light"}),
    );
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "pcb.default_units", "value": "mil"}),
    );
    let after = read_prefs(home.path());
    assert_eq!(
        after,
        "\
# My REDA settings.

[general]
# Dark at night.
theme = \"light\" # keep this

[schematic]
auto_pan = false   # it jumps around

[pcb]
default_units = \"mil\"
"
    );
}

#[test]
fn set_rereads_the_file_so_outside_edits_are_kept() {
    let home = TempDir::new("prefs-reread");
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.auto_pan", "value": false}),
    );
    // Another instance (or the user) edits the file between two Commands.
    let edited = format!("{}# added by hand\n", read_prefs(home.path()));
    std::fs::write(prefs_file(home.path()), &edited).unwrap();
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.electrical_snap", "value": false}),
    );
    let after = read_prefs(home.path());
    assert!(after.contains("# added by hand"), "{after}");
    assert!(after.contains("auto_pan = false"), "{after}");
    assert!(after.contains("electrical_snap = false"), "{after}");
}

#[test]
fn set_refuses_unknown_keys_and_invalid_values() {
    let home = TempDir::new("prefs-invalid");
    let mut engine = test_engine_in(home.path());
    let err = engine.run_err(
        "app.preferences.set",
        json!({"key": "schematic.no_such_thing", "value": 1}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details, Some(json!({"pointer": "/key"})));

    let err = engine.run_err(
        "app.preferences.set",
        json!({"key": "general.theme", "value": "purple"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details, Some(json!({"pointer": "/value"})));

    let err = engine.run_err(
        "app.preferences.set",
        json!({"key": "general.ui_scale", "value": 40}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert!(!prefs_file(home.path()).exists(), "nothing was written");
}

#[test]
fn unknown_and_invalid_keys_warn_and_fall_back_to_defaults() {
    let home = TempDir::new("prefs-warn");
    write_prefs(
        home.path(),
        "\
[general]
theme = \"purple\"
reopen_last_project = false
[schematic]
repeat_placement = \"yes\"
mystery = 3
[gizmo]
x = 1
",
    );
    let out = test_engine_in(home.path()).run_ok("app.preferences.get", json!({}));
    assert_eq!(out["values"]["general"]["theme"], "follow_os");
    assert_eq!(out["values"]["general"]["reopen_last_project"], false);
    assert_eq!(out["values"]["schematic"]["repeat_placement"], true);
    let warnings: Vec<&str> = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    assert_eq!(warnings.len(), 4, "{warnings:?}");
    for key in [
        "general.theme",
        "schematic.repeat_placement",
        "schematic.mystery",
        "gizmo.x",
    ] {
        assert!(
            warnings.iter().any(|w| w.contains(key)),
            "no warning names {key}: {warnings:?}"
        );
    }
}

#[test]
fn a_file_that_does_not_parse_warns_and_is_never_overwritten() {
    let home = TempDir::new("prefs-broken");
    write_prefs(home.path(), "[general\ntheme = dark\n");
    let mut engine = test_engine_in(home.path());
    let out = engine.run_ok("app.preferences.get", json!({}));
    assert_eq!(out["values"]["general"]["theme"], "follow_os");
    assert_eq!(out["warnings"].as_array().unwrap().len(), 1);
    let err = engine.run_err(
        "app.preferences.set",
        json!({"key": "general.theme", "value": "dark"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(read_prefs(home.path()), "[general\ntheme = dark\n");
}

#[test]
fn optional_and_list_values_round_trip() {
    let home = TempDir::new("prefs-kinds");
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "general.undo_depth", "value": 200}),
    );
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": ["/libs/a", "/libs/b"]}),
    );
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "general.ui_scale", "value": 1.25}),
    );
    assert_eq!(
        read_prefs(home.path()),
        "[general]\nundo_depth = 200\nui_scale = 1.25\n\n[libraries]\npaths = [\"/libs/a\", \"/libs/b\"]\n"
    );
    let out = engine.run_ok("app.preferences.get", json!({}));
    assert_eq!(out["values"]["general"]["undo_depth"], 200);
    assert_eq!(
        out["values"]["libraries"]["paths"],
        json!(["/libs/a", "/libs/b"])
    );
    assert_eq!(out["values"]["general"]["ui_scale"], 1.25);
}

#[test]
fn preference_commands_need_a_per_user_folder() {
    let mut engine = test_engine();
    let err = engine.run_err("app.preferences.get", json!({}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}

#[test]
fn preference_commands_are_journalled_and_dirty_nothing() {
    let home = TempDir::new("prefs-journal");
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.auto_pan", "value": false}),
    );
    engine.run_err(
        "app.preferences.set",
        json!({"key": "schematic.auto_pan", "value": 7}),
    );
    let journal = engine.journal();
    assert_eq!(journal.len(), 2);
    assert_eq!(journal[0].id, "app.preferences.set");
    assert_eq!(
        journal[0].params,
        json!({"key": "schematic.auto_pan", "value": false})
    );
    assert!(journal[0].outcome.is_ok());
    assert_eq!(
        journal[1].outcome.as_ref().unwrap_err().code,
        ErrorCode::InvalidParams
    );
    assert!(!engine.is_dirty());
}

#[test]
fn the_schema_comes_from_the_preferences_type() {
    let schema = reda_engine::preferences::schema();
    assert_eq!(schema["type"], "object");
    let theme = &schema["$defs"]["General"]["properties"]["theme"];
    assert!(theme.is_object(), "{schema:#}");
}

#[test]
fn layer_colours_are_overridden_per_theme_and_layer_kind() {
    let home = TempDir::new("prefs-layer-colours");
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "display.layer_colors.dark.copper_top", "value": "#ff8800"}),
    );
    assert_eq!(
        get(home.path(), "display.layer_colors.dark.copper_top")["value"],
        "#ff8800"
    );
    assert_eq!(
        get(home.path(), "display.layer_colors.light.copper_top")["value"],
        Value::Null
    );
    assert!(read_prefs(home.path()).contains("copper_top = \"#ff8800\""));

    let err = engine.run_err(
        "app.preferences.set",
        json!({"key": "display.layer_colors.dark.silkscreen_bottom", "value": "orange"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}
