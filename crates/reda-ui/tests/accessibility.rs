//! Accessibility, checked on the AccessKit tree with `egui_kittest`: names and shortcut
//! descriptions of icon-only buttons, tabs and Dock icons; focus that moves in layout
//! order and shows where it is; F6 between areas; the Ctrl+Tab switcher; and the canvas as
//! one element that describes its Document and the selection.

use egui::accesskit::Role;
use egui::os::OperatingSystem;
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::model::DocumentKind;
use reda_ui::keymap::Chord;
use reda_ui::layout::{PanelId, TabKey};
use reda_ui::shell::Shell;
use serde_json::json;

fn engine() -> Engine {
    Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness_on(shell: Shell) -> Harness<'static, App> {
    harness_as(shell, OperatingSystem::Nix)
}

/// A harness whose egui context reports `os`, which picks the key names shortcuts show.
fn harness_as(shell: Shell, os: OperatingSystem) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1000.0, 640.0])
        .with_os(os)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn harness() -> Harness<'static, App> {
    harness_on(Shell::new(engine()))
}

/// The portable chord `key` (`"Mod+N"`) as `h`'s platform writes it.
fn shown(h: &Harness<'_, App>, key: &str) -> String {
    Chord::parse(key).unwrap().label(h.ctx.os().is_mac())
}

fn description(h: &Harness<'_, App>, role: Role, label: &str) -> Option<String> {
    h.get_by_role_and_label(role, label)
        .accesskit_node()
        .description()
}

/// The label of the node with keyboard focus.
fn focused(h: &Harness<'_, App>) -> Option<String> {
    h.query_by(|n| n.is_focused())
        .and_then(|n| n.accesskit_node().label())
}

fn focus(h: &Harness<'_, App>) -> Option<TabKey> {
    h.state().shell().layout().focus.clone()
}

fn press(h: &mut Harness<'_, App>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run();
}

/// A key pressed while `modifiers` stay held.
fn press_held(h: &mut Harness<'_, App>, modifiers: Modifiers, key: Key) {
    for pressed in [true, false] {
        h.event(Event::Key {
            key,
            pressed,
            modifiers,
            repeat: false,
            physical_key: None,
        });
    }
    h.run();
}

#[test]
fn icon_buttons_tabs_and_dock_icons_are_named_by_title_and_described_by_shortcut() {
    // The keys read as the platform writes them: checked as egui's default and as macOS,
    // whose names differ (Cmd, Option).
    for os in [OperatingSystem::Nix, OperatingSystem::Mac] {
        named_by_title_and_described_by_shortcut(os);
    }
}

fn named_by_title_and_described_by_shortcut(os: OperatingSystem) {
    let mut h = harness_as(Shell::new(engine()), os);
    // A toolbar icon: its Command's title, its key. (The toolbar's comes before the Start
    // page's button of the same name.)
    let toolbar = h
        .get_all_by_role_and_label(Role::Button, "New Project…")
        .next()
        .unwrap()
        .accesskit_node()
        .description();
    assert_eq!(toolbar, Some(shown(&h, "Mod+N")));
    assert_eq!(
        description(&h, Role::Button, "Save All"),
        Some(shown(&h, "Mod+S"))
    );
    // Tabs: the Panel's title, and the key of its show Preset; a tab without one has no
    // description.
    assert_eq!(
        description(&h, Role::Tab, "Projects"),
        Some(shown(&h, "Alt+1"))
    );
    assert_eq!(
        description(&h, Role::Tab, "Messages"),
        Some(shown(&h, "Alt+3"))
    );
    assert_eq!(description(&h, Role::Tab, "Start"), None);
    assert!(
        h.get_by_role_and_label(Role::Tab, "Start")
            .accesskit_node()
            .is_selected()
            .unwrap_or(false),
        "the shown tab reads as selected"
    );

    // A collapsed Dock's icon.
    h.get_by_role_and_label(Role::Tab, "Projects")
        .click_secondary();
    h.run();
    h.get_by_label("Collapse Dock").click();
    h.run();
    assert_eq!(
        description(&h, Role::Button, "Projects"),
        Some(shown(&h, "Alt+1"))
    );
    h.get_by_role_and_label(Role::Button, "Expand Dock");
}

