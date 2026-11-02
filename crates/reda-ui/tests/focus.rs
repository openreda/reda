//! Keyboard focus between areas, driven through its Commands on a Shell with no window:
//! F6 / Shift+F6 (`layout.focus.cycle`) visit the tab groups and Docks in layout order, and
//! Ctrl+Tab / Ctrl+Shift+Tab (`layout.tab.switch`) walk the tabs most recently used first.

use reda_ui::engine::Engine;
use reda_ui::layout::{PanelId, TabKey};
use reda_ui::shell::Shell;
use serde_json::json;

fn shell() -> Shell {
    Shell::new(
        Engine::builder()
            .user_dirs(None)
            .starter_library(None)
            .headless(false)
            .build(),
    )
}

fn focus(shell: &Shell) -> Option<TabKey> {
    shell.layout().focus.clone()
}

fn run(shell: &mut Shell, id: &str) {
    shell.run(id, json!({})).unwrap();
}

fn activate(shell: &mut Shell, tab: &TabKey) {
    shell
        .run("layout.tab.activate", json!({"tab": tab.to_string()}))
        .unwrap();
}

const PROJECTS: TabKey = TabKey::Panel(PanelId::Projects);
const PROPERTIES: TabKey = TabKey::Panel(PanelId::Properties);
const MESSAGES: TabKey = TabKey::Panel(PanelId::Messages);

#[test]
fn f6_visits_the_groups_and_docks_in_layout_order_and_wraps() {
    let mut shell = shell();
    assert_eq!(focus(&shell), Some(TabKey::Start));
    // Left Dock, centre, right Dock, bottom Dock.
    for expected in [PROPERTIES, MESSAGES, PROJECTS, TabKey::Start] {
        run(&mut shell, "layout.focus.cycle:next");
        assert_eq!(focus(&shell), Some(expected));
    }
    for expected in [PROJECTS, MESSAGES, PROPERTIES, TabKey::Start] {
        run(&mut shell, "layout.focus.cycle:previous");
        assert_eq!(focus(&shell), Some(expected));
    }
}

#[test]
fn f6_stops_once_on_a_collapsed_dock_and_opens_its_flyout() {
    let mut shell = shell();
    shell
        .run("panel.show:library", json!({}))
        .expect("the Library opens in its Dock");
    let library = TabKey::Panel(PanelId::Library);
    let home = shell.layout().place(&library).unwrap().area;
    let reda_ui::layout::Area::Dock(side) = home else {
        panic!("the Library lives in a Dock")
    };
    shell
        .run("layout.dock.collapse", json!({"dock": side}))
        .unwrap();
    activate(&mut shell, &TabKey::Start);

    let mut seen = Vec::new();
    for _ in 0..4 {
        run(&mut shell, "layout.focus.cycle:next");
        seen.push(focus(&shell).unwrap());
    }
    let in_dock = seen
        .iter()
        .filter(|t| shell.layout().place(t).unwrap().area == home)
        .count();
    assert_eq!(in_dock, 1, "one stop for the collapsed Dock: {seen:?}");
    assert_eq!(seen.last(), Some(&TabKey::Start), "{seen:?}");
    // Landing on it opened its tab as a flyout.
    run(&mut shell, "layout.focus.cycle:previous");
    assert_eq!(
        shell.layout().place(&focus(&shell).unwrap()).unwrap().area,
        home
    );
    assert_eq!(shell.layout().flyout, focus(&shell));
}

#[test]
fn ctrl_tab_switches_to_the_tab_used_before_and_back() {
    let mut shell = shell();
    activate(&mut shell, &PROJECTS);
    activate(&mut shell, &MESSAGES);
    activate(&mut shell, &TabKey::Start);

    run(&mut shell, "layout.tab.switch:next");
    let switch = shell.tab_switch().expect("the switcher is open");
    // Most recent first; Properties was never used, so it comes last.
    assert_eq!(switch.tabs, [TabKey::Start, MESSAGES, PROJECTS, PROPERTIES]);
    assert_eq!(switch.index, 1);
    // Nothing moves until the switch ends.
    assert_eq!(focus(&shell), Some(TabKey::Start));
    shell.end_tab_switch(true);
    assert_eq!(focus(&shell), Some(MESSAGES));
    assert!(shell.tab_switch().is_none());

    // A second quick switch goes back.
    run(&mut shell, "layout.tab.switch:next");
    shell.end_tab_switch(true);
    assert_eq!(focus(&shell), Some(TabKey::Start));
}

#[test]
fn holding_ctrl_walks_further_back_and_shift_walks_forward_again() {
    let mut shell = shell();
    activate(&mut shell, &PROJECTS);
    activate(&mut shell, &MESSAGES);
    activate(&mut shell, &TabKey::Start);

    run(&mut shell, "layout.tab.switch:next");
    run(&mut shell, "layout.tab.switch:next");
    assert_eq!(shell.tab_switch().unwrap().index, 2);
    run(&mut shell, "layout.tab.switch:previous");
    assert_eq!(shell.tab_switch().unwrap().index, 1);
    run(&mut shell, "layout.tab.switch:previous");
    run(&mut shell, "layout.tab.switch:previous");
    // Wraps to the least recent.
    assert_eq!(shell.tab_switch().unwrap().index, 3);

    // Escape cancels: focus stays put.
    shell.end_tab_switch(false);
    assert_eq!(focus(&shell), Some(TabKey::Start));

    // Ctrl+Shift+Tab first goes to the least recently used tab.
    run(&mut shell, "layout.tab.switch:previous");
    shell.end_tab_switch(true);
    assert_eq!(focus(&shell), Some(PROPERTIES));
}

#[test]
fn closed_tabs_drop_out_of_the_switcher() {
    let mut shell = shell();
    activate(&mut shell, &MESSAGES);
    activate(&mut shell, &TabKey::Start);
    shell
        .run("layout.tab.close", json!({"tab": "messages"}))
        .unwrap();
    run(&mut shell, "layout.tab.switch:next");
    assert!(!shell.tab_switch().unwrap().tabs.contains(&MESSAGES));
}
