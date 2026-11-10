//! Cross-select through the Shell with no window (docs/spec/v0.1/sync.md#cross-select-and-net-highlighting):
//! picking Units really selects their Footprints (and back) in pick order, one Footprint
//! per Component; a box-select goes in natural Designator order; edit Commands act only
//! on the active Document's objects; the Dim, Zoom, Focus and Reposition Preferences;
//! `view.cross_probe` and its jump option; Select Footprints of This Sheet.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::keymap::{self, Chord};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

/// A Shell on a new Project in `dir`, with its Preferences under `home`.
fn shell(dir: &TempDir, home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").expect("project.new");
    shell
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

/// Places `part` at (`x`, 0) mm; returns its Component's and Unit's UUIDs.
fn place(shell: &mut Shell, part: &str, x: i64) -> (Uuid, Uuid) {
    let out = run(
        shell,
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": {"x": format!("{x}mm"), "y": 0}}),
    );
    (
        uuid(&out["result"]["component"]["uuid"]),
        uuid(&out["result"]["unit"]),
    )
}

fn sync(shell: &mut Shell) {
    let list = run(shell, "sync.preview", json!({}));
    run(shell, "sync.apply", json!({"hash": list["hash"]}));
}

fn pcb(shell: &Shell) -> Uuid {
    shell.engine().project().unwrap().pcb().unwrap().uuid
}

fn footprint_of(shell: &Shell, component: Uuid) -> Uuid {
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    pcb.footprints()
        .find(|f| f.component.as_ref().is_some_and(|c| c.uuid == component))
        .expect("a synced Footprint")
        .uuid
}

fn exists(shell: &Shell, object: Uuid) -> bool {
    shell.engine().project().unwrap().object(object).is_some()
}

fn select(shell: &mut Shell, objects: &[Uuid]) {
    run(shell, "selection.set", json!({ "objects": objects }));
}

/// The Command key `key` runs on `document`, with its placeholders filled.
fn resolve_key(shell: &Shell, key: &str, document: Uuid) -> (String, Value) {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(document));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    match bars::resolve_for(shell, &action, Some(document)).run {
        Ok(command) => command,
        Err(why) => panic!("{key} cannot run: {why}"),
    }
}

/// R1 and R2 and the two Units of U1 (an LM358), synced to the board.
struct Fixture {
    shell: Shell,
    r1: (Uuid, Uuid),
    r2: (Uuid, Uuid),
    u1: Uuid,
    u1a: Uuid,
    u1b: Uuid,
    _dirs: (TempDir, TempDir),
}

fn fixture(name: &str) -> Fixture {
    let dir = TempDir::new(name);
    let home = TempDir::new(&format!("{name}-home"));
    let mut shell = shell(&dir, &home);
    let r1 = place(&mut shell, R_10K, 0);
    let r2 = place(&mut shell, R_10K, 10);
    let (u1, u1a) = place(&mut shell, LM358, 30);
    let u1b = uuid(
        &run(
            &mut shell,
            "schematic.unit.place",
            json!({"document": SHEET, "component": u1, "at": {"x": "60mm", "y": 0}}),
        )["result"]["unit"],
    );
    sync(&mut shell);
    Fixture {
        shell,
        r1,
        r2,
        u1,
        u1a,
        u1b,
        _dirs: (dir, home),
    }
}

#[test]
fn picked_units_really_select_one_footprint_per_component_in_pick_order() {
    let mut f = fixture("cross-select-order");
    let (fp_r1, fp_r2, fp_u1) = (
        footprint_of(&f.shell, f.r1.0),
        footprint_of(&f.shell, f.r2.0),
        footprint_of(&f.shell, f.u1),
    );
    // R2, then U1's second Unit, then R1, then U1's first Unit.
    select(&mut f.shell, &[f.r2.1, f.u1b, f.r1.1, f.u1a]);
    assert_eq!(f.shell.selection().mirrored, [fp_r2, fp_u1, fp_r1]);
    // Board Commands act on what was picked on the schematic, in that order.
    let board = pcb(&f.shell);
    let (id, params) = resolve_key(&f.shell, "Del", board);
    assert_eq!(id, "pcb.copper.delete");
    assert_eq!(params["objects"], json!([fp_r2, fp_u1, fp_r1]));
    // The canvases draw them selected.
    for tab in f.shell.document_tabs() {
        for o in [fp_r1, fp_r2, fp_u1, f.r1.1] {
            assert!(tab.selection.objects.contains(&o));
        }
    }

    // A Footprint picked on the board selects every Unit of its Component.
    select(&mut f.shell, &[fp_u1]);
    assert_eq!(f.shell.selection().mirrored, [f.u1a, f.u1b]);
    // Switched off, nothing is mirrored.
    run(
        &mut f.shell,
        "view.cross_select.toggle",
        json!({"on": false}),
    );
    assert!(f.shell.selection().mirrored.is_empty());
}

