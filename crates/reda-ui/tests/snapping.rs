//! The one snapping model on a board, through the Shell with no window: the Snap Distance
//! on both axes, the object snap modes, Gridless, the Hotspot checklist (kept in
//! `.reda/view.json`), and `Mod` / `Mod`+Shift suspending object snap and the grid in
//! flight. Sheets' Electrical Range is in `schematic_wiring.rs`.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{LayerKind, Side, Uuid};
use reda_ui::shell::Shell;
use reda_ui::snap::{Hit, Hotspot};
use serde_json::{Value, json};

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
    let home = TempDir::new("snapping-home");
    let work = TempDir::new("snapping-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Snap").unwrap();
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
    fn hover(&mut self, at: Point) -> (Point, Hit) {
        let s = self.shell.pcb_hover(at);
        (s.at, s.hit)
    }

    fn kind(&mut self, at: Point) -> Option<Hotspot> {
        self.shell.pcb_hover(at).hit.hotspot()
    }

    fn copper(&self, side: Side) -> Uuid {
        let pcb = self.shell.engine().project().unwrap().pcb().unwrap();
        pcb.layers()
            .find(LayerKind::Copper, Some(side))
            .unwrap()
            .uuid
    }

    fn snap_set(&mut self, params: Value) -> Value {
        let mut params = params;
        params["document"] = json!(self.pcb);
        run(&mut self.shell, "view.snap.set", params)
    }

    fn keys(&mut self, modifiers: Modifiers) {
        let pcb = self.pcb;
        self.shell.hold_snap_keys(pcb, modifiers, false);
    }
}

#[test]
fn a_board_captures_within_the_snap_distance_on_both_axes() {
    let mut b = board();
    // 8 mil is 0.2032 mm: 0.19 mm off on both axes (0.27 mm in a straight line) snaps.
    assert_eq!(
        b.hover(mm(10.19, 0.19)),
        (
            mm(10.0, 0.0),
            Hit::Hotspot {
                kind: Hotspot::PadCenter,
                object: b
                    .shell
                    .engine()
                    .project()
                    .unwrap()
                    .pcb()
                    .unwrap()
                    .footprints()
                    .next()
                    .unwrap()
                    .uuid
            }
        )
    );
    // 0.21 mm off on one axis does not.
    assert_eq!(b.hover(mm(10.21, 0.0)), (mm(10.0, 0.0), Hit::Grid));
    assert_eq!(b.hover(mm(10.0, 0.4)), (mm(10.0, 0.5), Hit::Grid));
    // The Snap Distance is a Preference.
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "pcb.snap_distance", "value": "20mil"}),
    );
    assert_eq!(b.kind(mm(10.4, 0.4)), Some(Hotspot::PadCenter));
}

#[test]
fn the_object_snap_mode_picks_layers_and_off_leaves_the_grid() {
    let mut b = board();
    let top = b.copper(Side::Top);
    let bottom = b.copper(Side::Bottom);
    run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": top, "points": [{"x": "20.1mm", "y": "0.1mm"}, {"x": "25mm", "y": "0.1mm"}]}),
    );
    run(
        &mut b.shell,
        "pcb.via.add",
        json!({"at": {"x": "30.1mm", "y": "0.1mm"}}),
    );
    // Active Layer (the default) on top copper: the Track end and the Via.
    assert_eq!(b.kind(mm(20.0, 0.0)), Some(Hotspot::TrackEnd));
    assert_eq!(b.kind(mm(30.0, 0.0)), Some(Hotspot::ViaCenter));
    // On bottom copper the Track is on another Layer; the Via is on every copper Layer.
    run(&mut b.shell, "pcb.layer.activate", json!({"layer": bottom}));
    assert_eq!(b.hover(mm(20.0, 0.0)), (mm(20.0, 0.0), Hit::Grid));
    assert_eq!(b.kind(mm(30.0, 0.0)), Some(Hotspot::ViaCenter));
    // All Layers: the Track end again.
    b.snap_set(json!({"mode": "all_layers"}));
    assert_eq!(b.kind(mm(20.0, 0.0)), Some(Hotspot::TrackEnd));
    // Off: only the grid.
    b.snap_set(json!({"mode": "off"}));
    assert_eq!(b.hover(mm(30.0, 0.0)), (mm(30.0, 0.0), Hit::Grid));
}

#[test]
fn gridless_turns_the_grid_level_off_and_hotspots_keep_working() {
    let mut b = board();
    b.snap_set(json!({"gridless": true}));
    assert_eq!(b.hover(mm(3.123, 4.567)), (mm(3.123, 4.567), Hit::Free));
    assert_eq!(b.kind(mm(10.1, 0.1)), Some(Hotspot::PadCenter));
}

