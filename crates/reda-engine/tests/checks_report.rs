//! `checks.report`: an ERC, DRC or Sync report written as JSON or HTML.

use std::path::{Path, PathBuf};

use reda_engine::command::{ErrorCode, FixedClock, SeededIds, Timestamp};
use reda_testkit::{TempDir, TestEngine};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
/// The starter Library's Resistor and its 0603 Footprint.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESC1608: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";

fn starter_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn engine_at(clock: FixedClock) -> TestEngine {
    TestEngine(
        reda_engine::Engine::builder()
            .ids(SeededIds::new(0))
            .clock(clock)
            .user_dirs(None)
            .starter_library(Some(starter_library()))
            .build(),
    )
}

fn narrow_track(engine: &mut TestEngine, y: &str, length: &str) -> String {
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.1mm",
               "points": [{"x": 0, "y": y}, {"x": length, "y": y}]}),
    );
    out["result"]["tracks"][0].as_str().unwrap().to_owned()
}

/// A Project whose board has: a too-narrow Track, a waived too-narrow Track, a stale
/// Waiver (its Track deleted, reason "gone") and a micro-segment (a health finding); and
/// whose sheet has a Resistor not yet on the board.
fn design(dir: &Path, clock: FixedClock) -> TestEngine {
    let mut e = engine_at(clock);
    e.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    narrow_track(&mut e, "0mm", "5mm");
    let waived = narrow_track(&mut e, "10mm", "5mm");
    let gone = narrow_track(&mut e, "20mm", "5mm");
    narrow_track(&mut e, "30mm", "0.005mm");
    e.run_ok(
        "drc.waive",
        json!({"violations": [{"check": "track_width", "objects": [waived]}], "reason": "coupon"}),
    );
    e.run_ok(
        "drc.waive",
        json!({"violations": [{"check": "track_width", "objects": [gone]}], "reason": "gone"}),
    );
    e.run_ok("pcb.track.delete", json!({"tracks": [gone]}));
    e.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": RESISTOR, "footprint": RESC1608,
               "at": {"x": "1000mil", "y": "1000mil"}}),
    );
    e
}

fn report(e: &mut TestEngine, params: Value) -> (Value, String) {
    let out = e.run_ok("checks.report", params.clone());
    let path = PathBuf::from(out["path"].as_str().unwrap());
    let text = std::fs::read_to_string(&path).unwrap();
    (out, text)
}

