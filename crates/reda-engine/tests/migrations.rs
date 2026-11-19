//! The migration chain: raw-JSON steps that run before typed deserialisation.
//!
//! Release inputs live under `tests/projects/formats/v<n>`. Their canonical successors
//! live beside them in `migrated-v<n+1>`. The standalone codec tests in `format::v2`
//! exercise the same file pairs for Library files, user presets and Rules Files.
//! The frozen Demo likewise has a canonical migrated successor.

use std::path::{Path, PathBuf};

use reda_engine::format::{FORMAT, FileKind, MIGRATIONS, Migration, migrate, migrate_with};
use reda_testkit::{TempDir, assert_golden, read_tree, test_engine};
use serde_json::{Value, json};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/migrations");
const FORMATS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/formats");
const CURRENT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/formats/migrated-v2"
);

#[test]
fn every_frozen_format_opens_and_saves_as_the_current_goldens() {
    for n in 1..FORMAT {
        let frozen = PathBuf::from(FORMATS).join(format!("v{n}"));
        let files = read_tree(&frozen);
        assert!(
            !files.is_empty(),
            "no frozen Project in {}",
            frozen.display()
        );

        let dir = TempDir::new(&format!("frozen-v{n}"));
        for (rel, bytes) in &files {
            std::fs::write(dir.path().join(rel), bytes).unwrap();
        }
        let mut engine = test_engine();
        engine.run_ok(
            "project.open",
            json!({"path": dir.path().join("Demo.reda")}),
        );
        engine.run_ok("project.save", json!({}));
        let saved = read_tree(dir.path());
        for (rel, current) in read_tree(Path::new(CURRENT)) {
            assert_eq!(saved[&rel], current, "v{n}: {rel}");
        }
    }
}

/// A stand-in step from format 1 to 2: renames each Document row's `path` to `file`.
fn scaffold_step(kind: FileKind, mut value: Value) -> Result<Value, String> {
    if kind == FileKind::Project {
        let rows = value["documents"]
            .as_array_mut()
            .ok_or("\"documents\" is not a list")?;
        for row in rows {
            let row = row
                .as_object_mut()
                .ok_or("a Document row is not an object")?;
            let path = row.remove("path").ok_or("a Document row has no \"path\"")?;
            row.insert("file".into(), path);
        }
    }
    Ok(value)
}

const SCAFFOLD: &[Migration] = &[scaffold_step];

/// Runs `steps` on `<dir>/<ext>.before.json` and compares the result with
/// `<dir>/<ext>.after.json`.
fn check_step_golden(dir: &Path, steps: &[Migration], kind: FileKind) {
    let before = dir.join(format!("{}.before.json", kind.extension()));
    let bytes = std::fs::read(&before)
        .unwrap_or_else(|e| panic!("every migration step needs {}: {e}", before.display()));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let after = migrate_with(kind, value, steps).unwrap();
    let mut text = serde_json::to_string_pretty(&after).unwrap();
    text.push('\n');
    assert_golden(
        dir.join(format!("{}.after.json", kind.extension())),
        text.as_bytes(),
    );
}

#[test]
fn the_scaffold_step_matches_its_goldens() {
    for kind in FileKind::ALL {
        check_step_golden(&PathBuf::from(GOLDEN).join("scaffold"), SCAFFOLD, kind);
    }
}

#[test]
fn every_real_step_has_goldens_for_every_file_kind() {
    for (i, _) in MIGRATIONS.iter().enumerate() {
        let n = i + 1;
        let steps = &MIGRATIONS[..n];
        for kind in FileKind::ALL {
            if n == 1 {
                check_release_step(kind, steps);
            } else {
                let dir = Path::new(GOLDEN).join(format!("v{n}-to-v{}", n + 1));
                check_step_golden(&dir, steps, kind);
            }
        }
    }
}