#[test]
fn delete_on_a_sheet_never_deletes_the_mirrored_footprints() {
    let mut f = fixture("cross-select-delete");
    let fp_r1 = footprint_of(&f.shell, f.r1.0);
    select(&mut f.shell, &[f.r1.1]);
    let (id, params) = resolve_key(&f.shell, "Del", sheet());
    assert_eq!(params["objects"], json!([f.r1.1]));
    run(&mut f.shell, &id, params);
    assert!(!exists(&f.shell, f.r1.1));
    assert!(exists(&f.shell, fp_r1), "the board is not changed");

    // And on the board, Lock (from the Edit menu) acts on the mirrored Footprint only.
    select(&mut f.shell, &[f.r2.1]);
    let fp_r2 = footprint_of(&f.shell, f.r2.0);
    let lock = bars::Action {
        id: "edit.lock".into(),
        binding: bars::Binding::Raw {
            command: "pcb.lock".into(),
            params: json!({"objects": "$selection", "locked": true}),
        },
        title: None,
        icon: None,
    };
    let (id, params) = bars::resolve_for(&f.shell, &lock, Some(pcb(&f.shell)))
        .run
        .unwrap();
    assert_eq!(params["objects"], json!([fp_r2]));
    run(&mut f.shell, &id, params);
    let project = f.shell.engine().project().unwrap();
    assert!(project.pcb().unwrap().footprint(fp_r2).unwrap().locked);
}

#[test]
fn a_single_box_select_goes_in_natural_designator_order() {
    let dir = TempDir::new("cross-select-box");
    let home = TempDir::new("cross-select-box-home");
    let mut shell = shell(&dir, &home);
    let a = place(&mut shell, R_10K, 0);
    let b = place(&mut shell, R_10K, 10);
    let c = place(&mut shell, R_10K, 20);
    for ((component, _), designator) in [(a, "R10"), (b, "R2"), (c, "R1")] {
        run(
            &mut shell,
            "schematic.component.set",
            json!({"components": [component], "designator": designator}),
        );
    }
    sync(&mut shell);
    run(&mut shell, "document.open", json!({"document": SHEET}));
    let (id, params) = reda_ui::selection::picking::box_selection(
        &shell,
        sheet(),
        (-20e6, -20e6),
        (40e6, 20e6),
        egui::Modifiers::NONE,
    )
    .expect("a box-select");
    run(&mut shell, &id, params);
    // R1, R2, R10: neither placement nor text order.
    assert_eq!(shell.selection().objects, [c.1, b.1, a.1]);
    let footprints: Vec<Uuid> = [c, b, a]
        .iter()
        .map(|(component, _)| footprint_of(&shell, *component))
        .collect();
    assert_eq!(shell.selection().mirrored, footprints);
    let out = run(&mut shell, "pcb.place.reposition", json!({}));
    assert_eq!(out["footprints"], json!(footprints));
}

fn pref(shell: &mut Shell, key: &str, value: Value) {
    run(
        shell,
        "app.preferences.set",
        json!({"key": key, "value": value}),
    );
}

fn active(shell: &Shell) -> Option<Uuid> {
    shell.active_document().map(|t| t.document)
}

/// Shows the board beside the sheet: both canvases visible, the sheet active.
fn side_by_side(shell: &mut Shell) {
    let board = pcb(shell);
    run(shell, "document.open", json!({"document": SHEET}));
    run(shell, "document.open", json!({"document": board}));
    run(
        shell,
        "layout.tab.split",
        json!({"tab": board.to_string(), "side": "right"}),
    );
    run(shell, "layout.tab.activate", json!({"tab": SHEET}));
    assert_eq!(active(shell), Some(sheet()));
}

#[test]
fn the_cross_select_preferences_default_to_dim_and_focus_on_zoom_and_reposition_off() {
    let f = fixture("cross-select-prefs");
    let prefs = &f.shell.preferences().cross_select;
    assert!(prefs.dim);
    assert!(!prefs.zoom);
    assert!(prefs.focus);
    assert!(!prefs.reposition);
}

