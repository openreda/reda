//! Free Pads (plated or non-plated, optionally on a Net) and the Output Origin.

mod common;

use common::{PCB, SHEET, created, engine, pcb, synced};
use reda_command::ErrorCode;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{ConnectionStyle, Corner, Hole, PadKind, PadShape, Ratio, Side, Uuid};
use reda_testkit::{TempDir, TestEngine};
use serde_json::json;

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

/// Gives R1's first Pin the Net Label `SIG` and syncs, so the board has a Net `SIG`.
fn net_sig(engine: &mut TestEngine) -> Uuid {
    synced(engine, 1);
    let nets = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    let pin = nets["nets"][0]["pins"][0]["at"].clone();
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SIG", "at": pin}),
    );
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash": hash}));
    pcb(engine).nets().find(|n| n.name == "SIG").unwrap().uuid
}

#[test]
fn a_plated_free_pad_goes_on_a_net() {
    let dir = TempDir::new("free-pad-plated");
    let mut engine = engine(&dir);
    let sig = net_sig(&mut engine);

    let out = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "at": {"x": "3mm", "y": "4mm"},
               "width": "6mm", "hole": {"width": "3.2mm"}, "net": sig}),
    );

    let pad = pcb(&engine).free_pad(created(&out)).unwrap();
    assert_eq!(pad.kind, PadKind::PlatedThroughHole);
    assert_eq!(pad.shape, PadShape::Circle);
    assert_eq!(pad.at, mm(3, 4));
    assert_eq!((pad.width, pad.height), (Nm::from_mm(6), Nm::from_mm(6)));
    assert_eq!(
        pad.hole,
        Some(Hole::new(Nm::from_um(3200), Nm::from_um(3200)))
    );
    assert_eq!(pad.side, None);
    assert_eq!(pad.net, Some(sig));
}

#[test]
fn smd_and_non_plated_free_pads() {
    let dir = TempDir::new("free-pad-kinds");
    let mut engine = engine(&dir);

    let fiducial = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": 0, "y": 0}, "width": "1mm", "side": "bottom"}),
    );
    let fiducial = pcb(&engine).free_pad(created(&fiducial)).unwrap().clone();
    assert_eq!(fiducial.side, Some(Side::Bottom));
    assert_eq!(fiducial.hole, None);

    let smd_top = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "shape": "rect", "at": {"x": 0, "y": 0}, "width": "2mm",
               "height": "1mm", "rotation": "45deg"}),
    );
    let smd_top = pcb(&engine).free_pad(created(&smd_top)).unwrap();
    assert_eq!(smd_top.side, None, "SMD Pads default to the top, left out");
    assert_eq!(smd_top.form().copper_side(), Some(Side::Top));
    assert_eq!(smd_top.rotation, Millideg::from_deg(45));

    let hole = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "at": {"x": "10mm", "y": 0}, "width": "3.2mm",
               "hole": {"width": "3.2mm"}}),
    );
    assert_eq!(pcb(&engine).free_pad(created(&hole)).unwrap().net, None);
}

#[test]
fn free_pads_with_a_bad_shape_or_net_are_refused() {
    let dir = TempDir::new("free-pad-bad");
    let mut engine = engine(&dir);
    let sig = net_sig(&mut engine);
    let at = json!({"x": 0, "y": 0});

    for (params, pointer) in [
        (
            json!({"kind": "smd", "at": at, "width": "1mm", "hole": {"width": "0.5mm"}}),
            "/hole",
        ),
        (
            json!({"kind": "plated_through_hole", "at": at, "width": "1mm"}),
            "/hole",
        ),
        (
            json!({"kind": "plated_through_hole", "at": at, "width": "1mm",
                   "hole": {"width": "2mm"}}),
            "/hole",
        ),
        (
            json!({"kind": "plated_through_hole", "at": at, "width": "2mm",
                   "hole": {"width": "1mm"}, "side": "top"}),
            "/side",
        ),
        (
            json!({"kind": "non_plated", "at": at, "width": "2mm",
                   "hole": {"width": "2mm"}, "net": sig}),
            "/net",
        ),
        (json!({"kind": "smd", "at": at, "width": 0}), "/width"),
        (
            json!({"kind": "smd", "at": at, "width": "1mm", "net": PCB}),
            "/net",
        ),
    ] {
        let err = engine.run_err("pcb.free_pad.add", params.clone());
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
        assert_eq!(err.details.unwrap()["pointer"], pointer, "{params}");
    }
}

