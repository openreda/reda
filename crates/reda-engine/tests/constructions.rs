//! Starting boards from named physical builds and taking those builds to another Project.

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, test_engine};
use serde_json::json;

#[test]
fn project_new_selects_a_named_construction_and_refuses_a_wrong_count_before_writing() {
    let dir = TempDir::new("construction-choice");
    let mut engine = test_engine();
    let params = json!({"path":dir.path(), "name":"Demo", "construction":"2 layer"});
    let mut wrong = params.clone();
    wrong["layers"] = json!(3);
    let error = engine.run_err("project.new", wrong);
    assert_eq!(error.code, ErrorCode::InvalidParams);
    assert_eq!(error.details.unwrap()["pointer"], "/construction");
    assert!(!dir.path().join("Demo").exists());
    engine.run_ok("project.new", params);
    let stackup = serde_json::to_value(engine.project().unwrap().pcb().unwrap().stackup()).unwrap();
    assert_eq!(stackup["construction"], "2 Layer");
}

fn four_layer() -> serde_json::Value {
    json!({
        "default":true, "notes":"Nominal pressed thickness", "thickness":1_200_000,
        "entries":[{"copper":35000},{"kind":"prepreg","name":"Bond","thickness":120000,
            "material":"2116","dk":4100,"df":19000,"test_frequency":1_000_000_000},
            {"kind":"prepreg","thickness":70000},{"copper":18000},{"kind":"core"},
            {"copper":18000},{"kind":"prepreg","thickness":160000},{"copper":35000}],
        "via_types":[{"from":1,"to":4,"hole":250000,"diameter":{"start":550000}},
            {"name":"Laser","from":4,"to":3,"micro":true,"hole":90000,
                "diameter":{"start":210000,"end":190000},"unused_layers":"remove"}]
    })
}

#[test]
fn adding_a_board_from_a_project_copy_maps_the_default_build_and_drilling_positions() {
    let dir = TempDir::new("construction-document");
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path":dir.path(),"name":"Demo"}));
    let project = engine.project().unwrap();
    let preset = project.fab_presets().next().unwrap().uuid;
    let job = project.output_jobs().next().unwrap().uuid;
    let document = project.pcb().unwrap().uuid;
    engine.run_ok(
        "fab_preset.set",
        json!({"preset":preset,"values":{"constructions":{"4 Layer":null,"Four":four_layer()}}}),
    );
    engine.run_ok("output.job.delete", json!({"job":job}));
    engine.run_ok("document.delete", json!({"document":document}));
    engine.run_ok(
        "document.add",
        json!({"kind":"pcb","fab_preset":preset,"layers":4}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let stack = pcb.stackup();
    assert_eq!(stack.construction.as_deref(), Some("Four"));
    assert_eq!(stack.thickness.0, 1_200_000);
    assert_eq!(stack.entries.len(), 8);
    let sheet = stack.dielectrics().next().unwrap();
    assert_eq!(sheet.name.as_deref(), Some("Bond"));
    assert_eq!(sheet.material.as_deref(), Some("2116"));
    assert_eq!(
        (sheet.dk, sheet.df, sheet.test_frequency),
        (Some(4100), Some(19000), Some(1_000_000_000))
    );
    assert_eq!(stack.through_type().hole.0, 250000);
    let laser = &stack.via_types[1];
    assert_eq!(
        (pcb.copper_number(laser.from), pcb.copper_number(laser.to)),
        (Some(4), Some(3))
    );
    assert_eq!(laser.diameter.end.unwrap().0, 190000);
}

#[test]
fn saved_stackups_travel_through_a_user_preset_and_follow_source_updates_by_name() {
    let dir = TempDir::new("saved-construction");
    let mut engine = reda_testkit::test_engine_in(dir.path());
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"First","layers":4}),
    );
    let project = engine.project().unwrap();
    let copy = project.fab_presets().next().unwrap().uuid;
    let pcb = project.pcb().unwrap();
    let document = pcb.uuid;
    let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    let sheet = pcb.stackup().dielectrics().next().unwrap().uuid;
    engine.run_ok("pcb.stackup.set",json!({"dielectrics":[{"uuid":sheet,"material":"FR408HR","dk":3700,"df":9000,"test_frequency":2000000000}]}));
    engine.run_ok(
        "pcb.via_type.add",
        json!({"document":document,"from":layers[3],"to":layers[2],"micro":true,"name":"Laser"}),
    );
    engine.run_ok(
        "pcb.stackup.save_as_construction",
        json!({"document":document,"preset":copy,"name":"Custom"}),
    );
    let saved = engine.run_ok(
        "fab_preset.save_user",
        json!({"preset":copy,"name":"Workshop"}),
    );
    let file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(saved["path"].as_str().unwrap()).unwrap()).unwrap();
    let custom = file["constructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Custom")
        .unwrap();
    assert_eq!(custom["entries"][1]["material"], "FR408HR");
    assert_eq!(custom["entries"][1]["test_frequency"], 2000000000);
    assert_eq!(custom["via_types"][1]["from"], 4);
    assert_eq!(custom["via_types"][1]["to"], 3);
    engine.run_ok("project.new",json!({"path":dir.path(),"name":"Second","fab_preset":"Workshop","layers":4,"construction":"custom"}));
    let project = engine.project().unwrap();
    let copy = project.fab_presets().next().unwrap().uuid;
    let document = project.pcb().unwrap().uuid;
    let info = engine.run_ok("pcb.stackup.info", json!({"document":document}));
    assert_eq!(info["construction"], "Custom");
    assert_eq!(info["construction_matches"], true);
    engine.run_ok(
        "pcb.stackup.set",
        json!({"finish":"ENIG","mask_color":"green"}),
    );
    assert_eq!(
        engine.run_ok("pcb.stackup.info", json!({"document":document}))["construction_matches"],
        true
    );
    engine.run_ok(
        "fab_preset.set_user",
        json!({"name":"Workshop","values":{"constructions":{"Custom":null}}}),
    );
    engine.run_ok("fab_preset.update_from_source", json!({"preset":copy}));
    let info = engine.run_ok("pcb.stackup.info", json!({"document":document}));
    assert_eq!(info["construction"], "Custom");
    assert_eq!(info["construction_matches"], false);
}

