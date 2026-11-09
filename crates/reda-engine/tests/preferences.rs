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
fn setting_the_undo_depth_caps_the_undo_stacks_at_once() {
    let home = TempDir::new("prefs-undo-depth");
    let mut engine = test_engine_in(home.path());
    assert_eq!(engine.history().depth(), None);
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "general.undo_depth", "value": 25}),
    );
    assert_eq!(engine.history().depth(), Some(25));
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "general.undo_depth", "value": null}),
    );
    assert_eq!(engine.history().depth(), None);
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

#[test]
fn reset_puts_the_given_keys_back_and_keeps_the_rest() {
    let home = TempDir::new("prefs-reset-keys");
    write_prefs(
        home.path(),
        "# mine\n[general]\ntheme = \"dark\"\nui_scale = 1.5\n\n[pcb]\nauto_pan = false\n",
    );
    let mut engine = test_engine_in(home.path());
    let out = engine.run_ok(
        "app.preferences.reset",
        json!({"keys": ["general.theme", "pcb.auto_pan", "pcb.electrical_snap"]}),
    );
    // Only the keys that were changed are reported, sorted.
    assert_eq!(out, json!({"reset": ["general.theme", "pcb.auto_pan"]}));
    assert_eq!(
        read_prefs(home.path()),
        "# mine\n[general]\nui_scale = 1.5\n\n[pcb]\n"
    );
}

#[test]
fn reset_a_page_or_its_branch() {
    let home = TempDir::new("prefs-reset-page");
    write_prefs(
        home.path(),
        "[schematic]\nrepeat_placement = false\nelectrical_snap = false\noptimize_wires = false\n",
    );
    let mut engine = test_engine_in(home.path());
    // Editing holds repeat placement; its Snap sub-page holds electrical snap.
    let out = engine.run_ok("app.preferences.reset", json!({"page": "editing"}));
    assert_eq!(out, json!({"reset": ["schematic.repeat_placement"]}));
    let out = engine.run_ok(
        "app.preferences.reset",
        json!({"page": "editing", "branch": true}),
    );
    assert_eq!(out, json!({"reset": ["schematic.electrical_snap"]}));
    assert_eq!(
        read_prefs(home.path()),
        "[schematic]\noptimize_wires = false\n"
    );
}

#[test]
fn reset_all_empties_the_file_of_values() {
    let home = TempDir::new("prefs-reset-all");
    write_prefs(home.path(), "[agents]\nbridge = false\n");
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "general.undo_depth", "value": 3}),
    );
    assert_eq!(engine.history().depth(), Some(3));
    let out = engine.run_ok("app.preferences.reset", json!({"all": true}));
    assert_eq!(
        out,
        json!({"reset": ["agents.bridge", "general.undo_depth"]})
    );
    assert_eq!(read_prefs(home.path()), "[agents]\n\n[general]\n");
    // Resetting the undo depth applies at once, like setting it.
    assert_eq!(engine.history().depth(), None);
}

#[test]
fn reset_takes_exactly_one_of_keys_page_and_all() {
    let home = TempDir::new("prefs-reset-bad");
    let mut engine = test_engine_in(home.path());
    for params in [
        json!({}),
        json!({"keys": ["general.theme"], "all": true}),
        json!({"page": "general", "keys": []}),
        json!({"all": false}),
    ] {
        let e = engine
            .run("app.preferences.reset", params.clone())
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams, "{params}");
    }
    let e = engine
        .run("app.preferences.reset", json!({"keys": ["general.nope"]}))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidParams);
    assert_eq!(e.details.unwrap()["pointer"], "/keys/0");
    let e = engine
        .run("app.preferences.reset", json!({"page": "nope"}))
        .unwrap_err();
    assert_eq!(e.details.unwrap()["pointer"], "/page");
}

#[test]
fn every_preference_is_on_a_page_of_the_tree() {
    use reda_engine::preferences::pages;
    let ids: Vec<&str> = pages::PAGES.iter().map(|p| p.id).collect();
    for page in pages::PAGES {
        if let Some(parent) = page.parent {
            assert!(ids.contains(&parent), "{} has an unknown parent", page.id);
        }
    }
    for key in reda_engine::preferences::keys() {
        assert!(ids.contains(&pages::page_of(&key)), "{key}");
    }
    assert_eq!(
        pages::page_of("schematic.optimize_wires"),
        "schematic.wires"
    );
    assert_eq!(pages::page_of("pcb.electrical_snap"), "editing.snap");
    assert_eq!(pages::page_of("general.default_layout"), "window");
}

