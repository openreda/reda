//! The Symbol editor on a Shell, without a window: its tab with or without a Project,
//! Units as tabs, the Pin and drawing Tools, select, move and delete, the pin table's
//! paste, Save with the Library check in Messages, read-only starter Symbols, Edit in
//! Library and Recovery Snapshots of element tabs with no Project open.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars::{self, DocumentType};
use reda_ui::canvas::interact::Pointer;
use reda_ui::element_editor;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::format::library::write_library_info;
use reda_ui::engine::model::geom::{Millideg, Nm, Point};
use reda_ui::engine::model::{LibraryElement, LibraryInfo, Scope, Shape, Symbol, Uuid};
use reda_ui::engine::recovery as snapshots;
use reda_ui::keymap::{self, Chord};
use reda_ui::layout::TabKey;
use reda_ui::shell::Shell;
use reda_ui::symbol_editor;
use reda_ui::tools::Tool;
use serde_json::json;

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";
const MIL: f64 = 25_400.0;
/// The starter Library's Diode Symbol.
const STARTER_DIODE: &str = "4b324015-989b-4479-95fe-83588e1620c6";

struct Fixture {
    dir: TempDir,
    shell: Shell,
}

fn engine(home: &Path, starter: Option<PathBuf>) -> Engine {
    Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(starter)
        .headless(false)
        .build()
}

fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

impl Fixture {
    /// A Shell with no Project open and the empty user Library `mine` registered.
    fn new(label: &str) -> Fixture {
        Fixture::with_starter(label, None)
    }

    fn with_starter(label: &str, starter: Option<PathBuf>) -> Fixture {
        let dir = TempDir::new(label);
        let library = dir.path().join("mine");
        std::fs::create_dir_all(&library).unwrap();
        let info = LibraryInfo {
            uuid: LIBRARY.parse().unwrap(),
            name: "Mine".into(),
            description: String::new(),
        };
        std::fs::write(
            library.join("Mine.redalib"),
            write_library_info(&info).unwrap(),
        )
        .unwrap();
        let home = dir.path().join("home");
        let dirs = UserDirs::under(&home);
        std::fs::create_dir_all(&dirs.config).unwrap();
        std::fs::write(
            dirs.config.join("preferences.toml"),
            format!(
                "[libraries]\npaths = [{:?}]\n",
                library.display().to_string()
            ),
        )
        .unwrap();
        let shell = Shell::new(engine(&home, starter));
        Fixture { dir, shell }
    }

    fn new_symbol(&mut self, name: &str) -> Uuid {
        let out = self
            .shell
            .run(
                "library.element.new",
                json!({"library": LIBRARY, "kind": "symbol", "name": name}),
            )
            .unwrap();
        out["element"].as_str().unwrap().parse().unwrap()
    }

    fn symbol(&self, uuid: Uuid) -> Symbol {
        match self.shell.engine().element(uuid).unwrap() {
            LibraryElement::Symbol(s) => s.clone(),
            _ => panic!("not a Symbol"),
        }
    }

    /// Clicks at (x, y) mil on the Symbol's canvas, one mil per point, and runs what the
    /// editor asks for.
    fn click(&mut self, symbol: Uuid, x: f64, y: f64) {
        self.click_with(symbol, x, y, Modifiers::NONE);
    }

    fn click_with(&mut self, symbol: Uuid, x: f64, y: f64, modifiers: Modifiers) {
        let click = Pointer::Click {
            at: (x * MIL, y * MIL),
            modifiers,
        };
        self.events(symbol, &[click]);
    }

    fn events(&mut self, symbol: Uuid, events: &[Pointer]) {
        let commands = symbol_editor::pointer(&mut self.shell, symbol, events, MIL);
        for (id, params) in commands {
            self.shell.run(&id, params).unwrap();
        }
    }

    /// Runs what the default Keymap binds `key` to in the active context.
    fn key(&mut self, key: &str) {
        let keymap = self.shell.keymap().clone();
        let action = keymap
            .lookup(&Chord::parse(key).unwrap(), &keymap::active(&self.shell))
            .unwrap_or_else(|| panic!("{key} is bound"))
            .clone();
        let (id, params) = bars::resolve(&self.shell, &action).run.unwrap();
        self.shell.run(&id, params).unwrap();
    }

    fn selected(&self, symbol: Uuid) -> Vec<Uuid> {
        element_editor::selected_on(&self.shell, symbol)
    }
}

fn mil(n: i64) -> Nm {
    Nm(n * 25_400)
}

