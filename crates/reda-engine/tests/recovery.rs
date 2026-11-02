//! Recovery Snapshots: writing them from a read view, finding them after an unclean exit,
//! and `recovery.restore` loading them back as unsaved changes.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_engine::model::{Scope, Uuid};
use reda_engine::recovery::{self, Found, State, Target};
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

/// A saved Project `Demo` in `dir`, open in a fresh engine.
fn project(dir: &Path) -> (TestEngine, PathBuf) {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    (engine, dir.join("Demo"))
}

fn add_wire(engine: &mut TestEngine) {
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [{"x": "0mm", "y": "0mm"}, {"x": "10mm", "y": "0mm"}]}),
    );
}

fn wires(engine: &TestEngine) -> usize {
    engine
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .wires()
        .count()
}

/// Snapshots every dirty scope, as the app's background writer does.
fn snapshot_dirty(engine: &TestEngine, dir: &Path) {
    let scopes = engine.dirty_scopes();
    recovery::View::of(engine)
        .write(dir, &scopes, 1_000)
        .unwrap();
}

/// The engine after an unclean exit: the Project reopened from disk.
fn reopen(folder: &Path) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder}));
    engine
}

fn found(dir: &Path, folder: &Path) -> Vec<Found> {
    recovery::scan(dir, Some(folder))
}

#[test]
fn a_dirty_sheet_snapshot_restores_as_unsaved_changes() {
    let tmp = TempDir::new("recovery-sheet");
    let (mut engine, folder) = project(tmp.path());
    add_wire(&mut engine);
    let dir = recovery::project_dir(&folder);
    snapshot_dirty(&engine, &dir);

    assert!(dir.join(format!("{SHEET}.redasch")).is_file());
    assert!(dir.join(recovery::MANIFEST).is_file());

    let mut after = reopen(&folder);
    assert_eq!(wires(&after), 0, "the file on disk has no wire");
    let rows = found(&dir, &folder);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.snapshot.path, PathBuf::from("Demo.redasch"));
    assert!(matches!(row.snapshot.target, Target::Document { .. }));
    assert_eq!(row.state, State::Ready);
    assert!(!row.disk_changed && !row.missing);
    assert_eq!(row.snapshot.taken, 1_000);

    after.run_ok(
        "recovery.restore",
        json!({"snapshots": [row.snapshot.file.clone()]}),
    );
    assert_eq!(wires(&after), 1);
    assert_eq!(after.dirty_scopes(), [Scope::Document(sheet())]);
    after.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(wires(&after), 0, "a restore undoes like any edit");
}

#[test]
fn only_the_scopes_asked_for_are_written_and_the_first_disk_hash_is_kept() {
    let tmp = TempDir::new("recovery-scopes");
    let (mut engine, folder) = project(tmp.path());
    add_wire(&mut engine);
    engine.run_ok("project.properties.set", json!({"title": "Amp"}));
    let dir = recovery::project_dir(&folder);
    recovery::View::of(&engine)
        .write(&dir, &[Scope::Project], 1)
        .unwrap();
    assert!(dir.join("project.reda").is_file());
    assert!(!dir.join(format!("{SHEET}.redasch")).exists());

    // The file on disk changes behind REDA's back; the next snapshot keeps the hash of the
    // file the edits started from, so recovery can tell.
    std::fs::write(folder.join("Demo.reda"), b"changed").unwrap();
    recovery::View::of(&engine)
        .write(&dir, &[Scope::Project], 2)
        .unwrap();
    let rows = found(&dir, &folder);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].disk_changed);
    assert_eq!(rows[0].snapshot.taken, 2);
}

#[test]
fn a_restored_project_file_brings_back_its_properties() {
    let tmp = TempDir::new("recovery-project");
    let (mut engine, folder) = project(tmp.path());
    engine.run_ok("project.properties.set", json!({"title": "Amp"}));
    let dir = recovery::project_dir(&folder);
    snapshot_dirty(&engine, &dir);

    let mut after = reopen(&folder);
    after.run_ok("recovery.restore", json!({"snapshots": ["project.reda"]}));
    let project = after.project().unwrap();
    assert_eq!(project.props().properties.title, "Amp");
    assert_eq!(after.dirty_scopes(), [Scope::Project]);
}

#[test]
fn a_new_sheet_never_saved_is_recreated_at_its_manifest_path() {
    let tmp = TempDir::new("recovery-new-sheet");
    let (mut engine, folder) = project(tmp.path());
    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let uuid: Uuid = serde_json::from_value(added["result"]["uuid"].clone()).unwrap();
    let dir = recovery::project_dir(&folder);
    snapshot_dirty(&engine, &dir);

    let mut after = reopen(&folder);
    let rows = found(&dir, &folder);
    let row = rows
        .iter()
        .find(|r| matches!(&r.snapshot.target, Target::Document { document } if document.uuid == uuid))
        .expect("the new sheet has a snapshot");
    assert!(row.missing, "its file was never written");
    after.run_ok(
        "recovery.restore",
        json!({"snapshots": [row.snapshot.file.clone()]}),
    );
    let project = after.project().unwrap();
    assert!(project.document(uuid).is_some());
    assert!(after.dirty_scopes().contains(&Scope::Document(uuid)));
}

