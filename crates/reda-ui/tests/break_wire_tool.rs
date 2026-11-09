//! The Break Wire Tool on a Shell, without a window: a click cuts as the Cutting Length
//! Preference says, Space cycles it and the options write straight back to Preferences,
//! and the canvas previews the cut with the Cutter Box and Extremity Markers (Never,
//! Always, On Wire), or highlights it with both Never. Plus Convert Cross Junctions'
//! dialog from the Design menu.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::keymap::{self, Chord};
use reda_ui::schematic_dialogs::ConvertCrossDialog;
use reda_ui::schematic_editor::{self, Scale};
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir, home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    shell
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn at(x: f64, y: f64) -> (f64, f64) {
    (x * MIL, y * MIL)
}

fn mil(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 25_400), Nm(y * 25_400))
}

fn wire(shell: &mut Shell, points: &[(i64, i64)]) -> Uuid {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mil"), "y": format!("{y}mil")}))
        .collect();
    let out = shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": points}),
        )
        .unwrap();
    out["result"]["wires"][0].as_str().unwrap().parse().unwrap()
}

/// Every Wire on the sheet as its vertices in mil, sorted.
fn wires(shell: &Shell) -> Vec<Vec<(i64, i64)>> {
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap();
    let mut all: Vec<Vec<(i64, i64)>> = sheet
        .wires()
        .map(|w| {
            (w.vertices.iter())
                .map(|p| (p.x.0 / 25_400, p.y.0 / 25_400))
                .collect()
        })
        .collect();
    all.sort();
    all
}

fn pointer(shell: &mut Shell, events: &[Pointer]) -> Vec<String> {
    let commands = schematic_editor::pointer(shell, sheet(), events, Scale::at(MIL));
    commands
        .into_iter()
        .map(|(id, params)| {
            shell.run(&id, params).unwrap();
            id
        })
        .collect()
}

fn click(shell: &mut Shell, p: (f64, f64)) -> Vec<String> {
    pointer(
        shell,
        &[Pointer::Click {
            at: p,
            modifiers: Modifiers::NONE,
        }],
    )
}

fn press(shell: &mut Shell, key: &str) {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(sheet()));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    let (id, params) = bars::resolve_for(shell, &action, Some(sheet()))
        .run
        .unwrap();
    shell.run(&id, params).unwrap();
}

fn set(shell: &mut Shell, key: &str, value: Value) {
    shell
        .run(
            "app.preferences.set",
            json!({"key": format!("schematic.break_wire.{key}"), "value": value}),
        )
        .unwrap();
}

fn activate(shell: &mut Shell) {
    shell.run("tool.activate:break_wire", json!({})).unwrap();
    assert_eq!(shell.tool(), Tool::BreakWire);
}

const RUN: [(i64, i64); 3] = [(0, 0), (1000, 0), (1000, 1000)];

#[test]
fn a_click_cuts_the_whole_segment_with_snap_to_segment() {
    let (dir, home) = (TempDir::new("break-tool"), TempDir::new("break-tool-home"));
    let mut shell = shell(&dir, &home);
    let w = wire(&mut shell, &RUN);
    activate(&mut shell);
    assert!(
        click(&mut shell, at(3000.0, 3000.0)).is_empty(),
        "no Wire there"
    );
    assert_eq!(click(&mut shell, at(500.0, 2.0)), ["schematic.wire.break"]);
    assert_eq!(wires(&shell), [vec![(1000, 0), (1000, 1000)]]);
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap();
    assert!(sheet.wire(w).is_some(), "the piece left keeps the UUID");
    assert_eq!(shell.tool(), Tool::BreakWire, "the Tool stays");
}

