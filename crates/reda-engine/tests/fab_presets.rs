//! Fab Presets: the shipped ones, user `.redafab` presets, Project copies and what
//! `project.new` makes of them.

use reda_engine::Engine;
use reda_engine::command::ErrorCode;
use reda_engine::command::{FixedClock, SeededIds, UserDirs};
use reda_engine::model::geom::Nm;
use reda_engine::model::{RuleField, RuleKind};
use reda_engine::state;
use reda_testkit::{FolderTrash, TempDir, TestEngine, test_engine, test_engine_in};
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
    let pcb = project.pcb().unwrap();
    let value = |kind: RuleKind, field: RuleField| pcb.board_rule(kind).values.length(field);
    // JLCPCB's published 2-layer minimums (fab research, openreda/reda#4), and the
    // preferred seeds of `checks.md`.
    assert_eq!(
        value(RuleKind::TrackWidth, RuleField::Min),
        Some(Nm::from_um(100))
    );
    assert_eq!(
        value(RuleKind::TrackWidth, RuleField::Preferred),
        Some(Nm::from_um(250))
    );
    assert_eq!(
        value(RuleKind::Clearance, RuleField::Value),
        Some(Nm::from_um(100))
    );
    assert_eq!(
        value(RuleKind::ViaSize, RuleField::HoleMin),
        Some(Nm::from_um(150))
    );
    assert_eq!(
        value(RuleKind::ViaSize, RuleField::DiameterPreferred),
        Some(Nm::from_um(600))
    );
    assert_eq!(
        value(RuleKind::ViaSize, RuleField::HolePreferred),
        Some(Nm::from_um(300))
    );
    assert_eq!(
        value(RuleKind::HoleSize, RuleField::Min),
        Some(Nm::from_um(200))
    );
    assert_eq!(
        value(RuleKind::HoleSize, RuleField::Max),
        Some(Nm::from_um(6300))
    );

    let preset = only_preset(&engine);
    assert_eq!(preset.name, "JLCPCB");
    assert!(
        preset.bom.columns.iter().any(|c| c.header == "LCSC Part #"),
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

/// The JLCPCB seed Via (0.60/0.30 mm, a 0.15 mm ring) is below JLCPCB's 0.18 mm annular
/// ring minimum. Both values are as `checks.md` gives them; the conflict is left for the
/// check of unverified defaults (openreda/reda#260).
#[test]
fn a_default_via_on_a_generic_or_pcbway_board_has_enough_annular_ring() {
    for preset in ["Generic", "PCBWay"] {
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
    let pcb = engine.project().unwrap().pcb().unwrap();
    let value = |kind: RuleKind, field: RuleField| pcb.board_rule(kind).values.length(field);
    assert_eq!(
        value(RuleKind::TrackWidth, RuleField::Min),
        Some(Nm::from_um(200))
    );
    assert_eq!(
        value(RuleKind::Clearance, RuleField::Value),
        Some(Nm::from_um(160))
    );
    assert_eq!(
        value(RuleKind::ViaSize, RuleField::HoleMin),
        Some(Nm::from_um(300))
    );
    assert_eq!(
        value(RuleKind::EdgeClearance, RuleField::Value),
        Some(Nm::from_um(400))
    );
    assert_eq!(
        value(RuleKind::HoleSize, RuleField::Min),
        Some(Nm::from_um(500))
    );
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
    let board = engine.project().unwrap().pcb().unwrap();
    assert_eq!(
        board.board_rule(RuleKind::Clearance).values.value,
        Some(Nm::from_um(100))
    );

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

/// An engine whose per-user folders are under `home` and whose trash is `home/trash`, with
/// the user preset "My JLC" saved from a Project in `dir`.
fn with_user_preset(home: &TempDir, dir: &TempDir) -> TestEngine {
    let mut engine = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(home.path())))
            .starter_library(None)
            .trash(FolderTrash(home.path().join("trash")))
            .build(),
    );
    new_project(&mut engine, dir, json!({"fab_preset": "JLCPCB"}));
    let copy = only_preset(&engine);
    engine.run_ok(
        "fab_preset.save_user",
        json!({"preset": copy.uuid, "name": "My JLC"}),
    );
    engine
}

