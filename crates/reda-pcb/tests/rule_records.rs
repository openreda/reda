//! Design rule records and the one resolver: the board-wide records a new board is seeded
//! with, `pcb.rules.set`, and which value applies to a Net.
//!
//! The board is the Generic preset's: clearance 0.16 mm, Track width 0.20 mm min and
//! 0.25 mm preferred, Via 0.70 mm with a 0.30 mm hole.

use reda_command::ErrorCode;
use reda_model::geom::Nm;
use reda_model::{PcbDocument, RuleKind, Uuid};
use reda_pcb::rules::{clearance_between, net_rules};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
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

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name})))
}

fn class(engine: &mut TestEngine, name: &str, nets: &[Uuid]) -> Uuid {
    uuid(&engine.run_ok("pcb.net_class.add", json!({"name": name, "nets": nets})))
}

fn set(engine: &mut TestEngine, params: Value) -> Value {
    engine.run_ok("pcb.rules.set", params)["result"]["rule"].clone()
}

fn um(n: i64) -> Nm {
    Nm::from_um(n)
}

#[test]
fn a_new_board_has_one_board_wide_record_per_kind_seeded_from_the_preset() {
    let dir = TempDir::new("rules-seed");
    let engine = engine(&dir);
    let pcb = pcb(&engine);
    let kinds: Vec<RuleKind> = pcb.rules().map(|r| r.kind).collect();
    assert_eq!(kinds.len(), RuleKind::ALL.len());
    for kind in RuleKind::ALL {
        let rule = pcb.board_rule(kind);
        assert!(rule.scope.is_board_wide());
        assert!(rule.enabled);
    }
    let track = pcb.board_rule(RuleKind::TrackWidth).values;
    assert_eq!(
        (track.min, track.preferred, track.max),
        (Some(um(200)), Some(um(250)), None)
    );
    let via = pcb.board_rule(RuleKind::ViaSize).values;
    assert_eq!(via.diameter.unwrap().preferred, Some(um(700)));
    assert_eq!(via.hole.unwrap().min, Some(um(300)));
    let holes = pcb.board_rule(RuleKind::HoleSize).values;
    assert_eq!((holes.min, holes.max), (Some(um(500)), Some(um(6300))));
}

#[test]
fn new_tracks_and_vias_take_the_preferred_size() {
    let dir = TempDir::new("rules-preferred");
    let mut engine = engine(&dir);
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "points": [{"x": "0mm", "y": "0mm"}, {"x": "5mm", "y": "0mm"}]}),
    );
    let track: Uuid = out["result"]["tracks"][0]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(pcb(&engine).track(track).unwrap().width, um(250));
    let out = engine.run_ok("pcb.via.add", json!({"at": {"x": "10mm", "y": "0mm"}}));
    let via = pcb(&engine).via(uuid(&out)).unwrap().clone();
    assert_eq!((via.diameter, via.hole), (um(700), um(300)));
}

#[test]
fn commands_accept_any_positive_size_and_leave_limits_to_drc() {
    let dir = TempDir::new("rules-any-size");
    let mut engine = engine(&dir);
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.05mm",
               "points": [{"x": "0mm", "y": "0mm"}, {"x": "5mm", "y": "0mm"}]}),
    );
    let track: Uuid = out["result"]["tracks"][0]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(pcb(&engine).track(track).unwrap().width, um(50));
    let run = engine.run_ok("drc.run", json!({}));
    assert!(
        run["violations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["check"] == "track_width"),
        "{run}"
    );
}

