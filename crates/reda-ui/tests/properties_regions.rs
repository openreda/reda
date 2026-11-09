//! The Properties panel's regions, through the Shell with no window
//! (docs/spec/v0.1/editing.md#properties): numeric expressions on Tracks, Vias and
//! Footprints, the Rules region from `pcb.rules.applicable`, kind chips, a Pad's local
//! edits with Revert, and, with nothing selected, Ready for fab? and the check summary
//! with Fix for the health checks.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::{DrcCheck, RuleKind, Uuid};
use reda_ui::engine::preferences::Units;
use reda_ui::messages::CheckKind;
use reda_ui::properties::overview::{
    BoardFigures, CheckSummary, FabAction, FabItem, ready_for_fab,
};
use reda_ui::properties::rules::Standing;
use reda_ui::properties::{Properties, Region};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

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
        Ok(out) => out["result"].clone(),
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn properties(shell: &Shell) -> Properties {
    Properties::of(shell.engine().project().unwrap(), shell.selection())
}

fn via(shell: &mut Shell, x: &str, net: Option<Uuid>) -> Uuid {
    let mut params = json!({"at": {"x": x, "y": 0}});
    if let Some(net) = net {
        params["net"] = json!(net);
    }
    uuid(&run(shell, "pcb.via.add", params)["uuid"])
}

fn track(shell: &mut Shell, net: Option<Uuid>) -> Uuid {
    let mut params = json!({"layer": "copper_top",
        "points": [{"x": "0mm", "y": "2mm"}, {"x": "5mm", "y": "2mm"}]});
    if let Some(net) = net {
        params["net"] = json!(net);
    }
    uuid(&run(shell, "pcb.track.add", params)["tracks"][0])
}

/// Places a resistor and syncs it to the board; returns its Footprint instance.
fn synced_footprint(shell: &mut Shell) -> Uuid {
    run(
        shell,
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": 0, "y": 0}}),
    );
    let hash = shell.run("sync.preview", json!({})).unwrap()["hash"].clone();
    run(shell, "sync.apply", json!({"hash": hash}));
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    pcb.footprints().next().unwrap().uuid
}

/// Runs `(id, params)` as the panel would.
fn commit(shell: &mut Shell, command: (&str, Value)) {
    run(shell, command.0, command.1);
}

#[test]
fn via_and_track_sizes_take_numeric_expressions() {
    let dir = TempDir::new("properties-regions-copper");
    let mut shell = shell(&dir);
    let a = via(&mut shell, "0mm", None);
    let b = via(&mut shell, "5mm", None);
    run(
        &mut shell,
        "pcb.via.set",
        json!({"vias": [b], "diameter": "0.8mm"}),
    );
    run(&mut shell, "selection.set", json!({"objects": [a, b]}));
    let p = properties(&shell);
    let names: Vec<&str> = p.numbers.rows.iter().map(|r| r.name).collect();
    assert_eq!(names, ["diameter", "hole"]);
    assert!(p.numbers.rows.iter().all(|r| r.region == Region::Graphical));

    let diameter_a = {
        let project = shell.engine().project().unwrap();
        project.pcb().unwrap().via(a).unwrap().diameter.0
    };
    let command = p
        .numbers
        .command("diameter", "! + 0.1mm", Units::Mm)
        .unwrap()
        .unwrap();
    assert_eq!(
        command,
        (
            "batch",
            json!({"commands": [
                {"id": "pcb.via.set", "params": {"vias": [a], "diameter": diameter_a + 100_000}},
                {"id": "pcb.via.set", "params": {"vias": [b], "diameter": 900_000}},
            ]})
        )
    );
    commit(&mut shell, command);
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.via(b).unwrap().diameter.0, 900_000);

    let t = track(&mut shell, None);
    run(&mut shell, "selection.set", json!({"objects": [t]}));
    let p = properties(&shell);
    let names: Vec<&str> = p.numbers.rows.iter().map(|r| r.name).collect();
    assert_eq!(names, ["width", "length"]);
    let width = p.numbers.rows[0].value.same().unwrap().unwrap();
    let command = p
        .numbers
        .command("width", "! * 2", Units::Mm)
        .unwrap()
        .unwrap();
    assert_eq!(
        command,
        ("pcb.track.set", json!({"tracks": [t], "width": width * 2}))
    );
    let command = p
        .numbers
        .command("length", "length - 1mm", Units::Mm)
        .unwrap()
        .unwrap();
    assert_eq!(
        command,
        ("pcb.track.set", json!({"tracks": [t], "length": 4_000_000}))
    );
    commit(&mut shell, command);
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.track(t).unwrap().geometry.length(), 4_000_000.0);
}

