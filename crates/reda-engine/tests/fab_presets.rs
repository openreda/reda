//! Fab Presets: the shipped ones, user `.redafab` presets, Project copies and what
//! `project.new` makes of them.

use reda_engine::command::ErrorCode;
use reda_engine::model::geom::Nm;
use reda_engine::state;
use reda_testkit::{TempDir, TestEngine, test_engine, test_engine_in};
use serde_json::{Value, json};

fn new_project(engine: &mut TestEngine, dir: &TempDir, params: Value) -> Value {
    let mut params = params;
    params["path"] = json!(dir.path().join("work"));
    params["name"] = json!("Demo");
    engine.run_ok("project.new", params)
}

fn only_preset(engine: &TestEngine) -> reda_engine::model::ProjectFabPreset {
    let project = engine.project().unwrap();
    let presets: Vec<_> = project.fab_presets().cloned().collect();
    assert_eq!(presets.len(), 1);
    presets[0].clone()
}

#[test]
fn a_new_project_is_seeded_from_the_chosen_shipped_preset() {
    let dir = TempDir::new("fab-jlcpcb");
    let mut engine = test_engine();
    new_project(&mut engine, &dir, json!({"fab_preset": "jlcpcb"}));
    let project = engine.project().unwrap();
    let rules = project.pcb().unwrap().setup().rules;
    // JLCPCB's published 2-layer minimums (fab research, openreda/reda#4).
    assert_eq!(rules.track_width, Nm::from_um(100));
    assert_eq!(rules.clearance, Nm::from_um(100));
    assert_eq!(rules.via_hole, Nm::from_um(150));

    let preset = only_preset(&engine);
    assert_eq!(preset.name, "JLCPCB");
    assert!(
        preset.bom.iter().any(|c| c.header == "LCSC Part #"),
        "{:?}",
        preset.bom
    );
    let jobs: Vec<_> = project.output_jobs().collect();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].name, "JLCPCB");
    assert_eq!(jobs[0].fab_preset, preset.uuid);
    assert_eq!(jobs[0].pcb, project.pcb().unwrap().uuid);
    assert_eq!(jobs[0].destination(), "outputs/JLCPCB");
}

