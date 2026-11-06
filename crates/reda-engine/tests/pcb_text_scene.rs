use reda_testkit::{TempDir, TestEngine};
use serde_json::json;
fn pcb(engine: &TestEngine) -> &reda_model::PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

#[test]
fn each_placed_text_has_an_independent_scene_identity_under_its_footprint() {
    let dir = TempDir::new("footprint-text-scene");
    let mut engine = reda_testkit::test_engine_with_library(&reda_testkit::starter_library());
    engine.run_ok("project.new", json!({"path":dir.path(),"name":"Demo"}));
    for x in [0, 10] {
        engine.run_ok("schematic.component.place",json!({"document":"00000000-0000-4000-8000-000000000002","part":"84d5651a-bf4b-462e-9480-39e8f454a042","at":{"x":x,"y":0}}));
    }
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash":hash}));
    let ids: Vec<_> = pcb(&engine).footprints().map(|f| f.uuid).collect();
    let scene = engine.document_scene(pcb(&engine).uuid).unwrap();
    for id in ids {
        let entry = pcb(&engine)
            .footprint_copy(pcb(&engine).footprint(id).unwrap().footprint)
            .unwrap()
            .footprint
            .designator_text()
            .unwrap()
            .uuid;
        assert!(scene.items.iter().any(|item| item.object == id
            && item.child
                == Some(reda_engine::render::Child::FootprintText {
                    footprint: id,
                    text: entry
                })));
    }
}
