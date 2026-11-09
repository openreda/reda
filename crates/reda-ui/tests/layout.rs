//! The docking layout, driven through its layout Commands on a Shell, with no window: the
//! Default Layout, moves between groups, Docks and windows, splits, closing, collapsing
//! Docks, `panel.show` and refusals that leave the layout as it was.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::ErrorCode;
use reda_ui::engine::model::DocumentKind;
use reda_ui::layout::{Area, DockSide, Layout, Node, PanelId, TabKey};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn engine() -> Engine {
    Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build()
}

/// A Shell with a new Project open on its first sheet. The folder must outlive it.
fn shell_with_project(work: &TempDir) -> Shell {
    let mut shell = Shell::new(engine());
    shell.new_project(work.path(), "Dock").unwrap();
    shell
}

fn sheet(shell: &Shell) -> TabKey {
    TabKey::Document(
        shell
            .engine()
            .project()
            .unwrap()
            .documents()
            .next()
            .unwrap()
            .uuid,
    )
}

fn tab_param(tab: &TabKey) -> Value {
    json!(tab.to_string())
}

fn dock_tabs(layout: &Layout, side: DockSide) -> Vec<TabKey> {
    layout
        .dock(side)
        .groups
        .iter()
        .flat_map(|g| g.tabs.iter().cloned())
        .collect()
}

#[test]
fn the_default_layout_puts_projects_left_properties_right_and_messages_bottom() {
    let shell = Shell::new(engine());
    let layout = shell.layout();
    assert_eq!(
        dock_tabs(layout, DockSide::Left),
        [TabKey::Panel(PanelId::Projects)]
    );
    assert_eq!(
        dock_tabs(layout, DockSide::Right),
        [TabKey::Panel(PanelId::Properties)]
    );
    assert_eq!(
        dock_tabs(layout, DockSide::Bottom),
        [TabKey::Panel(PanelId::Messages)]
    );
    // With no Project open the centre holds the Start page.
    let Node::Group(centre) = &layout.centre else {
        panic!("the centre starts as one group")
    };
    assert_eq!(centre.tabs, [TabKey::Start]);
    assert!(layout.windows.is_empty());
}

#[test]
fn opening_a_project_puts_its_first_sheet_in_the_centre() {
    let work = TempDir::new("layout-work");
    let shell = shell_with_project(&work);
    let sheet = sheet(&shell);
    let layout = shell.layout();
    let Node::Group(centre) = &layout.centre else {
        panic!("one centre group")
    };
    assert_eq!(centre.tabs, std::slice::from_ref(&sheet));
    assert_eq!(layout.place(&sheet).unwrap().area, Area::Centre);
    assert_eq!(shell.active_document().unwrap().title, "Sheet 1");
}

#[test]
fn a_document_moves_into_a_dock_and_leaves_an_empty_centre() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    let sheet = sheet(&shell);
    shell
        .run(
            "layout.tab.move_to_dock",
            json!({"tab": tab_param(&sheet), "dock": "right"}),
        )
        .unwrap();
    let layout = shell.layout();
    assert_eq!(
        dock_tabs(layout, DockSide::Right),
        [TabKey::Panel(PanelId::Properties), sheet.clone()]
    );
    let Node::Group(centre) = &layout.centre else {
        panic!("the centre keeps one empty group")
    };
    assert!(centre.tabs.is_empty(), "an empty centre is allowed");
    // The Document stays open and active, in its Dock.
    assert_eq!(shell.active_document().unwrap().title, "Sheet 1");
}

#[test]
fn a_panel_moves_into_the_centre_group() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    let centre = shell.layout().place(&sheet(&shell)).unwrap().group;
    let messages = TabKey::Panel(PanelId::Messages);
    let out = shell
        .run(
            "layout.tab.move_to_group",
            json!({"tab": "messages", "group": centre, "index": 0}),
        )
        .unwrap();
    assert_eq!(out, json!({"group": centre}));
    let layout = shell.layout();
    let Node::Group(group) = &layout.centre else {
        panic!("one centre group")
    };
    assert_eq!(group.tabs, [messages, sheet(&shell)]);
    // The bottom Dock had only Messages, so it is empty now.
    assert!(layout.dock(DockSide::Bottom).groups.is_empty());
}

