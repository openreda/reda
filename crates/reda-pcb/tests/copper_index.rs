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
