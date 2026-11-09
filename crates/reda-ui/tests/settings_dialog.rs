//! The Project Settings dialog in the window, driven through AccessKit with
//! `egui_kittest`: opened from the Projects panel, its page tree, invalid values flagged
//! inline with OK and Apply disabled, and OK and Apply each committing one `batch`.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{ErcCheck, Severity};
use reda_ui::shell::Shell;

fn harness(home: &Path, work: &Path, name: &str) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, name).unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

/// Opens Project Settings from the Project's menu in the Projects panel. The PCB Document
/// shares the Project's name, so the root is the one that is not a tree item.
fn open_settings(h: &mut Harness<'static, App>, project: &str) {
    h.query_all_by_label(project)
        .find(|n| n.accesskit_node().role() != Role::TreeItem)
        .expect("the Project's row")
        .click_secondary();
    h.run();
    h.get_by_label("Project Settings…").click();
    h.run();
}

fn last_journal_id(h: &Harness<'static, App>) -> String {
    let journal = h.state().shell().engine().journal();
    journal.last().map(|e| e.id.clone()).unwrap_or_default()
}

fn disabled(h: &Harness<'static, App>, label: &str) -> bool {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

fn type_into(h: &mut Harness<'static, App>, field: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text(text);
    h.run();
}

#[test]
fn project_settings_opens_from_the_projects_panel_with_its_page_tree() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_settings(&mut h, "Dlg");
    h.get_by_label("Project Settings");
    for page in ["Properties", "ERC", "Schematic", "Fab Presets", "Libraries"] {
        h.get_by_role_and_label(Role::TreeItem, page);
    }
    // Nothing changed yet: OK closes, Apply has nothing to do.
    assert!(!disabled(&h, "OK"));
    assert!(disabled(&h, "Apply"));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.query_by_label("Project Settings").is_none());
}

#[test]
fn an_invalid_value_is_flagged_and_disables_ok_and_apply_until_fixed() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_settings(&mut h, "Dlg");
    type_into(&mut h, "Title", "Motor driver");
    h.get_by_role_and_label(Role::TreeItem, "Libraries").click();
    h.run();
    h.get_by_label("Add Path").click();
    h.run();
    h.get_by_label("Give a folder.");
    h.get_by_role_and_label(Role::TreeItem, "Libraries (has invalid values)");
    assert!(disabled(&h, "OK") && disabled(&h, "Apply"));

    type_into(&mut h, "Library folder 1", "parts");
    assert!(!disabled(&h, "OK") && !disabled(&h, "Apply"));
    let journal = h.state().shell().engine().journal().len();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_by_label("Project Settings").is_none(), "OK closes");
    assert_eq!(h.state().shell().engine().journal().len(), journal + 1);
    assert_eq!(last_journal_id(&h), "batch");
    let props = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .props()
        .clone();
    assert_eq!(props.properties.title, "Motor driver");
    assert_eq!(props.libraries, ["parts"]);
}

#[test]
fn apply_on_the_erc_page_commits_and_keeps_the_dialog_open() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_settings(&mut h, "Dlg");
    h.get_by_role_and_label(Role::TreeItem, "ERC").click();
    h.run();
    h.get_by_label("Pin Matrix");
    h.get_by_label("Single-Pin Net: Off").click();
    h.run();
    h.get_by_label("Apply").click();
    h.run();
    assert_eq!(last_journal_id(&h), "batch");
    let erc = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .props()
        .erc
        .clone();
    assert_eq!(
        erc.severities.get(ErcCheck::SinglePinNet),
        Some(Severity::Off)
    );
    h.get_by_label("Project Settings");
    assert!(disabled(&h, "Apply"), "nothing left to apply");
}