fn user_names(engine: &mut TestEngine) -> Vec<String> {
    let list = engine.run_ok("fab_preset.list", json!({}));
    list["presets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["origin"] == "user")
        .map(|p| p["preset"]["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_user_preset_is_renamed_with_its_file() {
    let (home, dir) = (TempDir::new("fab-rename-home"), TempDir::new("fab-rename"));
    let mut engine = with_user_preset(&home, &dir);
    // One component at a time: the path is compared as a JSON string.
    let folder = home.path().join("config").join("fab-presets");
    let out = engine.run_ok(
        "fab_preset.rename_user",
        json!({"name": "my jlc", "new_name": "JLC Fast"}),
    );
    assert_eq!(out["path"], json!(folder.join("JLC Fast.redafab")));
    assert!(!folder.join("My JLC.redafab").exists());
    assert_eq!(user_names(&mut engine), ["JLC Fast"]);

    for taken in ["PCBWay", "", "JLC/Fast"] {
        let err = engine.run_err(
            "fab_preset.rename_user",
            json!({"name": "JLC Fast", "new_name": taken}),
        );
        assert_eq!(err.code, ErrorCode::InvalidParams, "{taken:?}");
    }
    let err = engine.run_err(
        "fab_preset.rename_user",
        json!({"name": "Nope", "new_name": "Other"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn a_deleted_user_preset_goes_to_the_trash() {
    let (home, dir) = (TempDir::new("fab-delete-home"), TempDir::new("fab-delete"));
    let mut engine = with_user_preset(&home, &dir);
    engine.run_ok("fab_preset.delete_user", json!({"name": "My JLC"}));
    assert!(user_names(&mut engine).is_empty());
    assert!(home.path().join("trash/My JLC.redafab").exists());
    let err = engine.run_err("fab_preset.delete_user", json!({"name": "JLCPCB"}));
    assert_eq!(
        err.code,
        ErrorCode::NotFound,
        "a shipped preset is not the user's"
    );
}

#[test]
fn a_new_board_takes_its_stackup_thickness_and_copper_from_the_preset() {
    let home = TempDir::new("fab-stackup-home");
    let dir = TempDir::new("fab-stackup");
    let mut engine = with_user_preset(&home, &dir);
    let generic = engine.project().unwrap().pcb().unwrap().stackup().clone();
    // JLCPCB seeds 1.6 mm with 1 oz copper; finish and colours stay unset.
    assert_eq!(generic.thickness, Nm::from_um(1_600));
    assert!(generic.copper().all(|c| c.thickness == Nm::from_um(35)));

    let path = home.path().join("config/fab-presets/My JLC.redafab");
    let text = std::fs::read_to_string(&path).unwrap();
    let text = text.replace(
        "\"stackup\": {\"thickness\":1600000,\"copper\":35000}",
        "\"stackup\": {\"thickness\":800000,\"copper\":70000}",
    );
    std::fs::write(&path, text).unwrap();

    let other = TempDir::new("fab-stackup-2");
    new_project(&mut engine, &other, json!({"fab_preset": "My JLC"}));
    let stackup = engine.project().unwrap().pcb().unwrap().stackup().clone();
    assert_eq!(stackup.thickness, Nm::from_um(800));
    let copper: Vec<Nm> = stackup.copper().map(|c| c.thickness).collect();
    assert_eq!(copper, [Nm::from_um(70), Nm::from_um(70)]);
    assert_eq!(stackup.dielectrics().count(), 1);
    assert_eq!(only_preset(&engine).stackup.copper, Some(Nm::from_um(70)));
    // The copy keeps the seeds, and they come back after save and open.
    let folder = other.path().join("work/Demo");
    engine.run_ok("project.save", json!({}));
    let mut reopened = test_engine();
    reopened.run_ok("project.open", json!({"path": folder}));
    assert_eq!(
        only_preset(&reopened).stackup.thickness,
        Some(Nm::from_um(800))
    );
    let board = reopened.project().unwrap().pcb().unwrap().stackup().clone();
    assert_eq!(board, stackup);
}

#[test]
fn shipped_presets_describe_the_fabs_complete_output_conventions() {
    let mut engine = test_engine();
    let list = engine.run_ok("fab_preset.list", json!({}));
    for (name, origin, required, mount, unplaced, rotation, suffix, tht) in [
        (
            "Generic",
            "absolute",
            json!([]),
            "SMT",
            true,
            "plus_180",
            false,
            false,
        ),
        (
            "JLCPCB",
            "absolute",
            json!(["LCSC"]),
            "SMT",
            false,
            "negate",
            false,
            false,
        ),
        (
            "PCBWay",
            "relative",
            json!(["MPN"]),
            "SMD",
            false,
            "plus_180",
            true,
            true,
        ),
    ] {
        let preset = &list["presets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["preset"]["name"] == name)
            .unwrap()["preset"];
        assert_eq!(preset["gerber"]["file_origin"], origin, "{name}");
        assert_eq!(preset["gerber"]["naming"], "classic");
        assert_eq!(preset["gerber"]["slot_command"], "route");
        assert_eq!(preset["gerber"]["merge_drill"], false);
        assert_eq!(preset["gerber"]["outline_on_every_plot"], false);
        assert_eq!(preset["gerber"]["file_names"], json!([]));
        assert_eq!(preset["bom"]["required_fields"], required);
        assert_eq!(
            preset["bom"]["mount_values"],
            json!({"smt": mount, "tht": "THT"})
        );
        assert_eq!(preset["bom"]["list_unplaced"], unplaced);
        assert_eq!(preset["bom"]["layout"], "grouped");
        assert_eq!(
            preset["bom"]["group_by"],
            json!(["part", "footprint", "fields"])
        );
        assert_eq!(preset["bom"]["sort"], json!(["designator"]));
        assert_eq!(preset["bom"]["designator_separator"], ",");
        assert_eq!(preset["bom"]["not_fitted"], "omit");
        assert_eq!(
            preset["bom"]["line_numbers"],
            json!({"enabled": name == "PCBWay", "start": 1, "step": 1})
        );
        assert_eq!(preset["cpl"]["bottom_rotation"], rotation);
        assert_eq!(preset["cpl"]["unit_suffix"], suffix);
        assert_eq!(preset["cpl"]["exclude_through_hole"], tht);
        assert_eq!(preset["cpl"]["units"], "mm");
        assert_eq!(preset["cpl"]["bottom_position"], "top_view");
        assert_eq!(
            preset["cpl"]["side_values"],
            json!({"top": "Top", "bottom": "Bottom"})
        );
        assert_eq!(preset["zip_assembly"], false);
        let headers: Vec<_> = preset["bom"]["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["header"].as_str().unwrap())
            .collect();
        let expected = match name {
            "Generic" => vec![
                "Designator",
                "Quantity",
                "Value",
                "Footprint",
                "Manufacturer",
                "MPN",
            ],
            "JLCPCB" => vec!["Comment", "Designator", "Footprint", "LCSC Part #"],
            _ => vec![
                "Item #",
                "Designator",
                "Qty",
                "Manufacturer",
                "Mfg Part #",
                "Description / Value",
                "Package/Footprint",
                "Type",
            ],
        };
        assert_eq!(headers, expected);
        let cpl_headers: Vec<_> = preset["cpl"]["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["header"].as_str().unwrap())
            .collect();
        assert_eq!(
            cpl_headers,
            if name == "PCBWay" {
                vec![
                    "Designator",
                    "Footprint",
                    "Mid X",
                    "Mid Y",
                    "Ref X",
                    "Ref Y",
                    "Pad X",
                    "Pad Y",
                    "TB",
                    "Rotation",
                    "Comment",
                ]
            } else {
                vec!["Designator", "Mid X", "Mid Y", "Layer", "Rotation"]
            }
        );
        let suggestions = &preset["stackup_suggestions"];
        assert_eq!(
            suggestions["thickness"],
            if name == "PCBWay" {
                json!([
                    200000, 400000, 600000, 800000, 1000000, 1200000, 1600000, 2000000, 2400000,
                    2600000, 2800000, 3000000, 3200000
                ])
            } else {
                json!([400000, 600000, 800000, 1000000, 1200000, 1600000, 2000000])
            }
        );
        assert_eq!(
            suggestions["copper"],
            match name {
                "JLCPCB" => json!([35000, 70000, 87500, 122500, 157500]),
                "PCBWay" => json!([35000, 70000, 105000, 140000, 175000, 210000, 245000, 280000]),
                _ => json!([35000, 70000]),
            }
        );
        assert_eq!(
            suggestions["finish"],
            if name == "PCBWay" {
                json!([
                    "HASL with lead",
                    "HASL lead free",
                    "Immersion Gold",
                    "OSP",
                    "Hard Gold",
                    "Immersion Silver",
                    "Immersion Tin",
                    "ENEPIG"
                ])
            } else {
                json!(["HASL (leaded)", "HASL (lead-free)", "ENIG", "OSP"])
            }
        );
        assert_eq!(
            suggestions["mask_color"],
            if name == "PCBWay" {
                json!([
                    "Green",
                    "Red",
                    "Yellow",
                    "Blue",
                    "White",
                    "Black",
                    "Matt green",
                    "Matte black",
                    "Purple",
                    "none"
                ])
            } else {
                json!(["Green", "Purple", "Red", "Yellow", "Blue", "White", "Black"])
            }
        );
        assert_eq!(
            suggestions["silk_color"],
            if name == "PCBWay" {
                json!(["White", "Black", "Yellow", "None"])
            } else {
                json!([])
            }
        );
        if name == "Generic" {
            assert!(preset.get("stackup").is_none());
        } else {
            assert_eq!(
                preset["stackup"],
                json!({"thickness":1600000,"copper":35000})
            );
        }
    }
}

#[test]
fn custom_output_conventions_survive_user_and_project_file_roundtrips() {
    let (home, dir) = (
        TempDir::new("fab-conventions-home"),
        TempDir::new("fab-conventions"),
    );
    let mut engine = with_user_preset(&home, &dir);
    let source = home.path().join("config/fab-presets/My JLC.redafab");
    let mut file: Value = serde_json::from_slice(&std::fs::read(&source).unwrap()).unwrap();
    file["gerber"] = json!({"x2_attributes":false,"naming":"descriptive","file_origin":"relative",
        "slot_command":"g85","merge_drill":true,"outline_on_every_plot":true,
        "file_names":[{"layer":"Top Copper","name":"top.gbr"}]});
    file["bom"]["layout"] = json!("flat");
    file["bom"]["group_by"] = json!(["part","footprint","fields","value",{"field":"MPN"}]);
    file["bom"]["sort"] = json!(["quantity", "designator"]);
    file["bom"]["line_numbers"] = json!({"enabled":true,"start":5,"step":10});
    file["bom"]["not_fitted"] = json!("list");
    file["bom"]["designator_separator"] = json!("; ");
    file["bom"]["columns"] = json!([{"header":"Fitted","source":"fitted"}]);
    file["cpl"]["columns"] = json!([
        {"header":"CX","source":"centre_x"},{"header":"CY","source":"centre_y"},
        {"header":"OX","source":"origin_x"},{"header":"OY","source":"origin_y"},
        {"header":"P1X","source":"pad1_x"},{"header":"P1Y","source":"pad1_y"},
        {"header":"Side","source":"side"}]);
    file["cpl"]["side_values"] = json!({"top":"T","bottom":"B"});
    file["cpl"]["units"] = json!("in");
    file["cpl"]["unit_suffix"] = json!(true);
    file["cpl"]["bottom_rotation"] = json!("stored");
    file["cpl"]["bottom_position"] = json!("negate_x");
    file["cpl"]["exclude_through_hole"] = json!(true);
    file["zip_assembly"] = json!(true);
    std::fs::write(&source, serde_json::to_vec_pretty(&file).unwrap()).unwrap();
    let destination = TempDir::new("fab-conventions-project");
    new_project(&mut engine, &destination, json!({"fab_preset":"My JLC"}));
    let copy = only_preset(&engine);
    let mut expected = file.clone();
    expected.as_object_mut().unwrap().remove("format");
    assert_eq!(serde_json::to_value(copy.preset()).unwrap(), expected);
    engine.run_ok(
        "fab_preset.save_user",
        json!({"preset":copy.uuid,"name":"Roundtrip"}),
    );
    let saved: Value = serde_json::from_slice(
        &std::fs::read(home.path().join("config/fab-presets/Roundtrip.redafab")).unwrap(),
    )
    .unwrap();
    file["name"] = json!("Roundtrip");
    assert_eq!(saved, file);
    engine.run_ok("project.save", json!({}));
    let mut reopened = test_engine();
    reopened.run_ok(
        "project.open",
        json!({"path": destination.path().join("work/Demo")}),
    );
    assert_eq!(only_preset(&reopened), copy);
}
