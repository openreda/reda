//! The Properties panel's regions in the window, driven through AccessKit with
//! `egui_kittest`: numeric expressions on a Track, the Rules region (also from right-click
//! Applicable Rules) whose rows open Board Setup, kind chips, a Pad's Revert, Fix for a
//! health check in the nothing-selected Checks region, and collapsed regions kept in
//! `state.json`.

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::layout::SettingsWindow;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn harness(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let library = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-schematic/tests/library/Test");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    shell
        .run("document.open", json!({"document": PCB}))
        .unwrap();
    Harness::builder()
        .with_size([1400.0, 2600.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()["result"].clone()
}

fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = (h.state().shell().engine().journal().iter())
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

fn track(h: &mut Harness<'_, App>, net: Option<&Value>, y: &str) -> Value {
    let mut params = json!({"layer": "copper_top",
        "points": [{"x": "0mm", "y": y}, {"x": "5mm", "y": y}]});
    if let Some(net) = net {
        params["net"] = net.clone();
    }
    run(h, "pcb.track.add", params)["tracks"][0].clone()
}

#[test]
fn a_track_width_takes_an_expression_in_the_graphical_region() {
    let (home, work) = (
        TempDir::new("props-panel-home"),
        TempDir::new("props-panel-work"),
    );
    let mut h = harness(&home, &work);
    let t = track(&mut h, None, "0mm");
    run(&mut h, "selection.set", json!({"objects": [t]}));
    h.run();
    h.get_by_label("Graphical");
    h.get_by_role_and_label(Role::TextInput, "Width").focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Width")
        .set_text_value_accesskit("! * 2");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.track.set");
    assert_eq!(params["tracks"], json!([t]));
    assert!(params["width"].as_i64().unwrap() > 0);
}

#[test]
fn applicable_rules_opens_the_rules_region_whose_rows_open_board_setup() {
    let (home, work) = (
        TempDir::new("props-rules-home"),
        TempDir::new("props-rules-work"),
    );
    let mut h = harness(&home, &work);
    let vcc = run(&mut h, "test.net.add", json!({"name": "VCC"}))["uuid"].clone();
    let power = run(
        &mut h,
        "pcb.net_class.add",
        json!({"name": "Power", "nets": [vcc]}),
    )["uuid"]
        .clone();
    run(
        &mut h,
        "pcb.rules.set",
        json!({"kind": "track_width", "scope": {"net_class": power},
               "values": {"min": "0.3mm", "preferred": "0.35mm"}}),
    );
    run(
        &mut h,
        "pcb.rules.set",
        json!({"kind": "clearance", "scope": {"net_class": power}, "values": {"value": "1mm"},
               "enabled": false}),
    );
    let t = track(&mut h, Some(&vcc), "0mm");
    run(&mut h, "selection.set", json!({"objects": [t]}));
    h.run();
    // Collapsed by the user, the region opens again for Applicable Rules.
    h.get_by_label("Rules").click();
    h.run();
    assert!(h.query_by_label_contains("Applies: min = 0.3").is_none());
    run(&mut h, "view.properties.rules", json!({}));
    h.run();
    h.run();
    h.get_by_label_contains("Applies: min = 0.3");
    h.get_by_label_contains("Overridden: Board: min");
    h.get_by_label_contains("Disabled: Power");

    h.get_by_label_contains("Applies: min = 0.3").click();
    h.run();
    h.run();
    let dialog = h
        .state()
        .settings_windows()
        .dialog(SettingsWindow::BoardSetup)
        .expect("Board Setup opens");
    assert_eq!(dialog.page_titles()[dialog.selected()], "Track Width");
}

#[test]
fn a_kind_chip_narrows_and_clear_restores_the_selection() {
    let (home, work) = (
        TempDir::new("props-chips-home"),
        TempDir::new("props-chips-work"),
    );
    let mut h = harness(&home, &work);
    let t = track(&mut h, None, "0mm");
    let v = run(&mut h, "pcb.via.add", json!({"at": {"x": "9mm", "y": 0}}))["uuid"].clone();
    run(&mut h, "selection.set", json!({"objects": [t, v]}));
    h.run();
    h.get_by_label("Via (1)").click();
    h.run();
    h.run();
    let selected = h.state().shell().selection().objects.clone();
    assert_eq!(json!(selected), json!([v]));
    h.get_by_label("Clear").click();
    h.run();
    h.run();
    let selected = h.state().shell().selection().objects.clone();
    assert_eq!(json!(selected), json!([t, v]));
}

#[test]
fn a_locally_edited_pad_reverts_from_properties() {
    let (home, work) = (
        TempDir::new("props-revert-home"),
        TempDir::new("props-revert-work"),
    );
    let mut h = harness(&home, &work);
    run(
        &mut h,
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": 0, "y": 0}}),
    );
    let hash = h
        .state_mut()
        .shell_mut()
        .run("sync.preview", json!({}))
        .unwrap()["hash"]
        .clone();
    run(&mut h, "sync.apply", json!({"hash": hash}));
    let (footprint, pad) = {
        let pcb = h.state().shell().engine().project().unwrap().pcb().unwrap();
        let f = pcb.footprints().next().unwrap();
        (
            f.uuid,
            pcb.footprint_copy(f.footprint).unwrap().footprint.pads[0].uuid,
        )
    };
    run(
        &mut h,
        "pcb.pad.edit",
        json!({"instance": footprint, "pad": pad, "width": "2mm"}),
    );
    run(
        &mut h,
        "selection.set",
        json!({"pads": [{"footprint": footprint, "pad": pad}]}),
    );
    h.run();
    h.get_by_label("Locally edited");
    h.get_by_label("Compare").click();
    h.run();
    h.get_by_label("Edited Pads over the original");
    h.get_by_label("Revert").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.footprint.revert_local_edits");
    assert_eq!(params, json!({"instances": [footprint]}));
}

#[test]
fn nothing_selected_shows_the_overview_with_fix_for_health_checks() {
    let (home, work) = (
        TempDir::new("props-overview-home"),
        TempDir::new("props-overview-work"),
    );
    let mut h = harness(&home, &work);
    track(&mut h, None, "0mm");
    track(&mut h, None, "0mm");
    run(&mut h, "selection.set", json!({}));
    h.run();
    for region in [
        "Ready for fab?",
        "Selection Filter",
        "Snap Options",
        "Board",
        "Grid",
        "Other",
        "Checks",
        "Links",
    ] {
        assert!(
            h.query_all_by_label(region).next().is_some(),
            "no {region} region"
        );
    }
    h.get_by_label("DRC run since the last edit");
    h.get_by_label("Not run yet");
    h.get_by_label("Fix Duplicate Copper").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "drc.fix");
    assert_eq!(params, json!({"check": "duplicate_copper"}));
    h.run();
    assert!(h.query_by_label("Fix Duplicate Copper").is_none());

    // Ready for fab?'s Run (the check summary's Run DRC goes the same way).
    h.get_by_label("Run (DRC run since the last edit)").click();
    h.run();
    h.run();
    h.get_by_label("Last run just now");
}

#[test]
fn a_collapsed_region_stays_collapsed_in_state_json() {
    let (home, work) = (
        TempDir::new("props-state-home"),
        TempDir::new("props-state-work"),
    );
    let mut h = harness(&home, &work);
    run(&mut h, "selection.set", json!({}));
    h.run();
    h.get_by_label("Grid").click();
    h.run();
    let state = reda_ui::engine::state::read(&UserDirs::under(home.path()));
    assert!(state.properties_collapsed.contains("grid"));
    // A new window reads it back.
    drop(h);
    let work2 = TempDir::new("props-state-work2");
    let mut h = harness(&home, &work2);
    run(&mut h, "selection.set", json!({}));
    h.run();
    assert!(
        h.query_by_role_and_label(Role::ComboBox, "Snap grid")
            .is_none()
    );
    h.get_by_label("Grid").click();
    h.run();
    h.get_by_role_and_label(Role::ComboBox, "Snap grid");
    let state = reda_ui::engine::state::read(&UserDirs::under(home.path()));
    assert!(state.properties_collapsed.is_empty());
}
