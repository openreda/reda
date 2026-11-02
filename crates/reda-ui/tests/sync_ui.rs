//! Sync in the app, through the Shell with no window: the out-of-sync count, the Change
//! List dialog's model (ticks that cascade, Apply through `sync.apply`), cross-select by
//! Component UUID and Net highlighting mapped through Sync's matching.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::properties::{Edit, Properties, Shared};
use reda_ui::settings::SettingsDialog;
use reda_ui::settings::change_list::{self, ChangeListPage};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// A Shell on a new Project (one sheet, one PCB) with the fixture Library.
fn shell(dir: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .new_project(dir.path(), "Demo")
        .expect("project.new runs");
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

/// Places a 10k resistor at (`x`, 0) mm; returns its Component's and Unit's UUIDs.
fn place(shell: &mut Shell, x: i64) -> (Uuid, Uuid) {
    let out = run(
        shell,
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": format!("{x}mm"), "y": 0}}),
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

#[test]
fn the_shell_counts_the_pending_changes_after_every_edit() {
    let dir = TempDir::new("sync-ui-count");
    let mut shell = shell(&dir);
    assert_eq!(shell.out_of_sync(), 0);

    place(&mut shell, 0);
    assert_eq!(shell.out_of_sync(), 1);
    place(&mut shell, 10);
    assert_eq!(shell.out_of_sync(), 2);

    sync(&mut shell);
    assert_eq!(shell.out_of_sync(), 0);
}

fn wire(shell: &mut Shell, from: (f64, f64), to: (f64, f64)) -> Uuid {
    let point = |(x, y): (f64, f64)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")});
    let out = run(
        shell,
        "schematic.wire.add",
        json!({"document": SHEET, "points": [point(from), point(to)]}),
    );
    uuid(&out["result"]["wires"][0])
}

/// R1 at the origin and R2 10 mm to the right, their pin 2s wired together and labelled
/// SIG: five changes (two Components, the Net, two Pad Nets). Returns R1 and R2
/// (Component, Unit) and the Wire.
fn two_wired_resistors(shell: &mut Shell) -> ((Uuid, Uuid), (Uuid, Uuid), Uuid) {
    let r1 = place(shell, 0);
    let r2 = place(shell, 10);
    let wire = wire(shell, (0.0, -3.81), (10.0, -3.81));
    run(
        shell,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SIG", "at": {"x": "5mm", "y": "-3.81mm"}}),
    );
    (r1, r2, wire)
}

fn change_list(shell: &Shell) -> SettingsDialog {
    change_list::dialog(shell.engine().project().unwrap())
}

fn kind_of(change: &reda_ui::engine::sync::Change) -> String {
    serde_json::to_value(&change.change).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn ids_of(page: &ChangeListPage, kind: &str) -> Vec<String> {
    page.list()
        .changes()
        .filter(|c| kind_of(c) == kind)
        .map(|c| c.id.clone())
        .collect()
}

#[test]
fn every_change_starts_ticked_and_unticking_cascades_to_its_dependants() {
    let dir = TempDir::new("sync-ui-ticks");
    let mut shell = shell(&dir);
    two_wired_resistors(&mut shell);
    let mut dialog = change_list(&shell);
    let page = dialog.page_mut::<ChangeListPage>().unwrap();
    assert_eq!(page.list().count, 5);
    assert!(page.list().changes().all(|c| page.is_ticked(&c.id)));

    let add_net = ids_of(page, "add_net")[0].clone();
    page.set_ticked(&add_net, false);
    let pads = ids_of(page, "pad_net");
    assert_eq!(pads.len(), 2);
    assert!(pads.iter().all(|p| !page.is_ticked(p)));
    assert_eq!(page.ticked_count(), 2);

    // Ticking a Pad Net again ticks the Net it needs, and only that.
    page.set_ticked(&pads[0], true);
    assert!(page.is_ticked(&add_net));
    assert!(page.is_ticked(&pads[0]));
    assert!(page.is_ticked(&pads[1]));
    assert_eq!(page.ticked_count(), 5);
}

#[test]
fn apply_runs_sync_apply_leaving_out_the_unticked_changes() {
    let dir = TempDir::new("sync-ui-apply");
    let mut shell = shell(&dir);
    two_wired_resistors(&mut shell);
    let mut dialog = change_list(&shell);
    let page = dialog.page_mut::<ChangeListPage>().unwrap();
    let add_net = ids_of(page, "add_net")[0].clone();
    page.set_ticked(&add_net, false);
    let before = shell.engine().journal().len();

    dialog.apply(&mut shell).expect("Apply runs");

    assert_eq!(
        shell.engine().journal().len(),
        before + 1,
        "one Journal entry"
    );
    // The Net and its Pad Nets show up again; nothing else is pending.
    assert_eq!(shell.out_of_sync(), 3);
    let page = dialog.page::<ChangeListPage>().unwrap();
    assert_eq!(page.list().count, 3);
    assert!(page.list().changes().all(|c| page.is_ticked(&c.id)));
}

#[test]
fn the_change_list_follows_edits_made_while_it_is_open() {
    let dir = TempDir::new("sync-ui-follow");
    let mut shell = shell(&dir);
    place(&mut shell, 0);
    let mut dialog = change_list(&shell);
    let page = dialog.page_mut::<ChangeListPage>().unwrap();
    let id = page.list().changes().next().unwrap().id.clone();
    page.set_ticked(&id, false);

    place(&mut shell, 10);
    page.follow(shell.change_list());

    assert_eq!(page.list().count, 2);
    // The untick of a change still listed stays.
    assert!(!page.is_ticked(&id));
    assert_eq!(page.ticked_count(), 1);
}

/// The Footprint instance of Component `component` on the board.
fn footprint_of(shell: &Shell, component: Uuid) -> Uuid {
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    pcb.footprints()
        .find(|f| f.component.as_ref().is_some_and(|c| c.uuid == component))
        .expect("a synced Footprint")
        .uuid
}

fn select(shell: &mut Shell, params: Value) {
    run(shell, "selection.set", params);
}

fn marked(shell: &Shell) -> Vec<Uuid> {
    sorted(shell.cross_probe().to_vec())
}

fn sorted(mut uuids: Vec<Uuid>) -> Vec<Uuid> {
    uuids.sort();
    uuids
}

#[test]
fn selecting_a_unit_highlights_its_footprint_and_back() {
    let dir = TempDir::new("sync-ui-cross");
    let mut shell = shell(&dir);
    let (r1, r1_unit) = place(&mut shell, 0);
    place(&mut shell, 10);
    // Un-synced Components have nothing to select on the other side.
    select(&mut shell, json!({"objects": [r1_unit]}));
    assert!(marked(&shell).is_empty());

    sync(&mut shell);
    let footprint = footprint_of(&shell, r1);
    select(&mut shell, json!({"objects": [r1_unit]}));
    assert_eq!(marked(&shell), [footprint]);

    select(&mut shell, json!({"objects": [footprint]}));
    assert_eq!(marked(&shell), [r1_unit]);
    // Every open canvas carries the marks, so a sheet not in view shows them too.
    assert!(!shell.document_tabs().is_empty());
    for tab in shell.document_tabs() {
        assert_eq!(&tab.selection.highlighted[..], [r1_unit]);
    }

    select(&mut shell, json!({}));
    assert!(marked(&shell).is_empty());
}

#[test]
fn cross_select_can_be_switched_off_and_is_view_state() {
    let dir = TempDir::new("sync-ui-cross-off");
    let mut shell = shell(&dir);
    let (r1, r1_unit) = place(&mut shell, 0);
    sync(&mut shell);
    run(&mut shell, "project.save", json!({}));
    select(&mut shell, json!({"objects": [r1_unit]}));
    assert_eq!(marked(&shell), [footprint_of(&shell, r1)]);

    run(&mut shell, "view.cross_select.toggle", json!({}));
    assert!(marked(&shell).is_empty());
    assert!(
        !shell.engine().is_dirty(),
        "a view setting never dirties the Project"
    );

    run(&mut shell, "view.cross_select.toggle", json!({"on": true}));
    assert_eq!(marked(&shell), [footprint_of(&shell, r1)]);
}

fn first_of_type(shell: &mut Shell, kind: &str) -> Uuid {
    let out = run(shell, "objects.query", json!({ "type": kind }));
    uuid(&out["objects"][0]["object"]["uuid"])
}

#[test]
fn net_highlighting_maps_across_through_syncs_matching() {
    let dir = TempDir::new("sync-ui-nets");
    let mut shell = shell(&dir);
    let ((r1, r1_unit), (r2, r2_unit), wire) = two_wired_resistors(&mut shell);
    sync(&mut shell);
    let sig = first_of_type(&mut shell, "net");
    let label = first_of_type(&mut shell, "net_label");

    // A highlighted PCB Net lights up its Wires, Net Label and Units on the sheets, and
    // its Footprints on the board.
    select(&mut shell, json!({"nets": [sig]}));
    let pcb_side = sorted(vec![footprint_of(&shell, r1), footprint_of(&shell, r2)]);
    let mut both = vec![wire, label, r1_unit, r2_unit];
    both.extend(&pcb_side);
    assert_eq!(marked(&shell), sorted(both));

    // A schematic Net (clicking its Wire) lights up the PCB Net's copper and Footprints.
    select(&mut shell, json!({"objects": [wire]}));
    assert_eq!(marked(&shell), pcb_side);

    run(
        &mut shell,
        "view.net_cross_probe.toggle",
        json!({"on": false}),
    );
    assert!(marked(&shell).is_empty());
}

fn properties(shell: &Shell) -> Properties {
    Properties::of(shell.engine().project().unwrap(), shell.selection())
}

fn value_of(p: &Properties, name: &str) -> Shared<String> {
    let component = p.component.as_ref().expect("Component properties");
    component
        .fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no {name} row"))
        .value
        .clone()
}

#[test]
fn a_selected_footprint_edits_its_component_through_write_through() {
    let dir = TempDir::new("sync-ui-props");
    let mut shell = shell(&dir);
    let (r1, r1_unit) = place(&mut shell, 0);
    sync(&mut shell);
    let footprint = footprint_of(&shell, r1);
    select(&mut shell, json!({"objects": [footprint]}));

    let p = properties(&shell);
    assert_eq!(value_of(&p, "designator"), Shared::Same("R1".into()));
    assert_eq!(value_of(&p, "value"), Shared::Same("10k".into()));
    let component = p.component.as_ref().unwrap();
    assert_eq!(component.footprints, [footprint]);
    // Which fields show on the sheet is not the board's to say.
    assert_eq!(p.command(&Edit::Visible("mpn".into(), true)), None);

    let (id, params) = p
        .command(&Edit::Field("value".into(), "4k7".into()))
        .expect("an edit");
    assert_eq!(id, "pcb.footprint.set");
    assert_eq!(params, json!({"footprints": [footprint], "value": "4k7"}));
    run(&mut shell, id, params);

    // The Unit changed with the Footprint, and nothing is left to sync.
    select(&mut shell, json!({"objects": [r1_unit]}));
    assert_eq!(
        value_of(&properties(&shell), "value"),
        Shared::Same("4k7".into())
    );
    assert_eq!(shell.out_of_sync(), 0);

    select(&mut shell, json!({"objects": [footprint]}));
    let p = properties(&shell);
    let (id, params) = p.command(&Edit::NotFitted(true)).unwrap();
    assert_eq!(
        (id, params),
        (
            "pcb.footprint.set",
            json!({"footprints": [footprint], "not_fitted": true})
        )
    );
    let (id, params) = p
        .command(&Edit::Field("designator".into(), "R9".into()))
        .unwrap();
    assert_eq!(
        params,
        json!({"footprints": [footprint], "designator": "R9"})
    );
    run(&mut shell, id, params);
    select(&mut shell, json!({"objects": [r1_unit]}));
    assert_eq!(
        value_of(&properties(&shell), "designator"),
        Shared::Same("R9".into())
    );
}
