//! Object Defaults on the board (docs/spec/v0.3/input-and-defaults.md#object-defaults):
//! each placing Tool starts from its kind's record, shows it as Tool options in
//! Properties (Tab moves the keyboard there), and with Remember last used on an edit
//! there is written back to `defaults.*`; off, it lasts until the Tool is left.

use std::path::{Path, PathBuf};

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{
    ArrowPosition, DimensionFormat, GraphicShape, LayerKind, PadKind, PadShape, Side, Uuid,
};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const TOLERANCE: Nm = Nm::from_um(200);

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

/// A Shell on a new board, its tab active and Properties open.
fn board(home: &TempDir, work: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    for (id, params) in [
        ("document.open", json!({"document": pcb})),
        ("layout.tab.activate", json!({"tab": pcb.to_string()})),
        ("panel.show", json!({"panel": "properties"})),
        ("view.grid.set", json!({"document": pcb, "grid": "0.5mm"})),
        (
            "app.preferences.set",
            json!({"key": "pcb.zone.auto_refill", "value": "off"}),
        ),
    ] {
        shell.run(id, params).unwrap();
    }
    shell
}

fn activate(shell: &mut Shell, kind: LayerKind, side: Option<Side>) -> Uuid {
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    let layer = pcb.layers().find(kind, side).unwrap().uuid;
    shell
        .run("pcb.layer.activate", json!({"layer": layer}))
        .unwrap();
    layer
}

fn set(shell: &mut Shell, key: &str, value: Value) {
    shell
        .run("app.preferences.set", json!({"key": key, "value": value}))
        .unwrap();
}

fn click(shell: &mut Shell, at: Point) {
    shell.pcb_hover(at);
    shell.pcb_press(at, false, TOLERANCE);
    shell.pcb_release();
}

fn tool(shell: &mut Shell, tool: &str) {
    shell
        .run(&format!("tool.activate:{tool}"), json!({}))
        .unwrap();
}

fn pcb(shell: &Shell) -> &reda_ui::engine::model::PcbDocument {
    shell.engine().project().unwrap().pcb().unwrap()
}

/// The pen width of the newest graphic.
fn graphic_widths(shell: &Shell) -> Vec<(GraphicShape, Nm)> {
    pcb(shell).graphics().map(|g| (g.shape, g.width)).collect()
}

fn line_width(shell: &Shell, from: Point) -> Nm {
    graphic_widths(shell)
        .into_iter()
        .find_map(|(shape, width)| match shape {
            GraphicShape::Line { from: f, .. } if f == from => Some(width),
            _ => None,
        })
        .expect("a line from there")
}

fn prefs_text(home: &TempDir) -> String {
    std::fs::read_to_string(UserDirs::under(home.path()).preferences_file()).unwrap_or_default()
}

struct Window {
    h: Harness<'static, App>,
    home: TempDir,
    _work: TempDir,
}

