//! The PCB canvas and the Layers panel in the window, driven through AccessKit with
//! `egui_kittest`: dragging a Footprint with the mouse runs nothing until release, the
//! view keys run their Commands, the Layers panel hides Layers, and a board with no
//! Board Outline offers to create a rectangle.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::Point;
use reda_ui::engine::model::{LayerKind, Side, Uuid};
use reda_ui::engine::preferences::Units;
use reda_ui::shell::Shell;
use serde_json::json;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    footprint: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// The window on a board with one Footprint (a 2 mm Pad) at the origin, with its PCB tab
/// focused and the Layers panel open.
fn window() -> Window {
    let home = TempDir::new("pcb-canvas-home");
    let work = TempDir::new("pcb-canvas-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let out = shell
        .run(
            "test.footprint.add",
            json!({"at": {"x": 0, "y": 0}, "pads": [
                {"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm"}
            ]}),
        )
        .unwrap();
    let footprint = out["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    shell.run("panel.show", json!({"panel": "layers"})).unwrap();
    let h = Harness::builder()
        .with_size([1000.0, 1100.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Window {
        h,
        pcb,
        footprint,
        _home: home,
        _work: work,
    }
}

fn shell(w: &Window) -> &Shell {
    w.h.state().shell()
}

fn journal_len(w: &Window) -> usize {
    shell(w).engine().journal().len()
}

fn last_command(w: &Window) -> String {
    shell(w).engine().journal().last().unwrap().id.clone()
}

/// Where the board point `p` shows in the window.
fn on_screen(w: &Window, p: Point) -> Pos2 {
    let canvas = w.h.get_by_role(Role::Canvas);
    let rect = canvas.rect();
    let camera = shell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64))
}

fn footprint_at(w: &Window) -> Point {
    let pcb = shell(w).engine().project().unwrap().pcb().unwrap();
    pcb.footprint(w.footprint).unwrap().at
}

#[test]
fn dragging_a_footprint_runs_one_move_on_release() {
    let mut w = window();
    w.h.run();
    let from = on_screen(&w, Point::default());
    let before = journal_len(&w);

    w.h.drag_at(from);
    w.h.run();
    w.h.hover_at(from + egui::vec2(40.0, 0.0));
    w.h.run();
    w.h.hover_at(from + egui::vec2(80.0, 0.0));
    w.h.run();
    assert_eq!(
        footprint_at(&w),
        Point::default(),
        "nothing moves in flight"
    );
    assert!(
        shell(&w).engine().journal()[before..]
            .iter()
            .all(|e| e.id != "pcb.move"),
        "the move waits for release"
    );

    w.h.drop_at(from + egui::vec2(80.0, 0.0));
    w.h.run();
    assert_eq!(last_command(&w), "pcb.move");
    let at = footprint_at(&w);
    assert!(
        at.x.0 > 0 && at.y.0 == 0,
        "moved right, on the grid: {at:?}"
    );
    assert_eq!(at.x.0 % 500_000, 0);
}

#[test]
fn the_view_keys_run_their_commands_on_the_focused_board() {
    let mut w = window();
    w.h.run();
    // Focus the board: a press on its canvas.
    let empty = w.h.get_by_role(Role::Canvas).rect().left_bottom() + egui::vec2(30.0, -60.0);
    w.h.drag_at(empty);
    w.h.drop_at(empty);
    w.h.run();

    // The Keymap binds logical keys: "+" is egui's Key::Plus (typed with Shift or not).
    w.h.key_press_modifiers(Modifiers::SHIFT, Key::Plus);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.step:next");
    let bottom = {
        let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
        pcb.layers()
            .find(LayerKind::Copper, Some(Side::Bottom))
            .unwrap()
            .uuid
    };
    let project = shell(&w).engine().project().unwrap();
    assert_eq!(shell(&w).board_view().active_layer(project), Some(bottom));

    w.h.key_press_modifiers(Modifiers::CTRL, Key::Q);
    w.h.run();
    assert_eq!(shell(&w).board_view().units, Units::Mil);

    w.h.key_press_modifiers(Modifiers::SHIFT, Key::S);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.view.inactive_layers");

    w.h.key_press(Key::G);
    w.h.run();
    assert_eq!(last_command(&w), "view.grid.cycle");

    // Space and L reach the Tool through the Keymap; Esc returns to Select.
    w.h.key_press(Key::Space);
    w.h.run();
    assert_eq!(last_command(&w), "tool.rotate");
    w.h.key_press(Key::Escape);
    w.h.run();
    assert_eq!(last_command(&w), "tool.activate:select");
}

#[test]
fn the_layers_panel_hides_a_layer() {
    let mut w = window();
    w.h.run();
    let silk = {
        let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
        pcb.layers()
            .find(LayerKind::Silkscreen, Some(Side::Top))
            .unwrap()
            .uuid
    };
    w.h.get_by_label("Show Top Silkscreen").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.show");
    assert!(shell(&w).board_view().layers.hidden.contains(&silk));

    w.h.get_by_label("Ratsnest").click();
    w.h.run();
    assert!(shell(&w).board_view().layers.ratsnest.hidden);
}

#[test]
fn a_board_with_no_outline_offers_a_rectangle() {
    let mut w = window();
    w.h.run();
    let has_outline = |w: &Window| {
        let pcb = shell(w).engine().project().unwrap().pcb().unwrap();
        pcb.outline().is_some()
    };
    assert!(!has_outline(&w));

    w.h.get_by_label("Create rectangle…").click();
    w.h.run();
    w.h.get_by_label("Create").click();
    w.h.run();

    assert!(has_outline(&w));
    assert_eq!(last_command(&w), "pcb.board_outline.add_rect");
    assert!(w.h.query_by_label("Create rectangle…").is_none());
}