/// How far `label`'s text starts from the left edge of its page tree row, in points.
fn tree_label_indent(h: &Harness<'static, App>, label: &str) -> f32 {
    let row = h.get_by_role_and_label(Role::TreeItem, label).rect();
    let mut found = None;
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(text)
                    if text.galley.text() == label && row.contains(text.pos) =>
                {
                    found = Some(text.pos.x - row.left());
                }
                _ => {}
            }
        }
    }
    found.unwrap_or_else(|| panic!("no text {label:?} in its row {row:?}"))
}
#[test]
fn page_tree_labels_are_left_aligned() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_settings(&mut h, "Dlg");
    for page in ["Properties", "ERC", "Schematic", "Fab Presets", "Libraries"] {
        let indent = tree_label_indent(&h, page);
        assert!(indent < 10.0, "{page:?} starts {indent} pt into its row");
    }
}

#[test]
fn the_schematic_page_sets_options_and_resets_the_saved_reannotate_settings() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    h.state_mut()
        .shell_mut()
        .run(
            "schematic.reannotate",
            serde_json::json!({"order": "across_then_up"}),
        )
        .unwrap();
    h.run();
    // The run dirtied the Project.
    open_settings(&mut h, "Dlg *");
    h.get_by_role_and_label(Role::TreeItem, "Schematic").click();
    h.run();
    for group in [
        "Nets",
        "Cross-references",
        "Assembly",
        "Re-annotate",
        "Update from Library",
    ] {
        h.get_by_label(group);
    }
    h.get_by_label("Order: Across, then up");
    h.get_by_label("Nothing saved yet: the defaults apply.");
    h.get_by_label("Automatic Cross References").click();
    h.run();
    h.get_by_label("Not Fitted display: Mesh").click();
    h.run();
    h.get_by_label("Reset Re-annotate Settings").click();
    h.run();
    h.get_by_label("Cleared on Apply.");
    h.get_by_label("Apply").click();
    h.run();
    assert_eq!(last_journal_id(&h), "batch");
    let props = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .props()
        .clone();
    assert!(props.schematic.cross_references);
    assert_eq!(
        props.schematic.not_fitted_display,
        reda_ui::engine::model::NotFittedDisplay::Mesh
    );
    assert!(props.reannotate.is_none());
    let none = h
        .query_all_by_label("Nothing saved yet: the defaults apply.")
        .count();
    assert_eq!(none, 2, "both summaries are empty now");
}

#[test]
fn the_erc_page_edits_the_pin_matrix_and_sets_picked_checks_together() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_settings(&mut h, "Dlg");
    h.get_by_role_and_label(Role::TreeItem, "ERC").click();
    h.run();
    for group in ["Pins", "Nets", "Ports", "Components", "Drawing"] {
        h.get_by_label(group);
    }
    // A click cycles a cell: Error goes to Off.
    h.get_by_label("Output Pin connected to Output Pin: Error").click();
    h.run();
    h.get_by_label("Output Pin connected to Output Pin: Off");
    // The right-click menu sets every cell.
    h.get_by_label("Input Pin unconnected: Error").click_secondary();
    h.run();
    h.get_by_label("All Warning").click();
    h.run();
    h.get_by_label("Input Pin unconnected: Warning");
    h.get_by_label("Output Pin connected to Output Pin: Warning");

    // Pick two checks (the second with Mod held) and set both to Off.
    h.get_by_role_and_label(Role::Row, "Undriven Power").click();
    h.run();
    h.get_by_role_and_label(Role::Row, "No Driving Source")
        .click_modifiers(egui::Modifiers::COMMAND);
    h.run();
    h.get_by_label("Set picked checks to Off").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    let erc = h.state().shell().engine().project().unwrap().props().erc.clone();
    assert_eq!(erc.severities.get(ErcCheck::UndrivenPower), Some(Severity::Off));
    assert_eq!(erc.severities.get(ErcCheck::NoDrivingSource), Some(Severity::Off));
    let input = reda_ui::engine::model::Cell::unconnected(reda_ui::engine::model::PinType::Input);
    assert_eq!(erc.pin_matrix_overrides.get(input), Severity::Warning);
}