#[test]
fn dim_marks_the_mirrored_objects_so_the_rest_of_their_canvas_dims() {
    let mut f = fixture("cross-select-dim");
    let fp_r1 = footprint_of(&f.shell, f.r1.0);
    let board = pcb(&f.shell);
    run(&mut f.shell, "document.open", json!({"document": board}));
    select(&mut f.shell, &[f.r1.1]);
    assert_eq!(f.shell.cross_probe(), [fp_r1]);

    pref(&mut f.shell, "cross_select.dim", json!(false));
    assert!(
        f.shell.cross_probe().is_empty(),
        "nothing highlighted, nothing dims"
    );
    // Still really selected.
    assert_eq!(f.shell.selection().mirrored, [fp_r1]);
    let tab = f.shell.document_tab(board).unwrap();
    assert!(tab.selection.objects.contains(&fp_r1));
    assert!(tab.selection.highlighted.is_empty());
}

#[test]
fn zoom_frames_the_other_sides_canvas_only_when_switched_on() {
    let mut f = fixture("cross-select-zoom");
    let board = pcb(&f.shell);
    run(&mut f.shell, "document.open", json!({"document": board}));
    select(&mut f.shell, &[f.r1.1]);
    assert!(f.shell.document_tab(board).unwrap().zoom_to.is_none());

    pref(&mut f.shell, "cross_select.zoom", json!(true));
    select(&mut f.shell, &[f.r2.1]);
    assert!(f.shell.document_tab(board).unwrap().zoom_to.is_some());
}

#[test]
fn focus_moves_to_the_other_document_only_when_it_is_visible() {
    let mut f = fixture("cross-select-focus");
    let board = pcb(&f.shell);
    // One tab group: the board is not visible, so the sheet keeps the focus.
    run(&mut f.shell, "document.open", json!({"document": board}));
    run(&mut f.shell, "document.open", json!({"document": SHEET}));
    select(&mut f.shell, &[f.r1.1]);
    assert_eq!(active(&f.shell), Some(sheet()));

    side_by_side(&mut f.shell);
    select(&mut f.shell, &[f.r2.1]);
    assert_eq!(active(&f.shell), Some(board));

    pref(&mut f.shell, "cross_select.focus", json!(false));
    run(&mut f.shell, "layout.tab.activate", json!({"tab": SHEET}));
    select(&mut f.shell, &[f.r1.1]);
    assert_eq!(active(&f.shell), Some(sheet()));
}

#[test]
fn reposition_floats_a_picked_units_footprint_on_the_pcb_cursor_skipping_locked_ones() {
    let mut f = fixture("cross-select-reposition");
    let (fp_r1, fp_r2) = (
        footprint_of(&f.shell, f.r1.0),
        footprint_of(&f.shell, f.r2.0),
    );
    side_by_side(&mut f.shell);
    pref(&mut f.shell, "cross_select.focus", json!(false));
    // Off by default.
    select(&mut f.shell, &[f.r1.1]);
    assert_eq!(f.shell.pcb_tool().carried(), None);

    pref(&mut f.shell, "cross_select.reposition", json!(true));
    run(
        &mut f.shell,
        "pcb.lock",
        json!({"objects": [fp_r1], "locked": true}),
    );
    select(&mut f.shell, &[f.r1.1, f.r2.1]);
    assert_eq!(f.shell.pcb_tool().carried(), Some(&[fp_r2][..]));
}

#[test]
fn reposition_waits_for_the_pcb_canvas_to_be_visible() {
    let mut f = fixture("cross-select-reposition-hidden");
    let board = pcb(&f.shell);
    run(&mut f.shell, "document.open", json!({"document": board}));
    run(&mut f.shell, "document.open", json!({"document": SHEET}));
    pref(&mut f.shell, "cross_select.reposition", json!(true));
    select(&mut f.shell, &[f.r1.1]);
    assert_eq!(f.shell.pcb_tool().carried(), None);
}

