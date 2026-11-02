//! The reference Projects under `tests/projects/`, driven end to end: ERC, DRC, the Sync
//! preview, every Output Job and the schematic PDF, each checked against goldens in
//! `tests/projects/goldens/<project>/`. `REDA_BLESS=1` rewrites them.
//!
//! **Where they come from.** Each one was first built by a scenario script in
//! `tests/scenarios/` (`<project>.jsonl`, whose goldens beside it are the Project as the
//! script leaves it) and copied here; from then on it is kept as files. Change a reference
//! Project by editing these files (or by opening them in REDA and saving), not by
//! rerunning its script.
//!
//! **Format bumps.** On every `format::FORMAT` bump, before the migration lands, copy each
//! reference Project as it is, still in the old format, to
//! `tests/projects/formats/v<old FORMAT>/<project>/`. Those copies are never edited again.
//! [`every_older_format_has_a_frozen_copy_that_opens_and_saves`] requires one per older
//! format and checks that each still opens through the migration chain and saves.
//!
//! **Timing.** The `#[ignore]`d `time_*` tests time the main steps on each Project; run
//! them by hand with `cargo test -p reda-engine --test reference_projects -- --ignored
//! --nocapture`. There are no performance gates in v0.1.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use reda_engine::Engine;
use reda_engine::command::{FixedClock, SeededIds};
use reda_engine::format::FORMAT;
use reda_engine::store;
use reda_testkit::{
    TempDir, TestEngine, assert_golden, read_tree, starter_library, test_engine_with_library,
};
use serde_json::{Value, json};

/// Every reference Project: its folder under `tests/projects/` and its Project file.
const PROJECTS: &[(&str, &str)] = &[
    ("minimal_board", "Minimal.reda"),
    ("blinky_555", "Blinky.reda"),
];

/// Folders under `tests/projects/` that are not Projects.
const NOT_PROJECTS: &[&str] = &["goldens", "formats"];

fn projects_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects")
}

fn goldens(project: &str) -> PathBuf {
    projects_dir().join("goldens").join(project)
}

/// Copies `from` (every file below it) into `to`.
fn copy_tree(from: &Path, to: &Path) {
    for (rel, bytes) in read_tree(from) {
        let path = to.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

/// Opens a copy of the reference Project in a fresh folder, so runs never write into
/// the repo.
fn open_copy(dir: &str, file: &str) -> (TempDir, TestEngine) {
    let work = TempDir::new(&format!("reference-{dir}"));
    copy_tree(&projects_dir().join(dir), work.path());
    // The Projects were made on seed 0; another seed keeps new objects' UUIDs clear of
    // theirs.
    let mut engine = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(1 << 32))
            .clock(FixedClock::EPOCH)
            .user_dirs(None)
            .starter_library(Some(starter_library()))
            .build(),
    );
    let opened = engine.run_ok("project.open", json!({"path": work.path().join(file)}));
    assert!(
        opened.get("missing").is_none(),
        "{dir}: Documents are missing: {opened}"
    );
    (work, engine)
}

/// Pretty JSON with a final newline, for goldens.
fn pretty(value: &Value) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    text.into_bytes()
}

/// Every path string under `folder` in `value`, made relative to it, so goldens do not
/// hold the temporary folder.
fn relative(value: &mut Value, folder: &Path) {
    match value {
        Value::String(s) => {
            if let Ok(rest) = Path::new(s.as_str()).strip_prefix(folder) {
                *s = rest.to_string_lossy().replace('\\', "/");
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| relative(v, folder)),
        Value::Object(map) => map.values_mut().for_each(|v| relative(v, folder)),
        _ => {}
    }
}

/// Checks a folder of files against a golden folder, both ways: with `REDA_BLESS=1` it
/// rewrites the golden folder, removing stale files.
fn assert_golden_tree(golden: &Path, actual: &BTreeMap<String, Vec<u8>>) {
    if std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1") {
        let _ = std::fs::remove_dir_all(golden);
    }
    for (rel, bytes) in actual {
        assert_golden(golden.join(rel), bytes);
    }
    let stale: Vec<_> = read_tree(golden)
        .into_keys()
        .filter(|rel| !actual.contains_key(rel))
        .collect();
    assert!(
        stale.is_empty(),
        "{}: stale goldens {stale:?}",
        golden.display()
    );
}

#[test]
fn every_folder_under_tests_projects_is_a_listed_reference_project() {
    let mut folders: Vec<String> = std::fs::read_dir(projects_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !NOT_PROJECTS.contains(&name.as_str()))
        .collect();
    folders.sort();
    let mut listed: Vec<String> = PROJECTS.iter().map(|(d, _)| d.to_string()).collect();
    listed.sort();
    assert_eq!(folders, listed);
}

