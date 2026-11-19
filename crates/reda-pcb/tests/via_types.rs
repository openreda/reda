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
fn a_command_keeps_a_types_out_of_limit_sizes_for_drc_and_the_tools_clip_them() {
    let dir = TempDir::new("via-size-clipping");
    let (mut e, l) = board(&dir);
    let diameter = json!({"start":"0.28mm","middle":"0.31mm","end":"0.4mm"});
    e.run_ok(
        "pcb.via_type.add",
        json!({"document":PCB,"from":l[0],"to":l[2],
        "micro":true,"hole":"0.10mm","diameter":diameter}),
    );
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"microvia_size","values":{
        "diameter":{"min":"0.30mm","max":"0.35mm"},"hole":{"min":"0.12mm"}}}),
    );
    let implicit = created(&e.run_ok(
        "pcb.via.add",
        json!({"from":l[0],"to":l[2],"at":{"x":0,"y":0}}),
    ));
    let explicit = created(&e.run_ok(
        "pcb.via.add",
        json!({"from":l[0],"to":l[2],"at":{"x":"2mm","y":0},
        "diameter":diameter,"hole":"0.10mm"}),
    ));
    let sizes = |id| {
        let via = pcb(&e).via(id).unwrap();
        (serde_json::to_value(via.diameter).unwrap(), via.hole.0)
    };
    let as_typed = (json!({"start":280000,"middle":310000,"end":400000}), 100000);
    assert_eq!(sizes(implicit), as_typed);
    assert_eq!(sizes(explicit), as_typed);
    // DRC flags the Vias placed with the out-of-limit type.
    let run = e.run_ok("drc.run", json!({"checks":["microvia_size"]}));
    let flagged: std::collections::BTreeSet<String> = run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|v| v["objects"].as_array().unwrap().clone())
        .map(|o| o.as_str().unwrap().to_owned())
        .collect();
    assert!(
        flagged.contains(&implicit.to_string()) && flagged.contains(&explicit.to_string()),
        "{run:#}"
    );
    // The routing and stitching Tools start clipped to the limits.
    let board = pcb(&e);
    let kind = board
        .stackup()
        .type_for_span(board.via(implicit).unwrap().span)
        .unwrap();
    let (diameter, hole) = reda_pcb::rules::via_starting_size(board, None, kind);
    assert_eq!(
        (serde_json::to_value(diameter).unwrap(), hole.0),
        (json!({"start":300000,"middle":310000,"end":350000}), 120000)
    );
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

#[test]
fn rank_and_restore_route_choices_by_excess_single_and_table_order_on_either_direction() {
    use reda_pcb::route::ViaCandidates;
    let dir = TempDir::new("ranked-route-via-types");
    let (mut e, l) = board(&dir);
    let through = pcb(&e).stackup().through_type().uuid;
    let mut add = |from: usize, to: usize| {
        created(&e.run_ok(
            "pcb.via_type.add",
            json!({"document":PCB,"from":l[from],"to":l[to]}),
        ))
    };
    let left = add(2, 1);
    let right = add(2, 4);
    let exact = add(4, 1);
    let over = add(0, 4);
    let outer_left = add(0, 2);
    let alt_left = add(1, 3);
    let alt_right = add(4, 3);
    let expected = [
        vec![exact],
        vec![left, right],
        vec![alt_left, alt_right],
        vec![over],
        vec![outer_left, right],
        vec![through],
    ];
    for (from, to) in [(1, 4), (4, 1)] {
        let choices = ViaCandidates::between(pcb(&e), None, l[from], l[to]);
        assert_eq!(choices.len(), expected.len());
        for (i, ids) in expected.iter().enumerate() {
            assert_eq!(choices.get(i).as_ref(), Some(ids));
            assert_eq!(choices.index_of(ids), Some(i));
        }
        assert_eq!(choices.get(expected.len()), None);
        assert_eq!(choices.index_of(&[left, over]), None);
    }
    let net = created(&e.run_ok("test.net.add", json!({"name":"Restricted"})));
    let class = created(&e.run_ok("pcb.net_class.add", json!({"name":"HDI","nets":[net]})));
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"via_span","scope":{"net_class":class},"values":{"allowed":[left,right]}}),
    );
    let allowed = ViaCandidates::between(pcb(&e), Some(net), l[1], l[4]);
    assert_eq!(allowed.len(), 1);
    assert_eq!(allowed.get(0), Some(vec![left, right]));
    assert!(ViaCandidates::between(pcb(&e), Some(net), l[0], l[5]).is_empty());
}

#[test]
fn routed_via_preview_uses_each_physical_lands_diameter_within_its_span() {
    use reda_model::geom::{Nm, Point};
    use reda_model::{LayerSpan, ViaDiameter};
    use reda_pcb::{Copper, Proposal};
    let dir = TempDir::new("preview-via-land");
    let (mut e, l) = board(&dir);
    for layer in l {
        e.run_ok("pcb.track.add",json!({
            "layer":layer,"width":"0.1mm","points":[{"x":"0.6mm","y":"-1mm"},{"x":"0.6mm","y":"1mm"}]
        }));
    }
    let diameter = ViaDiameter {
        start: Nm(300000),
        middle: Some(Nm(1200000)),
        end: Some(Nm(400000)),
    };
    let pcb = pcb(&e);
    let proposal = Proposal::Via {
        omitted: Vec::new(),
        span: LayerSpan {
            from: l[4],
            to: l[1],
        },
        at: Point::default(),
        diameter,
        hole: Nm::from_um(100),
        net: None,
    };
    let mut layers: Vec<_> = Copper::build(pcb)
        .clearance(pcb, &proposal, &[])
        .into_iter()
        .map(|h| h.layer)
        .collect();
    layers.sort();
    let mut expected = [l[2], l[3]];
    expected.sort();
    assert_eq!(layers, expected);
}