#[test]
fn a_free_pads_net_can_be_set_and_cleared() {
    let dir = TempDir::new("free-pad-net");
    let mut engine = engine(&dir);
    let sig = net_sig(&mut engine);
    let pad = created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": 0, "y": 0}, "width": "1mm"}),
    ));

    engine.run_ok("pcb.free_pad.set_net", json!({"pads": [pad], "net": sig}));
    assert_eq!(pcb(&engine).free_pad(pad).unwrap().net, Some(sig));
    engine.run_ok("pcb.free_pad.set_net", json!({"pads": [pad], "net": null}));
    assert_eq!(pcb(&engine).free_pad(pad).unwrap().net, None);

    let hole = created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "at": {"x": 0, "y": 0}, "width": "1mm",
               "hole": {"width": "1mm"}}),
    ));
    let err = engine.run_err("pcb.free_pad.set_net", json!({"pads": [hole], "net": sig}));
    assert_eq!(err.details.unwrap()["pointer"], "/pads/0");
}

#[test]
fn sync_takes_a_removed_net_off_free_pads() {
    let dir = TempDir::new("free-pad-sync");
    let mut engine = engine(&dir);
    let sig = net_sig(&mut engine);
    let pad = created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": 0, "y": 0}, "width": "1mm", "net": sig}),
    ));
    let label = engine.run_ok("objects.query", json!({"type": "net_label"}))["objects"][0]
        ["object"]["uuid"]
        .clone();
    engine.run_ok("schematic.delete", json!({"objects": [label]}));

    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash": hash}));

    assert!(pcb(&engine).net(sig).is_none());
    assert_eq!(pcb(&engine).free_pad(pad).unwrap().net, None);
}

#[test]
fn free_pads_move_turn_flip_and_delete() {
    let dir = TempDir::new("free-pad-move");
    let mut engine = engine(&dir);
    let pad = created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": "1mm", "y": "1mm"}, "width": "1mm"}),
    ));

    engine.run_ok(
        "pcb.move",
        json!({"objects": [pad], "by": {"x": "1mm", "y": 0}}),
    );
    engine.run_ok("pcb.rotate", json!({"objects": [pad], "angle": "30deg"}));
    engine.run_ok("pcb.flip", json!({"objects": [pad]}));
    let moved = pcb(&engine).free_pad(pad).unwrap();
    assert_eq!(moved.at, mm(2, 1));
    assert_eq!(moved.rotation, Millideg::from_deg(330));
    assert_eq!(moved.side, Some(Side::Bottom));

    engine.run_ok("pcb.free_pad.delete", json!({"pads": [pad]}));
    assert!(pcb(&engine).free_pad(pad).is_none());
}

