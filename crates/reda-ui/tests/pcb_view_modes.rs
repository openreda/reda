//! The PCB view modes added for the board view (docs/spec/v0.1/pcb.md#view): Layer
//! sets, pinned Layers, per-kind transparency and Draft, `F5` Net colours on copper, the
//! Net label toggles, per-Footprint Ratsnest hiding, Smart Track Ends, and masking
//! highlights cleared by `view.highlight.clear`. All are view state, kept in `view.json`.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::bars::{Binding, DocumentType};
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{LayerKind, ObjectType, Side, Uuid};
use reda_ui::engine::preferences::KeymapBase;
use reda_ui::engine::render::{ColorKey, Primitive, Rgba, Scene, SceneLayer};
use reda_ui::keymap::{Active, Chord, Keymap};
use reda_ui::selection::picking;
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
    track: Uuid,
    project: PathBuf,
    home: TempDir,
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

/// Two one-Pad Footprints on Net SIG, 10 mm apart, with a Track from the left Pad 6 mm
/// towards the right one; the PCB open.
fn board() -> Board {
    let home = TempDir::new("pcb-modes-home");
    let work = TempDir::new("pcb-modes-work");
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
    let track = run(
        &mut shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net, "width": "0.5mm",
               "points": [{"x": 0, "y": 0}, {"x": "6mm", "y": 0}]}),
    );
    let track = uuid(&track["result"]["tracks"][0]);
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    let project = work.path().join("Board/Board.reda");
    Board {
        shell,
        pcb,
        net,
        footprints,
        track,
        project,
        home,
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

fn scene(b: &Board) -> &Scene {
    &b.shell.document_tab(b.pcb).unwrap().scene.scene
}

/// The board Layers the canvas composites, bottom first.
fn drawn(b: &Board) -> Vec<Uuid> {
    let tab = b.shell.document_tab(b.pcb).unwrap();
    (tab.scene.layers.as_ref().unwrap().iter())
        .filter_map(|c| match c.layer {
            SceneLayer::Board(l) => Some(l),
            _ => None,
        })
        .collect()
}

fn ratsnest(b: &Board) -> Vec<(Point, Point)> {
    scene(b)
        .items
        .iter()
        .filter(|i| i.layer == SceneLayer::Ratsnest)
        .filter_map(|i| match i.primitive {
            Primitive::Hairline { a, b } => Some((a, b)),
            _ => None,
        })
        .collect()
}

fn reopen(b: &mut Board) -> Shell {
    b.shell.save_view();
    let mut again = Shell::new(engine_in(b.home.path()));
    again.open_project(&b.project).unwrap();
    again
}

fn mm(v: i64) -> Nm {
    Nm(v * 1_000_000)
}

#[test]
fn layer_sets_apply_built_in_and_saved_sets_and_saved_ones_live_in_the_preferences() {
    let mut b = board();
    let copper = [
        layer(&b, LayerKind::Copper, Some(Side::Top)),
        layer(&b, LayerKind::Copper, Some(Side::Bottom)),
    ];
    let outline = layer(&b, LayerKind::BoardOutline, None);
    let silk_top = layer(&b, LayerKind::Silkscreen, Some(Side::Top));
    run(&mut b.shell, "project.save", json!({}));

    let out = run(
        &mut b.shell,
        "pcb.view.layer_set.apply",
        json!({"set": "signal"}),
    );
    assert_eq!(out["layers"].as_array().unwrap().len(), 2, "{out}");
    let mut shown = drawn(&b);
    shown.sort();
    let mut want = copper.to_vec();
    want.sort();
    assert_eq!(shown, want);

    run(
        &mut b.shell,
        "pcb.view.layer_set.apply",
        json!({"set": "top"}),
    );
    assert!(drawn(&b).contains(&silk_top) && drawn(&b).contains(&outline));
    assert!(!drawn(&b).contains(&copper[1]));

    // Save the shown Layers by kind: copper top + bottom and the outline.
    run(
        &mut b.shell,
        "pcb.view.layer_set.apply",
        json!({"set": "all"}),
    );
    for l in b
        .shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .layers()
        .layers
        .clone()
    {
        let visible = copper.contains(&l.uuid) || l.uuid == outline;
        run(
            &mut b.shell,
            "pcb.layer.show",
            json!({"layer": l.uuid, "visible": visible}),
        );
    }
    let saved = run(
        &mut b.shell,
        "pcb.view.layer_set.save",
        json!({"name": "Routing"}),
    );
    assert_eq!(
        saved["layers"],
        json!(["copper_top", "copper_bottom", "board_outline"])
    );
    let prefs = reda_ui::engine::preferences::current(Some(&UserDirs::under(b.home.path())));
    assert_eq!(prefs.pcb.view.layer_sets[0].name, "Routing");

    run(
        &mut b.shell,
        "pcb.view.layer_set.apply",
        json!({"set": "all"}),
    );
    assert!(drawn(&b).contains(&silk_top));
    run(
        &mut b.shell,
        "pcb.view.layer_set.apply",
        json!({"set": "Routing"}),
    );
    assert_eq!(drawn(&b).len(), 3);
    assert!(!drawn(&b).contains(&silk_top));

    let refused = b
        .shell
        .run("pcb.view.layer_set.save", json!({"name": "fab"}))
        .unwrap_err();
    assert!(refused.message.contains("built-in"), "{refused:?}");
    run(
        &mut b.shell,
        "pcb.view.layer_set.delete",
        json!({"name": "Routing"}),
    );
    assert!(
        b.shell
            .run("pcb.view.layer_set.apply", json!({"set": "Routing"}))
            .is_err()
    );
    assert!(
        !b.shell.engine().is_dirty(),
        "viewing never dirties the board"
    );
}

#[test]
fn a_pinned_layer_stays_in_single_layer_mode_and_view_json_keeps_the_pin() {
    let mut b = board();
    let outline = layer(&b, LayerKind::BoardOutline, None);
    run(
        &mut b.shell,
        "pcb.view.inactive_layers",
        json!({"mode": "hidden"}),
    );
    assert!(!drawn(&b).contains(&outline));
    let out = run(&mut b.shell, "pcb.layer.pin", json!({"layer": outline}));
    assert_eq!(out["pinned"], true);
    assert!(drawn(&b).contains(&outline));

    let again = reopen(&mut b);
    let tab = again.document_tab(b.pcb).unwrap();
    assert!(tab.layers.pinned.contains(&outline));

    run(&mut b.shell, "pcb.layer.pin", json!({"layer": outline}));
    assert!(!drawn(&b).contains(&outline), "toggled off");
}

#[test]
fn object_type_set_draws_a_kind_transparent_or_in_draft_and_view_json_keeps_it() {
    let mut b = board();
    let out = run(
        &mut b.shell,
        "pcb.view.object_type.set",
        json!({"type": "track", "transparency": 60}),
    );
    assert_eq!(out, json!({"transparency": 60, "draft": false}));
    let track: Vec<_> = scene(&b).items_of(b.track).collect();
    assert!(!track.is_empty());
    assert!(track.iter().all(|i| i.look.transparency == 60));
    let pads: Vec<_> = scene(&b).items_of(b.footprints[0]).collect();
    assert!(
        pads.iter().all(|i| i.look.transparency == 0),
        "other kinds stay"
    );

    run(
        &mut b.shell,
        "pcb.view.object_type.set",
        json!({"type": "track", "draft": true}),
    );
    let track: Vec<_> = scene(&b).items_of(b.track).collect();
    assert!(
        track.iter().any(|i| matches!(
            i.primitive,
            Primitive::Rect {
                stroke: Some(_),
                ..
            }
        )),
        "the Track is drawn as its outline"
    );
    assert!(
        !track
            .iter()
            .any(|i| matches!(i.primitive, Primitive::Capsule { .. }))
    );

    let again = reopen(&mut b);
    let kinds = &again.document_tab(b.pcb).unwrap().layers.kinds;
    let track = kinds[&ObjectType::Track];
    assert_eq!((track.transparency, track.draft), (60, true));
    assert!(
        b.shell
            .run(
                "pcb.view.object_type.set",
                json!({"type": "track", "transparency": 101})
            )
            .is_err()
    );
}

#[test]
fn f5_puts_net_colours_on_copper_and_net_labels_toggle() {
    let mut b = board();
    let (keymap, _) = Keymap::load(KeymapBase::Reda, None);
    let pcb = Active {
        tool: None,
        document: Some(DocumentType::Pcb),
        part: false,
    };
    let action = keymap.lookup(&Chord::parse("F5").unwrap(), &pcb).unwrap();
    assert!(matches!(&action.binding, Binding::Command(id) if id == "pcb.view.net_colors"));

    run(
        &mut b.shell,
        "pcb.net.set_color",
        json!({"nets": [b.net], "color": "#ff0000"}),
    );
    let red = ColorKey::Net(Rgba::rgb(0xff, 0, 0));
    let on_track = |b: &Board| scene(b).items_of(b.track).any(|i| i.color == red);
    assert!(!on_track(&b));
    assert!(
        scene(&b)
            .items
            .iter()
            .any(|i| i.layer == SceneLayer::Ratsnest && i.color == red)
    );
    assert_eq!(
        run(&mut b.shell, "pcb.view.net_colors", json!({}))["on"],
        true
    );
    assert!(on_track(&b));
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "pcb.view.net_color_pattern", "value": "checker"}),
    );
    assert!(
        scene(&b)
            .items_of(b.track)
            .any(|i| i.color == red && i.look.checker)
    );
    run(&mut b.shell, "pcb.view.net_colors", json!({}));
    assert!(!on_track(&b));

    let names = |b: &Board| -> usize {
        scene(b)
            .items_of(b.footprints[0])
            .filter(|i| matches!(&i.primitive, Primitive::Glyphs(r) if r.text == "SIG"))
            .count()
    };
    assert_eq!(names(&b), 1);
    let out = run(&mut b.shell, "pcb.view.labels", json!({"pad_nets": false}));
    assert_eq!(
        out,
        json!({"pad_nets": false, "via_nets": true, "repeated_net_names": false})
    );
    assert_eq!(names(&b), 0);
    let again = reopen(&mut b);
    assert!(!again.document_tab(b.pcb).unwrap().layers.labels.pad_nets);
}

