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
        json!([{"kind": "clearance", "field": "value", "board": 500000, "preset": 100000,
                "limit": true, "ticked": true}])
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

#[test]
fn mask_and_paste_rules_are_seeded_and_accept_signed_values() {
    let dir = TempDir::new("rules-openings");
    let mut engine = engine(&dir);
    let out = set(
        &mut engine,
        json!({"kind": "mask_expansion", "values": {"bottom": "-0.025mm"}}),
    );
    assert_eq!(out["values"]["top"], 50_000);
    assert_eq!(out["values"]["bottom"], -25_000);
    assert_eq!(out["values"]["tent_vias_top"], true);
    assert_eq!(out["values"]["tent_vias_bottom"], true);
    assert_eq!(out["values"]["from_hole_edge"], false);
    let out = set(
        &mut engine,
        json!({"kind": "paste_expansion", "values": {"expansion": null, "percent": -10}}),
    );
    assert_eq!(out["values"]["percent"], -10);
    assert!(out["values"].get("expansion").is_none());
    let bad = engine.run_err(
        "pcb.rules.set",
        json!({"kind": "paste_expansion", "values": {"percent": -100}}),
    );
    assert_eq!(bad.code, ErrorCode::InvalidParams);
}

#[test]
fn mask_and_paste_preset_seeds_match_the_specification_table() {
    for (preset, expansion) in [("Generic", 50_000), ("JLCPCB", 0), ("PCBWay", 50_000)] {
        let dir = TempDir::new("opening-preset");
        let mut engine = probe_engine();
        engine.run_ok(
            "project.new",
            json!({"path":dir.path(),"name":"Demo","fab_preset":preset}),
        );
        let mask = serde_json::to_value(pcb(&engine).board_rule(RuleKind::MaskExpansion)).unwrap();
        assert_eq!(
            mask["values"],
            json!({"top":expansion,"bottom":expansion,"from_hole_edge":false,"tent_vias_top":true,"tent_vias_bottom":true}),
            "{preset}"
        );
        let paste =
            serde_json::to_value(pcb(&engine).board_rule(RuleKind::PasteExpansion)).unwrap();
        assert_eq!(paste["values"], json!({"expansion":0}), "{preset}");
    }
}

/// A JLCPCB board with the Generic preset copied into the Project: Generic raises the
/// limits (Track 0.10 to 0.20 mm, Via diameter 0.51 to 0.70 mm).
fn jlcpcb_board(dir: &TempDir) -> (TestEngine, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "fab_preset": "JLCPCB"}),
    );
    let generic = uuid(&engine.run_ok("fab_preset.add", json!({"name": "Generic"})));
    (engine, generic)
}

/// `(kind, field, limit, ticked)` of every preview row.
fn rows(diff: &Value) -> Vec<(String, String, bool, bool)> {
    diff["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["kind"].as_str().unwrap().to_owned(),
                c["field"].as_str().unwrap().to_owned(),
                c["limit"].as_bool().unwrap(),
                c["ticked"].as_bool().unwrap(),
            )
        })
        .collect()
}

#[test]
fn the_preview_ticks_limits_and_only_preferred_sizes_that_fall_outside() {
    let dir = TempDir::new("rules-preview-ticks");
    let (mut engine, generic) = jlcpcb_board(&dir);
    let diff = engine.run_ok("pcb.rules.fab_preset_diff", json!({"preset": generic}));
    let rows = rows(&diff);
    let row = |kind: &str, field: &str| {
        rows.iter()
            .find(|r| r.0 == kind && r.1 == field)
            .map(|r| (r.2, r.3))
    };
    assert_eq!(row("track_width", "min"), Some((true, true)), "{diff:#}");
    assert_eq!(row("via_size", "diameter_min"), Some((true, true)));
    // 0.66 mm falls below the new 0.70 mm minimum.
    assert_eq!(row("via_size", "diameter_preferred"), Some((false, true)));
    // The board's 0.30 mm hole stays inside the new range... and is already Generic's.
    assert_eq!(row("via_size", "hole_preferred"), None);
    assert_eq!(row("track_width", "preferred"), None);
}

