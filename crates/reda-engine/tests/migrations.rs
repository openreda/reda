//! The migration chain: raw-JSON steps that run before typed deserialisation.
//!
//! Release inputs live under `tests/projects/formats/v<n>`. Their canonical successors
//! live beside them in `migrated-v<n+1>`. The standalone codec tests in `format::v2`
//! exercise the same file pairs for Library files, user presets and Rules Files.
//! The frozen Demo likewise has a canonical migrated successor.

use std::path::{Path, PathBuf};

use proptest::strategy::Strategy;
use reda_engine::format::{
    FORMAT, FileKind, MIGRATIONS, Migration, RELEASED, check_released, migrate,
    migrate_checked_with, migrate_with,
};
use reda_testkit::{TempDir, assert_golden, read_tree, test_engine};
use serde_json::{Value, json};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/migrations");
const FORMATS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/formats");
const CURRENT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/formats/migrated-v3"
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
        assert_eq!(
            saved
                .keys()
                .filter(|rel| !rel.starts_with(".reda/"))
                .count(),
            files.len(),
            "v{n}"
        );
        for rel in files.keys() {
            assert_golden(Path::new(CURRENT).join(rel), &saved[rel]);
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
                check_frozen_step(n as u32, kind);
            }
        }
    }
}

/// Runs the chain on the frozen file of `kind` that release `n` wrote, and compares the
/// raw step output with `tests/golden/migrations/v<n>-to-v<n+1>/<ext>.after.json`.
fn check_frozen_step(n: u32, kind: FileKind) {
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
        FileKind::BoardTemplate => "standalone/PCI_Express_add-in_card.redaboard",
    };
    let before: Value =
        serde_json::from_slice(&std::fs::read(frozen_inputs(n).join(file)).unwrap()).unwrap();
    let after = migrate_checked_with(kind, before, &MIGRATIONS[..n as usize]).unwrap();
    let mut text = serde_json::to_string_pretty(&after).unwrap();
    text.push('\n');
    let dir = Path::new(GOLDEN).join(format!("v{n}-to-v{}", n + 1));
    assert_golden(
        dir.join(format!("{}.after.json", kind.extension())),
        text.as_bytes(),
    );
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
        // Board Templates arrived after format 1: there is no format-1 file to migrate.
        FileKind::BoardTemplate => return,
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
        let migrated = to_v2(kind, expected.clone()).unwrap();
        expected["format"] = json!(2);
        assert_eq!(migrated, expected, "{}", kind.extension());
    }
}

/// Runs only the format 1 to 2 step, checked, for tests of what that step did.
fn to_v2(kind: FileKind, file: Value) -> Result<Value, String> {
    migrate_checked_with(kind, file, &MIGRATIONS[..1])
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
        // The preset's thickness is its two copper seeds plus a positive core, so the
        // migrated "2 Layer" Construction always leaves room for its blank dielectric.
        seeds in (proptest::option::of(100_000i64..2_600_000), proptest::option::of(10_000i64..210_000))
            .prop_map(|(core, copper)| (core.map(|c| c + 2 * copper.unwrap_or(35_000)), copper)),
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
            proptest::prop_assert!(layer.get("role").is_none());
            proptest::prop_assert_eq!(layer.get("name").is_some(), layer["kind"] == "mechanical");
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
    assert_eq!(to_v2(FileKind::Rules, original).unwrap(), expected);
}

/// What the canonical writer saves of the Project at `manifest`, read through the chain.
fn saved_now(manifest: &Path) -> Vec<(String, Vec<u8>)> {
    let (project, _) = reda_engine::store::load(manifest).unwrap();
    reda_engine::store::encode(&project).unwrap()
}

/// Writes `files` under a new folder and saves the Project again from there.
fn saved_after_reopen(files: &[(String, Vec<u8>)], manifest: &str) -> Vec<(String, Vec<u8>)> {
    let reopened = TempDir::new("canonical-migration");
    for (rel, bytes) in files {
        let file = reopened.path().join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, bytes).unwrap();
    }
    saved_now(&reopened.path().join(manifest))
}