#[test]
fn a_new_symbol_opens_in_a_document_tab_with_no_project_open() {
    let mut f = Fixture::new("sym-tab");
    let symbol = f.new_symbol("Diode");
    assert!(f.shell.engine().project().is_none());
    let tab = TabKey::Document(symbol);
    assert!(f.shell.layout().place(&tab).is_some(), "the tab is open");
    assert_eq!(f.shell.document_tab(symbol).unwrap().title, "Diode");
    assert_eq!(f.shell.layout().active_document(), Some(symbol));
    assert_eq!(bars::active_type(&f.shell), Some(DocumentType::Symbol));
    assert_eq!(
        keymap::active(&f.shell).document,
        Some(DocumentType::Symbol)
    );
    let menus: Vec<&str> = bars::for_document(bars::active_type(&f.shell))
        .menus
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    // The element's own items fold into File and Tools.
    assert_eq!(
        menus,
        [
            "file", "edit", "view", "project", "place", "tools", "window", "help"
        ]
    );
}

#[test]
fn the_tab_survives_a_project_opening_and_goes_with_the_symbol() {
    let mut f = Fixture::new("sym-tab-project");
    let symbol = f.new_symbol("Diode");
    let work = f.dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    f.shell.new_project(&work, "Demo").unwrap();
    assert!(
        f.shell.layout().place(&TabKey::Document(symbol)).is_some(),
        "the Symbol's tab stays open beside the Project's"
    );
    assert!(f.shell.document_tab(symbol).is_some());

    f.shell
        .run("library.element.close", json!({"element": symbol}))
        .unwrap();
    assert!(f.shell.layout().place(&TabKey::Document(symbol)).is_none());
    assert!(f.shell.document_tab(symbol).is_none());
}

#[test]
fn the_pin_tool_places_pins_on_the_50_mil_grid_with_rising_numbers() {
    let mut f = Fixture::new("sym-pin-tool");
    let symbol = f.new_symbol("Chip");
    f.key("P");
    assert_eq!(f.shell.tool(), Tool::Pin);
    f.click(symbol, -213.0, 30.0);
    f.click(symbol, -190.0, -70.0);
    let s = f.symbol(symbol);
    let mut pins: Vec<(String, Point)> = s.pins.iter().map(|p| (p.number.clone(), p.at)).collect();
    pins.sort();
    assert_eq!(
        pins,
        [
            ("1".to_owned(), Point::new(mil(-200), mil(50))),
            ("2".to_owned(), Point::new(mil(-200), mil(-50))),
        ]
    );
    let out = f
        .shell
        .run("library.check", json!({"element": symbol}))
        .unwrap();
    assert_eq!(out["violations"], json!([]), "on grid, numbers unique");
}

#[test]
fn space_turns_the_next_pin_by_90_degrees() {
    let mut f = Fixture::new("sym-pin-rotate");
    let symbol = f.new_symbol("Chip");
    f.key("P");
    f.key("Space");
    f.click(symbol, 0.0, -200.0);
    assert_eq!(f.symbol(symbol).pins[0].rotation, Millideg(90_000));
}

#[test]
fn units_show_as_tabs_and_tools_draw_on_the_shown_unit() {
    let mut f = Fixture::new("sym-units");
    let symbol = f.new_symbol("OpAmp");
    let out = f
        .shell
        .run("library.symbol.unit.add", json!({"symbol": symbol}))
        .unwrap();
    let b: Uuid = out["result"]["unit"].as_str().unwrap().parse().unwrap();
    let a = f.symbol(symbol).units[0].uuid;
    assert_eq!(symbol_editor::shown_unit(&f.shell, symbol), Some(a));
    f.shell
        .run(
            "library.symbol.unit.show",
            json!({"symbol": symbol, "unit": b}),
        )
        .unwrap();
    assert_eq!(symbol_editor::shown_unit(&f.shell, symbol), Some(b));
    f.key("P");
    f.click(symbol, 0.0, 0.0);
    let pin = f.symbol(symbol).pins[0].clone();
    assert_eq!(pin.unit, b);
    let scene = &f.shell.document_tab(symbol).unwrap().scene.scene;
    assert!(scene.items.iter().any(|i| i.object == pin.uuid));

    f.shell
        .run(
            "library.symbol.unit.show",
            json!({"symbol": symbol, "unit": a}),
        )
        .unwrap();
    let scene = &f.shell.document_tab(symbol).unwrap().scene.scene;
    assert!(
        scene.items.iter().all(|i| i.object != pin.uuid),
        "Unit B's Pin is not on Unit A's canvas"
    );
}