#[test]
fn tab_moves_focus_through_the_window_in_layout_order() {
    let mut h = harness();
    let mut order = Vec::new();
    for _ in 0..40 {
        press(&mut h, Modifiers::NONE, Key::Tab);
        if let Some(label) = focused(&h)
            && !order.contains(&label)
        {
            order.push(label);
        }
    }
    let at = |label: &str| {
        order
            .iter()
            .position(|l| l == label)
            .unwrap_or_else(|| panic!("{label} never took focus: {order:?}"))
    };
    // Toolbar, left Dock, centre, right Dock, bottom Dock, status bar.
    let sequence = [
        at("New Project…"),
        at("Projects"),
        at("Start"),
        at("Properties"),
        at("Messages"),
        at("Panels"),
    ];
    assert!(sequence.is_sorted(), "focus left layout order: {order:?}");
}

#[test]
fn f6_takes_keyboard_focus_to_the_next_group_and_shift_f6_back() {
    let mut h = harness();
    press(&mut h, Modifiers::NONE, Key::F6);
    assert_eq!(focus(&h), Some(TabKey::Panel(PanelId::Properties)));
    h.run();
    assert_eq!(focused(&h).as_deref(), Some("Properties"));
    press(&mut h, Modifiers::NONE, Key::F6);
    h.run();
    assert_eq!(focused(&h).as_deref(), Some("Messages"));
    press(&mut h, Modifiers::SHIFT, Key::F6);
    press(&mut h, Modifiers::SHIFT, Key::F6);
    h.run();
    assert_eq!(focus(&h), Some(TabKey::Start));
    assert_eq!(focused(&h).as_deref(), Some("Start"));
}

#[test]
fn ctrl_tab_shows_the_switcher_while_ctrl_is_held_and_switches_on_release() {
    let mut h = harness();
    h.get_by_role_and_label(Role::Tab, "Messages").click();
    h.run();
    h.get_by_role_and_label(Role::Tab, "Start").click();
    h.run();

    h.event(Event::ModifiersChanged(Modifiers::CTRL));
    press_held(&mut h, Modifiers::CTRL, Key::Tab);
    // The popup lists the tabs most recently used first, the pick selected.
    let list = h.get_by_role_and_label(Role::List, "Switch Tab");
    let items: Vec<String> = list
        .query_all_by_role(Role::ListItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert_eq!(items[..2], ["Start", "Messages"]);
    let picked = |h: &Harness<'_, App>| {
        h.get_by_role_and_label(Role::List, "Switch Tab")
            .query_all_by_role(Role::ListItem)
            .find(|n| n.accesskit_node().is_selected() == Some(true))
            .and_then(|n| n.accesskit_node().label())
    };
    assert_eq!(picked(&h).as_deref(), Some("Messages"));
    press_held(&mut h, Modifiers::CTRL, Key::Tab);
    assert_eq!(picked(&h).as_deref(), Some("Projects"));
    press_held(&mut h, Modifiers::CTRL | Modifiers::SHIFT, Key::Tab);
    assert_eq!(picked(&h).as_deref(), Some("Messages"));
    // Nothing switched yet.
    assert_eq!(focus(&h), Some(TabKey::Start));

    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
    h.run();
    assert_eq!(focus(&h), Some(TabKey::Panel(PanelId::Messages)));
    assert!(
        h.query_by_role_and_label(Role::List, "Switch Tab")
            .is_none()
    );
    assert_eq!(focused(&h).as_deref(), Some("Messages"));

    // A quick Ctrl+Tab goes straight back.
    press(&mut h, Modifiers::CTRL, Key::Tab);
    h.run();
    assert_eq!(focus(&h), Some(TabKey::Start));
}

