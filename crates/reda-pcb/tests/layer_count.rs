//! Layer edits, counted previews and PCB history through the shared Command path.
use reda_model::{PcbDocument, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn board(dir: &TempDir, count: usize) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":count}),
    );
    engine
}

fn pcb(e: &TestEngine) -> &PcbDocument {
    e.project().unwrap().pcb().unwrap()
}

fn copper(e: &TestEngine) -> Vec<Uuid> {
    pcb(e).copper_layers().map(|l| l.uuid).collect()
}

fn preview(e: &mut TestEngine, id: &str, params: Value) -> Value {
    e.run_ok(
        "pcb.stackup.change_preview",
        json!({"change":{"id":id,"params":params}}),
    )
}

#[test]
fn adding_splits_the_selected_material_and_preserves_the_outer_copper() {
    let dir = TempDir::new("layer-add");
    let mut e = board(&dir, 2);
    let original = copper(&e);
    let doc = pcb(&e).uuid;
    let sheet = pcb(&e).stackup().dielectrics().next().unwrap().uuid;
    e.run_ok(
        "pcb.stackup.set",
        json!({"dielectrics":[{"uuid":sheet,"material":"FR408","thickness":500001}]}),
    );
    let new = Uuid::from_u128(9999);
    let params = json!({"split":sheet,"document":doc,"uuid":new});
    let counts = preview(&mut e, "pcb.layer.add", params.clone());
    assert_eq!(counts["via_types_changed"], 1);
    assert_eq!(copper(&e), original);
    let added = e.run_ok("pcb.layer.add", params);
    assert_eq!(copper(&e), [original[0], new, original[1]]);
    let stack = pcb(&e).stackup();
    let halves: Vec<_> = stack
        .dielectrics()
        .map(|d| (d.material.as_deref(), d.thickness.unwrap().0))
        .collect();
    assert_eq!(halves, [(Some("FR408"), 250000), (Some("FR408"), 250001)]);
    assert_eq!(stack.copper_of(new).unwrap().thickness.0, 35000);
    assert_eq!(stack.dielectrics().next().unwrap().uuid, sheet);
    assert_eq!(
        added["result"]["dielectric"],
        json!(stack.dielectrics().nth(1).unwrap().uuid)
    );
    assert_eq!(pcb(&e).layers().layer(new).unwrap().name, None);
    e.run_ok("edit.undo", json!({"document":doc}));
    assert_eq!(copper(&e), original);
}