fn checks(violations: &Value) -> Vec<String> {
    violations
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["check"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_drc_json_report_holds_the_run_result_its_summary_and_stale_waivers() {
    let dir = TempDir::new("report-drc-json");
    let mut e = design(dir.path(), FixedClock::EPOCH);
    let run = e.run_ok("drc.run", json!({"include_waived": true}));
    let path = dir.path().join("drc.json");
    let (out, text) = report(
        &mut e,
        json!({"kind": "drc", "format": "json", "path": path}),
    );
    assert_eq!(out["path"], path.to_str().unwrap());
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["kind"], "drc");
    assert_eq!(file["generated"], 1_767_225_600_000_i64);
    assert_eq!(file["project"], "Demo");
    assert_eq!(file["partial"], false);
    // Waived Violations are always listed, each naming its Waiver.
    assert_eq!(file["violations"], run["violations"]);
    assert_eq!(file["design"], run["design"]);
    assert!(checks(&file["violations"]).contains(&"micro_segment".to_owned()));
    let width = file["summary"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["check"] == "track_width")
        .unwrap()
        .clone();
    assert_eq!(
        width,
        json!({"check": "track_width", "severity": "error", "count": 2, "waived": 1})
    );
    let stale = file["stale_waivers"].as_array().unwrap();
    assert_eq!(stale.len(), 1, "{text}");
    assert_eq!(stale[0]["reason"], "gone");
    assert_eq!(stale[0]["check"], "track_width");
    assert_eq!(stale[0]["uuid"], run["unmatched_waivers"][0]);
}

#[test]
fn a_report_takes_its_run_commands_parameters() {
    let dir = TempDir::new("report-params");
    let mut e = design(dir.path(), FixedClock::EPOCH);
    let path = dir.path().join("width.json");
    let (_, text) = report(
        &mut e,
        json!({"kind": "drc", "format": "json", "path": path,
               "checks": ["track_width"], "layers": ["copper_top"]}),
    );
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["partial"], true);
    assert!(
        checks(&file["violations"])
            .iter()
            .all(|c| c == "track_width")
    );
    assert_eq!(file["stale_waivers"], json!([]));

    let erc = dir.path().join("erc.json");
    let (_, text) = report(
        &mut e,
        json!({"kind": "erc", "format": "json", "path": erc, "documents": [SHEET]}),
    );
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["kind"], "erc");
    assert_eq!(file["partial"], true);
    let run = e.run_ok(
        "erc.run",
        json!({"documents": [SHEET], "include_waived": true}),
    );
    assert_eq!(file["violations"], run["violations"]);

    let err = e.run_err(
        "checks.report",
        json!({"kind": "erc", "format": "json", "path": erc, "layers": ["copper_top"]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = e.run_err(
        "checks.report",
        json!({"kind": "sync", "format": "json", "path": erc, "checks": []}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn a_relative_path_is_taken_from_the_project_folder_and_runs_stay_pure_queries() {
    let dir = TempDir::new("report-relative");
    let mut e = design(dir.path(), FixedClock::EPOCH);
    e.run_ok("project.save", json!({}));
    let (out, _) = report(
        &mut e,
        json!({"kind": "drc", "format": "html", "path": "reports/drc.html"}),
    );
    let written = dir.path().join("Demo/reports/drc.html");
    assert_eq!(out["path"], written.to_str().unwrap());
    assert!(written.is_file());
    // Writing a report edits nothing.
    assert!(!e.0.is_dirty());
}

#[test]
fn a_drc_html_report_has_a_per_check_summary_and_waived_stale_waiver_and_health_sections() {
    let dir = TempDir::new("report-drc-html");
    let mut e = design(dir.path(), FixedClock::EPOCH);
    let (_, html) = report(
        &mut e,
        json!({"kind": "drc", "format": "html", "path": dir.path().join("drc.html")}),
    );
    assert!(html.starts_with("<!doctype html>"), "{html}");
    assert!(html.contains("2026-01-01 00:00:00 UTC"), "{html}");
    for section in [
        "id=\"summary\"",
        "id=\"violations\"",
        "id=\"waived\"",
        "id=\"stale-waivers\"",
        "id=\"health\"",
    ] {
        assert!(html.contains(section), "missing {section}");
    }
    let section = |id: &str| {
        let start = html.find(&format!("id=\"{id}\"")).unwrap();
        let end = html[start..].find("</section>").unwrap() + start;
        html[start..end].to_owned()
    };
    assert!(section("summary").contains("<td>track_width</td>"));
    assert!(
        section("waived").contains("coupon"),
        "{}",
        section("waived")
    );
    assert!(section("stale-waivers").contains("gone"));
    assert!(section("health").contains("micro_segment"));
    assert!(!section("violations").contains("micro_segment"));
}

#[test]
fn an_erc_html_report_has_no_health_section() {
    let dir = TempDir::new("report-erc-html");
    let mut e = design(dir.path(), FixedClock::EPOCH);
    let (_, html) = report(
        &mut e,
        json!({"kind": "erc", "format": "html", "path": dir.path().join("erc.html")}),
    );
    assert!(html.contains("id=\"summary\""));
    assert!(!html.contains("id=\"health\""));
    assert!(html.contains("unconnected_pin"), "{html}");
}

#[test]
fn a_sync_report_writes_the_change_list_from_sync_preview() {
    let dir = TempDir::new("report-sync");
    let mut e = design(dir.path(), FixedClock::EPOCH);
    let preview = e.run_ok("sync.preview", json!({}));
    let (out, text) = report(
        &mut e,
        json!({"kind": "sync", "format": "json", "path": dir.path().join("sync.json")}),
    );
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["kind"], "sync");
    assert_eq!(file["change_list"], preview);
    assert_eq!(out["count"], preview["count"]);
    let (_, html) = report(
        &mut e,
        json!({"kind": "sync", "format": "html", "path": dir.path().join("sync.html")}),
    );
    assert!(html.contains("add_component"), "{html}");
    assert!(html.contains("R1"), "{html}");
}

#[test]
fn reports_are_byte_for_byte_the_same_under_the_fixed_clock() {
    let one = TempDir::new("report-same-1");
    let two = TempDir::new("report-same-2");
    let mut engines = [
        design(one.path(), FixedClock::EPOCH),
        design(two.path(), FixedClock::EPOCH),
    ];
    for kind in ["erc", "drc", "sync"] {
        for format in ["json", "html"] {
            let name = format!("{kind}.{format}");
            let mut texts = Vec::new();
            for e in &mut engines {
                for _ in 0..2 {
                    let params = json!({"kind": kind, "format": format, "path": name});
                    let path = e.run_ok("checks.report", params)["path"].clone();
                    texts.push(std::fs::read(path.as_str().unwrap()).unwrap());
                }
            }
            assert!(texts.windows(2).all(|w| w[0] == w[1]), "{name} differs");
        }
    }
    // Another Clock changes only the times it reads, when the report and the Waivers
    // were made, and so the fingerprint of the board that holds the Waivers.
    let three = TempDir::new("report-same-3");
    let later = FixedClock(Timestamp(FixedClock::EPOCH.0.0 + 86_400_000));
    let mut e = design(three.path(), later);
    let path = e.run_ok(
        "checks.report",
        json!({"kind": "drc", "format": "json", "path": "drc.json"}),
    )["path"]
        .clone();
    let mut file: Value =
        serde_json::from_str(&std::fs::read_to_string(path.as_str().unwrap()).unwrap()).unwrap();
    let mut first: Value =
        serde_json::from_slice(&std::fs::read(one.path().join("Demo/drc.json")).unwrap()).unwrap();
    assert_eq!(file["generated"], 1_767_312_000_000_i64);
    assert_eq!(file["stale_waivers"][0]["created"], 1_767_312_000_000_i64);
    for f in [&mut file, &mut first] {
        f["generated"] = Value::Null;
        f["stale_waivers"][0]["created"] = Value::Null;
        f["design"] = Value::Null;
    }
    assert_eq!(file, first);
}
