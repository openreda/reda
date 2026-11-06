//! The Properties panel's model, through the Shell with no window: it follows the
//! selection (`selection.set`), shows shared values with `<mixed>` where they differ, and
//! turns every edit into one Command that applies to everything selected.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{ErrorCode, FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::properties::{Edit, FieldRow, Properties, Shared};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// A Shell on a new Project with the fixture Library and the test PCB Commands.
fn shell(dir: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
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

/// Places `part` and returns its Component's and its Unit's UUIDs.
fn place(shell: &mut Shell, part: &str, x: i64) -> (Uuid, Uuid) {
    let out = run(
        shell,
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": {"x": format!("{x}mil"), "y": 0}}),
    );
    (
        uuid(&out["result"]["component"]["uuid"]),
        uuid(&out["result"]["unit"]),
    )
}

fn select(shell: &mut Shell, params: Value) {
    run(shell, "selection.set", params);
}

fn properties(shell: &Shell) -> Properties {
    Properties::of(shell.engine().project().unwrap(), shell.selection())
}

fn field<'a>(p: &'a Properties, name: &str) -> &'a FieldRow {
    let component = p.component.as_ref().expect("Component properties");
    component
        .fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no {name} row"))
}

/// Runs what the panel would run for `edit`; returns the Command id and params.
fn apply(shell: &mut Shell, edit: Edit) -> (String, Value) {
    let (id, params) = properties(shell).command(&edit).expect("an edit Command");
    run(shell, id, params.clone());
    (id.to_owned(), params)
}

#[test]
fn nothing_selected_shows_nothing_to_edit() {
    let dir = TempDir::new("properties-empty");
    let shell = shell(&dir);
    let p = properties(&shell);
    assert_eq!(p.count, 0);
    assert!(p.component.is_none());
    assert!(p.net_class.is_none());
}

#[test]
fn shared_fields_show_mixed_where_values_differ_and_an_edit_applies_to_all() {
    let dir = TempDir::new("properties-mixed");
    let mut shell = shell(&dir);
    let (r1, u1) = place(&mut shell, R_10K, 0);
    let (r2, u2) = place(&mut shell, R_10K, 1000);
    let (c1, u3) = place(&mut shell, C_100N, 2000);
    select(&mut shell, json!({"objects": [u1, u2, u3]}));

    let p = properties(&shell);
    assert_eq!(p.count, 3);
    assert_eq!(field(&p, "value").value, Shared::Mixed);
    assert_eq!(field(&p, "designator").value, Shared::Mixed);
    assert_eq!(field(&p, "value").visible, Shared::Same(true));
    assert_eq!(field(&p, "mpn").visible, Shared::Same(false));
    let component = p.component.as_ref().unwrap();
    assert_eq!(component.components, [r1, r2, c1]);
    assert_eq!(component.not_fitted, Shared::Same(false));
    assert_eq!(Shared::<String>::Mixed.to_string(), "<mixed>");

    let (id, params) = apply(&mut shell, Edit::Field("value".into(), "1k".into()));
    assert_eq!(id, "schematic.component.set");
    assert_eq!(params, json!({"components": [r1, r2, c1], "value": "1k"}));
    let p = properties(&shell);
    assert_eq!(field(&p, "value").value, Shared::Same("1k".to_owned()));

    apply(&mut shell, Edit::Field("LCSC".into(), "C25804".into()));
    apply(&mut shell, Edit::NotFitted(true));
    apply(&mut shell, Edit::Visible("mpn".into(), true));
    let p = properties(&shell);
    assert_eq!(field(&p, "LCSC").value, Shared::Same("C25804".to_owned()));
    assert_eq!(field(&p, "mpn").visible, Shared::Same(true));
    assert_eq!(p.component.as_ref().unwrap().not_fitted, Shared::Same(true));
}

#[test]
fn the_designator_is_editable_for_one_component_only() {
    let dir = TempDir::new("properties-designator");
    let mut shell = shell(&dir);
    let (r1, u1) = place(&mut shell, R_10K, 0);
    let (_, u2) = place(&mut shell, R_10K, 1000);

    select(&mut shell, json!({"objects": [u1, u2]}));
    let p = properties(&shell);
    assert!(!field(&p, "designator").editable);
    assert!(
        p.command(&Edit::Field("designator".into(), "R9".into()))
            .is_none()
    );

    select(&mut shell, json!({"objects": [u1]}));
    let p = properties(&shell);
    assert!(field(&p, "designator").editable);
    assert_eq!(field(&p, "designator").value, Shared::Same("R1".to_owned()));
    let (_, params) = apply(&mut shell, Edit::Field("designator".into(), "R9".into()));
    assert_eq!(params, json!({"components": [r1], "designator": "R9"}));
}