#[test]
fn cross_probe_always_highlights_and_zooms_and_its_jump_activates_the_other_tab() {
    let mut f = fixture("cross-probe");
    let board = pcb(&f.shell);
    let fp_r1 = footprint_of(&f.shell, f.r1.0);
    run(&mut f.shell, "document.open", json!({"document": board}));
    run(&mut f.shell, "document.open", json!({"document": SHEET}));
    // Even with Dim and Zoom off for cross-select.
    pref(&mut f.shell, "cross_select.dim", json!(false));
    bars::take_requests(f.shell.engine());
    select(&mut f.shell, &[f.r1.1]);
    let (id, params) = resolve_key(&f.shell, "Mod+Shift+C", sheet());
    let out = run(&mut f.shell, &id, params);
    assert_eq!(out["objects"], json!([fp_r1]));
    assert_eq!(f.shell.highlight().objects, [fp_r1]);
    assert!(f.shell.cross_probe().contains(&fp_r1), "the rest dims");
    let requests = bars::take_requests(f.shell.engine());
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, bars::Request::ZoomClose(d, o, c) if *d == board && *o == [fp_r1] && *c == 0.6)),
        "zoomed as close as the shared highlight Preference says: {requests:?}"
    );
    assert_eq!(active(&f.shell), Some(sheet()), "no jump: the sheet stays");

    // The shared highlight Preference governs Cross-probe: with Dim and Zoom off it only
    // finds.
    pref(&mut f.shell, "editing.highlight.dim", json!(false));
    pref(&mut f.shell, "editing.highlight.zoom", json!(false));
    run(&mut f.shell, "view.highlight.clear", json!({}));
    let out = run(
        &mut f.shell,
        "view.cross_probe",
        json!({"objects": [f.r1.1]}),
    );
    assert_eq!(out["objects"], json!([fp_r1]));
    assert!(f.shell.highlight().objects.is_empty());
    assert!(
        !bars::take_requests(f.shell.engine())
            .iter()
            .any(|r| matches!(r, bars::Request::ZoomClose(..)))
    );
    pref(&mut f.shell, "editing.highlight.dim", json!(true));
    pref(&mut f.shell, "editing.highlight.zoom", json!(true));

    run(
        &mut f.shell,
        "view.cross_probe",
        json!({"objects": [f.r1.1], "jump": true}),
    );
    assert_eq!(active(&f.shell), Some(board));
}

#[test]
fn mod_shift_x_toggles_cross_select_and_mod_shift_r_toggles_reposition() {
    let mut f = fixture("cross-select-keys");
    let (id, params) = resolve_key(&f.shell, "Mod+Shift+X", sheet());
    run(&mut f.shell, &id, params);
    assert!(!f.shell.cross_probe_switches().selection);
    select(&mut f.shell, &[f.r1.1]);
    assert!(f.shell.selection().mirrored.is_empty());

    let board = pcb(&f.shell);
    let (id, params) = resolve_key(&f.shell, "Mod+Shift+R", board);
    run(&mut f.shell, &id, params);
    assert!(f.shell.preferences().cross_select.reposition);
    let (id, params) = resolve_key(&f.shell, "Mod+Shift+R", sheet());
    run(&mut f.shell, &id, params);
    assert!(!f.shell.preferences().cross_select.reposition);
}

#[test]
fn select_footprints_of_this_sheet_selects_them_in_natural_designator_order() {
    let mut f = fixture("cross-select-sheet");
    // A Component placed after Sync has no Footprint yet.
    place(&mut f.shell, R_10K, 90);
    let out = run(
        &mut f.shell,
        "selection.footprints_of_sheet",
        json!({"document": SHEET}),
    );
    assert_eq!(out["count"], 3);
    let expected: Vec<Uuid> = [f.r1.0, f.r2.0, f.u1]
        .iter()
        .map(|c| footprint_of(&f.shell, *c))
        .collect();
    assert_eq!(f.shell.selection().objects, expected);
    let board = pcb(&f.shell);
    assert!(
        f.shell
            .run("selection.footprints_of_sheet", json!({"document": board}))
            .is_err(),
        "only a sheet has footprints of its own"
    );
}

#[test]
fn a_sheet_tab_offers_select_footprints_of_this_sheet() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use reda_ui::App;

    let f = fixture("cross-select-tab");
    let expected: Vec<Uuid> = [f.r1.0, f.r2.0, f.u1]
        .iter()
        .map(|c| footprint_of(&f.shell, *c))
        .collect();
    let mut shell = f.shell;
    run(&mut shell, "document.open", json!({"document": SHEET}));
    let mut h = Harness::builder()
        .with_size([1200.0, 800.0])
        .with_max_steps(100)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::Tab, "Sheet 1")
        .click_secondary();
    h.run();
    h.get_by_label("Select Footprints of This Sheet").click();
    h.run();
    assert_eq!(h.state().shell().selection().objects, expected);
}
