//! Cartesian Local Grids on a board, through the Shell with no window
//! (docs/spec/v0.3/grids-guides.md#local-grids): Place ▸ Local Grid drags a rectangle and
//! repeats until Esc; inside one the cursor snaps to its grid, the first in list order
//! winning and the everyday grid last, for what its Applies to covers; the snap palette's
//! switch, Gridless and `Mod`+Shift turn them off; the status bar names the one under the
//! cursor and G cycles only the everyday grid; arrow keys and nudges step by it; it is a
//! Selection Filter kind picked last, its handles resize it, and a locked one has none.

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{AppliesTo, LocalGrid, LocalGridShape, PcbDocument, Uuid};
use reda_ui::shell::Shell;
use reda_ui::snap::Hit;
use serde_json::{Value, json};

const TOLERANCE: Nm = Nm::from_um(200);

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

/// A one-Pad Footprint at (10, 0) mm, the board open and active on a 0.5 mm grid.
fn board() -> Board {
    let home = TempDir::new("local-grids-home");
    let work = TempDir::new("local-grids-work");
    let library = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-schematic/tests/library/Test");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Grids").unwrap();
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
        "layout.tab.activate",
        json!({"tab": pcb.to_string()}),
    );
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

    fn grids(&self) -> Vec<LocalGrid> {
        self.pcb().local_grids().cloned().collect()
    }

    fn grid(&self, uuid: Uuid) -> LocalGrid {
        self.pcb().local_grid(uuid).unwrap().clone()
    }

    /// A Local Grid named `name`: `w` × `h` mm from `origin`, turned `deg`, every
    /// `step`.
    fn add(&mut self, name: &str, origin: Point, step: &str, (w, h): (f64, f64), deg: i64) -> Uuid {
        let out = run(
            &mut self.shell,
            "pcb.local_grid.add",
            json!({"name": name, "shape": {"kind": "cartesian", "origin": origin,
                   "rotation": deg * 1000, "step_x": step,
                   "width": format!("{w}mm"), "height": format!("{h}mm")}}),
        );
        serde_json::from_value(out["result"]["uuid"].clone()).unwrap()
    }

    fn set(&mut self, uuid: Uuid, fields: Value) {
        let mut params = fields;
        params["uuid"] = json!(uuid);
        run(&mut self.shell, "pcb.local_grid.set", params);
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

    fn drag(&mut self, from: Point, to: Point) {
        self.shell.pcb_hover(from);
        self.shell.pcb_press(from, false, TOLERANCE);
        self.shell.pcb_drag(to);
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

    fn status_grid(&self) -> String {
        reda_ui::status_bar::readout(&self.shell).grid.unwrap()
    }

    fn step(&mut self, dx: i64, dy: i64) -> Point {
        let out = run(
            &mut self.shell,
            "view.cursor.step",
            json!({"dx": dx, "dy": dy}),
        );
        serde_json::from_value(out["at"].clone()).unwrap()
    }
}

fn cartesian(grid: &LocalGrid) -> reda_ui::engine::model::CartesianGrid {
    let LocalGridShape::Cartesian(c) = grid.shape;
    c
}

/// "A", 0.3 mm over (0, 2)–(6, 8) mm.
fn grid_a(b: &mut Board) -> Uuid {
    b.add("A", mm(0.0, 2.0), "0.3mm", (6.0, 6.0), 0)
}

#[test]
fn place_local_grid_drags_a_rectangle_on_the_everyday_step_until_esc() {
    let mut b = board();
    b.tool("local_grid");
    b.shell.pcb_hover(mm(1.1, 0.9));
    b.shell.pcb_press(mm(1.1, 0.9), false, TOLERANCE);
    b.shell.pcb_drag(mm(5.1, 3.1));
    let preview = b
        .shell
        .pcb_draw()
        .preview()
        .expect("the area a drag places");
    assert!(preview.closed);
    assert!(preview.points.contains(&mm(1.0, 1.0)) && preview.points.contains(&mm(5.0, 3.0)));
    b.shell.pcb_release();
    let grids = b.grids();
    assert_eq!(grids.len(), 1);
    let (g, c) = (&grids[0], cartesian(&grids[0]));
    assert_eq!(g.name, "Local Grid 1");
    assert_eq!(g.applies_to, AppliesTo::All);
    assert_eq!(
        (c.origin, c.width, c.height),
        (mm(1.0, 1.0), mm(4.0, 0.0).x, mm(2.0, 0.0).x)
    );
    assert_eq!(
        (c.step_x.to_string(), c.step_y.to_string()),
        ("0.5mm".into(), "0.5mm".into())
    );
    // From the top right corner down to the left: the same rectangle shape.
    b.drag(mm(24.0, 22.0), mm(20.0, 20.0));
    assert_eq!(b.shell.tool().id(), "local_grid", "the Tool repeats");
    let grids = b.grids();
    assert_eq!(grids.len(), 2);
    assert_eq!(grids[1].name, "Local Grid 2", "a new one goes last");
    assert_eq!(cartesian(&grids[1]).origin, mm(20.0, 20.0));
    // Esc is Select again.
    b.tool("select");
    b.drag(mm(30.0, 30.0), mm(32.0, 32.0));
    assert_eq!(b.grids().len(), 2);
}

#[test]
fn inside_a_local_grid_the_cursor_snaps_to_it_the_first_in_the_list_winning() {
    let mut b = board();
    let a = grid_a(&mut b);
    // 0.3 mm from (0, 2): (2.1, 3.2) mm, not the everyday (2.0, 3.0).
    assert_eq!(
        b.hover(mm(2.2, 3.1)),
        (mm(2.1, 3.2), Hit::LocalGrid { object: a })
    );
    // Outside it, the everyday grid.
    assert_eq!(b.hover(mm(7.1, 3.1)), (mm(7.0, 3.0), Hit::Grid));
    // "B", 0.7 mm from (1, 3), over A: A is first in the list, so A wins.
    let b_grid = b.add("B", mm(1.0, 3.0), "0.7mm", (4.0, 4.0), 0);
    assert_eq!(
        b.hover(mm(2.2, 3.1)),
        (mm(2.1, 3.2), Hit::LocalGrid { object: a })
    );
    run(
        &mut b.shell,
        "pcb.local_grid.reorder",
        json!({"local_grid": b_grid, "before": a}),
    );
    assert_eq!(
        b.hover(mm(2.2, 3.1)),
        (mm(2.4, 3.0), Hit::LocalGrid { object: b_grid })
    );
    // A disabled one gives way to the next.
    b.set(b_grid, json!({"enabled": false}));
    assert_eq!(
        b.hover(mm(2.2, 3.1)),
        (mm(2.1, 3.2), Hit::LocalGrid { object: a })
    );
    // A Hotspot beats it.
    b.add("Pad", mm(8.0, -2.0), "0.3mm", (4.0, 4.0), 0);
    let (at, hit) = b.hover(mm(10.1, 0.1));
    assert_eq!(at, mm(10.0, 0.0));
    assert!(hit.hotspot().is_some(), "{hit:?}");
}

#[test]
fn a_turned_local_grid_snaps_along_its_own_axes() {
    let mut b = board();
    let g = b.add("Turned", mm(0.0, 0.0), "1mm", (5.0, 5.0), 90);
    // Turned 90°: its X runs up, its Y to the left, so it covers x −5..0.
    assert_eq!(
        b.hover(mm(-1.9, 2.1)),
        (mm(-2.0, 2.0), Hit::LocalGrid { object: g })
    );
    assert_eq!(b.hover(mm(1.9, 2.1)), (mm(2.0, 2.0), Hit::Grid));
}

#[test]
fn applies_to_picks_what_a_local_grid_snaps_for() {
    let mut b = board();
    // Over the Footprint at (10, 0), for Components only.
    let c = b.add("Parts", mm(8.0, -2.0), "0.3mm", (5.0, 5.0), 0);
    b.set(c, json!({"applies_to": "components"}));
    // The bare cursor is not a Component: the everyday grid.
    b.tool("line");
    assert_eq!(b.hover(mm(9.1, 1.6)), (mm(9.0, 1.5), Hit::Grid));
    b.tool("select");
    // Dragging the Footprint by (0.1, 0.13) mm: its origin lands on the Parts grid.
    let fp = b.pcb().footprints().next().unwrap().uuid;
    b.drag(mm(10.0, 0.0), mm(10.1, 0.13));
    let at = b.pcb().footprint(fp).unwrap().at;
    assert_eq!(at, mm(10.1, 0.1));
    // All but Components: the Footprint takes the everyday grid; the cursor the Local.
    run(&mut b.shell, "edit.undo", json!({"document": b.pcb}));
    b.set(c, json!({"applies_to": "not_components"}));
    b.drag(mm(10.0, 0.0), mm(10.1, 0.13));
    assert_eq!(b.pcb().footprint(fp).unwrap().at, mm(10.0, 0.0));
    b.tool("line");
    assert_eq!(
        b.hover(mm(9.1, 1.6)),
        (mm(9.2, 1.6), Hit::LocalGrid { object: c })
    );
}

#[test]
fn the_palette_switch_gridless_and_mod_shift_turn_local_grids_off() {
    let mut b = board();
    let a = grid_a(&mut b);
    let local = (mm(2.1, 3.2), Hit::LocalGrid { object: a });
    assert_eq!(b.hover(mm(2.2, 3.1)), local);
    let out = b.snap_set(json!({"local_grids": false}));
    assert_eq!(out["local_grids"], json!(false));
    assert_eq!(b.hover(mm(2.2, 3.1)), (mm(2.0, 3.0), Hit::Grid));
    // Kept in `.reda/view.json`.
    b.shell.save_view();
    let folder = b.work.path().join("Grids");
    let saved: Value =
        serde_json::from_slice(&std::fs::read(folder.join(".reda/view.json")).unwrap()).unwrap();
    assert_eq!(
        saved["documents"][b.pcb.to_string()]["snap_local_grids"],
        json!(false)
    );
    b.snap_set(json!({"local_grids": true}));
    assert_eq!(b.hover(mm(2.2, 3.1)), local);

    // Gridless: neither grid.
    b.snap_set(json!({"gridless": true}));
    assert_eq!(b.hover(mm(2.2, 3.1)), (mm(2.2, 3.1), Hit::Free));
    b.snap_set(json!({"gridless": false}));

    // `Mod` in flight leaves it; `Mod`+Shift suspends it with the everyday grid.
    b.tool("line");
    let pcb = b.pcb;
    b.shell.hold_snap_keys(pcb, Modifiers::COMMAND, false);
    assert_eq!(b.hover(mm(2.2, 3.1)), local);
    b.shell
        .hold_snap_keys(pcb, Modifiers::COMMAND | Modifiers::SHIFT, false);
    assert_eq!(b.hover(mm(2.2, 3.1)), (mm(2.2, 3.1), Hit::Free));
    b.shell.hold_snap_keys(pcb, Modifiers::NONE, false);
    assert_eq!(b.hover(mm(2.2, 3.1)), local);
}

#[test]
fn the_preference_gives_new_views_their_local_grids_switch() {
    let mut b = board();
    assert_eq!(b.snap_set(json!({}))["local_grids"], json!(true));
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "snap.local_grids", "value": false}),
    );
    assert_eq!(b.snap_set(json!({}))["local_grids"], json!(false));
}

