//! The PCB canvas's select-and-place Tool, through the Shell with no window: clicking
//! selects (through `selection.set`), dragging moves Footprints with a preview that never
//! touches the Journal until release, Space and L turn and flip (in flight too), a
//! rubber band selects, and moves snap to the grid and to Pads, Track ends and Vias.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Millideg, Nm, Point};
use reda_ui::engine::model::{FootprintInstance, Side, Uuid};
use reda_ui::pcb::snap::SnapKind;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// How far from an object a click still hits it: 0.2 mm.
const TOLERANCE: Nm = Nm::from_um(200);

struct Board {
    shell: Shell,
    pcb: Uuid,
    footprints: [Uuid; 2],
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
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

/// Two one-Pad (1 mm square) Footprints on Net SIG at (0, 0) and (10, 0) mm, the PCB
/// open, the grid at 0.5 mm.
fn board() -> Board {
    let home = TempDir::new("pcb-editor-home");
    let work = TempDir::new("pcb-editor-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let net = run(&mut shell, "test.net.add", json!({"name": "SIG"}))["result"]["uuid"].clone();
    let mut footprint = |x: &str| {
        let out = run(
            &mut shell,
            "test.footprint.add",
            json!({"at": {"x": x, "y": 0}, "pads": [
                {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}
            ]}),
        );
        out["result"]["footprint"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    let footprints = [footprint("0mm"), footprint("10mm")];
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    Board {
        shell,
        pcb,
        footprints,
        _home: home,
        _work: work,
    }
}

fn footprint(b: &Board, uuid: Uuid) -> FootprintInstance {
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    pcb.footprint(uuid).unwrap().clone()
}

fn journal_ids(b: &Board) -> Vec<String> {
    b.shell
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

/// The centre of what the canvas draws for `object`.
fn drawn_center(b: &Board, object: Uuid) -> Point {
    let tab = b.shell.document_tab(b.pcb).unwrap();
    let (mut lo, mut hi) = (
        Point::new(Nm(i64::MAX), Nm(i64::MAX)),
        Point::new(Nm(i64::MIN), Nm(i64::MIN)),
    );
    for item in tab.scene.scene.items_of(object).filter(|item| {
        !matches!(
            item.child,
            Some(reda_ui::engine::render::Child::FootprintText { .. })
        )
    }) {
        if let Some((a, z)) = item.primitive.bounds() {
            lo = Point::new(lo.x.min(a.x), lo.y.min(a.y));
            hi = Point::new(hi.x.max(z.x), hi.y.max(z.y));
        }
    }
    Point::new(Nm((lo.x.0 + hi.x.0) / 2), Nm((lo.y.0 + hi.y.0) / 2))
}

fn click(b: &mut Board, at: Point, toggle: bool) {
    b.shell.pcb_press_with_modifiers(
        at,
        if toggle {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        },
        TOLERANCE,
    );
    b.shell.pcb_release();
}

#[test]
fn a_click_selects_a_pad_and_shift_click_toggles() {
    let mut b = board();
    let [f1, f2] = b.footprints;

    click(&mut b, mm(0.2, 0.1), false);
    assert_eq!(
        b.shell
            .selection()
            .pads
            .iter()
            .map(|p| p.footprint)
            .collect::<Vec<_>>(),
        [f1]
    );
    assert_eq!(journal_ids(&b).last().unwrap(), "selection.set");

    click(&mut b, mm(10.0, 0.0), true);
    assert_eq!(
        b.shell
            .selection()
            .pads
            .iter()
            .map(|p| p.footprint)
            .collect::<Vec<_>>(),
        [f1, f2]
    );
    click(&mut b, mm(0.0, 0.0), true);
    assert_eq!(
        b.shell
            .selection()
            .pads
            .iter()
            .map(|p| p.footprint)
            .collect::<Vec<_>>(),
        [f2]
    );

    click(&mut b, mm(5.0, 5.0), false);
    assert!(b.shell.selection().is_empty(), "a click on nothing clears");
}

#[test]
fn a_drag_previews_the_move_and_runs_one_command_on_release() {
    let mut b = board();
    let [f1, _] = b.footprints;
    let entries = b.shell.engine().journal().len();

    b.shell.pcb_press(mm(0.3, 0.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(3.4, 2.1));

    // In flight: drawn at the snapped spot, but nothing has run.
    assert_eq!(drawn_center(&b, f1), mm(3.0, 2.0));
    assert_eq!(footprint(&b, f1).at, mm(0.0, 0.0));
    assert_eq!(
        b.shell.engine().journal().len(),
        entries + 1,
        "only the selection"
    );

    b.shell.pcb_drag(mm(3.6, 2.1));
    assert_eq!(drawn_center(&b, f1), mm(3.5, 2.0));
    assert_eq!(b.shell.engine().journal().len(), entries + 1);

    b.shell.pcb_release();
    assert_eq!(footprint(&b, f1).at, mm(3.5, 2.0));
    assert_eq!(journal_ids(&b).last().unwrap(), "pcb.move");
    assert_eq!(b.shell.engine().journal().len(), entries + 2);
    assert_eq!(b.shell.selection().objects, [f1]);
}

#[test]
fn space_and_l_turn_and_flip_in_flight_and_release_is_one_undo_step() {
    let mut b = board();
    let [f1, _] = b.footprints;

    b.shell.pcb_press(mm(0.0, 0.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(2.0, 0.0));
    b.shell.run("tool.rotate", json!({})).unwrap();
    b.shell.run("tool.flip", json!({})).unwrap();
    b.shell.pcb_drag(mm(4.0, 1.0));
    let entries = b.shell.engine().journal().len();
    b.shell.pcb_release();

    let f = footprint(&b, f1);
    assert_eq!(f.at, mm(4.0, 1.0));
    assert_eq!(f.rotation, Millideg::from_deg(270));
    assert_eq!(f.side, Side::Bottom);
    assert_eq!(b.shell.engine().journal().len(), entries + 1);
    assert_eq!(journal_ids(&b).last().unwrap(), "batch");

    b.shell.undo(false, None).unwrap();
    let f = footprint(&b, f1);
    assert_eq!(
        (f.at, f.rotation, f.side),
        (mm(0.0, 0.0), Millideg(0), Side::Top)
    );
}

#[test]
fn space_and_l_turn_and_flip_the_selection_when_nothing_is_in_flight() {
    let mut b = board();
    let [f1, _] = b.footprints;
    run(&mut b.shell, "selection.set", json!({"objects":[f1]}));

    b.shell.run("tool.rotate", json!({})).unwrap();
    assert_eq!(journal_ids(&b).last().unwrap(), "pcb.rotate");
    assert_eq!(footprint(&b, f1).rotation, Millideg::from_deg(90));
    b.shell.run("tool.flip", json!({})).unwrap();
    assert_eq!(journal_ids(&b).last().unwrap(), "pcb.flip");
    assert_eq!(footprint(&b, f1).side, Side::Bottom);
}

#[test]
fn escape_cancels_a_move_in_flight() {
    let mut b = board();
    let [f1, _] = b.footprints;
    b.shell.pcb_press(mm(0.0, 0.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(5.0, 5.0));
    let entries = b.shell.engine().journal().len();

    b.shell.pcb_cancel();
    b.shell.pcb_release();

    assert_eq!(b.shell.engine().journal().len(), entries);
    assert_eq!(drawn_center(&b, f1), mm(0.0, 0.0));
}

#[test]
fn a_moved_pad_snaps_onto_another_pad() {
    let mut b = board();
    let [f1, _] = b.footprints;
    // Grabbed by its Pad, dropped 0.3 mm off the other Footprint's Pad.
    b.shell.pcb_press(mm(0.1, 0.1), false, TOLERANCE);
    b.shell.pcb_drag(mm(10.4, 0.2));
    assert_eq!(b.shell.pcb_snap(), Some((mm(10.0, 0.0), SnapKind::Pad)));
    b.shell.pcb_release();
    assert_eq!(footprint(&b, f1).at, mm(10.0, 0.0));
}

#[test]
fn the_cursor_snaps_to_track_ends_and_via_centres() {
    let mut b = board();
    let top = {
        let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
        pcb.copper_layers().next().unwrap().uuid
    };
    run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": top, "points": [{"x": "20mm", "y": 0}, {"x": "20.13mm", "y": "5.07mm"}]}),
    );
    run(
        &mut b.shell,
        "pcb.via.add",
        json!({"at": {"x": "30.07mm", "y": "1.11mm"}}),
    );

    assert_eq!(
        b.shell.pcb_hover(mm(20.0, 5.0)),
        (mm(20.13, 5.07), SnapKind::TrackEnd)
    );
    assert_eq!(
        b.shell.pcb_hover(mm(30.0, 1.0)),
        (mm(30.07, 1.11), SnapKind::Via)
    );
    assert_eq!(
        b.shell.pcb_hover(mm(40.3, 1.1)),
        (mm(40.5, 1.0), SnapKind::Grid)
    );
}

#[test]
fn a_rubber_band_selects_what_lies_inside_it() {
    let mut b = board();
    let [f1, f2] = b.footprints;
    b.shell.pcb_press(mm(-2.0, -2.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(12.0, 2.0));
    assert_eq!(b.shell.pcb_band(), Some((mm(-2.0, -2.0), mm(12.0, 2.0))));
    b.shell.pcb_release();
    let mut selected = b.shell.selection().objects.clone();
    selected.sort();
    let mut both = vec![f1, f2];
    both.sort();
    assert_eq!(selected, both);
    assert_eq!(b.shell.pcb_band(), None);

    // Only what lies wholly inside.
    b.shell.pcb_press(mm(-2.0, -2.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(10.0, 2.0));
    b.shell.pcb_release();
    assert_eq!(b.shell.selection().objects, [f1]);
}

#[test]
fn a_protected_locked_footprint_is_skipped_by_the_canvas() {
    let mut b = board();
    let [f1, _] = b.footprints;
    run(
        &mut b.shell,
        "pcb.lock",
        json!({"footprints": [f1], "locked": true}),
    );
    run(
        &mut b.shell,
        "selection.filter.set",
        json!({"document":b.pcb,"kinds":{"footprint_text":false}}),
    );
    let entries = b.shell.engine().journal().len();

    b.shell.pcb_press(mm(0.0, 0.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(5.0, 0.0));
    b.shell.pcb_release();

    assert_eq!(footprint(&b, f1).at, mm(0.0, 0.0));
    assert!(b.shell.message().is_none());
    assert!(b.shell.selection().objects.is_empty());
    assert_eq!(journal_ids(&b)[entries..], ["selection.set"]);
}