/// A 0.1.0 Project runs both steps, and lands where the 1 to 2 step's canonical goldens
/// (what the format-2 writer saved of it) land through the 2 to 3 step.
#[test]
fn released_v1_projects_run_both_steps_and_save_identically_on_reopen() {
    let frozen = Path::new(V1);
    let goldens = frozen.parent().unwrap().join("migrated-v2");
    for (folder, manifest) in V1_PROJECTS {
        let files = saved_now(&frozen.join(folder).join(manifest));
        let via_v2 = saved_now(&goldens.join(folder).join(manifest));
        assert_eq!(files, via_v2, "{folder}");
        assert_eq!(saved_after_reopen(&files, manifest), files, "{folder}");
    }
}

/// Every 0.2.0 reference Project migrates to its canonical format-3 golden and saves the
/// same again on reopen.
#[test]
fn released_v2_projects_migrate_to_canonical_goldens_and_save_identically_on_reopen() {
    let frozen = frozen_inputs(2);
    let goldens = frozen.parent().unwrap().join("migrated-v3");
    for (folder, manifest) in V2_PROJECTS {
        let files = saved_now(&frozen.join(folder).join(manifest));
        for (rel, bytes) in &files {
            if !rel.starts_with(".reda/") {
                assert_golden(goldens.join(folder).join(rel), bytes);
            }
        }
        assert_eq!(saved_after_reopen(&files, manifest), files, "{folder}");
    }
}

const V1_PROJECTS: [(&str, &str); 3] = [
    ("minimal_board", "Minimal.reda"),
    ("blinky_555", "Blinky.reda"),
    ("usb_c_mcu", "UsbMcu.reda"),
];

const V2_PROJECTS: [(&str, &str); 7] = [
    ("minimal_board", "Minimal.reda"),
    ("blinky_555", "Blinky.reda"),
    ("usb_c_mcu", "UsbMcu.reda"),
    ("four_layer", "FourLayer.reda"),
    ("six_layer_hdi", "SixLayerHdi.reda"),
    ("castellated_module", "Castellated.reda"),
    ("m2_card", "M2Card.reda"),
];

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
        .join("migrated-v2/blinky_555/Blinky.reda");
    let (_, expected) = saved_now(&golden)
        .into_iter()
        .find(|(name, _)| name == "Blinky.redapcb")
        .unwrap();
    assert_eq!(migrated, expected);
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

/// The frozen inputs of release `n`: `tests/projects/formats/v<n>/`.
fn frozen_inputs(n: u32) -> PathBuf {
    Path::new(V1).parent().unwrap().join(format!("v{n}"))
}

/// Every REDA file under `dir`, skipping frozen `outputs/`, with its kind and parsed JSON.
fn frozen_files(dir: &Path) -> Vec<(String, FileKind, Value)> {
    let files: Vec<_> = read_tree(dir)
        .into_iter()
        .filter(|(rel, _)| !rel.starts_with("outputs/"))
        .map(|(rel, bytes)| {
            let ext = Path::new(&rel).extension().unwrap().to_str().unwrap();
            let kind = FileKind::ALL
                .into_iter()
                .find(|k| k.extension() == ext)
                .unwrap_or_else(|| panic!("{rel} is not a REDA file"));
            (rel, kind, serde_json::from_slice(&bytes).unwrap())
        })
        .collect();
    assert!(!files.is_empty(), "nothing frozen in {}", dir.display());
    files
}

#[test]
fn every_frozen_file_fits_the_schemas_its_release_shipped() {
    for n in 1..=RELEASED {
        let demo = Path::new(FORMATS).join(format!("v{n}"));
        for (rel, kind, file) in frozen_files(&frozen_inputs(n))
            .into_iter()
            .chain(frozen_files(&demo))
        {
            assert_eq!(file["format"], n, "{rel}");
            if let Err(why) = check_released(n, kind, &file) {
                panic!("v{n}/{rel}: {why}");
            }
        }
    }
}

