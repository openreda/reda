//! The Re-annotate dialogs in the window: Design ▸ Re-annotate… with its settings, sheet
//! grid and lock switches, and Design ▸ Re-annotate (last settings)…, which confirms a
//! count before running the saved settings.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn place(shell: &mut Shell, x: i64) {
    shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K,
                   "at": {"x": format!("{x}mil"), "y": 0}}),
        )
        .unwrap();
}

/// R1, R2 and R3 placed at 0, 1000 and -1000 mil, so R3 sits leftmost.
fn harness(dir: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    for x in [0, 1000, -1000] {
        place(&mut shell, x);
    }
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

/// The Designators left to right.
fn designators(h: &Harness<'_, App>) -> Vec<String> {
    let project = h.state().shell().engine().project().unwrap();
    let sheet: Uuid = SHEET.parse().unwrap();
    let mut out: Vec<(i64, String)> = project
        .schematic_sheet(sheet)
        .unwrap()
        .units()
        .map(|u| (u.at.x.0, u.designator.clone()))
        .collect();
    out.sort();
    out.into_iter().map(|(_, d)| d).collect()
}

fn design_menu(h: &mut Harness<'static, App>, item: &str) {
    h.get_by_role_and_label(Role::Button, "Design").click();
    h.run();
    h.get_by_label(item).click();
    h.run();
}

#[test]
fn the_dialog_shows_its_settings_and_a_row_lock_keeps_a_designator() {
    let dir = TempDir::new("rw-lock");
    let mut h = harness(&dir);
    design_menu(&mut h, "Re-annotate…");
    for label in [
        "Renumber all",
        "Fix duplicates only",
        "Sheets",
        "Sheet × 100",
        "Only changed",
    ] {
        h.get_by_label(label);
    }
    h.get_by_role_and_label(Role::TextInput, "Threshold");
    for source in ["Schematic", "Board"] {
        h.get_by_role_and_label(Role::RadioButton, source);
    }
    h.get_by_label("R3 » R1");
    // Locking R3 keeps it: R1 and R2 keep theirs too, so nothing would change.
    h.get_by_role_and_label(Role::CheckBox, "Lock R3").click();
    h.run();
    assert!(h.query_by_label("R3 » R1").is_none());
    h.get_by_label("Nothing would be renumbered.");
    // Sheet × 100 numbers from R101.
    h.get_by_label("Sheet × 100").click();
    h.run();
    h.get_by_label("R1 » R101");
    h.get_by_role_and_label(Role::Button, "Re-annotate").click();
    h.run();
    assert_eq!(designators(&h), ["R3", "R101", "R102"]);
}

#[test]
fn reannotate_last_settings_confirms_the_count_then_runs_them() {
    let dir = TempDir::new("rw-last");
    let mut h = harness(&dir);
    design_menu(&mut h, "Re-annotate…");
    h.get_by_role_and_label(Role::Button, "Re-annotate").click();
    h.run();
    assert_eq!(designators(&h), ["R1", "R2", "R3"]);

    // A new part at the far left: the saved settings would renumber all four.
    place(h.state_mut().shell_mut(), -2000);
    h.run();
    assert_eq!(designators(&h), ["R4", "R1", "R2", "R3"]);
    design_menu(&mut h, "Re-annotate (last settings)…");
    h.get_by_label("The last Re-annotate settings renumber 4 Components.");
    h.get_by_role_and_label(Role::Button, "Re-annotate").click();
    h.run();
    assert_eq!(designators(&h), ["R1", "R2", "R3", "R4"]);
    let journal = h.state().shell().engine().journal();
    assert_eq!(journal.last().unwrap().params, json!({"saved": true}));
}