fn check_release_step(kind: FileKind, steps: &[Migration]) {
    let file = match kind {
        FileKind::Project => "blinky_555/Blinky.reda",
        FileKind::Pcb => "blinky_555/Blinky.redapcb",
        FileKind::Sheet => "blinky_555/Blinky.redasch",
        FileKind::Library => "standalone/Starter.redalib",
        FileKind::Symbol => "standalone/Resistor.redasym",
        FileKind::Footprint => "standalone/CAPC1005X40N.redafp",
        FileKind::Part => "standalone/Resistor.redapart",
        FileKind::FabPreset => "standalone/Blinky.redafab",
        FileKind::Rules => "standalone/Blinky.redarules",
    };
    let root = Path::new(V1);
    let read = |path| serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap();
    let original = read(root.join(file));
    let mut expected = read(root.parent().unwrap().join("migrated-v2").join(file));
    let mut migrated = migrate_with(kind, original.clone(), steps).unwrap();

    // A typed save writes default severities even for the new checks omitted in v1.
    // The raw step leaves the user's severity table alone. The writer also sorts PCB
    // rule records by UUID.
    if matches!(kind, FileKind::Pcb | FileKind::Rules) {
        expected["severities"] = original["severities"].clone();
    }
    if kind == FileKind::Rules {
        // The canonical writer includes the two copper Layers implicit in every v1 file.
        assert_eq!(
            expected.as_object_mut().unwrap().remove("copper_layers"),
            Some(json!(2))
        );
    }
    if kind == FileKind::Pcb {
        migrated["rules"]
            .as_array_mut()
            .unwrap()
            .sort_by(|a, b| a["uuid"].as_str().cmp(&b["uuid"].as_str()));
    }
    assert_eq!(migrated, expected, "{file}");
}

#[test]
fn a_chain_stamps_the_version_it_reaches() {
    let v1 = json!({"format": 1, "sheet": {"uuid": "00000000-0000-4000-8000-000000000002"}});
    let out = migrate_with(FileKind::Sheet, v1, SCAFFOLD).unwrap();
    assert_eq!(out["format"], 2);
}

#[test]
fn a_chain_only_runs_the_steps_after_the_files_version() {
    let v2 = json!({"format": 2, "documents": [{"file": "Demo.redasch"}]});
    let out = migrate_with(FileKind::Project, v2.clone(), SCAFFOLD).unwrap();
    assert_eq!(out, v2);
}

#[test]
fn a_failing_step_names_the_format_it_migrates_from() {
    let broken = json!({"format": 1, "documents": "nope"});
    let err = migrate_with(FileKind::Project, broken, SCAFFOLD).unwrap_err();
    assert!(err.contains("format 1"), "{err}");
    assert!(err.contains("not a list"), "{err}");
}

#[test]
fn a_step_that_returns_a_non_object_is_an_error_not_a_panic() {
    fn bad(_: FileKind, _: Value) -> Result<Value, String> {
        Ok(json!([1, 2]))
    }
    let err = migrate_with(FileKind::Sheet, json!({"format": 1}), &[bad]).unwrap_err();
    assert!(err.contains("object"), "{err}");
}

#[test]
fn newer_formats_are_refused_with_a_clear_message() {
    let err = migrate(FileKind::Sheet, json!({"format": FORMAT + 1})).unwrap_err();
    assert!(err.contains("newer REDA"), "{err}");
}

