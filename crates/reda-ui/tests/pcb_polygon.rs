//! The polygon editor and the Zone, Zone Cutout and Keepout Tools, through the Shell with
//! no window: a selected polygon's corner and edge-midpoint handles reshape it by one
//! `pcb.polygon.set_vertices` per drag, a self-crossing drag goes back on release, the
//! Tools pre-fill from their Preferences, and the board moves whole.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Millideg, Nm, Point, Vertex};
use reda_ui::engine::model::{LayerKind, Side, Uuid};
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

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// A new Project with its board open, the grid at 0.5 mm and automatic refill off.
fn board() -> Board {
    let home = TempDir::new("pcb-polygon-home");
    let work = TempDir::new("pcb-polygon-work");
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
    run(
        &mut shell,
        "app.preferences.set",
        json!({"key": "pcb.zone.auto_refill", "value": "off"}),
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

    fn press(&mut self, at: Point) {
        self.shell.pcb_hover(at);
        self.shell.pcb_press(at, false, TOLERANCE);
    }

    fn drag(&mut self, from: Point, to: Point) {
        self.press(from);
        self.shell.pcb_drag(to);
        self.shell.pcb_release();
    }

    /// The Commands run since `from` (an index into the Journal), view ones left out.
    fn edits_since(&self, from: usize) -> Vec<String> {
        let registry = self.shell.engine().registry();
        self.shell.engine().journal()[from..]
            .iter()
            .filter(|e| e.outcome.is_ok())
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

    /// A selected 10 × 10 mm Zone on the top copper, corners from the origin.
    fn square_zone(&mut self) -> Uuid {
        let out = run(
            &mut self.shell,
            "pcb.zone.add",
            json!({"layer": "copper_top", "vertices": [
                {"x": "0mm", "y": "0mm"}, {"x": "10mm", "y": "0mm"},
                {"x": "10mm", "y": "10mm"}, {"x": "0mm", "y": "10mm"}]}),
        );
        let zone = uuid(&out["result"]["uuid"]);
        run(&mut self.shell, "selection.set", json!({"objects": [zone]}));
        zone
    }

    fn zone_points(&self, zone: Uuid) -> Vec<Point> {
        let z = self.pcb().zone(zone).unwrap();
        self.pcb()
            .zone_vertices(z)
            .iter()
            .map(Vertex::point)
            .collect()
    }
}

#[test]
fn dragging_a_corner_handle_moves_that_vertex_as_one_command() {
    let mut b = board();
    let zone = b.square_zone();
    let start = b.mark();
    b.press(mm(10.0, 10.0));
    b.shell.pcb_drag(mm(12.0, 11.0));
    assert!(b.edits_since(start).is_empty(), "nothing runs in flight");
    b.shell.pcb_drag(mm(13.0, 12.0));
    b.shell.pcb_release();
    assert_eq!(b.edits_since(start), ["pcb.polygon.set_vertices"]);
    assert_eq!(
        b.zone_points(zone),
        [mm(0.0, 0.0), mm(10.0, 0.0), mm(13.0, 12.0), mm(0.0, 10.0)]
    );
}

#[test]
fn each_drag_is_one_undo_step() {
    let mut b = board();
    let zone = b.square_zone();
    b.drag(mm(10.0, 10.0), mm(13.0, 12.0));
    let pcb = b.pcb().uuid;
    run(&mut b.shell, "edit.undo", json!({"document": pcb}));
    assert_eq!(
        b.zone_points(zone),
        [mm(0.0, 0.0), mm(10.0, 0.0), mm(10.0, 10.0), mm(0.0, 10.0)]
    );
}

#[test]
fn a_vertex_drag_snaps_to_an_alignment_guide_through_another_vertex() {
    let mut b = board();
    let out = run(
        &mut b.shell,
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": [
            {"x": "0mm", "y": "0mm"}, {"x": "10mm", "y": "0mm"},
            {"x": "10mm", "y": "10mm"}, {"x": "3.3mm", "y": "10mm"}]}),
    );
    let zone = uuid(&out["result"]["uuid"]);
    run(&mut b.shell, "selection.set", json!({"objects": [zone]}));
    b.press(mm(10.0, 10.0));
    // 3.35 mm is off the 0.5 mm grid but within the snap distance of x = 3.3 mm.
    b.shell.pcb_drag(mm(3.35, 14.1));
    let edit = b
        .shell
        .pcb_tool()
        .polygon_edit()
        .expect("a reshape in flight");
    assert_eq!(
        edit.guide_lines(),
        [reda_ui::snap::Guide::X(Nm::from_um(3300))]
    );
    assert_eq!(
        b.shell.pcb_tool().guide_lines(),
        edit.guide_lines(),
        "drawn"
    );
    b.shell.pcb_release();
    assert_eq!(
        b.zone_points(zone),
        [mm(0.0, 0.0), mm(10.0, 0.0), mm(3.3, 14.0), mm(3.3, 10.0)],
        "x on the guide, y on the grid"
    );
}

