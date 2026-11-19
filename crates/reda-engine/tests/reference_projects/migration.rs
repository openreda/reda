//! Compatibility with manufacturing files written by the 0.1.0 release.
use super::{V1_PROJECTS, assert_golden_tree, open_copy_from, projects_dir};
use reda_testkit::read_tree;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

/// Only the two reference copper files and the plated through drill have a
/// changed file-function record. Compare all other bytes, including line endings.
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
    } else if path.ends_with("-PTH.drl") {
        (
            [
                "TF.FileFunction,Plated,1,2,PTH,Drill\n",
                "TF.FileFunction,Plated,1,2,PTH,Mixed\n",
            ]
            .into_iter()
            .find(|line| text.contains(line))?,
            "TF.FileFunction,Plated,1,2,PTH\n",
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
        let (work, mut engine) = open_copy_from(&frozen.join(name), project);
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