#[test]
fn grid_multiples_and_fixed_lengths_cut_that_long_and_space_cycles_the_preference() {
    let (dir, home) = (
        TempDir::new("break-lengths"),
        TempDir::new("break-lengths-home"),
    );
    let mut shell = shell(&dir, &home);
    wire(&mut shell, &[(0, 0), (2000, 0)]);
    activate(&mut shell);
    // Space: Snap To Segment -> Snap Grid Size Multiple, written to the Preference.
    press(&mut shell, "Space");
    let prefs = shell.preferences().schematic.break_wire.clone();
    assert_eq!(
        serde_json::to_value(prefs.cutting_length).unwrap(),
        "grid_multiple"
    );
    // Two snap grids of 50 mil, centred on the click.
    click(&mut shell, at(500.0, 0.0));
    assert_eq!(
        wires(&shell),
        [vec![(0, 0), (450, 0)], vec![(550, 0), (2000, 0)]]
    );
    set(&mut shell, "grid_multiple", json!(4));
    click(&mut shell, at(1000.0, 0.0));
    assert_eq!(
        wires(&shell),
        [
            vec![(0, 0), (450, 0)],
            vec![(550, 0), (900, 0)],
            vec![(1100, 0), (2000, 0)]
        ]
    );
    // Fixed Length: 100 mil at first.
    press(&mut shell, "Space");
    click(&mut shell, at(1500.0, 0.0));
    assert!(wires(&shell).contains(&vec![(1100, 0), (1450, 0)]));
    assert!(wires(&shell).contains(&vec![(1550, 0), (2000, 0)]));
    press(&mut shell, "Space");
    let prefs = shell.preferences().schematic.break_wire.clone();
    assert_eq!(
        serde_json::to_value(prefs.cutting_length).unwrap(),
        "segment"
    );
}

#[test]
fn the_cutter_box_and_extremity_markers_follow_the_display_preferences() {
    let (dir, home) = (
        TempDir::new("break-preview"),
        TempDir::new("break-preview-home"),
    );
    let mut shell = shell(&dir, &home);
    wire(&mut shell, &[(0, 0), (2000, 0)]);
    activate(&mut shell);
    set(&mut shell, "cutting_length", json!("fixed"));
    let cutter = |shell: &mut Shell, p: (f64, f64)| {
        pointer(shell, &[Pointer::Hover(p)]);
        shell.document_tab(sheet()).unwrap().overlay.cutter
    };
    // Always (the default) for both: over the Wire, the box spans the 100 mil gap.
    let over = cutter(&mut shell, at(500.0, 3.0)).unwrap();
    assert_eq!(over.gap, (mil(450, 0), mil(550, 0)));
    let corners = over.cutter_box.unwrap();
    assert_eq!((corners[0].x, corners[1].x), (mil(450, 0).x, mil(550, 0).x));
    assert!(
        corners[0].y != corners[3].y,
        "the box has width across the Wire"
    );
    let [first, second] = over.extremities.unwrap();
    assert_eq!((first.0.x, second.0.x), (mil(450, 0).x, mil(550, 0).x));
    assert!(!over.highlight);
    // Always also shows off the Wire; On Wire only over one.
    assert!(
        cutter(&mut shell, at(500.0, 1000.0))
            .unwrap()
            .cutter_box
            .is_some()
    );
    set(&mut shell, "show_cutter_box", json!("on_wire"));
    set(&mut shell, "show_extremity_markers", json!("on_wire"));
    assert_eq!(cutter(&mut shell, at(500.0, 1000.0)), None);
    let over = cutter(&mut shell, at(500.0, 3.0)).unwrap();
    assert!(over.cutter_box.is_some() && over.extremities.is_some());
    // Never for both: the stretch to be cut is highlighted instead.
    set(&mut shell, "show_cutter_box", json!("never"));
    set(&mut shell, "show_extremity_markers", json!("never"));
    let over = cutter(&mut shell, at(500.0, 3.0)).unwrap();
    assert!(over.highlight);
    assert_eq!((over.cutter_box, over.extremities), (None, None));
    // The cutter shrinks to fit a shorter segment.
    set(&mut shell, "fixed_length", json!("1000mil"));
    wire(&mut shell, &[(0, 3000), (200, 3000)]);
    let short = cutter(&mut shell, at(100.0, 3000.0)).unwrap();
    assert_eq!(short.gap, (mil(0, 3000), mil(200, 3000)));
}

#[test]
fn tab_opens_the_options_and_another_tool_ends_the_preview() {
    let (dir, home) = (TempDir::new("break-tab"), TempDir::new("break-tab-home"));
    let mut shell = shell(&dir, &home);
    activate(&mut shell);
    press(&mut shell, "Tab");
    let open = |shell: &Shell| {
        (shell.engine().view())
            .get::<reda_ui::tools::BreakWireTool>()
            .unwrap()
            .options_open
    };
    assert!(open(&shell));
    press(&mut shell, "Tab");
    assert!(!open(&shell));
    pointer(&mut shell, &[Pointer::Hover(at(10.0, 10.0))]);
    assert!(
        shell
            .document_tab(sheet())
            .unwrap()
            .overlay
            .cutter
            .is_some()
    );
    shell.run("tool.activate:select", json!({})).unwrap();
    pointer(&mut shell, &[Pointer::Hover(at(10.0, 10.0))]);
    assert_eq!(shell.document_tab(sheet()).unwrap().overlay.cutter, None);
}

