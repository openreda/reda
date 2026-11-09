//! Zones, Keepouts and the polygon editor in the window, driven through AccessKit with
//! `egui_kittest`: the Tools' options and a Zone's areas in Properties, Detach, the Place
//! and Design menu items, the gap retry, and a corner handle dragged with the mouse.

use std::path::{Path, PathBuf};

use egui::Pos2;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// The window on a new board, its PCB tab focused, Properties open and automatic refill
/// off.
fn window() -> Window {
    let home = TempDir::new("zone-tools-home");
    let work = TempDir::new("zone-tools-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    for (id, params) in [
        ("document.open", json!({"document": pcb})),
        ("layout.tab.activate", json!({"tab": pcb.to_string()})),
        ("panel.show", json!({"panel": "properties"})),
        (
            "app.preferences.set",
            json!({"key": "pcb.zone.auto_refill", "value": "off"}),
        ),
    ] {
        shell.run(id, params).unwrap();
    }
    let h = Harness::builder()
        .with_step_dt(0.25)
        .with_max_steps(100)
        .with_size([1200.0, 1200.0])
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

fn run(w: &mut Window, id: &str, params: Value) -> Value {
    w.h.state_mut().shell_mut().run(id, params).unwrap()
}

fn last_entry(w: &Window) -> (String, Value) {
    let entry = shell(w)
        .engine()
        .journal()
        .iter()
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

/// Where the board point `p` shows in the window.
fn on_screen(w: &Window, p: Point) -> Pos2 {
    let rect = w.h.get_by_role(Role::Canvas).rect();
    let camera = shell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64))
}

/// A 10 × 10 mm Zone on the top copper from the origin.
fn zone(w: &mut Window) -> Uuid {
    let out = run(
        w,
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": [
            {"x": "0mm", "y": "0mm"}, {"x": "10mm", "y": "0mm"},
            {"x": "10mm", "y": "10mm"}, {"x": "0mm", "y": "10mm"}]}),
    );
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

#[test]
fn the_zone_tool_options_in_properties_set_its_preference() {
    let mut w = window();
    run(&mut w, "tool.activate:zone", json!({}));
    w.h.run();
    w.h.get_by_label("Zone Tool options");
    w.h.get_by_role_and_label(Role::ComboBox, "Pour Over")
        .click();
    w.h.run();
    w.h.get_by_label("Same-Net Zones and Tracks").click();
    w.h.run();
    assert_eq!(
        last_entry(&w),
        (
            "app.preferences.set".to_owned(),
            json!({"key": "pcb.zone.tool.pour_over", "value": "all"})
        )
    );
    assert_eq!(
        shell(&w).preferences().pcb.zone.tool.pour_over,
        reda_ui::engine::model::PourOver::All
    );
    w.h.get_by_role_and_label(Role::CheckBox, "Remove Unconnected Islands")
        .click();
    w.h.run();
    assert!(!shell(&w).preferences().pcb.zone.tool.remove_unconnected);
}

#[test]
fn the_keepout_tool_options_pick_its_shape_and_what_it_keeps_out() {
    let mut w = window();
    run(&mut w, "tool.activate:keepout", json!({}));
    w.h.run();
    w.h.get_by_role_and_label(Role::ComboBox, "Shape").click();
    w.h.run();
    w.h.get_by_label("Circle").click();
    w.h.run();
    w.h.get_by_role_and_label(Role::CheckBox, "Footprints")
        .click();
    w.h.run();
    let prefs = &shell(&w).preferences().pcb.keepout;
    assert_eq!(
        prefs.shape,
        reda_ui::engine::preferences::KeepoutToolShape::Circle
    );
    assert!(
        prefs
            .restrict
            .contains(&reda_ui::engine::model::Restriction::Footprints)
    );
}