#[test]
fn a_footprints_ratsnest_hides_and_lines_start_at_track_ends_unless_smart_track_ends_is_off() {
    let mut b = board();
    // Smart Track Ends: from the Track end at 6 mm to the right Pad at 10 mm.
    let lines = ratsnest(&b);
    assert_eq!(lines.len(), 1);
    let ends = [lines[0].0, lines[0].1];
    assert!(ends.contains(&Point::new(mm(6), mm(0))), "{ends:?}");
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": "pcb.ratsnest.smart_track_ends", "value": false}),
    );
    let lines = ratsnest(&b);
    let ends = [lines[0].0, lines[0].1];
    assert!(
        ends.contains(&Point::new(mm(0), mm(0))),
        "from the Pad: {ends:?}"
    );

    let out = run(
        &mut b.shell,
        "pcb.ratsnest.show",
        json!({"footprints": [b.footprints[1]], "visible": false}),
    );
    assert_eq!(out["visible"], false);
    assert!(ratsnest(&b).is_empty());
    let again = reopen(&mut b);
    let tab = again.document_tab(b.pcb).unwrap();
    assert!(
        tab.layers
            .ratsnest
            .hidden_footprints
            .contains(&b.footprints[1])
    );
    run(
        &mut b.shell,
        "pcb.ratsnest.show",
        json!({"footprints": [b.footprints[1]]}),
    );
    assert_eq!(ratsnest(&b).len(), 1, "toggled back");
}