#[test]
fn the_footprint_is_chosen_from_the_parts_alternates() {
    let dir = TempDir::new("properties-footprint");
    let mut shell = shell(&dir);
    let (r1, u1) = place(&mut shell, R_10K, 0);
    let (r2, u2) = place(&mut shell, R_10K, 1000);
    let (_, u3) = place(&mut shell, C_100N, 2000);

    select(&mut shell, json!({"objects": [u1, u2]}));
    let p = properties(&shell);
    let component = p.component.as_ref().unwrap();
    let names: Vec<&str> = component
        .alternates
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(names.len(), 2, "R_10k has two alternates: {names:?}");
    assert!(names.iter().all(|n| !n.is_empty()));
    let (first, second) = (
        component.alternates[0].footprint,
        component.alternates[1].footprint,
    );
    assert_eq!(component.footprint, Shared::Same(Some(first)));

    let (_, params) = apply(&mut shell, Edit::Footprint(second));
    assert_eq!(params, json!({"components": [r1, r2], "footprint": second}));
    assert_eq!(
        properties(&shell).component.unwrap().footprint,
        Shared::Same(Some(second))
    );

    // Different Parts have different alternates: there is nothing to choose from.
    select(&mut shell, json!({"objects": [u1, u3]}));
    let p = properties(&shell);
    assert!(p.component.as_ref().unwrap().alternates.is_empty());
    assert!(p.command(&Edit::Footprint(first)).is_none());
}

#[test]
fn selected_copper_and_highlighted_nets_join_or_leave_a_net_class_together() {
    let dir = TempDir::new("properties-net-class");
    let mut shell = shell(&dir);
    let net = |shell: &mut Shell, name: &str| -> Uuid {
        uuid(&run(shell, "test.net.add", json!({"name": name}))["result"]["uuid"])
    };
    let (gnd, vcc, sig) = (
        net(&mut shell, "GND"),
        net(&mut shell, "VCC"),
        net(&mut shell, "SIG"),
    );
    let power = run(&mut shell, "pcb.net_class.add", json!({"name": "Power"}));
    let power = uuid(&power["result"]["uuid"]);
    let layer = {
        let pcb = shell.engine().project().unwrap().pcb().unwrap();
        pcb.copper_layers().next().unwrap().uuid
    };
    let track = run(
        &mut shell,
        "pcb.track.add",
        json!({"layer": layer, "points": [{"x": 0, "y": 0}, {"x": "5mm", "y": 0}], "net": gnd}),
    );
    let track = track["result"]["tracks"][0].clone();
    let via = run(
        &mut shell,
        "pcb.via.add",
        json!({"at": {"x": "10mm", "y": "10mm"}, "net": vcc}),
    );
    let via = via["result"]["uuid"].clone();
    let footprint = run(
        &mut shell,
        "test.footprint.add",
        json!({"at": {"x": "20mm", "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": sig}
        ]}),
    );
    let pad = json!({
        "footprint": footprint["result"]["footprint"],
        "pad": footprint["result"]["pads"][0],
    });

    select(&mut shell, json!({"objects": [track, via]}));
    let p = properties(&shell);
    let class = p.net_class.as_ref().expect("a Net Class row");
    assert_eq!(class.nets, sorted(vec![gnd, vcc]));
    assert_eq!(
        class.class,
        Shared::Same(vec![]),
        "the Default Net Class only"
    );
    assert_eq!(class.classes.len(), 1, "the stored classes");

    let (id, params) = apply(&mut shell, Edit::NetClass(power));
    assert_eq!(id, "pcb.net.add_to_class");
    assert_eq!(
        params,
        json!({"nets": sorted(vec![gnd, vcc]), "class": power})
    );
    // With every Net in it, picking the class again takes them out.
    let p = properties(&shell);
    assert_eq!(
        p.net_class.as_ref().unwrap().class,
        Shared::Same(vec![power])
    );
    let (id, _) = apply(&mut shell, Edit::NetClass(power));
    assert_eq!(id, "pcb.net.remove_from_class");
    apply(&mut shell, Edit::NetClass(power));

    // A Footprint's Pad and a highlighted Net: one is now Power, the other Default.
    select(&mut shell, json!({"pads": [pad], "nets": [gnd]}));
    let p = properties(&shell);
    let class = p.net_class.as_ref().unwrap();
    assert_eq!(class.nets, sorted(vec![gnd, sig]));
    assert_eq!(class.class, Shared::Mixed);
}

fn sorted(mut v: Vec<Uuid>) -> Vec<Uuid> {
    v.sort();
    v
}

#[test]
fn other_objects_show_their_properties_read_only() {
    let dir = TempDir::new("properties-generic");
    let mut shell = shell(&dir);
    let text = run(
        &mut shell,
        "schematic.text.add",
        json!({"document": SHEET, "text": "Hello", "at": {"x": 0, "y": 0}}),
    );
    let text = text["result"]["text"].clone();
    select(&mut shell, json!({"objects": [text]}));
    let p = properties(&shell);
    assert_eq!(p.kinds, [("sheet_text".to_owned(), 1)]);
    assert!(p.component.is_none());
    assert!(p.details.iter().any(|(k, v)| k == "text" && v == "Hello"));
}

#[test]
fn selecting_an_unknown_object_is_not_found_and_keeps_the_selection() {
    let dir = TempDir::new("properties-unknown");
    let mut shell = shell(&dir);
    let err = shell
        .run(
            "selection.set",
            json!({"objects": ["00000000-0000-4000-8000-0000000000ff"]}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(shell.selection().objects.is_empty());
}