#[test]
fn the_status_bar_names_the_local_grid_and_g_cycles_only_the_everyday_grid() {
    let mut b = board();
    grid_a(&mut b);
    b.hover(mm(7.1, 3.1));
    assert_eq!(b.status_grid(), "Grid: 0.5 mm");
    b.hover(mm(2.2, 3.1));
    assert_eq!(b.status_grid(), "Grid: A 0.3 mm");
    let before = b.shell.grid(b.pcb);
    run(&mut b.shell, "view.grid.cycle", json!({"document": b.pcb}));
    assert_ne!(b.shell.grid(b.pcb), before, "G steps the everyday grid");
    b.hover(mm(2.2, 3.1));
    assert_eq!(
        b.status_grid(),
        "Grid: A 0.3 mm",
        "and leaves the Local Grid"
    );
    assert_eq!(cartesian(&b.grids()[0]).step_x.to_string(), "0.3mm");
}

#[test]
fn arrow_keys_step_by_the_local_grid_inside_it() {
    let mut b = board();
    b.add("Fine", mm(0.0, 0.0), "0.3mm", (5.0, 5.0), 0);
    // The keys start at the grid's origin, inside Fine.
    assert_eq!(b.step(1, 0), mm(0.3, 0.0));
    assert_eq!(b.step(1, 1), mm(0.6, 0.3));
    assert_eq!(b.step(20, 0), mm(6.6, 0.3));
    // Outside, the everyday grid: X to the next line, Y onto its nearest.
    assert_eq!(b.step(1, 0), mm(7.0, 0.5));
}

