//! The copper index kept up to date from change sets matches one built from scratch.

use reda_model::{Scope, Uuid};
use reda_pcb::{Copper, Item};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

/// Feeds every edit's change set to an index and checks it against a fresh build.
struct Follower {
    copper: Copper,
}

impl Follower {
    fn new(engine: &TestEngine) -> Follower {
        Follower {
            copper: Copper::build(engine.project().unwrap().pcb().unwrap()),
        }
    }

    fn edit(&mut self, engine: &mut TestEngine, id: &str, params: Value) -> Value {
        let out = engine.run_ok(id, params);
        let pcb = engine.project().unwrap().pcb().unwrap();
        let entry = engine
            .history()
            .next_undo(Scope::Document(pcb.uuid))
            .unwrap();
        self.copper.apply(pcb, &entry.changes);
        self.check(engine);
        out
    }

    fn undo(&mut self, engine: &mut TestEngine) {
        let doc = engine.project().unwrap().pcb().unwrap().uuid;
        engine.run_ok("edit.undo", json!({"document": doc.to_string()}));
        let pcb = engine.project().unwrap().pcb().unwrap();
        let entry = engine.history().next_redo(Scope::Document(doc)).unwrap();
        self.copper.apply(pcb, &entry.changes.inverted());
        self.check(engine);
    }

    fn check(&self, engine: &TestEngine) {
        let fresh = Copper::build(engine.project().unwrap().pcb().unwrap());
        assert_eq!(self.copper, fresh);
        assert_eq!(self.copper.ratsnest(), fresh.ratsnest());
    }
}

#[test]
fn the_index_follows_adds_deletes_and_undo() {
    let dir = TempDir::new("copper-index");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let mut f = Follower::new(&engine);
    let sig = f.edit(&mut engine, "test.net.add", json!({"name": "SIG"}))["result"]["uuid"].clone();
    let fp = f.edit(
        &mut engine,
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [
            {"at": {"x": "-2mm", "y": 0}, "width": "1mm", "height": "1mm", "net": sig},
            {"at": {"x": "2mm", "y": 0}, "width": "1mm", "height": "1mm", "net": sig}]}),
    );
    let fp = uuid(&fp["result"]["footprint"]);
    let pads = fp_pads(&fp, &engine);
    assert_eq!(f.copper.ratsnest().len(), 1);

    let track = f.edit(
        &mut engine,
        "pcb.track.add",
        json!({"layer": "copper_top", "points": [
            {"x": "-2mm", "y": 0}, {"x": "-2mm", "y": "3mm"}, {"x": "2mm", "y": "3mm"},
            {"x": "2mm", "y": 0}]}),
    );
    assert!(f.copper.ratsnest().is_empty());
    let connected = f.copper.connected(pads[0]);
    assert!(connected.contains(&pads[1]), "{connected:?}");

    f.edit(
        &mut engine,
        "pcb.via.add",
        json!({"at": {"x": 0, "y": "3mm"}}),
    );
    let middle = track["result"]["tracks"][1].clone();
    f.edit(&mut engine, "pcb.track.delete", json!({"tracks": [middle]}));
    // Three SIG islands now: each half of the route and the Via left between them.
    assert_eq!(
        f.copper.ratsnest().len(),
        2,
        "the cut route needs joining again"
    );
    assert!(!f.copper.connected(pads[0]).contains(&pads[1]));
    f.undo(&mut engine);
    assert!(f.copper.ratsnest().is_empty());
    f.undo(&mut engine);
    f.undo(&mut engine);
    assert_eq!(f.copper.ratsnest().len(), 1);
}

