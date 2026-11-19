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

#[test]
fn via_types_reopen_with_their_drilling_direction_and_independent_sizes() {
    let folder = repo().join("tests/scenarios/via_types");
    let (project, _) = reda_engine::store::load(&folder.join("ViaTypes.reda")).unwrap();
    let pcb = project.pcb().unwrap();
    let actual: Vec<_> = pcb
        .vias()
        .map(|via| {
            let kind = pcb.stackup().type_for_span(via.span);
            (
                pcb.copper_number(via.span.from).unwrap(),
                pcb.copper_number(via.span.to).unwrap(),
                kind.map(|v| v.display_name(pcb)),
                via.hole.0,
                via.diameter.start.0,
                via.diameter.middle.map(|d| d.0),
                via.diameter.end.map(|d| d.0),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (
                2,
                1,
                Some("Top laser".into()),
                100_000,
                280_000,
                None,
                Some(300_000)
            ),
            (2, 3, Some("Micro 2:3".into()), 120_000, 320_000, None, None),
            (1, 6, Some("Through".into()), 300_000, 700_000, None, None),
            (4, 5, None, 300_000, 700_000, Some(800_000), Some(900_000)),
            (
                1,
                4,
                Some("Inner laser".into()),
                200_000,
                500_000,
                None,
                None
            ),
        ]
    );
}

#[test]
fn via_types_reopen_with_the_broken_spans_sizes_and_stack_in_the_drc_report() {
    use reda_testkit::{TempDir, assert_golden, test_engine};
    use serde_json::json;

    let scenarios = repo().join("tests/scenarios");
    let work = TempDir::new("via-types-report");
    for (name, bytes) in read_tree(&scenarios.join("via_types")) {
        std::fs::write(work.path().join(name), bytes).unwrap();
    }
    let mut engine = test_engine();
    engine.run_ok(
        "project.open",
        json!({"path":work.path().join("ViaTypes.reda")}),
    );
    let checks = ["via_span", "microvia_size", "aspect_ratio", "max_stack"];
    let mut report = engine.run_ok("drc.run", json!({"checks":checks}));
    let violations = report["violations"].as_array().unwrap();
    for check in checks {
        assert!(
            violations.iter().any(|v| v["check"] == check),
            "missing {check}"
        );
    }
    for reason in ["no_type", "micro_too_long", "not_allowed"] {
        assert!(
            violations.iter().any(|v| v["args"]["reason"] == reason),
            "missing {reason}"
        );
    }
    report.as_object_mut().unwrap().remove("design");
    let text = serde_json::to_string_pretty(&report).unwrap() + "\n";
    assert_golden(
        scenarios.join("goldens/via_types/drc.json"),
        text.as_bytes(),
    );
}

#[test]
fn inner_layers_reopens_an_outline_plane_and_both_drill_clearance_measurements() {
    let dir = repo().join("tests/scenarios/inner_layers");
    let mut engine = reda_testkit::test_engine_with_library(&reda_testkit::starter_library());
    engine.run_ok(
        "project.open",
        serde_json::json!({"path":dir.join("InnerLayers.reda")}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let plane = pcb.copper_layers().nth(1).unwrap();
    assert_eq!(plane.role, reda_engine::model::CopperRole::Plane);
    assert!(
        pcb.zones()
            .any(|z| z.layer == plane.uuid && z.follows_outline && z.vertices.is_empty())
    );
    let via = pcb.vias().next().unwrap();
    let relief = via.zone_connection.unwrap();
    assert_eq!(
        relief.style,
        Some(reda_engine::model::ConnectionStyle::Relief)
    );
    assert_eq!(relief.spokes, Some(2));
    let plane = plane.uuid;
    let report = engine.run_ok("drc.run", serde_json::json!({"checks":["clearance"]}));
    let mut holes: Vec<_> = report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| {
            Some((
                v["args"]["part"].as_str()?,
                v["layers"].clone(),
                v["measured"]["length"].as_i64()?,
                v["required"]["length"].as_i64()?,
            ))
        })
        .collect();
    holes.sort_by_key(|r| r.0);
    assert_eq!(
        holes,
        [
            ("hole.pad", serde_json::json!([plane]), 650000, 1100000),
            ("hole.via", serde_json::json!([plane]), 650000, 1200000)
        ]
    );
}