#[test]
fn arrow_keys_step_along_a_turned_local_grids_axes() {
    let mut b = board();
    b.add("Turned", mm(0.0, 0.0), "0.3mm", (5.0, 5.0), 30);
    // Right is the grid's X, at 30°; Up its Y, at 120°.
    let right = b.step(1, 0);
    assert_eq!(right, Point::new(Nm(259_808), Nm(150_000)));
    let up = b.step(0, 1);
    assert_eq!(up, Point::new(Nm(109_808), Nm(409_808)));
}

#[test]
fn a_nudge_steps_by_the_local_grid_the_selection_is_in() {
    let mut b = board();
    b.add("Fine", mm(0.0, 0.0), "0.3mm", (5.0, 5.0), 0);
    let out = run(
        &mut b.shell,
        "pcb.free_pad.add",
        json!({"kind": "smd", "shape": "circle", "at": mm(1.1, 1.1),
               "width": "0.5mm", "height": "0.5mm"}),
    );
    let pad: Uuid = serde_json::from_value(out["result"]["uuid"].clone()).unwrap();
    run(&mut b.shell, "selection.set", json!({"objects": [pad]}));
    run(&mut b.shell, "pcb.nudge", json!({"dx": 1, "dy": 0}));
    // The next 0.3 mm line to the right of 1.1 mm.
    assert_eq!(b.pcb().free_pads().next().unwrap().at, mm(1.2, 1.1));
    run(&mut b.shell, "pcb.nudge", json!({"dx": 1, "dy": 0}));
    assert_eq!(b.pcb().free_pads().next().unwrap().at, mm(1.5, 1.1));
}