#[test]
fn the_highest_ranked_class_that_sets_a_value_wins_value_by_value() {
    let dir = TempDir::new("rules-rank");
    let mut engine = engine(&dir);
    let vcc = net(&mut engine, "VCC");
    let power = class(&mut engine, "Power", &[vcc]);
    let wide = class(&mut engine, "Wide", &[vcc]);
    set(
        &mut engine,
        json!({"kind": "track_width", "scope": {"net_class": power}, "values": {"preferred": "0.5mm"}}),
    );
    set(
        &mut engine,
        json!({"kind": "track_width", "scope": {"net_class": wide},
               "values": {"preferred": "1mm", "min": "0.3mm"}}),
    );
    let rules = net_rules(pcb(&engine), Some(vcc));
    // Power ranks above Wide, so its preferred wins; only Wide sets a minimum.
    assert_eq!(rules.track_width.preferred, um(500));
    assert_eq!(rules.track_width.min, Some(um(300)));
    assert_eq!(rules.clearance, Some(um(160)), "the board-wide record");
    // A Net on no class, and copper on no Net, use the board-wide records.
    let gnd = net(&mut engine, "GND");
    assert_eq!(
        net_rules(pcb(&engine), Some(gnd)).track_width.preferred,
        um(250)
    );
    assert_eq!(net_rules(pcb(&engine), None).track_width.preferred, um(250));
}

#[test]
fn between_two_nets_the_larger_clearance_applies() {
    let dir = TempDir::new("rules-pair");
    let mut engine = engine(&dir);
    let hv = net(&mut engine, "HV");
    let sig = net(&mut engine, "SIG");
    let high = class(&mut engine, "High Voltage", &[hv]);
    set(
        &mut engine,
        json!({"kind": "clearance", "scope": {"net_class": high}, "values": {"value": "1mm"}}),
    );
    let pcb = pcb(&engine);
    assert_eq!(
        clearance_between(pcb, Some(hv), Some(sig)),
        Some(Nm::from_mm(1))
    );
    assert_eq!(clearance_between(pcb, Some(sig), None), Some(um(160)));
}

#[test]
fn a_disabled_record_is_ignored_but_a_disabled_board_preferred_still_routes() {
    let dir = TempDir::new("rules-disabled");
    let mut engine = engine(&dir);
    let vcc = net(&mut engine, "VCC");
    let power = class(&mut engine, "Power", &[vcc]);
    set(
        &mut engine,
        json!({"kind": "track_width", "scope": {"net_class": power},
               "values": {"preferred": "0.5mm"}, "enabled": false}),
    );
    assert_eq!(
        net_rules(pcb(&engine), Some(vcc)).track_width.preferred,
        um(250)
    );

    set(
        &mut engine,
        json!({"kind": "track_width", "enabled": false}),
    );
    set(&mut engine, json!({"kind": "clearance", "enabled": false}));
    let rules = net_rules(pcb(&engine), None);
    assert_eq!(rules.track_width.min, None, "the limit is absent");
    assert_eq!(
        rules.track_width.preferred,
        um(250),
        "preferred still applies"
    );
    assert_eq!(rules.clearance, None);
    // DRC skips an absent limit.
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.05mm",
               "points": [{"x": "0mm", "y": "0mm"}, {"x": "5mm", "y": "0mm"}]}),
    );
    let run = engine.run_ok("drc.run", json!({}));
    assert!(
        !run["violations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["check"] == "track_width"),
        "{run}"
    );
}

#[test]
fn rules_set_changes_only_the_values_given_and_null_clears_one() {
    let dir = TempDir::new("rules-set");
    let mut engine = engine(&dir);
    let rule = set(
        &mut engine,
        json!({"kind": "via_size", "values": {"diameter": {"max": "2mm"}}, "comment": "fab"}),
    );
    assert_eq!(
        rule["values"],
        json!({"diameter": {"min": 700000, "preferred": 700000, "max": 2000000},
               "hole": {"min": 300000, "preferred": 300000}})
    );
    assert_eq!(rule["comment"], "fab");
    let rule = set(
        &mut engine,
        json!({"kind": "via_size", "values": {"diameter": {"max": null}}, "comment": null}),
    );
    assert_eq!(
        rule["values"]["diameter"],
        json!({"min": 700000, "preferred": 700000})
    );
    assert!(rule.get("comment").is_none());
}