#[test]
fn invalid_physical_builds_are_refused_before_they_enter_a_preset() {
    let dir = TempDir::new("invalid-physical-build");
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path":dir.path(),"name":"Demo"}));
    let copy = engine.project().unwrap().fab_presets().next().unwrap().uuid;
    let original = engine
        .project()
        .unwrap()
        .fab_preset(copy)
        .unwrap()
        .constructions
        .clone();
    for (path, value) in [
        ("/entries/1/thickness", json!(-1)),
        ("/entries/0/copper", json!(0)),
        ("/via_types/1/from", json!(5)),
        ("/via_types/1/hole", json!(900000)),
        ("/via_types/1/to", json!(1)),
    ] {
        let mut build = four_layer();
        *build.pointer_mut(path).unwrap() = value;
        let error = engine.run_err(
            "fab_preset.set",
            json!({"preset":copy,"values":{"constructions":{"4 Layer":null,"Bad":build}}}),
        );
        assert_eq!(error.code, ErrorCode::InvalidParams, "{path}");
        assert_eq!(
            engine
                .project()
                .unwrap()
                .fab_preset(copy)
                .unwrap()
                .constructions,
            original
        );
    }
}

#[test]
fn a_readded_board_can_use_the_source_while_the_project_keeps_its_edited_copy() {
    let dir = TempDir::new("construction-copy-and-source");
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path":dir.path(),"name":"Demo"}));
    let project = engine.project().unwrap();
    let original = project
        .pcb()
        .unwrap()
        .stackup()
        .construction
        .clone()
        .unwrap();
    let copy = project.fab_presets().next().unwrap().uuid;
    let job = project.output_jobs().next().unwrap().uuid;
    let mut board = project.pcb().unwrap().uuid;
    engine.run_ok("output.job.delete", json!({"job":job}));
    engine.run_ok(
        "fab_preset.set",
        json!({"preset":copy,"values":{"constructions":{(&original):null}}}),
    );
    for selection in [Some("Generic"), None] {
        engine.run_ok("document.delete", json!({"document":board}));
        let mut params = json!({"kind":"pcb"});
        if let Some(name) = selection {
            params["fab_preset"] = json!(name);
        }
        engine.run_ok("document.add", params);
        let project = engine.project().unwrap();
        let pcb = project.pcb().unwrap();
        assert_eq!(pcb.stackup().construction.as_ref(), Some(&original));
        assert!(project.construction_matches(pcb.stackup()));
        assert_eq!(project.fab_presets().count(), 2);
        assert!(
            project
                .fab_preset(copy)
                .unwrap()
                .constructions
                .iter()
                .all(|b| b.name != original)
        );
        board = pcb.uuid;
    }
}

