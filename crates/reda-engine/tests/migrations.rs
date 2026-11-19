//! The migration chain: raw-JSON steps that run before typed deserialisation.
//!
//! Every real step in `format::MIGRATIONS` gets a golden pair per file kind under
//! `tests/golden/migrations/v<n>-to-v<n+1>/`: `<ext>.before.json` (a file of format n) and
//! `<ext>.after.json` (what the chain turns it into). The scaffold step below exercises the
//! same harness while REDA is still on its first format.
//!
//! On every `FORMAT` bump, a copy of the Demo Project in the old format is frozen under
//! `tests/golden/formats/v<n>/`; each one must open and save as today's goldens.

use std::path::{Path, PathBuf};

use reda_engine::format::{FORMAT, FileKind, MIGRATIONS, Migration, migrate, migrate_with};
use reda_testkit::{TempDir, assert_golden, read_tree, test_engine};
use serde_json::{Value, json};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/migrations");
const FORMATS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/formats");
const CURRENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/new_project");

#[test]
fn every_frozen_format_opens_and_saves_as_the_current_goldens() {
    for n in 1..=FORMAT {
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
    for n in 1..=MIGRATIONS.len() {
        let dir = PathBuf::from(GOLDEN).join(format!("v{n}-to-v{}", n + 1));
        let steps = &MIGRATIONS[..n];
        for kind in FileKind::ALL {
            check_step_golden(&dir, steps, kind);
        }
    }
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
    for kind in [
        FileKind::Library,
        FileKind::Symbol,
        FileKind::Footprint,
        FileKind::Part,
        FileKind::Sheet,
    ] {
        let before = std::fs::read(
            Path::new(GOLDEN)
                .join("scaffold")
                .join(format!("{}.before.json", kind.extension())),
        )
        .unwrap();
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
    fn migrating_a_board_moves_only_the_board_preferences_into_its_through_type(
        start in 500_001i64..5_000_000,
        hole in 100_000i64..500_001,
    ) {
        let original = std::fs::read(Path::new(V1).join("blinky_555/Blinky.redapcb")).unwrap();
        let mut pcb: Value = serde_json::from_slice(&original).unwrap();
        let rules = pcb["rules"].as_array_mut().unwrap();
        let board = rules.iter_mut().find(|r| r["kind"] == "via_size" && r["scope"] == json!({})).unwrap();
        board["values"]["diameter"]["preferred"] = json!(start);
        board["values"]["hole"]["preferred"] = json!(hole);
        let class = json!({"uuid":"afffffff-0000-4000-8000-000000000001", "kind":"via_size",
            "scope":{"net_class":"afffffff-0000-4000-8000-000000000002"}, "values":{"hole":{"preferred":345678}}});
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