#[test]
fn the_format_two_schemas_refuse_fields_that_arrive_in_format_three() {
    let read = |file: &str| -> Value {
        serde_json::from_slice(&std::fs::read(frozen_inputs(2).join(file)).unwrap()).unwrap()
    };
    let mut board = read("blinky_555/Blinky.redapcb");
    board["layers"]["layers"][0]["purpose"] = json!("dimensions");
    let mut footprint = read("standalone/CAPC1005X40N.redafp");
    footprint["bodies"] = json!([]);
    let mut template = read("standalone/PCI_Express_add-in_card.redaboard");
    let group = template["drawing_groups"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|g| g["layer"] == "mechanical")
        .unwrap();
    group["purpose"] = json!("dimensions");
    for (kind, file, field) in [
        (FileKind::Pcb, board, "purpose"),
        (FileKind::Footprint, footprint, "bodies"),
        (FileKind::BoardTemplate, template, "purpose"),
    ] {
        let why = check_released(2, kind, &file).unwrap_err();
        assert!(why.contains(field), "{}: {why}", kind.extension());
    }
}

#[test]
fn only_released_formats_have_schemas_to_check_against() {
    let file = json!({"format": RELEASED + 1});
    assert!(check_released(RELEASED + 1, FileKind::Sheet, &file).is_err());
    assert!(check_released(0, FileKind::Sheet, &file).is_err());
    let why = check_released(1, FileKind::BoardTemplate, &json!({"format": 1})).unwrap_err();
    assert!(why.contains("format 1"), "{why}");
}

/// The chain, not each step, checks a step's input against the schemas of the format it
/// reads, so the step from format 2 gets format-2 files exactly as 0.2.0 declared them.
#[test]
fn the_chain_checks_format_two_input_against_the_released_schemas() {
    fn number_only(_: FileKind, file: Value) -> Result<Value, String> {
        Ok(file)
    }
    let steps: &[Migration] = &[MIGRATIONS[0], number_only];
    for (rel, kind, file) in frozen_files(&frozen_inputs(2)) {
        let migrated =
            migrate_checked_with(kind, file, steps).unwrap_or_else(|why| panic!("v2/{rel}: {why}"));
        assert_eq!(migrated["format"], 3, "{rel}");
    }
    let path = frozen_inputs(2).join("blinky_555/Blinky.redapcb");
    let mut board: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    board["layers"]["layers"][0]["purpose"] = json!("dimensions");
    let why = migrate_checked_with(FileKind::Pcb, board.clone(), steps).unwrap_err();
    assert!(
        why.starts_with("migrating from format 2: /layers/layers/0/purpose"),
        "{why}"
    );
    // Stand-in steps on made-up files run unchecked.
    assert_eq!(
        migrate_with(FileKind::Pcb, board, steps).unwrap()["format"],
        3
    );
}

#[test]
fn every_real_step_refuses_input_its_release_did_not_declare() {
    for n in 1..FORMAT {
        let path = frozen_inputs(n).join("blinky_555/Blinky.reda");
        let mut project: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        project["hand_edited"] = json!(true);
        let why = migrate(FileKind::Project, project).unwrap_err();
        assert!(why.contains(&format!("format {n}")), "{why}");
        assert!(why.contains("hand_edited"), "{why}");
    }
}

/// Reads a frozen format-2 file.
fn frozen_v2(file: &str) -> Value {
    serde_json::from_slice(&std::fs::read(frozen_inputs(2).join(file)).unwrap()).unwrap()
}

/// The format-2 Layer list of `board` with the Mechanical and Documentation Layers
/// rewritten as the 2 to 3 step writes them, and moved right after the Board Outline.
fn three_layers(board: &Value) -> Vec<Value> {
    let layers = board["layers"]["layers"].as_array().unwrap();
    let mut out: Vec<Value> = layers
        .iter()
        .filter(|l| l["kind"] != "mechanical" && l["kind"] != "documentation")
        .cloned()
        .collect();
    let outline = out
        .iter()
        .position(|l| l["kind"] == "board_outline")
        .unwrap();
    let renamed = |kind: &'static str, name: &'static str, purpose: &'static str| {
        layers.iter().filter(move |l| l["kind"] == kind).map(move |l| {
            json!({"uuid": l["uuid"], "kind": "mechanical", "name": name, "purpose": purpose})
        })
    };
    let mechanical: Vec<Value> = renamed("mechanical", "Dimensions", "dimensions")
        .chain(renamed("documentation", "Reference", "reference"))
        .collect();
    out.splice(outline + 1..outline + 1, mechanical);
    out
}

