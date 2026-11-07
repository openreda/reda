//! The PCB drawing Tools, through the Shell with no window: each previews while it is
//! drawn, touches the Journal only when the shape is finished, and then runs one Command
//! (Board Outline and Cutouts, Zones (Z) and Keepouts (K), text, lines, arcs and
//! rectangles on the active Layer, Free Pads and the Output Origin); the cursor reads
//! from the Output Origin; the measure Tool changes nothing; and the Shell says when
//! Zones need refilling.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Millideg, Nm, Point};
use reda_ui::engine::model::{GraphicShape, LayerKind, Side, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

const TOLERANCE: Nm = Nm::from_um(200);

struct Board {
    shell: Shell,
    _home: TempDir,
    _work: TempDir,
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

/// A new Project with its board open and the grid at 0.5 mm.
fn board() -> Board {
    let home = TempDir::new("pcb-draw-home");
    let work = TempDir::new("pcb-draw-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    run(
        &mut shell,
        "view.grid.set",
        json!({"document": pcb, "grid": "0.5mm"}),
    );
    Board {
        shell,
        _home: home,
        _work: work,
    }
}

impl Board {
    fn tool(&mut self, tool: &str) {
        run(&mut self.shell, &format!("tool.activate:{tool}"), json!({}));
    }

    fn click(&mut self, at: Point) {
        self.shell.pcb_hover(at);
        self.shell.pcb_press(at, false, TOLERANCE);
        self.shell.pcb_release();
    }

    fn drag(&mut self, from: Point, to: Point) {
        self.shell.pcb_press(from, false, TOLERANCE);
        self.shell.pcb_drag(to);
        self.shell.pcb_release();
    }

    /// The Commands run since `from` (an index into the Journal), view ones left out.
    fn edits_since(&self, from: usize) -> Vec<String> {
        let registry = self.shell.engine().registry();
        self.shell.engine().journal()[from..]
            .iter()
            .filter(|e| {
                let base = e.id.split(':').next().unwrap();
                registry.get(base).map(|m| m.kind) != Some(reda_ui::engine::command::Kind::View)
            })
            .map(|e| e.id.clone())
            .collect()
    }

    fn mark(&self) -> usize {
        self.shell.engine().journal().len()
    }

    fn pcb(&self) -> &reda_ui::engine::model::PcbDocument {
        self.shell.engine().project().unwrap().pcb().unwrap()
    }

    fn layer(&self, kind: LayerKind, side: Option<Side>) -> Uuid {
        self.pcb().layers().find(kind, side).unwrap().uuid
    }
}

#[test]
fn z_draws_a_zone_on_the_active_copper_layer_with_a_preview_then_one_command() {
    let mut b = board();
    let bottom = b.layer(LayerKind::Copper, Some(Side::Bottom));
    run(&mut b.shell, "pcb.layer.activate", json!({"layer": bottom}));
    b.tool("zone");
    let start = b.mark();
    b.click(mm(0.0, 0.0));
    b.click(mm(10.0, 0.0));
    b.click(mm(10.0, 8.0));
    b.shell.pcb_hover(mm(0.2, 7.9));

    let preview = b
        .shell
        .pcb_draw()
        .preview()
        .expect("a preview while drawing");
    assert_eq!(
        preview.points,
        vec![mm(0.0, 0.0), mm(10.0, 0.0), mm(10.0, 8.0), mm(0.0, 8.0)],
        "the corners so far, then the snapped cursor"
    );
    assert!(
        b.edits_since(start).is_empty(),
        "nothing runs while drawing"
    );
    assert_eq!(b.pcb().zones().count(), 0);

    b.click(mm(0.0, 8.0));
    b.click(mm(0.1, 0.1)); // back on the first corner closes it
    assert_eq!(b.edits_since(start), ["pcb.zone.add"]);
    let zone = b.pcb().zones().next().unwrap();
    assert_eq!(zone.layer, bottom);
    assert_eq!(zone.vertices.len(), 4);
    assert!(
        b.shell.pcb_draw().preview().is_none(),
        "ready for the next one"
    );
}

#[test]
fn a_zone_is_refused_off_copper_and_k_draws_a_keepout_on_every_copper_layer() {
    let mut b = board();
    let silk = b.layer(LayerKind::Silkscreen, Some(Side::Top));
    run(&mut b.shell, "pcb.layer.activate", json!({"layer": silk}));
    b.tool("zone");
    let start = b.mark();
    b.click(mm(0.0, 0.0));
    assert!(b.shell.message().is_some_and(|m| m.contains("copper")));
    assert!(b.shell.pcb_draw().preview().is_none());

    b.tool("keepout");
    for p in [mm(0.0, 0.0), mm(5.0, 0.0), mm(5.0, 5.0)] {
        b.click(p);
    }
    run(&mut b.shell, "tool.finish", json!({}));
    assert_eq!(b.edits_since(start), ["pcb.keepout.add"]);
    assert_eq!(b.pcb().keepouts().next().unwrap().layers.len(), 2);
}

#[test]
fn the_board_outline_is_drawn_as_a_polygon_and_backspace_takes_back_a_corner() {
    let mut b = board();
    b.tool("board_outline");
    let start = b.mark();
    for p in [mm(0.0, 0.0), mm(40.0, 0.0), mm(40.0, 7.0)] {
        b.click(p);
    }
    run(&mut b.shell, "tool.undo_segment", json!({}));
    b.click(mm(40.0, 30.0));
    b.click(mm(0.0, 30.0));
    b.click(mm(0.0, 30.0)); // the last corner again finishes too
    assert_eq!(b.edits_since(start), ["pcb.board_outline.add"]);
    let vertices: Vec<Point> = b
        .pcb()
        .outline()
        .unwrap()
        .vertices
        .iter()
        .map(|v| Point::new(v.x, v.y))
        .collect();
    assert_eq!(
        vertices,
        [mm(0.0, 0.0), mm(40.0, 0.0), mm(40.0, 30.0), mm(0.0, 30.0)]
    );
}

#[test]
fn a_rectangle_outline_and_a_cutout_are_drawn_by_dragging_or_clicking() {
    let mut b = board();
    b.tool("board_outline_rect");
    let start = b.mark();
    b.drag(mm(0.0, 0.0), mm(30.0, 20.0));
    assert_eq!(b.edits_since(start), ["pcb.board_outline.add"]);
    assert_eq!(b.pcb().outline().unwrap().vertices.len(), 4);

    b.tool("board_cutout");
    let start = b.mark();
    for p in [mm(5.0, 5.0), mm(10.0, 5.0), mm(10.0, 10.0), mm(5.0, 5.0)] {
        b.click(p);
    }
    assert_eq!(b.edits_since(start), ["pcb.board_cutout.add"]);
    assert_eq!(b.pcb().cutouts().count(), 1);
}

#[test]
fn lines_rectangles_and_arcs_go_on_the_active_layer() {
    let mut b = board();
    let silk = b.layer(LayerKind::Silkscreen, Some(Side::Bottom));
    run(&mut b.shell, "pcb.layer.activate", json!({"layer": silk}));
    let start = b.mark();

    b.tool("line");
    b.click(mm(0.0, 0.0));
    b.shell.pcb_hover(mm(5.0, 0.0));
    assert_eq!(
        b.shell.pcb_draw().preview().unwrap().points,
        [mm(0.0, 0.0), mm(5.0, 0.0)]
    );
    b.click(mm(5.0, 0.0));

    b.tool("rect");
    b.click(mm(0.0, 2.0));
    b.click(mm(4.0, 4.0));

    b.tool("arc");
    b.click(mm(0.0, 10.0));
    b.click(mm(10.0, 10.0));
    b.click(mm(5.0, 15.0)); // the arc passes through the top

    assert_eq!(
        b.edits_since(start),
        [
            "pcb.graphic.add_line",
            "pcb.graphic.add_rect",
            "pcb.graphic.add_arc"
        ]
    );
    let shapes: Vec<GraphicShape> = b.pcb().graphics().map(|g| g.shape).collect();
    assert!(b.pcb().graphics().all(|g| g.layer == silk));
    assert!(shapes.contains(&GraphicShape::Line {
        from: mm(0.0, 0.0),
        to: mm(5.0, 0.0)
    }));
    assert!(shapes.contains(&GraphicShape::Arc {
        from: mm(0.0, 10.0),
        to: mm(10.0, 10.0),
        sweep: Millideg::from_deg(-180),
    }));
}

#[test]
fn graphics_are_refused_on_a_layer_that_takes_none() {
    let mut b = board();
    let courtyard = b.layer(LayerKind::Courtyard, Some(Side::Top));
    run(
        &mut b.shell,
        "pcb.layer.activate",
        json!({"layer": courtyard}),
    );
    b.tool("line");
    let start = b.mark();
    b.click(mm(0.0, 0.0));
    b.click(mm(5.0, 0.0));
    assert!(b.edits_since(start).is_empty());
    assert!(b.shell.message().is_some());
}

#[test]
fn text_asks_for_its_words_then_runs_one_command() {
    let mut b = board();
    b.tool("text");
    let start = b.mark();
    b.click(mm(3.0, 4.0));
    assert_eq!(b.shell.pcb_draw().pending_text(), Some(mm(3.0, 4.0)));
    assert!(b.edits_since(start).is_empty());
    b.shell.pcb_draw_text("HELLO");
    assert_eq!(b.edits_since(start), ["pcb.text.add"]);
    let text = b.pcb().texts().next().unwrap();
    assert_eq!(text.text, "HELLO");
    assert_eq!(text.position, mm(3.0, 4.0));
    assert_eq!(b.shell.pcb_draw().pending_text(), None);
}

#[test]
fn free_pads_and_the_output_origin_are_placed_by_a_click() {
    let mut b = board();
    let start = b.mark();
    b.tool("free_pad");
    b.click(mm(2.0, 2.0));
    b.tool("output_origin");
    b.click(mm(1.0, 1.0));
    assert_eq!(
        b.edits_since(start),
        ["pcb.free_pad.add", "pcb.output_origin.set"]
    );
    assert_eq!(b.pcb().free_pads().next().unwrap().at, mm(2.0, 2.0));
    assert_eq!(b.pcb().output_origin().unwrap().at, mm(1.0, 1.0));
}

#[test]
fn the_cursor_reads_from_the_output_origin() {
    let mut b = board();
    assert_eq!(
        b.shell.pcb_cursor_from_origin(mm(12.0, 4.0)),
        mm(12.0, 4.0),
        "without an Output Origin the cursor reads from design zero"
    );
    b.tool("output_origin");
    b.click(mm(10.0, 5.0));
    assert_eq!(
        b.shell.pcb_cursor_from_origin(mm(12.0, 4.0)),
        mm(2.0, -1.0),
        "the cursor reads from the Output Origin"
    );
    run(&mut b.shell, "pcb.output_origin.delete", json!({}));
    assert_eq!(b.shell.pcb_cursor_from_origin(mm(12.0, 4.0)), mm(12.0, 4.0));
}

#[test]
fn measure_changes_nothing() {
    let mut b = board();
    let start = b.mark();
    b.tool("measure");
    b.click(mm(0.0, 0.0));
    b.shell.pcb_hover(mm(3.0, 4.0));
    let m = b.shell.pcb_draw().measurement().unwrap();
    assert_eq!((m.from, m.to), (mm(0.0, 0.0), mm(3.0, 4.0)));
    assert_eq!(m.distance(), Nm::from_mm(5));
    b.click(mm(3.0, 4.0));
    b.shell.pcb_hover(mm(8.0, 8.0));
    let m = b.shell.pcb_draw().measurement().unwrap();
    assert_eq!(m.to, mm(3.0, 4.0), "a second click holds the measurement");
    assert!(b.edits_since(start).is_empty());
}

#[test]
fn escape_drops_a_shape_in_progress() {
    let mut b = board();
    b.tool("zone");
    b.click(mm(0.0, 0.0));
    b.click(mm(5.0, 0.0));
    run(&mut b.shell, "tool.activate:select", json!({}));
    assert!(b.shell.pcb_draw().preview().is_none());
}

#[test]
fn the_shell_says_when_zones_need_refilling_until_they_are_refilled() {
    let mut b = board();
    assert!(!b.shell.zones_need_refill());
    run(
        &mut b.shell,
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "20mm"}),
    );
    b.tool("zone");
    for p in [mm(-5.0, -5.0), mm(5.0, -5.0), mm(5.0, 5.0)] {
        b.click(p);
    }
    run(&mut b.shell, "tool.finish", json!({}));
    assert!(b.shell.zones_need_refill());
    run(&mut b.shell, "pcb.zone.fill", json!({}));
    assert!(!b.shell.zones_need_refill());
}

