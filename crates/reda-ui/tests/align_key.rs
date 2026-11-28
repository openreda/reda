//! Align to the first-selected object (docs/spec/v0.3/grids-guides.md#align-to-a-key-object),
//! through a Shell with no window: clicks make the first pick the key, which the canvas
//! marks and the Align items pass as `key`; a rectangle selection has none, so Align uses
//! the selection's extremes.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars::{self, Action, DocumentType, Item};
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::schematic_editor::{self, Scale};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const TOLERANCE: Nm = Nm::from_um(200);

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

fn shell_on(library: PathBuf, home: &TempDir, work: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Align").unwrap();
    shell
}

/// The item `id` in `items` or their submenus.
fn find<'a>(items: &'a [Item], id: &str) -> Option<&'a Action> {
    items.iter().find_map(|i| match i {
        Item::Action(a) if a.id == id => Some(a),
        Item::Submenu(m) => find(&m.items, id),
        _ => None,
    })
}

/// Runs the item `id` of `document`'s context menu as the canvas would.
fn run_context_item(shell: &mut Shell, document: Uuid, kind: DocumentType, id: &str) -> Value {
    let menu = reda_ui::context_menu::for_type(kind);
    let item = menu
        .sections
        .iter()
        .find_map(|section| find(&section.items, id))
        .unwrap_or_else(|| panic!("no context item {id}"));
    let (command, params) = bars::resolve_for(shell, item, Some(document))
        .run
        .expect("runs");
    run(shell, &command, params.clone());
    params
}

struct Board {
    shell: Shell,
    pcb: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// A board with three one-Pad Footprints (2 mm square Pads) at (0, 0), (10, 5) and
/// (-6, -3) mm, each with a silkscreen line 2 mm above its origin: a click there picks
/// the Footprint, where one on the Pad would pick the Pad alone.
fn board(name: &str) -> (Board, [Uuid; 3]) {
    let home = TempDir::new(&format!("{name}-home"));
    let work = TempDir::new(&format!("{name}-work"));
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
    let mut shell = shell_on(library, &home, &work);
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    let mut footprint = |x: f64, y: f64| -> Uuid {
        run(
            &mut shell,
            "test.footprint.add",
            json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")},
                   "graphics": [{"layer": "silkscreen_top", "width": 150_000,
                       "shape": {"kind": "line", "start": {"x": -1_000_000, "y": 2_000_000},
                                 "end": {"x": 1_000_000, "y": 2_000_000}}}],
                   "pads": [{"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm"}]}),
        )["result"]["footprint"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    let f = [
        footprint(0.0, 0.0),
        footprint(10.0, 5.0),
        footprint(-6.0, -3.0),
    ];
    let b = Board {
        shell,
        pcb,
        _home: home,
        _work: work,
    };
    (b, f)
}

impl Board {
    fn at(&self, f: Uuid) -> Point {
        let pcb = self.shell.engine().project().unwrap().pcb().unwrap();
        pcb.footprint(f).unwrap().at
    }

    /// A click on the silkscreen line of the Footprint whose origin is at `origin`.
    fn click(&mut self, origin: Point, modifiers: Modifiers) {
        let at = Point::new(origin.x, origin.y + Nm::from_mm(2));
        self.shell.pcb_hover(at);
        self.shell
            .pcb_press_with_modifiers(at, modifiers, TOLERANCE);
        self.shell.pcb_release();
    }

    fn rectangle(&mut self, from: Point, to: Point) {
        self.shell.pcb_hover(from);
        self.shell.pcb_press(from, false, TOLERANCE);
        self.shell.pcb_drag(to);
        self.shell.pcb_release();
    }

    fn marked_key(&self) -> Option<Uuid> {
        self.shell.document_tab(self.pcb).unwrap().selection.key
    }