#[test]
fn a_count_with_no_vendor_build_starts_from_generic_with_no_construction_name() {
    let dir = TempDir::new("construction-fallback");
    let mut engine = test_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Plain","layers":8}),
    );
    let plain = engine.project().unwrap().pcb().unwrap().stackup().clone();
    assert_eq!(plain.construction.as_deref(), Some("8 Layer"));
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Vendor","layers":8,"fab_preset":"JLCPCB"}),
    );
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    assert!(pcb.stackup().construction.is_none());
    let compare = reda_engine::model::Construction::from_stackup(String::new(), &plain);
    assert!(compare.matches_stackup(pcb.stackup()));
    assert_eq!(project.output_jobs().next().unwrap().name, "JLCPCB");
}

#[test]
fn applying_a_build_replaces_the_physical_stack_and_keeps_board_appearance_and_copper() {
    let dir = TempDir::new("apply-same");
    let mut e = test_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4}),
    );
    let project = e.project().unwrap();
    let preset = project.fab_presets().next().unwrap().uuid;
    let pcb = project.pcb().unwrap();
    let document = pcb.uuid;
    let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    e.run_ok(
        "pcb.layer.set",
        json!({"layer":layers[1],"role":"plane","name":"GND"}),
    );
    e.run_ok(
        "pcb.stackup.set",
        json!({"finish":"ENIG","mask_color":"blue","silk_color":"white"}),
    );
    e.run_ok(
        "fab_preset.set",
        json!({"preset":preset,"values":{"constructions":{"4 Layer":null,"Four":four_layer()}}}),
    );
    let before = e.project().unwrap().pcb().unwrap().clone();
    e.run_ok(
        "pcb.stackup.apply_construction",
        json!({"document":document,"preset":preset,"construction":"four"}),
    );
    let project = e.project().unwrap();
    let pcb = project.pcb().unwrap();
    assert_eq!(pcb.layers(), before.layers());
    let stack = pcb.stackup();
    assert!(project.construction_matches(stack));
    assert_eq!(stack.construction.as_deref(), Some("Four"));
    assert_eq!(stack.thickness.0, 1_200_000);
    assert_eq!(stack.entries.len(), 8);
    assert_eq!(
        stack.dielectrics().next().unwrap().test_frequency,
        Some(1_000_000_000)
    );
    assert_eq!(
        (
            stack.finish.as_deref(),
            stack.mask_color.as_deref(),
            stack.silk_color.as_deref()
        ),
        (Some("ENIG"), Some("blue"), Some("white"))
    );
    assert_eq!(stack.via_types, before.stackup().via_types);
    e.run_ok("edit.undo", json!({"document":document}));
    assert_eq!(e.project().unwrap().pcb().unwrap(), &before);
}

