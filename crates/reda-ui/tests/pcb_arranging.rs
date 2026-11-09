//! Arranging on the board through the Shell with no window: Move by ΔX/ΔY (a typed delta
//! or two picked points), Rotate about a point, the Swap Tool, Arrange Outside Board and
//! Paste Special from the Edit menu, and X and Y refused on Footprints.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::bars::{self, Bars, DocumentType, Item};
use reda_ui::command_dialog::{Call, Dialogs};
use reda_ui::engine::Engine;
use reda_ui::engine::command::{ErrorCode, SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Millideg, Nm, Point};
use reda_ui::engine::model::{LayerKind, PcbDocument, Side, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const TOLERANCE: Nm = Nm::from_um(200);
/// The starter Library's MountingHole_M3 (Mechanical, no Symbol).
const MOUNTING_HOLE_M3: &str = "326ace02-ef44-4af4-8f9d-21d6c65a74ef";
const MIRROR_NOTE: &str = "Mirroring makes an unbuildable part: L flips to the other side";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
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

struct Board {
    shell: Shell,
    _home: TempDir,
    _work: TempDir,
}

/// A new Project with its board open, the grid at 0.5 mm, and two one-Pad Footprints at
/// (0, 0) and (10, 0) mm.
fn board(name: &str) -> (Board, Uuid, Uuid) {
    let home = TempDir::new(&format!("{name}-home"));
    let work = TempDir::new(&format!("{name}-work"));
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
    let mut b = Board {
        shell,
        _home: home,
        _work: work,
    };
    let a = b.footprint(0.0);
    let c = b.footprint(10.0);
    (b, a, c)
}

impl Board {
    fn footprint(&mut self, x: f64) -> Uuid {
        run(
            &mut self.shell,
            "test.footprint.add",
            json!({"at": {"x": format!("{x}mm"), "y": 0},
                   "pads": [{"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm"}]}),
        )["result"]["footprint"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn pcb(&self) -> &PcbDocument {
        self.shell.engine().project().unwrap().pcb().unwrap()
    }

    fn at(&self, f: Uuid) -> Point {
        self.pcb().footprint(f).unwrap().at
    }

    fn select(&mut self, objects: &[Uuid]) {
        run(
            &mut self.shell,
            "selection.set",
            json!({ "objects": objects }),
        );
    }

    fn click(&mut self, at: Point) {
        self.shell.pcb_hover(at);
        self.shell.pcb_press(at, false, TOLERANCE);
        self.shell.pcb_release();
    }
}

fn pcb_bars() -> &'static Bars {
    bars::for_document(Some(DocumentType::Pcb))
}

fn edit_item<'a>(bars: &'a Bars, id: &str) -> &'a bars::Action {
    let edit = bars
        .menus
        .iter()
        .find(|m| m.id == "edit")
        .expect("an Edit menu");
    fn find<'a>(items: &'a [Item], id: &str) -> Option<&'a bars::Action> {
        items.iter().find_map(|i| match i {
            Item::Action(a) if a.id == id => Some(a),
            Item::Submenu(m) => find(&m.items, id),
            _ => None,
        })
    }
    find(&edit.items, id).unwrap_or_else(|| panic!("no item {id}"))
}

#[test]
fn move_by_asks_for_a_typed_delta() {
    let (mut b, a, c) = board("arranging-move-by");
    b.select(&[a, c]);
    let item = bars::resolve(&b.shell, edit_item(pcb_bars(), "edit.move_by"));
    let (id, params) = item.run.expect("runs with a selection");
    assert_eq!(id, "pcb.move");
    // `by` is missing, so the call opens the form for it.
    match Dialogs::new().call(&mut b.shell, &id, params.clone()) {
        Call::Dialog(dialog) => assert_eq!(dialog.command(), "pcb.move"),
        Call::Ran(_) => panic!("Move by ran without a delta"),
    }
    let mut typed = params;
    typed["by"] = json!({"x": "2mm", "y": "-1mm"});
    run(&mut b.shell, &id, typed);
    assert_eq!(b.at(a), mm(2.0, -1.0));
    assert_eq!(b.at(c), mm(12.0, -1.0));
}

#[test]
fn move_by_two_points_moves_the_selection_by_their_offset() {
    let (mut b, a, c) = board("arranging-move-points");
    b.select(&[a]);
    run(&mut b.shell, "tool.activate:move_by", json!({}));
    let from = b.shell.engine().journal().len();
    b.click(mm(20.0, 20.0));
    assert_eq!(
        b.shell.engine().journal().len(),
        from,
        "one point does nothing yet"
    );
    b.click(mm(23.0, 18.5));
    assert_eq!(b.at(a), mm(3.0, -1.5));
    assert_eq!(b.at(c), mm(10.0, 0.0));
    let last = b.shell.engine().journal().last().unwrap();
    assert_eq!(last.id, "pcb.move");
}

#[test]
fn rotate_about_a_point_turns_by_the_typed_angle_about_the_picked_centre() {
    let (mut b, a, _) = board("arranging-rotate-about");
    // The angle is required, so the menu item opens the form for it.
    let item = bars::resolve(&b.shell, edit_item(pcb_bars(), "edit.rotate_about"));
    let (id, params) = item.run.expect("runs");
    assert_eq!(id, "tool.rotate_about");
    b.select(&[a]);
    assert!(matches!(
        Dialogs::new().call(&mut b.shell, &id, params),
        Call::Dialog(_)
    ));
    run(
        &mut b.shell,
        "tool.rotate_about",
        json!({"angle": "180deg"}),
    );
    b.click(mm(5.0, 0.0));
    let f = b.pcb().footprint(a).unwrap();
    assert_eq!(f.at, mm(10.0, 0.0), "half a turn about (5, 0)");
    assert_eq!(f.rotation, Millideg::from_deg(180));
}