#[test]
fn an_unparsable_snapshot_is_reported_and_cannot_be_restored() {
    let tmp = TempDir::new("recovery-bad");
    let (mut engine, folder) = project(tmp.path());
    add_wire(&mut engine);
    let dir = recovery::project_dir(&folder);
    snapshot_dirty(&engine, &dir);
    let file = format!("{SHEET}.redasch");
    std::fs::write(dir.join(&file), b"{ not json").unwrap();

    let mut after = reopen(&folder);
    let rows = found(&dir, &folder);
    assert!(matches!(&rows[0].state, State::Unparsable(why) if why.contains("JSON")));
    let err = after.run_err("recovery.restore", json!({"snapshots": [file]}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(wires(&after), 0);
}

#[test]
fn removing_and_discarding_deletes_snapshots_and_their_manifest_rows() {
    let tmp = TempDir::new("recovery-remove");
    let (mut engine, folder) = project(tmp.path());
    add_wire(&mut engine);
    engine.run_ok("project.properties.set", json!({"title": "Amp"}));
    let dir = recovery::project_dir(&folder);
    snapshot_dirty(&engine, &dir);
    assert_eq!(found(&dir, &folder).len(), 2);

    recovery::remove(&dir, &[Scope::Project]).unwrap();
    assert!(!dir.join("project.reda").exists());
    assert_eq!(found(&dir, &folder).len(), 1);

    recovery::discard(&dir, &[format!("{SHEET}.redasch")]).unwrap();
    assert!(found(&dir, &folder).is_empty());

    std::fs::write(dir.join(recovery::JOURNAL), b"{}\n").unwrap();
    recovery::clear(&dir).unwrap();
    assert!(!dir.exists(), "a clean close leaves nothing behind");
}

#[test]
fn a_snapshot_missing_from_the_manifest_is_still_listed() {
    let tmp = TempDir::new("recovery-orphan");
    let (_engine, folder) = project(tmp.path());
    let dir = recovery::project_dir(&folder);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{SHEET}.redasch")), b"{}").unwrap();
    let rows = found(&dir, &folder);
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].state, State::Unparsable(_)));
}

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

#[test]
fn a_new_library_element_snapshots_with_its_absolute_path_and_restores() {
    use reda_engine::format::library::write_library_info;
    use reda_engine::model::LibraryInfo;
    let tmp = TempDir::new("recovery-element");
    let (mut engine, folder) = project(tmp.path());
    let lib = folder.join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let info = LibraryInfo {
        uuid: LIBRARY.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(lib.join("Mine.redalib"), write_library_info(&info).unwrap()).unwrap();
    engine.run_ok("project.library.add", json!({"path": "lib"}));
    let out = engine.run_ok(
        "library.element.new",
        json!({"library": LIBRARY, "kind": "symbol", "name": "Opamp"}),
    );
    let element: Uuid = out["element"].as_str().unwrap().parse().unwrap();
    let dir = recovery::project_dir(&folder);
    recovery::View::of(&engine)
        .write(&dir, &[Scope::Element(element)], 5)
        .unwrap();

    let rows = found(&dir, &folder);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].snapshot.path, lib.join("symbols/Opamp.redasym"));
    assert!(rows[0].missing);
    let mut after = reopen(&folder);
    after.run_ok(
        "recovery.restore",
        json!({"snapshots": [rows[0].snapshot.file.clone()]}),
    );
    assert!(after.project().unwrap().element(element).is_some());
    assert!(after.dirty_scopes().contains(&Scope::Element(element)));
}

#[test]
fn starter_library_elements_never_get_snapshots() {
    let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
    let tmp = TempDir::new("recovery-starter");
    let mut engine = reda_testkit::test_engine_with_library(&starter);
    engine.run_ok("project.new", json!({"path": tmp.path(), "name": "Demo"}));
    let capacitor = "e2a20189-4b01-467b-a2c0-1e5ddd811319";
    let out = engine.run_ok("library.element.open", json!({"element": capacitor}));
    assert_eq!(out["read_only"], true);
    let folder = tmp.path().join("Demo");
    let dir = recovery::project_dir(&folder);
    recovery::View::of(&engine)
        .write(&dir, &[Scope::Element(capacitor.parse().unwrap())], 5)
        .unwrap();
    assert!(found(&dir, &folder).is_empty());
}
