//! DXF import in the window: the imported objects are selected afterwards.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use serde_json::json;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-import-dxf/tests/fixtures")
        .join(name)
}

#[test]
fn the_imported_objects_are_selected_and_one_undo_takes_them_away() {
    let home = TempDir::new("dxf-ui-home");
    let work = TempDir::new("dxf-ui-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    let out = shell
        .run("pcb.import_dxf", json!({"path": fixture("lwpolyline.dxf")}))
        .unwrap();
    let objects: Vec<Uuid> = serde_json::from_value(out["result"]["objects"].clone()).unwrap();
    assert_eq!(objects.len(), 6);
    assert_eq!(shell.selection().objects, objects);

    shell.run("edit.undo", json!({"document": pcb})).unwrap();
    let board = shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(board.graphics().count(), 0);
    assert!(shell.selection().objects.is_empty());
}