#[test]
fn a_masking_highlight_makes_the_rest_unpickable_and_shift_c_clears_it_but_not_the_selection() {
    let mut b = board();
    let right = (10_000_000.0, 0.0);
    let hit = |b: &Board, at| picking::hit(&b.shell, b.pcb, at, 100_000.0);
    // The right Footprint's Pad.
    let pad = hit(&b, right).unwrap();
    assert!(!b.shell.is_masked(b.pcb, pad, Some(b.footprints[1])));

    // Dimmed objects can still be picked.
    run(
        &mut b.shell,
        "view.highlight",
        json!({"objects": [b.footprints[0]]}),
    );
    assert_eq!(hit(&b, right), Some(pad));

    // Masked ones can't.
    run(
        &mut b.shell,
        "view.highlight",
        json!({"objects": [b.footprints[0]], "mask": true}),
    );
    assert!(b.shell.is_masked(b.pcb, b.footprints[1], None));
    assert!(b.shell.is_masked(b.pcb, pad, Some(b.footprints[1])));
    assert!(!b.shell.is_masked(b.pcb, b.footprints[0], None));
    assert_eq!(hit(&b, right), None);
    assert!(
        hit(&b, (0.0, 0.0)).is_some(),
        "the highlighted Footprint still picks"
    );

    run(
        &mut b.shell,
        "selection.set",
        json!({"objects": [b.footprints[0]]}),
    );
    let (keymap, _) = Keymap::load(KeymapBase::Reda, None);
    let global = Active {
        tool: None,
        document: None,
        part: false,
    };
    let action = keymap
        .lookup(&Chord::parse("Shift+C").unwrap(), &global)
        .unwrap();
    assert!(matches!(&action.binding, Binding::Command(id) if id == "view.highlight.clear"));
    run(&mut b.shell, "view.highlight.clear", json!({}));
    assert!(b.shell.highlight().objects.is_empty());
    assert!(!b.shell.highlight().mask);
    assert!(b.shell.cross_probe().is_empty());
    assert_eq!(hit(&b, right), Some(pad));
    assert_eq!(
        b.shell.selection().objects,
        [b.footprints[0]],
        "the selection stays"
    );
}

