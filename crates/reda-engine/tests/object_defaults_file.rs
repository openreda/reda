//! Resetting, saving and loading Object Defaults
//! (docs/spec/v0.3/input-and-defaults.md#the-defaults-page): `app.defaults.reset`,
//! `app.defaults.save` and `app.defaults.load`, for one kind or all.

use std::path::{Path, PathBuf};

use reda_engine::command::{ErrorCode, UserDirs};
use reda_engine::preferences;
use reda_testkit::{TempDir, TestEngine, test_engine_in};
use serde_json::{Value, json};

fn prefs_file(home: &Path) -> PathBuf {
    UserDirs::under(home).preferences_file()
}

fn prefs_text(home: &Path) -> String {
    std::fs::read_to_string(prefs_file(home)).unwrap_or_default()
}

fn set(engine: &mut TestEngine, key: &str, value: Value) {
    engine.run_ok("app.preferences.set", json!({"key": key, "value": value}));
}

fn get(engine: &mut TestEngine, key: &str) -> Value {
    engine.run_ok("app.preferences.get", json!({"key": key}))["value"].clone()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{value} is not a list"))
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn reset_puts_one_kind_or_every_kind_back_to_its_shipped_values() {
    let home = TempDir::new("defaults-reset");
    let mut engine = test_engine_in(home.path());
    set(&mut engine, "defaults.symbol.pin.length", json!("200mil"));
    set(
        &mut engine,
        "defaults.symbol.pin.electrical_type",
        json!("input"),
    );
    set(
        &mut engine,
        "defaults.pcb.line.silkscreen.width",
        json!("0.2mm"),
    );
    set(&mut engine, "defaults.remember_last_used", json!(false));
    set(&mut engine, "schematic.repeat_placement", json!(false));

    let out = engine.run_ok("app.defaults.reset", json!({"kind": "symbol.pin"}));
    assert_eq!(
        strings(&out["reset"]),
        [
            "defaults.symbol.pin.electrical_type",
            "defaults.symbol.pin.length"
        ]
    );
    assert_eq!(
        get(&mut engine, "defaults.symbol.pin.length"),
        json!("100mil")
    );
    assert_eq!(
        get(&mut engine, "defaults.pcb.line.silkscreen.width"),
        json!("0.2mm")
    );

    let out = engine.run_ok("app.defaults.reset", json!({}));
    assert_eq!(
        strings(&out["reset"]),
        ["defaults.pcb.line.silkscreen.width"]
    );
    assert_eq!(
        get(&mut engine, "defaults.pcb.line.silkscreen.width"),
        json!("0.15mm")
    );
    // Remember last used is not a record, and the rest of Preferences is untouched.
    assert_eq!(
        get(&mut engine, "defaults.remember_last_used"),
        json!(false)
    );
    assert_eq!(get(&mut engine, "schematic.repeat_placement"), json!(false));

    let err = engine.run_err("app.defaults.reset", json!({"kind": "pcb.track"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], json!("/kind"));
}

/// The lines of `text` from `[header]` to the next blank line or header.
fn block<'t>(text: &'t str, header: &str) -> Vec<&'t str> {
    let mut lines = text.lines().skip_while(|l| *l != header);
    let mut out: Vec<&str> = lines.next().into_iter().collect();
    out.extend(lines.take_while(|l| !l.is_empty() && !l.starts_with('[')));
    out
}