/// A 4-way at (500, `y`): four Wires ending there. Returns them.
fn four_way(shell: &mut Shell, y: i64) -> Vec<Uuid> {
    let off = json!({"convert_cross_junctions": false});
    for points in [
        [(0, y), (500, y)],
        [(500, y), (1000, y)],
        [(500, y - 500), (500, y)],
        [(500, y), (500, y + 400)],
    ] {
        let points: Vec<Value> = (points.iter())
            .map(|(x, y)| json!({"x": format!("{x}mil"), "y": format!("{y}mil")}))
            .collect();
        shell
            .run(
                "schematic.wire.add",
                json!({"document": SHEET, "points": points, "wiring": off}),
            )
            .unwrap();
    }
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap();
    let at = mil(500, y);
    sheet
        .wires()
        .filter(|w| w.contains(at))
        .map(|w| w.uuid)
        .collect()
}

fn junction_count(shell: &mut Shell, document: Uuid) -> usize {
    let nets = shell
        .run("schematic.nets", json!({"document": document}))
        .unwrap();
    let nets = if nets.get("result").is_some() {
        &nets["result"]
    } else {
        &nets
    };
    nets["junctions"].as_array().unwrap().len()
}

#[test]
fn convert_cross_junctions_opens_from_the_design_menu_with_a_miter_of_one_snap_grid() {
    let (dir, home) = (
        TempDir::new("convert-cross"),
        TempDir::new("convert-cross-home"),
    );
    let mut shell = shell(&dir, &home);
    let menu = bars::parse(include_str!("../bars/schematic.json")).unwrap();
    let design = menu.menus.iter().find(|m| m.id == "design").unwrap();
    assert!(
        format!("{design:?}").contains("schematic.dialog.open:convert_cross_junctions"),
        "the Design menu has Convert Cross Junctions"
    );
    four_way(&mut shell, 500);
    assert_eq!(junction_count(&mut shell, sheet()), 1);
    let mut dialog = ConvertCrossDialog::new(&shell).unwrap();
    assert!(!dialog.project && !dialog.selected_only);
    assert_eq!(dialog.miter, "50 mil");
    assert_eq!(
        dialog.params().unwrap(),
        json!({"miter": 50 * 25_400, "document": SHEET})
    );
    assert_eq!(dialog.apply(&mut shell).unwrap(), 1);
    // Two adjacent 3-ways now.
    assert_eq!(junction_count(&mut shell, sheet()), 2);
}

#[test]
fn convert_cross_junctions_on_selected_objects_needs_every_wire_of_the_4_way_selected() {
    let (dir, home) = (
        TempDir::new("convert-cross-sel"),
        TempDir::new("convert-cross-sel-home"),
    );
    let mut shell = shell(&dir, &home);
    let first = four_way(&mut shell, 500);
    four_way(&mut shell, 3000);
    // Only some of the first 4-way's Wires: nothing converts.
    assert_eq!(first.len(), 4);
    shell
        .run("selection.set", json!({"objects": &first[..3]}))
        .unwrap();
    let mut dialog = ConvertCrossDialog::new(&shell).unwrap();
    dialog.selected_only = true;
    dialog.miter = "100".to_owned();
    assert_eq!(dialog.params().unwrap()["miter"], 100 * 25_400);
    assert_eq!(dialog.apply(&mut shell).unwrap(), 0);
    shell
        .run("selection.set", json!({"objects": first}))
        .unwrap();
    let mut dialog = ConvertCrossDialog::new(&shell).unwrap();
    dialog.selected_only = true;
    assert_eq!(dialog.apply(&mut shell).unwrap(), 1);
    assert_eq!(
        junction_count(&mut shell, sheet()),
        3,
        "the other 4-way stays"
    );
    // The Project scope reaches every sheet.
    let other = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let other: Uuid = other["result"]["uuid"].as_str().unwrap().parse().unwrap();
    let mut dialog = ConvertCrossDialog::new(&shell).unwrap();
    dialog.project = true;
    assert_eq!(dialog.params().unwrap().get("document"), None);
    assert_eq!(dialog.apply(&mut shell).unwrap(), 1);
    assert_eq!(junction_count(&mut shell, other), 0);
    // A Miter Size that isn't a positive length can't run.
    dialog.miter = "-5".to_owned();
    assert!(!dialog.can_apply());
}