fn created(out: Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn saved(e: &mut TestEngine, dir: &TempDir) -> Vec<u8> {
    e.run_ok("project.save", json!({}));
    std::fs::read(dir.path().join("Demo/Demo.redapcb")).unwrap()
}

#[test]
fn removing_an_endpoint_repairs_copper_types_allowed_lists_and_named_rules_in_one_undo() {
    let dir = TempDir::new("layer-remove");
    let mut e = board(&dir, 5);
    let l = copper(&e);
    let doc = pcb(&e).uuid;
    let mut kind =
        |a, b| created(e.run_ok("pcb.via_type.add", json!({"document":doc,"from":a,"to":b})));
    let shortening = kind(l[2], l[0]);
    let survivor = kind(l[0], l[1]);
    let collapsing = kind(l[2], l[3]);
    let mut via = |a, b| {
        created(e.run_ok("pcb.via.add",json!({"at":{"x":0,"y":0},"from":a,"to":b,"diameter":{"start":600000,"middle":650000,"end":700000}})))
    };
    let shortened = via(l[2], l[0]);
    let deleted = via(l[2], l[3]);
    let through = via(l[0], l[4]);
    let class = created(e.run_ok("pcb.net_class.add", json!({"name":"Power"})));
    e.run_ok(
        "pcb.rules.set",
        json!({"scope":{"net_class":class},"kind":"via_span","values":{"allowed":[collapsing]}}),
    );
    let other = created(e.run_ok("pcb.net_class.add", json!({"name":"Clock"})));
    e.run_ok("pcb.rules.set",json!({"scope":{"net_class":other},"kind":"via_span","values":{"allowed":[shortening,survivor]}}));
    e.run_ok(
        "pcb.rules.set",
        json!({"scope":{"layers":[l[2],l[3]]},"kind":"track_width","values":{"min":100000}}),
    );
    e.run_ok(
        "pcb.rules.set",
        json!({"scope":{"layers":[l[2]]},"kind":"clearance","values":{"value":100000}}),
    );
    let line = created(e.run_ok(
        "pcb.graphic.add_line",
        json!({"layer":l[2],"from":{"x":0,"y":0},"to":{"x":1000000,"y":0},"width":200000}),
    ));
    let pts = json!([{"x":0,"y":0},{"x":3000000,"y":0},{"x":3000000,"y":3000000}]);
    e.run_ok(
        "pcb.track.add",
        json!({"layer":l[2],"width":200000,"points":pts}),
    );
    e.run_ok("pcb.zone.add", json!({"layer":l[2],"vertices":pts}));
    e.run_ok(
        "pcb.text.add",
        json!({"layer":l[2],"x":0,"y":0,"text":"Ground"}),
    );
    let bytes = saved(&mut e, &dir);
    let params = json!({"layer":l[2]});
    let counts = preview(&mut e, "pcb.layer.remove", params.clone());
    assert_eq!(
        counts["objects_deleted"],
        json!({"graphic":1,"track":2,"zone":1,"text":1})
    );
    assert_eq!(counts["vias_shortened"], 1);
    assert_eq!(counts["vias_deleted"], 1);
    assert_eq!(counts["via_types_removed"], 2);
    assert_eq!(counts["rules_changed"], 3);
    assert_eq!(counts["rules_removed"], 1);
    assert_eq!(counts["allowed_lists_emptied"], 1);
    assert_eq!(counts["classes_allowing_all"], json!(["Power"]));
    e.run_ok("pcb.layer.remove", params);
    let p = pcb(&e);
    assert!(p.graphic(line).is_none());
    assert!(p.via(deleted).is_none());
    let v = p.via(shortened).unwrap();
    assert_eq!((v.span.from, v.span.to), (l[0], l[1]));
    assert_eq!(v.diameter.start.0, 700000);
    assert_eq!(v.diameter.end.unwrap().0, 600000);
    assert_eq!(p.via(through).unwrap().span.to, l[4]);
    assert_eq!(p.stackup().via_types.len(), 2);
    assert_eq!(
        p.class_rule(reda_model::RuleKind::ViaSpan, other)
            .unwrap()
            .values
            .allowed,
        Some(vec![survivor])
    );
    assert_eq!(
        p.class_rule(reda_model::RuleKind::ViaSpan, class)
            .unwrap()
            .values
            .allowed,
        Some(vec![])
    );
    assert_eq!(copper(&e), [l[0], l[1], l[3], l[4]]);
    e.run_ok("edit.undo", json!({"document":doc}));
    assert_eq!(saved(&mut e, &dir), bytes);
}

#[test]
fn moving_inner_copper_keeps_its_identity_thickness_and_endpoints_and_counts_only_crossed_spans() {
    let dir = TempDir::new("layer-move");
    let mut e = board(&dir, 5);
    let l = copper(&e);
    let doc = pcb(&e).uuid;
    e.run_ok(
        "pcb.stackup.set",
        json!({"copper":[{"layer":l[1],"thickness":60000}]}),
    );
    let mut via =
        |a, b| created(e.run_ok("pcb.via.add", json!({"from":a,"to":b,"at":{"x":0,"y":0}})));
    let moved = via(l[0], l[1]);
    via(l[0], l[4]);
    via(l[1], l[2]);
    let sheets = pcb(&e).stackup().dielectrics().cloned().collect::<Vec<_>>();
    let params = json!({"layer":l[1],"direction":"down"});
    let counts = preview(&mut e, "pcb.layer.move", params.clone());
    assert_eq!(counts["vias_using_span"], 1);
    e.run_ok("pcb.layer.move", params);
    assert_eq!(copper(&e), [l[0], l[2], l[1], l[3], l[4]]);
    assert_eq!(
        pcb(&e).stackup().copper_of(l[1]).unwrap().thickness.0,
        60000
    );
    assert_eq!(
        pcb(&e).stackup().dielectrics().cloned().collect::<Vec<_>>(),
        sheets
    );
    let span = pcb(&e).via(moved).unwrap().span;
    assert_eq!((span.from, span.to), (l[0], l[1]));
    assert_eq!(pcb(&e).span_layers(span), [l[0], l[2], l[1]]);
    e.run_ok("edit.undo", json!({"document":doc}));
    assert_eq!(copper(&e), l);
}

#[test]
fn neither_a_thirty_third_layer_nor_a_changed_outer_layer_enters_history() {
    use reda_command::ErrorCode;
    let dir = TempDir::new("layer-limit");
    let mut e = board(&dir, 32);
    let doc = pcb(&e).uuid;
    let original = copper(&e);
    for (command, params) in [
        ("pcb.layer.add", json!({"document":doc})),
        ("pcb.layer.remove", json!({"layer":original[0]})),
        ("pcb.layer.remove", json!({"layer":original[31]})),
        (
            "pcb.layer.move",
            json!({"layer":original[1],"direction":"up"}),
        ),
    ] {
        assert_eq!(e.run_err(command, params.clone()).code, ErrorCode::Rejected);
        assert_eq!(
            e.run_err(
                "pcb.stackup.change_preview",
                json!({"change":{"id":command,"params":params}})
            )
            .code,
            ErrorCode::Rejected
        );
    }
    assert_eq!(copper(&e), original);
}

#[test]
fn stretched_micro_vias_keep_their_flag_and_drilling_ends() {
    let dir = TempDir::new("layer-micro");
    let mut e = board(&dir, 4);
    let l = copper(&e);
    let doc = pcb(&e).uuid;
    let kind = created(e.run_ok(
        "pcb.via_type.add",
        json!({"document":doc,"from":l[2],"to":l[0],"micro":true}),
    ));
    let via = created(e.run_ok(
        "pcb.via.add",
        json!({"from":l[0],"to":l[2],"at":{"x":0,"y":0}}),
    ));
    let first = pcb(&e).stackup().dielectrics().next().unwrap().uuid;
    let counts = preview(
        &mut e,
        "pcb.layer.add",
        json!({"document":doc,"split":first}),
    );
    assert_eq!(counts["vias_stretched"], 1);
    assert_eq!(counts["via_types_changed"], 2);
    e.run_ok("pcb.layer.add", json!({"document":doc,"split":first}));
    let p = pcb(&e);
    let span = p.via(via).unwrap().span;
    assert_eq!((span.from, span.to), (l[2], l[0]));
    assert_eq!(p.span_layers(span).len(), 4);
    let t = p.stackup().type_for_span(span).unwrap();
    assert!(t.micro);
    assert_eq!(t.uuid, kind);
    // A route from either direction cannot use the stretched type, including
    // as the first or last member of an otherwise permitted stack.
    let next = created(e.run_ok(
        "pcb.via_type.add",
        json!({"document":doc,"from":l[2],"to":l[3]}),
    ));
    let net = created(e.run_ok("test.net.add", json!({"name":"Restricted"})));
    let class = created(e.run_ok("pcb.net_class.add", json!({"name":"Laser","nets":[net]})));
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"via_span","scope":{"net_class":class},"values":{"allowed":[kind,next]}}),
    );
    for (from, to) in [(l[0], l[2]), (l[0], l[3]), (l[3], l[0])] {
        assert!(reda_pcb::route::ViaCandidates::between(pcb(&e), Some(net), from, to).is_empty());
    }
}