#[test]
fn escape_closes_the_switcher_without_switching() {
    let mut h = harness();
    h.event(Event::ModifiersChanged(Modifiers::CTRL));
    press_held(&mut h, Modifiers::CTRL, Key::Tab);
    h.get_by_role_and_label(Role::List, "Switch Tab");
    press_held(&mut h, Modifiers::CTRL, Key::Escape);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
    assert!(
        h.query_by_role_and_label(Role::List, "Switch Tab")
            .is_none()
    );
    assert_eq!(focus(&h), Some(TabKey::Start));
}

#[test]
fn the_canvas_is_one_element_that_describes_its_document_and_selection() {
    let work = TempDir::new("a11y-work");
    let mut shell = Shell::new(engine());
    shell.new_project(work.path(), "Reader").unwrap();
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .find(|d| d.kind == DocumentKind::Schematic)
        .unwrap()
        .uuid;
    let mut texts = Vec::new();
    for text in ["one", "two"] {
        let out = shell
            .run(
                "schematic.text.add",
                json!({"document": sheet, "text": text, "at": {"x": 0, "y": 0}}),
            )
            .unwrap();
        texts.push(out["result"]["text"].clone());
    }
    let mut h = harness_on(shell);
    let canvas = |h: &Harness<'_, App>| {
        h.get_by_role_and_label(Role::Canvas, "Schematic canvas: Sheet 1")
            .accesskit_node()
            .description()
    };
    assert_eq!(canvas(&h).as_deref(), Some("Nothing selected"));

    h.state_mut()
        .shell_mut()
        .run("selection.set", json!({"objects": texts}))
        .unwrap();
    h.run();
    assert_eq!(canvas(&h).as_deref(), Some("2 objects selected"));
    h.state_mut()
        .shell_mut()
        .run("selection.set", json!({"objects": [texts[0]]}))
        .unwrap();
    h.run();
    assert_eq!(canvas(&h).as_deref(), Some("1 object selected"));
    // One element: nothing inside the canvas is in the tree.
    let node = h.get_by_role(Role::Canvas);
    assert_eq!(node.accesskit_node().children().count(), 0);
}

/// The buttons in the tree with no name, by where they are.
fn unnamed_buttons(h: &Harness<'_, App>) -> Vec<String> {
    h.query_all_by_role(Role::Button)
        .filter(|b| {
            let node = b.accesskit_node();
            node.label().unwrap_or_default().trim().is_empty()
                && node.value().unwrap_or_default().trim().is_empty()
        })
        .map(|b| format!("{:?}", b.accesskit_node().raw_bounds()))
        .collect()
}

#[test]
fn every_button_has_a_name_on_a_sheet_and_on_the_board() {
    let work = TempDir::new("a11y-names");
    let mut shell = Shell::new(engine());
    shell.new_project(work.path(), "Names").unwrap();
    let project = shell.engine().project().unwrap();
    let pcb = project
        .documents()
        .find(|d| d.kind == DocumentKind::Pcb)
        .map(|d| d.uuid);
    let mut h = harness_on(shell);
    h.run();
    assert!(h.query_all_by_role(Role::Button).count() > 10);
    assert_eq!(unnamed_buttons(&h), Vec::<String>::new(), "on the sheet");
    if let Some(pcb) = pcb {
        h.state_mut()
            .shell_mut()
            .run("document.open", json!({"document": pcb}))
            .unwrap();
        h.run();
        assert_eq!(unnamed_buttons(&h), Vec::<String>::new(), "on the board");
    }
    for panel in PanelId::ALL {
        let id = format!("panel.show:{}", panel.id());
        h.state_mut().shell_mut().run(&id, json!({})).unwrap();
        h.run();
        assert_eq!(unnamed_buttons(&h), Vec::<String>::new(), "with {id}");
    }
}
