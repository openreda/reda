//! Guides on a board, through the Shell with no window (docs/spec/v0.3/grids-guides.md):
//! Place ▸ Guide repeats until Esc and Space cycles a line's angle; the cursor snaps onto
//! a line Guide with the other axis on the grid, and to crossings and point Guides, in
//! the order Hotspot, Guide, grid; the snap palette's Guides switch and `Mod` in flight
//! turn the level off, while Commands are never snapped; Guides are their own Selection
//! Filter kind, picked last; Show Grids & Guides hides them from drawing and picking but
//! not from snapping; and a Guide drags, nudges and deletes like any object.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Millideg, Nm, Point};
use reda_ui::engine::model::{Guide, GuideKind, PcbDocument, Uuid};
use reda_ui::shell::Shell;
use reda_ui::snap::{Hit, Hotspot};
use serde_json::{Value, json};

const TOLERANCE: Nm = Nm::from_um(200);

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Board {
    shell: Shell,
    pcb: Uuid,
    _home: TempDir,
    work: TempDir,
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

/// A one-Pad Footprint at (10, 0) mm, the board open on a 0.5 mm grid.
fn board() -> Board {
    let home = TempDir::new("guides-home");
    let work = TempDir::new("guides-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Guides").unwrap();
    run(
        &mut shell,
        "test.footprint.add",
        json!({"at": {"x": "10mm", "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}
        ]}),
    );
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    run(
        &mut shell,
        "view.grid.set",
        json!({"document": pcb, "grid": "0.5mm"}),
    );
    Board {
        shell,
        pcb,
        _home: home,
        work,
    }
}

impl Board {
    fn pcb(&self) -> &PcbDocument {
        self.shell.engine().project().unwrap().pcb().unwrap()
    }

    fn guides(&self) -> Vec<Guide> {
        self.pcb().guides().cloned().collect()
    }

    fn add(&mut self, params: Value) -> Uuid {
        let out = run(&mut self.shell, "pcb.guide.add", params);
        serde_json::from_value(out["result"]["uuid"].clone()).unwrap()
    }

    /// A line Guide through `at` at `deg` degrees.
    fn line(&mut self, at: Point, deg: i64) -> Uuid {
        self.add(json!({"kind": "line", "at": at, "angle": deg * 1000}))
    }

    fn hover(&mut self, at: Point) -> (Point, Hit) {
        let s = self.shell.pcb_hover(at);
        (s.at, s.hit)
    }

    fn click(&mut self, at: Point) {
        self.shell.pcb_hover(at);
        self.shell.pcb_press(at, false, TOLERANCE);
        self.shell.pcb_release();
    }

    fn tool(&mut self, tool: &str) {
        run(&mut self.shell, &format!("tool.activate:{tool}"), json!({}));
    }

    fn snap_set(&mut self, params: Value) -> Value {
        let mut params = params;
        params["document"] = json!(self.pcb);
        run(&mut self.shell, "view.snap.set", params)
    }

    fn selected(&self) -> Vec<Uuid> {
        self.shell.selection().objects.clone()
    }
}

#[test]
fn place_guide_repeats_until_esc_and_space_cycles_the_angle() {
    let mut b = board();
    b.tool("guide");
    assert_eq!(b.shell.pcb_draw().guide_angle(), Millideg(0));
    b.shell.pcb_hover(mm(1.0, 1.0));
    let preview = b
        .shell
        .pcb_draw()
        .preview()
        .expect("the Guide a click places");
    assert!(
        preview.points.iter().all(|p| p.y == mm(1.0, 1.0).y),
        "a horizontal line through the cursor"
    );
    b.click(mm(1.0, 1.0));
    for (expected, at) in [(90, 2.0), (45, 3.0), (135, 4.0), (0, 5.0)] {
        run(&mut b.shell, "tool.rotate", json!({}));
        assert_eq!(b.shell.pcb_draw().guide_angle(), Millideg(expected * 1000));
        b.click(mm(at, at));
    }
    assert_eq!(b.shell.tool().id(), "guide", "the Tool repeats");
    let placed: Vec<(GuideKind, Point, Option<i64>)> = b
        .guides()
        .iter()
        .map(|g| (g.kind, g.at, g.angle.map(|a| a.0)))
        .collect();
    let mut expected = vec![
        (GuideKind::Line, mm(1.0, 1.0), Some(0)),
        (GuideKind::Line, mm(2.0, 2.0), Some(90_000)),
        (GuideKind::Line, mm(3.0, 3.0), Some(45_000)),
        (GuideKind::Line, mm(4.0, 4.0), Some(135_000)),
        (GuideKind::Line, mm(5.0, 5.0), Some(0)),
    ];
    let key = |g: &(GuideKind, Point, Option<i64>)| (g.1.x, g.1.y);
    let mut placed = placed;
    placed.sort_by_key(key);
    expected.sort_by_key(key);
    assert_eq!(placed, expected);

    b.tool("point_guide");
    b.click(mm(6.0, 6.0));
    b.click(mm(7.0, 7.0));
    let points: Vec<Point> = b
        .guides()
        .iter()
        .filter(|g| g.kind == GuideKind::Point)
        .map(|g| g.at)
        .collect();
    assert_eq!(points.len(), 2);
    assert!(points.contains(&mm(6.0, 6.0)) && points.contains(&mm(7.0, 7.0)));
    // Esc is Select again.
    b.tool("select");
    b.click(mm(8.0, 8.0));
    assert_eq!(b.guides().len(), 7);
}

#[test]
fn the_cursor_snaps_onto_a_guide_along_the_grid_and_to_point_guides() {
    let mut b = board();
    let g = b.line(mm(0.0, 1.23), 0);
    // 0.15 mm off the Guide, inside the 8 mil Snap Distance: on the Guide, X on the grid.
    assert_eq!(
        b.hover(mm(2.1, 1.38)),
        (mm(2.0, 1.23), Hit::Guide { object: g })
    );
    // Too far off it, the grid.
    assert_eq!(b.hover(mm(2.1, 1.6)), (mm(2.0, 1.5), Hit::Grid));
    let p = b.add(json!({"kind": "point", "at": mm(4.07, 3.07)}));
    assert_eq!(
        b.hover(mm(4.0, 3.0)),
        (mm(4.07, 3.07), Hit::Guide { object: p })
    );
    // A Hotspot beats a Guide through it.
    b.line(mm(0.0, 0.05), 0);
    let (at, hit) = b.hover(mm(10.1, 0.1));
    assert_eq!(
        (at, hit.hotspot()),
        (mm(10.0, 0.0), Some(Hotspot::PadCenter))
    );
}

#[test]
fn the_guides_switch_disabled_guides_and_mod_in_flight_leave_the_grid() {
    let mut b = board();
    let g = b.line(mm(0.0, 1.23), 0);
    let on_guide = (mm(2.0, 1.23), Hit::Guide { object: g });
    assert_eq!(b.hover(mm(2.1, 1.38)), on_guide);
    let out = b.snap_set(json!({"guides": false}));
    assert_eq!(out["guides"], json!(false));
    assert_eq!(b.hover(mm(2.1, 1.38)), (mm(2.0, 1.5), Hit::Grid));
    // Kept in `.reda/view.json`.
    b.shell.save_view();
    let folder = b.work.path().join("Guides");
    let saved: Value =
        serde_json::from_slice(&std::fs::read(folder.join(".reda/view.json")).unwrap()).unwrap();
    assert_eq!(
        saved["documents"][b.pcb.to_string()]["snap_guides"],
        json!(false)
    );
    b.snap_set(json!({"guides": true}));
    assert_eq!(b.hover(mm(2.1, 1.38)), on_guide);

    run(
        &mut b.shell,
        "pcb.guide.set",
        json!({"uuid": g, "enabled": false}),
    );
    assert_eq!(b.hover(mm(2.1, 1.38)), (mm(2.0, 1.5), Hit::Grid));
    run(
        &mut b.shell,
        "pcb.guide.set",
        json!({"uuid": g, "enabled": true}),
    );

    // `Mod` in flight suspends Guides with the Hotspots.
    b.tool("line");
    let pcb = b.pcb;
    b.shell.hold_snap_keys(pcb, Modifiers::NONE, false);
    assert_eq!(b.hover(mm(2.1, 1.38)), on_guide);
    b.shell.hold_snap_keys(pcb, Modifiers::COMMAND, false);
    assert_eq!(b.hover(mm(2.1, 1.38)), (mm(2.0, 1.5), Hit::Grid));
    b.shell.hold_snap_keys(pcb, Modifiers::NONE, false);
    assert_eq!(b.hover(mm(2.1, 1.38)), on_guide);
}

#[test]
fn the_preference_gives_new_views_their_guides_switch() {
    let mut b = board();
    assert_eq!(b.snap_set(json!({}))["guides"], json!(true));
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "snap.guides", "value": false}),
    );
    assert_eq!(b.snap_set(json!({}))["guides"], json!(false));
}