#[test]
fn the_hotspot_checklist_turns_kinds_off_and_is_kept_in_view_json() {
    let mut b = board();
    let out = b.snap_set(json!({"hotspots": {"pad_center": false}}));
    assert_eq!(out["hotspots_off"], json!(["pad_center"]));
    // The Footprint's origin, under its Pad here, is a Hotspot of its own.
    assert_eq!(b.kind(mm(10.1, 0.1)), Some(Hotspot::FootprintOrigin));
    b.snap_set(json!({"hotspots": {"footprint_origin": false}}));
    assert_eq!(b.hover(mm(10.1, 0.1)), (mm(10.0, 0.0), Hit::Grid));
    // A kind the Document type does not offer is refused.
    let err = b
        .shell
        .run(
            "view.snap.set",
            json!({"document": b.pcb, "hotspots": {"pin_end": false}}),
        )
        .unwrap_err();
    assert_eq!(err.details, Some(json!({"pointer": "/hotspots/pin_end"})));

    b.shell.save_view();
    let folder = b.work.path().join("Snap");
    let saved: Value =
        serde_json::from_slice(&std::fs::read(folder.join(".reda/view.json")).unwrap()).unwrap();
    assert_eq!(
        saved["documents"][b.pcb.to_string()]["hotspots_off"],
        json!(["footprint_origin", "pad_center"])
    );
    let out = b.snap_set(json!({"hotspots": {"pad_center": true}}));
    assert_eq!(out["hotspots_off"], json!(["footprint_origin"]));
}

#[test]
fn the_preferences_give_new_views_their_checklist_and_mode() {
    let mut b = board();
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "pcb.hotspots_off", "value": ["via_center"]}),
    );
    let out = b.snap_set(json!({}));
    assert_eq!(out["hotspots_off"], json!(["via_center"]));
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "pcb.electrical_snap", "value": false}),
    );
    let out = b.snap_set(json!({}));
    assert_eq!(out["mode"], json!("off"));
}

#[test]
fn mod_suspends_object_snap_and_mod_shift_the_grid_in_flight() {
    let mut b = board();
    // The line Tool is an operation in flight.
    run(&mut b.shell, "tool.activate:line", json!({}));
    b.keys(Modifiers::NONE);
    assert_eq!(b.kind(mm(10.1, 0.1)), Some(Hotspot::PadCenter));
    b.keys(Modifiers::COMMAND);
    assert_eq!(b.hover(mm(10.1, 0.1)), (mm(10.0, 0.0), Hit::Grid));
    b.keys(Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(b.hover(mm(10.1, 0.1)), (mm(10.1, 0.1), Hit::Free));
    b.keys(Modifiers::NONE);
    assert_eq!(b.kind(mm(10.1, 0.1)), Some(Hotspot::PadCenter));
}

#[test]
fn mod_held_when_the_operation_starts_means_detach_until_pressed_again() {
    let mut b = board();
    // Mod is already down before the Tool starts.
    b.keys(Modifiers::COMMAND);
    run(&mut b.shell, "tool.activate:line", json!({}));
    b.keys(Modifiers::COMMAND);
    assert_eq!(b.kind(mm(10.1, 0.1)), Some(Hotspot::PadCenter));
    b.keys(Modifiers::NONE);
    b.keys(Modifiers::COMMAND);
    assert_eq!(b.hover(mm(10.1, 0.1)), (mm(10.0, 0.0), Hit::Grid));
    // Out of flight (back to Select, nothing carried), Mod suspends nothing.
    run(&mut b.shell, "tool.activate:select", json!({}));
    b.keys(Modifiers::COMMAND);
    assert_eq!(b.kind(mm(10.1, 0.1)), Some(Hotspot::PadCenter));
}

#[test]
fn a_board_offers_every_kind_on_its_checklist() {
    let mut b = board();
    b.snap_set(json!({"mode": "all_layers"}));
    run(
        &mut b.shell,
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "Hi", "x": "40.1mm", "y": "20.1mm"}),
    );
    run(
        &mut b.shell,
        "pcb.graphic.add_arc",
        json!({"layer": "silkscreen_top", "from": {"x": "60.1mm", "y": "0.1mm"},
               "to": {"x": "50.1mm", "y": "10.1mm"}, "sweep": "90deg"}),
    );
    run(
        &mut b.shell,
        "pcb.graphic.add_line",
        json!({"layer": "silkscreen_top", "from": {"x": "70.1mm", "y": "0.1mm"},
               "to": {"x": "75mm", "y": "0.1mm"}}),
    );
    run(
        &mut b.shell,
        "pcb.board_outline.add_rect",
        json!({"width": "100mm", "height": "80mm"}),
    );
    let corner = {
        let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
        let v = &pcb.outline().unwrap().vertices[0];
        Point::new(v.x, v.y)
    };
    assert_eq!(b.kind(mm(40.0, 20.0)), Some(Hotspot::TextAnchor));
    assert_eq!(b.kind(mm(60.0, 0.0)), Some(Hotspot::ArcEnd));
    assert_eq!(b.kind(mm(50.0, 0.0)), Some(Hotspot::ArcCenter));
    assert_eq!(b.kind(mm(70.0, 0.0)), Some(Hotspot::OutlineVertex));
    assert_eq!(b.kind(corner + mm(0.1, 0.1)), Some(Hotspot::OutlineVertex));
    assert_eq!(b.shell.pcb_hover(corner + mm(0.1, 0.1)).at, corner);
}

#[test]
fn the_palette_command_counts_requests_per_document() {
    let mut b = board();
    let pcb = b.pcb;
    run(&mut b.shell, "view.snap.palette", json!({"document": pcb}));
    run(&mut b.shell, "view.snap.palette", json!({"document": pcb}));
    let requests = b
        .shell
        .engine()
        .view()
        .get::<reda_ui::snap::PaletteRequests>()
        .unwrap();
    assert_eq!(requests.0.get(&pcb), Some(&2));
}