    fn align(&mut self, to: &str) -> Value {
        let id = format!("context.align.{to}");
        run_context_item(&mut self.shell, self.pcb, DocumentType::Pcb, &id)
    }
}

#[test]
fn clicked_footprints_align_on_the_first_selected_which_is_marked_and_stays_put() {
    let (mut b, [a, key, c]) = board("align-key-clicks");
    b.click(mm(10.0, 5.0), Modifiers::NONE);
    b.click(mm(0.0, 0.0), Modifiers::SHIFT);
    b.click(mm(-6.0, -3.0), Modifiers::SHIFT);
    assert_eq!(b.shell.selection().objects, [key, a, c]);
    assert_eq!(b.shell.selection().key, Some(key));
    assert_eq!(b.marked_key(), Some(key), "the key is drawn heavier");

    let params = b.align("left");
    assert_eq!(params["key"], json!(key));
    assert_eq!(b.at(key), mm(10.0, 5.0), "the key stays put");
    assert_eq!(b.at(a), mm(10.0, 0.0));
    assert_eq!(b.at(c), mm(10.0, -3.0));

    b.align("bottom");
    assert_eq!(b.at(key), mm(10.0, 5.0));
    assert_eq!(b.at(a), mm(10.0, 5.0));
    assert_eq!(b.at(c), mm(10.0, 5.0));
}

#[test]
fn a_rectangle_selection_has_no_key_and_aligns_on_the_extremes() {
    let (mut b, [a, k, c]) = board("align-key-rectangle");
    b.rectangle(mm(-20.0, -20.0), mm(20.0, 20.0));
    assert_eq!(b.shell.selection().objects.len(), 3);
    assert_eq!(b.shell.selection().key, None);
    assert_eq!(b.marked_key(), None);

    let params = b.align("left");
    assert!(params.get("key").is_none(), "{params}");
    for f in [a, k, c] {
        assert_eq!(b.at(f).x, Nm::from_mm(-6), "on the leftmost");
    }
}

#[test]
fn a_lone_pick_is_not_marked_and_dropping_the_key_hands_it_to_the_next_pick() {
    let (mut b, [a, key, c]) = board("align-key-toggle");
    b.click(mm(10.0, 5.0), Modifiers::NONE);
    assert_eq!(
        b.marked_key(),
        None,
        "one object selected: nothing to tell apart"
    );
    b.click(mm(0.0, 0.0), Modifiers::SHIFT);
    assert_eq!(b.marked_key(), Some(key));
    b.click(mm(-6.0, -3.0), Modifiers::SHIFT);
    // Shift+click on the key drops it: the next pick, in pick order, takes over.
    b.click(mm(10.0, 5.0), Modifiers::SHIFT);
    assert_eq!(b.shell.selection().objects, [a, c]);
    assert_eq!(b.shell.selection().key, Some(a));
    b.align("top");
    assert_eq!(b.at(a), mm(0.0, 0.0));
    assert_eq!(b.at(c), mm(-6.0, 0.0));
}

#[test]
fn a_plain_click_after_a_rectangle_starts_a_keyed_selection_again() {
    let (mut b, [a, _, c]) = board("align-key-after-rectangle");
    b.rectangle(mm(-20.0, -20.0), mm(20.0, 20.0));
    // Shift+click on a rectangle selection adds to it, still with no key.
    b.click(mm(10.0, 5.0), Modifiers::SHIFT);
    b.click(mm(10.0, 5.0), Modifiers::SHIFT);
    assert_eq!(b.shell.selection().key, None);
    b.click(mm(-6.0, -3.0), Modifiers::NONE);
    b.click(mm(0.0, 0.0), Modifiers::SHIFT);
    assert_eq!(b.shell.selection().key, Some(c));
    assert_eq!(b.shell.selection().objects, [c, a]);
}

#[test]
fn deleting_the_key_hands_it_to_the_next_pick() {
    let (mut b, [a, key, c]) = board("align-key-deleted");
    b.click(mm(10.0, 5.0), Modifiers::NONE);
    b.click(mm(0.0, 0.0), Modifiers::SHIFT);
    b.click(mm(-6.0, -3.0), Modifiers::SHIFT);
    run(
        &mut b.shell,
        "pcb.mechanical.delete",
        json!({"footprints": [key]}),
    );
    assert_eq!(b.shell.selection().objects, [a, c]);
    assert_eq!(b.shell.selection().key, Some(a));
    let params = b.align("left");
    assert_eq!(params["key"], json!(a));
    assert_eq!(b.at(c), mm(0.0, -3.0));
}

#[test]
fn selection_set_takes_a_key_among_its_objects() {
    let (mut b, [a, key, c]) = board("align-key-set");
    run(
        &mut b.shell,
        "selection.set",
        json!({"objects": [a, key, c], "key": key}),
    );
    assert_eq!(b.shell.selection().key, Some(key));
    assert_eq!(b.marked_key(), Some(key));
    let err = b
        .shell
        .run("selection.set", json!({"objects": [a, c], "key": key}))
        .unwrap_err();
    assert_eq!(err.details.unwrap()["pointer"], "/key", "{}", err.message);
}

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const MIL: f64 = 25_400.0;

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn sheet_click(shell: &mut Shell, x: f64, y: f64, modifiers: Modifiers) {
    let events = [Pointer::Click {
        at: (x * MIL, y * MIL),
        modifiers,
    }];
    for (id, params) in schematic_editor::pointer(shell, sheet(), &events, Scale::at(MIL)) {
        run(shell, &id, params);
    }
}

fn unit_at(shell: &Shell, unit: Uuid) -> (i64, i64) {
    let sheet = shell.engine().project().unwrap().schematic_sheet(sheet());
    let at = sheet.unwrap().unit(unit).unwrap().at;
    (at.x.0 / 25_400, at.y.0 / 25_400)
}

/// Runs the sheet's Edit ▸ Align item `id`.
fn edit_align(shell: &mut Shell, id: &str) -> Value {
    let bars = bars::for_document(Some(DocumentType::Schematic));
    let edit = bars.menus.iter().find(|m| m.id == "edit").unwrap();
    let item = find(&edit.items, id).unwrap_or_else(|| panic!("no item {id}"));
    let (command, params) = bars::resolve_for(shell, item, Some(sheet()))
        .run
        .expect("runs");
    run(shell, &command, params.clone());
    params
}

#[test]
fn on_a_sheet_clicked_units_align_on_the_first_selected_and_a_rectangle_on_the_extremes() {
    let home = TempDir::new("align-key-sheet-home");
    let work = TempDir::new("align-key-sheet-work");
    let library =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test");
    let mut shell = shell_on(library, &home, &work);
    let mut place = |x: i64, y: i64| -> Uuid {
        run(
            &mut shell,
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K,
                   "at": {"x": format!("{x}mil"), "y": format!("{y}mil")}}),
        )["result"]["unit"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    let (r1, r2, r3) = (place(0, 0), place(1000, 300), place(2000, -400));
    run(&mut shell, "document.open", json!({"document": SHEET}));

    // A resistor's Pins run 150 mil above and below its origin; click on its top Pin.
    sheet_click(&mut shell, 1000.0, 450.0, Modifiers::NONE);
    sheet_click(&mut shell, 0.0, 150.0, Modifiers::SHIFT);
    sheet_click(&mut shell, 2000.0, -250.0, Modifiers::SHIFT);
    assert_eq!(shell.selection().objects, [r2, r1, r3]);
    assert_eq!(shell.selection().key, Some(r2));
    assert_eq!(shell.document_tab(sheet()).unwrap().selection.key, Some(r2));

    let params = edit_align(&mut shell, "edit.align.left");
    assert_eq!(params["key"], json!(r2));
    assert_eq!(unit_at(&shell, r1), (1000, 0));
    assert_eq!(unit_at(&shell, r2), (1000, 300));
    assert_eq!(unit_at(&shell, r3), (1000, -400));
    run(&mut shell, "edit.undo", json!({"document": SHEET}));

    // A rectangle around all three: no key, so left edges go to the leftmost (R1, x = 0).
    let events = [
        Pointer::DragStart {
            from: (-500.0 * MIL, 1000.0 * MIL),
            at: (0.0, 0.0),
            modifiers: Modifiers::NONE,
        },
        Pointer::Drag((2500.0 * MIL, -1000.0 * MIL)),
        Pointer::DragEnd((2500.0 * MIL, -1000.0 * MIL)),
    ];
    for (id, params) in schematic_editor::pointer(&mut shell, sheet(), &events, Scale::at(MIL)) {
        run(&mut shell, &id, params);
    }
    assert_eq!(shell.selection().objects.len(), 3);
    assert_eq!(shell.selection().key, None);
    assert_eq!(shell.document_tab(sheet()).unwrap().selection.key, None);
    let params = edit_align(&mut shell, "edit.align.left");
    assert!(params.get("key").is_none(), "{params}");
    assert_eq!(unit_at(&shell, r2), (0, 300));
    assert_eq!(unit_at(&shell, r3), (0, -400));
}
