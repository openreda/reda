//! Smart Track Ends: where a partly routed connection's Ratsnest line starts.

use reda_model::geom::Point;
use reda_pcb::{Copper, RatsnestEnds};
use reda_testkit::{TempDir, probe_engine};
use serde_json::json;

fn mm(v: i64) -> reda_model::geom::Nm {
    reda_model::geom::Nm(v * 1_000_000)
}

/// Two Pads of SIG 10 mm apart, with a Track leading 6 mm from the left one towards the
/// right one.
fn partly_routed() -> (TempDir, std::sync::Arc<Copper>) {
    let dir = TempDir::new("ratsnest-ends");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let sig = engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"].clone();
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": sig},
            {"at": {"x": "10mm", "y": 0}, "width": "1mm", "height": "1mm", "net": sig}]}),
    );
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": sig, "points": [
            {"x": 0, "y": 0}, {"x": "6mm", "y": 0}]}),
    );
    let copper = Copper::of(engine.project().unwrap()).unwrap();
    (dir, copper)
}

#[test]
fn under_smart_track_ends_the_line_starts_at_the_track_end() {
    let (_dir, copper) = partly_routed();
    let lines = copper.ratsnest_with(RatsnestEnds::SmartTrackEnds);
    assert_eq!(lines.len(), 1);
    let ends = [lines[0].from, lines[0].to];
    assert!(ends.contains(&Point::new(mm(6), mm(0))), "{ends:?}");
    assert_eq!(copper.ratsnest(), lines, "Smart Track Ends is the default");
}

#[test]
fn without_smart_track_ends_the_line_starts_at_the_nearest_pad() {
    let (_dir, copper) = partly_routed();
    let lines = copper.ratsnest_with(RatsnestEnds::Pads);
    assert_eq!(lines.len(), 1);
    let ends = [lines[0].from, lines[0].to];
    assert!(ends.contains(&Point::new(mm(0), mm(0))), "{ends:?}");
    assert!(ends.contains(&Point::new(mm(10), mm(0))), "{ends:?}");
}
