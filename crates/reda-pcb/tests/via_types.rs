//! Via types are identified by their unordered span, independent of table order.
mod common;

use common::{PCB, created, pcb};
use reda_command::ErrorCode;
use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::json;

fn board(dir: &TempDir) -> (TestEngine, [Uuid; 6]) {
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Vias","layers":6}),
    );
    let layers = pcb(&e)
        .copper_layers()
        .map(|l| l.uuid)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    (e, layers)
}

#[test]
fn add_a_named_micro_type_and_place_it_in_its_drilling_direction() {
    let dir = TempDir::new("via-type-place");
    let (mut e, l) = board(&dir);
    let kind = created(&e.run_ok(
        "pcb.via_type.add",
        json!({
            "document":PCB,"from":l[1],"to":l[0],"micro":true,"name":"laser",
            "hole":"0.12mm","diameter":{"start":"0.28mm","end":"0.3mm"}
        }),
    ));
    let via = created(&e.run_ok(
        "pcb.via.add",
        json!({"from":l[0],"to":l[1],"at":{"x":0,"y":0}}),
    ));
    let via_json = serde_json::to_value(pcb(&e).via(via).unwrap()).unwrap();
    assert_eq!(via_json["span"], json!({"from":l[1],"to":l[0]}));
    assert_eq!(via_json["hole"], 120000);
    assert_eq!(via_json["diameter"], json!({"start":280000,"end":300000}));
    let types = serde_json::to_value(&pcb(&e).stackup().via_types).unwrap();
    assert_eq!(types[1]["uuid"], json!(kind));
    assert_eq!(types[1]["name"], "laser");
    assert_eq!(types[1]["micro"], true);
    assert_eq!(
        e.run_err(
            "pcb.via_type.add",
            json!({"document":PCB,"from":l[0],"to":l[1]})
        )
        .code,
        ErrorCode::Rejected
    );
}

#[test]
fn previewing_and_moving_a_used_span_preserves_sizes_and_undo_restores_both() {
    let dir = TempDir::new("via-type-move");
    let (mut e, l) = board(&dir);
    let kind = created(&e.run_ok(
        "pcb.via_type.add",
        json!({"document":PCB,"from":l[0],"to":l[1]}),
    ));
    let via = created(&e.run_ok(
        "pcb.via.add",
        json!({"at":{"x":0,"y":0},"from":l[0],"to":l[1],"diameter":"1mm"}),
    ));
    let before = pcb(&e).via(via).unwrap().clone();
    let edit = json!({"via_type":kind,"from":l[3],"to":l[2],"hole":"0.4mm"});
    let preview = e.run_ok(
        "pcb.stackup.change_preview",
        json!({"change":{"id":"pcb.via_type.set","params":edit}}),
    );
    assert_eq!(preview["vias_using_span"], 1);
    assert_eq!(preview["vias_without_type"], 1);
    assert_eq!(pcb(&e).via(via).unwrap(), &before);
    e.run_ok("pcb.via_type.set", edit.clone());
    assert_eq!(pcb(&e).via(via).unwrap(), &before);
    e.run_ok("edit.undo", json!({"document":PCB}));
    let mut moving = edit;
    moving["move_vias"] = json!(true);
    e.run_ok("pcb.via_type.set", moving);
    let moved = pcb(&e).via(via).unwrap();
    assert_eq!((moved.span.from, moved.span.to), (l[3], l[2]));
    assert_eq!((moved.hole, moved.diameter), (before.hole, before.diameter));
    e.run_ok("edit.undo", json!({"document":PCB}));
    assert_eq!(pcb(&e).via(via).unwrap(), &before);
    assert_eq!(pcb(&e).stackup().via_types[1].to, l[1]);
}

