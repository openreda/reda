//! The Messages panel's Group By and details pane, its right-click menu, and the
//! Violation Details window, driven through AccessKit with `egui_kittest`.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{Object, Uuid};
use reda_ui::layout::SettingsWindow;
use reda_ui::messages::CheckKind;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

const FIRST_END: &str = "Wire end at (25.4mm, 25.4mm) touches nothing";
const SECOND_END: &str = "Wire end at (50.8mm, 25.4mm) touches nothing";

/// A window on a Project whose second sheet holds a lone Wire, so ERC finds its two
/// dangling ends there; with ERC run.
fn harness(home: &TempDir, work: &TempDir) -> (Harness<'static, App>, Uuid) {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    shell
        .run(
            "app.preferences.set",
            json!({"key": "user.name", "value": "Ada"}),
        )
        .unwrap();
    let out = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let sheet2 = out["result"]["uuid"].as_str().unwrap().to_owned();
    let out = shell
        .run(
            "schematic.wire.add",
            json!({"document": sheet2, "points": [at(1000, 1000), at(2000, 1000)]}),
        )
        .unwrap();
    let wire = out["result"]["wires"][0].as_str().unwrap().parse().unwrap();
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();
    (h, wire)
}

fn journal(h: &Harness<'_, App>) -> Vec<String> {
    h.state()
        .shell()
        .engine()
        .journal()
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect()
}

fn current(h: &Harness<'_, App>) -> Option<String> {
    let shell = h.state().shell();
    let key = shell.messages().current()?;
    Some(shell.messages().entry(key)?.text.clone())
}

/// Whether some node shows `text`, as its label or (a plain label's) value.
fn shows(h: &Harness<'_, App>, text: &str) -> bool {
    h.query_all_by(|n: &_| {
        let n: &egui_kittest::kittest::AccessKitNode<'_> = n;
        n.label().as_deref() == Some(text) || n.value().as_deref() == Some(text)
    })
    .next()
    .is_some()
}

fn double_click(h: &mut Harness<'_, App>, label: &str) {
    // The harness runs each click in its own frames, further apart than a person would.
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 10.0);
    let line = h.get_by_role_and_label(Role::Button, label);
    line.click();
    line.click();
    h.run();
}

#[test]
fn group_by_regroups_the_list_and_the_details_pane_shows_the_selected_line() {
    let (home, work) = (TempDir::new("details-home"), TempDir::new("details-work"));
    let (mut h, wire) = harness(&home, &work);
    h.get_by_label("Sheet 2 (2)");
    h.get_by_label("Select a message to see its details.");

    h.get_by_role_and_label(Role::ComboBox, "Group by").click();
    h.run();
    h.get_by_label("Check").click();
    h.run();
    h.get_by_label("Dangling Wire (2)");
    assert!(h.query_by_label("Sheet 2 (2)").is_none());

    h.get_by_label(FIRST_END).click();
    h.run();
    // Under the list: the message again, and its objects as links.
    assert_eq!(h.get_all_by_label(FIRST_END).count(), 2);
    h.get_by_label("Objects:");
    // Clear the selection going to the line made, so the link's own selecting shows.
    h.state_mut()
        .shell_mut()
        .run("selection.set", json!({}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Link, "Wire").click();
    h.run();
    assert_eq!(h.state().shell().selection().objects, [wire]);
}

#[test]
fn the_right_click_menu_offers_details_run_this_check_only_and_show_rule() {
    let (home, work) = (TempDir::new("details-home"), TempDir::new("details-work"));
    let (mut h, _) = harness(&home, &work);
    h.get_by_label(FIRST_END).click_secondary();
    h.run();
    h.get_by_label("Details…");
    h.get_by_label("Waive");
    h.get_by_label("Set severity of “Dangling Wire”");
    h.get_by_label("Run this check only").click();
    h.run();
    let shell = h.state().shell();
    let last = shell.engine().journal().last().unwrap();
    assert_eq!(last.id, "erc.run");
    assert_eq!(last.params["checks"], json!(["dangling_wire"]));

    h.get_by_label(FIRST_END).click_secondary();
    h.run();
    h.get_by_label("Show rule").click();
    h.run();
    h.run();
    let dialog = h
        .state()
        .settings_windows()
        .dialog(SettingsWindow::ProjectSettings)
        .expect("Project Settings opens");
    assert_eq!(dialog.page_titles()[dialog.selected()], "ERC");
}

#[test]
fn show_rule_goes_to_the_board_setup_page_of_a_drc_check() {
    let (home, work) = (TempDir::new("details-home"), TempDir::new("details-work"));
    let (mut h, _) = harness(&home, &work);
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "0.01mm",
                   "points": [{"x": "10mm", "y": "10mm"}, {"x": "15mm", "y": "10mm"}]}),
        )
        .unwrap();
    shell.run_check(CheckKind::Drc).unwrap();
    assert!(
        (shell.messages().run(CheckKind::Drc).unwrap().entries.iter())
            .any(|e| e.violation.check == "track_width")
    );
    let (window, page) = reda_ui::messages::rule_page(CheckKind::Drc, "track_width");
    assert_eq!(window, SettingsWindow::BoardSetup);
    assert_eq!(page, "Track Width");
    h.state_mut().open_settings_page(window, &page);
    h.run();
    let dialog = h
        .state()
        .settings_windows()
        .dialog(SettingsWindow::BoardSetup)
        .expect("Board Setup opens");
    assert_eq!(dialog.page_titles()[dialog.selected()], "Track Width");
}