#[test]
fn applying_a_fab_preset_takes_the_chosen_rows_and_flags_class_records_it_breaks() {
    let dir = TempDir::new("rules-apply-rows");
    let (mut engine, generic) = jlcpcb_board(&dir);
    // Valid over the board's 0.10 mm minimum, broken by Generic's 0.20 mm.
    let power = class(&mut engine, "Power", &[]);
    let narrow = set(
        &mut engine,
        json!({"kind": "track_width", "scope": {"net_class": power},
               "values": {"preferred": "0.12mm", "max": "0.15mm"}}),
    );
    let narrow: Uuid = narrow["uuid"].as_str().unwrap().parse().unwrap();
    let diff = engine.run_ok("pcb.rules.fab_preset_diff", json!({"preset": generic}));
    assert_eq!(
        diff["class_problems"],
        json!([{"rule": narrow, "net_class": power, "kind": "track_width", "value": "min",
                "message": "`min` is above `max`"}]),
        "{diff:#}"
    );

    // Only the chosen rows change.
    let out = engine.run_ok(
        "pcb.rules.apply_fab_preset",
        json!({"preset": generic, "rows": [{"kind": "clearance", "field": "value"}]}),
    );
    assert_eq!(out["result"]["changes"].as_array().unwrap().len(), 1);
    assert_eq!(out["result"]["class_problems"], json!([]));
    let board = pcb(&engine);
    assert_eq!(
        board.board_rule(RuleKind::Clearance).values.value,
        Some(um(160))
    );
    assert_eq!(
        board.board_rule(RuleKind::TrackWidth).values.min,
        Some(um(100))
    );

    // A row that would leave the board's own record blocking is refused.
    let err = engine.run_err(
        "pcb.rules.apply_fab_preset",
        json!({"preset": generic, "rows": [{"kind": "via_size", "field": "diameter_min"}]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{}", err.message);

    // The ticked rows apply; the class record is flagged and left as it was.
    let out = engine.run_ok("pcb.rules.apply_fab_preset", json!({"preset": generic}));
    assert_eq!(out["result"]["class_problems"][0]["rule"], json!(narrow));
    let board = pcb(&engine);
    assert_eq!(
        board.board_rule(RuleKind::TrackWidth).values.min,
        Some(um(200))
    );
    assert_eq!(board.rule(narrow).unwrap().values.max, Some(um(150)));
}

#[test]
fn saving_rules_as_a_fab_preset_writes_board_wide_values_only() {
    let dir = TempDir::new("rules-save-preset");
    let mut engine = engine(&dir);
    let power = class(&mut engine, "Power", &[]);
    set(
        &mut engine,
        json!({"kind": "clearance", "scope": {"net_class": power}, "values": {"value": "1mm"}}),
    );
    set(
        &mut engine,
        json!({"kind": "clearance", "values": {"value": "0.3mm"}}),
    );
    set(
        &mut engine,
        json!({"kind": "track_width", "values": {"preferred": "0.4mm", "max": "2mm"}}),
    );
    let generic = engine
        .project()
        .unwrap()
        .fab_presets()
        .next()
        .unwrap()
        .clone();
    let out = engine.run_ok(
        "pcb.rules.save_as_preset",
        json!({"name": "  House rules ", "from": generic.uuid}),
    );
    let saved = engine
        .project()
        .unwrap()
        .fab_preset(uuid(&out))
        .unwrap()
        .clone();
    assert_eq!(saved.name, "House rules");
    assert_eq!(saved.rules.clearance, um(300));
    assert_eq!(saved.rules.preferred.track_width, um(400));
    assert_eq!(saved.rules.track_width, generic.rules.track_width);
    assert_eq!(saved.rules.hole_max, generic.rules.hole_max);
    assert_eq!(saved.gerber, generic.gerber);
    assert_eq!(saved.bom, generic.bom);
    // A new board from it is seeded with the same board-wide values.
    let diff = engine.run_ok("pcb.rules.fab_preset_diff", json!({"preset": saved.uuid}));
    assert_eq!(diff["changes"], json!([]), "{diff:#}");

    let err = engine.run_err("pcb.rules.save_as_preset", json!({"name": " "}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}