#[test]
fn a_class_record_is_created_on_first_set_and_deleted_once_empty() {
    let dir = TempDir::new("rules-class-record");
    let mut engine = engine(&dir);
    let power = class(&mut engine, "Power", &[]);
    set(
        &mut engine,
        json!({"kind": "clearance", "scope": {"net_class": power}, "values": {"value": "0.3mm"}}),
    );
    assert!(
        pcb(&engine)
            .class_rule(RuleKind::Clearance, power)
            .is_some()
    );
    let rule = set(
        &mut engine,
        json!({"kind": "clearance", "scope": {"net_class": power}, "values": {"value": null}}),
    );
    assert!(rule.is_null());
    assert!(
        pcb(&engine)
            .class_rule(RuleKind::Clearance, power)
            .is_none()
    );
}

#[test]
fn rules_set_refuses_what_it_cannot_store_or_what_blocks() {
    let dir = TempDir::new("rules-refused");
    let mut engine = engine(&dir);
    let refused = |engine: &mut TestEngine, params: Value, pointer: &str| {
        let err = engine.run_err("pcb.rules.set", params.clone());
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
        assert_eq!(err.details.unwrap()["pointer"], pointer, "{params}");
    };
    // A value the kind does not hold.
    refused(
        &mut engine,
        json!({"kind": "clearance", "values": {"min": "1mm"}}),
        "/values/min",
    );
    // A negative value.
    refused(
        &mut engine,
        json!({"kind": "clearance", "values": {"value": "-1mm"}}),
        "/values/value",
    );
    // Clearing a value the board-wide record needs.
    refused(
        &mut engine,
        json!({"kind": "track_width", "values": {"preferred": null}}),
        "/values/preferred",
    );
    // `preferred` outside the range, and `min` above `max`.
    refused(
        &mut engine,
        json!({"kind": "track_width", "values": {"preferred": "0.1mm"}}),
        "/values/preferred",
    );
    refused(
        &mut engine,
        json!({"kind": "hole_size", "values": {"max": "0.4mm"}}),
        "/values/min",
    );
    // A class record is judged on its values over the board-wide record.
    let power = class(&mut engine, "Power", &[]);
    refused(
        &mut engine,
        json!({"kind": "track_width", "scope": {"net_class": power}, "values": {"max": "0.22mm"}}),
        "/values/preferred",
    );
    let err = engine.run_err(
        "pcb.rules.set",
        json!({"kind": "clearance", "scope": {"net_class": "00000000-0000-4000-8000-0000000000ff"},
               "values": {"value": "1mm"}}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn ignore_pad_to_pad_is_a_clearance_value() {
    let dir = TempDir::new("rules-pad-to-pad");
    let mut engine = engine(&dir);
    let rule = set(
        &mut engine,
        json!({"kind": "clearance", "values": {"ignore_pad_to_pad": true}}),
    );
    assert_eq!(rule["values"]["ignore_pad_to_pad"], true);
    assert!(net_rules(pcb(&engine), None).ignore_pad_to_pad);
}

#[test]
fn applying_a_fab_preset_reseeds_board_records_and_keeps_class_records() {
    let dir = TempDir::new("rules-apply-preset");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "fab_preset": "JLCPCB"}),
    );
    let power = class(&mut engine, "Power", &[]);
    set(
        &mut engine,
        json!({"kind": "track_width", "scope": {"net_class": power}, "values": {"preferred": "1mm"}}),
    );
    set(
        &mut engine,
        json!({"kind": "clearance", "values": {"value": "0.5mm"}}),
    );
    let preset = engine.project().unwrap().fab_presets().next().unwrap().uuid;
    let diff = engine.run_ok("pcb.rules.fab_preset_diff", json!({"preset": preset}));
    assert_eq!(
        diff["changes"],
        json!([{"kind": "clearance", "field": "value", "board": 500000, "preset": 100000}])
    );
    engine.run_ok("pcb.rules.apply_fab_preset", json!({"preset": preset}));
    let pcb = pcb(&engine);
    assert_eq!(
        pcb.board_rule(RuleKind::Clearance).values.value,
        Some(um(100))
    );
    assert_eq!(
        pcb.class_rule(RuleKind::TrackWidth, power)
            .unwrap()
            .values
            .preferred,
        Some(Nm::from_mm(1))
    );
}