#[test]
fn space_cycles_a_corner_handle_through_chamfer_and_fillet() {
    let mut b = board();
    let zone = b.square_zone();
    b.press(mm(10.0, 10.0));
    b.shell.pcb_drag(mm(9.0, 9.0));
    run(&mut b.shell, "tool.rotate", json!({}));
    // The bevel's middle follows the cursor: 2 mm back along each edge.
    b.shell.pcb_release();
    assert_eq!(
        b.zone_points(zone),
        [
            mm(0.0, 0.0),
            mm(10.0, 0.0),
            mm(10.0, 8.0),
            mm(8.0, 10.0),
            mm(0.0, 10.0)
        ]
    );

    let zone = b.square_zone();
    b.press(mm(10.0, 10.0));
    b.shell.pcb_drag(mm(9.0, 9.0));
    run(&mut b.shell, "tool.rotate", json!({}));
    run(&mut b.shell, "tool.rotate", json!({}));
    b.shell.pcb_release();
    let z = b.pcb().zone(zone).unwrap();
    assert_eq!(z.vertices.len(), 5);
    assert!(
        z.vertices
            .iter()
            .any(|v| v.sweep.0.abs() == Millideg::from_deg(90).0),
        "a quarter arc rounds the corner: {:?}",
        z.vertices
    );
}

#[test]
fn edge_handles_move_split_and_bow_and_an_edge_slides() {
    let mut b = board();
    // The middle of the right edge moves it, the adjacent edges keeping their angles.
    let zone = b.square_zone();
    b.drag(mm(10.0, 5.0), mm(12.0, 5.0));
    assert_eq!(
        b.zone_points(zone),
        [mm(0.0, 0.0), mm(12.0, 0.0), mm(12.0, 10.0), mm(0.0, 10.0)]
    );
    // Dragging the top edge away from its handle slides it.
    let zone = b.square_zone();
    b.drag(mm(3.0, 10.0), mm(3.0, 12.0));
    assert_eq!(
        b.zone_points(zone),
        [mm(0.0, 0.0), mm(10.0, 0.0), mm(10.0, 12.0), mm(0.0, 12.0)]
    );
    // Space once: split.
    let zone = b.square_zone();
    b.press(mm(5.0, 0.0));
    b.shell.pcb_drag(mm(5.0, -2.0));
    run(&mut b.shell, "tool.rotate", json!({}));
    b.shell.pcb_release();
    assert_eq!(
        b.zone_points(zone),
        [
            mm(0.0, 0.0),
            mm(5.0, -2.0),
            mm(10.0, 0.0),
            mm(10.0, 10.0),
            mm(0.0, 10.0)
        ]
    );
    // Space twice: bow into an arc through the cursor.
    let zone = b.square_zone();
    b.press(mm(5.0, 0.0));
    b.shell.pcb_drag(mm(5.0, -2.0));
    run(&mut b.shell, "tool.rotate", json!({}));
    run(&mut b.shell, "tool.rotate", json!({}));
    b.shell.pcb_release();
    let z = b.pcb().zone(zone).unwrap();
    assert_eq!(z.vertices.len(), 4);
    assert_ne!(
        z.vertices[0].sweep,
        Millideg::ZERO,
        "the bottom edge is an arc"
    );
}

#[test]
fn a_self_crossing_drag_shows_invalid_and_goes_back_on_release() {
    let mut b = board();
    let zone = b.square_zone();
    let start = b.mark();
    b.press(mm(10.0, 10.0));
    b.shell.pcb_drag(mm(-5.0, 5.0));
    assert!(!b.shell.pcb_tool().polygon_edit().unwrap().valid());
    b.shell.pcb_release();
    assert!(b.edits_since(start).is_empty(), "nothing runs");
    assert_eq!(
        b.zone_points(zone),
        [mm(0.0, 0.0), mm(10.0, 0.0), mm(10.0, 10.0), mm(0.0, 10.0)]
    );
    assert!(b.shell.pcb_tool().polygon_edit().is_none());
}

