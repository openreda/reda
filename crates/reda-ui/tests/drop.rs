//! Drag-and-drop targets in the layout model, with no window: each drop target names the
//! one layout Command a drop on it runs, and a dry run says before the release whether the
//! layout would refuse it.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::ErrorCode;
use reda_ui::layout::{
    Area, Axis, DockSide, DropOutcome, DropPlan, DropTarget, Node, PanelId, SplitSide, TabKey,
};
use reda_ui::shell::Shell;
use serde_json::json;

fn engine() -> Engine {
    Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build()
}

fn shell_with_project(work: &TempDir) -> Shell {
    let mut shell = Shell::new(engine());
    shell.new_project(work.path(), "Drop").unwrap();
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

fn group_tabs(node: &Node) -> Vec<TabKey> {
    match node {
        Node::Group(g) => g.tabs.clone(),
        Node::Split { .. } => panic!("expected a group, got {node:?}"),
    }
}

#[test]
fn a_tab_splits_to_the_left_of_another_group() {
    let work = TempDir::new("drop-work");
    let mut shell = shell_with_project(&work);
    let sheet = sheet(&shell);
    let centre = shell.layout().place(&sheet).unwrap().group;
    let out = shell
        .run(
            "layout.tab.split",
            json!({"tab": "properties", "group": centre, "side": "left"}),
        )
        .unwrap();
    let layout = shell.layout();
    let Node::Split { axis, children } = &layout.centre else {
        panic!("a split, got {:?}", layout.centre)
    };
    assert_eq!(*axis, Axis::Horizontal);
    assert_eq!(
        group_tabs(&children[0]),
        [TabKey::Panel(PanelId::Properties)]
    );
    assert_eq!(group_tabs(&children[1]), [sheet]);
    let place = layout.place(&TabKey::Panel(PanelId::Properties)).unwrap();
    assert_eq!(out, json!({"group": place.group}));
    // The right Dock gave up its only tab.
    assert!(layout.dock(DockSide::Right).groups.is_empty());
}

#[test]
fn a_tab_splits_up_inside_a_dock_but_not_sideways() {
    let mut shell = Shell::new(engine());
    let projects = shell
        .layout()
        .place(&TabKey::Panel(PanelId::Projects))
        .unwrap()
        .group;
    shell
        .run(
            "layout.tab.split",
            json!({"tab": "messages", "group": projects, "side": "up"}),
        )
        .unwrap();
    let left = &shell.layout().dock(DockSide::Left).groups;
    let tabs: Vec<_> = left.iter().map(|g| g.tabs.clone()).collect();
    assert_eq!(
        tabs,
        [
            vec![TabKey::Panel(PanelId::Messages)],
            vec![TabKey::Panel(PanelId::Projects)]
        ]
    );

    let before = shell.layout().clone();
    let err = shell
        .run(
            "layout.tab.split",
            json!({"tab": "properties", "group": projects, "side": "left"}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
    assert_eq!(*shell.layout(), before);
    assert_eq!(
        before
            .place(&TabKey::Panel(PanelId::Properties))
            .unwrap()
            .area,
        Area::Dock(DockSide::Right)
    );
}

/// Runs a planned drop's Command and checks it leaves the layout the dry run promised.
fn drop_on(shell: &mut Shell, tab: &TabKey, target: DropTarget) -> DropPlan {
    let plan = shell.layout().plan_drop(tab, target);
    let before = shell.layout().clone();
    let result = shell.run(plan.command, plan.params.clone());
    match &plan.outcome {
        DropOutcome::Refused(reason) => {
            assert_eq!(&result.unwrap_err().message, reason);
            assert_eq!(*shell.layout(), before);
        }
        _ => {
            result.unwrap();
        }
    }
    plan
}

#[test]
fn each_drop_target_names_one_layout_command() {
    let work = TempDir::new("drop-work");
    let mut shell = shell_with_project(&work);
    let sheet = sheet(&shell);
    let centre = shell.layout().place(&sheet).unwrap().group;
    let projects = TabKey::Panel(PanelId::Projects);

    let plan = drop_on(
        &mut shell,
        &projects,
        DropTarget::Group {
            group: centre,
            slot: Some(0),
        },
    );
    assert_eq!(plan.command, "layout.tab.move_to_group");
    assert_eq!(
        plan.params,
        json!({"tab": "projects", "group": centre, "index": 0})
    );
    assert_eq!(plan.outcome, DropOutcome::Moves);
    assert_eq!(
        shell.layout().group(centre).unwrap().tabs,
        [projects.clone(), sheet.clone()]
    );

    let plan = drop_on(
        &mut shell,
        &projects,
        DropTarget::Split {
            group: centre,
            side: SplitSide::Down,
        },
    );
    assert_eq!(plan.command, "layout.tab.split");
    assert_eq!(
        plan.params,
        json!({"tab": "projects", "group": centre, "side": "down"})
    );

    let plan = drop_on(&mut shell, &projects, DropTarget::Dock(DockSide::Bottom));
    assert_eq!(plan.command, "layout.tab.move_to_dock");
    assert_eq!(plan.params, json!({"tab": "projects", "dock": "bottom"}));
    assert_eq!(
        shell.layout().place(&projects).unwrap().area,
        Area::Dock(DockSide::Bottom)
    );
}

#[test]
fn reordering_within_a_group_counts_slots_around_the_dragged_tab() {
    let work = TempDir::new("drop-work");
    let mut shell = shell_with_project(&work);
    let sheet = sheet(&shell);
    let centre = shell.layout().place(&sheet).unwrap().group;
    for panel in ["projects", "messages"] {
        shell
            .run(
                "layout.tab.move_to_group",
                json!({"tab": panel, "group": centre}),
            )
            .unwrap();
    }
    // [sheet, projects, messages]: dropping the sheet before messages puts it second.
    let plan = drop_on(
        &mut shell,
        &sheet,
        DropTarget::Group {
            group: centre,
            slot: Some(2),
        },
    );
    assert_eq!(plan.params["index"], json!(1));
    assert_eq!(
        shell.layout().group(centre).unwrap().tabs,
        [
            TabKey::Panel(PanelId::Projects),
            sheet.clone(),
            TabKey::Panel(PanelId::Messages)
        ]
    );
    // Dropping it back where it is changes nothing, so the drop runs nothing.
    let plan = shell.layout().plan_drop(
        &sheet,
        DropTarget::Group {
            group: centre,
            slot: Some(1),
        },
    );
    assert_eq!(plan.outcome, DropOutcome::Unchanged);
    let plan = shell.layout().plan_drop(
        &sheet,
        DropTarget::Group {
            group: centre,
            slot: Some(2),
        },
    );
    assert_eq!(plan.outcome, DropOutcome::Unchanged);
}

#[test]
fn a_drop_the_layout_would_refuse_is_known_before_the_release() {
    let mut shell = Shell::new(engine());
    let messages = TabKey::Panel(PanelId::Messages);
    let bottom = shell.layout().place(&messages).unwrap().group;
    let plan = drop_on(
        &mut shell,
        &messages,
        DropTarget::Split {
            group: bottom,
            side: SplitSide::Up,
        },
    );
    let DropOutcome::Refused(reason) = plan.outcome else {
        panic!("a lone tab cannot split from its own group")
    };
    assert!(reason.contains("alone in its group"), "{reason}");

    let projects = shell
        .layout()
        .place(&TabKey::Panel(PanelId::Projects))
        .unwrap()
        .group;
    let plan = drop_on(
        &mut shell,
        &messages,
        DropTarget::Split {
            group: projects,
            side: SplitSide::Right,
        },
    );
    assert!(matches!(plan.outcome, DropOutcome::Refused(r) if r.contains("one column")));
    // The refusals reached the Journal as failed Commands, not as layout changes.
    assert!(shell.engine().journal().iter().all(|e| e.outcome.is_err()));
}
