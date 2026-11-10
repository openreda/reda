//! Runs every data-driven scenario in `tests/scenarios/` and compares the resulting
//! Documents with the goldens beside each script. `REDA_BLESS=1` rewrites them.
//!
//! The reference Projects under `tests/projects/` are what their scripts here build: each
//! kept Project must equal its script's goldens, file for file. `REDA_BLESS=1` copies the
//! goldens over the kept Project after blessing them; bless `reference_projects` again
//! afterwards, since its goldens are made from the kept files.

use std::path::{Path, PathBuf};

use reda_testkit::read_tree;

/// Each reference Project under `tests/projects/`, by the folder it shares with the
/// scenario script that builds it.
const REFERENCE_PROJECTS: &[&str] = &["minimal_board", "blinky_555", "usb_c_mcu"];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bless() -> bool {
    std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1")
}

#[test]
fn every_scenario_replays_to_its_goldens_and_rebuilds_the_reference_projects() {
    let scenarios = repo().join("tests/scenarios");
    reda_testkit::run_scenarios(&scenarios);

    let mut differ = Vec::new();
    for project in REFERENCE_PROJECTS {
        let built = scenarios.join(project);
        let kept = repo().join("tests/projects").join(project);
        let built_files = read_tree(&built);
        assert!(!built_files.is_empty(), "{project}.jsonl builds no Project");
        if bless() {
            let _ = std::fs::remove_dir_all(&kept);
            for (rel, bytes) in &built_files {
                let path = kept.join(rel);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, bytes).unwrap();
            }
            continue;
        }
        if read_tree(&kept) != built_files {
            differ.push(*project);
        }
    }
    assert!(
        differ.is_empty(),
        "tests/projects/{differ:?} differ from what their scenario scripts build; rerun \
         with REDA_BLESS=1 to copy the scripts' goldens over them"
    );
}
