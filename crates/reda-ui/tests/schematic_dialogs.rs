//! The schematic's dialogs as models on a Shell, without a window: Select Similar, Re-annotate with its old → new
//! preview, Update from Library with its preview, and Export PDF.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::Nm;
use reda_ui::schematic_dialogs::{
    ReannotateDialog, ReannotateScope, SimilarDialog, UpdateDialog, export_pdf,
};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";

fn fixture_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell_with(dir: &TempDir, library: PathBuf) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .new_project(&dir.path().join("project"), "Demo")
        .unwrap();
    shell
}

fn shell(dir: &TempDir) -> Shell {
    shell_with(dir, fixture_library())
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// Places `part` on `document` at (`x`, `y`) mil; returns (Unit, Component).
fn place(shell: &mut Shell, document: Uuid, part: &str, x: i64, y: i64) -> (Uuid, Uuid) {
    let out = shell
        .run(
            "schematic.component.place",
            json!({"document": document, "part": part,
                   "at": {"x": format!("{x}mil"), "y": format!("{y}mil")}}),
        )
        .unwrap();
    (
        uuid(&out["result"]["unit"]),
        uuid(&out["result"]["component"]["uuid"]),
    )
}

fn add_sheet(shell: &mut Shell) -> Uuid {
    let added = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    uuid(&added["result"]["uuid"])
}

fn designator(shell: &Shell, unit: Uuid) -> String {
    let project = shell.engine().project().unwrap();
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .find_map(|s| s.unit(unit))
        .unwrap()
        .designator
        .clone()
}

#[test]
fn select_similar_selects_matching_units_on_the_sheet_or_across_the_project() {
    let dir = TempDir::new("similar");
    let mut shell = shell(&dir);
    let (r1, _) = place(&mut shell, sheet(), R_10K, 0, 0);
    let (r2, _) = place(&mut shell, sheet(), R_10K, 1000, 0);
    place(&mut shell, sheet(), C_100N, 2000, 0);
    let second = add_sheet(&mut shell);
    let (r3, _) = place(&mut shell, second, R_10K, 0, 0);

    // It needs a selected Unit to compare with.
    assert!(SimilarDialog::new(&shell).is_none());
    shell
        .run("selection.set", json!({"objects": [r1]}))
        .unwrap();
    let mut dialog = SimilarDialog::new(&shell).unwrap();
    // The reference's free-form fields can be matched on too.
    assert!(
        dialog
            .criteria()
            .iter()
            .any(|c| c.param == json!({"field": "LCSC"}))
    );
    dialog.set(&json!("value"), true);
    dialog.apply(&mut shell).unwrap();
    assert_eq!(shell.selection().objects, [r1, r2]);

    dialog.project = true;
    dialog.apply(&mut shell).unwrap();
    assert_eq!(shell.selection().objects, [r1, r2, r3]);

    // With nothing to match on, there is nothing to run.
    for c in dialog.criteria().to_vec() {
        dialog.set(&c.param, false);
    }
    assert!(!dialog.can_apply());
}

#[test]
fn reannotate_previews_old_to_new_honours_locks_and_applies_as_one_command() {
    let dir = TempDir::new("reannotate");
    let mut shell = shell(&dir);
    // Placed R1, R2, R3 from left to right as 0, 1000, -1000 mil.
    let (a, _) = place(&mut shell, sheet(), R_10K, 0, 0);
    let (b, b_component) = place(&mut shell, sheet(), R_10K, 1000, 0);
    let (c, _) = place(&mut shell, sheet(), R_10K, -1000, 0);

    let mut dialog = ReannotateDialog::new(&shell);
    assert_eq!(dialog.scope, ReannotateScope::Project);
    dialog.refresh(&mut shell);
    let renames: Vec<(String, String)> = dialog
        .changes()
        .map(|r| (r.old.clone(), r.new.clone()))
        .collect();
    assert_eq!(
        renames,
        [
            ("R3".to_owned(), "R1".to_owned()),
            ("R1".to_owned(), "R2".to_owned()),
            ("R2".to_owned(), "R3".to_owned()),
        ]
    );
    // Nothing changes before Apply.
    assert_eq!(designator(&shell, c), "R3");

    // Locking R2 keeps it; the preview follows.
    dialog.set_locked(&mut shell, b_component, true);
    assert_eq!(dialog.locked().len(), 1);
    let renames: Vec<(String, String)> = dialog
        .changes()
        .map(|r| (r.old.clone(), r.new.clone()))
        .collect();
    assert_eq!(
        renames,
        [
            ("R3".to_owned(), "R1".to_owned()),
            ("R1".to_owned(), "R3".to_owned())
        ]
    );

    let before = shell.engine().journal().len();
    dialog.apply(&mut shell).unwrap();
    assert_eq!(
        (
            designator(&shell, a),
            designator(&shell, b),
            designator(&shell, c)
        ),
        ("R3".to_owned(), "R2".to_owned(), "R1".to_owned())
    );
    let ran: Vec<&str> = shell.engine().journal()[before..]
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(ran, ["schematic.reannotate"]);

    // Scope selection with nothing selected cannot run.
    dialog.scope = ReannotateScope::Selection;
    dialog.refresh(&mut shell);
    assert!(!dialog.can_apply());
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn update_from_library_previews_the_changes_and_updates_the_checked_components() {
    let dir = TempDir::new("update");
    let library = dir.path().join("Test");
    copy_dir(&fixture_library(), &library);
    let mut shell = shell_with(&dir, library.clone());
    let (r1, _) = place(&mut shell, sheet(), R_10K, 0, 0);

    assert!(UpdateDialog::new(&mut shell).rows().is_empty());
    let part = library.join("parts/R_10k.redapart");
    let text = std::fs::read_to_string(&part).unwrap();
    std::fs::write(
        &part,
        text.replace("\"value\":\"10k\"", "\"value\":\"22k\""),
    )
    .unwrap();

    let mut dialog = UpdateDialog::new(&mut shell);
    let rows = dialog.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].designator, "R1");
    assert!(rows[0].checked);
    assert!(
        rows[0]
            .changes
            .iter()
            .any(|c| c.contains("10k") && c.contains("22k")),
        "{:?}",
        rows[0].changes
    );
    // Nothing changes before Apply.
    let value = |shell: &Shell| {
        let project = shell.engine().project().unwrap();
        let sheet = project.schematic_sheet(sheet()).unwrap();
        sheet.unit(r1).unwrap().fields.value.clone()
    };
    assert_eq!(value(&shell), "10k");
    dialog.apply(&mut shell).unwrap();
    assert_eq!(value(&shell), "22k");
    assert!(UpdateDialog::new(&mut shell).rows().is_empty());
}

#[test]
fn export_pdf_writes_every_sheet_and_says_so() {
    let dir = TempDir::new("export-pdf");
    let mut shell = shell(&dir);
    add_sheet(&mut shell);
    let path = dir.path().join("out.pdf");
    export_pdf(&mut shell, &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert!(shell.note().unwrap().contains("2"), "{:?}", shell.note());
}