#[test]
fn moving_turning_and_flipping_a_footprint_reconnects_its_copper() {
    let dir = TempDir::new("copper-move");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let mut f = Follower::new(&engine);
    let sig = f.edit(&mut engine, "test.net.add", json!({"name": "SIG"}))["result"]["uuid"].clone();
    // A Footprint whose one Pad sits 2 mm right of its origin, and a Free Pad with a Track
    // leading from it up to where the Footprint's Pad will land.
    let fp = f.edit(
        &mut engine,
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [
            {"at": {"x": "2mm", "y": 0}, "width": "1mm", "height": "1mm", "net": sig}]}),
    );
    let fp = uuid(&fp["result"]["footprint"]);
    let pad = fp_pads(&fp, &engine)[0];
    let free = f.edit(
        &mut engine,
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": "5mm", "y": 0}, "width": "1mm", "net": sig}),
    );
    let free = Item::FreePad {
        uuid: uuid(&free["result"]["uuid"]),
    };
    assert_eq!(f.copper.net(free), Some(uuid(&sig)));
    f.edit(
        &mut engine,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": sig, "points": [
            {"x": "5mm", "y": 0}, {"x": "5mm", "y": "3mm"}]}),
    );
    assert_eq!(f.copper.ratsnest().len(), 1, "the Pad is not reached yet");
    assert!(!f.copper.connected(pad).contains(&free));

    // Moving the Footprint puts its Pad on the Track's end.
    f.edit(
        &mut engine,
        "pcb.move",
        json!({"objects": [fp], "by": {"x": "3mm", "y": "3mm"}}),
    );
    assert!(f.copper.ratsnest().is_empty());
    assert!(f.copper.connected(pad).contains(&free));

    // Turning it half way round in place swings the Pad off the Track, and back on.
    f.edit(
        &mut engine,
        "pcb.rotate",
        json!({"objects": [fp], "angle": "180deg"}),
    );
    assert_eq!(f.copper.ratsnest().len(), 1);
    assert!(!f.copper.connected(pad).contains(&free));
    f.undo(&mut engine);
    assert!(f.copper.ratsnest().is_empty());

    // Flipping it takes its SMD Pad to the bottom copper, away from the top Track.
    f.edit(&mut engine, "pcb.flip", json!({"objects": [fp]}));
    assert_eq!(f.copper.ratsnest().len(), 1);
    assert!(!f.copper.connected(pad).contains(&free));
    f.undo(&mut engine);
    assert!(f.copper.connected(pad).contains(&free));

    // Moving the Free Pad off the Track's end breaks the route there.
    f.edit(
        &mut engine,
        "pcb.move",
        json!({"objects": [free.object()], "by": {"x": "-3mm", "y": 0}}),
    );
    assert_eq!(f.copper.ratsnest().len(), 1);
    f.undo(&mut engine);
    f.undo(&mut engine);
    assert_eq!(
        f.copper.ratsnest().len(),
        1,
        "undoing the move parts them again"
    );
}

fn fp_pads(fp: &Uuid, engine: &TestEngine) -> Vec<Item> {
    let pcb = engine.project().unwrap().pcb().unwrap();
    let instance = pcb.footprint(*fp).unwrap();
    let copy = pcb.footprint_copy(instance.footprint).unwrap();
    let mut pads: Vec<_> = copy.footprint.pads.iter().collect();
    pads.sort_by_key(|p| p.number.clone());
    pads.iter()
        .map(|p| Item::Pad {
            footprint: *fp,
            pad: p.uuid,
        })
        .collect()
}

#[test]
fn the_projects_index_is_built_once_and_rebuilt_after_an_edit_or_undo() {
    let dir = TempDir::new("copper-cached");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let first = Copper::of(engine.project().unwrap()).unwrap();
    let again = Copper::of(engine.project().unwrap()).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &again), "built once");

    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.2mm",
               "points": [{"x": 0, "y": 0}, {"x": 1_000_000, "y": 0}]}),
    );
    let after = Copper::of(engine.project().unwrap()).unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&first, &after),
        "rebuilt after the edit"
    );
    assert_eq!(
        *after,
        Copper::build(engine.project().unwrap().pcb().unwrap())
    );

    let doc = engine.project().unwrap().pcb().unwrap().uuid;
    engine.run_ok("edit.undo", json!({"document": doc.to_string()}));
    let undone = Copper::of(engine.project().unwrap()).unwrap();
    assert_eq!(*undone, *first);
}

