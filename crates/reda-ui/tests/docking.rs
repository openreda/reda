//! The docking layer in the window, driven through AccessKit with `egui_kittest`: the
//! Default Layout's tabs, the tab menu from the keyboard and the mouse, collapsed Docks and
//! the Panels button. Every move must reach the engine's Journal as a layout Command.

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::layout::{Area, DockSide, PanelId, TabKey};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn harness() -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build();
    Harness::builder()
        .with_size([1000.0, 640.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(Shell::new(engine)))
}

/// The Journal's successful entries, as `(id, params)`.
fn journal(h: &Harness<'_, App>) -> Vec<(String, Value)> {
    h.state()
        .shell()
        .engine()
        .journal()
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| (e.id.clone(), e.params.clone()))
        .collect()
}

fn place(h: &Harness<'_, App>, panel: PanelId) -> Option<Area> {
    h.state()
        .shell()
        .layout()
        .place(&TabKey::Panel(panel))
        .map(|p| p.area)
}

#[test]
fn the_default_layout_shows_the_start_page_and_three_panels() {
    let mut h = harness();
    h.run();
    for label in ["Start", "Projects", "Properties", "Messages"] {
        h.get_by_label(label);
    }
    h.get_by_label("Welcome to REDA");
    h.get_by_label("Panels");
}

#[test]
fn the_keyboard_tab_menu_moves_a_panel_to_another_dock() {
    let mut h = harness();
    h.run();
    h.get_by_label("Properties").focus();
    h.run();
    h.key_press_modifiers(Modifiers::SHIFT, Key::F10);
    h.run();
    h.get_by_label("Move to").hover();
    h.run();
    h.get_by_label("Left Dock").click();
    h.run();

    assert_eq!(
        place(&h, PanelId::Properties),
        Some(Area::Dock(DockSide::Left))
    );
    assert_eq!(
        journal(&h),
        [(
            "layout.tab.move_to_dock".to_owned(),
            json!({"tab": "properties", "dock": "left"})
        )]
    );
}

#[test]
fn the_menu_key_opens_the_tab_menu_and_closes_a_tab() {
    let mut h = harness();
    h.run();
    h.get_by_label("Messages").focus();
    h.run();
    h.key_press(Key::Menu);
    h.run();
    h.get_by_label("Close").click();
    h.run();
    assert_eq!(place(&h, PanelId::Messages), None);
    assert_eq!(
        journal(&h),
        [("layout.tab.close".to_owned(), json!({"tab": "messages"}))]
    );

    // The Panels button reopens it through its Preset.
    h.get_by_label("Panels").click();
    h.run();
    h.get_by_label("Messages (closed)").click();
    h.run();
    assert_eq!(
        place(&h, PanelId::Messages),
        Some(Area::Dock(DockSide::Bottom))
    );
    assert_eq!(journal(&h)[1].0, "panel.show:messages");
}

#[test]
fn a_right_click_tab_menu_splits_and_moves_to_a_new_window() {
    let mut h = harness();
    h.run();
    h.get_by_label("Start").click_secondary();
    h.run();
    // The Start page is alone in its group, so it cannot split: the item is greyed out.
    h.get_by_label("Move to").hover();
    h.run();
    assert!(h.get_by_label("Split Right").accesskit_node().is_disabled());
    h.get_by_label("New Window").click();
    h.run();
    assert_eq!(
        journal(&h),
        [(
            "layout.tab.move_to_new_window".to_owned(),
            json!({"tab": "start"})
        )]
    );
    let layout = h.state().shell().layout();
    assert_eq!(layout.windows.len(), 1);
    assert!(matches!(
        layout.place(&TabKey::Start).map(|p| p.area),
        Some(Area::Window(_))
    ));
}

#[test]
fn a_docks_header_button_collapses_it() {
    let mut h = harness();
    h.run();
    h.get_by_role_and_label(Role::Button, "Collapse Left Dock")
        .click();
    h.run();
    assert!(h.state().shell().layout().dock(DockSide::Left).collapsed);
    assert_eq!(
        journal(&h),
        [("layout.dock.collapse".to_owned(), json!({"dock": "left"}))]
    );
}

#[test]
fn the_active_tabs_close_button_closes_it() {
    let mut h = harness();
    h.run();
    // Each group's active tab shows its close button.
    assert!(h.query_by_label("Close Projects").is_some());
    assert!(h.query_by_label("Close Start").is_some());
    h.get_by_role_and_label(Role::Button, "Close Projects")
        .click();
    h.run();
    assert_eq!(
        journal(&h),
        [("layout.tab.close".to_owned(), json!({"tab": "projects"}))]
    );
    assert!(
        h.state()
            .shell()
            .layout()
            .place(&TabKey::Panel(PanelId::Projects))
            .is_none()
    );
}