#[test]
fn dragging_a_footprint_preserves_the_filtered_free_pad_selection_and_position() {
    let mut b = board();
    let footprint: Uuid = run(
        &mut b.shell,
        "test.footprint.add",
        json!({"at":{"x":0,"y":0},"pads":[{"at":{"x":0,"y":0},"width":"2mm","height":"2mm"}]}),
    )["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let pad: Uuid = run(
        &mut b.shell,
        "pcb.free_pad.add",
        json!({"kind":"smd","at":{"x":"5mm","y":0},"width":"1mm"}),
    )["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let locked: Uuid = run(
        &mut b.shell,
        "test.footprint.add",
        json!({"at":{"x":"10mm","y":0},"pads":[{"at":{"x":0,"y":0},"width":"2mm","height":"2mm"}]}),
    )["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    run(
        &mut b.shell,
        "pcb.lock",
        json!({"footprints":[locked],"locked":true}),
    );
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key":"editing.protect_locked_objects","value":true}),
    );
    run(
        &mut b.shell,
        "selection.set",
        json!({"objects":[footprint,pad,locked]}),
    );
    let pcb = b.pcb().uuid;
    run(
        &mut b.shell,
        "selection.filter.set",
        json!({"document":pcb,"kinds":{"free_pad":false}}),
    );
    b.drag(mm(0.0, 0.0), mm(2.0, 0.0));
    assert_eq!(b.shell.selection().objects, [footprint, pad, locked]);
    assert_eq!(b.pcb().footprint(locked).unwrap().at, mm(10.0, 0.0));
    assert_eq!(b.pcb().footprint(footprint).unwrap().at, mm(2.0, 0.0));
    assert_eq!(b.pcb().free_pad(pad).unwrap().at, mm(5.0, 0.0));
}