#[test]
fn a_local_grid_is_picked_by_its_boundary_last_and_is_a_selection_filter_kind() {
    let mut b = board();
    let a = grid_a(&mut b);
    // Its boundary picks it; its area does not.
    b.click(mm(0.0, 5.0));
    assert_eq!(b.selected(), [a]);
    run(&mut b.shell, "selection.set", json!({}));
    b.click(mm(3.0, 5.0));
    assert!(b.selected().is_empty());
    // A Guide on its edge is picked before it.
    let out = run(
        &mut b.shell,
        "pcb.guide.add",
        json!({"kind": "line", "at": mm(0.0, 0.0), "angle": 90_000}),
    );
    let guide: Uuid = serde_json::from_value(out["result"]["uuid"].clone()).unwrap();
    b.click(mm(0.0, 5.0));
    assert_eq!(b.selected(), [guide]);
    // Filtered out, it isn't picked.
    run(&mut b.shell, "selection.set", json!({}));
    run(
        &mut b.shell,
        "selection.filter.set",
        json!({"document": b.pcb, "kinds": {"local_grid": false}}),
    );
    b.click(mm(6.0, 5.0));
    assert!(b.selected().is_empty());
    // Hidden by Show Grids & Guides, nor is it.
    run(
        &mut b.shell,
        "selection.filter.set",
        json!({"document": b.pcb, "kinds": {"local_grid": true}}),
    );
    run(&mut b.shell, "view.grids_guides.show", json!({"on": false}));
    b.click(mm(6.0, 5.0));
    assert!(b.selected().is_empty());
    run(&mut b.shell, "view.grids_guides.show", json!({"on": true}));
    b.click(mm(6.0, 5.0));
    assert_eq!(b.selected(), [a]);
}

#[test]
fn the_handles_resize_a_local_grid_from_its_origin_and_a_locked_one_has_none() {
    let mut b = board();
    let a = grid_a(&mut b);
    run(&mut b.shell, "selection.set", json!({"objects": [a]}));
    // The width handle, mid far X edge, dragged out past it onto the everyday grid.
    b.drag(mm(6.0, 5.0), mm(8.05, 5.1));
    let c = cartesian(&b.grid(a));
    assert_eq!(
        (c.origin, c.width, c.height),
        (mm(0.0, 2.0), Nm(8_000_000), Nm(6_000_000))
    );
    assert_eq!(b.selected(), [a], "still selected");
    // The corner handle, dragged in: both, on the Local Grid's own points.
    b.drag(mm(8.0, 8.0), mm(5.1, 6.1));
    let c = cartesian(&b.grid(a));
    assert_eq!((c.width, c.height), (Nm(5_100_000), Nm(4_200_000)));
    // One undo step each.
    run(&mut b.shell, "edit.undo", json!({"document": b.pcb}));
    assert_eq!(cartesian(&b.grid(a)).width, Nm(8_000_000));
    // Locked: no handles, and the press does not move it either.
    b.set(a, json!({"locked": true}));
    run(&mut b.shell, "selection.set", json!({"objects": [a]}));
    b.drag(mm(8.0, 5.0), mm(10.0, 5.0));
    let c = cartesian(&b.grid(a));
    assert_eq!((c.origin, c.width), (mm(0.0, 2.0), Nm(8_000_000)));
}

#[test]
fn a_selected_local_grid_deletes_with_the_selection_unless_locked() {
    let mut b = board();
    let a = grid_a(&mut b);
    b.set(a, json!({"locked": true}));
    assert!(
        b.shell
            .run("pcb.local_grid.delete", json!({"uuid": a}))
            .is_err()
    );
    b.set(a, json!({"locked": false, "include_locked": true}));
    run(&mut b.shell, "pcb.copper.delete", json!({"objects": [a]}));
    assert!(b.grids().is_empty());
}
