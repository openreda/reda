//! What the Library panel and the placement Tool read from the engine without running a
//! Command: the scanned Libraries, one Part copied from them, and the Project as an edit
//! would leave it (a preview), none of which touches the Journal, undo or dirty state.

use std::path::{Path, PathBuf};

use reda_engine::library::{ReadyParts, SearchQuery};
use reda_engine::model::{ComponentType, ObjectType};
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
/// The schematic fixture Library's R_10K and Mechanical Hole.
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const HOLE: &str = "a0000000-0000-4000-8000-000000000130";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

#[test]
fn the_scanned_libraries_are_read_without_a_command() {
    let dir = TempDir::new("preview-scan");
    let engine = demo(&dir);
    let journal = engine.journal().len();

    let index = engine.libraries();
    let mechanical = index.search(&SearchQuery {
        component_type: Some(ComponentType::Mechanical),
        ..SearchQuery::default()
    });
    let names: Vec<&str> = mechanical.iter().map(|h| h.part.name.as_str()).collect();
    assert_eq!(names, ["Hole"]);
    assert_eq!(mechanical[0].part.uuid.to_string(), HOLE);

    let copy = engine.library_part(R_10K.parse().unwrap()).unwrap();
    assert_eq!(copy.part.name, "R_10k");
    assert_eq!(copy.symbol.unwrap().designator_prefix, "R");
    assert_eq!(copy.footprints.len(), 2);
    assert_eq!(engine.journal().len(), journal);
}

#[test]
fn a_preview_shows_an_edit_without_making_it() {
    let dir = TempDir::new("preview-place");
    let engine = demo(&dir);
    let journal = engine.journal().len();
    let copy = engine.library_part(R_10K.parse().unwrap()).unwrap();

    let params = json!({"document": SHEET, "part": R_10K, "at": {"x": "10mm", "y": "0mm"}});
    let (project, result) = engine
        .preview(
            "schematic.component.place",
            params,
            &ReadyParts::new(vec![copy]),
        )
        .unwrap();

    assert_eq!(result["designator"], "R1");
    let units = |p: &reda_engine::model::Project| {
        p.objects()
            .iter()
            .filter(|o| o.object_type() == ObjectType::Unit)
            .count()
    };
    assert_eq!(units(&project), 1);
    assert_eq!(units(engine.project().unwrap()), 0);
    assert_eq!(engine.journal().len(), journal);
    assert!(!engine.is_dirty());

    // Only edits preview, and a Part the source lacks is not found.
    let query = engine.preview("project.overview", json!({}), &ReadyParts::new(vec![]));
    assert!(query.is_err());
    let missing = engine.preview(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": 0, "y": 0}}),
        &ReadyParts::new(vec![]),
    );
    assert_eq!(
        missing.unwrap_err().code,
        reda_engine::command::ErrorCode::NotFound
    );
}
