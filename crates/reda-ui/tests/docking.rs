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
fn the_window_menu_collapses_each_dock() {
    let mut h = harness();
    h.run();
    // The Docks have no header: collapsing is in the Window menu and the tab menu.
    assert!(h.query_by_label("Collapse Left Dock").is_none());
    for (side, label) in [
        (DockSide::Left, "Collapse Left Dock"),
        (DockSide::Bottom, "Collapse Bottom Dock"),
    ] {
        h.get_by_role_and_label(Role::Button, "Window").click();
        h.run();
        h.get_by_label(label).click();
        h.run();
        assert!(h.state().shell().layout().dock(side).collapsed, "{label}");
    }
    assert!(!h.state().shell().layout().dock(DockSide::Right).collapsed);
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
    // The sheet is active: the menus are the schematic ones, with no Route menu.
    assert!(h.query_by_role_and_label(Role::Button, "Route").is_none());
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
    h.run();
    h.get_by_role_and_label(Role::Button, "Route");
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

/// Shows immediate viewports as if they were OS windows of their own, as eframe does: each
/// runs its own pass with the events queued in `keys`. Every AccessKit label a viewport
/// showed goes into `labels`.
fn native_viewports(
    ctx: &egui::Context,
    keys: std::rc::Rc<std::cell::RefCell<Vec<egui::Event>>>,
    labels: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) {
    ctx.set_embed_viewports(false);
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let id = viewport.ids.this;
        let input = egui::RawInput {
            viewport_id: id,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(480.0, 320.0),
            )),
            events: keys.borrow_mut().drain(..).collect(),
            viewports: [(
                id,
                egui::ViewportInfo {
                    focused: Some(true),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
        if let Some(update) = output.platform_output.accesskit_update {
            for (_, node) in update.nodes {
                labels.borrow_mut().extend(node.label().map(str::to_owned));
            }
        }
    });
}

#[test]
fn a_floating_window_has_no_menus_or_toolbar_and_its_keys_go_through_the_keymap() {
    let work = reda_testkit::TempDir::new("floating-keys");
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build();
    engine
        .run("project.new", json!({"path": work.path(), "name": "Float"}))
        .unwrap();
    let file = work.path().join("Float").join("Float.reda");
    let mut shell = Shell::new(engine);
    shell.open_project(&file).unwrap();
    let sheet = shell.active_document().unwrap().document;
    shell
        .run(
            "schematic.wire.add",
            json!({"document": sheet, "points": [{"x": 0, "y": 0}, {"x": 2_540_000, "y": 0}]}),
        )
        .unwrap();
    shell
        .run(
            "layout.tab.move_to_new_window",
            json!({"tab": sheet.to_string()}),
        )
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1000.0, 640.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    let keys = std::rc::Rc::default();
    let labels = std::rc::Rc::default();
    native_viewports(
        &h.ctx,
        std::rc::Rc::clone(&keys),
        std::rc::Rc::clone(&labels),
    );
    h.run();
    // The floating window shows its tab and nothing of the top row; the main window has
    // the menus and the quick strip.
    let shown: Vec<String> = labels.borrow().clone();
    let title = h
        .state()
        .shell()
        .document_title(&TabKey::Document(sheet))
        .unwrap()
        .to_owned();
    assert!(shown.contains(&title), "{shown:?}");
    for top_row in ["File", "Edit", "Help", "Save All", "Undo", "Preferences…"] {
        assert!(
            !shown.iter().any(|l| l == top_row),
            "{top_row} in {shown:?}"
        );
        h.get_by_role_and_label(Role::Button, top_row);
    }
    // Mod+Z pressed in the floating window undoes the sheet's edit, through the Keymap.
    keys.borrow_mut().push(egui::Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run();
    assert!(keys.borrow().is_empty(), "the window saw the key");
    assert!(
        journal(&h).contains(&(
            "edit.undo".to_owned(),
            json!({"document": sheet.to_string()})
        )),
        "{:?}",
        journal(&h)
    );
}

/// The filled rects painted this frame, as `(rect, fill)`.
fn filled_rects(h: &Harness<'_, App>) -> Vec<(egui::Rect, egui::Color32)> {
    h.output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r) => Some((r.rect, r.fill)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_active_tab_has_white_at_15_percent_and_a_2px_white_underline() {
    let mut h = harness();
    h.run();
    let tab = h.get_by_role_and_label(Role::Tab, "Start").rect();
    let rects = filled_rects(&h);
    let near = |a: egui::Rect, b: egui::Rect| {
        (a.min - b.min).length() < 0.6 && (a.max - b.max).length() < 0.6
    };
    // White at 15 %: 38 of 255, premultiplied.
    assert_eq!(
        reda_ui::theme::ACTIVE_TAB_FILL,
        egui::Color32::from_white_alpha(38)
    );
    assert!(
        rects
            .iter()
            .any(|(r, fill)| near(*r, tab) && *fill == egui::Color32::from_white_alpha(38)),
        "a 15 % white fill behind the active tab"
    );
    let underline = egui::Rect::from_min_max(egui::pos2(tab.left(), tab.bottom() - 2.0), tab.max);
    assert!(
        rects
            .iter()
            .any(|(r, fill)| near(*r, underline) && *fill == egui::Color32::WHITE),
        "a 2 px white underline across the active tab"
    );
}

#[test]
fn a_middle_click_closes_any_tab() {
    let mut h = harness();
    h.state_mut()
        .shell_mut()
        .run("panel.show:journal", json!({}))
        .unwrap();
    h.run();
    // Messages is not the active tab of its group: Journal is.
    h.get_by_role_and_label(Role::Tab, "Messages")
        .click_button(egui::PointerButton::Middle);
    h.run();
    assert_eq!(
        journal(&h).last().unwrap(),
        &("layout.tab.close".to_owned(), json!({"tab": "messages"}))
    );
    assert!(place(&h, PanelId::Messages).is_none());
}