#[test]
fn a_pads_copper_follows_its_side_offset_and_cut_corners() {
    use reda_model::geom::{Nm, Point};
    let dir = TempDir::new("copper-pad-model");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    // A 2 × 1 mm SMD Pad on the Footprint's bottom side, anchored at the origin with its
    // copper 1 mm to the right and its top-right corner chamfered by 0.5 mm (100%).
    let fp = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "2mm", "height": "1mm", "side": "bottom",
             "shape": "chamfered_rect", "chamfer_ratio": 100_000, "corners": ["top_right"],
             "offset": {"x": "1mm", "y": 0}}]}),
    );
    let fp = uuid(&fp["result"]["footprint"]);
    let pad = fp_pads(&fp, &engine)[0];
    let pcb = engine.project().unwrap().pcb().unwrap();
    let copper = Copper::build(pcb);
    let layers: Vec<Uuid> = pcb.copper_layers().map(|l| l.uuid).collect();
    let (top, bottom) = (layers[0], *layers.last().unwrap());
    let at = |x: i64, y: i64| Point::new(Nm::from_um(x), Nm::from_um(y));

    assert!(
        copper.items_at(top, at(1000, 0)).is_empty(),
        "no copper on the top"
    );
    assert_eq!(copper.items_at(bottom, at(1000, 0)), [pad]);
    assert_eq!(copper.items_at(bottom, at(100, 0)), [pad]);
    assert!(
        copper.items_at(bottom, at(-100, 0)).is_empty(),
        "left of the copper"
    );
    assert!(
        copper.items_at(bottom, at(1900, 400)).is_empty(),
        "the chamfer"
    );
    assert_eq!(copper.items_at(bottom, at(1900, -400)), [pad]);
    assert_eq!(copper.items_at(bottom, at(100, 400)), [pad]);

    // Flipped, the Footprint takes the Pad to the top, mirrored: copper left of the
    // anchor and the chamfer on the top-left.
    engine.run_ok("pcb.flip", json!({"objects": [fp]}));
    let pcb = engine.project().unwrap().pcb().unwrap();
    let copper = Copper::build(pcb);
    let x = pcb.footprint(fp).unwrap().at.x.0 / 1000;
    assert_eq!(copper.items_at(top, at(x - 1000, 0)), [pad]);
    assert!(copper.items_at(top, at(x - 1900, 400)).is_empty());
    assert_eq!(copper.items_at(top, at(x + 1900 - 2000, 400)), [pad]);
    assert!(copper.items_at(bottom, at(x - 1000, 0)).is_empty());
}

#[test]
fn clearance_halos_grow_other_net_copper_near_the_cursor() {
    use reda_model::geom::{Nm, Point};
    use reda_model::{LayerKind, Side};
    let dir = TempDir::new("halos");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let sig = uuid(&engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    let gnd = uuid(&engine.run_ok("test.net.add", json!({"name": "GND"}))["result"]["uuid"]);
    let track = |engine: &mut TestEngine, net: Uuid, y: &str| {
        uuid(
            &engine.run_ok(
                "pcb.track.add",
                json!({"layer": "copper_top", "net": net, "width": "0.2mm",
                       "points": [{"x": 0, "y": y}, {"x": "1mm", "y": y}]}),
            )["result"]["tracks"][0],
        )
    };
    let own = track(&mut engine, sig, "0mm");
    let near = track(&mut engine, gnd, "2mm");
    let far = track(&mut engine, gnd, "20mm");
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let top = pcb
        .layers()
        .find(LayerKind::Copper, Some(Side::Top))
        .unwrap()
        .uuid;
    let copper = Copper::of(project).unwrap();
    let origin = Point::new(Nm::ZERO, Nm::ZERO);

    let all = copper.halos(pcb, top, Some(sig), None);
    let items: Vec<Uuid> = all.iter().map(|h| h.item.object()).collect();
    assert!(!items.contains(&own), "the route's own Net has no halo");
    assert_eq!(items.len(), 2);
    assert!(items.contains(&near) && items.contains(&far));
    let halo = &all[0];
    assert!(halo.clearance > Nm::ZERO);
    assert_eq!(
        halo.shape.prims[0].radius,
        Nm::from_um(100).0 + halo.clearance.0,
        "half the width plus the clearance"
    );

    let close = copper.halos(pcb, top, Some(sig), Some((origin, Nm::from_mm(5))));
    let items: Vec<Uuid> = close.iter().map(|h| h.item.object()).collect();
    assert_eq!(items, [near], "only copper within 5 mm");
}

#[test]
fn only_the_etched_part_of_a_free_drawing_is_netless_copper() {
    use reda_model::geom::{Nm, Point};

    let dir = TempDir::new("index-free-drawing");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name":"Drawing","layers":4}),
    );
    let inner = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .nth(1)
        .unwrap()
        .uuid;
    let mut f = Follower::new(&engine);
    let rect = f.edit(
        &mut engine,
        "pcb.graphic.add_rect",
        json!({
            "layer":inner,"from":{"x":"2mm","y":"2mm"},"to":{"x":"6mm","y":"5mm"},"width":"0.2mm"
        }),
    );
    let rect = uuid(&rect["result"]["uuid"]);
    let p = |x, y| Point::new(Nm::from_mm(x), Nm::from_mm(y));
    let edge = f.copper.items_at(inner, p(4, 5));
    assert_eq!(edge.iter().map(Item::object).collect::<Vec<_>>(), [rect]);
    assert_eq!(f.copper.net(edge[0]), None);
    assert!(f.copper.items_at(inner, p(4, 3)).is_empty());

    f.edit(&mut engine, "pcb.graphic.delete", json!({"uuid":rect}));
    assert!(f.copper.items_at(inner, p(4, 5)).is_empty());
    f.undo(&mut engine);

    // An outline O has a hole which is free of copper. A single stroke spans the other glyph.
    for (font, text, x) in [("Noto Sans", "O", 10), ("Hershey Roman Simplex", "I", 20)] {
        let out = f.edit(
            &mut engine,
            "pcb.text.add",
            json!({
                "layer":inner,"font":font,"text":text,"height":"10mm","x":Nm::from_mm(x),"y":0
            }),
        );
        let id = uuid(&out["result"]["uuid"]);
        let midline: Vec<_> = (0..120)
            .map(|i| {
                f.copper
                    .items_at(
                        inner,
                        Point::new(Nm(x * 1_000_000 + i * 100_000), Nm::from_mm(5)),
                    )
                    .iter()
                    .any(|i| i.object() == id)
            })
            .collect();
        let strokes = midline.windows(2).filter(|w| !w[0] && w[1]).count();
        assert_eq!(strokes, if text == "O" { 2 } else { 1 }, "{font} {text}");
        f.edit(
            &mut engine,
            "pcb.text.set",
            json!({"uuid":id,"layer":"silkscreen_top"}),
        );
        assert!(f.copper.items().all(|i| i.object() != id));
        f.undo(&mut engine);
    }
}

