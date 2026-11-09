//! Routing and Track editing in the window, driven with the mouse and keys through
//! `egui_kittest`: T and two clicks route a Track that commits as one `pcb.track.add`,
//! dragging the segment runs one `pcb.track.drag`, and Del deletes it.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::shell::Shell;
use serde_json::json;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// The window on a board with two 2 mm Pads on Net SIG at (0, 0) and (10, 0) mm, its PCB
/// tab focused.
fn window() -> Window {
    let home = TempDir::new("pcb-route-canvas-home");
    let work = TempDir::new("pcb-route-canvas-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let net = shell.run("test.net.add", json!({"name": "SIG"})).unwrap()["result"]["uuid"].clone();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "test.footprint.add",
                json!({"at": {"x": x, "y": 0}, "pads": [
                    {"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm", "net": net}
                ]}),
            )
            .unwrap();
    }
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    let h = Harness::builder()
        .with_size([1000.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Window {
        h,
        pcb,
        _home: home,
        _work: work,
    }
}

fn shell(w: &Window) -> &Shell {
    w.h.state().shell()
}

fn ids_since(w: &Window, from: usize) -> Vec<String> {
    shell(w).engine().journal()[from..]
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

fn last_command(w: &Window) -> String {
    shell(w).engine().journal().last().unwrap().id.clone()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

fn on_screen(w: &Window, p: Point) -> Pos2 {
    let rect = w.h.get_by_role(Role::Canvas).rect();
    let camera = shell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64))
}

fn click(w: &mut Window, at: Pos2) {
    w.h.hover_at(at);
    w.h.run();
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
}

fn tracks(w: &Window) -> Vec<(Point, Point)> {
    let pcb = shell(w).engine().project().unwrap().pcb().unwrap();
    pcb.tracks().map(|t| t.geometry.ends()).collect()
}

#[test]
fn t_and_two_clicks_route_a_track_then_it_drags_and_deletes() {
    let mut w = window();
    w.h.run();
    // Focus the board: a click on empty canvas.
    let empty = on_screen(&w, mm(5.0, 8.0));
    click(&mut w, empty);

    let start = shell(&w).engine().journal().len();
    w.h.key_press(Key::T);
    w.h.run();
    assert_eq!(last_command(&w), "tool.activate:route");
    let a = on_screen(&w, mm(0.0, 0.0));
    let b = on_screen(&w, mm(10.0, 0.0));
    click(&mut w, a);
    w.h.hover_at(a + (b - a) / 2.0);
    w.h.run();
    assert!(
        !ids_since(&w, start).iter().any(|id| id == "pcb.track.add"),
        "nothing commits in flight"
    );
    click(&mut w, b);
    let ids = ids_since(&w, start);
    assert_eq!(
        ids.iter().filter(|id| *id == "pcb.track.add").count(),
        1,
        "{ids:?}"
    );
    assert_eq!(tracks(&w), [(mm(0.0, 0.0), mm(10.0, 0.0))]);

    // Back to Select; drag the segment down.
    w.h.key_press(Key::Escape);
    w.h.run();
    assert_eq!(last_command(&w), "tool.activate:select");
    let grab = on_screen(&w, mm(5.0, 0.0));
    let to = on_screen(&w, mm(5.0, -3.0));
    let before = shell(&w).engine().journal().len();
    w.h.drag_at(grab);
    w.h.run();
    w.h.hover_at(grab + (to - grab) / 2.0);
    w.h.run();
    w.h.hover_at(to);
    w.h.run();
    assert!(
        !ids_since(&w, before)
            .iter()
            .any(|id| id == "pcb.track.drag")
    );
    w.h.drop_at(to);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.track.drag");
    let moved = tracks(&w);
    assert_eq!(moved.len(), 3, "bridges back to both Pads: {moved:?}");

    // The dragged segment is selected: Del deletes it.
    w.h.key_press(Key::Delete);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.copper.delete");
    assert_eq!(tracks(&w).len(), 2);
}

fn route_with_keys(w: &mut Window) {
    w.h.run();
    let empty = on_screen(w, mm(5.0, 8.0));
    click(w, empty);
    w.h.key_press(Key::T);
    w.h.run();
}

#[test]
fn a_route_with_a_via_commits_as_one_batch() {
    let mut w = window();
    route_with_keys(&mut w);
    let start = shell(&w).engine().journal().len();
    let a = on_screen(&w, mm(0.0, 0.0));
    let mid = on_screen(&w, mm(0.0, 4.0));
    let end = on_screen(&w, mm(4.0, 4.0));
    click(&mut w, a);
    w.h.hover_at(mid);
    w.h.run();
    w.h.key_press(Key::V);
    w.h.run();
    click(&mut w, end);
    w.h.key_press(Key::Enter);
    w.h.run();
    let edits: Vec<String> = ids_since(&w, start)
        .into_iter()
        .filter(|id| {
            shell(&w)
                .engine()
                .registry()
                .get(id.split(':').next().unwrap())
                .is_none_or(|m| m.kind == reda_ui::engine::command::Kind::Edit)
        })
        .collect();
    assert_eq!(edits, ["batch"], "one batch per route");
    let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.tracks().count(), 2);
    assert_eq!(pcb.vias().count(), 1);
}