#[test]
fn zones_refill_after_each_edit_unless_turned_off() {
    let home = TempDir::new("prefs-auto-refill");
    let out = get(home.path(), "pcb.zone.auto_refill");
    assert_eq!(out["value"], "after_edit");
    assert_eq!(out["default"], "after_edit");
    assert_eq!(
        reda_engine::preferences::pages::page_of("pcb.zone.auto_refill"),
        "pcb.zones"
    );
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "pcb.zone.auto_refill", "value": "off"}),
    );
    assert_eq!(get(home.path(), "pcb.zone.auto_refill")["value"], "off");
    let err = engine.run_err(
        "app.preferences.set",
        json!({"key": "pcb.zone.auto_refill", "value": "always"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn placement_preferences_keep_the_footprint_move_modes() {
    let home = TempDir::new("prefs-placement");
    for (key, default) in [
        ("pcb.place.obstacle_mode", json!("ignore")),
        ("pcb.place.alignment_guides", json!(true)),
        ("pcb.place.ratsnest_mode", json!("breaks")),
    ] {
        let out = get(home.path(), key);
        assert_eq!(out["value"], default, "{key}");
        assert_eq!(out["default"], default, "{key}");
        assert_eq!(
            reda_engine::preferences::pages::page_of(key),
            "pcb.placement",
            "{key}"
        );
    }
    let mut engine = test_engine_in(home.path());
    for (key, value) in [
        ("pcb.place.obstacle_mode", json!("avoid")),
        ("pcb.place.alignment_guides", json!(false)),
        ("pcb.place.ratsnest_mode", json!("pad_to_pad")),
        ("pcb.place.ratsnest_mode", json!("hidden")),
    ] {
        engine.run_ok("app.preferences.set", json!({"key": key, "value": value}));
        assert_eq!(get(home.path(), key)["value"], value, "{key}");
    }
    // `push` is reserved for v0.4.
    let err = engine.run_err(
        "app.preferences.set",
        json!({"key": "pcb.place.obstacle_mode", "value": "push"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn the_board_view_preferences_default_to_smart_track_ends_and_solid_net_colours() {
    use reda_engine::preferences::{NetColorPattern, NetColorZoomOut, Preferences, pages};
    let prefs = Preferences::default();
    assert!(prefs.pcb.ratsnest.smart_track_ends);
    assert_eq!(prefs.pcb.view.net_color_pattern, NetColorPattern::Solid);
    assert_eq!(prefs.pcb.view.net_color_zoom_out, NetColorZoomOut::NetColor);
    assert!(prefs.pcb.view.layer_sets.is_empty());
    for key in [
        "pcb.ratsnest.smart_track_ends",
        "pcb.view.net_color_pattern",
        "pcb.view.net_color_zoom_out",
        "pcb.view.layer_sets",
    ] {
        assert_eq!(pages::page_of(key), "pcb.view", "{key}");
    }
}

#[test]
fn break_wire_preferences_start_at_snap_to_segment_with_both_displays_always() {
    let home = TempDir::new("prefs-break-wire");
    let mut engine = test_engine_in(home.path());
    let out = engine.run_ok("app.preferences.get", json!({}));
    let break_wire = &out["values"]["schematic"]["break_wire"];
    assert_eq!(break_wire["cutting_length"], "segment");
    assert_eq!(break_wire["grid_multiple"], 2);
    // 10 default units of 10 mil.
    assert_eq!(break_wire["fixed_length"], 2_540_000);
    assert_eq!(break_wire["show_cutter_box"], "always");
    assert_eq!(break_wire["show_extremity_markers"], "always");
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.break_wire.show_cutter_box", "value": "on_wire"}),
    );
    assert!(read_prefs(home.path()).contains("show_cutter_box = \"on_wire\""));
    // Snap Grid Size Multiple runs from 2 to 10.
    for refused in [1, 11] {
        let err = engine.run_err(
            "app.preferences.set",
            json!({"key": "schematic.break_wire.grid_multiple", "value": refused}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{refused}");
    }
}

#[test]
fn checks_preferences_default_to_a_cross_pattern_and_the_override_colour_zoomed_out() {
    let home = TempDir::new("prefs-checks");
    let out = get(home.path(), "checks.overlay_style");
    assert_eq!(out["value"], "cross_pattern");
    let out = get(home.path(), "checks.overlay_zoomed_out");
    assert_eq!(out["value"], "override_color_dominates");
    assert_eq!(
        reda_engine::preferences::pages::page_of("checks.overlay_style"),
        "checks"
    );
}

#[test]
fn the_check_display_grid_has_details_on_for_all_and_overlay_on_three_checks() {
    use reda_engine::preferences::{CheckDisplays, Preferences};
    let grid = Preferences::default().checks.display;
    let checks = CheckDisplays::checks();
    assert!(checks.contains(&"dangling_wire") && checks.contains(&"clearance"));
    for check in &checks {
        let row = grid.get(check);
        assert!(row.details, "{check}");
        let on = ["clearance", "track_width", "courtyard_overlap"].contains(check);
        assert_eq!(row.overlay, on, "{check}");
    }
    // Each cell is a Preference of its own, written only when it differs.
    let home = TempDir::new("prefs-check-grid");
    let mut engine = test_engine_in(home.path());
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "checks.display.dangling_wire.overlay", "value": true}),
    );
    assert!(read_prefs(home.path()).contains("overlay = true"));
    let out = get(home.path(), "checks.display.dangling_wire.overlay");
    assert_eq!(out["value"], true);
    assert_eq!(
        get(home.path(), "checks.display.clearance.overlay")["value"],
        true
    );
    let err = engine.run(
        "app.preferences.set",
        json!({"key": "checks.display.no_such_check.overlay", "value": true}),
    );
    assert!(err.is_err());
}

#[test]
fn grid_tables_ship_per_document_type_and_take_edited_rows() {
    let home = TempDir::new("prefs-grid");
    let mut engine = test_engine_in(home.path());
    let mil = 25_400;
    let all = engine.run_ok("app.preferences.get", json!({}));
    let grid = all["values"]["editing"]["grid"].clone();
    // Sheet rows: Snap, Electrical Range, Visible (equal to Snap) and In cycle.
    assert_eq!(
        grid["schematic"]["imperial"],
        json!([
            {"snap": 10 * mil, "range": 8 * mil, "visible": 10 * mil, "in_cycle": true},
            {"snap": 25 * mil, "range": 20 * mil, "visible": 25 * mil, "in_cycle": true},
            {"snap": 50 * mil, "range": 40 * mil, "visible": 50 * mil, "in_cycle": true},
            {"snap": 100 * mil, "range": 80 * mil, "visible": 100 * mil, "in_cycle": true},
        ])
    );
    assert_eq!(grid["symbol"], grid["schematic"]);
    // Board rows have no range.
    assert_eq!(
        grid["pcb"]["metric"],
        json!([
            {"snap": 100_000, "visible": 100_000, "in_cycle": true},
            {"snap": 250_000, "visible": 250_000, "in_cycle": true},
            {"snap": 500_000, "visible": 500_000, "in_cycle": true},
            {"snap": 1_000_000, "visible": 1_000_000, "in_cycle": true},
        ])
    );
    assert_eq!(grid["pcb"]["imperial"][0]["snap"], 5 * mil);
    assert_eq!(grid["footprint"], grid["pcb"]);
    assert_eq!(grid["style"], "dots");
    assert_eq!(grid["coarse_multiplier"], 5);
    let hud = all["values"]["editing"]["hud"].clone();
    assert_eq!(
        hud,
        json!({"show": true, "follow_cursor": true, "xy": true, "delta": true,
               "layer": true, "grid": true})
    );
    assert_eq!(
        all["values"]["editing"]["display_precision"],
        json!({"mm": 3, "mil": 1})
    );

    // A custom row with lengths as text; a range above the step is kept, not clamped.
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "editing.grid.schematic.imperial", "value": [
            {"snap": "20mil", "range": "30mil", "visible": "100mil", "in_cycle": false},
        ]}),
    );
    assert_eq!(
        get(home.path(), "editing.grid.schematic.imperial")["value"],
        json!([{"snap": 20 * mil, "range": 30 * mil, "visible": 100 * mil, "in_cycle": false}])
    );
    for (key, value) in [
        ("editing.grid.coarse_multiplier", json!(3)),
        ("editing.grid.pcb.metric", json!([])),
        (
            "editing.grid.pcb.metric",
            json!([{"snap": 0, "visible": 1}]),
        ),
        ("editing.display_precision.mm", json!(9)),
    ] {
        let err = engine.run_err("app.preferences.set", json!({"key": key, "value": value}));
        assert_eq!(err.code, ErrorCode::InvalidParams, "{key} = {value}");
    }
}