#[test]
fn a_collapsed_dock_shows_icons_that_open_flyouts() {
    let mut h = harness();
    h.run();
    h.get_by_label("Projects").click_secondary();
    h.run();
    h.get_by_label("Collapse Dock").click();
    h.run();
    assert!(h.state().shell().layout().dock(DockSide::Left).collapsed);

    // The strip's icon is named after its Panel and opens it as a flyout.
    h.get_by_role_and_label(Role::Button, "Projects").click();
    h.run();
    assert_eq!(
        h.state().shell().layout().flyout,
        Some(TabKey::Panel(PanelId::Projects))
    );
    h.get_by_label("Expand Dock").click();
    h.run();
    assert!(!h.state().shell().layout().dock(DockSide::Left).collapsed);

    let ids: Vec<String> = journal(&h).into_iter().map(|(id, _)| id).collect();
    assert_eq!(
        ids,
        [
            "layout.dock.collapse",
            "layout.tab.activate",
            "layout.dock.collapse"
        ]
    );
}

/// Presses on the tab `label` and drags it to `to`, holding the button down.
fn drag_tab(h: &mut Harness<'_, App>, label: &str, to: egui::Pos2) {
    let from = h.get_by_label(label).rect().center();
    h.drag_at(from);
    h.run();
    h.hover_at(to);
    h.run();
}

// Tests run with no OS windows and no screen positions, as on Wayland: a tab released
// outside the window is a blind drop.

#[test]
fn a_tab_dragged_out_of_the_window_floats_in_a_new_window_after_a_wait() {
    let mut h = harness();
    h.run();
    let outside = egui::pos2(-60.0, 300.0);
    drag_tab(&mut h, "Properties", outside);
    h.drop_at(outside);
    // Each step is 0.25 s. A quarter of a second after the release nothing has happened
    // yet; after 0.7 s the tab floats in a window of its own.
    h.step();
    assert_eq!(
        place(&h, PanelId::Properties),
        Some(Area::Dock(DockSide::Right))
    );
    h.run_steps(3);
    assert_eq!(
        journal(&h),
        [(
            "layout.tab.move_to_new_window".to_owned(),
            json!({"tab": "properties"})
        )]
    );
    assert!(matches!(
        place(&h, PanelId::Properties),
        Some(Area::Window(_))
    ));
}

#[test]
fn the_first_window_to_see_the_pointer_after_a_blind_drop_takes_the_tab() {
    let mut h = harness();
    h.run();
    let start = h.get_by_label("Start").rect();
    let outside = egui::pos2(-60.0, 300.0);
    drag_tab(&mut h, "Properties", outside);
    h.drop_at(outside);
    h.step();
    // The pointer comes back over the centre's tab strip, after the Start tab.
    h.hover_at(egui::pos2(start.right() + 30.0, start.center().y));
    h.run_steps(2);
    let layout = h.state().shell().layout();
    let centre = layout.place(&TabKey::Start).unwrap().group;
    assert_eq!(
        journal(&h),
        [(
            "layout.tab.move_to_group".to_owned(),
            json!({"tab": "properties", "group": centre, "index": 1})
        )]
    );
    // No new window follows later.
    h.run_steps(8);
    assert_eq!(journal(&h).len(), 1);
}