#[test]
fn opening_and_saving_leaves_every_file_as_it_was() {
    for (dir, file) in PROJECTS {
        let (work, mut engine) = open_copy(dir, file);
        engine.run_ok("project.save", json!({}));
        let kept = read_tree(&projects_dir().join(dir));
        let saved: BTreeMap<_, _> = read_tree(work.path())
            .into_iter()
            .filter(|(rel, _)| !rel.starts_with(".reda/"))
            .collect();
        assert_eq!(
            saved.keys().collect::<Vec<_>>(),
            kept.keys().collect::<Vec<_>>(),
            "{dir}"
        );
        for (rel, bytes) in &kept {
            assert!(saved[rel] == *bytes, "{dir}: saving rewrote {rel}");
        }
    }
}

#[test]
fn erc_drc_and_sync_preview_match_their_goldens() {
    for (dir, file) in PROJECTS {
        let (_work, mut engine) = open_copy(dir, file);
        for (id, golden) in [
            ("erc.run", "erc.json"),
            ("drc.run", "drc.json"),
            ("sync.preview", "sync.json"),
        ] {
            let result = engine.run_ok(id, json!({}));
            assert_golden(goldens(dir).join(golden), &pretty(&result));
        }
    }
}

#[test]
fn every_output_matches_its_goldens() {
    for (dir, file) in PROJECTS {
        let (work, mut engine) = open_copy(dir, file);
        let jobs: Vec<String> = engine
            .project()
            .unwrap()
            .output_jobs()
            .map(|job| job.uuid.to_string())
            .collect();
        assert!(!jobs.is_empty(), "{dir} has no Output Job");
        let mut reports = Vec::new();
        for job in jobs {
            let mut report = engine.run_ok("output.job.run", json!({"job": job}));
            relative(&mut report, work.path());
            reports.push(report);
        }
        let mut pdf = engine.run_ok(
            "schematic.export_pdf",
            json!({"path": "outputs/schematic.pdf"}),
        );
        relative(&mut pdf, work.path());
        reports.push(pdf);
        assert_golden(goldens(dir).join("outputs.json"), &pretty(&json!(reports)));

        let outputs = read_tree(&work.path().join("outputs"));
        assert_golden_tree(&goldens(dir).join("outputs"), &outputs);
    }
}

#[test]
fn every_older_format_has_a_frozen_copy_that_opens_and_saves() {
    // Empty while REDA is on its first format.
    #[allow(clippy::reversed_empty_ranges)]
    for n in 1..FORMAT {
        for (dir, file) in PROJECTS {
            let frozen = projects_dir()
                .join("formats")
                .join(format!("v{n}"))
                .join(dir);
            assert!(
                frozen.join(file).is_file(),
                "no format {n} copy of {dir}: freeze one at {} before bumping FORMAT",
                frozen.display()
            );
            let work = TempDir::new(&format!("frozen-v{n}-{dir}"));
            copy_tree(&frozen, work.path());
            let mut engine = test_engine_with_library(&starter_library());
            engine.run_ok("project.open", json!({"path": work.path().join(file)}));
            engine.run_ok("project.save", json!({}));
            store::encode(engine.project().unwrap()).unwrap();
        }
    }
}

/// Runs `f` and prints how long it took.
fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{what}: {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
    out
}

#[test]
#[ignore = "timing; run by hand with --ignored --nocapture"]
fn time_open_checks_and_outputs() {
    for (dir, file) in PROJECTS {
        let (work, mut engine) = timed(&format!("{dir}: open"), || open_copy(dir, file));
        for id in ["erc.run", "sync.preview", "drc.run", "pcb.ratsnest"] {
            timed(&format!("{dir}: {id}"), || engine.run_ok(id, json!({})));
        }
        timed(&format!("{dir}: pcb.zone.fill"), || {
            engine.run_ok("pcb.zone.fill", json!({}))
        });
        let job = engine
            .project()
            .unwrap()
            .output_jobs()
            .next()
            .unwrap()
            .uuid
            .to_string();
        timed(&format!("{dir}: output.job.run"), || {
            engine.run_ok("output.job.run", json!({"job": job}))
        });
        timed(&format!("{dir}: project.save"), || {
            engine.run_ok("project.save", json!({}))
        });
        drop(work);
    }
}

#[test]
#[ignore = "timing; run by hand with --ignored --nocapture"]
fn time_drc_after_a_track_edit() {
    for (dir, file) in PROJECTS {
        let (_work, mut engine) = open_copy(dir, file);
        engine.run_ok("drc.run", json!({}));
        engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "points": [{"x": "1mm", "y": "1mm"}, {"x": "2mm", "y": "1mm"}]}),
        );
        timed(&format!("{dir}: drc.run after an edit"), || {
            engine.run_ok("drc.run", json!({}))
        });
    }
}