#[test]
fn board_markers_sit_where_the_check_put_them_even_after_the_objects_change() {
    use reda_ui::messages::CheckKind;
    let mut b = board();
    let other =
        uuid(&run(&mut b.shell, "test.net.add", json!({"name": "OTHER"}))["result"]["uuid"]);
    let track = run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": other, "width": "0.25mm",
               "points": [{"x": "3mm", "y": "0.5mm"}, {"x": "3mm", "y": "5mm"}]}),
    );
    let track = uuid(&track["result"]["tracks"][0]);
    b.shell.run_check(CheckKind::Drc).unwrap();
    let run_ = b.shell.messages().run(CheckKind::Drc).unwrap();
    let placed: Vec<Point> = run_
        .entries
        .iter()
        .filter(|e| e.violation.document == Some(b.pcb))
        .filter_map(|e| e.violation.at)
        .collect();
    assert!(!placed.is_empty(), "the crossing Track breaks clearance");
    let markers =
        |b: &Board| -> Vec<Point> { b.shell.markers(b.pcb).iter().map(|m| m.at).collect() };
    assert_eq!(markers(&b), placed, "each marker is the check's `at`");

    // The left Footprint moves away and the crossing Track goes: the stale markers stay
    // where the check put them.
    run(
        &mut b.shell,
        "pcb.move",
        json!({"objects": [b.footprints[0]], "by": {"x": "-4mm", "y": "3mm"}}),
    );
    run(&mut b.shell, "pcb.track.delete", json!({"tracks": [track]}));
    assert_eq!(markers(&b), placed);
}