#[test]
fn menus_follow_the_document_last_pressed_on_in_any_window() {
    let work = reda_testkit::TempDir::new("docking-work");
    let mut shell = Shell::new(
        Engine::builder()
            .user_dirs(None)
            .starter_library(None)
            .headless(false)
            .build(),
    );
    shell.new_project(work.path(), "Menus").unwrap();
    let project = shell.engine().project().unwrap();
    let kind = |k| project.documents().find(|d| d.kind == k).unwrap().uuid;
    let (sheet, pcb) = (
        kind(reda_ui::engine::model::DocumentKind::Schematic),
        kind(reda_ui::engine::model::DocumentKind::Pcb),
    );
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run(
            "layout.tab.move_to_new_window",
            json!({"tab": pcb.to_string()}),
        )
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": sheet.to_string()}))
        .unwrap();
    let pcb_title = shell
        .document_title(&TabKey::Document(pcb))
        .unwrap()
        .to_owned();
    let mut h = Harness::builder()
        .with_size([1000.0, 640.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    // The sheet is active: the menus and toolbar are the schematic ones.
    assert!(h.query_all_by_label("Run ERC").next().is_some());
    assert_eq!(
        reda_ui::bars::active_type(h.state().shell()),
        Some(reda_ui::bars::DocumentType::Schematic)
    );

    // A press on the PCB's canvas, in its own window, makes it the active Document, and
    // the main window's menus and toolbar follow.
    let tab = h.get_by_role_and_label(Role::Tab, &pcb_title).rect();
    let canvas = egui::pos2(tab.center().x, tab.bottom() + 80.0);
    h.drag_at(canvas);
    h.drop_at(canvas);
    h.run();
    assert_eq!(h.state().shell().layout().active_document(), Some(pcb));
    assert_eq!(
        reda_ui::bars::active_type(h.state().shell()),
        Some(reda_ui::bars::DocumentType::Pcb)
    );
}

/// Moves the held drag onto the drop-target button `label`, then releases it there.
fn drop_on_button(h: &mut Harness<'_, App>, label: &str) {
    let at = h.get_by_label(label).rect().center();
    h.hover_at(at);
    h.run();
    h.drop_at(at);
    h.run();
}

#[test]
fn dropping_a_tab_on_a_split_button_runs_one_split_command() {
    let mut h = harness();
    h.run();
    let start = h.get_by_label("Welcome to REDA").rect().center();
    drag_tab(&mut h, "Properties", start);
    // Over the centre group, its five drop targets and the three Dock edges show.
    for label in [
        "Drop into Group",
        "Drop to the Left",
        "Drop to the Right",
        "Drop Above",
        "Drop Below",
        "Drop in Left Dock",
        "Drop in Right Dock",
        "Drop in Bottom Dock",
    ] {
        h.get_by_label(label);
    }
    drop_on_button(&mut h, "Drop to the Left");

    let centre = h
        .state()
        .shell()
        .layout()
        .place(&TabKey::Start)
        .unwrap()
        .group;
    assert_eq!(
        journal(&h),
        [(
            "layout.tab.split".to_owned(),
            json!({"tab": "properties", "group": centre, "side": "left"})
        )]
    );
    assert_eq!(place(&h, PanelId::Properties), Some(Area::Centre));
    // The drag is over, and its targets are gone.
    assert!(h.query_by_label("Drop to the Left").is_none());
}

#[test]
fn dropping_a_tab_on_a_tab_strip_or_a_dock_edge_moves_it_there() {
    let mut h = harness();
    h.run();
    // Onto the left half of the Start tab: first in the centre group.
    let start_tab = h.get_by_label("Start").rect();
    let before_start = egui::pos2(start_tab.left() + 2.0, start_tab.center().y);
    drag_tab(&mut h, "Messages", before_start);
    h.drop_at(before_start);
    h.run();
    let centre = h
        .state()
        .shell()
        .layout()
        .place(&TabKey::Start)
        .unwrap()
        .group;
    assert_eq!(
        journal(&h),
        [(
            "layout.tab.move_to_group".to_owned(),
            json!({"tab": "messages", "group": centre, "index": 0})
        )]
    );

    // Over the centre group, which now shows Messages.
    let start = start_tab.center() + egui::vec2(0.0, 200.0);
    drag_tab(&mut h, "Projects", start);
    drop_on_button(&mut h, "Drop in Right Dock");
    assert_eq!(
        journal(&h)[1],
        (
            "layout.tab.move_to_dock".to_owned(),
            json!({"tab": "projects", "dock": "right"})
        )
    );
    assert_eq!(
        place(&h, PanelId::Projects),
        Some(Area::Dock(DockSide::Right))
    );
}

#[test]
fn a_drop_the_layout_would_refuse_shows_why_before_the_release() {
    let mut h = harness();
    h.run();
    let before = h.state().shell().layout().clone();
    // Messages is alone in the bottom Dock, so it cannot split from its own group.
    let messages = h.get_by_label("Messages").rect();
    let below = egui::pos2(messages.center().x + 200.0, messages.bottom() + 15.0);
    drag_tab(&mut h, "Messages", below);
    let above = h.get_by_label("Drop Above").rect().center();
    h.hover_at(above);
    h.run();
    h.get_by_label_contains("Can't drop here");
    h.get_by_label_contains("alone in its group");

    h.drop_at(above);
    h.run();
    assert!(journal(&h).is_empty(), "a refused drop runs nothing");
    assert_eq!(*h.state().shell().layout(), before);
    assert!(h.query_by_label_contains("Can't drop here").is_none());
}

#[test]
fn releasing_a_tab_where_it_already_is_runs_nothing() {
    let mut h = harness();
    h.run();
    let start = h.get_by_label("Welcome to REDA").rect().center();
    drag_tab(&mut h, "Start", start);
    drop_on_button(&mut h, "Drop into Group");
    assert!(journal(&h).is_empty());
}