#[test]
fn the_drawing_tools_add_lines_rectangles_circles_arcs_polygons_and_text() {
    let mut f = Fixture::new("sym-draw");
    let symbol = f.new_symbol("Body");
    f.key("L");
    f.click(symbol, 0.0, 0.0);
    f.click(symbol, 100.0, 0.0);
    f.key("R");
    f.click(symbol, -100.0, -100.0);
    f.click(symbol, 100.0, 100.0);
    f.key("C");
    f.click(symbol, 0.0, 0.0);
    f.click(symbol, 50.0, 0.0);
    f.key("A");
    f.click(symbol, 0.0, 0.0);
    f.click(symbol, 100.0, 0.0);
    f.click(symbol, 0.0, 100.0);
    f.key("O");
    f.click(symbol, 0.0, 0.0);
    f.click(symbol, 100.0, 0.0);
    f.click(symbol, 100.0, 100.0);
    f.click(symbol, 0.0, 0.0);
    // A second polygon closes with Enter, after Backspace takes back a corner.
    f.click(symbol, 0.0, -100.0);
    f.click(symbol, 50.0, -100.0);
    f.click(symbol, 50.0, -150.0);
    f.click(symbol, 99.0, -199.0);
    f.key("Backspace");
    f.key("Enter");
    f.key("T");
    f.click(symbol, 0.0, 150.0);
    let (id, params) = symbol_editor::commit_text(&mut f.shell, symbol, "OUT").unwrap();
    f.shell.run(&id, params).unwrap();

    let mut shapes: Vec<Shape> = f
        .symbol(symbol)
        .graphics
        .iter()
        .map(|g| g.shape.clone())
        .collect();
    shapes.sort_by_key(|s| format!("{s:?}"));
    let p = |x: i64, y: i64| Point::new(mil(x), mil(y));
    let mut want = vec![
        Shape::Line {
            start: p(0, 0),
            end: p(100, 0),
        },
        Shape::Rect {
            start: p(-100, -100),
            end: p(100, 100),
        },
        Shape::Circle {
            center: p(0, 0),
            radius: mil(50),
        },
        Shape::Arc {
            center: p(0, 0),
            start: p(100, 0),
            sweep: Millideg(90_000),
        },
        Shape::Polygon {
            points: vec![p(0, 0), p(100, 0), p(100, 100)],
        },
        Shape::Polygon {
            points: vec![p(0, -100), p(50, -100), p(50, -150)],
        },
        Shape::Text {
            at: p(0, 150),
            text: "OUT".into(),
            size: mil(50),
            rotation: Millideg(0),
        },
    ];
    want.sort_by_key(|s| format!("{s:?}"));
    assert_eq!(shapes, want);
}

#[test]
fn escape_drops_a_shape_in_flight() {
    let mut f = Fixture::new("sym-draw-cancel");
    let symbol = f.new_symbol("Body");
    f.key("L");
    f.click(symbol, 0.0, 0.0);
    f.key("Esc");
    assert_eq!(f.shell.tool(), Tool::Select);
    f.key("L");
    f.click(symbol, 50.0, 0.0);
    f.click(symbol, 100.0, 0.0);
    let shapes: Vec<Shape> = f
        .symbol(symbol)
        .graphics
        .iter()
        .map(|g| g.shape.clone())
        .collect();
    assert_eq!(
        shapes,
        [Shape::Line {
            start: Point::new(mil(50), mil(0)),
            end: Point::new(mil(100), mil(0))
        }]
    );
}