#[test]
fn splitting_right_and_down_builds_splits_in_the_centre() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    let sheet = sheet(&shell);
    let centre = shell.layout().place(&sheet).unwrap().group;
    shell
        .run(
            "layout.tab.move_to_group",
            json!({"tab": "properties", "group": centre}),
        )
        .unwrap();
    shell
        .run(
            "layout.tab.split",
            json!({"tab": "properties", "side": "right"}),
        )
        .unwrap();
    let layout = shell.layout();
    let Node::Split { axis, children } = &layout.centre else {
        panic!("a split, got {:?}", layout.centre)
    };
    assert_eq!(*axis, reda_ui::layout::Axis::Horizontal);
    let tabs: Vec<_> = children
        .iter()
        .map(|c| match c {
            Node::Group(g) => g.tabs.clone(),
            Node::Split { .. } => panic!("groups side by side"),
        })
        .collect();
    assert_eq!(
        tabs,
        [
            vec![sheet.clone()],
            vec![TabKey::Panel(PanelId::Properties)]
        ]
    );

    // A Document can split down from its own group once it shares one.
    shell
        .run(
            "layout.tab.move_to_group",
            json!({"tab": "projects", "group": centre}),
        )
        .unwrap();
    shell
        .run(
            "layout.tab.split",
            json!({"tab": "projects", "side": "down"}),
        )
        .unwrap();
    let Node::Split { children, .. } = &shell.layout().centre else {
        panic!("still a split")
    };
    assert!(matches!(
        &children[0],
        Node::Split { axis: reda_ui::layout::Axis::Vertical, children } if children.len() == 2
    ));
}

