//! Routing and Track editing in the window, driven with the mouse and keys through
//! `egui_kittest`: T and two clicks route a Track that commits as one `pcb.track.add`,
//! dragging the segment runs one `pcb.track.drag`, and Del deletes it.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
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