#[test]
fn removing_a_type_previews_empty_class_permissions_and_undo_restores_them() {
    let dir = TempDir::new("via-type-class");
    let (mut e, l) = board(&dir);
    let kind = created(&e.run_ok(
        "pcb.via_type.add",
        json!({"document":PCB,"from":l[2],"to":l[1]}),
    ));
    let net = created(&e.run_ok("test.net.add", json!({"name":"Power"})));
    let class = created(&e.run_ok("pcb.net_class.add", json!({"name":"Power","nets":[net]})));
    let scope = json!({"net_class":class});
    let record = e.run_ok(
        "pcb.rules.set",
        json!({"kind":"via_span","scope":scope,"values":{"allowed":[kind]}}),
    );
    assert_eq!(record["result"]["rule"]["values"]["allowed"], json!([kind]));
    e.run_ok("pcb.rules.set",json!({"kind":"via_size","scope":scope,"values":{"hole":{"preferred":"0.4mm"},"diameter":{"preferred":"1mm"}}}));
    let via = created(&e.run_ok(
        "pcb.via.add",
        json!({"at":{"x":0,"y":0},"net":net,"from":l[1],"to":l[2]}),
    ));
    assert_eq!(pcb(&e).via(via).unwrap().hole.0, 400000);
    assert_eq!(pcb(&e).via(via).unwrap().diameter.start.0, 1000000);
    let p = json!({"via_type":kind});
    let preview = e.run_ok(
        "pcb.stackup.change_preview",
        json!({"change":{"id":"pcb.via_type.remove","params":p}}),
    );
    assert_eq!(preview["vias_without_type"], 1);
    assert_eq!(preview["allowed_lists_emptied"], 1);
    e.run_ok("pcb.via_type.remove", p);
    assert_eq!(
        serde_json::to_value(pcb(&e).class_rule(reda_model::RuleKind::ViaSpan, class)).unwrap()["values"]
            ["allowed"],
        json!([])
    );
    assert!(pcb(&e).via(via).is_some());
    e.run_ok("edit.undo", json!({"document":PCB}));
    let restored =
        serde_json::to_value(pcb(&e).class_rule(reda_model::RuleKind::ViaSpan, class)).unwrap();
    assert_eq!(restored["values"]["allowed"], json!([kind]));
}

#[test]
fn reordering_keeps_the_through_type_permanent_and_micro_spans_short() {
    let dir = TempDir::new("via-type-order");
    let (mut e, l) = board(&dir);
    let thru = pcb(&e).stackup().through_type().uuid;
    let blind = created(&e.run_ok(
        "pcb.via_type.add",
        json!({"document":PCB,"from":l[5],"to":l[4]}),
    ));
    let buried = created(&e.run_ok(
        "pcb.via_type.add",
        json!({"document":PCB,"from":l[2],"to":l[1],"name":"Inner"}),
    ));
    e.run_ok(
        "pcb.via_type.reorder",
        json!({"via_type":buried,"before":thru}),
    );
    assert_eq!(
        pcb(&e)
            .stackup()
            .via_types
            .iter()
            .map(|t| t.uuid)
            .collect::<Vec<_>>(),
        [buried, thru, blind]
    );
    e.run_ok("pcb.via_type.set", json!({"via_type":buried,"name":null}));
    assert_eq!(
        serde_json::to_value(&pcb(&e).stackup().via_types[0])
            .unwrap()
            .get("name"),
        None
    );
    assert_eq!(
        e.run_err("pcb.via_type.remove", json!({"via_type":thru}))
            .code,
        ErrorCode::Rejected
    );
    for patch in [json!({"to":l[3]}), json!({"micro":true})] {
        let mut p = patch;
        p["via_type"] = json!(thru);
        assert_eq!(e.run_err("pcb.via_type.set", p).code, ErrorCode::Rejected);
    }
    assert_eq!(
        e.run_err(
            "pcb.via_type.set",
            json!({"via_type":blind,"to":l[1],"micro":true})
        )
        .code,
        ErrorCode::InvalidParams
    );
    assert_eq!(
        e.run_err(
            "pcb.via_type.set",
            json!({"via_type":blind,"from":l[2],"to":l[1]})
        )
        .code,
        ErrorCode::Rejected
    );
}