#[test]
fn clicking_selects_dragging_moves_and_del_deletes_each_in_one_step() {
    let mut f = Fixture::new("sym-select");
    let symbol = f.new_symbol("Chip");
    f.key("P");
    f.click(symbol, -200.0, 0.0);
    f.key("L");
    f.click(symbol, 0.0, 100.0);
    f.click(symbol, 100.0, 100.0);
    f.key("Esc");
    f.click(symbol, -150.0, 0.0);
    let pin = f.symbol(symbol).pins[0].uuid;
    assert_eq!(f.selected(symbol), [pin]);
    f.click_with(symbol, 50.0, 100.0, Modifiers::SHIFT);
    assert_eq!(f.selected(symbol).len(), 2);

    // A drag moves both on the grid.
    f.events(
        symbol,
        &[
            Pointer::DragStart {
                from: (-150.0 * MIL, 0.0),
                at: (-140.0 * MIL, 0.0),
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag((-100.0 * MIL, 52.0 * MIL)),
            Pointer::DragEnd((-100.0 * MIL, 52.0 * MIL)),
        ],
    );
    assert_eq!(f.symbol(symbol).pins[0].at, Point::new(mil(-150), mil(50)));

    f.key("Del");
    let s = f.symbol(symbol);
    assert!(s.pins.is_empty() && s.graphics.is_empty());
    assert!(f.selected(symbol).is_empty());
    f.shell
        .run("edit.undo", json!({"document": symbol}))
        .unwrap();
    let s = f.symbol(symbol);
    assert_eq!((s.pins.len(), s.graphics.len()), (1, 1), "one undo step");
}

#[test]
fn undo_with_no_project_open_uses_the_focused_symbol_tab() {
    let mut f = Fixture::new("sym-undo");
    let symbol = f.new_symbol("Chip");
    f.key("P");
    f.click(symbol, 0.0, 0.0);
    f.shell
        .run("layout.tab.activate", json!({"tab": symbol.to_string()}))
        .unwrap();
    // The tab's stack is named by the element's UUID, as a Document's is.
    assert_eq!(f.shell.undo_scope(None), Some(Scope::Document(symbol)));
    f.shell.undo(false, None).unwrap();
    assert!(f.symbol(symbol).pins.is_empty());
}

#[test]
fn pasted_pin_rows_add_and_update_pins_in_one_step() {
    let mut f = Fixture::new("sym-paste");
    let symbol = f.new_symbol("MCU");
    f.key("P");
    f.click(symbol, -200.0, 0.0);
    let text = "Number\tName\tType\n1\tVCC\tpower_in\n2\tGND\tPower In\n3\tPA0\tbidirectional\n";
    let out = symbol_editor::paste_pins(&mut f.shell, symbol, text).unwrap();
    assert_eq!(out.added, 2);
    assert_eq!(out.updated, 1);
    let s = f.symbol(symbol);
    let mut rows: Vec<(String, String, String)> = s
        .pins
        .iter()
        .map(|p| {
            (
                p.number.clone(),
                p.name.clone(),
                format!("{:?}", p.electrical_type),
            )
        })
        .collect();
    rows.sort();
    assert_eq!(
        rows,
        [
            ("1".into(), "VCC".into(), "PowerIn".into()),
            ("2".into(), "GND".into(), "PowerIn".into()),
            ("3".into(), "PA0".into(), "Bidirectional".into()),
        ]
    );
    let out = f
        .shell
        .run("library.check", json!({"element": symbol}))
        .unwrap();
    assert_eq!(out["violations"], json!([]), "new Pins land on the grid");
    f.shell
        .run("edit.undo", json!({"document": symbol}))
        .unwrap();
    assert_eq!(f.symbol(symbol).pins.len(), 1, "the paste is one undo step");
}

#[test]
fn pin_rows_parse_from_tabs_or_commas_and_bad_rows_are_reported() {
    let rows = symbol_editor::parse_pin_rows("1, A, input, 0mil, 100mil\n\n2;B\n").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].number, "1");
    assert_eq!(rows[0].name, "A");
    assert_eq!(rows[0].at, Some(Point::new(Nm(0), mil(100))));
    assert_eq!(rows[1].name, "B");
    let err = symbol_editor::parse_pin_rows("1\tA\tsideways\n").unwrap_err();
    assert!(err.contains("row 1"), "{err}");
}

#[test]
fn saving_writes_the_file_and_shows_the_library_check_in_messages() {
    let mut f = Fixture::new("sym-save");
    let symbol = f.new_symbol("Chip");
    f.shell
        .run(
            "library.symbol.pin.add",
            json!({"symbol": symbol, "name": "A", "number": "1", "at": {"x": "13mil", "y": 0}}),
        )
        .unwrap();
    f.key("Mod+S");
    assert!(f.dir.path().join("mine/symbols/Chip.redasym").is_file());
    let texts: Vec<String> = f
        .shell
        .message_groups()
        .iter()
        .flat_map(|g| g.lines.iter().map(|l| l.text.clone()))
        .collect();
    assert!(texts.iter().any(|t| t.contains("Pin 1")), "{texts:?}");

    // Fixed and saved again, the line goes.
    let pin = f.symbol(symbol).pins[0].uuid;
    f.shell
        .run(
            "library.symbol.pin.edit",
            json!({"symbol": symbol, "pin": pin, "at": {"x": 0, "y": 0}}),
        )
        .unwrap();
    f.key("Mod+S");
    let texts: Vec<String> = f
        .shell
        .message_groups()
        .iter()
        .flat_map(|g| g.lines.iter().map(|l| l.text.clone()))
        .collect();
    assert!(texts.iter().all(|t| !t.contains("Pin 1")), "{texts:?}");
}

