//! The derived sheet-name label above a sheet's Footprints that lie entirely outside the
//! Board Outline (docs/spec/v0.1/sync.md#placement-of-new-footprints).

use std::path::Path;

use reda_engine::Engine;
use reda_engine::command::{FixedClock, SeededIds};
use reda_engine::model::{DocumentKind, Uuid};
use reda_engine::render::{BoardStyle, Primitive, Scene, SceneLayer};
use reda_testkit::{TempDir, TestEngine, read_tree, starter_library};
use serde_json::json;

fn open_blinky() -> (TempDir, TestEngine) {
    let work = TempDir::new("sheet-labels");
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/blinky_555");
    for (rel, bytes) in read_tree(&from) {
        let path = work.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let mut engine = TestEngine(
        Engine::builder()
            .version(reda_testkit::TEST_VERSION)
            .ids(SeededIds::new(1 << 32))
            .clock(FixedClock::EPOCH)
            .user_dirs(None)
            .starter_library(Some(starter_library()))
            .build(),
    );
    engine.run_ok(
        "project.open",
        json!({"path": work.path().join("Blinky.reda")}),
    );
    (work, engine)
}

/// The labels drawn: (tag, text, left end of the baseline).
fn labels(scene: &Scene) -> Vec<(Uuid, String, (i64, i64))> {
    scene
        .items
        .iter()
        .filter(|i| i.layer == SceneLayer::Annotation)
        .filter_map(|i| match &i.primitive {
            Primitive::Glyphs(run) => Some((i.object, run.text.clone(), (run.at.x.0, run.at.y.0))),
            _ => None,
        })
        .collect()
}

#[test]
fn a_sheets_footprints_off_the_board_get_its_name_above_them_until_they_move_on() {
    let (_dir, mut engine) = open_blinky();
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let doc = pcb.uuid;
    let sheet = project
        .documents()
        .find(|d| d.kind == DocumentKind::Schematic)
        .unwrap()
        .uuid;
    let footprints: Vec<Uuid> = pcb.footprints().map(|f| f.uuid).collect();
    let style = BoardStyle::default();
    assert!(
        labels(&engine.board_scene(doc, &style).unwrap()).is_empty(),
        "every part is on the board"
    );

    // Two parts moved far right of the board: one label, at their group's top-left.
    engine.run_ok(
        "pcb.move",
        json!({"objects": [footprints[0], footprints[1]], "by": {"x": "200mm", "y": "0mm"}}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let boxes: Vec<_> = [footprints[0], footprints[1]]
        .iter()
        .map(|f| reda_engine::pcb::placed::footprint_bounds(pcb, pcb.footprint(*f).unwrap()))
        .collect();
    let left = boxes.iter().map(|b| b.min.x.0).min().unwrap();
    let top = boxes.iter().map(|b| b.max.y.0).max().unwrap();
    let scene = engine.board_scene(doc, &style).unwrap();
    assert_eq!(
        labels(&scene),
        [(sheet, "Sheet 1".to_owned(), (left, top + 1_000_000))]
    );

    // Derived live: renaming the sheet renames the label.
    engine.run_ok(
        "document.rename",
        json!({"document": sheet, "title": "Timer"}),
    );
    let scene = engine.board_scene(doc, &style).unwrap();
    assert_eq!(labels(&scene)[0].1, "Timer");

    // Moved back onto the board, the label goes.
    engine.run_ok(
        "pcb.move",
        json!({"objects": [footprints[0], footprints[1]], "by": {"x": "-200mm", "y": "0mm"}}),
    );
    assert!(labels(&engine.board_scene(doc, &style).unwrap()).is_empty());
}

#[test]
fn with_no_board_outline_every_footprint_counts_as_outside() {
    let (_dir, mut engine) = open_blinky();
    let doc = engine.project().unwrap().pcb().unwrap().uuid;
    engine.run_ok("pcb.board_outline.delete", json!({}));
    let scene = engine.board_scene(doc, &BoardStyle::default()).unwrap();
    let found = labels(&scene);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].1, "Sheet 1");
}
