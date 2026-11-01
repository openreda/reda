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