use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {cases:16,..Default::default()})]
    #[test]
    fn random_layer_edits_preserve_identities_and_undo_to_the_original_file(
        steps in prop::collection::vec((0u8..4,any::<u8>()),6..24)
    ){
        let dir=TempDir::new("random-layers");
        let mut e=board(&dir,6);
        let mut expected=copper(&e);
        let doc=pcb(&e).uuid;
        for (a,b) in [(0,2),(2,3),(5,2)] {
            e.run_ok("pcb.via_type.add",json!({"document":doc,"from":expected[a],"to":expected[b]}));
            e.run_ok("pcb.via.add",json!({"at":{"x":0,"y":0},"from":expected[a],"to":expected[b]}));
        }
        e.run_ok("pcb.rules.set",json!({"kind":"track_width","scope":{"layers":[expected[2],expected[3]]},"values":{"min":100000}}));
        let original=saved(&mut e,&dir);
        let mut edits=0;
        for (action,pos) in steps {
            let len=expected.len();
            let pos=usize::from(pos);
            match action {
                1 if len>2=>{
                    let id=expected.remove(1+pos%(len-2));
                    e.run_ok("pcb.layer.remove",json!({"layer":id}));
                }
                2 if len>3=>{
                    let i=1+pos%(len-3);
                    e.run_ok("pcb.layer.move",json!({"layer":expected[i],"direction":"down"}));
                    expected.swap(i,i+1);
                }
                3=>{
                    let target=[2,4,6,8][pos%4];
                    let mut params=apply_params(&e,&format!("{target} Layer"));
                    let mut remove=Vec::new();
                    while expected.len()>target {
                        remove.push(expected.remove(1+pos%(expected.len()-2)));
                    }
                    params["remove"]=json!(remove);
                    let ids=e.run_ok(APPLY,params)["result"]["layers"].clone();
                    if target>len {
                        let middle=len.div_ceil(2);
                        let new:Vec<Uuid>=serde_json::from_value(ids).unwrap();
                        for (i,id) in new[middle..middle+target-len].iter().enumerate() {
                            prop_assert!(!expected.contains(id));
                            expected.insert(middle+i,*id);
                        }
                    }
                }
                _=>{
                    let gap=pos%(len-1);
                    let sheet=pcb(&e).stackup().dielectrics().nth(gap).unwrap().uuid;
                    let id=created(e.run_ok("pcb.layer.add",json!({"document":doc,"split":sheet})));
                    expected.insert(gap+1,id);
                }
            }
            edits+=1;
            prop_assert_eq!(&copper(&e),&expected);
            let p=pcb(&e);
            for span in p.vias().map(|v|v.span).chain(p.stackup().via_types.iter().map(|v|v.span())){
                prop_assert!(expected.contains(&span.from) && expected.contains(&span.to));
                prop_assert_ne!(span.from,span.to);
            }
            for rule in p.rules() {
                if let Some(reda_model::RuleLayers::Named(ids))=&rule.scope.layers {
                    prop_assert!(ids.iter().all(|id| expected.contains(id)));
                }
            }
        }
        for _ in 0..edits {e.run_ok("edit.undo",json!({"document":doc}));}
        prop_assert_eq!(saved(&mut e,&dir),original);
    }
}

