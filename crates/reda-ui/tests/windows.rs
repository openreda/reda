//! Dragging tabs between native windows, with no window: a release hit-tested by screen
//! position on X11, Windows and macOS, the Wayland blind drop, and where new windows open.
//!
//! The main window's content sits at (100, 100) on screen and is 1000 × 700; a floating
//! window holding Properties alone sits at (1200, 100) and is 400 × 300.

use egui::{Pos2, Rect, pos2};
use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::layout::{GroupId, Node, PanelId, TabKey, WindowId};
use reda_ui::shell::Shell;
use reda_ui::windows::{
    BlindDrop, Drop, GroupZone, Placements, Release, WindowView, Zones, release,
};
use serde_json::json;

fn engine() -> Engine {
    Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build()
}

struct Setup {
    shell: Shell,
    sheet: TabKey,
    centre: GroupId,
    window: WindowId,
    window_group: GroupId,
    _work: TempDir,
}

/// A Project open on its sheet, with Properties moved into a window of its own.
fn setup() -> Setup {
    let work = TempDir::new("windows-work");
    let mut shell = Shell::new(engine());
    shell.new_project(work.path(), "Windows").unwrap();
    let uuid = shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .next()
        .unwrap()
        .uuid;
    let sheet = TabKey::Document(uuid);
    shell
        .run(
            "layout.tab.move_to_new_window",
            json!({"tab": "properties"}),
        )
        .unwrap();
    let layout = shell.layout();
    let centre = layout.place(&sheet).unwrap().group;
    let window = layout.windows[0].id;
    let Node::Group(g) = &layout.windows[0].root else {
        panic!("a window of one group")
    };
    let window_group = g.id;
    Setup {
        shell,
        sheet,
        centre,
        window,
        window_group,
        _work: work,
    }
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect::from_min_max(pos2(x0, y0), pos2(x1, y1))
}

/// A group drawn in `r` with one 96-point tab at the start of its 24-point strip.
fn group(id: GroupId, r: Rect) -> GroupZone {
    GroupZone {
        group: id,
        rect: r,
        strip: rect(r.left(), r.top(), r.right(), r.top() + 24.0),
        tabs: vec![rect(
            r.left() + 4.0,
            r.top() + 2.0,
            r.left() + 100.0,
            r.top() + 22.0,
        )],
    }
}

fn views(s: &Setup, origins: bool) -> (WindowView, WindowView) {
    let main_area = rect(0.0, 0.0, 1000.0, 700.0);
    let main = WindowView {
        window: None,
        origin: origins.then_some(pos2(100.0, 100.0)),
        rect: main_area,
        zones: Zones {
            area: main_area,
            groups: vec![group(s.centre, rect(200.0, 50.0, 800.0, 600.0))],
            docks: Vec::new(),
            dock_edges: true,
        },
        first_group: Some(s.centre),
    };
    let win_area = rect(0.0, 0.0, 400.0, 300.0);
    let floating = WindowView {
        window: Some(s.window),
        origin: origins.then_some(pos2(1200.0, 100.0)),
        rect: win_area,
        zones: Zones {
            area: win_area,
            groups: vec![group(s.window_group, win_area)],
            docks: Vec::new(),
            dock_edges: false,
        },
        first_group: Some(s.window_group),
    };
    (main, floating)
}

fn run(command: &'static str, params: serde_json::Value) -> Release {
    Release::Run(Drop {
        command,
        params,
        new_window_at: None,
    })
}

#[test]
fn a_release_inside_its_own_window_drops_there() {
    let s = setup();
    let (main, floating) = views(&s, true);
    let windows = [main.clone(), floating];
    // On the sheet's own strip, after its tab: the tab stays where it is.
    let out = release(
        s.shell.layout(),
        &s.sheet,
        &main,
        pos2(400.0, 60.0),
        &windows,
    );
    assert_eq!(out, Release::Nothing);
    // Projects (in the left Dock) dropped on the centre's strip before the sheet.
    let projects = TabKey::Panel(PanelId::Projects);
    let out = release(
        s.shell.layout(),
        &projects,
        &main,
        pos2(205.0, 60.0),
        &windows,
    );
    assert_eq!(
        out,
        run(
            "layout.tab.move_to_group",
            json!({"tab": "projects", "group": s.centre, "index": 0})
        )
    );
}

#[test]
fn a_release_over_another_window_drops_on_its_target_by_screen_position() {
    let s = setup();
    let (main, floating) = views(&s, true);
    let windows = [main.clone(), floating];
    // Screen (1250, 110) = the floating window's strip at (50, 10), before Properties.
    let out = release(
        s.shell.layout(),
        &s.sheet,
        &main,
        pos2(1150.0, 10.0),
        &windows,
    );
    assert_eq!(
        out,
        run(
            "layout.tab.move_to_group",
            json!({"tab": s.sheet.to_string(), "group": s.window_group, "index": 0})
        )
    );
    // Screen (1400, 262) = the "Drop into Group" button at the centre of its body.
    let out = release(
        s.shell.layout(),
        &s.sheet,
        &main,
        pos2(1300.0, 162.0),
        &windows,
    );
    assert_eq!(
        out,
        run(
            "layout.tab.move_to_group",
            json!({"tab": s.sheet.to_string(), "group": s.window_group})
        )
    );
    // Screen (1250, 250): over the window but on no target, so nothing happens.
    let out = release(
        s.shell.layout(),
        &s.sheet,
        &main,
        pos2(1150.0, 150.0),
        &windows,
    );
    assert_eq!(out, Release::Nothing);
}