#[test]
fn a_default_via_on_a_board_from_any_shipped_preset_has_enough_annular_ring() {
    for preset in ["Generic", "JLCPCB", "PCBWay"] {
        let dir = TempDir::new("fab-via");
        let mut engine = test_engine();
        new_project(
            &mut engine,
            &dir,
            json!({"fab_preset": preset, "board": {"width": "20mm", "height": "20mm"}}),
        );
        engine.run_ok("pcb.via.add", json!({"at": {"x": "10mm", "y": "10mm"}}));
        let drc = engine.run_ok("drc.run", json!({}));
        let rings: Vec<_> = drc["violations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["check"] == "annular_ring")
            .collect();
        assert!(rings.is_empty(), "{preset}: {rings:?}");
    }
}

#[test]
fn the_generic_preset_is_conservative() {
    let dir = TempDir::new("fab-generic");
    let mut engine = test_engine();
    new_project(&mut engine, &dir, json!({}));
    let rules = engine.project().unwrap().pcb().unwrap().setup().rules;
    assert_eq!(rules.track_width, Nm::from_um(200));
    assert_eq!(rules.clearance, Nm::from_um(160));
    assert_eq!(rules.via_hole, Nm::from_um(300));
    assert_eq!(rules.edge_clearance, Nm::from_um(400));
    assert_eq!(only_preset(&engine).name, "Generic");
}

#[test]
fn a_new_project_can_start_with_a_rectangular_board() {
    let dir = TempDir::new("fab-board");
    let mut engine = test_engine();
    new_project(
        &mut engine,
        &dir,
        json!({"board": {"width": "100mm", "height": "80mm", "corner_radius": "2mm"}}),
    );
    let outline = engine.project().unwrap().pcb().unwrap().outline().unwrap();
    assert_eq!(outline.vertices.len(), 8);

    let err = test_engine().run_err(
        "project.new",
        json!({"path": dir.path().join("other"), "name": "Demo", "board": {"width": 0, "height": 10}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/board/width");
}

#[test]
fn an_unknown_preset_is_refused_and_writes_nothing() {
    let dir = TempDir::new("fab-unknown");
    let err = test_engine().run_err(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "fab_preset": "Nope"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert!(err.message.contains("JLCPCB"), "{}", err.message);
    assert!(!dir.path().join("Demo").exists());
}

#[test]
fn project_new_is_one_journal_entry_and_leaves_nothing_to_undo() {
    let dir = TempDir::new("fab-journal");
    let mut engine = test_engine();
    new_project(&mut engine, &dir, json!({}));
    assert_eq!(engine.journal().len(), 1);
    assert!(!engine.is_dirty());
    let err = engine.run_err("edit.undo", json!({"document": "project"}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}

#[test]
fn the_last_used_preset_is_remembered_and_listed_as_the_default() {
    let home = TempDir::new("fab-last-home");
    let dir = TempDir::new("fab-last");
    let mut engine = test_engine_in(home.path());
    let list = engine.run_ok("fab_preset.list", json!({}));
    assert_eq!(list["default"], "Generic");

    new_project(&mut engine, &dir, json!({"fab_preset": "PCBWay"}));
    let dirs = engine.user_dirs().unwrap().clone();
    assert_eq!(
        state::read(&dirs).last_fab_preset.as_deref(),
        Some("PCBWay")
    );
    let list = engine.run_ok("fab_preset.list", json!({}));
    assert_eq!(list["default"], "PCBWay");
    let names: Vec<&str> = list["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["preset"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Generic", "JLCPCB", "PCBWay"]);
}

#[test]
fn a_project_copy_saved_as_a_user_preset_can_seed_a_new_project() {
    let home = TempDir::new("fab-user-home");
    let dir = TempDir::new("fab-user");
    let mut engine = test_engine_in(home.path());
    new_project(&mut engine, &dir, json!({"fab_preset": "JLCPCB"}));
    let copy = only_preset(&engine);

    let out = engine.run_ok(
        "fab_preset.save_user",
        json!({"preset": copy.uuid, "name": "My JLC"}),
    );
    let path = home
        .path()
        .join("config")
        .join("fab-presets")
        .join("My JLC.redafab");
    assert_eq!(out["path"], json!(path));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with("{\n  \"format\": 1,\n  \"name\": \"My JLC\",\n"),
        "{text}"
    );

    let err = engine.run_err(
        "fab_preset.save_user",
        json!({"preset": copy.uuid, "name": "pcbway"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);

    let mut other = test_engine_in(home.path());
    let other_dir = TempDir::new("fab-user-2");
    new_project(&mut other, &other_dir, json!({"fab_preset": "my jlc"}));
    let seeded = only_preset(&other);
    assert_eq!(seeded.name, "My JLC");
    assert_eq!(seeded.rules, copy.rules);
    assert_eq!(seeded.bom, copy.bom);
}

#[test]
fn update_from_source_previews_then_replaces_the_copy() {
    let home = TempDir::new("fab-update-home");
    let dir = TempDir::new("fab-update");
    let mut engine = test_engine_in(home.path());
    new_project(&mut engine, &dir, json!({"fab_preset": "JLCPCB"}));
    let jlc = only_preset(&engine);
    engine.run_ok(
        "fab_preset.save_user",
        json!({"preset": jlc.uuid, "name": "Mine"}),
    );
    let out = engine.run_ok("fab_preset.add", json!({"name": "mine"}));
    let uuid = out["result"]["uuid"].clone();
    let copy = |engine: &TestEngine| {
        let uuid = uuid.as_str().unwrap().parse().unwrap();
        engine.project().unwrap().fab_preset(uuid).unwrap().clone()
    };
    assert_eq!(copy(&engine).name, "Mine");
    // The user edits their preset by hand.
    let path = home.path().join("config/fab-presets/Mine.redafab");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace("\"clearance\":100000", "\"clearance\":125000"),
    )
    .unwrap();

    let preview = engine.run_ok("fab_preset.source_diff", json!({"preset": uuid}));
    assert_eq!(
        preview["changes"],
        json!([{"field": "rules.clearance", "project": 100000, "source": 125000}])
    );
    // Previewing changes nothing.
    assert_eq!(copy(&engine).rules.clearance, Nm::from_um(100));

    engine.run_ok("fab_preset.update_from_source", json!({"preset": uuid}));
    assert_eq!(copy(&engine).rules.clearance, Nm::from_um(125));
    // The board keeps its own rules: the preset only seeds them.
    let board = engine.project().unwrap().pcb().unwrap().setup().rules;
    assert_eq!(board.clearance, Nm::from_um(100));

    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(copy(&engine).rules.clearance, Nm::from_um(100));
}

#[test]
fn a_copy_of_a_shipped_preset_updates_from_the_shipped_values() {
    let dir = TempDir::new("fab-update-shipped");
    let mut engine = test_engine();
    new_project(&mut engine, &dir, json!({"fab_preset": "PCBWay"}));
    let copy = only_preset(&engine);
    let preview = engine.run_ok("fab_preset.source_diff", json!({"preset": copy.uuid}));
    assert_eq!(preview["changes"], json!([]));
    let err = engine.run_err(
        "fab_preset.update_from_source",
        json!({"preset": "00000000-0000-4000-8000-0000000000ff"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn copies_can_be_added_and_unused_ones_deleted() {
    let dir = TempDir::new("fab-add");
    let mut engine = test_engine();
    new_project(&mut engine, &dir, json!({}));
    let generic = only_preset(&engine);
    let out = engine.run_ok("fab_preset.add", json!({"name": "JLCPCB"}));
    let jlc = out["result"]["uuid"].as_str().unwrap().to_owned();
    assert_eq!(engine.project().unwrap().fab_presets().count(), 2);

    let err = engine.run_err("fab_preset.delete", json!({"preset": generic.uuid}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("Generic"), "{}", err.message);
    engine.run_ok("fab_preset.delete", json!({"preset": jlc}));
    assert_eq!(engine.project().unwrap().fab_presets().count(), 1);
}

#[test]
fn copies_and_jobs_survive_save_and_open() {
    let dir = TempDir::new("fab-roundtrip");
    let mut engine = test_engine();
    new_project(&mut engine, &dir, json!({"fab_preset": "JLCPCB"}));
    let before = engine.project().unwrap().clone();
    let mut other = test_engine();
    other.run_ok(
        "project.open",
        json!({"path": dir.path().join("work/Demo")}),
    );
    assert_eq!(other.project().unwrap(), &before);
}