#[test]
fn removal_joins_the_two_neighbouring_sheets_at_their_physical_thickness() {
    let dir = TempDir::new("layer-join");
    let mut e = board(&dir, 4);
    let l = copper(&e);
    let d = pcb(&e)
        .stackup()
        .dielectrics()
        .map(|d| d.uuid)
        .collect::<Vec<_>>();
    e.run_ok("pcb.stackup.set",json!({"dielectrics":[{"uuid":d[0],"thickness":500000,"material":"Glass"},{"uuid":d[1],"thickness":null},{"uuid":d[2],"thickness":null}]}));
    let doc = pcb(&e).uuid;
    e.run_ok("pcb.layer.remove", json!({"layer":l[1]}));
    let stack = pcb(&e).stackup();
    let merged = stack.dielectrics().next().unwrap();
    assert_eq!(merged.uuid, d[0]);
    assert_eq!(merged.material.as_deref(), Some("Glass"));
    assert_eq!(stack.info().entries[1].thickness, 980000.);
    assert_eq!(stack.info().entries[3].thickness, 480000.);
    assert_eq!(stack.totals().total.0, 1565000);
    e.run_ok("edit.undo", json!({"document":doc}));
    assert!(
        pcb(&e)
            .stackup()
            .dielectrics()
            .nth(1)
            .unwrap()
            .thickness
            .is_none()
    );
}

#[test]
fn previewing_an_unassigned_layer_does_not_reserve_or_reuse_an_existing_id() {
    let dir = TempDir::new("layer-preview-id");
    let mut e = board(&dir, 2);
    let doc = pcb(&e).uuid;
    e.run_ok(
        "pcb.layer.add",
        json!({"document":doc,"uuid":doc.child("preview-copper")}),
    );
    let counts = preview(&mut e, "pcb.layer.add", json!({"document":doc}));
    assert_eq!(counts["via_types_changed"], 1);
    assert_eq!(copper(&e).len(), 3);
}

#[test]
fn an_add_without_a_selection_splits_the_middle_dielectric_among_several_sheets() {
    let dir = TempDir::new("layer-middle-sheet");
    let mut e = board(&dir, 2);
    let doc = pcb(&e).uuid;
    let first = pcb(&e).stackup().dielectrics().next().unwrap().uuid;
    let second = created(e.run_ok("pcb.stackup.dielectric.add", json!({"beside":first})));
    let third = created(e.run_ok("pcb.stackup.dielectric.add", json!({"beside":second})));
    e.run_ok(
        "pcb.stackup.set",
        json!({"dielectrics":[{"uuid":first,"thickness":null}]}),
    );
    let added = created(e.run_ok("pcb.layer.add", json!({"document":doc})));
    let entries = &pcb(&e).stackup().entries;
    assert_eq!(entries[2].uuid(), second);
    assert_eq!(entries[3].uuid(), added);
    assert_eq!(entries[5].uuid(), third);
}

#[test]
fn an_added_layer_inherits_the_nearest_inner_copper_across_a_multi_sheet_gap() {
    let dir = TempDir::new("layer-nearest-copper");
    let mut e = board(&dir, 4);
    let l = copper(&e);
    let doc = pcb(&e).uuid;
    let upper = pcb(&e).stackup().dielectrics().nth(1).unwrap().uuid;
    let lower = created(e.run_ok("pcb.stackup.dielectric.add", json!({"beside":upper})));
    e.run_ok(
        "pcb.stackup.set",
        json!({"copper":[{"layer":l[1],"thickness":18000},{"layer":l[2],"thickness":70000}],
               "dielectrics":[{"uuid":upper,"thickness":200000}]}),
    );
    let added = created(e.run_ok("pcb.layer.add", json!({"document":doc,"split":lower})));
    assert_eq!(
        pcb(&e).stackup().copper_of(added).unwrap().thickness.0,
        70000
    );
}