#[test]
fn a_starter_symbol_opens_read_only_and_its_tools_change_nothing() {
    let mut f = Fixture::with_starter("sym-starter", Some(starter()));
    let diode: Uuid = STARTER_DIODE.parse().unwrap();
    f.shell
        .run("library.element.open", json!({"element": diode}))
        .unwrap();
    assert!(
        f.shell
            .document_tab(diode)
            .unwrap()
            .title
            .contains("read-only")
    );
    assert!(element_editor::read_only(f.shell.engine(), diode));
    f.key("P");
    let commands = symbol_editor::pointer(
        &mut f.shell,
        diode,
        &[Pointer::Click {
            at: (0.0, 0.0),
            modifiers: Modifiers::NONE,
        }],
        MIL,
    );
    assert!(commands.is_empty());
}

#[test]
fn edit_in_library_opens_the_symbol_of_a_placed_component() {
    let mut f = Fixture::with_starter("sym-edit-in-library", Some(starter()));
    let work = f.dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    f.shell.new_project(&work, "Demo").unwrap();
    let sheet = f
        .shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .next()
        .unwrap()
        .uuid;
    let parts = f
        .shell
        .run("library.search", json!({"text": "Diode"}))
        .unwrap();
    let part = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Diode")
        .unwrap()["uuid"]
        .clone();
    let out = f
        .shell
        .run(
            "schematic.component.place",
            json!({"document": sheet, "part": part, "at": {"x": 0, "y": 0}}),
        )
        .unwrap();
    let unit = out["result"]["unit"].clone();
    f.shell
        .run(
            "library.element.edit_in_library",
            json!({"objects": [unit]}),
        )
        .unwrap();
    let diode: Uuid = STARTER_DIODE.parse().unwrap();
    assert!(f.shell.document_tab(diode).is_some());
    assert_eq!(f.shell.layout().active_document(), Some(diode));

    let err = f
        .shell
        .run("library.element.edit_in_library", json!({"objects": []}))
        .unwrap_err();
    assert!(err.message.contains("select a placed Component"), "{err:?}");
}

#[test]
fn element_tabs_snapshot_into_the_per_user_folder_with_no_project_open() {
    let mut f = Fixture::new("sym-recovery");
    let t0 = Instant::now();
    f.shell.tick_recovery(t0);
    let symbol = f.new_symbol("Chip");
    f.key("P");
    f.click(symbol, 0.0, 0.0);
    f.shell.tick_recovery(t0 + Duration::from_secs(1));
    f.shell.tick_recovery(t0 + Duration::from_secs(10));
    f.shell.flush_recovery();
    let dirs = UserDirs::under(&f.dir.path().join("home"));
    let folder = snapshots::user_dir(&dirs);
    let found = snapshots::scan(&folder, None);
    assert_eq!(found.len(), 1, "{found:?}");

    // The next session, with no Project, offers it and restores it in its tab.
    let mut next = Shell::new(engine(&f.dir.path().join("home"), None));
    let prompt = next.recovery_prompt().expect("the snapshot is offered");
    assert_eq!(prompt.rows.len(), 1);
    let file = prompt.rows[0].found.snapshot.file.clone();
    next.restore_snapshot(&file).unwrap();
    assert!(next.document_tab(symbol).is_some(), "it opens in its tab");
    assert!(
        next.engine()
            .dirty_scopes()
            .contains(&Scope::Element(symbol))
    );
}

#[test]
fn opening_a_project_keeps_unanswered_per_user_snapshots() {
    let mut f = Fixture::new("sym-recovery-kept");
    let t0 = Instant::now();
    f.shell.tick_recovery(t0);
    let symbol = f.new_symbol("Chip");
    f.key("P");
    f.click(symbol, 0.0, 0.0);
    f.shell.tick_recovery(t0 + Duration::from_secs(1));
    f.shell.tick_recovery(t0 + Duration::from_secs(10));
    f.shell.flush_recovery();
    let home = f.dir.path().join("home");
    let folder = snapshots::user_dir(&UserDirs::under(&home));

    let mut next = Shell::new(engine(&home, None));
    assert!(next.recovery_prompt().is_some());
    let work = f.dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    next.new_project(&work, "Demo").unwrap();
    next.flush_recovery();
    assert_eq!(snapshots::scan(&folder, None).len(), 1, "still there");
}