#[test]
fn a_selected_zone_shows_its_areas_and_one_from_the_outline_detaches() {
    let mut w = window();
    let z = zone(&mut w);
    run(&mut w, "selection.set", json!({"objects": [z]}));
    w.h.run();
    w.h.get_by_label("Outline Area");
    w.h.get_by_label("Filled Area");
    w.h.get_by_label("100 mm²");

    run(
        &mut w,
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "10mm"}),
    );
    let out = run(
        &mut w,
        "pcb.zone.add_from_board_outline",
        json!({"layers": ["copper_top"]}),
    );
    let follows = out["result"]["zones"][0].clone();
    run(&mut w, "selection.set", json!({"objects": [follows]}));
    w.h.run();
    w.h.get_by_label("Detach from Board Outline").click();
    w.h.run();
    assert_eq!(
        last_entry(&w),
        (
            "pcb.zone.detach_outline".to_owned(),
            json!({"zone": follows})
        )
    );
}

#[test]
fn place_and_design_menus_add_from_the_outline_and_move_the_board() {
    let mut w = window();
    run(
        &mut w,
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "10mm"}),
    );
    w.h.run();
    w.h.get_by_label("Place").click();
    w.h.run();
    w.h.get_by_label("Zone from Board Outline (on the Active Layer)")
        .click();
    w.h.run();
    let (id, params) = last_entry(&w);
    assert_eq!(id, "pcb.zone.add_from_board_outline");
    let top = shell(&w)
        .board_view()
        .active_layer(shell(&w).engine().project().unwrap())
        .unwrap();
    assert_eq!(params, json!({"layers": [top]}), "no Net is highlighted");

    w.h.get_by_label("Place").click();
    w.h.run();
    w.h.get_by_label("Zone Cutout").click();
    w.h.run();
    assert_eq!(
        shell(&w).tool(),
        reda_ui::tools::Tool::ZoneCutout,
        "the Zone Cutout Tool is on the Place menu"
    );
    run(&mut w, "tool.activate:select", json!({}));

    w.h.get_by_label("Design").click();
    w.h.run();
    w.h.get_by_label("Move Board").click();
    w.h.run();
    assert!(shell(&w).pcb_tool().moving(), "the board is picked up");
}

#[test]
fn a_gap_asks_to_retry_along_the_outer_edge() {
    let mut w = window();
    let mut lines = Vec::new();
    for (from, to) in [
        (("0mm", "0mm"), ("10mm", "0mm")),
        (("10mm", "0mm"), ("5mm", "8mm")),
        (("5mm", "8mm"), ("0.03mm", "0.04mm")),
    ] {
        let out = run(
            &mut w,
            "pcb.graphic.add_line",
            json!({"layer": "copper_top", "width": "0.2mm",
                   "from": {"x": from.0, "y": from.1}, "to": {"x": to.0, "y": to.1}}),
        );
        lines.push(out["result"]["uuid"].clone());
    }
    let refused = w.h.state_mut().shell_mut().run(
        "pcb.keepout.add_from_graphics",
        json!({ "graphics": lines }),
    );
    assert!(refused.is_err());
    w.h.run();
    w.h.get_by_label("Retry Along the Outer Edge").click();
    w.h.run();
    assert_eq!(last_entry(&w).0, "pcb.keepout.add_from_graphics");
    let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.keepouts().count(), 1);
}

#[test]
fn a_corner_handle_dragged_with_the_mouse_reshapes_the_zone() {
    let mut w = window();
    let z = zone(&mut w);
    run(&mut w, "selection.set", json!({"objects": [z]}));
    w.h.run();
    let from = on_screen(&w, mm(10.0, 10.0));
    let to = on_screen(&w, mm(12.0, 11.0));
    w.h.drag_at(from);
    w.h.run();
    w.h.hover_at(Pos2::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0));
    w.h.run();
    w.h.drop_at(to);
    w.h.run();
    assert_eq!(last_entry(&w).0, "pcb.polygon.set_vertices");
    let pcb = shell(&w).engine().project().unwrap().pcb().unwrap();
    let corner = pcb.zone(z).unwrap().vertices[2].point();
    let near = |a: Nm, b: Nm| (a.0 - b.0).abs() <= 500_000;
    assert!(
        near(corner.x, mm(12.0, 0.0).x) && near(corner.y, mm(0.0, 11.0).y),
        "the corner went to about (12, 11) mm: {corner:?}"
    );
}
