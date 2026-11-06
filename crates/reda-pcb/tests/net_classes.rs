//! Net Classes and assigning Nets to them, through Commands.

use reda_command::ErrorCode;
use reda_model::geom::Nm;
use reda_model::{PcbDocument, RuleKind, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn uuid(out: &Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn add_class(engine: &mut TestEngine, params: Value) -> Uuid {
    uuid(&engine.run_ok("pcb.net_class.add", params))
}

fn add_net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name})))
}

fn classes(engine: &TestEngine, net: Uuid) -> Vec<Uuid> {
    pcb(engine).net(net).unwrap().classes.clone()
}

#[test]
fn a_net_class_is_a_name_and_a_rank_with_its_values_in_rule_records() {
    let dir = TempDir::new("class-add");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(
        &mut engine,
        json!({"name": "Power", "rules": [
            {"kind": "track_width", "values": {"preferred": "0.5mm"}},
            {"kind": "via_size", "values": {"diameter": {"preferred": "0.8mm"}}}
        ]}),
    );
    let signal = add_class(&mut engine, json!({"name": "Signal"}));
    let pcb = pcb(&engine);
    let class = pcb.net_class(power).unwrap();
    assert_eq!(class.name, "Power");
    assert_eq!(class.rank, 1);
    assert_eq!(
        pcb.net_class(signal).unwrap().rank,
        2,
        "new classes rank lowest"
    );
    let track = pcb.class_rule(RuleKind::TrackWidth, power).unwrap();
    assert_eq!(track.values.preferred, Some(Nm::from_um(500)));
    assert_eq!(track.values.min, None, "only the override is held");
    let via = pcb.class_rule(RuleKind::ViaSize, power).unwrap();
    assert_eq!(
        via.values.diameter.unwrap().preferred,
        Some(Nm::from_um(800))
    );
    assert!(pcb.class_rule(RuleKind::Clearance, power).is_none());
    assert!(pcb.class_rule(RuleKind::TrackWidth, signal).is_none());
}

#[test]
fn net_class_names_are_unique_ignoring_case_and_never_default() {
    let dir = TempDir::new("class-unique");
    let mut engine = engine_with_pcb(&dir);
    add_class(&mut engine, json!({"name": "Power"}));
    for name in ["power", "DEFAULT", "  "] {
        let err = engine.run_err("pcb.net_class.add", json!({"name": name}));
        assert_eq!(err.code, ErrorCode::InvalidParams, "{name:?}");
        assert_eq!(err.details.as_ref().unwrap()["pointer"], "/name");
    }
}