#[test]
fn mod_click_on_an_edge_inserts_a_vertex_and_del_removes_a_held_one() {
    let mut b = board();
    let zone = b.square_zone();
    b.shell.pcb_hover(mm(3.0, 0.0));
    b.shell.pcb_press(mm(3.0, 0.0), true, TOLERANCE);
    b.shell.pcb_release();
    assert_eq!(
        b.zone_points(zone),
        [
            mm(0.0, 0.0),
            mm(3.0, 0.0),
            mm(10.0, 0.0),
            mm(10.0, 10.0),
            mm(0.0, 10.0)
        ]
    );
    b.press(mm(10.0, 10.0));
    b.shell.pcb_drag(mm(11.0, 11.0));
    run(
        &mut b.shell,
        "pcb.copper.delete",
        json!({"objects": [zone]}),
    );
    b.shell.pcb_release();
    assert_eq!(
        b.zone_points(zone),
        [mm(0.0, 0.0), mm(3.0, 0.0), mm(10.0, 0.0), mm(0.0, 10.0)],
        "the held vertex went, not the Zone"
    );
}

#[test]
fn a_zone_following_the_board_outline_has_no_handles_until_detached() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "10mm"}),
    );
    let top = b.layer(LayerKind::Copper, Some(Side::Top));
    let out = run(
        &mut b.shell,
        "pcb.zone.add_from_board_outline",
        json!({"layers": [top]}),
    );
    let zone = uuid(&out["result"]["zones"][0]);
    run(&mut b.shell, "selection.set", json!({"objects": [zone]}));
    assert!(reda_ui::pcb::polygon_edit::selected(&b.shell).is_none());
    run(
        &mut b.shell,
        "pcb.zone.detach_outline",
        json!({"zone": zone}),
    );
    assert!(reda_ui::pcb::polygon_edit::selected(&b.shell).is_some());
}

fn pref(b: &mut Board, key: &str, value: Value) {
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": key, "value": value}),
    );
}

fn draw_square(b: &mut Board) {
    for p in [mm(20.0, 0.0), mm(30.0, 0.0), mm(30.0, 10.0), mm(20.0, 10.0)] {
        b.click(p);
    }
    run(&mut b.shell, "tool.finish", json!({}));
}

#[test]
fn the_zone_tool_pre_fills_from_its_preference() {
    let mut b = board();
    pref(&mut b, "pcb.zone.tool.pour_over", json!("all"));
    pref(&mut b, "pcb.zone.tool.clearance", json!("0.4mm"));
    pref(&mut b, "pcb.zone.tool.remove_unconnected", json!(false));
    pref(&mut b, "pcb.zone.tool.min_island_area", json!("1mm2"));
    b.tool("zone");
    draw_square(&mut b);
    let z = b.pcb().zones().next().expect("a Zone").clone();
    assert_eq!(z.pour_over, reda_ui::engine::model::PourOver::All);
    assert_eq!(z.clearance, Some(Nm::from_um(400)));
    assert!(!z.remove_unconnected);
    assert_eq!(z.min_island_area, 1_000_000_000_000);
    assert_eq!(z.min_width, None, "unset stays unset");
}

#[test]
fn the_zone_cutout_tool_cuts_on_the_active_copper_layer() {
    let mut b = board();
    let bottom = b.layer(LayerKind::Copper, Some(Side::Bottom));
    run(&mut b.shell, "pcb.layer.activate", json!({"layer": bottom}));
    b.tool("zone_cutout");
    let start = b.mark();
    draw_square(&mut b);
    assert_eq!(b.edits_since(start), ["pcb.zone_cutout.add"]);
    let cutout = b.pcb().zone_cutouts().next().unwrap();
    assert_eq!(cutout.layer, bottom);
    assert_eq!(cutout.vertices.len(), 4);
}