#[test]
fn unused_lands_follow_track_ends_and_zone_outlines_before_filling() {
    use reda_model::geom::{Nm, Point};
    let dir = TempDir::new("copper-unused-lands");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4,"board":{"width":"20mm","height":"20mm"}}),
    );
    let layers: Vec<_> = e
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    let mut f = Follower::new(&e);
    let net = f.edit(&mut e, "test.net.add", json!({"name":"GND"}))["result"]["uuid"].clone();
    let at = json!({"x":"5mm","y":"5mm"});
    let via = uuid(
        &f.edit(
            &mut e,
            "pcb.via.add",
            json!({"at":at,"net":net,"unused_layers":"remove"}),
        )["result"]["uuid"],
    );
    let item = Item::Via { uuid: via };
    let on = |f: &Follower| -> Vec<Uuid> {
        layers
            .iter()
            .copied()
            .filter(|l| {
                f.copper
                    .items_at(*l, Point::new(Nm(5_000_000), Nm(5_000_000)))
                    .contains(&item)
            })
            .collect()
    };
    assert!(on(&f).is_empty());
    f.edit(
        &mut e,
        "pcb.track.add",
        json!({"layer":layers[1],"net":net,"points":[{"x":"3mm","y":"5mm"},{"x":"7mm","y":"5mm"}]}),
    );
    assert!(on(&f).is_empty(), "passing over a hole leaves the land out");
    f.edit(&mut e,"pcb.track.add",json!({"layer":layers[1],"net":net,"points":[{"x":"5.1mm","y":"5mm"},{"x":"6mm","y":"8mm"}]}));
    assert_eq!(
        on(&f),
        [layers[1]],
        "a Track end lands within the Via's shape"
    );
    f.undo(&mut e);
    assert!(on(&f).is_empty());
    let zone = f.edit(
        &mut e,
        "pcb.zone.add_from_board_outline",
        json!({"layers":[layers[2]],"net":net}),
    )["result"]["zones"][0]
        .clone();
    assert_eq!(on(&f), [layers[2]]);
    f.edit(
        &mut e,
        "pcb.via.set",
        json!({"vias":[via],"zone_connection":{"style":"none"}}),
    );
    assert!(on(&f).is_empty());
    f.edit(&mut e,"pcb.via.set",json!({"vias":[via],"unused_layers":"remove_keep_ends","zone_connection":{"style":"solid"}}));
    assert_eq!(on(&f), [layers[0], layers[2], layers[3]]);
    f.edit(&mut e, "pcb.zone.delete", json!({"zones":[zone]}));
    assert_eq!(on(&f), [layers[0], layers[3]]);
}