#[test]
fn the_swap_tool_takes_two_picks_and_repeats_until_escape() {
    let (mut b, a, c) = board("arranging-swap");
    let d = b.footprint(20.0);
    run(&mut b.shell, "pcb.rotate", json!({"objects": [c]}));
    b.select(&[]);
    run(&mut b.shell, "tool.activate:swap", json!({}));
    b.click(mm(0.0, 0.0));
    b.click(mm(10.0, 0.0));
    assert_eq!((b.at(a), b.at(c)), (mm(10.0, 0.0), mm(0.0, 0.0)));
    assert_eq!(
        b.pcb().footprint(c).unwrap().rotation,
        Millideg::from_deg(90),
        "each keeps its own rotation"
    );
    // Still the Swap Tool: another pair.
    b.click(mm(10.0, 0.0));
    b.click(mm(20.0, 0.0));
    assert_eq!((b.at(a), b.at(d)), (mm(20.0, 0.0), mm(10.0, 0.0)));
    run(&mut b.shell, "tool.activate:select", json!({}));
}

#[test]
fn swap_with_a_pair_selected_swaps_them_at_once() {
    let (mut b, a, c) = board("arranging-swap-pair");
    b.select(&[a, c]);
    run(&mut b.shell, "tool.activate:swap", json!({}));
    assert_eq!((b.at(a), b.at(c)), (mm(10.0, 0.0), mm(0.0, 0.0)));
    assert_eq!(b.shell.tool(), reda_ui::tools::Tool::Select);
}

#[test]
fn arrange_outside_board_runs_on_the_selection_from_the_edit_menu() {
    let (mut b, a, c) = board("arranging-outside");
    run(
        &mut b.shell,
        "pcb.board_outline.add_rect",
        json!({"width": "30mm", "height": "20mm"}),
    );
    b.select(&[a]);
    let item = bars::resolve(
        &b.shell,
        edit_item(pcb_bars(), "edit.arrange_outside_board"),
    );
    let (id, params) = item.run.expect("runs");
    run(&mut b.shell, &id, params);
    let outline_right = b
        .pcb()
        .outline()
        .unwrap()
        .vertices
        .iter()
        .map(|v| v.x)
        .max()
        .unwrap();
    assert!(b.at(a).x > outline_right, "{:?}", b.at(a));
    assert_eq!(b.at(c), mm(10.0, 0.0));
}

#[test]
fn paste_special_keeps_nets_and_pastes_on_the_active_layer() {
    let (mut b, _, _) = board("arranging-paste-special");
    let net: Uuid = run(&mut b.shell, "test.net.add", json!({"name": "SIG"}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let track: Uuid = run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.25mm", "net": net,
               "points": [{"x": "0mm", "y": "5mm"}, {"x": "5mm", "y": "5mm"}]}),
    )["result"]["tracks"][0]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    b.select(&[track]);
    let copy = bars::resolve(&b.shell, edit_item(pcb_bars(), "edit.copy"));
    let (id, params) = copy.run.expect("copies");
    run(&mut b.shell, &id, params);
    run(&mut b.shell, "pcb.layer.swap_side", json!({}));
    let bottom = b
        .pcb()
        .layers()
        .find(LayerKind::Copper, Some(Side::Bottom))
        .unwrap()
        .uuid;

    let special = bars::resolve(&b.shell, edit_item(pcb_bars(), "edit.paste_special"));
    let (id, params) = special.run.expect("pastes");
    assert_eq!(id, "pcb.paste");
    assert_eq!(params["keep_nets"], true);
    assert_eq!(params["layer"], json!(bottom));
    run(&mut b.shell, &id, params);
    let pasted = b.shell.selection().objects.clone();
    assert_eq!(pasted.len(), 1, "the paste is selected");
    let t = b.pcb().track(pasted[0]).unwrap();
    assert_eq!((t.layer, t.net), (bottom, Some(net)));

    // A plain paste keeps the Layer and drops the Net.
    let paste = bars::resolve(&b.shell, edit_item(pcb_bars(), "edit.paste"));
    let (id, params) = paste.run.expect("pastes");
    run(&mut b.shell, &id, params);
    let t = b.pcb().track(b.shell.selection().objects[0]).unwrap();
    assert_ne!(t.layer, bottom);
    assert_eq!(t.net, None);
}

#[test]
fn x_and_y_are_refused_on_a_selected_or_moving_footprint() {
    let (mut b, a, _) = board("arranging-mirror");
    b.select(&[a]);
    for axis in ["x", "y"] {
        let err = b
            .shell
            .run(&format!("tool.mirror_{axis}"), json!({}))
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Rejected);
        assert_eq!(err.message, MIRROR_NOTE);
    }
    // In flight too.
    b.shell.pcb_press(mm(0.0, 0.0), false, TOLERANCE);
    b.shell.pcb_drag(mm(2.0, 0.0));
    let err = b.shell.run("tool.mirror_x", json!({})).unwrap_err();
    assert_eq!(err.message, MIRROR_NOTE);
    b.shell.pcb_release();
    let f = b.pcb().footprint(a).unwrap();
    assert_eq!((f.side, f.rotation), (Side::Top, Millideg(0)));
}

#[test]
fn x_and_y_are_refused_on_a_mechanical_part_being_placed() {
    let (mut b, _, _) = board("arranging-mirror-place");
    let board = b.pcb().uuid;
    b.select(&[]);
    run(
        &mut b.shell,
        "tool.place",
        json!({"document": board, "part": MOUNTING_HOLE_M3}),
    );
    let err = b.shell.run("tool.mirror_y", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(err.message, MIRROR_NOTE);
}