fn height_record(file: &Value) -> &Value {
    file["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "component_height")
        .expect("a component_height record")
}

#[test]
fn the_two_to_three_step_turns_the_old_layers_into_dimensions_and_reference() {
    for project in ["blinky_555/Blinky.redapcb", "four_layer/FourLayer.redapcb"] {
        let original = frozen_v2(project);
        let migrated = migrate(FileKind::Pcb, original.clone()).unwrap();
        let mut expected = original.clone();
        expected["format"] = json!(3);
        expected["layers"]["layers"] = json!(three_layers(&original));
        let uuid = height_record(&migrated)["uuid"].clone();
        expected["rules"]
            .as_array_mut()
            .unwrap()
            .push(json!({"uuid": uuid, "kind": "component_height", "scope": {}, "values": {}}));
        assert_eq!(migrated, expected, "{project}");
        let kinds: Vec<_> = migrated["layers"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .take(3)
            .map(|l| l["kind"].clone())
            .collect();
        assert_eq!(kinds, ["mechanical", "mechanical", "board_outline"]);
    }
}

#[test]
fn the_two_to_three_step_gives_the_height_record_an_identity_no_object_has() {
    let original = frozen_v2("blinky_555/Blinky.redapcb");
    let taken = height_record(&migrate(FileKind::Pcb, original.clone()).unwrap())["uuid"].clone();
    let mut clash = original;
    clash["rules"][0]["uuid"] = taken.clone();
    let migrated = migrate(FileKind::Pcb, clash.clone()).unwrap();
    assert_ne!(height_record(&migrated)["uuid"], taken);
    assert_eq!(migrate(FileKind::Pcb, clash).unwrap(), migrated);
}

/// A hand-edited format-2 board its schema allowed: two Mechanical Layers, one of them
/// before the copper, and no Documentation Layer.
#[test]
fn the_two_to_three_step_moves_misplaced_layers_and_numbers_a_repeated_name() {
    let mut board = frozen_v2("minimal_board/Minimal.redapcb");
    check_released(2, FileKind::Pcb, &board).unwrap();
    let layers = board["layers"]["layers"].as_array_mut().unwrap();
    let documentation = layers
        .iter()
        .position(|l| l["kind"] == "documentation")
        .unwrap();
    let mut first = layers.remove(documentation);
    first["kind"] = json!("mechanical");
    layers.insert(0, first.clone());
    let migrated = migrate(FileKind::Pcb, board).unwrap();
    let layers = migrated["layers"]["layers"].as_array().unwrap();
    let outline = layers
        .iter()
        .position(|l| l["kind"] == "board_outline")
        .unwrap();
    assert!(layers[..outline].iter().all(|l| l["kind"] != "mechanical"));
    let tail: Vec<_> = layers[outline + 1..]
        .iter()
        .map(|l| (l["uuid"].clone(), l["name"].clone()))
        .collect();
    assert_eq!(tail.len(), 2);
    assert_eq!(tail[0], (first["uuid"].clone(), json!("Dimensions")));
    assert_eq!(tail[1].1, "Dimensions 2");
}

#[test]
fn the_two_to_three_step_names_drawing_groups_by_purpose() {
    let original = frozen_v2("standalone/PCI_Express_add-in_card.redaboard");
    let migrated = migrate(FileKind::BoardTemplate, original.clone()).unwrap();
    let mut expected = original;
    expected["format"] = json!(3);
    for group in expected["drawing_groups"].as_array_mut().unwrap() {
        let layer = group.as_object_mut().unwrap().remove("layer").unwrap();
        group["purpose"] = match layer.as_str().unwrap() {
            "mechanical" => json!("dimensions"),
            "documentation" => json!("reference"),
            other => panic!("{other}"),
        };
    }
    assert_eq!(migrated, expected);
    let purposes: Vec<_> = migrated["drawing_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["purpose"].clone())
        .collect();
    assert_eq!(purposes, ["dimensions", "reference"]);
}

#[test]
fn every_other_frozen_format_two_file_moves_to_format_three_unchanged() {
    for (rel, kind, file) in frozen_files(&frozen_inputs(2)) {
        if matches!(kind, FileKind::Pcb | FileKind::BoardTemplate) {
            continue;
        }
        let mut expected = file.clone();
        expected["format"] = json!(3);
        assert_eq!(migrate(kind, file).unwrap(), expected, "{rel}");
    }
}
