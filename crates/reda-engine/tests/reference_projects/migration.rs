//! Compatibility with manufacturing files written by the 0.1.0 release.
use super::{V1_PROJECTS, assert_golden_tree, open_reference_copy, projects_dir};
use reda_testkit::read_tree;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

/// Only the two reference copper files have a changed file-function record. Compare all other bytes, including line endings.
fn permitted_file_function(path: &str, old: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(old).ok()?;
    let (before, after) = if path.ends_with(".gtl") {
        (
            "TF.FileFunction,Copper,L1,Top*",
            "TF.FileFunction,Copper,L1,Top,Signal*",
        )
    } else if path.ends_with(".gbl") {
        (
            "TF.FileFunction,Copper,L2,Bot*",
            "TF.FileFunction,Copper,L2,Bot,Signal*",
        )
    } else {
        return None;
    };
    (text.matches(before).count() == 1).then(|| text.replacen(before, after, 1).into_bytes())
}

fn is_manufacturing_file(path: &str) -> bool {
    let ext = Path::new(path).extension().and_then(|s| s.to_str());
    matches!(
        ext,
        Some("gbl" | "gbo" | "gbp" | "gbs" | "gm1" | "gtl" | "gto" | "gtp" | "gts" | "drl" | "csv")
    )
}

#[test]
fn migrated_boards_preserve_v01_manufacturing_outputs() {
    let formats = projects_dir().join("formats");
    let frozen = formats.join("v1");
    for &(name, project) in V1_PROJECTS {
        let (work, mut engine) = open_reference_copy(&frozen.join(name), project);
        engine.run_ok("project.save", json!({}));
        let jobs: Vec<_> = engine
            .project()
            .unwrap()
            .output_jobs()
            .map(|j| j.uuid.to_string())
            .collect();
        for job in jobs {
            engine.run_ok("output.job.run", json!({"job": job}));
        }
        let files = read_tree(&work.path().join("outputs"));
        let expected = read_tree(&frozen.join("outputs").join(name));
        assert!(
            !expected.is_empty(),
            "{name}: missing frozen manufacturing outputs"
        );
        let manufactured: BTreeMap<_, _> = files
            .iter()
            .filter(|(path, _)| is_manufacturing_file(path))
            .collect();
        assert!(
            manufactured.keys().copied().eq(expected.keys()),
            "{name}: manufacturing file set changed"
        );
        for (path, old) in expected {
            let new = &files[&path];
            assert!(
                new == &old || permitted_file_function(&path, &old).as_ref() == Some(new),
                "{name}: manufacturing bytes changed in {path}"
            );
        }
        let reports = files
            .into_iter()
            .filter(|(path, _)| {
                path.ends_with(".gbrjob")
                    || path.ends_with("-fab-notes.txt")
                    || path.ends_with("/manifest.json")
            })
            .collect();
        assert_golden_tree(&formats.join("migrated-outputs").join(name), &reports);
    }
}

/// A fab output: everything in a job's `fab/` folder, and its BOM and CPL.
fn is_fab_output(path: &str) -> bool {
    path.contains("/fab/") || path.ends_with("-BOM.csv") || path.ends_with("-CPL.csv")
}

/// From 0.2.0 on, each released board's frozen fab outputs (Gerber, drill, BOM, CPL, job
/// file, Fab Notes) are what its frozen copy still produces, byte for byte, through every
/// later migration: Mechanical Layers are not plotted, so any difference is a bug.
#[test]
fn every_frozen_board_since_0_2_0_still_produces_its_frozen_fab_outputs() {
    use super::PROJECTS;
    use reda_engine::format::RELEASED;
    for n in 2..=RELEASED {
        let frozen = projects_dir().join("formats").join(format!("v{n}"));
        for &(name, project) in PROJECTS {
            let (work, mut engine) = open_reference_copy(&frozen.join(name), project);
            let jobs: Vec<_> = engine
                .project()
                .unwrap()
                .output_jobs()
                .map(|j| j.uuid.to_string())
                .collect();
            for job in jobs {
                engine.run_ok("output.job.run", json!({"job": job}));
            }
            let made: BTreeMap<_, _> = read_tree(&work.path().join("outputs"))
                .into_iter()
                .filter(|(path, _)| is_fab_output(path))
                .collect();
            let expected = read_tree(&frozen.join("outputs").join(name));
            assert!(!expected.is_empty(), "v{n}/{name}: no frozen fab outputs");
            assert!(
                made.keys().eq(expected.keys()),
                "v{n}/{name}: fab file set changed"
            );
            for (path, old) in expected {
                assert!(
                    made[&path] == old,
                    "v{n}/{name}: fab bytes changed in {path}"
                );
            }
        }
    }
}

#[test]
fn a_migrated_project_gets_the_new_fab_catalogue_only_on_explicit_source_update() {
    let (_work, mut engine) =
        open_reference_copy(&projects_dir().join("formats/v1/blinky_555"), "Blinky.reda");
    let project = engine.project().unwrap();
    let original = project.pcb().unwrap().stackup().clone();
    assert_eq!(original.dielectrics().next().unwrap().thickness, None);
    let copy = project.fab_presets().next().unwrap();
    let preset = copy.uuid;
    assert_eq!(copy.name, "JLCPCB");
    assert_eq!(copy.constructions.len(), 1);
    assert_eq!(copy.constructions[0].name, "2 Layer");
    let changes = engine.run_ok("fab_preset.source_diff", json!({"preset":preset}));
    assert!(!changes["changes"].as_array().unwrap().is_empty());
    engine.run_ok("fab_preset.update_from_source", json!({"preset":preset}));
    let project = engine.project().unwrap();
    assert_eq!(project.fab_preset(preset).unwrap().constructions.len(), 37);
    assert_eq!(*project.pcb().unwrap().stackup(), original);
}

#[test]
fn a_migrated_board_switches_its_pours_only_when_the_copied_hole_rules_are_applied() {
    use reda_engine::model::RuleKind;
    let (work, mut e) =
        open_reference_copy(&projects_dir().join("formats/v1/blinky_555"), "Blinky.reda");
    let board_holes = |e: &reda_testkit::TestEngine| {
        e.project()
            .unwrap()
            .pcb()
            .unwrap()
            .board_rule(RuleKind::Clearance)
            .values
            .hole
    };
    let project = e.project().unwrap();
    let job = project.output_jobs().next().unwrap().uuid;
    let preset = project.fab_presets().next().unwrap();
    let copy = preset.uuid;
    // The v0.1 preset copy gains the Generic hole limits. The board itself keeps
    // its v0.1 clearance until Apply, including its old pours around mounting holes.
    assert_eq!(
        serde_json::to_value(preset.rules.clearance_hole).unwrap(),
        json!({"via":250000,"pad":250000})
    );
    assert_eq!(board_holes(&e), None);
    let copper = work.path().join("outputs/JLCPCB/fab/Blinky.gbl");
    e.run_ok("output.job.run", json!({"job":job}));
    let old = std::fs::read(&copper).unwrap();

    e.run_ok("pcb.rules.apply_fab_preset", json!({"preset":copy}));
    e.run_ok("project.save", json!({}));
    e.run_ok("project.close", json!({}));
    e.run_ok(
        "project.open",
        json!({"path":work.path().join("Blinky.reda")}),
    );
    assert_eq!(
        serde_json::to_value(board_holes(&e)).unwrap(),
        json!({"via":250000,"pad":250000})
    );
    e.run_ok("output.job.run", json!({"job":job}));
    assert!(
        std::fs::read(copper).unwrap() != old,
        "Apply switches the mounting-hole and plated-hole clearances"
    );
}