#[test]
fn a_new_class_with_an_invalid_rule_adds_nothing() {
    let dir = TempDir::new("class-add-bad-rule");
    let mut engine = engine_with_pcb(&dir);
    let err = engine.run_err(
        "pcb.net_class.add",
        json!({"name": "Power", "rules": [
            {"kind": "track_width", "values": {"max": "0.1mm"}}
        ]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(
        err.details.as_ref().unwrap()["pointer"],
        "/rules/0/values/min"
    );
    assert!(pcb(&engine).net_class_named("Power").is_none());
}

#[test]
fn a_net_class_can_be_renamed() {
    let dir = TempDir::new("class-rename");
    let mut engine = engine_with_pcb(&dir);
    let class = add_class(&mut engine, json!({"name": "Pwr"}));
    engine.run_ok(
        "pcb.net_class.rename",
        json!({"class": class, "name": "Power"}),
    );
    // Renaming to its own name in another case is fine.
    engine.run_ok(
        "pcb.net_class.rename",
        json!({"class": class, "name": "POWER"}),
    );
    assert_eq!(pcb(&engine).net_class(class).unwrap().name, "POWER");
    let err = engine.run_err(
        "pcb.net_class.rename",
        json!({"class": class, "name": "Default"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn new_nets_are_in_the_default_class_only_and_can_join_several() {
    let dir = TempDir::new("net-add-to-class");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(&mut engine, json!({"name": "Power"}));
    let wide = add_class(&mut engine, json!({"name": "Wide"}));
    let vcc = add_net(&mut engine, "VCC");
    let gnd = add_net(&mut engine, "GND");
    let sig = add_net(&mut engine, "SIG");
    assert!(classes(&engine, vcc).is_empty());

    let out = engine.run_ok(
        "pcb.net.add_to_class",
        json!({"nets": [vcc, gnd], "class": power}),
    );
    assert_eq!(out["result"]["nets"].as_array().unwrap().len(), 2);
    engine.run_ok(
        "pcb.net.add_to_class",
        json!({"nets": [vcc], "class": wide}),
    );
    let mut both = vec![power, wide];
    both.sort();
    assert_eq!(classes(&engine, vcc), both);
    assert_eq!(classes(&engine, gnd), vec![power]);
    assert!(classes(&engine, sig).is_empty());

    // Adding again changes nothing.
    let out = engine.run_ok(
        "pcb.net.add_to_class",
        json!({"nets": [vcc], "class": wide}),
    );
    assert_eq!(out["result"]["nets"], json!([]));

    engine.run_ok(
        "pcb.net.remove_from_class",
        json!({"nets": [vcc], "class": power}),
    );
    assert_eq!(classes(&engine, vcc), vec![wide]);
}

#[test]
fn a_pattern_assigns_the_nets_whose_names_match_now() {
    let dir = TempDir::new("net-class-pattern");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(&mut engine, json!({"name": "Power"}));
    let v33 = add_net(&mut engine, "VCC_3V3");
    let v5 = add_net(&mut engine, "vcc_5v");
    let gnd = add_net(&mut engine, "GND");
    engine.run_ok(
        "pcb.net.add_to_class",
        json!({"pattern": "VCC_*", "class": power}),
    );
    assert_eq!(classes(&engine, v33), vec![power]);
    assert_eq!(classes(&engine, v5), vec![power], "ignoring case");
    assert!(classes(&engine, gnd).is_empty());
    // Nothing is stored: a Net added later is not matched.
    let later = add_net(&mut engine, "VCC_12V");
    assert!(classes(&engine, later).is_empty());

    engine.run_ok(
        "pcb.net.remove_from_class",
        json!({"pattern": "*5?", "class": power}),
    );
    assert!(classes(&engine, v5).is_empty());
    assert_eq!(classes(&engine, v33), vec![power]);

    let err = engine.run_err(
        "pcb.net.add_to_class",
        json!({"nets": [gnd], "pattern": "*", "class": power}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn assigning_an_unknown_net_or_class_changes_nothing() {
    let dir = TempDir::new("net-set-class-unknown");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(&mut engine, json!({"name": "Power"}));
    let vcc = add_net(&mut engine, "VCC");
    let nowhere = "00000000-0000-4000-8000-0000000000ff";
    let err = engine.run_err(
        "pcb.net.add_to_class",
        json!({"nets": [vcc, nowhere], "class": power}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = engine.run_err(
        "pcb.net.add_to_class",
        json!({"nets": [vcc], "class": nowhere}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(classes(&engine, vcc).is_empty());
}

#[test]
fn deleting_a_class_removes_it_from_its_nets_and_deletes_its_records() {
    let dir = TempDir::new("class-delete");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(
        &mut engine,
        json!({"name": "Power", "rules": [{"kind": "clearance", "values": {"value": "0.3mm"}}]}),
    );
    let vcc = add_net(&mut engine, "VCC");
    engine.run_ok(
        "pcb.net.add_to_class",
        json!({"nets": [vcc], "class": power}),
    );
    engine.run_ok("pcb.net_class.delete", json!({"class": power}));
    let pcb = pcb(&engine);
    assert!(pcb.net_class(power).is_none());
    assert!(pcb.net(vcc).unwrap().classes.is_empty());
    assert!(pcb.rules().all(|r| r.scope.is_board_wide()));
}

#[test]
fn deleting_a_class_undoes_with_its_nets_and_records() {
    let dir = TempDir::new("class-delete-undo");
    let mut engine = engine_with_pcb(&dir);
    let power = add_class(
        &mut engine,
        json!({"name": "Power", "rules": [{"kind": "clearance", "values": {"value": "0.3mm"}}]}),
    );
    let vcc = add_net(&mut engine, "VCC");
    engine.run_ok(
        "pcb.net.add_to_class",
        json!({"nets": [vcc], "class": power}),
    );
    let before = engine.project().unwrap().clone();
    engine.run_ok("pcb.net_class.delete", json!({"class": power}));
    let document = pcb(&engine).uuid;
    engine.run_ok("edit.undo", json!({"document": document}));
    assert_eq!(engine.project().unwrap(), &before);
}

#[test]
fn a_net_class_is_added_with_its_nets_in_one_step() {
    let dir = TempDir::new("class-add-nets");
    let mut engine = engine_with_pcb(&dir);
    let (vcc, gnd) = (add_net(&mut engine, "VCC"), add_net(&mut engine, "GND"));
    let power = add_class(&mut engine, json!({"name": "Power", "nets": [vcc]}));
    assert_eq!(classes(&engine, vcc), vec![power]);
    assert!(classes(&engine, gnd).is_empty());

    let err = engine.run_err(
        "pcb.net_class.add",
        json!({"name": "Other", "nets": [gnd, power]}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(
        pcb(&engine).net_class_named("Other").is_none(),
        "nothing added"
    );
}

#[test]
fn pattern_assignment_ignores_case_as_connectivity_does() {
    let dir = TempDir::new("net-class-pattern-case");
    let mut engine = engine_with_pcb(&dir);
    let usb = add_class(&mut engine, json!({"name": "USB"}));
    let dp = add_net(&mut engine, "USB_DP");
    let dm = add_net(&mut engine, "Usb_Dm");
    let other = add_net(&mut engine, "USB_VBUS");
    engine.run_ok(
        "pcb.net.add_to_class",
        json!({"pattern": "usb_d?", "class": usb}),
    );
    assert_eq!(classes(&engine, dp), vec![usb]);
    assert_eq!(classes(&engine, dm), vec![usb]);
    assert!(classes(&engine, other).is_empty());
    engine.run_ok(
        "pcb.net.remove_from_class",
        json!({"pattern": "USB_DM", "class": usb}),
    );
    assert!(classes(&engine, dm).is_empty());
    assert_eq!(classes(&engine, dp), vec![usb]);
}