#[test]
fn a_release_outside_every_window_floats_the_tab_in_a_new_window_there() {
    let s = setup();
    let (main, floating) = views(&s, true);
    let windows = [main.clone(), floating.clone()];
    let out = release(
        s.shell.layout(),
        &s.sheet,
        &main,
        pos2(-50.0, 300.0),
        &windows,
    );
    assert_eq!(
        out,
        Release::Run(Drop {
            command: "layout.tab.move_to_new_window",
            params: json!({"tab": s.sheet.to_string()}),
            new_window_at: Some(pos2(50.0, 400.0)),
        })
    );
    // A tab alone in its window takes the window with it instead.
    let properties = TabKey::Panel(PanelId::Properties);
    let out = release(
        s.shell.layout(),
        &properties,
        &floating,
        pos2(500.0, 500.0),
        &windows,
    );
    assert_eq!(
        out,
        Release::MoveWindow {
            window: s.window,
            to: pos2(1700.0, 600.0)
        }
    );
}

#[test]
fn without_screen_positions_a_release_outside_the_window_is_blind() {
    let s = setup();
    let (main, floating) = views(&s, false);
    let windows = [main.clone(), floating];
    // Even where another window would be on X11.
    let out = release(
        s.shell.layout(),
        &s.sheet,
        &main,
        pos2(1150.0, 10.0),
        &windows,
    );
    assert_eq!(out, Release::Blind);
    // Inside its own window a drop works as ever.
    let projects = TabKey::Panel(PanelId::Projects);
    let out = release(
        s.shell.layout(),
        &projects,
        &main,
        pos2(205.0, 60.0),
        &windows,
    );
    assert!(matches!(out, Release::Run(_)));
}

#[test]
fn the_first_window_to_see_the_pointer_takes_a_blind_drop() {
    let s = setup();
    let (main, floating) = views(&s, false);
    let blind = BlindDrop {
        tab: s.sheet.clone(),
        since: 10.0,
    };
    // The pointer is not in this window yet.
    assert_eq!(
        blind.claim(s.shell.layout(), &floating, pos2(-5.0, 10.0)),
        None
    );
    // On its strip: among its tabs, at the pointer.
    assert_eq!(
        blind.claim(s.shell.layout(), &floating, pos2(150.0, 10.0)),
        Some(run(
            "layout.tab.move_to_group",
            json!({"tab": s.sheet.to_string(), "group": s.window_group, "index": 1})
        ))
    );
    // On a group's body: into that group.
    assert_eq!(
        blind.claim(s.shell.layout(), &floating, pos2(300.0, 250.0)),
        Some(run(
            "layout.tab.move_to_group",
            json!({"tab": s.sheet.to_string(), "group": s.window_group})
        ))
    );
    // Off every group (say, over a Dock edge with no Dock): into the first group.
    let properties = BlindDrop {
        tab: TabKey::Panel(PanelId::Properties),
        since: 10.0,
    };
    assert_eq!(
        properties.claim(s.shell.layout(), &main, pos2(20.0, 650.0)),
        Some(run(
            "layout.tab.move_to_group",
            json!({"tab": "properties", "group": s.centre})
        ))
    );
    // The source window seeing the pointer back where the tab already is: nothing.
    assert_eq!(
        blind.claim(s.shell.layout(), &main, pos2(20.0, 650.0)),
        Some(Release::Nothing)
    );
}

#[test]
fn a_blind_drop_no_window_takes_floats_in_a_new_window_after_the_timeout() {
    let s = setup();
    let blind = BlindDrop {
        tab: s.sheet.clone(),
        since: 10.0,
    };
    assert!(!blind.expired(10.69));
    assert!(blind.expired(10.7));
    assert_eq!(
        blind.expire(s.shell.layout()),
        run(
            "layout.tab.move_to_new_window",
            json!({"tab": s.sheet.to_string()})
        )
    );
    // A tab already alone in its window stays there.
    let properties = BlindDrop {
        tab: TabKey::Panel(PanelId::Properties),
        since: 10.0,
    };
    assert_eq!(properties.expire(s.shell.layout()), Release::Nothing);
}

#[test]
fn a_new_window_opens_where_its_tab_was_dropped_once() {
    let mut s = setup();
    let mut placements = Placements::default();
    placements.insert(s.sheet.clone(), pos2(50.0, 400.0));
    // Not yet in a window: the placement waits for the drop's Command.
    assert_eq!(placements.take(s.shell.layout(), s.window), None);
    s.shell
        .run(
            "layout.tab.move_to_new_window",
            json!({"tab": s.sheet.to_string()}),
        )
        .unwrap();
    let new = s.shell.layout().place(&s.sheet).unwrap().area;
    let reda_ui::layout::Area::Window(new) = new else {
        panic!("in a window")
    };
    assert_eq!(placements.take(s.shell.layout(), s.window), None);
    assert_eq!(
        placements.take(s.shell.layout(), new),
        Some(pos2(50.0, 400.0))
    );
    assert_eq!(placements.take(s.shell.layout(), new), None::<Pos2>);
}

#[test]
fn a_window_keeps_where_it_was_put_through_save_and_open() {
    let mut s = setup();
    s.shell
        .run(
            "layout.window.place",
            json!({"window": s.window, "x": 1200, "y": 100, "width": 400, "height": 300}),
        )
        .unwrap();
    let rect = s.shell.layout().windows[0].rect.unwrap();
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (1200, 100, 400, 300)
    );
    let err = s
        .shell
        .run(
            "layout.window.place",
            json!({"window": 999, "x": 0, "y": 0, "width": 400, "height": 300}),
        )
        .unwrap_err();
    assert_eq!(err.code, reda_ui::engine::command::ErrorCode::NotFound);

    // The layout, and so view.json, keeps it.
    let json = serde_json::to_value(s.shell.layout()).unwrap();
    let back: reda_ui::layout::Layout = serde_json::from_value(json).unwrap();
    assert_eq!(back.windows[0].rect, Some(rect));
}
