//! The Change List window, driven through AccessKit with `egui_kittest`: the tree grid's
//! columns, group headers with counts and tick-all, the toolbar, row selection with
//! Space and the context menu, the Export button, and Show with its banner (Back to
//! Change List and the switch between board and sheet).

use std::path::{Path, PathBuf};

use egui::Key;
use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::layout::TabKey;
use reda_ui::settings::change_list;
use reda_ui::settings::{FileRequest, Outcome, SettingsDialog};
use reda_ui::shell::Shell;
use reda_ui::show_on_board::Side;
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// A Shell on a Project with R1 and R2 whose pin 2s are wired and labelled SIG, not yet
/// synced: five pending changes.
fn shell(work: &Path) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Demo").unwrap();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": R_10K, "at": {"x": x, "y": 0}}),
            )
            .unwrap();
    }
    shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": [
                {"x": "0mm", "y": "-3.81mm"}, {"x": "10mm", "y": "-3.81mm"}]}),
        )
        .unwrap();
    shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": "SIG", "at": {"x": "5mm", "y": "-3.81mm"}}),
        )
        .unwrap();
    shell
}

struct Window {
    dialog: SettingsDialog,
    shell: Shell,
    outcome: Outcome,
}

/// The Change List window's content alone, drawn as its tab draws it.
fn window(work: &Path) -> Harness<'static, Window> {
    let shell = shell(work);
    let dialog = change_list::dialog(shell.engine().project().unwrap());
    let state = Window {
        dialog,
        shell,
        outcome: Outcome::Open,
    };
    let mut h = Harness::builder()
        .with_size([1000.0, 680.0])
        .build_ui_state(
            |ui, w: &mut Window| {
                let outcome = w.dialog.show(ui, &mut w.shell);
                if outcome != Outcome::Open {
                    w.outcome = outcome;
                }
            },
            state,
        );
    h.run();
    h
}

fn ticked(h: &Harness<'static, Window>, label: &str) -> bool {
    h.get_by_role_and_label(Role::CheckBox, label)
        .accesskit_node()
        .toggled()
        == Some(Toggled::True)
}

fn row_selected(h: &Harness<'static, Window>, label: &str) -> bool {
    h.get_by_role_and_label(Role::Row, label)
        .accesskit_node()
        .toggled()
        == Some(Toggled::True)
}

#[test]
fn the_tree_grid_has_columns_group_counts_and_tick_all() {
    let work = TempDir::new("clw-grid");
    let mut h = window(work.path());
    for column in ["Kind", "Target", "From", "To", "Status"] {
        h.get_by_label(column);
    }
    h.get_by_label("Add Components (2)");
    h.get_by_label("Pad Nets (2)");
    assert_eq!(h.get_all_by_label("(none) to R_0603").count(), 2);
    assert_eq!(h.get_all_by_label("(no Net) to SIG").count(), 2);
    h.get_by_role_and_label(Role::Row, "Add R1 (R_0603): OK");

    h.get_by_role_and_label(Role::CheckBox, "Tick all: Pad Nets")
        .click();
    h.run();
    assert!(!ticked(&h, "R1.2: (no Net) to SIG"));
    assert!(!ticked(&h, "R2.2: (no Net) to SIG"));
    assert!(ticked(&h, "Add Net SIG"));
}

#[test]
fn the_toolbar_filters_folds_and_includes_or_excludes() {
    let work = TempDir::new("clw-toolbar");
    let mut h = window(work.path());
    h.get_by_role_and_label(Role::Button, "Collapse All")
        .click();
    h.run();
    assert!(h.query_all_by_label("(none) to R_0603").next().is_none());
    h.get_by_label("Add Components (2)");
    h.get_by_role_and_label(Role::Button, "Expand Add Components")
        .click();
    h.run();
    assert_eq!(h.get_all_by_label("(none) to R_0603").count(), 2);
    h.get_by_role_and_label(Role::Button, "Expand All").click();
    h.run();
    assert_eq!(h.get_all_by_label("(no Net) to SIG").count(), 2);

    h.get_by_role_and_label(Role::Button, "Exclude All").click();
    h.run();
    assert!(!ticked(&h, "Add R1 (R_0603)"));
    h.get_by_role_and_label(Role::Button, "Include All").click();
    h.run();
    assert!(ticked(&h, "Add R1 (R_0603)"));

    h.get_by_role_and_label(Role::TextInput, "Filter changes")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Filter changes")
        .type_text("R2.2");
    h.run();
    assert!(h.query_by_label("Add Components (2)").is_none());
    h.get_by_label("Pad Nets (1)");

    h.get_by_role_and_label(Role::CheckBox, "Only Problems")
        .click();
    h.run();
    assert!(h.query_by_label("Pad Nets (1)").is_none());
    assert!(
        h.state()
            .dialog
            .page::<change_list::ChangeListPage>()
            .unwrap()
            .only_problems()
    );
}

