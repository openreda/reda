//! The PCB view, through the Shell with no window: the Layer state the Layers panel and
//! the view keys change (visibility, the active Layer, the Inactive-Layer mode, the view
//! from the bottom, per-type visibility, the Ratsnest), units and the snap grid. Each
//! change is a view Command, never touches the Journal's undo or dirties the board, and
//! the Layer state is kept in `view.json`.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::model::{LayerKind, ObjectType, Side, Uuid};
use reda_ui::engine::preferences::{InactiveLayerMode, Units};
use reda_ui::engine::render::{DIMMED_OPACITY, SceneLayer};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Board {
    shell: Shell,
    pcb: Uuid,
    net: Uuid,
    footprints: [Uuid; 2],
    project: PathBuf,
    _home: TempDir,
    _work: TempDir,
}

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build()
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// A Project whose board has two one-Pad Footprints on Net SIG, with the PCB open.
fn board() -> Board {
    let home = TempDir::new("pcb-view-home");
    let work = TempDir::new("pcb-view-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Board").unwrap();
    let net = uuid(&run(&mut shell, "test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    let mut footprint = |x: &str| {
        let out = run(
            &mut shell,
            "test.footprint.add",
            json!({"at": {"x": x, "y": 0}, "pads": [
                {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}
            ]}),
        );
        uuid(&out["result"]["footprint"])
    };
    let footprints = [footprint("0mm"), footprint("10mm")];
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    let project = work.path().join("Board/Board.reda");
    Board {
        shell,
        pcb,
        net,
        footprints,
        project,
        _home: home,
        _work: work,
    }
}

fn layer(b: &Board, kind: LayerKind, side: Option<Side>) -> Uuid {
    let project = b.shell.engine().project().unwrap();
    project
        .pcb()
        .unwrap()
        .layers()
        .find(kind, side)
        .unwrap()
        .uuid
}

/// The Layers the PCB canvas composites, bottom first, with their opacity.
fn composites(b: &Board) -> Vec<(SceneLayer, f32)> {
    let tab = b.shell.document_tab(b.pcb).unwrap();
    let layers = tab
        .scene
        .layers
        .as_ref()
        .expect("a board composites its Layers");
    layers.iter().map(|c| (c.layer, c.opacity)).collect()
}

fn drawn(b: &Board, layer: Uuid) -> bool {
    composites(b)
        .iter()
        .any(|(l, _)| *l == SceneLayer::Board(layer))
}

#[test]
fn hiding_a_layer_drops_it_from_the_canvas_and_view_json_keeps_it() {
    let mut b = board();
    let silk = layer(&b, LayerKind::Silkscreen, Some(Side::Top));
    run(&mut b.shell, "project.save", json!({}));
    let entries = b.shell.engine().journal().len();
    assert!(drawn(&b, silk));

    run(
        &mut b.shell,
        "pcb.layer.show",
        json!({"layer": silk, "visible": false}),
    );

    assert!(!drawn(&b, silk));
    assert!(
        b.shell
            .document_tab(b.pcb)
            .unwrap()
            .layers
            .hidden
            .contains(&silk)
    );
    assert!(
        !b.shell.engine().is_dirty(),
        "viewing never dirties the board"
    );
    assert_eq!(b.shell.engine().journal().len(), entries + 1);

    b.shell.save_view();
    let mut again = Shell::new(engine_in(b._home.path()));
    again.open_project(&b.project).unwrap();
    assert!(
        again
            .document_tab(b.pcb)
            .unwrap()
            .layers
            .hidden
            .contains(&silk)
    );
    let again_layers = again
        .document_tab(b.pcb)
        .unwrap()
        .scene
        .layers
        .clone()
        .unwrap();
    assert!(
        !again_layers
            .iter()
            .any(|c| c.layer == SceneLayer::Board(silk))
    );

    // Leaving `visible` out toggles.
    run(&mut b.shell, "pcb.layer.show", json!({"layer": silk}));
    assert!(drawn(&b, silk));
}

#[test]
fn plus_and_minus_step_through_copper_and_star_swaps_sides() {
    let mut b = board();
    let top = layer(&b, LayerKind::Copper, Some(Side::Top));
    let bottom = layer(&b, LayerKind::Copper, Some(Side::Bottom));
    let active = |b: &Board| {
        b.shell
            .board_view()
            .active_layer(b.shell.engine().project().unwrap())
    };
    assert_eq!(active(&b), Some(top), "the top copper is active at first");

    run(&mut b.shell, "pcb.layer.step:next", json!({}));
    assert_eq!(active(&b), Some(bottom));
    run(&mut b.shell, "pcb.layer.step:next", json!({}));
    assert_eq!(active(&b), Some(top), "stepping wraps around");
    run(&mut b.shell, "pcb.layer.step:previous", json!({}));
    assert_eq!(active(&b), Some(bottom));
    run(&mut b.shell, "pcb.layer.swap_side", json!({}));
    assert_eq!(active(&b), Some(top));

    let silk = layer(&b, LayerKind::Silkscreen, Some(Side::Bottom));
    run(&mut b.shell, "pcb.layer.activate", json!({"layer": silk}));
    assert_eq!(active(&b), Some(silk));
    let last_board = composites(&b)
        .into_iter()
        .rev()
        .find(|(l, _)| matches!(l, SceneLayer::Board(_)))
        .unwrap();
    assert_eq!(
        last_board,
        (SceneLayer::Board(silk), 1.0),
        "the active Layer draws last"
    );
    run(&mut b.shell, "pcb.layer.swap_side", json!({}));
    assert_eq!(
        active(&b),
        Some(layer(&b, LayerKind::Silkscreen, Some(Side::Top))),
        "a sided Layer swaps to its twin"
    );
}

#[test]
fn shift_s_cycles_the_inactive_layer_mode() {
    let mut b = board();
    let silk = layer(&b, LayerKind::Silkscreen, Some(Side::Top));
    let mode = |b: &Board| b.shell.document_tab(b.pcb).unwrap().layers.inactive_mode;
    assert_eq!(mode(&b), None, "unset follows the Preference");

    run(&mut b.shell, "pcb.view.inactive_layers", json!({}));
    assert_eq!(mode(&b), Some(InactiveLayerMode::Dimmed));
    assert!(composites(&b).contains(&(SceneLayer::Board(silk), DIMMED_OPACITY)));

    run(&mut b.shell, "pcb.view.inactive_layers", json!({}));
    assert_eq!(mode(&b), Some(InactiveLayerMode::Hidden));
    assert!(!drawn(&b, silk), "single-Layer mode hides inactive Layers");

    run(&mut b.shell, "pcb.view.inactive_layers", json!({}));
    assert_eq!(mode(&b), Some(InactiveLayerMode::Normal));

    run(
        &mut b.shell,
        "pcb.view.inactive_layers",
        json!({"mode": "hidden"}),
    );
    assert_eq!(mode(&b), Some(InactiveLayerMode::Hidden));
}

#[test]
fn the_view_from_the_bottom_mirrors_the_canvas_and_reverses_the_layers() {
    let mut b = board();
    let top = layer(&b, LayerKind::Copper, Some(Side::Top));
    let bottom = layer(&b, LayerKind::Copper, Some(Side::Bottom));
    let index = |b: &Board, l: Uuid| {
        composites(b)
            .iter()
            .position(|(c, _)| *c == SceneLayer::Board(l))
            .unwrap()
    };
    let documentation = layer(&b, LayerKind::Documentation, None);
    run(
        &mut b.shell,
        "pcb.layer.activate",
        json!({"layer": documentation}),
    );
    assert!(index(&b, bottom) < index(&b, top));

    run(&mut b.shell, "pcb.view.from_bottom", json!({}));

    assert!(b.shell.document_tab(b.pcb).unwrap().layers.from_bottom);
    assert!(index(&b, top) < index(&b, bottom));
    run(
        &mut b.shell,
        "pcb.view.from_bottom",
        json!({"from_bottom": false}),
    );
    assert!(!b.shell.document_tab(b.pcb).unwrap().layers.from_bottom);
}

#[test]
fn object_types_hide_from_the_canvas() {
    let mut b = board();
    let [f1, _] = b.footprints;
    let shows = |b: &Board| {
        let tab = b.shell.document_tab(b.pcb).unwrap();
        tab.scene.scene.items_of(f1).next().is_some()
    };
    assert!(shows(&b));

    run(
        &mut b.shell,
        "pcb.view.object_type.show",
        json!({"type": "footprint_instance", "visible": false}),
    );

    assert!(!shows(&b));
    let hidden = &b.shell.document_tab(b.pcb).unwrap().layers.hidden_types;
    assert!(hidden.contains(&ObjectType::FootprintInstance));
    let err = b
        .shell
        .run(
            "pcb.view.object_type.show",
            json!({"type": "unit", "visible": false}),
        )
        .unwrap_err();
    assert_eq!(err.details.unwrap()["pointer"], "/type");
}

#[test]
fn the_ratsnest_hides_per_net_and_as_a_whole() {
    let mut b = board();
    let ratsnest = |b: &Board| {
        let tab = b.shell.document_tab(b.pcb).unwrap();
        tab.scene
            .scene
            .items
            .iter()
            .filter(|i| i.layer == SceneLayer::Ratsnest)
            .count()
    };
    let composited = |b: &Board| {
        composites(b)
            .iter()
            .any(|(l, _)| *l == SceneLayer::Ratsnest)
    };
    assert_eq!(ratsnest(&b), 1, "two Pads of SIG, one line");
    assert!(composited(&b));

    run(
        &mut b.shell,
        "pcb.ratsnest.show",
        json!({"net": b.net, "visible": false}),
    );
    assert_eq!(ratsnest(&b), 0);
    run(&mut b.shell, "pcb.ratsnest.show", json!({"net": b.net}));
    assert_eq!(ratsnest(&b), 1);

    run(&mut b.shell, "pcb.ratsnest.show", json!({"visible": false}));
    assert!(!composited(&b));
    run(&mut b.shell, "pcb.ratsnest.show", json!({}));
    assert!(composited(&b));
}

#[test]
fn ctrl_q_toggles_units_and_g_cycles_the_grid_presets() {
    let mut b = board();
    let pcb = b.pcb;
    let view = |b: &Board| (b.shell.board_view().units, b.shell.grid(b.pcb));
    assert_eq!(view(&b), (Units::Mm, Nm::from_um(500)));

    // G: the board's grid is per Document, like a sheet's, and cycles within its unit.
    let mut seen = Vec::new();
    for _ in 0..4 {
        run(&mut b.shell, "view.grid.cycle", json!({ "document": pcb }));
        seen.push(view(&b).1);
    }
    assert_eq!(
        seen,
        [
            Nm::from_mm(1),
            Nm::from_um(100),
            Nm::from_um(250),
            Nm::from_um(500)
        ]
    );

    run(&mut b.shell, "view.units.toggle", json!({}));
    assert_eq!(
        view(&b),
        (Units::Mil, Nm(25 * 25_400)),
        "0.5 mm becomes 25 mil"
    );
    run(&mut b.shell, "view.grid.cycle", json!({ "document": pcb }));
    assert_eq!(view(&b).1, Nm(50 * 25_400));
    // Picking a preset in the other unit switches the board's units too.
    run(
        &mut b.shell,
        "view.grid.set",
        json!({ "document": pcb, "grid": "0.1mm" }),
    );
    assert_eq!(view(&b), (Units::Mm, Nm::from_um(100)));
    run(
        &mut b.shell,
        "view.grid.set",
        json!({ "document": pcb, "grid": "5mil" }),
    );
    assert_eq!(view(&b), (Units::Mil, Nm(5 * 25_400)));
    run(&mut b.shell, "view.units.toggle", json!({}));
    assert_eq!(view(&b), (Units::Mm, Nm::from_um(100)));

    let err = b
        .shell
        .run("view.grid.set", json!({ "document": pcb, "grid": "0.3mm" }))
        .unwrap_err();
    assert_eq!(err.details.unwrap()["pointer"], "/grid");
    // A sheet keeps its own grid and presets.
    let sheet = b
        .shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .find(|d| d.uuid != pcb)
        .unwrap()
        .uuid;
    assert_eq!(b.shell.grid(sheet), Nm(50 * 25_400));
    assert!(
        b.shell
            .run(
                "view.grid.set",
                json!({ "document": sheet, "grid": "5mil" })
            )
            .is_err(),
        "5 mil is a board preset only"
    );
}

#[test]
fn a_layer_colour_preference_reaches_the_canvas() {
    use reda_ui::engine::render::{BoardColor, ColorKey, Rgba, Theme};
    let mut b = board();
    let key = ColorKey::Board(BoardColor::CopperTop);
    let before = b.shell.document_tab(b.pcb).unwrap().palette.clone();
    assert_ne!(
        before.palette(Theme::Dark).color(key),
        Rgba::rgb(0x12, 0x34, 0x56)
    );

    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "display.layer_colors.dark.copper_top", "value": "#123456"}),
    );

    let palette = b.shell.document_tab(b.pcb).unwrap().palette.clone();
    assert_ne!(palette.generation, before.generation);
    assert_eq!(
        palette.palette(Theme::Dark).color(key),
        Rgba::rgb(0x12, 0x34, 0x56)
    );
    assert_eq!(
        palette.palette(Theme::Light).color(key),
        before.palette(Theme::Light).color(key),
        "per theme"
    );
}