fn window() -> Window {
    let home = TempDir::new("object-defaults-home");
    let work = TempDir::new("object-defaults-work");
    let mut shell = board(&home, &work);
    activate(&mut shell, LayerKind::Silkscreen, Some(Side::Top));
    let h = Harness::builder()
        .with_step_dt(0.25)
        .with_max_steps(100)
        .with_size([1200.0, 1000.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Window {
        h,
        home,
        _work: work,
    }
}

impl Window {
    fn shell(&self) -> &Shell {
        self.h.state().shell()
    }

    fn shell_mut(&mut self) -> &mut Shell {
        self.h.state_mut().shell_mut()
    }

    fn type_into(&mut self, label: &str, text: &str) {
        self.h.get_by_role_and_label(Role::TextInput, label).click();
        self.h.run();
        self.h
            .key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        self.h.run();
        self.h
            .get_by_role_and_label(Role::TextInput, label)
            .type_text(text);
        self.h.run();
        self.h.key_press(Key::Enter);
        self.h.run();
    }

    fn last_entry(&self) -> (String, Value) {
        let entry = self
            .shell()
            .engine()
            .journal()
            .iter()
            .rfind(|e| e.outcome.is_ok())
            .unwrap();
        (entry.id.clone(), entry.params.clone())
    }

    fn draw_line(&mut self, from: Point, to: Point) {
        click(self.shell_mut(), from);
        click(self.shell_mut(), to);
    }
}

#[test]
fn with_remember_last_used_on_a_tool_option_edit_writes_the_record() {
    let mut w = window();
    tool(w.shell_mut(), "line");
    w.h.run();
    w.h.get_by_label("Line Tool options");
    w.type_into("Width", "0.3");
    assert_eq!(
        w.last_entry(),
        (
            "app.preferences.set".to_owned(),
            json!({"key": "defaults.pcb.line.silkscreen.width", "value": "0.3mm"})
        )
    );
    assert!(
        prefs_text(&w.home).contains("[defaults.pcb.line.silkscreen]"),
        "{}",
        prefs_text(&w.home)
    );
    // The next placement and the next session start from it.
    w.draw_line(mm(0.0, 0.0), mm(5.0, 0.0));
    assert_eq!(line_width(w.shell(), mm(0.0, 0.0)), Nm::from_um(300));
    tool(w.shell_mut(), "select");
    tool(w.shell_mut(), "line");
    w.draw_line(mm(0.0, 2.0), mm(5.0, 2.0));
    assert_eq!(line_width(w.shell(), mm(0.0, 2.0)), Nm::from_um(300));
    let stored = reda_ui::engine::preferences::load(&UserDirs::under(w.home.path())).values;
    assert_eq!(
        stored.defaults.pcb.line.silkscreen.width.nm(),
        Nm::from_um(300)
    );
}

#[test]
fn with_remember_last_used_off_a_tool_option_edit_lasts_until_the_tool_is_left() {
    let mut w = window();
    set(w.shell_mut(), "defaults.remember_last_used", json!(false));
    let before = prefs_text(&w.home);
    tool(w.shell_mut(), "line");
    w.h.run();
    w.type_into("Width", "0.3");
    assert_eq!(
        w.last_entry(),
        (
            "tool.option.set".to_owned(),
            json!({"key": "defaults.pcb.line.silkscreen.width", "value": "0.3mm"})
        )
    );
    assert_eq!(prefs_text(&w.home), before, "the record is not written");
    // The Tool places with it while it runs...
    w.draw_line(mm(0.0, 0.0), mm(5.0, 0.0));
    w.draw_line(mm(0.0, 2.0), mm(5.0, 2.0));
    assert_eq!(line_width(w.shell(), mm(0.0, 0.0)), Nm::from_um(300));
    assert_eq!(line_width(w.shell(), mm(0.0, 2.0)), Nm::from_um(300));
    // ...and once it is left, it starts from the record again.
    tool(w.shell_mut(), "select");
    tool(w.shell_mut(), "line");
    w.h.run();
    w.draw_line(mm(0.0, 4.0), mm(5.0, 4.0));
    assert_eq!(line_width(w.shell(), mm(0.0, 4.0)), Nm::from_um(150));
    assert_eq!(
        w.shell()
            .preferences()
            .defaults
            .pcb
            .line
            .silkscreen
            .width
            .nm(),
        Nm::from_um(150)
    );
}

#[test]
fn tab_shows_the_tool_options_with_the_keyboard_in_the_first() {
    let mut w = window();
    w.h.run();
    // Focus the canvas, then start the Text Tool.
    let canvas = w.h.get_by_role(Role::Canvas).rect().center();
    w.h.hover_at(canvas);
    w.h.run();
    w.h.drag_at(canvas);
    w.h.drop_at(canvas);
    w.h.run();
    tool(w.shell_mut(), "text");
    w.h.run();
    w.h.key_press(Key::Tab);
    w.h.run();
    w.h.run();
    assert!(
        w.h.get_by_role_and_label(Role::TextInput, "Height")
            .is_focused(),
        "the keyboard is in the first Tool option"
    );
}

#[test]
fn a_records_layer_kind_follows_the_active_layer() {
    let home = TempDir::new("object-defaults-kind-home");
    let work = TempDir::new("object-defaults-kind-work");
    let mut shell = board(&home, &work);
    set(
        &mut shell,
        "defaults.pcb.line.copper.width",
        json!("0.25mm"),
    );
    set(
        &mut shell,
        "defaults.pcb.line.mechanical.width",
        json!("0.05mm"),
    );
    tool(&mut shell, "line");
    activate(&mut shell, LayerKind::Copper, Some(Side::Top));
    click(&mut shell, mm(0.0, 0.0));
    click(&mut shell, mm(5.0, 0.0));
    activate(&mut shell, LayerKind::Silkscreen, Some(Side::Bottom));
    click(&mut shell, mm(0.0, 2.0));
    click(&mut shell, mm(5.0, 2.0));
    activate(&mut shell, LayerKind::Mechanical, None);
    click(&mut shell, mm(0.0, 4.0));
    click(&mut shell, mm(5.0, 4.0));
    assert_eq!(line_width(&shell, mm(0.0, 0.0)), Nm::from_um(250));
    assert_eq!(line_width(&shell, mm(0.0, 2.0)), Nm::from_um(150));
    assert_eq!(line_width(&shell, mm(0.0, 4.0)), Nm::from_um(50));
}

#[test]
fn the_board_tools_place_from_their_records() {
    let home = TempDir::new("object-defaults-board-home");
    let work = TempDir::new("object-defaults-board-work");
    let mut shell = board(&home, &work);
    activate(&mut shell, LayerKind::Silkscreen, Some(Side::Top));
    for (key, value) in [
        ("defaults.pcb.rectangle.silkscreen.width", json!("0.2mm")),
        ("defaults.pcb.rectangle.silkscreen.fill", json!(true)),
        ("defaults.pcb.arc.silkscreen.width", json!("0.12mm")),
        ("defaults.pcb.text.silkscreen.height", json!("1.5mm")),
        ("defaults.pcb.text.silkscreen.thickness", json!("0.2mm")),
        ("defaults.pcb.text.silkscreen.font", json!("Noto Sans")),
        ("defaults.pcb.text.justify.h", json!("right")),
        ("defaults.pcb.dimension.precision", json!(3)),
        ("defaults.pcb.dimension.arrow_position", json!("outside")),
        ("defaults.pcb.dimension.format", json!("value")),
        ("defaults.pcb.dimension.text_height", json!("0.8mm")),
        ("defaults.pcb.dimension.width", json!("0.1mm")),
        ("defaults.pcb.free_pad.kind", json!("smd")),
        ("defaults.pcb.free_pad.shape", json!("rect")),
        ("defaults.pcb.free_pad.width", json!("1.2mm")),
        ("defaults.pcb.free_pad.height", json!("0.6mm")),
        ("defaults.pcb.via.teardrop", json!(false)),
    ] {
        set(&mut shell, key, value);
    }

    tool(&mut shell, "rect");
    click(&mut shell, mm(0.0, 0.0));
    click(&mut shell, mm(4.0, 3.0));
    let (shape, width) = graphic_widths(&shell)
        .into_iter()
        .find(|(s, _)| matches!(s, GraphicShape::Rect { .. }))
        .unwrap();
    assert_eq!(width, Nm::from_um(200));
    assert!(matches!(shape, GraphicShape::Rect { filled: true, .. }));

    tool(&mut shell, "arc");
    for p in [mm(10.0, 0.0), mm(14.0, 0.0), mm(12.0, 2.0)] {
        click(&mut shell, p);
    }
    let (_, width) = graphic_widths(&shell)
        .into_iter()
        .find(|(s, _)| matches!(s, GraphicShape::Arc { .. }))
        .unwrap();
    assert_eq!(width, Nm::from_um(120));

    tool(&mut shell, "text");
    click(&mut shell, mm(20.0, 0.0));
    shell.pcb_draw_text("REV A");
    let text = pcb(&shell).texts().next().unwrap().clone();
    assert_eq!(text.style.height, Nm::from_um(1500));
    assert_eq!(text.style.thickness, Nm::from_um(200));
    assert_eq!(text.style.font, "Noto Sans");
    assert_eq!(
        text.style.justify.h,
        reda_ui::engine::model::TextHJustify::Right
    );

    tool(&mut shell, "dimension");
    for p in [mm(0.0, 10.0), mm(10.0, 10.0), mm(5.0, 12.0)] {
        click(&mut shell, p);
    }
    let d = pcb(&shell).dimensions().next().unwrap().clone();
    assert_eq!(d.precision, 3);
    assert_eq!(d.arrow_position, ArrowPosition::Outside);
    assert_eq!(d.format, DimensionFormat::Value);
    assert_eq!(d.style.height, Nm::from_um(800));
    assert_eq!(d.width, Nm::from_um(100));
    assert_eq!(d.offset, Nm::from_mm(2), "the third click sets the offset");

    tool(&mut shell, "free_pad");
    click(&mut shell, mm(30.0, 0.0));
    let pad = pcb(&shell).free_pads().next().unwrap().clone();
    assert_eq!((pad.kind, pad.shape), (PadKind::Smd, PadShape::Rect));
    assert_eq!(
        (pad.width, pad.height),
        (Nm::from_um(1200), Nm::from_um(600))
    );

    tool(&mut shell, "via");
    shell.route_hover(mm(30.0, 20.0));
    shell.route_click(mm(30.0, 20.0));
    let via = pcb(&shell).vias().next().expect("a Via").clone();
    assert!(!via.teardrop);
}

#[test]
fn commands_never_read_a_record() {
    let home = TempDir::new("object-defaults-commands-home");
    let work = TempDir::new("object-defaults-commands-work");
    let mut shell = board(&home, &work);
    set(
        &mut shell,
        "defaults.pcb.line.silkscreen.width",
        json!("0.5mm"),
    );
    set(
        &mut shell,
        "defaults.pcb.text.silkscreen.height",
        json!("3mm"),
    );
    set(&mut shell, "defaults.pcb.via.teardrop", json!(false));
    shell
        .run(
            "pcb.graphic.add_line",
            json!({"layer": "silkscreen_top", "from": {"x": 0, "y": 0},
                   "to": {"x": "5mm", "y": 0}}),
        )
        .unwrap();
    shell
        .run(
            "pcb.text.add",
            json!({"layer": "silkscreen_top", "text": "A", "x": 0, "y": "5mm"}),
        )
        .unwrap();
    shell
        .run("pcb.via.add", json!({"at": {"x": "10mm", "y": "10mm"}}))
        .unwrap();
    assert_eq!(line_width(&shell, mm(0.0, 0.0)), Nm::from_um(150));
    assert_eq!(
        pcb(&shell).texts().next().unwrap().style.height,
        Nm::from_mm(1)
    );
    assert!(pcb(&shell).vias().next().unwrap().teardrop);
}