#[test]
fn unmatched_vias_can_be_edited_and_only_open_mask_on_sides_they_reach() {
    let dir = TempDir::new("via-mask");
    let (mut e, l) = board(&dir);
    let via = created(&e.run_ok(
        "pcb.via.add",
        json!({"at":{"x":0,"y":0},"from":l[4],"to":l[5],
         "mask":{"top":{"expansion":"0.1mm"},"bottom":{"expansion":"0.1mm"}}}),
    ));
    let sides = |e: &TestEngine| {
        reda_pcb::openings::resolve(pcb(e))
            .into_iter()
            .filter(|o| o.objects.contains(&via))
            .map(|o| o.side)
            .collect::<Vec<_>>()
    };
    assert_eq!(sides(&e), [reda_model::Side::Bottom]);
    e.run_ok("pcb.via.set",json!({"vias":[via],"to":l[2],"diameter":{"start":"0.9mm","middle":"1mm"},"unused_layers":"remove_keep_ends","teardrop":false,"zone_connection":{"style":"solid"}}));
    assert!(sides(&e).is_empty());
    let data = serde_json::to_value(pcb(&e).via(via).unwrap()).unwrap();
    assert_eq!(data["span"], json!({"from":l[4],"to":l[2]}));
    assert_eq!(data["diameter"], json!({"start":900000,"middle":1000000}));
    assert_eq!(data["unused_layers"], "remove_keep_ends");
    assert_eq!(data["zone_connection"]["style"], "solid");
    assert_eq!(data["teardrop"], false);
    e.run_ok("edit.undo", json!({"document":PCB}));
    assert_eq!(sides(&e), [reda_model::Side::Bottom]);
}

#[test]
fn a_new_type_claims_existing_unmatched_vias_in_its_drilling_direction() {
    let dir = TempDir::new("via-new-match");
    let (mut e, l) = board(&dir);
    let via = created(&e.run_ok(
        "pcb.via.add",
        json!({"at":{"x":0,"y":0},"from":l[1],"to":l[2]}),
    ));
    e.run_ok(
        "pcb.via_type.add",
        json!({"document":PCB,"from":l[2],"to":l[1]}),
    );
    assert_eq!(pcb(&e).via(via).unwrap().span.from, l[2]);
    e.run_ok("edit.undo", json!({"document":PCB}));
    assert_eq!(pcb(&e).via(via).unwrap().span.from, l[1]);
}

#[test]
fn the_mask_opens_over_the_diameter_at_the_reached_outer_face() {
    let dir = TempDir::new("via-mask-end");
    let (mut e, l) = board(&dir);
    let via = created(&e.run_ok(
        "pcb.via.add",
        json!({"at":{"x":0,"y":0},"from":l[2],"to":l[0],
        "diameter":{"start":"0.8mm","end":"1.2mm"},"mask":{"top":{"expansion":"0.05mm"}}}),
    ));
    let opening = reda_pcb::openings::resolve(pcb(&e))
        .into_iter()
        .find(|o| o.objects.contains(&via))
        .unwrap();
    assert_eq!(opening.side, reda_model::Side::Top);
    assert_eq!(opening.copper.outline.width.0, 1200000);
    assert_eq!(opening.geometry.unwrap().outline.width.0, 1300000);
}

#[test]
fn pasting_a_via_copied_before_a_type_exists_keeps_each_physical_pad() {
    let dir = TempDir::new("via-paste-direction");
    let (mut e, l) = board(&dir);
    let source = created(&e.run_ok(
        "pcb.via.add",
        json!({"at":{"x":0,"y":0},"from":l[0],"to":l[2],
       "diameter":{"start":"1mm","end":"0.8mm"}}),
    ));
    let clip = e.run_ok("pcb.copy", json!({"objects":[source]}));
    e.run_ok(
        "pcb.via_type.add",
        json!({"document":PCB,"from":l[2],"to":l[0]}),
    );
    let paste = e.run_ok(
        "pcb.paste",
        json!({"clipboard":clip,"at":{"x":"4mm","y":0}}),
    );
    let id = paste["result"]["objects"][0]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    for uuid in [source, id] {
        let via = pcb(&e).via(uuid).unwrap();
        assert_eq!(via.span.from, l[2]);
        assert_eq!(via.diameter.start.0, 800000);
        assert_eq!(via.diameter.end.unwrap().0, 1000000);
        assert_eq!(via.diameter.middle.unwrap().0, 1000000);
    }
}