#[test]
fn rounded_triangles_in_mirrored_rotated_letters_do_not_extend_their_copper() {
    use reda_model::geom::{Nm, Point};
    let dir = TempDir::new("copper-rotated-text");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Small lettering"}),
    );
    let id = uuid(
        &engine.run_ok(
            "pcb.text.add",
            json!({
                "layer":"copper_bottom", "x":0, "y":0, "height":890, "rotation":"37deg",
                "text":"O    O", "font":"Noto Sans"
            }),
        )["result"]["uuid"],
    );
    let project = engine.project().unwrap();
    let layer = project.pcb().unwrap().text(id).unwrap().layer;
    let copper = Copper::of(project).unwrap();
    assert_eq!(
        copper.items_at(layer, Point::new(Nm(-702), Nm(-386))),
        [Item::Text { uuid: id }]
    );
    // Placement rounds one narrow triangle onto a line. This point on that line
    // is over 300 nm beyond the nearest drawn copper.
    assert!(
        copper
            .items_at(layer, Point::new(Nm(-1323), Nm(511)))
            .is_empty()
    );
}

#[test]
fn routing_clearance_and_halos_observe_a_plated_bore_without_a_land() {
    use reda_model::geom::{Nm, Point};
    let dir = TempDir::new("live-plated-clearance");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Bores","layers":4}),
    );
    let layers: Vec<_> = e
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    let document = e.project().unwrap().pcb().unwrap().uuid;
    e.run_ok(
        "pcb.via_type.add",
        json!({"document":document,"from":layers[1],"to":layers[2]}),
    );
    let via = uuid(
        &e.run_ok(
            "pcb.via.add",
            json!({"at":{"x":0,"y":0},"from":layers[1],"to":layers[2],
        "diameter":"0.4mm","hole":"0.2mm","unused_layers":"remove"}),
        )["result"]["uuid"],
    );
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","values":{"value":"0.01mm","hole":{"via":"0.35mm"}}}),
    );
    let project = e.project().unwrap();
    let pcb = project.pcb().unwrap();
    let copper = Copper::of(project).unwrap();
    let origin = Point::default();
    for (i, layer) in layers.iter().enumerate() {
        let shape = reda_pcb::Proposal::Track {
            layer: *layer,
            net: None,
            width: Nm::from_um(100),
            geometry: reda_model::TrackGeometry::Line {
                from: Point::new(Nm::from_um(450), Nm::ZERO),
                to: Point::new(Nm::from_um(450), Nm::from_mm(1)),
            },
        };
        let hits = copper.clearance(pcb, &shape, &[]);
        let halos = copper.halos(pcb, *layer, None, Some((origin, Nm::from_mm(2))));
        if i == 1 || i == 2 {
            assert_eq!(
                hits.iter()
                    .map(|h| (h.item.object(), h.required, h.actual))
                    .collect::<Vec<_>>(),
                [(via, Nm::from_um(350), Nm::from_um(300))]
            );
            assert!(
                halos
                    .iter()
                    .any(|h| h.item.object() == via && h.shape.prims[0].radius == 450_000)
            );
            assert!(copper.clearance(pcb, &shape, &[via]).is_empty());
        } else {
            assert!(hits.is_empty() && halos.is_empty());
        }
    }
}

#[test]
fn a_proposed_via_checks_its_own_drilled_diameter_against_stored_copper() {
    use reda_model::{
        LayerSpan,
        geom::{Nm, Point},
    };
    let dir = TempDir::new("planned-bore");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Bores","layers":4}),
    );
    let layers: Vec<_> = e
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","values":{"value":"0.01mm","hole":{"via":"0.35mm"}}}),
    );
    for l in &layers {
        e.run_ok("pcb.track.add",json!({"layer":l,"width":"0.1mm","points":[{"x":"0.45mm","y":0},{"x":"0.45mm","y":"1mm"}]}));
    }
    let pcb = e.project().unwrap().pcb().unwrap();
    let copper = Copper::build(pcb);
    let check = |hole| {
        copper.clearance(
            pcb,
            &reda_pcb::Proposal::Via {
                omitted: Vec::new(),
                span: LayerSpan {
                    from: layers[1],
                    to: layers[2],
                },
                at: Point::default(),
                net: None,
                diameter: Nm::from_um(400).into(),
                hole: Nm::from_um(hole),
            },
            &[],
        )
    };
    let hits = check(200);
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| layers[1..3].contains(&h.layer)
        && h.actual == Nm::from_um(300)
        && h.required == Nm::from_um(350)));
    assert!(check(80).is_empty());
    let halos = copper.via_halos(
        pcb,
        layers[1],
        None,
        None,
        Some(Nm::from_um(400)),
        Nm::from_um(200),
    );
    assert_eq!(halos.len(), 1);
    assert_eq!(halos[0].clearance, Nm::from_um(250));
}