#[test]
fn a_footprints_location_moves_and_turns_it_by_the_difference() {
    let dir = TempDir::new("properties-regions-location");
    let mut shell = shell(&dir);
    let footprint = synced_footprint(&mut shell);
    run(&mut shell, "selection.set", json!({"objects": [footprint]}));
    let p = properties(&shell);
    let location: Vec<&str> = (p.numbers.rows.iter())
        .filter(|r| r.region == Region::Location)
        .map(|r| r.name)
        .collect();
    assert_eq!(location, ["x", "y", "rotation"]);
    let at = shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .footprint(footprint)
        .unwrap()
        .at;

    let command = p
        .numbers
        .command("x", "! + 1mm", Units::Mm)
        .unwrap()
        .unwrap();
    assert_eq!(
        command,
        (
            "pcb.move",
            json!({"objects": [footprint], "by": {"x": 1_000_000, "y": 0}})
        )
    );
    commit(&mut shell, command);
    let command = properties(&shell)
        .numbers
        .command("rotation", "90", Units::Mm)
        .unwrap()
        .unwrap();
    assert_eq!(
        command,
        (
            "pcb.rotate",
            json!({"objects": [footprint], "angle": 90_000})
        )
    );
    commit(&mut shell, command);
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    let placed = pcb.footprint(footprint).unwrap();
    assert_eq!(placed.at.x.0, at.x.0 + 1_000_000);
    assert_eq!(placed.at.y, at.y);
    assert_eq!(placed.rotation.0, 90_000);
    // The value it already has is no edit.
    let p = properties(&shell);
    assert_eq!(p.numbers.command("rotation", "90", Units::Mm), Ok(None));
}

#[test]
fn the_rules_region_shows_winners_losers_and_disabled_records() {
    let dir = TempDir::new("properties-regions-rules");
    let mut shell = shell(&dir);
    let vcc = uuid(&run(&mut shell, "test.net.add", json!({"name": "VCC"}))["uuid"]);
    let power = uuid(
        &run(
            &mut shell,
            "pcb.net_class.add",
            json!({"name": "Power", "nets": [vcc]}),
        )["uuid"],
    );
    let class_rule = uuid(
        &run(
            &mut shell,
            "pcb.rules.set",
            json!({"kind": "track_width", "scope": {"net_class": power}, "values": {"min": "0.3mm", "preferred": "0.35mm"}}),
        )["rule"]["uuid"],
    );
    let off = uuid(
        &run(
            &mut shell,
            "pcb.rules.set",
            json!({"kind": "clearance", "scope": {"net_class": power}, "values": {"value": "1mm"},
               "enabled": false}),
        )["rule"]["uuid"],
    );
    let t = track(&mut shell, Some(vcc));
    run(&mut shell, "selection.set", json!({"objects": [t]}));

    let rules = properties(&shell)
        .rules
        .expect("a Rules region for one Track");
    assert_eq!(rules.objects, 1);
    let width = rules
        .kinds
        .iter()
        .find(|k| k.kind == RuleKind::TrackWidth)
        .unwrap();
    let won = width
        .rows
        .iter()
        .find(|r| r.standing == Standing::Won && r.fields == ["min"])
        .unwrap();
    assert_eq!(
        (won.rule, won.class.as_deref()),
        (class_rule, Some("Power"))
    );
    let lost = width
        .rows
        .iter()
        .find(|r| r.standing == Standing::Lost)
        .unwrap();
    assert_eq!(lost.class, None, "the board-wide record loses `min`");
    assert!(lost.fields.contains(&"min".to_owned()));
    let clearance = rules
        .kinds
        .iter()
        .find(|k| k.kind == RuleKind::Clearance)
        .unwrap();
    let disabled = clearance
        .rows
        .iter()
        .find(|r| r.standing == Standing::Disabled)
        .unwrap();
    assert_eq!(disabled.rule, off);
    assert_eq!(
        reda_ui::properties::rules::page(RuleKind::TrackWidth),
        "Track Width"
    );

    // Two objects say which clearance applies between them; three hide the region.
    let v = via(&mut shell, "9mm", None);
    run(&mut shell, "selection.set", json!({"objects": [t, v]}));
    let rules = properties(&shell).rules.unwrap();
    let clearance = rules
        .kinds
        .iter()
        .find(|k| k.kind == RuleKind::Clearance)
        .unwrap();
    assert!(clearance.between.is_some());
    let w = via(&mut shell, "12mm", None);
    run(&mut shell, "selection.set", json!({"objects": [t, v, w]}));
    assert!(properties(&shell).rules.is_none());
}

#[test]
fn a_kind_chip_narrows_the_selection_to_that_kind() {
    let dir = TempDir::new("properties-regions-chips");
    let mut shell = shell(&dir);
    let a = via(&mut shell, "0mm", None);
    let t = track(&mut shell, None);
    let b = via(&mut shell, "5mm", None);
    run(&mut shell, "selection.set", json!({"objects": [a, t, b]}));
    let p = properties(&shell);
    assert_eq!(p.kinds, [("via".to_owned(), 2), ("track".to_owned(), 1)]);
    let narrow = p.narrow("via").unwrap();
    assert_eq!(narrow, ("selection.set", json!({"objects": [a, b]})));
    commit(&mut shell, narrow);
    assert_eq!(shell.selection().objects, [a, b]);
    assert_eq!(p.narrow("zone"), None);
}