#[test]
fn output_origin_phases_the_board_grid_for_hover_drawing_and_electrical_snap() {
    use reda_ui::pcb::snap::SnapKind;
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.output_origin.set",
        json!({"at":{"x":"0.2mm","y":"0.3mm"}}),
    );
    assert_eq!(
        b.shell.pcb_hover(mm(1.1, 1.1)),
        (mm(1.2, 1.3), SnapKind::Grid)
    );
    b.tool("free_pad");
    b.click(mm(2.1, 2.1));
    let pad = b.pcb().free_pads().next().unwrap().clone();
    assert_eq!(pad.at, mm(2.2, 2.3));
    b.tool("select");
    assert_eq!(
        b.shell.pcb_hover(mm(2.21, 2.31)),
        (pad.at, SnapKind::Pad),
        "electrical targets still win"
    );
    run(
        &mut b.shell,
        "pcb.output_origin.set",
        json!({"at":{"x":"0.1mm","y":"0.1mm"}}),
    );
    assert_eq!(
        b.pcb().free_pad(pad.uuid).unwrap().at,
        pad.at,
        "moving the grid moves no objects"
    );
    assert_eq!(
        b.shell.pcb_hover(mm(4.0, 4.0)),
        (mm(4.1, 4.1), SnapKind::Grid)
    );
    run(&mut b.shell, "pcb.output_origin.delete", json!({}));
    assert_eq!(
        b.shell.pcb_hover(mm(4.0, 4.0)),
        (mm(4.0, 4.0), SnapKind::Grid)
    );
}