#[test]
fn a_track_from_a_removed_land_takes_its_owners_net_and_brings_the_land_back() {
    use reda_model::geom::{Nm, Point};
    let dir = TempDir::new("copper-route-from-removed-land");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4,"board":{"width":"20mm","height":"20mm"}}),
    );
    let inner = e
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .nth(1)
        .unwrap()
        .uuid;
    let net = uuid(&e.run_ok("test.net.add", json!({"name":"GND"}))["result"]["uuid"]);
    let pad = uuid(
        &e.run_ok(
            "pcb.free_pad.add",
            json!({"kind":"plated_through_hole","width":"2mm","hole":{"width":"0.8mm"},
            "at":{"x":"5mm","y":"5mm"},"unused_layers":"remove","net":net}),
        )["result"]["uuid"],
    );
    let item = Item::FreePad { uuid: pad };
    let centre = Point::new(Nm::from_mm(5), Nm::from_mm(5));
    let copper = Copper::of(e.project().unwrap()).unwrap();
    assert!(copper.unused_lands().contains(item, inner));
    assert_eq!(
        copper.removed_lands_at(e.project().unwrap().pcb().unwrap(), inner, centre),
        [item]
    );
    let track = &e.run_ok(
        "pcb.track.add",
        json!({"layer":inner,"points":[{"x":"5mm","y":"5mm"},{"x":"9mm","y":"5mm"}]}),
    )["result"]["tracks"][0];
    let project = e.project().unwrap();
    let pcb = project.pcb().unwrap();
    assert_eq!(pcb.track(uuid(track)).unwrap().net, Some(net));
    let copper = Copper::of(project).unwrap();
    assert!(!copper.unused_lands().contains(item, inner));
    assert!(copper.items_at(inner, centre).contains(&item));
}

#[test]
fn stacked_micro_vias_keep_the_lands_they_join_on() {
    use reda_model::geom::{Nm, Point};
    let dir = TempDir::new("copper-stacked-micro-vias");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4,"board":{"width":"20mm","height":"20mm"}}),
    );
    let pcb = e.project().unwrap().pcb().unwrap();
    let document = pcb.uuid;
    let l: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    let net = uuid(&e.run_ok("test.net.add", json!({"name":"HDI"}))["result"]["uuid"]);
    let mut f = Follower::new(&e);
    let mut vias = Vec::new();
    for (from, to) in [(l[0], l[1]), (l[1], l[2])] {
        f.edit(
            &mut e,
            "pcb.via_type.add",
            json!({"document":document,"from":from,"to":to,"micro":true,
            "hole":"0.1mm","diameter":"0.3mm"}),
        );
        vias.push(uuid(
            &f.edit(
                &mut e,
                "pcb.via.add",
                json!({"at":{"x":"5mm","y":"5mm"},"from":from,"to":to,"net":net,
                "unused_layers":"remove"}),
            )["result"]["uuid"],
        ));
    }
    for (layer, x) in [(l[0], "8mm"), (l[2], "2mm")] {
        f.edit(
            &mut e,
            "pcb.track.add",
            json!({"layer":layer,"net":net,"points":[{"x":"5mm","y":"5mm"},{"x":x,"y":"5mm"}]}),
        );
    }
    let at = Point::new(Nm::from_mm(5), Nm::from_mm(5));
    for via in &vias {
        assert!(
            f.copper
                .items_at(l[1], at)
                .contains(&Item::Via { uuid: *via }),
            "a Via stacked end to end on another keeps its land where they join"
        );
    }
    f.edit(&mut e, "pcb.via.delete", json!({"vias":[vias[1]]}));
    assert!(
        f.copper
            .unused_lands()
            .contains(Item::Via { uuid: vias[0] }, l[1])
    );
}