#[test]
fn a_saved_file_is_the_same_toml_text_as_the_defaults_table_in_preferences_toml() {
    let home = TempDir::new("defaults-save");
    let work = TempDir::new("defaults-save-out");
    let mut engine = test_engine_in(home.path());
    set(
        &mut engine,
        "defaults.pcb.line.silkscreen.width",
        json!("0.2mm"),
    );
    set(
        &mut engine,
        "defaults.pcb.line.courtyard.width",
        json!("2mil"),
    );
    set(&mut engine, "defaults.symbol.pin.length", json!("200mil"));
    let prefs = prefs_text(home.path());

    let path = work.path().join("line.toml");
    let out = engine.run_ok(
        "app.defaults.save",
        json!({"path": path, "kind": "pcb.line"}),
    );
    assert_eq!(strings(&out["kinds"]), ["pcb.line"]);
    // Every field of the kind, set or shipped: five Layer kinds of one width.
    assert_eq!(out["entries"], json!(5));
    let saved = std::fs::read_to_string(&path).unwrap();
    for header in [
        "[defaults.pcb.line.silkscreen]",
        "[defaults.pcb.line.courtyard]",
    ] {
        assert_eq!(block(&saved, header), block(&prefs, header), "{saved}");
        assert_eq!(block(&saved, header).len(), 2, "{saved}");
    }
    assert_eq!(
        block(&saved, "[defaults.pcb.line.copper]"),
        ["[defaults.pcb.line.copper]", "width = \"0.15mm\""]
    );
    assert!(!saved.contains("pin"), "{saved}");
    // It reads back as Preferences with no warning, holding the same record.
    let read = preferences::parse(&saved);
    assert_eq!(read.warnings, Vec::<String>::new());
    let now = preferences::load(&UserDirs::under(home.path())).values;
    assert_eq!(read.values.defaults.pcb.line, now.defaults.pcb.line);

    let all = work.path().join("all.toml");
    let out = engine.run_ok("app.defaults.save", json!({"path": all}));
    assert_eq!(out["kinds"].as_array().unwrap().len(), 22);
    let saved = std::fs::read_to_string(&all).unwrap();
    let pin = block(&saved, "[defaults.symbol.pin]");
    assert_eq!(pin.len(), 5, "{saved}");
    assert!(pin.contains(&"length = \"200mil\""), "{saved}");
    assert!(pin.contains(&"electrical_type = \"passive\""), "{saved}");
    assert!(!saved.contains("remember_last_used"), "{saved}");
    let read = preferences::parse(&saved);
    assert_eq!(read.warnings, Vec::<String>::new());
    assert_eq!(read.values.defaults, now.defaults);

    // A relative path needs a Project folder to sit in.
    let err = engine.run_err("app.defaults.save", json!({"path": "x.toml"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn a_load_replaces_only_the_kinds_the_file_holds() {
    let home = TempDir::new("defaults-load");
    let work = TempDir::new("defaults-load-in");
    let mut engine = test_engine_in(home.path());
    set(&mut engine, "defaults.symbol.pin.length", json!("200mil"));
    set(
        &mut engine,
        "defaults.pcb.line.silkscreen.width",
        json!("0.2mm"),
    );
    let path = work.path().join("in.toml");
    std::fs::write(
        &path,
        "# A Silkscreen-less line set.\n[defaults.pcb.line.copper]\nwidth = \"0.3mm\"\n",
    )
    .unwrap();

    let out = engine.run_ok("app.defaults.load", json!({"path": path}));
    assert_eq!(strings(&out["kinds"]), ["pcb.line"]);
    assert_eq!(out["skipped"], json!(0));
    assert_eq!(
        get(&mut engine, "defaults.pcb.line.copper.width"),
        json!("0.3mm")
    );
    // The record is replaced: what the file leaves out is shipped again.
    assert_eq!(
        get(&mut engine, "defaults.pcb.line.silkscreen.width"),
        json!("0.15mm")
    );
    // Kinds the file doesn't hold are left alone.
    assert_eq!(
        get(&mut engine, "defaults.symbol.pin.length"),
        json!("200mil")
    );

    // A saved file loads back to the same records.
    let saved = work.path().join("saved.toml");
    engine.run_ok("app.defaults.save", json!({"path": saved}));
    let before = preferences::load(&UserDirs::under(home.path())).values;
    engine.run_ok("app.defaults.reset", json!({}));
    let out = engine.run_ok("app.defaults.load", json!({"path": saved}));
    assert_eq!(out["skipped"], json!(0));
    assert_eq!(out["kinds"].as_array().unwrap().len(), 22);
    let after = preferences::load(&UserDirs::under(home.path())).values;
    assert_eq!(after, before);
}

#[test]
fn a_load_skips_and_counts_unknown_or_unreadable_entries() {
    let home = TempDir::new("defaults-skip");
    let work = TempDir::new("defaults-skip-in");
    let mut engine = test_engine_in(home.path());
    set(&mut engine, "defaults.pcb.via.teardrop", json!(false));
    set(&mut engine, "schematic.repeat_placement", json!(false));
    let path = work.path().join("in.toml");
    std::fs::write(
        &path,
        "remember_last_used = true\n\
         [general]\ntheme = \"dark\"\n\
         [defaults]\nremember_last_used = false\n\
         [defaults.pcb.bogus]\nwidth = \"1mm\"\n\
         [defaults.pcb.via]\nsize = \"1mm\"\nteardrop = \"maybe\"\n\
         [defaults.symbol.pin]\nlength = \"-1mm\"\nelectrical_type = \"input\"\n\
         number_visible = [1, 2]\n\
         [pcb.zone.tool]\nclearance = \"1mm\"\n",
    )
    .unwrap();
    let out = engine.run_ok("app.defaults.load", json!({"path": path}));
    assert_eq!(out["skipped"], json!(9), "{out}");
    assert_eq!(out["warnings"].as_array().unwrap().len(), 9, "{out}");
    assert_eq!(strings(&out["kinds"]), ["pcb.via", "symbol.pin"]);
    // What could be read is in; the rest of each record held is shipped.
    assert_eq!(
        get(&mut engine, "defaults.symbol.pin.electrical_type"),
        json!("input")
    );
    assert_eq!(
        get(&mut engine, "defaults.symbol.pin.length"),
        json!("100mil")
    );
    assert_eq!(get(&mut engine, "defaults.pcb.via.teardrop"), json!(true));
    // Nothing outside the records changes.
    assert_eq!(get(&mut engine, "defaults.remember_last_used"), json!(true));
    assert_eq!(get(&mut engine, "schematic.repeat_placement"), json!(false));
    assert_eq!(get(&mut engine, "general.theme"), json!("follow_os"));
}

#[test]
fn a_load_never_fails_part_way() {
    let home = TempDir::new("defaults-atomic");
    let work = TempDir::new("defaults-atomic-in");
    let mut engine = test_engine_in(home.path());
    set(&mut engine, "defaults.symbol.pin.length", json!("200mil"));
    let before = prefs_text(home.path());
    // A file that is not TOML changes nothing.
    let broken = work.path().join("broken.toml");
    std::fs::write(&broken, "[defaults.symbol.pin]\nlength = \"300mil\"\n[[[\n").unwrap();
    let err = engine.run_err("app.defaults.load", json!({"path": broken}));
    assert_eq!(err.code, ErrorCode::InvalidParams, "{}", err.message);
    assert_eq!(prefs_text(home.path()), before);
    let err = engine.run_err(
        "app.defaults.load",
        json!({"path": work.path().join("missing.toml")}),
    );
    assert_eq!(err.code, ErrorCode::NotFound, "{}", err.message);
    assert_eq!(prefs_text(home.path()), before);
    // Nor when preferences.toml itself can't be read back to be rewritten.
    let good = work.path().join("good.toml");
    std::fs::write(&good, "[defaults.symbol.pin]\nlength = \"300mil\"\n").unwrap();
    std::fs::write(prefs_file(home.path()), "[[[\n").unwrap();
    let err = engine.run_err("app.defaults.load", json!({"path": good}));
    assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
    assert_eq!(prefs_text(home.path()), "[[[\n");
}

#[test]
fn every_object_default_belongs_to_one_kind_and_every_kind_has_fields() {
    for key in preferences::keys() {
        if !key.starts_with("defaults.") || key == "defaults.remember_last_used" {
            continue;
        }
        let kind = preferences::kind_of_key(&key).unwrap_or_else(|| panic!("{key} is in no kind"));
        assert!(key.starts_with(&format!("{}.", kind.prefix())), "{key}");
    }
    for kind in preferences::KINDS {
        assert!(!kind.keys().is_empty(), "{} has no fields", kind.id);
        assert!(!kind.editors.is_empty(), "{} is in no editor", kind.id);
    }
    // The spec's Kinds and fields table, with board and Footprint Text as two records.
    assert_eq!(preferences::KINDS.len(), 22);
}
