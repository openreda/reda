//! Moving Footprints through the Shell with no window (docs/spec/v0.1/pcb.md#moving-footprints,
//! #finding-and-queueing, #locks): `R` and `avoid`, the live overlap, alignment guides,
//! `N` and the Ratsnest in a move, Reposition Selected with Skip, Choose Component, and
//! Protect Locked Objects.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{PcbDocument, Uuid};
use reda_ui::engine::render::{Primitive, SceneLayer};
use reda_ui::pcb::placing::{self, PlaceFlight};
use reda_ui::shell::Shell;
use reda_ui::snap::Guide;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

const TOLERANCE: Nm = Nm::from_um(200);

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
    pcb: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// A new Project with its board open on a 0.5 mm grid.
fn board(name: &str) -> Board {
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
        _work: work,
    }
}

impl Board {
    /// A Footprint with one 2 × 2 mm Pad (on `net` if given) at `at`.
    fn footprint(&mut self, designator: &str, at: Point, net: Option<&Value>) -> Uuid {
        let mut pad = json!({"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm"});
        if let Some(net) = net {
            pad["net"] = net.clone();
        }
        run(
            &mut self.shell,
            "test.footprint.add",
            json!({"at": {"x": at.x.0, "y": at.y.0}, "designator": designator, "pads": [pad]}),
        )["result"]["footprint"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn net(&mut self, name: &str) -> Value {
        run(&mut self.shell, "test.net.add", json!({"name": name}))["result"]["uuid"].clone()
    }

    fn doc(&self) -> &PcbDocument {
        self.shell.engine().project().unwrap().pcb().unwrap()
    }

    fn at(&self, f: Uuid) -> Point {
        self.doc().footprint(f).unwrap().at
    }

    fn select(&mut self, objects: &[Uuid]) {
        run(
            &mut self.shell,
            "selection.set",
            json!({ "objects": objects }),
        );
    }

    fn pref(&mut self, key: &str, value: Value) {
        run(
            &mut self.shell,
            "app.preferences.set",
            json!({"key": key, "value": value}),
        );
    }

    fn click(&mut self, at: Point) {
        self.shell.pcb_hover(at);
        self.shell.pcb_press(at, false, TOLERANCE);
        self.shell.pcb_release();
    }

    /// Presses on `from` and drags to `to`, without letting go.
    fn hold(&mut self, from: Point, to: Point) {
        self.shell.pcb_press(from, false, TOLERANCE);
        self.shell.pcb_drag(to);
    }

    fn flight(&self) -> PlaceFlight {
        placing::flight(&self.shell)
    }

    fn journal(&self) -> Vec<String> {
        self.shell
            .engine()
            .journal()
            .iter()
            .map(|e| e.id.clone())
            .collect()
    }

    /// The Ratsnest lines the canvas draws now, as (from, to).
    fn ratsnest(&self) -> Vec<(Point, Point)> {
        let tab = self.shell.document_tab(self.pcb).unwrap();
        tab.scene
            .scene
            .items
            .iter()
            .filter(|i| i.layer == SceneLayer::Ratsnest)
            .filter_map(|i| match i.primitive {
                Primitive::Hairline { a, b } => Some((a, b)),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn r_cycles_ignore_and_avoid_and_avoid_stops_short_of_courtyard_clearance() {
    let mut b = board("place-r");
    let a = b.footprint("R1", mm(0.0, 0.0), None);
    let _c = b.footprint("R2", mm(10.0, 0.0), None);
    // Only with Footprints selected or moving.
    let err = b
        .shell
        .run("pcb.place.mode", json!({"mode": "obstacle_mode"}))
        .unwrap_err();
    assert!(err.message.contains("only for Footprints"), "{err}");

    b.select(&[a]);
    run(&mut b.shell, "pcb.place.mode:obstacle_mode", json!({}));
    assert_eq!(
        b.shell.preferences().pcb.place.obstacle_mode,
        reda_ui::engine::pcb::place::ObstacleMode::Avoid
    );
    // In `avoid` the drag stops before the other's box (9 mm to 11 mm): a's box ends at
    // +1 mm, so it stops at 8 mm at most, whatever the cursor does.
    b.hold(mm(0.0, 0.0), mm(20.0, 0.0));
    let (overlaps, _) = b.shell.pcb_tool().overlaps();
    assert!(overlaps.is_empty());
    b.shell.pcb_release();
    let x = b.at(a).x;
    assert!(x <= mm(8.0, 0.0).x && x > mm(7.9, 0.0).x, "{x:?}");

    // R again: `ignore`, which goes anywhere.
    run(&mut b.shell, "pcb.place.mode:obstacle_mode", json!({}));
    assert_eq!(
        b.shell.preferences().pcb.place.obstacle_mode,
        reda_ui::engine::pcb::place::ObstacleMode::Ignore
    );
    let from = b.at(a);
    b.hold(from, mm(20.0, 0.0));
    b.shell.pcb_release();
    assert_eq!(b.at(a), mm(20.0, 0.0));
}

#[test]
fn courtyards_too_close_show_as_the_live_overlap_in_every_mode() {
    let mut b = board("place-overlap");
    let a = b.footprint("R1", mm(0.0, 0.0), None);
    let c = b.footprint("R2", mm(10.0, 0.0), None);
    b.hold(mm(0.0, 0.0), mm(9.0, 0.0));
    let (overlaps, outlines) = b.shell.pcb_tool().overlaps();
    assert_eq!(overlaps.len(), 1);
    assert_eq!((overlaps[0].moved, overlaps[0].other), (a, c));
    assert_eq!(
        outlines.len(),
        2,
        "both Courtyards (here fallback boxes) are drawn"
    );
    assert!(b.flight().moving, "the Placement Commands see the move");
    b.shell.pcb_drag(mm(3.0, 0.0));
    assert!(b.shell.pcb_tool().overlaps().0.is_empty());
    b.shell.pcb_release();
    assert!(!b.flight().moving);
}

#[test]
fn a_footprint_move_snaps_across_an_alignment_guide_to_a_neighbours_pad_centre() {
    let mut b = board("place-guides");
    let a = b.footprint("R1", mm(0.0, 0.0), None);
    // Off the 0.5 mm grid, so only the guide puts a there.
    let _c = b.footprint("R2", mm(30.0, 7.3), None);
    b.hold(mm(0.0, 0.0), mm(20.0, 7.35));
    assert_eq!(b.shell.pcb_tool().guide_lines(), [Guide::Y(mm(0.0, 7.3).y)]);
    b.shell.pcb_release();
    assert_eq!(b.at(a), mm(20.0, 7.3));

    // Turned off (the Placement Bar's toggle), the grid wins.
    b.select(&[a]);
    run(&mut b.shell, "pcb.place.mode:alignment_guides", json!({}));
    assert!(!b.shell.preferences().pcb.place.alignment_guides);
    b.hold(mm(20.0, 7.3), mm(10.0, 7.35));
    assert!(b.shell.pcb_tool().guide_lines().is_empty());
    b.shell.pcb_release();
    assert_eq!(b.at(a), mm(10.0, 7.5));
}

#[test]
fn n_cycles_the_ratsnest_in_a_move_and_breaks_hides_the_other_lines_by_default() {
    let mut b = board("place-ratsnest");
    let sig = b.net("SIG");
    let other = b.net("OTHER");
    let _a = b.footprint("R1", mm(0.0, 0.0), Some(&sig));
    let c = b.footprint("R2", mm(10.0, 0.0), Some(&sig));
    let d = b.footprint("R3", mm(20.0, 0.0), Some(&sig));
    b.footprint("R4", mm(0.0, 10.0), Some(&other));
    b.footprint("R5", mm(10.0, 10.0), Some(&other));
    b.shell.redraw(b.pcb);
    assert_eq!(b.ratsnest().len(), 3);

    // Breaks (the default): only the line the move changes, d's to c.
    b.hold(mm(20.0, 0.0), mm(20.0, -5.0));
    assert_eq!(b.ratsnest(), [(mm(10.0, 0.0), mm(20.0, -5.0))]);
    // N: Hidden.
    run(&mut b.shell, "pcb.place.mode:ratsnest_mode", json!({}));
    assert_eq!(
        b.shell.preferences().pcb.place.ratsnest_mode,
        reda_ui::engine::pcb::place::RatsnestMode::Hidden
    );
    assert!(b.ratsnest().is_empty());
    // N: Pad To Pad, from the moved Pad to the nearest on its Net.
    run(&mut b.shell, "pcb.place.mode:ratsnest_mode", json!({}));
    assert_eq!(b.ratsnest(), [(mm(20.0, -5.0), mm(10.0, 0.0))]);
    // N: back to Breaks.
    run(&mut b.shell, "pcb.place.mode:ratsnest_mode", json!({}));
    assert_eq!(b.ratsnest().len(), 1);
    b.shell.pcb_release();
    assert_eq!(b.at(d), mm(20.0, -5.0));
    // Every line is back once the move is down.
    assert_eq!(b.ratsnest().len(), 3);
    let _ = c;
}

#[test]
fn reposition_selected_brings_footprints_in_pick_order_with_skip_and_esc() {
    let mut b = board("place-queue");
    let a = b.footprint("R1", mm(0.0, 0.0), None);
    let c = b.footprint("R2", mm(10.0, 0.0), None);
    let d = b.footprint("R3", mm(20.0, 0.0), None);
    let locked = b.footprint("R4", mm(30.0, 0.0), None);
    run(
        &mut b.shell,
        "pcb.lock",
        json!({"objects": [locked], "locked": true}),
    );
    // Picked c, then a, then d; the locked one stays put.
    b.select(&[c, a, locked, d]);
    let out = run(&mut b.shell, "pcb.place.reposition", json!({}));
    assert_eq!(out["footprints"], json!([c, a, d]));
    assert_eq!(b.flight().queue, Some((1, 3)));
    assert_eq!(b.shell.pcb_tool().carried(), Some(&[c][..]));

    // c floats on the cursor; a click places it and brings a.
    b.click(mm(0.0, 40.0));
    assert_eq!(b.at(c), mm(0.0, 40.0));
    assert_eq!(b.flight().queue, Some((2, 3)));
    assert_eq!(b.shell.pcb_tool().carried(), Some(&[a][..]));
    // Space turns the one on the cursor.
    run(&mut b.shell, "tool.rotate", json!({}));
    // Tab skips a: it stays where it was, unturned.
    b.shell.pcb_hover(mm(5.0, 40.0));
    run(&mut b.shell, "pcb.place.skip", json!({}));
    assert_eq!(b.at(a), mm(0.0, 0.0));
    assert_eq!(b.doc().footprint(a).unwrap().rotation.0, 0);
    assert_eq!(b.flight().queue, Some((3, 3)));
    // Esc stops, leaving d where it is.
    b.shell.pcb_hover(mm(9.0, 40.0));
    run(&mut b.shell, "tool.activate", json!({"tool": "select"}));
    assert_eq!(b.at(d), mm(20.0, 0.0));
    assert_eq!(b.flight().queue, None);
    assert!(b.shell.pcb_tool().carried().is_none());
    assert!(
        b.shell
            .run("pcb.place.skip", json!({}))
            .unwrap_err()
            .message
            .contains("only during Reposition Selected")
    );
}

#[test]
fn move_on_empty_space_opens_choose_component_which_floats_the_chosen_one() {
    let mut b = board("place-choose");
    let r1 = b.footprint("R1", mm(0.0, 0.0), None);
    let r10 = b.footprint("R10", mm(10.0, 0.0), None);
    let c1 = b.footprint("C1", mm(20.0, 0.0), None);
    let pcb = b.doc().clone();
    let names = |filter: &str| -> Vec<String> {
        placing::choices(&pcb, filter)
            .into_iter()
            .map(|(d, _)| d)
            .collect()
    };
    assert_eq!(names(""), ["C1", "R1", "R10"]);
    assert_eq!(names("r1"), ["R1", "R10"]);
    assert_eq!(names("R?"), ["R1"]);
    assert_eq!(names("*1"), ["C1", "R1"]);

    // M needs no selection on the board; a click on nothing opens the dialog.
    run(&mut b.shell, "tool.activate", json!({"tool": "move"}));
    b.click(mm(50.0, 50.0));
    assert_eq!(b.flight().choose, Some(String::new()));
    // Move component to cursor: R10 floats on the cursor, and a click puts it down.
    let out = run(
        &mut b.shell,
        "pcb.place.pick_up",
        json!({"footprint": r10, "jump": false}),
    );
    assert_eq!(out["jump"], false);
    assert_eq!(b.flight().choose, None);
    assert_eq!(b.shell.pcb_tool().carried(), Some(&[r10][..]));
    b.click(mm(40.0, 40.0));
    assert_eq!(b.at(r10), mm(40.0, 40.0));
    assert_eq!(b.shell.tool(), Tool::Select, "Move ends once it is down");

    // Jump to component: the view goes there and C1 floats from where it is.
    run(&mut b.shell, "tool.activate", json!({"tool": "move"}));
    run(&mut b.shell, "pcb.place.choose", json!({}));
    run(
        &mut b.shell,
        "pcb.place.pick_up",
        json!({"footprint": c1, "jump": true}),
    );
    assert!(b.journal().contains(&"view.zoom_to".to_owned()));
    assert_eq!(b.shell.pcb_tool().carried(), Some(&[c1][..]));
    b.click(mm(25.0, 5.0));
    assert_eq!(b.at(c1), mm(25.0, 5.0));

    // With Move, a click on a Footprint picks it up and the next click puts it down.
    run(&mut b.shell, "tool.activate", json!({"tool": "move"}));
    b.click(mm(0.0, 0.0));
    assert_eq!(b.shell.pcb_tool().carried(), Some(&[r1][..]));
    b.click(mm(0.0, 30.0));
    assert_eq!(b.at(r1), mm(0.0, 30.0));
}

#[test]
fn protect_locked_objects_off_asks_before_moving_or_turning_a_locked_footprint() {
    let mut b = board("place-locked");
    let a = b.footprint("R1", mm(0.0, 0.0), None);
    run(
        &mut b.shell,
        "pcb.lock",
        json!({"objects": [a], "locked": true}),
    );
    // On (the default), a drag leaves it behind and Reposition skips it.
    b.hold(mm(0.0, 0.0), mm(5.0, 0.0));
    b.shell.pcb_release();
    assert_eq!(b.at(a), mm(0.0, 0.0));
    assert!(b.flight().confirm.is_none());

    b.pref("editing.protect_locked_objects", json!(false));
    b.hold(mm(0.0, 0.0), mm(5.0, 0.0));
    b.shell.pcb_release();
    assert_eq!(b.at(a), mm(0.0, 0.0), "nothing moves before the answer");
    let pending = b
        .flight()
        .confirm
        .unwrap_or_else(|| panic!("a question: {:?} {:?}", b.journal(), b.shell.message()));
    assert_eq!(pending.locked, 1);
    // No: it stays.
    run(&mut b.shell, "pcb.place.confirm", json!({"proceed": false}));
    assert_eq!(b.at(a), mm(0.0, 0.0));
    assert!(b.flight().confirm.is_none());

    b.hold(mm(0.0, 0.0), mm(5.0, 0.0));
    b.shell.pcb_release();
    run(&mut b.shell, "pcb.place.confirm", json!({"proceed": true}));
    assert_eq!(b.at(a), mm(5.0, 0.0));
    assert!(b.doc().footprint(a).unwrap().locked, "the lock is kept");

    // Space on the selected locked Footprint asks too.
    b.select(&[a]);
    run(&mut b.shell, "tool.rotate", json!({}));
    assert!(b.flight().confirm.is_some());
    assert_eq!(b.doc().footprint(a).unwrap().rotation.0, 0);
    run(&mut b.shell, "pcb.place.confirm", json!({"proceed": true}));
    assert_eq!(b.doc().footprint(a).unwrap().rotation.0, 90_000);
}

#[test]
fn reposition_reads_cross_selects_pick_order_from_selected_units() {
    let home = TempDir::new("place-cross-home");
    let work = TempDir::new("place-cross-work");
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/blinky_555");
    let mut reda = None;
    for entry in std::fs::read_dir(&from).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let target = work.path().join(path.file_name().unwrap());
            std::fs::copy(&path, &target).unwrap();
            if path.extension().is_some_and(|e| e == "reda") {
                reda = Some(target);
            }
        }
    }
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.open_project(&reda.unwrap()).unwrap();
    let project = shell.engine().project().unwrap();
    let pcb = project.pcb().unwrap();
    // Two Units with Footprints, picked in reverse UUID order.
    let mut picks: Vec<(Uuid, Uuid)> = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units())
        .filter_map(|u| {
            pcb.footprints()
                .find(|f| f.component.as_ref() == Some(&u.component) && !f.locked)
                .map(|f| (u.uuid, f.uuid))
        })
        .take(2)
        .collect();
    assert_eq!(picks.len(), 2);
    picks.reverse();
    let units: Vec<Uuid> = picks.iter().map(|(u, _)| *u).collect();
    run(&mut shell, "selection.set", json!({ "objects": units }));
    let out = run(&mut shell, "pcb.place.reposition", json!({}));
    let expected: Vec<Uuid> = picks.iter().map(|(_, f)| *f).collect();
    assert_eq!(out["footprints"], json!(expected));
}