#[test]
fn a_free_pad_has_the_whole_pad_model_and_flipping_mirrors_it() {
    let dir = TempDir::new("free-pad-model");
    let mut engine = engine(&dir);
    let pad = created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "chamfered_rect",
               "at": {"x": "10mm", "y": "0mm"}, "width": "3mm", "height": "2mm",
               "chamfer_ratio": "50%", "corner_ratio": 20_000, "corners": ["top_right"],
               "offset": {"x": "0.5mm", "y": 0},
               "hole": {"width": "1.2mm", "height": "0.6mm", "rotation": "30deg",
                        "tolerance": {"plus": "0.05mm", "minus": "0.05mm"}},
               "zone_connection": {"style": "solid"}}),
    ));
    let stored = pcb(&engine).free_pad(pad).unwrap().clone();
    assert_eq!(stored.chamfer_ratio, Some(Ratio::percent(50)));
    assert_eq!(stored.corner_ratio, Some(Ratio::percent(20)));
    assert_eq!(
        stored.corners,
        Some([Corner::TopRight].into_iter().collect())
    );
    assert_eq!(stored.offset, Some(Point::new(Nm::from_um(500), Nm(0))));
    let hole = stored.hole.unwrap();
    assert_eq!(hole.rotation, Millideg::from_deg(30));
    assert_eq!(hole.tolerance.unwrap().plus, Nm::from_um(50));
    assert_eq!(
        stored.zone_connection.unwrap().style,
        Some(ConnectionStyle::Solid)
    );

    engine.run_ok("pcb.flip", json!({"objects": [pad]}));
    let flipped = pcb(&engine).free_pad(pad).unwrap();
    assert_eq!(
        flipped.corners,
        Some([Corner::TopLeft].into_iter().collect())
    );
    assert_eq!(flipped.offset, Some(Point::new(Nm::from_um(-500), Nm(0))));
    assert_eq!(flipped.hole.unwrap().rotation, Millideg::from_deg(150));

    let smd = created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": 0, "y": 0}, "width": "1mm"}),
    ));
    engine.run_ok("pcb.flip", json!({"objects": [smd]}));
    assert_eq!(pcb(&engine).free_pad(smd).unwrap().side, Some(Side::Bottom));
    engine.run_ok("pcb.flip", json!({"objects": [smd]}));
    assert_eq!(pcb(&engine).free_pad(smd).unwrap().side, None);

    let err = engine.run_err(
        "pcb.free_pad.add",
        json!({"kind": "smd", "shape": "chamfered_rect", "at": {"x": 0, "y": 0},
               "width": "1mm"}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/chamfer_ratio");
}

#[test]
fn the_output_origin_is_optional_and_single() {
    let dir = TempDir::new("placement-origin");
    let mut engine = engine(&dir);
    assert!(pcb(&engine).output_origin().is_none());

    let origin = created(&engine.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "2mm", "y": "3mm"}}),
    ));
    assert_eq!(pcb(&engine).output_origin().unwrap().at, mm(2, 3));

    let again = created(&engine.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "5mm", "y": "5mm"}}),
    ));
    assert_eq!(again, origin, "setting it again moves the one origin");
    assert_eq!(pcb(&engine).output_origin().unwrap().at, mm(5, 5));

    engine.run_ok(
        "pcb.move",
        json!({"objects": [origin], "by": {"x": "1mm", "y": 0}}),
    );
    assert_eq!(pcb(&engine).output_origin().unwrap().at, mm(6, 5));

    engine.run_ok("pcb.output_origin.delete", json!({}));
    assert!(pcb(&engine).output_origin().is_none());
    let err = engine.run_err("pcb.output_origin.delete", json!({}));
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn free_pads_and_the_output_origin_survive_save_and_open() {
    let dir = TempDir::new("free-pad-save");
    let mut engine = engine(&dir);
    let sig = net_sig(&mut engine);
    let pad = created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "rounded_rect",
               "at": {"x": "3mm", "y": "4mm"}, "width": "3mm", "height": "2mm",
               "corner_ratio": "50%", "hole": {"width": "1mm", "height": "1.5mm"},
               "rotation": "90deg", "net": sig}),
    ));
    engine.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "2mm", "y": "3mm"}}),
    );
    let before = pcb(&engine).clone();
    engine.run_ok("project.save", json!({}));
    let text = std::fs::read_to_string(dir.path().join("Demo/Demo.redapcb")).unwrap();
    assert!(text.contains("\"free_pads\""), "{text}");
    assert!(text.contains("\"output_origin\""), "{text}");

    let mut reopened = reda_testkit::test_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    let after = reopened.project().unwrap().pcb().unwrap();
    assert_eq!(after.free_pad(pad), before.free_pad(pad));
    assert_eq!(after.output_origin(), before.output_origin());
}
