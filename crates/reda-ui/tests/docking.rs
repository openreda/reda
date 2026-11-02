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