#[test]
fn violation_details_is_a_window_that_leaves_the_rest_usable() {
    let (home, work) = (TempDir::new("details-home"), TempDir::new("details-work"));
    let (mut h, _) = harness(&home, &work);
    double_click(&mut h, FIRST_END);
    assert!(shows(&h, "Violation Details"), "Violation Details");
    assert!(shows(&h, "Dangling Wire"), "Dangling Wire");
    assert!(shows(&h, "Not waived"), "Not waived");
    // Its objects, as links, here and in the details pane under the list.
    assert_eq!(h.get_all_by_role_and_label(Role::Link, "Wire").count(), 2);

    // Not modal: Messages still goes to other lines, and the keys still work.
    h.get_by_role_and_label(Role::Button, SECOND_END).click();
    h.run();
    assert_eq!(current(&h).as_deref(), Some(SECOND_END));
    h.key_press(egui::Key::F4);
    h.run();
    assert_eq!(current(&h).as_deref(), Some(FIRST_END));
    assert!(shows(&h, "Violation Details"), "Violation Details");

    h.get_by_label("Highlight").click();
    h.run();
    assert_eq!(journal(&h).last().unwrap(), "view.highlight");
    h.get_by_label("Jump").click();
    h.run();
    assert_eq!(journal(&h).last().unwrap(), "view.jump_to");

    // Waive there with a reason, then edit the reason and Unwaive.
    h.get_by_role_and_label(Role::TextInput, "Reason").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Reason")
        .type_text("Test point");
    h.run();
    h.get_all_by_label("Waive").last().unwrap().click();
    h.run();
    h.run();
    assert!(journal(&h).contains(&"erc.waive".to_owned()));
    assert!(shows(&h, "Waived"), "Waived");
    let waiver_by = h
        .query_all_by(|n: &_| {
            let n: &egui_kittest::kittest::AccessKitNode<'_> = n;
            n.value().is_some_and(|v| v.starts_with("By Ada on "))
        })
        .count();
    assert!(waiver_by > 0, "the Waiver's author and date show");
    let shell = h.state().shell();
    let key = shell.details_line().unwrap();
    let waiver = shell
        .messages()
        .entry(key)
        .unwrap()
        .violation
        .waiver
        .unwrap();
    let reason =
        |h: &Harness<'_, App>| match h.state().shell().engine().project().unwrap().object(waiver) {
            Some(Object::Waiver(w)) => w.object.reason.clone(),
            _ => panic!("a Waiver"),
        };
    assert_eq!(reason(&h), "Test point");

    h.get_by_role_and_label(Role::TextInput, "Reason").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Reason")
        .type_text(" by design");
    h.run();
    h.get_by_label("Save reason").click();
    h.run();
    assert!(journal(&h).contains(&"checks.waiver.set_reason".to_owned()));
    assert_eq!(reason(&h), "Test point by design");

    h.get_all_by_label("Unwaive").last().unwrap().click();
    h.run();
    assert!(journal(&h).contains(&"erc.unwaive".to_owned()));
    assert!(shows(&h, "Not waived"), "Not waived");
}