#[test]
fn commands_are_never_snapped_to_a_guide() {
    let mut b = board();
    b.line(mm(0.0, 1.23), 0);
    b.add(json!({"kind": "point", "at": mm(4.07, 3.07)}));
    run(
        &mut b.shell,
        "pcb.free_pad.add",
        json!({"kind": "smd", "shape": "circle", "at": mm(2.1, 1.38),
               "width": "0.5mm", "height": "0.5mm"}),
    );
    run(
        &mut b.shell,
        "pcb.output_origin.set",
        json!({"at": mm(4.0, 3.0)}),
    );
    assert_eq!(b.pcb().free_pads().next().unwrap().at, mm(2.1, 1.38));
    assert_eq!(b.pcb().output_origin().unwrap().at, mm(4.0, 3.0));
}

#[test]
fn a_guide_is_picked_last_and_is_a_selection_filter_kind() {
    let mut b = board();
    let g = b.line(mm(0.0, 3.0), 0);
    // On the Guide alone, the Guide.
    b.click(mm(20.0, 3.0));
    assert_eq!(b.selected(), [g]);
    // Under the Footprint's Pad, the Footprint.
    let through_pad = b.line(mm(10.0, 0.0), 90);
    b.click(mm(10.0, 0.0));
    assert!(!b.selected().contains(&through_pad));
    assert!(!b.shell.selection().is_empty(), "the Footprint or its Pad");
    // Filtered out, it isn't picked.
    run(
        &mut b.shell,
        "selection.filter.set",
        json!({"document": b.pcb, "kinds": {"guide": false}}),
    );
    run(&mut b.shell, "selection.set", json!({}));
    b.click(mm(20.0, 3.0));
    assert!(b.selected().is_empty());
}