#[test]
fn files_without_a_format_or_with_format_zero_are_refused() {
    for value in [
        json!({}),
        json!({"format": 0}),
        json!({"format": "1"}),
        json!([]),
    ] {
        assert!(
            migrate(FileKind::Project, value.clone()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn every_released_library_kind_moves_to_format_two_without_changing_contents() {
    for (kind, file) in [
        (FileKind::Library, "standalone/Starter.redalib"),
        (FileKind::Symbol, "standalone/Resistor.redasym"),
        (FileKind::Footprint, "standalone/CAPC1005X40N.redafp"),
        (FileKind::Part, "standalone/Resistor.redapart"),
        (FileKind::Sheet, "blinky_555/Blinky.redasch"),
    ] {
        let before = std::fs::read(Path::new(V1).join(file)).unwrap();
        let mut expected: Value = serde_json::from_slice(&before).unwrap();
        let migrated = migrate(kind, expected.clone()).unwrap();
        expected["format"] = json!(2);
        assert_eq!(migrated, expected, "{}", kind.extension());
    }
}

const V1: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/projects/formats/v1"
);

proptest::proptest! {
    #[test]
    fn missing_preset_seeds_default_independently_in_files_and_in_each_project_copy(
        thickness in proptest::option::of(400_000i64..3_000_000),
        copper in proptest::option::of(10_000i64..210_000),
    ) {
        let bytes = std::fs::read(Path::new(V1).join("standalone/Blinky.redafab")).unwrap();
        let mut preset: Value = serde_json::from_slice(&bytes).unwrap();
        let mut seeds = json!({});
        if let Some(n) = thickness { seeds["thickness"] = json!(n); }
        if let Some(n) = copper { seeds["copper"] = json!(n); }
        preset["stackup"] = seeds;
        let mut project: Value = serde_json::from_slice(&std::fs::read(Path::new(V1).join("minimal_board/Minimal.reda")).unwrap()).unwrap();
        let mut embedded = project["fab_presets"][0].clone();
        embedded["stackup"] = preset["stackup"].clone();
        embedded["rules"] = preset["rules"].clone();
        let mut empty = embedded.clone();
        empty.as_object_mut().unwrap().remove("stackup");
        project["fab_presets"] = json!([empty, embedded]);
        let migrated = migrate(FileKind::Project, project).unwrap();
        let standalone = migrate(FileKind::FabPreset, preset).unwrap();
        let construction = &standalone["constructions"][0];
        assert_eq!(construction["thickness"], thickness.unwrap_or(1_600_000));
        let mut entries = construction["entries"].as_array().unwrap().iter();
        assert_eq!(entries.next().unwrap()["copper"], copper.unwrap_or(35_000));
        assert_eq!(entries.next().unwrap(), &json!({"kind":"core"}));
        assert_eq!(entries.next().unwrap()["copper"], copper.unwrap_or(35_000));
        assert_eq!(migrated["fab_presets"][1]["constructions"], standalone["constructions"]);
        assert_eq!(migrated["fab_presets"][1]["rules"], standalone["rules"]);
        assert_eq!(migrated["fab_presets"][0]["constructions"][0]["thickness"], 1_600_000);
        assert_eq!(migrated["fab_presets"][0]["constructions"][0]["entries"][0]["copper"], 35_000);
    }

    #[test]
    fn migrating_a_board_moves_only_the_board_preferences_into_its_through_type(
        start in 510_000i64..5_000_000,
        hole in 150_000i64..500_001,
        class_hole in 150_000i64..400_000,
    ) {
        let original = std::fs::read(Path::new(V1).join("blinky_555/Blinky.redapcb")).unwrap();
        let mut pcb: Value = serde_json::from_slice(&original).unwrap();
        pcb["net_classes"].as_array_mut().unwrap().push(json!({"uuid":"afffffff-0000-4000-8000-000000000002","name":"Power","rank":1}));
        let rules = pcb["rules"].as_array_mut().unwrap();
        let board = rules.iter_mut().find(|r| r["kind"] == "via_size" && r["scope"] == json!({})).unwrap();
        board["values"]["diameter"]["preferred"] = json!(start);
        board["values"]["hole"]["preferred"] = json!(hole);
        let class = json!({"uuid":"afffffff-0000-4000-8000-000000000001", "kind":"via_size",
            "scope":{"net_class":"afffffff-0000-4000-8000-000000000002"}, "values":{"hole":{"preferred":class_hole}}});
        rules.push(class.clone());

        let migrated = migrate(FileKind::Pcb, pcb).unwrap();
        let stack = &migrated["stackups"][0];
        let through = &stack["via_types"][0];
        assert_eq!(through["from"], stack["entries"][0]["layer"]);
        assert_eq!(through["to"], stack["entries"][2]["layer"]);
        assert_eq!(through["diameter"], json!({"start":start}));
        assert_eq!(through["hole"], hole);
        assert!(through.get("name").is_none() && stack.get("construction").is_none());
        assert_eq!(stack["entries"][1]["kind"], "core");
        let rules = migrated["rules"].as_array().unwrap();
        let via_size = rules.iter().find(|r| r["kind"] == "via_size" && r["scope"] == json!({})).unwrap();
        assert!(via_size["values"]["diameter"].get("preferred").is_none());
        assert!(via_size["values"]["hole"].get("preferred").is_none());
        assert!(rules.contains(&class));
        assert!(rules.iter().all(|r| r["scope"].get("layers").is_none() && r["scope"].get("net_class_pair").is_none()));
        assert!(stack["entries"][1].get("material").is_none());
        assert!(stack["entries"][1].get("dk").is_none());
        assert!(stack["entries"][1].get("df").is_none());
        assert!(stack["entries"][1].get("test_frequency").is_none());
    }
}

proptest::proptest! {
    // Each case opens and saves a whole Project twice.
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    /// A v1 Project with varied Vias, dielectric, preferences and preset seeds opens with
    /// every v0.2 field at its migrated value, and save → load → save of the result is
    /// byte-identical.
    #[test]
    fn a_migrated_v1_project_gains_the_v2_fields_and_saves_back_identically(
        vias in proptest::collection::vec((510_000i64..5_000_000, 150_000i64..500_000), 1..4),
        dielectric in proptest::option::of(100_000i64..1_500_000),
        start in 510_000i64..5_000_000,
        hole in 150_000i64..500_001,
        seeds in (proptest::option::of(400_000i64..3_000_000), proptest::option::of(10_000i64..210_000)),
    ) {
        let mut files = read_tree(&Path::new(V1).join("blinky_555"));
        let mut pcb: Value = serde_json::from_slice(&files["Blinky.redapcb"]).unwrap();
        for (via, (d, h)) in pcb["vias"].as_array_mut().unwrap().iter_mut().zip(vias.iter().cycle()) {
            via["diameter"] = json!(d);
            via["hole"] = json!(h);
        }
        let sheet = &mut pcb["stackups"][0]["entries"][1];
        match dielectric {
            Some(t) => sheet["thickness"] = json!(t),
            None => { sheet.as_object_mut().unwrap().remove("thickness"); }
        }
        let rules = pcb["rules"].as_array_mut().unwrap();
        let board = rules.iter_mut().find(|r| r["kind"] == "via_size" && r["scope"] == json!({})).unwrap();
        board["values"]["diameter"]["preferred"] = json!(start);
        board["values"]["hole"]["preferred"] = json!(hole);
        let original = pcb.clone();
        files.insert("Blinky.redapcb".into(), serde_json::to_vec(&pcb).unwrap());
        let mut project: Value = serde_json::from_slice(&files["Blinky.reda"]).unwrap();
        let mut stackup = json!({});
        if let Some(n) = seeds.0 { stackup["thickness"] = json!(n); }
        if let Some(n) = seeds.1 { stackup["copper"] = json!(n); }
        project["fab_presets"][0]["stackup"] = stackup;
        files.insert("Blinky.reda".into(), serde_json::to_vec(&project).unwrap());

        type Files = std::collections::BTreeMap<String, Vec<u8>>;
        let load = |files: &Files| {
            let folder = PathBuf::from("Demo");
            reda_engine::store::load_from(&folder.join("Blinky.reda"), |path: &Path| {
                let rel = path.strip_prefix(&folder).unwrap().to_str().unwrap();
                files.get(rel).cloned().ok_or_else(|| reda_engine::format::LoadError::Missing(path.to_owned()))
            }).unwrap().0
        };
        let save = |project: &reda_engine::model::Project| -> Files {
            reda_engine::store::encode(project).unwrap().into_iter().collect()
        };
        let first = save(&load(&files));
        proptest::prop_assert_eq!(&save(&load(&first)), &first);

        let saved: Value = serde_json::from_slice(&first["Blinky.redapcb"]).unwrap();
        let (before, after) = (original["vias"].as_array().unwrap(), saved["vias"].as_array().unwrap());
        proptest::prop_assert_eq!(before.len(), after.len());
        for before in before {
            let after = after.iter().find(|v| v["uuid"] == before["uuid"]).unwrap();
            proptest::prop_assert_eq!(&after["diameter"], &json!({"start": before["diameter"]}));
            proptest::prop_assert_eq!(&after["hole"], &before["hole"]);
            for key in ["unused_layers", "zone_connection", "teardrop"] {
                proptest::prop_assert!(after.get(key).is_none(), "{}", key);
            }
        }
        for layer in saved["layers"]["layers"].as_array().unwrap() {
            proptest::prop_assert!(layer.get("name").is_none() && layer.get("role").is_none());
        }
        let stack = &saved["stackups"][0];
        let types = stack["via_types"].as_array().unwrap();
        proptest::prop_assert_eq!(types.len(), 1);
        proptest::prop_assert_eq!(&types[0]["diameter"], &json!({"start": start}));
        proptest::prop_assert_eq!(&types[0]["hole"], &json!(hole));
        for key in ["micro", "name", "unused_layers"] {
            proptest::prop_assert!(types[0].get(key).is_none(), "{}", key);
        }
        let sheet = &stack["entries"][1];
        proptest::prop_assert_eq!(&sheet["kind"], "core");
        proptest::prop_assert_eq!(sheet.get("thickness").cloned(), dielectric.map(|t| json!(t)));
        let rules = saved["rules"].as_array().unwrap();
        for added in reda_engine::pcb::multilayer_rules(|_| reda_engine::model::Uuid::NIL) {
            let kind = json!(added.kind);
            let record = rules.iter().find(|r| r["kind"] == kind && r["scope"] == json!({}));
            proptest::prop_assert!(record.is_some(), "no board-wide {}", kind);
            proptest::prop_assert_eq!(&record.unwrap()["values"], &json!(added.values));
        }
        proptest::prop_assert!(rules.iter().all(|r| r["scope"].get("layers").is_none() && r["scope"].get("net_class_pair").is_none()));

        let project: Value = serde_json::from_slice(&first["Blinky.reda"]).unwrap();
        let preset = &project["fab_presets"][0];
        proptest::prop_assert_eq!(&preset["placement_corrections"], &json!([]));
        proptest::prop_assert_eq!(&preset["rules"]["clearance_hole"], &json!(reda_engine::model::HoleClearance::GENERIC));
        let construction = &preset["constructions"].as_array().unwrap()[0];
        proptest::prop_assert_eq!(&construction["thickness"], &json!(seeds.0.unwrap_or(1_600_000)));
        let entries = construction["entries"].as_array().unwrap();
        proptest::prop_assert_eq!(entries.len(), 3);
        proptest::prop_assert_eq!(&entries[0]["copper"], &json!(seeds.1.unwrap_or(35_000)));
        proptest::prop_assert_eq!(&entries[1], &json!({"kind": "core"}));
    }
}

#[test]
fn a_released_via_keeps_its_own_size_after_save_and_reopen() {
    let dir = TempDir::new("released-via");
    let source = Path::new(V1).join("usb_c_mcu");
    for (rel, bytes) in read_tree(&source) {
        std::fs::write(dir.path().join(rel), bytes).unwrap();
    }
    let file = dir.path().join("UsbMcu.redapcb");
    let pcb: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let original = &pcb["vias"].as_array().unwrap()[0];
    for _ in 0..2 {
        let mut engine = test_engine();
        engine.run_ok(
            "project.open",
            json!({"path":dir.path().join("UsbMcu.reda")}),
        );
        engine.run_ok("project.save", json!({}));
        let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let via = saved["vias"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["uuid"] == original["uuid"])
            .unwrap();
        assert_eq!(via["diameter"], json!({"start":original["diameter"]}));
        assert_eq!(via["hole"], original["hole"]);
        assert_eq!(via["span"], original["span"]);
        for key in ["unused_layers", "zone_connection", "teardrop", "via_type"] {
            assert!(via.get(key).is_none(), "{key}");
        }
    }
}

#[test]
fn standalone_and_embedded_presets_get_the_same_construction_and_hole_clearances() {
    for (path, kind) in [
        ("standalone/Blinky.redafab", FileKind::FabPreset),
        ("minimal_board/Minimal.reda", FileKind::Project),
    ] {
        let bytes = std::fs::read(Path::new(V1).join(path)).unwrap();
        let file = serde_json::from_slice(&bytes).unwrap();
        let out = migrate(kind, file).unwrap();
        let preset = if kind == FileKind::Project {
            &out["fab_presets"][0]
        } else {
            &out
        };
        assert!(preset.get("stackup").is_none());
        assert_eq!(
            preset["constructions"],
            json!([{"name":"2 Layer","default":true,"thickness":1600000,"entries":[{"copper":35000},{"kind":"core"},{"copper":35000}]}])
        );
        assert_eq!(
            preset["rules"]["clearance_hole"],
            json!({"via":250000,"pad":250000})
        );
    }
}

#[test]
fn a_migrated_board_has_the_new_board_rules_with_each_teardrop_target_off() {
    let bytes = std::fs::read(Path::new(V1).join("blinky_555/Blinky.redapcb")).unwrap();
    let pcb = migrate(FileKind::Pcb, serde_json::from_slice(&bytes).unwrap()).unwrap();
    let rules = pcb["rules"].as_array().unwrap();
    let values = |kind: &str| {
        &rules
            .iter()
            .find(|r| r["kind"] == kind && r["scope"] == json!({}))
            .unwrap()["values"]
    };
    assert_eq!(values("via_span"), &json!({}));
    assert_eq!(values("microvia_size"), &json!({}));
    assert_eq!(values("silk_width"), &json!({"min":0}));
    assert_eq!(values("silk_text_height"), &json!({"min":0}));
    for target in ["through_hole_pad", "smd_pad", "via", "width_change"] {
        assert_eq!(
            values("teardrop")[target],
            json!({"enabled":false,"length":50,"max_length":1000000,"width":100,"max_width":2000000,"curved":false,"skip_ratio":90})
        );
    }
}

#[test]
fn a_released_rules_file_changes_only_its_version_and_boardwide_via_preferences() {
    let file = std::fs::read(Path::new(V1).join("standalone/Blinky.redarules")).unwrap();
    let mut original: Value = serde_json::from_slice(&file).unwrap();
    let scoped = json!({"kind":"via_size", "net_class":"Power","values":{"diameter":{"preferred":2345000},"hole":{"preferred":555000}}});
    original["rules"].as_array_mut().unwrap().push(scoped);
    let mut expected = original.clone();
    expected["format"] = json!(2);
    expected["rules"][2]["values"] = json!({"diameter":{"min":510000},"hole":{"min":150000}});
    assert_eq!(migrate(FileKind::Rules, original).unwrap(), expected);
}

#[test]
fn released_projects_migrate_to_canonical_goldens_and_save_identically_on_reopen() {
    let frozen = Path::new(V1);
    let goldens = frozen.parent().unwrap().join("migrated-v2");
    for (folder, manifest) in [
        ("minimal_board", "Minimal.reda"),
        ("blinky_555", "Blinky.reda"),
        ("usb_c_mcu", "UsbMcu.reda"),
    ] {
        let source = frozen.join(folder);
        let (project, _) = reda_engine::store::load(&source.join(manifest)).unwrap();
        let files = reda_engine::store::encode(&project).unwrap();
        let reopened = TempDir::new("canonical-migration");
        for (rel, bytes) in &files {
            let file = reopened.path().join(rel);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, bytes).unwrap();
            if !rel.starts_with(".reda/") {
                assert_golden(goldens.join(folder).join(rel), bytes);
            }
        }
        let (again, _) = reda_engine::store::load(&reopened.path().join(manifest)).unwrap();
        assert_eq!(reda_engine::store::encode(&again).unwrap(), files);
    }
}

#[test]
fn board_wide_via_preferences_cannot_be_set_or_cleared_after_migration() {
    let dir = TempDir::new("v2-rule-edit");
    let mut engine = test_engine();
    for (rel, bytes) in read_tree(&Path::new(V1).join("blinky_555")) {
        std::fs::write(dir.path().join(rel), bytes).unwrap();
    }
    engine.run_ok(
        "project.open",
        json!({"path": dir.path().join("Blinky.reda")}),
    );
    for dimension in ["hole", "diameter"] {
        for value in [json!(800000), Value::Null] {
            let error = engine.run_err(
                "pcb.rules.set",
                json!({"kind":"via_size", "values":{dimension:{"preferred":value}}}),
            );
            assert_eq!(error.code, reda_engine::command::ErrorCode::InvalidParams);
            assert_eq!(
                error.details.unwrap()["pointer"],
                format!("/values/{dimension}/preferred")
            );
        }
    }
}

#[test]
fn generated_migration_ids_do_not_replace_existing_objects() {
    let bytes = std::fs::read(Path::new(V1).join("minimal_board/Minimal.redapcb")).unwrap();
    let mut pcb: Value = serde_json::from_slice(&bytes).unwrap();
    let ordinary = migrate(FileKind::Pcb, pcb.clone()).unwrap();
    let generated = ordinary["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "via_span")
        .unwrap()["uuid"]
        .clone();
    let through = ordinary["stackups"][0]["via_types"][0]["uuid"].clone();
    pcb["rules"][0]["uuid"] = generated.clone();
    pcb["rules"][1]["uuid"] = through.clone();
    let migrated = migrate(FileKind::Pcb, pcb.clone()).unwrap();
    let rules = migrated["rules"].as_array().unwrap();
    assert_eq!(rules.iter().filter(|r| r["uuid"] == generated).count(), 1);
    assert_ne!(migrated["stackups"][0]["via_types"][0]["uuid"], through);
    assert_eq!(migrate(FileKind::Pcb, pcb).unwrap(), migrated);
}

#[test]
fn placing_an_unchanged_library_part_reuses_its_released_document_copies() {
    let mut engine = reda_testkit::TestEngine(
        reda_engine::Engine::builder()
            .ids(reda_engine::command::SeededIds::new(1 << 32))
            .user_dirs(None)
            .starter_library(Some(reda_testkit::starter_library()))
            .build(),
    );
    let source = Path::new(V1).join("blinky_555");
    let tmp = TempDir::new("reuse-library-copies");
    for (file, bytes) in read_tree(&source) {
        std::fs::write(tmp.path().join(file), bytes).unwrap();
    }
    let sheet = tmp.path().join("Blinky.redasch");
    let released: Value = serde_json::from_slice(&std::fs::read(&sheet).unwrap()).unwrap();
    engine.run_ok(
        "project.open",
        json!({"path":tmp.path().join("Blinky.reda")}),
    );
    // This is the resistor and its fitted Footprint in the released Blinky board.
    engine.run_ok("schematic.component.place", json!({"document":released["sheet"]["uuid"],
        "part":"84d5651a-bf4b-462e-9480-39e8f454a042", "footprint":"0691e5e7-9eb6-49f3-9f3e-d4159258693e",
        "at":{"x":"20mm","y":"20mm"}}));
    engine.run_ok("project.save", json!({}));
    let saved: Value = serde_json::from_slice(&std::fs::read(sheet).unwrap()).unwrap();
    assert_eq!(
        saved["units"].as_array().unwrap().len(),
        released["units"].as_array().unwrap().len() + 1
    );
    for table in ["parts", "footprints"] {
        assert_eq!(
            saved[table].as_array().unwrap().len(),
            released[table].as_array().unwrap().len(),
            "{table}"
        );
    }
}

#[test]
fn a_legacy_board_with_an_explicit_null_net_class_saves_as_the_same_migrated_board() {
    let source = Path::new(V1).join("blinky_555");
    let project = reda_engine::store::load_from(&source.join("Blinky.reda"), |path| {
        let bytes = std::fs::read(path).unwrap();
        if path.extension().is_some_and(|ext| ext == "redapcb") {
            let mut file: Value = serde_json::from_slice(&bytes).unwrap();
            let board_via = file["rules"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|r| r["kind"] == "via_size" && r["scope"] == json!({}))
                .unwrap();
            board_via["scope"]["net_class"] = Value::Null;
            Ok(serde_json::to_vec(&file).unwrap())
        } else {
            Ok(bytes)
        }
    })
    .unwrap()
    .0;
    let (_, migrated) = reda_engine::store::encode(&project)
        .unwrap()
        .into_iter()
        .find(|(name, _)| name == "Blinky.redapcb")
        .unwrap();
    let golden = Path::new(V1)
        .parent()
        .unwrap()
        .join("migrated-v2/blinky_555/Blinky.redapcb");
    assert_eq!(migrated, std::fs::read(golden).unwrap());
}

#[test]
fn migrating_refuses_fields_that_did_not_exist_in_the_declared_format() {
    for (kind, file, pointer, key) in [
        (
            FileKind::Project,
            "blinky_555/Blinky.reda",
            "/fab_presets/0",
            "placement_corrections",
        ),
        (
            FileKind::FabPreset,
            "standalone/Blinky.redafab",
            "",
            "constructions",
        ),
        (
            FileKind::FabPreset,
            "standalone/Blinky.redafab",
            "/rules",
            "clearance_hole",
        ),
        (
            FileKind::Pcb,
            "blinky_555/Blinky.redapcb",
            "/stackups/0",
            "via_types",
        ),
        (
            FileKind::Pcb,
            "blinky_555/Blinky.redapcb",
            "/stackups/0/entries/1",
            "kind",
        ),
        (
            FileKind::Pcb,
            "blinky_555/Blinky.redapcb",
            "/layers/layers/0",
            "name",
        ),
    ] {
        let mut source: Value =
            serde_json::from_slice(&std::fs::read(Path::new(V1).join(file)).unwrap()).unwrap();
        source.pointer_mut(pointer).unwrap()[key] = json!("hand edited");
        assert!(
            migrate(kind, source).is_err(),
            "{file} accepted {pointer}/{key}"
        );
    }
}