#[test]
fn rows_multi_select_space_toggles_and_the_menu_inverts() {
    let work = TempDir::new("clw-select");
    let mut h = window(work.path());
    h.get_by_role_and_label(Role::Row, "Add R1 (R_0603): OK")
        .click();
    h.run();
    h.get_by_role_and_label(Role::Row, "Add Net SIG: OK")
        .click_modifiers(egui::Modifiers::SHIFT);
    h.run();
    assert!(row_selected(&h, "Add R1 (R_0603): OK"));
    assert!(row_selected(&h, "Add R2 (R_0603): OK"));
    assert!(row_selected(&h, "Add Net SIG: OK"));
    assert!(!row_selected(&h, "R1.2: (no Net) to SIG: OK"));

    h.key_press(Key::Space);
    h.run();
    assert!(!ticked(&h, "Add R1 (R_0603)"));
    assert!(!ticked(&h, "Add Net SIG"));

    // Invert from the context menu flips the selection back.
    h.get_by_role_and_label(Role::Row, "Add R2 (R_0603): OK")
        .click_secondary();
    h.run();
    h.get_by_role_and_label(Role::Button, "Invert").click();
    h.run();
    assert!(ticked(&h, "Add R1 (R_0603)"));
    assert!(ticked(&h, "Add R2 (R_0603)"));
    assert!(ticked(&h, "Add Net SIG"));

    // Exclude All of This Kind, on one Pad Net.
    h.get_by_role_and_label(Role::Row, "R1.2: (no Net) to SIG: OK")
        .click_secondary();
    h.run();
    h.get_by_role_and_label(Role::Button, "Exclude All of This Kind")
        .click();
    h.run();
    assert!(!ticked(&h, "R2.2: (no Net) to SIG"));
    assert!(ticked(&h, "Add Net SIG"));
}

#[test]
fn export_asks_for_a_file_and_apply_keeps_applied_rows() {
    let work = TempDir::new("clw-export");
    let mut h = window(work.path());
    h.get_by_role_and_label(Role::Button, "Export…").click();
    h.run();
    assert_eq!(
        h.state().outcome,
        Outcome::PickFile(FileRequest::ExportChangeList)
    );

    h.get_by_role_and_label(Role::CheckBox, "Add Net SIG")
        .click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    h.get_by_role_and_label(Role::Row, "Add R1 (R_0603): Applied");
    h.get_by_role_and_label(Role::Row, "Add Net SIG: OK");
    assert!(!ticked(&h, "Add Net SIG"), "the untick is kept");
    h.get_by_label("Add Components (0)");
}

/// The whole app, with the Change List opened from F8.
fn app(work: &Path) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell(work)));
    h.run();
    h.key_press(Key::F8);
    h.run();
    h
}

#[test]
fn show_raises_the_canvas_view_only_with_back_and_a_board_sheet_switch() {
    let work = TempDir::new("clw-show");
    let mut h = app(work.path());
    // A synced Component whose Designator changed has both sides.
    {
        let shell = h.state_mut().shell_mut();
        let list = shell.run("sync.preview", json!({})).unwrap();
        shell
            .run("sync.apply", json!({"hash": list["hash"]}))
            .unwrap();
        let project = shell.engine().project().unwrap();
        let sheet = project.schematic_sheet(SHEET.parse().unwrap()).unwrap();
        let r1 = (sheet.units().find(|u| u.designator == "R1"))
            .unwrap()
            .component
            .uuid;
        shell
            .run(
                "schematic.component.set",
                json!({"components": [r1], "designator": "R9"}),
            )
            .unwrap();
    }
    h.run();
    h.get_by_role_and_label(Role::Row, "Designator R1 to R9: OK")
        .click_secondary();
    h.run();
    h.get_by_role_and_label(Role::Button, "Show").click();
    h.run();
    let pcb = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .uuid;
    let sheet = SHEET.parse().unwrap();
    assert_eq!(h.state().shell().shown_side(), Some(Side::Board));
    assert_eq!(
        h.state().shell().layout().focus,
        Some(TabKey::Document(pcb))
    );
    h.get_by_label("1 object shown (view only)");
    h.get_by_role_and_label(Role::Button, "Show on Sheet")
        .click();
    h.run();
    assert_eq!(h.state().shell().shown_side(), Some(Side::Sheet));
    assert!(h.state().shell().view_only(sheet));
    assert_eq!(
        h.state().shell().layout().focus,
        Some(TabKey::Document(sheet))
    );

    h.get_by_role_and_label(Role::Button, "Back to Change List")
        .click();
    h.run();
    assert_eq!(h.state().shell().shown_side(), None);
    assert!(!h.state().shell().view_only(sheet));
    h.get_by_label("Update PCB from Schematic");
}

#[test]
fn a_stale_list_shows_one_conflict_banner_and_refreshes() {
    let work = TempDir::new("clw-stale");
    let mut h = window(work.path());
    h.get_by_label("Add Components (2)");
    // The schematic changes after the window last drew its list, so Apply sends a stale
    // hash.
    let w = h.state_mut();
    w.shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K, "at": {"x": "20mm", "y": 0}}),
        )
        .unwrap();
    let err = w.dialog.apply(&mut w.shell).unwrap_err();
    assert_eq!(err.code, reda_ui::engine::command::ErrorCode::Conflict);
    h.run();
    h.get_by_label(
        "The design changed since this list was made, so nothing was applied. \
         The list is refreshed: check it and Apply again.",
    );
    // One banner: the dialog's own error line stays quiet.
    assert_eq!(h.state().dialog.error(), None);
    h.get_by_label("Add Components (3)");
    // Nothing was applied, so no row is marked Applied.
    assert!(h.query_by_label_contains("Applied").is_none());

    // Apply again works on the refreshed list, and the banner goes.
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    h.get_by_label("Add Components (0)");
    assert!(h.query_by_label_contains("nothing was applied").is_none());
}