#[test]
fn right_click_ends_the_route_instead_of_opening_the_menu() {
    let mut w = window();
    route_with_keys(&mut w);
    let a = on_screen(&w, mm(0.0, 0.0));
    let b = on_screen(&w, mm(0.0, 4.0));
    click(&mut w, a);
    click(&mut w, b);
    w.h.hover_at(on_screen(&w, mm(3.0, 4.0)));
    w.h.run();
    w.h.get_by_role(Role::Canvas).click_secondary();
    w.h.run();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.track.add", "the route ended, kept");
    assert_eq!(tracks(&w), [(mm(0.0, 0.0), mm(0.0, 4.0))]);
    assert!(
        w.h.query_by_label("Rotate").is_none(),
        "no context menu while routing"
    );
    assert_eq!(shell(&w).tool(), reda_ui::tools::Tool::Route);
}

#[test]
fn the_routing_bar_floats_over_the_bottom_while_routing_and_its_buttons_run_the_keys() {
    use reda_ui::bars::draw::tool_bar_anchor;
    let mut w = window();
    w.h.run();
    assert!(
        w.h.query_by_label("Toggle Look-Ahead").is_none(),
        "only in the Tool"
    );
    route_with_keys(&mut w);
    let canvas = w.h.get_by_role(Role::Canvas).rect();
    let anchor = tool_bar_anchor(canvas, false);
    let look = w.h.get_by_label("Toggle Look-Ahead");
    let bar = look.rect();
    assert!(
        (bar.bottom() - anchor.y).abs() < 8.0,
        "{bar:?} at {anchor:?}"
    );
    assert!(bar.center().y > canvas.center().y, "at the bottom");
    let toggled = |w: &Window, label: &str| {
        w.h.get_by_label(label).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
    };
    assert!(toggled(&w, "Toggle Look-Ahead"), "on by default");
    assert!(toggled(&w, "Next Obstacle Mode"), "toggled in `stop`");
    for label in [
        "Next Obstacle Mode",
        "Toggle Auto-Terminate",
        "Toggle Loop Removal",
        "Next Clearance Display",
    ] {
        assert!(w.h.query_by_label(label).is_some(), "{label}");
    }
    assert!(
        w.h.query_by_label_contains("Choose Width").is_none(),
        "the width shows only with a route in flight"
    );

    let start = shell(&w).engine().journal().len();
    w.h.get_by_label("Toggle Look-Ahead").click();
    w.h.run();
    assert_eq!(ids_since(&w, start), ["pcb.route.mode:lookahead"]);
    assert!(!shell(&w).router().lookahead());
    assert!(!toggled(&w, "Toggle Look-Ahead"));
    w.h.get_by_label("Toggle Loop Removal").click();
    w.h.run();
    assert!(!shell(&w).preferences().pcb.route.loop_removal);

    // With a route in flight the width shows; a click opens the chooser, and a row
    // runs `pcb.route.width`.
    let a = on_screen(&w, mm(0.0, 0.0));
    click(&mut w, a);
    w.h.hover_at(on_screen(&w, mm(0.0, 4.0)));
    w.h.run();
    w.h.get_by_label("Choose Width: 0.25 mm").click();
    w.h.run();
    assert!(shell(&w).router().chooser().is_some());
    w.h.get_by_label("0.25 mm  preferred").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.route.width");
    assert!(shell(&w).router().chooser().is_none());
}

#[test]
fn the_routing_bar_moves_up_with_the_layer_strip() {
    use reda_ui::bars::draw::tool_bar_anchor;
    let mut w = window();
    route_with_keys(&mut w);
    w.h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "pcb.view.layer_control", "value": "strip"}),
        )
        .unwrap();
    w.h.run();
    w.h.run();
    let canvas = w.h.get_by_role(Role::Canvas).rect();
    let anchor = tool_bar_anchor(canvas, true);
    let bar = w.h.get_by_label("Toggle Look-Ahead").rect();
    assert!(
        (bar.bottom() - anchor.y).abs() < 8.0,
        "{bar:?} at {anchor:?}"
    );
}

#[test]
fn the_tools_note_shows_just_above_the_bar() {
    let mut w = window();
    let sig = shell(&w)
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .nets()
        .next()
        .unwrap()
        .uuid;
    w.h.state_mut()
        .shell_mut()
        .run(
            "pcb.track.add",
            json!({"layer": "copper_top", "net": sig, "width": "1mm",
                   "points": [{"x": 0, "y": 0}, {"x": 0, "y": "5mm"},
                              {"x": "10mm", "y": "5mm"}, {"x": "10mm", "y": 0}]}),
        )
        .unwrap();
    w.h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "pcb.route.pickup_width", "value": false}),
        )
        .unwrap();
    route_with_keys(&mut w);
    let a = on_screen(&w, mm(0.0, 0.0));
    click(&mut w, a);
    w.h.hover_at(on_screen(&w, mm(10.0, 0.0)));
    w.h.run();
    w.h.run();
    let note =
        w.h.get_by_label("Loop kept: the new route is narrower than the old path");
    let bar = w.h.get_by_label("Toggle Look-Ahead").rect();
    assert!(note.rect().bottom() <= bar.top() + 1.0, "above the bar");
    assert!(note.rect().bottom() > bar.top() - 20.0, "just above");
}