const APPLY: &str = "pcb.stackup.apply_construction";

fn apply_params(e: &TestEngine, name: &str) -> Value {
    json!({"document":pcb(e).uuid,"preset":e.project().unwrap().fab_presets().next().unwrap().uuid,"construction":name})
}

#[test]
fn growing_an_odd_board_splits_survivors_around_new_middle_layers_and_counts_each_span_once() {
    let dir = TempDir::new("construction-growth");
    let mut e = board(&dir, 5);
    let old = copper(&e);
    let doc = pcb(&e).uuid;
    e.run_ok(
        "pcb.via_type.add",
        json!({"document":doc,"from":old[2],"to":old[3]}),
    );
    e.run_ok(
        "pcb.via.add",
        json!({"from":old[3],"to":old[2],"at":{"x":0,"y":0}}),
    );
    let before = saved(&mut e, &dir);
    let p = apply_params(&e, "8 Layer");
    let counts = preview(&mut e, APPLY, p.clone());
    assert_eq!(counts["vias_stretched"], 1);
    assert_eq!(counts["via_types_changed"], 2);
    assert_eq!(copper(&e), old);
    e.run_ok(APPLY, p);
    let new = copper(&e);
    assert_eq!(new.len(), 8);
    assert_eq!(new[..3], old[..3]);
    assert_eq!(new[6..], old[3..]);
    assert!(new[3..6].iter().all(|id| !old.contains(id)));
    let pcb = pcb(&e);
    assert!(pcb.stackup().copper().map(|c| c.layer).eq(new));
    e.run_ok("edit.undo", json!({"document":doc}));
    assert_eq!(saved(&mut e, &dir), before);
}

#[test]
fn shrink_requires_exact_distinct_inner_layers_and_counts_each_deleted_or_shortened_object_once() {
    let dir = TempDir::new("construction-shrink");
    let mut e = board(&dir, 6);
    let l = copper(&e);
    let doc = pcb(&e).uuid;
    let (keep, gone) = (l[1], l[4]);
    e.run_ok(
        "pcb.layer.set",
        json!({"layer":keep,"name":"Ground","role":"plane"}),
    );
    for layer in [keep, l[2], gone] {
        e.run_ok(
            "pcb.text.add",
            json!({"layer":layer,"x":0,"y":0,"text":"Copper"}),
        );
    }
    let via = created(e.run_ok(
        "pcb.via.add",
        json!({"from":l[2],"to":l[5],"at":{"x":0,"y":0}}),
    ));
    e.run_ok(
        "pcb.via.add",
        json!({"from":l[2],"to":gone,"at":{"x":1000,"y":0}}),
    );
    let before = saved(&mut e, &dir);
    let mut p = apply_params(&e, "4 Layer");
    for choices in [
        Value::Null,
        json!([l[2]]),
        json!([l[2], l[2]]),
        json!([l[2], l[0]]),
        json!([l[2], l[3], gone]),
    ] {
        if !choices.is_null() {
            p["remove"] = choices
        }
        assert!(e.run(APPLY, p.clone()).is_err());
        assert!(
            e.run(
                "pcb.stackup.change_preview",
                json!({"change":{"id":APPLY,"params":p}})
            )
            .is_err()
        );
    }
    p["remove"] = json!(["copper_l5", "copper_l3"]);
    let counts = preview(&mut e, APPLY, p.clone());
    assert_eq!(counts["objects_deleted"], json!({"text":2}));
    assert_eq!(
        (
            counts["vias_shortened"].as_u64(),
            counts["vias_deleted"].as_u64()
        ),
        (Some(1), Some(1))
    );
    assert_eq!(saved(&mut e, &dir), before);
    e.run_ok(APPLY, p);
    assert_eq!(copper(&e), [l[0], keep, l[3], l[5]]);
    let board = pcb(&e);
    assert_eq!(board.via(via).unwrap().span.from, l[3]);
    assert_eq!(board.texts().count(), 1);
    assert_eq!(board.vias().count(), 1);
    assert_eq!(
        board.layers().layer(keep).unwrap().name.as_deref(),
        Some("Ground")
    );
    e.run_ok("edit.undo", json!({"document":doc}));
    assert_eq!(saved(&mut e, &dir), before);
}