#[test]
fn show_grids_and_guides_hides_them_from_picking_but_not_from_snapping() {
    let mut b = board();
    let g = b.line(mm(0.0, 1.23), 0);
    let out = run(&mut b.shell, "view.grids_guides.show", json!({"on": false}));
    assert_eq!(out["on"], json!(false));
    b.click(mm(20.0, 1.23));
    assert!(b.selected().is_empty(), "a hidden Guide isn't picked");
    assert_eq!(
        b.hover(mm(2.1, 1.38)),
        (mm(2.0, 1.23), Hit::Guide { object: g }),
        "hidden ones still snap"
    );
    // A Guide not shown is hidden on its own.
    run(&mut b.shell, "view.grids_guides.show", json!({}));
    run(
        &mut b.shell,
        "pcb.guide.set",
        json!({"uuid": g, "shown": false}),
    );
    b.click(mm(20.0, 1.23));
    assert!(b.selected().is_empty());
    run(
        &mut b.shell,
        "pcb.guide.set",
        json!({"uuid": g, "shown": true}),
    );
    b.click(mm(20.0, 1.23));
    assert_eq!(b.selected(), [g]);
}

#[test]
fn a_guide_drags_from_where_it_is_held_and_is_not_its_own_snap() {
    let mut b = board();
    // Off the grid, so its own line would hold the cursor were it a snap.
    let g = b.line(mm(0.0, 1.23), 0);
    b.shell.pcb_hover(mm(20.0, 1.23));
    b.shell.pcb_press(mm(20.0, 1.23), false, TOLERANCE);
    // 0.1 mm off its own line: within the Snap Distance, but the grid takes it.
    b.shell.pcb_drag(mm(20.0, 1.33));
    b.shell.pcb_release();
    let moved = b.pcb().guide(g).unwrap().clone();
    assert_eq!(moved.at.y, mm(0.0, 1.5).y);
    assert_eq!(moved.angle, Some(Millideg(0)));
    // A second drag, held anywhere along it, moves it by the grid again.
    b.shell.pcb_hover(mm(-7.0, 1.5));
    b.shell.pcb_press(mm(-7.0, 1.5), false, TOLERANCE);
    b.shell.pcb_drag(mm(-7.1, 4.1));
    b.shell.pcb_release();
    assert_eq!(b.pcb().guide(g).unwrap().at.y, mm(0.0, 4.0).y);
}

#[test]
fn a_selected_guide_deletes_with_the_selection() {
    let mut b = board();
    let g = b.line(mm(0.0, 1.0), 0);
    let p = b.add(json!({"kind": "point", "at": mm(1.0, 1.0)}));
    run(
        &mut b.shell,
        "pcb.copper.delete",
        json!({"objects": [g, p]}),
    );
    assert!(b.guides().is_empty());
}