#[test]
fn optional_via_types_match_physical_spans_retain_sizes_and_report_newly_unmatched_vias() {
    let dir = TempDir::new("apply-types");
    let mut e = test_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4}),
    );
    let p = e.project().unwrap();
    let preset = p.fab_presets().next().unwrap().uuid;
    let pcb = p.pcb().unwrap();
    let document = pcb.uuid;
    let l: Vec<_> = pcb.copper_layers().map(|v| v.uuid).collect();
    let first = e.run_ok(
        "pcb.via_type.add",
        json!({"document":document,"from":l[0],"to":l[1]}),
    );
    let reverse = e.run_ok(
        "pcb.via_type.add",
        json!({"document":document,"from":l[2],"to":l[3]}),
    );
    for i in [0, 2] {
        e.run_ok("pcb.via.add",json!({"at":{"x":i as i64*1000,"y":0},"from":l[i],"to":l[i+1],"diameter":{"start":500000,"end":600000}}));
    }
    let class = e.run_ok("pcb.net_class.add", json!({"name":"Power"}));
    e.run_ok("pcb.rules.set",json!({"scope":{"net_class":class["result"]["uuid"]},"kind":"via_span","values":{"allowed":[first["result"]["uuid"]]}}));
    e.run_ok(
        "fab_preset.set",
        json!({"preset":preset,"values":{"constructions":{"4 Layer":null,"Four":four_layer()}}}),
    );
    let p = json!({"document":document,"preset":preset,"construction":"Four","via_types":true});
    let before = e.project().unwrap().pcb().unwrap().clone();
    let counts = e.run_ok(
        "pcb.stackup.change_preview",
        json!({"change":{"id":"pcb.stackup.apply_construction","params":p}}),
    );
    assert_eq!(counts["vias_without_type"], 1);
    assert_eq!(counts["via_types_changed"], 2);
    assert_eq!(counts["via_types_removed"], 1);
    assert_eq!(counts["classes_allowing_all"], json!(["Power"]));
    e.run_ok("pcb.stackup.apply_construction", p);
    let pcb = e.project().unwrap().pcb().unwrap();
    let stack = pcb.stackup();
    assert_eq!(
        stack.through_type().uuid,
        before.stackup().through_type().uuid
    );
    let laser = &stack.via_types[1];
    assert_eq!(json!(laser.uuid), reverse["result"]["uuid"]);
    assert_eq!(
        (laser.name.as_deref(), laser.from, laser.to),
        (Some("Laser"), l[3], l[2])
    );
    let via = pcb.vias().find(|v| v.span.joins(laser.span())).unwrap();
    assert_eq!(
        (via.span, via.diameter.start.0, via.diameter.end.unwrap().0),
        (laser.span(), 600000, 500000)
    );
    assert_eq!(
        pcb.vias()
            .filter(|v| stack.type_for_span(v.span).is_none())
            .count(),
        1
    );
    e.run_ok("edit.undo", json!({"document":document}));
    assert_eq!(e.project().unwrap().pcb().unwrap(), &before);
}

#[test]
fn fresh_and_format_one_boards_round_trip_after_applying_a_construction() {
    use reda_testkit::read_tree;
    use std::path::Path;
    for migrated in [false, true] {
        let dir = TempDir::new("apply-round-trip");
        let mut e = test_engine();
        let file = if migrated {
            let frozen = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/formats/v1");
            for (name, bytes) in read_tree(&frozen) {
                std::fs::write(dir.path().join(name), bytes).unwrap();
            }
            let file = dir.path().join("Demo.reda");
            e.run_ok("project.open", json!({"path":file}));
            file
        } else {
            e.run_ok("project.new", json!({"path":dir.path(),"name":"Demo"}));
            dir.path().join("Demo/Demo.reda")
        };
        let document = e.project().unwrap().pcb().unwrap().uuid;
        let preset = e.project().unwrap().fab_presets().next().unwrap().uuid;
        e.run_ok("fab_preset.set",json!({"preset":preset,"values":{"constructions":{"4 Layer":null,"Four":four_layer()}}}));
        e.run_ok(
            "pcb.stackup.apply_construction",
            json!({"document":document,"preset":preset,"construction":"Four","via_types":true}),
        );
        e.run_ok("project.save", json!({}));
        let bytes = std::fs::read(file.with_extension("redapcb")).unwrap();
        let mut reopened = test_engine();
        reopened.run_ok("project.open", json!({"path":file}));
        assert!(
            reopened
                .project()
                .unwrap()
                .construction_matches(reopened.project().unwrap().pcb().unwrap().stackup())
        );
        reopened.run_ok("project.save", json!({}));
        assert_eq!(
            std::fs::read(file.with_extension("redapcb")).unwrap(),
            bytes
        );
    }
}