#[test]
fn the_keepout_tool_draws_rectangles_and_circles_by_its_options() {
    let mut b = board();
    let top = b.layer(LayerKind::Copper, Some(Side::Top));
    run(&mut b.shell, "pcb.layer.activate", json!({"layer": top}));
    pref(&mut b, "pcb.keepout.shape", json!("rectangle"));
    pref(&mut b, "pcb.keepout.all_layers", json!(false));
    pref(
        &mut b,
        "pcb.keepout.restrict",
        json!(["tracks", "footprints"]),
    );
    b.tool("keepout");
    b.drag(mm(0.0, 0.0), mm(4.0, 3.0));
    let k = b.pcb().keepouts().next().unwrap().clone();
    assert_eq!(k.layer, Some(top));
    assert_eq!(
        k.vertices().iter().map(Vertex::point).collect::<Vec<_>>(),
        [mm(0.0, 0.0), mm(4.0, 0.0), mm(4.0, 3.0), mm(0.0, 3.0)]
    );
    assert_eq!(
        k.restrict.iter().copied().collect::<Vec<_>>(),
        [
            reda_ui::engine::model::Restriction::Tracks,
            reda_ui::engine::model::Restriction::Footprints
        ]
    );

    pref(&mut b, "pcb.keepout.shape", json!("circle"));
    pref(&mut b, "pcb.keepout.all_layers", json!(true));
    b.click(mm(10.0, 10.0));
    b.click(mm(12.0, 10.0));
    let k = b
        .pcb()
        .keepouts()
        .find(|k| k.layer.is_none())
        .expect("a circle on every copper Layer")
        .clone();
    assert_eq!(
        k.vertices().to_vec(),
        [
            Vertex::arc(mm(12.0, 10.0), Millideg::from_deg(180)),
            Vertex::arc(mm(8.0, 10.0), Millideg::from_deg(180))
        ]
    );
}

#[test]
fn move_board_carries_the_outline_and_moves_everything_inside_by_one_command() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "10mm"}),
    );
    b.shell.pcb_hover(mm(10.0, 5.0));
    let out = run(
        &mut b.shell,
        "test.footprint.add",
        json!({"at": {"x": "5mm", "y": "5mm"}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
    );
    let footprint = uuid(&out["result"]["footprint"]);
    run(&mut b.shell, "tool.move_board", json!({}));
    assert!(b.shell.pcb_tool().moving());
    let start = b.mark();
    b.shell.pcb_hover(mm(13.0, 6.0));
    assert!(b.edits_since(start).is_empty(), "nothing runs in flight");
    b.click(mm(13.0, 6.0));
    assert_eq!(b.edits_since(start), ["pcb.board.move"]);
    assert_eq!(
        b.pcb().footprint(footprint).unwrap().at,
        mm(8.0, 6.0),
        "the Footprint inside moved with the board"
    );
    assert_eq!(b.pcb().outline().unwrap().vertices[0].point(), mm(3.0, 1.0));
}

#[test]
fn a_gap_in_the_graphics_offers_the_outer_edge_retry() {
    let mut b = board();
    let mut lines = Vec::new();
    // A triangle whose last side stops 0.05 mm short, drawn 0.2 mm wide.
    for (from, to) in [
        ((0.0, 0.0), (10.0, 0.0)),
        ((10.0, 0.0), (5.0, 8.0)),
        ((5.0, 8.0), (0.03, 0.04)),
    ] {
        let out = run(
            &mut b.shell,
            "pcb.graphic.add_line",
            json!({"layer": "copper_top", "width": "0.2mm",
                   "from": {"x": format!("{}mm", from.0), "y": format!("{}mm", from.1)},
                   "to": {"x": format!("{}mm", to.0), "y": format!("{}mm", to.1)}}),
        );
        lines.push(out["result"]["uuid"].clone());
    }
    let refused = b.shell.run(
        "pcb.zone.add_from_graphics",
        json!({"graphics": lines, "layer": "copper_top"}),
    );
    assert!(refused.is_err());
    let retry = b.shell.loop_retry().expect("the retry is offered").clone();
    assert_eq!(retry.params["mode"], "outer_edge");
    assert!(retry.message.contains("gap"), "{}", retry.message);
    b.shell.answer_loop_retry(true).unwrap().unwrap();
    assert!(b.shell.loop_retry().is_none());
    assert_eq!(b.pcb().zones().count(), 1);
    assert_eq!(
        b.shell.engine().journal().last().unwrap().params["mode"],
        "outer_edge"
    );
}