#[test]
fn a_locally_edited_pad_shows_its_edits_and_reverts() {
    let dir = TempDir::new("properties-regions-local-edit");
    let mut shell = shell(&dir);
    let footprint = synced_footprint(&mut shell);
    let pad = {
        let pcb = shell.engine().project().unwrap().pcb().unwrap();
        let copy = pcb
            .footprint_copy(pcb.footprint(footprint).unwrap().footprint)
            .unwrap();
        copy.footprint.pads[0].uuid
    };
    run(
        &mut shell,
        "selection.set",
        json!({"pads": [{"footprint": footprint, "pad": pad}]}),
    );
    assert!(properties(&shell).local_edits.is_none());

    run(
        &mut shell,
        "pcb.pad.edit",
        json!({"instance": footprint, "pad": pad, "width": "2mm"}),
    );
    let p = properties(&shell);
    let edits = p
        .local_edits
        .as_ref()
        .expect("the Pad's Footprint is locally edited");
    assert_eq!(edits.instances, [footprint]);
    assert!(
        edits
            .edits
            .iter()
            .any(|e| e.pad == Some(pad) && e.property == "width"),
        "{:?}",
        edits.edits
    );
    let (edited, original) = edits.compare.as_ref().unwrap();
    assert_ne!(edited.pads[0].width, original.pads[0].width);

    let revert = p.revert().unwrap();
    assert_eq!(
        revert,
        (
            "pcb.footprint.revert_local_edits",
            json!({"instances": [footprint]})
        )
    );
    commit(&mut shell, revert);
    assert!(properties(&shell).local_edits.is_none());
}

#[test]
fn the_check_summary_offers_fix_for_health_checks() {
    let dir = TempDir::new("properties-regions-health");
    let mut shell = shell(&dir);
    track(&mut shell, None);
    let duplicate = track(&mut shell, None);
    let project = shell.engine().project().unwrap();
    let figures = BoardFigures::of(project).unwrap();
    let summary = CheckSummary::of(&shell, CheckKind::Drc, Some(&figures));
    assert_eq!(summary.ran_at, None);
    assert_eq!(summary.health.len(), 1);
    let line = &summary.health[0];
    assert_eq!((line.check, line.count), (DrcCheck::DuplicateCopper, 1));
    assert_eq!(line.fix, ("drc.fix", json!({"check": "duplicate_copper"})));

    commit(&mut shell, line.fix.clone());
    let project = shell.engine().project().unwrap();
    assert!(project.object(duplicate).is_none());
    assert!(!figures.current(project), "the board changed");
    let figures = BoardFigures::of(project).unwrap();
    assert!(
        CheckSummary::of(&shell, CheckKind::Drc, Some(&figures))
            .health
            .is_empty()
    );

    shell.run_check(CheckKind::Drc).unwrap();
    let summary = CheckSummary::of(&shell, CheckKind::Drc, Some(&figures));
    assert!(summary.ran_at.is_some());
    assert!(!summary.stale);
}

#[test]
fn ready_for_fab_lists_what_stands_between_the_board_and_fab() {
    let dir = TempDir::new("properties-regions-fab");
    let mut shell = shell(&dir);
    let vcc = uuid(&run(&mut shell, "test.net.add", json!({"name": "VCC"}))["uuid"]);
    for x in ["0mm", "8mm"] {
        run(
            &mut shell,
            "pcb.free_pad.add",
            json!({"kind": "plated_through_hole", "at": {"x": x, "y": 0}, "width": "2mm",
                   "hole": {"width": "1mm"}, "net": vcc}),
        );
    }
    let figures = BoardFigures::of(shell.engine().project().unwrap()).unwrap();
    let lines = ready_for_fab(&shell, &figures);
    let items: Vec<FabItem> = lines.iter().map(|l| l.item).collect();
    assert_eq!(
        items,
        [
            FabItem::Outline,
            FabItem::Unrouted,
            FabItem::Zones,
            FabItem::Drc,
            FabItem::Sync,
            FabItem::NotFitted
        ]
    );
    let unrouted = &lines[1];
    assert_eq!((unrouted.ok, unrouted.count), (false, Some(1)));
    assert_eq!(
        unrouted.action,
        Some(FabAction::Run(
            "pcb.ratsnest.show",
            json!({"visible": true})
        ))
    );
    assert_eq!(lines[3].action, Some(FabAction::RunDrc));

    shell.run_check(CheckKind::Drc).unwrap();
    let lines = ready_for_fab(&shell, &figures);
    assert!(lines[3].ok && lines[3].action.is_none());
    // An edit makes the run stale again.
    run(
        &mut shell,
        "pcb.via.add",
        json!({"at": {"x": "9mm", "y": 0}}),
    );
    let figures = BoardFigures::of(shell.engine().project().unwrap()).unwrap();
    assert!(!ready_for_fab(&shell, &figures)[3].ok);
}