#[test]
fn a_refused_move_leaves_the_layout_unchanged() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    let before = shell.layout().clone();

    // A Dock holds one column, so nothing splits to the right inside it.
    shell
        .run("panel.show", json!({"panel": "library"}))
        .unwrap();
    let with_library = shell.layout().clone();
    assert_ne!(with_library, before);
    let err = shell
        .run(
            "layout.tab.split",
            json!({"tab": "library", "side": "right"}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
    assert_eq!(*shell.layout(), with_library);

    // A tab alone in its group has nothing to split from.
    let err = shell
        .run(
            "layout.tab.split",
            json!({"tab": tab_param(&sheet(&shell)), "side": "right"}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(*shell.layout(), with_library);

    // Unknown tabs and groups are not found.
    let err = shell
        .run("layout.tab.close", json!({"tab": "layers"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = shell
        .run(
            "layout.tab.move_to_group",
            json!({"tab": "projects", "group": 999}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = shell
        .run("layout.tab.close", json!({"tab": "nonsense"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(*shell.layout(), with_library);
}

#[test]
fn a_tab_moves_into_a_new_window() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    shell
        .run(
            "layout.tab.move_to_new_window",
            json!({"tab": "properties"}),
        )
        .unwrap();
    let layout = shell.layout();
    assert_eq!(layout.windows.len(), 1);
    let place = layout.place(&TabKey::Panel(PanelId::Properties)).unwrap();
    assert_eq!(place.area, Area::Window(layout.windows[0].id));
    assert!(layout.dock(DockSide::Right).groups.is_empty());

    // Moving the last tab out closes the window.
    shell
        .run(
            "layout.tab.move_to_dock",
            json!({"tab": "properties", "dock": "left"}),
        )
        .unwrap();
    assert!(shell.layout().windows.is_empty());
}

#[test]
fn closing_a_window_closes_its_tabs_in_one_action() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    let sheet = sheet(&shell);
    shell
        .run(
            "layout.tab.move_to_new_window",
            json!({"tab": "properties"}),
        )
        .unwrap();
    let window = shell.layout().windows[0].id;
    let Node::Group(group) = &shell.layout().windows[0].root else {
        panic!("a window of one group")
    };
    let group = group.id;
    shell
        .run(
            "layout.tab.move_to_group",
            json!({"tab": tab_param(&sheet), "group": group}),
        )
        .unwrap();

    shell
        .run("layout.window.close", json!({"window": window}))
        .unwrap();
    let layout = shell.layout().clone();
    assert!(layout.windows.is_empty());
    assert!(layout.place(&sheet).is_none());
    assert!(layout.closed_panels().contains(&PanelId::Properties));

    // A window that is gone is refused, and the layout stays as it was.
    let err = shell
        .run("layout.window.close", json!({"window": window}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(*shell.layout(), layout);
}

#[test]
fn closing_panels_and_documents_and_reopening_panels() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    shell
        .run("layout.tab.close", json!({"tab": "messages"}))
        .unwrap();
    let messages = TabKey::Panel(PanelId::Messages);
    assert!(shell.layout().place(&messages).is_none());
    assert_eq!(
        shell.layout().closed_panels(),
        [
            PanelId::Messages,
            PanelId::Library,
            PanelId::Layers,
            PanelId::Journal,
            PanelId::Navigator
        ]
    );

    // The Preset reopens it in its home Dock, active.
    shell.run("panel.show:messages", json!({})).unwrap();
    let place = shell.layout().place(&messages).unwrap();
    assert_eq!(place.area, Area::Dock(DockSide::Bottom));
    assert_eq!(shell.layout().focus, Some(messages));

    let sheet = sheet(&shell);
    shell
        .run("layout.tab.close", json!({"tab": tab_param(&sheet)}))
        .unwrap();
    assert!(shell.layout().place(&sheet).is_none());
    assert!(shell.active_document().is_none());
}

#[test]
fn every_panel_has_a_show_preset() {
    let shell = Shell::new(engine());
    let presets: Vec<String> = shell.engine().registry().describe("panel.show").unwrap()["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        presets,
        [
            "panel.show:projects",
            "panel.show:properties",
            "panel.show:messages",
            "panel.show:library",
            "panel.show:layers",
            "panel.show:journal",
            "panel.show:navigator",
        ]
    );
}

#[test]
fn a_collapsed_dock_opens_its_tabs_as_flyouts() {
    let mut shell = Shell::new(engine());
    shell
        .run(
            "layout.dock.collapse",
            json!({"dock": "left", "collapsed": true}),
        )
        .unwrap();
    assert!(shell.layout().dock(DockSide::Left).collapsed);
    assert_eq!(shell.layout().flyout, None);

    shell
        .run("layout.tab.activate", json!({"tab": "projects"}))
        .unwrap();
    assert_eq!(
        shell.layout().flyout,
        Some(TabKey::Panel(PanelId::Projects))
    );

    // Collapsing again dismisses the flyout; expanding shows the Dock.
    shell
        .run("layout.dock.collapse", json!({"dock": "left"}))
        .unwrap();
    assert_eq!(shell.layout().flyout, None);
    shell
        .run(
            "layout.dock.collapse",
            json!({"dock": "left", "collapsed": false}),
        )
        .unwrap();
    assert!(!shell.layout().dock(DockSide::Left).collapsed);

    // An empty Dock has nothing to collapse.
    shell
        .run("layout.tab.close", json!({"tab": "messages"}))
        .unwrap();
    let err = shell
        .run("layout.dock.collapse", json!({"dock": "bottom"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn layout_commands_are_view_commands_unavailable_headless() {
    let mut headless = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    headless.register(reda_ui::layout::register_all);
    let err = headless
        .run("layout.tab.close", json!({"tab": "projects"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert_eq!(err.message, "requires the REDA app");
}

#[test]
fn layout_commands_go_in_the_journal() {
    let mut shell = Shell::new(engine());
    shell
        .run("layout.tab.close", json!({"tab": "projects"}))
        .unwrap();
    let _ = shell.run("layout.tab.close", json!({"tab": "projects"}));
    let journal = shell.engine().journal();
    let last: Vec<(&str, bool)> = journal
        .iter()
        .map(|e| (e.id.as_str(), e.outcome.is_ok()))
        .collect();
    assert_eq!(
        last,
        [("layout.tab.close", true), ("layout.tab.close", false)]
    );
}

#[test]
fn document_open_opens_a_document_beside_the_active_one_and_focuses_it() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    let first = sheet(&shell);
    let pcb = shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .find(|d| d.kind == DocumentKind::Pcb)
        .unwrap()
        .uuid;

    let out = shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    let tab = TabKey::Document(pcb);
    let place = shell.layout().place(&tab).unwrap();
    assert_eq!(place.group, shell.layout().place(&first).unwrap().group);
    assert_eq!(out["group"], json!(place.group));
    assert_eq!(shell.layout().focus, Some(tab));
    assert_eq!(shell.active_document().unwrap().document, pcb);
    assert_eq!(shell.document_tabs().len(), 2);

    // Opening it again only shows it.
    shell
        .run("layout.tab.activate", json!({"tab": tab_param(&first)}))
        .unwrap();
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    assert_eq!(shell.document_tabs().len(), 2);
    assert_eq!(shell.active_document().unwrap().document, pcb);

    let missing = "00000000-0000-4000-8000-0000000000ff";
    let err = shell
        .run("document.open", json!({"document": missing}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn closing_the_active_tab_activates_the_groups_last_active_tab() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    shell.run("panel.show:journal", json!({})).unwrap();
    let bottom = shell
        .layout()
        .place(&TabKey::Panel(PanelId::Messages))
        .unwrap()
        .group;
    shell.run("panel.show:library", json!({})).unwrap();
    shell
        .run(
            "layout.tab.move_to_group",
            json!({"tab": "library", "group": bottom}),
        )
        .unwrap();
    let group = |shell: &Shell| shell.layout().group(bottom).unwrap().clone();
    assert_eq!(
        group(&shell).tabs,
        [
            TabKey::Panel(PanelId::Messages),
            TabKey::Panel(PanelId::Journal),
            TabKey::Panel(PanelId::Library)
        ]
    );
    // Messages, then Library: closing Library goes back to Messages, not its neighbour.
    for tab in ["messages", "library"] {
        shell
            .run("layout.tab.activate", json!({"tab": tab}))
            .unwrap();
    }
    shell
        .run("layout.tab.close", json!({"tab": "library"}))
        .unwrap();
    assert_eq!(
        group(&shell).active_tab(),
        Some(&TabKey::Panel(PanelId::Messages))
    );
}

#[test]
fn a_closed_panel_reopens_where_it_was() {
    let work = TempDir::new("layout-work");
    let mut shell = shell_with_project(&work);
    let sheet = sheet(&shell);
    let centre = shell.layout().place(&sheet).unwrap().group;
    shell
        .run(
            "layout.tab.move_to_group",
            json!({"tab": "properties", "group": centre, "index": 0}),
        )
        .unwrap();
    shell
        .run("layout.tab.close", json!({"tab": "properties"}))
        .unwrap();
    shell.run("panel.show:properties", json!({})).unwrap();
    let place = shell
        .layout()
        .place(&TabKey::Panel(PanelId::Properties))
        .unwrap();
    assert_eq!(
        (place.area, place.group, place.index),
        (Area::Centre, centre, 0),
        "its group and position, not its home Dock"
    );

    // Its group is gone: it reopens in the Dock it was in, not its home Dock.
    shell
        .run(
            "layout.tab.move_to_dock",
            json!({"tab": "messages", "dock": "left"}),
        )
        .unwrap();
    shell
        .run(
            "layout.tab.split",
            json!({"tab": "messages", "side": "down"}),
        )
        .unwrap();
    shell
        .run("layout.tab.close", json!({"tab": "messages"}))
        .unwrap();
    shell.run("panel.show:messages", json!({})).unwrap();
    let place = shell
        .layout()
        .place(&TabKey::Panel(PanelId::Messages))
        .unwrap();
    assert_eq!(place.area, Area::Dock(DockSide::Left));
}
