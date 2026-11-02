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
