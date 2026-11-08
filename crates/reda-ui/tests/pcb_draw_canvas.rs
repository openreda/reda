//! The PCB drawing Tools in the window, driven with `egui_kittest`: Z starts the Zone
//! Tool, clicks on the canvas draw it and close it into one `pcb.zone.add`, the status
//! bar then says Zones need refilling and refills them, and the text Tool asks for its
//! words in a form.

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

/// The window on a board with a 40 × 30 mm outline, its PCB tab active.
fn window() -> Window {
    let home = TempDir::new("pcb-draw-canvas-home");
    let work = TempDir::new("pcb-draw-canvas-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    shell
        .run(
            "pcb.board_outline.add_rect",
            json!({"width": "40mm", "height": "30mm"}),
        )
        .unwrap();
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

fn last_command(w: &Window) -> String {
    shell(w).engine().journal().last().unwrap().id.clone()
}

fn on_screen(w: &Window, x_mm: i64, y_mm: i64) -> Pos2 {
    let rect = w.h.get_by_role(Role::Canvas).rect();
    let camera = shell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    let p = Point::new(Nm::from_mm(x_mm), Nm::from_mm(y_mm));
    camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64))
}

fn click(w: &mut Window, at: Pos2) {
    w.h.hover_at(at);
    w.h.run();
    w.h.drag_at(at);
    w.h.drop_at(at);
    w.h.run();
}

/// Focuses the board with a press on its canvas.
fn focus(w: &mut Window) {
    w.h.run();
    let at = on_screen(w, 20, 15);
    click(w, at);
}

#[test]
fn z_then_clicks_draw_a_zone_and_the_status_bar_offers_a_refill() {
    let mut w = window();
    // Automatic refill off, so the manual Refill shows.
    w.h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "pcb.zone.auto_refill", "value": "off"}),
        )
        .unwrap();
    focus(&mut w);
    w.h.key_press(Key::Z);
    w.h.run();
    assert_eq!(last_command(&w), "tool.activate:zone");

    for (x, y) in [(5, 5), (25, 5), (25, 25)] {
        let at = on_screen(&w, x, y);
        click(&mut w, at);
    }
    assert_ne!(
        last_command(&w),
        "pcb.zone.add",
        "nothing runs while drawing"
    );
    let first = on_screen(&w, 5, 5);
    click(&mut w, first);
    assert_eq!(last_command(&w), "pcb.zone.add");

    w.h.run();
    assert!(w.h.query_by_label("Zones need refilling").is_some());
    w.h.get_by_label("Refill").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.zone.fill");
    w.h.run();
    assert!(w.h.query_by_label("Zones need refilling").is_none());
}

#[test]
fn the_text_tool_asks_for_its_words() {
    let mut w = window();
    focus(&mut w);
    w.h.state_mut()
        .shell_mut()
        .run("tool.activate:text", json!({}))
        .unwrap();
    w.h.run();
    let at = on_screen(&w, 10, 10);
    click(&mut w, at);
    w.h.get_by_role(Role::TextInput).type_text("REV A");
    w.h.run();
    w.h.get_by_label("Place Text").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.text.add");
    let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.texts().next().unwrap().text, "REV A");
}
