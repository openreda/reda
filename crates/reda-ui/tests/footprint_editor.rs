//! The Footprint editor on a Shell, without a window: an open Footprint is a Document tab
//! (with or without a Project), and the shared canvas Tools turn pointer events into the
//! `library.footprint.*` Commands: Pads from the Pad Tool's settings, graphics on the
//! chosen Layer, moving Pads and the Designator and value text, and the origin.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars::{self, DocumentType};
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::{
    Footprint, FootprintLayer, LibraryElement, PadKind, PadShape, Shape, Uuid,
};
use reda_ui::footprint_editor::{self, PadTemplate};
use reda_ui::keymap::{self, Chord};
use reda_ui::layout::TabKey;
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

const MM: f64 = 1_000_000.0;
const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";
const STARTER_FOOTPRINT: &str = "00000030-0000-4000-8000-000000000000";

fn demo_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-engine/tests/golden/library/Demo")
}

/// A Shell with no Project open, the Demo Library as its (read-only) starter Library and
/// an empty user Library "Mine" registered in the Preferences.
fn shell(home: &TempDir) -> Shell {
    let lib = home.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let info = reda_ui::engine::model::LibraryInfo {
        uuid: LIBRARY.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_ui::engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home.path().join("home"))))
        .starter_library(Some(demo_library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    shell
}

fn new_footprint(shell: &mut Shell) -> Uuid {
    let out = shell
        .run(
            "library.element.new",
            json!({"library": LIBRARY, "kind": "footprint", "name": "Test"}),
        )
        .unwrap();
    out["element"].as_str().unwrap().parse().unwrap()
}

fn footprint(shell: &Shell, uuid: Uuid) -> Footprint {
    match shell.engine().element(uuid) {
        Some(LibraryElement::Footprint(f)) => f.clone(),
        other => panic!("no open Footprint: {other:?}"),
    }
}

fn mm(x: f64, y: f64) -> (f64, f64) {
    (x * MM, y * MM)
}

/// Feeds `events` to the editor at 0.01 mm per point, then runs what it asked for, as the
/// window does after a frame.
fn feed(shell: &mut Shell, element: Uuid, events: &[Pointer]) -> Vec<(String, Value)> {
    let commands = footprint_editor::pointer(shell, element, events, 10_000.0);
    for (id, params) in &commands {
        shell.run(id, params.clone()).unwrap();
    }
    commands
}

fn click(at: (f64, f64)) -> Pointer {
    Pointer::Click {
        at,
        modifiers: Modifiers::NONE,
    }
}

fn drag(shell: &mut Shell, element: Uuid, from: (f64, f64), to: (f64, f64)) {
    feed(
        shell,
        element,
        &[
            Pointer::DragStart {
                from,
                at: from,
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag(to),
            Pointer::DragEnd(to),
        ],
    );
}

#[test]
fn a_new_footprint_opens_in_a_document_tab_with_no_project_open() {
    let home = TempDir::new("fpe-tab");
    let mut shell = shell(&home);
    assert!(shell.engine().project().is_none());
    let fp = new_footprint(&mut shell);

    let tab = shell.document_tab(fp).expect("the element has a tab");
    assert_eq!(tab.title, "Test");
    assert!(shell.layout().place(&TabKey::Document(fp)).is_some());
    assert_eq!(shell.active_document().map(|t| t.document), Some(fp));
    assert_eq!(bars::active_type(&shell), Some(DocumentType::Footprint));
    // Board grids: the editor starts at 0.5 mm, and G cycles it.
    assert_eq!(shell.grid(fp).0, 500_000);
    shell
        .run("view.grid.cycle", json!({ "document": fp }))
        .unwrap();
    assert_eq!(shell.grid(fp).0, 1_000_000);

    shell
        .run("library.element.close", json!({ "element": fp }))
        .unwrap();
    assert!(shell.document_tab(fp).is_none(), "closing drops the tab");
}

#[test]
fn the_pad_tool_places_pads_from_its_settings_with_the_next_number() {
    let home = TempDir::new("fpe-pad");
    let mut shell = shell(&home);
    let fp = new_footprint(&mut shell);
    shell.run("tool.activate:pad", json!({})).unwrap();
    assert_eq!(shell.tool(), Tool::Pad);

    footprint_editor::set_pad_template(
        &mut shell,
        PadTemplate {
            kind: PadKind::PlatedThroughHole,
            shape: PadShape::Oval,
            width: reda_ui::engine::model::geom::Nm(1_700_000),
            height: reda_ui::engine::model::geom::Nm(2_000_000),
            hole: Some((
                reda_ui::engine::model::geom::Nm(800_000),
                reda_ui::engine::model::geom::Nm(1_200_000),
            )),
            mask_margin: Some(reda_ui::engine::model::geom::Nm(50_000)),
            paste_margin: None,
            ..PadTemplate::default()
        },
    )
    .unwrap();
    let last = shell.engine().journal().last().unwrap().id.clone();
    assert_eq!(last, "view.footprint_tool.set");
    // Clicks snap to the 0.5 mm grid.
    feed(&mut shell, fp, &[click(mm(0.1, 0.1)), click(mm(2.6, -0.1))]);
    let f = footprint(&shell, fp);
    assert_eq!(f.pads.len(), 2);
    let mut pads = f.pads.clone();
    pads.sort_by_key(|p| p.at.x);
    assert_eq!(
        (pads[0].number.as_str(), pads[1].number.as_str()),
        ("1", "2")
    );
    assert_eq!((pads[0].at.x.0, pads[0].at.y.0), (0, 0));
    assert_eq!((pads[1].at.x.0, pads[1].at.y.0), (2_500_000, 0));
    let pad = &pads[1];
    assert_eq!(pad.kind, PadKind::PlatedThroughHole);
    assert_eq!(pad.shape, PadShape::Oval);
    let hole = pad.hole.unwrap();
    assert_eq!(
        (hole.width.0, hole.height.0),
        (800_000, 1_200_000),
        "a slot"
    );
    assert_eq!(pad.mask_margin.map(|m| m.0), Some(50_000));

    // Undo takes back one Pad: each placement is one step on the element's stack.
    shell.undo(false, None).unwrap();
    assert_eq!(footprint(&shell, fp).pads.len(), 1);
}

#[test]
fn graphics_are_drawn_on_the_chosen_layer_top_or_bottom() {
    let home = TempDir::new("fpe-graphics");
    let mut shell = shell(&home);
    let fp = new_footprint(&mut shell);
    for (tool, layer) in [
        ("line", FootprintLayer::SilkscreenTop),
        ("rect", FootprintLayer::CourtyardBottom),
        ("circle", FootprintLayer::AssemblyTop),
    ] {
        footprint_editor::set_layer(&mut shell, layer).unwrap();
        shell
            .run(&format!("tool.activate:{tool}"), json!({}))
            .unwrap();
        drag(&mut shell, fp, mm(-1.0, -1.0), mm(1.0, 2.0));
    }
    let f = footprint(&shell, fp);
    assert_eq!(f.graphics.len(), 3);
    let find = |layer| f.graphics.iter().find(|g| g.layer == layer).unwrap();
    assert!(matches!(
        find(FootprintLayer::SilkscreenTop).shape,
        Shape::Line { .. }
    ));
    let Shape::Rect { start, end } = find(FootprintLayer::CourtyardBottom).shape else {
        panic!("a rectangle");
    };
    assert_eq!(
        (start.x.0, start.y.0, end.x.0, end.y.0),
        (-1_000_000, -1_000_000, 1_000_000, 2_000_000)
    );
    let Shape::Circle { center, radius } = find(FootprintLayer::AssemblyTop).shape else {
        panic!("a circle");
    };
    assert_eq!((center.x.0, center.y.0), (-1_000_000, -1_000_000));
    assert_eq!(radius.0, 3_605_551, "centre to the release point");
}

#[test]
fn dragging_moves_pads_and_the_designator_text_by_the_grid() {
    let home = TempDir::new("fpe-move");
    let mut shell = shell(&home);
    let fp = new_footprint(&mut shell);
    shell.run("tool.activate:pad", json!({})).unwrap();
    // Clear of the Designator and value text, which a new Footprint has at its origin.
    feed(&mut shell, fp, &[click(mm(0.0, 3.0))]);
    shell.run("tool.activate:select", json!({})).unwrap();

    drag(&mut shell, fp, mm(0.1, 3.0), mm(1.1, 3.4));
    let pad = footprint(&shell, fp).pads[0].clone();
    assert_eq!((pad.at.x.0, pad.at.y.0), (1_000_000, 3_500_000));
    assert_eq!(shell.selection().objects, vec![pad.uuid]);

    let text = |shell: &Shell, field: &str| {
        footprint(shell, fp)
            .texts
            .iter()
            .find(|t| t.field == field)
            .unwrap()
            .clone()
    };
    let value = text(&shell, "Value").uuid;
    shell
        .run(
            "library.footprint.text.edit",
            json!({"footprint": fp, "text": value, "at": {"x": 0, "y": "-3mm"}}),
        )
        .unwrap();
    let designator = text(&shell, "Designator");
    let d = designator.at;
    drag(
        &mut shell,
        fp,
        (d.x.0 as f64, d.y.0 as f64),
        (d.x.0 as f64, d.y.0 as f64 + 2.0 * MM),
    );
    assert_eq!(text(&shell, "Designator").at.y.0, d.y.0 + 2_000_000);
    assert_eq!(shell.selection().objects, vec![designator.uuid]);

    // Delete removes texts and Pads, through the Keymap binding.
    shell
        .run("selection.set", json!({"objects": [pad.uuid, value]}))
        .unwrap();
    let keymap = shell.keymap().clone();
    let action = keymap
        .lookup(&Chord::parse("Del").unwrap(), &keymap::active(&shell))
        .expect("Del is bound on a Footprint")
        .clone();
    let resolved = bars::resolve(&shell, &action);
    let (id, params) = resolved.run.unwrap();
    shell.run(&id, params).unwrap();
    let f = footprint(&shell, fp);
    assert!(f.pads.is_empty());
    assert!(f.texts.iter().all(|t| t.uuid != value));
}

#[test]
fn the_origin_tool_moves_the_origin_to_the_click() {
    let home = TempDir::new("fpe-origin");
    let mut shell = shell(&home);
    let fp = new_footprint(&mut shell);
    shell.run("tool.activate:pad", json!({})).unwrap();
    feed(&mut shell, fp, &[click(mm(2.0, 1.0))]);
    shell.run("tool.activate:origin", json!({})).unwrap();
    feed(&mut shell, fp, &[click(mm(2.0, 1.0))]);
    let pad = footprint(&shell, fp).pads[0].clone();
    assert_eq!(
        (pad.at.x.0, pad.at.y.0),
        (0, 0),
        "the Pad is now the origin"
    );
    assert_eq!(shell.tool(), Tool::Select, "the origin Tool is one-shot");
}

#[test]
fn starter_footprints_open_read_only_and_tools_change_nothing() {
    let home = TempDir::new("fpe-starter");
    let mut shell = shell(&home);
    let element: Uuid = STARTER_FOOTPRINT.parse().unwrap();
    shell
        .run("library.element.open", json!({ "element": element }))
        .unwrap();
    let tab = shell.document_tab(element).unwrap();
    assert!(tab.title.ends_with("(read-only)"), "{}", tab.title);
    assert!(reda_ui::element_editor::read_only(shell.engine(), element));
    let before = footprint(&shell, element);
    shell.run("tool.activate:pad", json!({})).unwrap();
    let commands = feed(&mut shell, element, &[click(mm(5.0, 5.0))]);
    assert!(commands.is_empty(), "{commands:?}");
    assert_eq!(footprint(&shell, element), before);

    // Save is refused; the Library check still runs and reports to Messages.
    assert!(
        shell
            .run("library.element.save", json!({ "element": element }))
            .is_err()
    );
    reda_ui::element_editor::check(&mut shell, element).unwrap();
}

#[test]
fn save_writes_the_footprint_and_the_check_reports_a_missing_courtyard() {
    let home = TempDir::new("fpe-save");
    let mut shell = shell(&home);
    let fp = new_footprint(&mut shell);
    shell.run("tool.activate:pad", json!({})).unwrap();
    feed(&mut shell, fp, &[click(mm(0.0, 0.0))]);

    reda_ui::element_editor::check(&mut shell, fp).unwrap();
    let texts: Vec<String> = shell
        .message_groups()
        .iter()
        .flat_map(|g| g.lines.iter().map(|l| l.text.clone()))
        .collect();
    assert!(texts.iter().any(|t| t.contains("courtyard")), "{texts:?}");

    // Mod+S saves the Footprint in its own tab.
    let keymap = shell.keymap().clone();
    let action = keymap
        .lookup(&Chord::parse("Mod+S").unwrap(), &keymap::active(&shell))
        .unwrap()
        .clone();
    let (id, params) = bars::resolve(&shell, &action).run.unwrap();
    shell.run(&id, params).unwrap();
    assert!(home.path().join("lib/footprints/Test.redafp").exists());
}

#[test]
fn open_footprint_tabs_stay_when_a_project_opens() {
    let home = TempDir::new("fpe-project");
    let mut shell = shell(&home);
    let fp = new_footprint(&mut shell);
    shell.new_project(home.path(), "Demo").unwrap();
    assert!(shell.document_tab(fp).is_some());
    assert!(shell.layout().place(&TabKey::Document(fp)).is_some());
}
