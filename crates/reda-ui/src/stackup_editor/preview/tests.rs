//! The Scene handed to the cross-section painter, in the board's own nanometres.
use std::path::Path;

use super::*;
use crate::theme::{Settings, Tokens};
use reda_engine::model::geom::Nm;
use reda_engine::render::Primitive;
use reda_testkit::{assert_golden, open_reference_copy};
use serde_json::json;

#[test]
fn cross_section_scene_draws_physical_thickness_and_an_upward_laser() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (_work, mut engine) = open_reference_copy(
        &root.join("../../tests/projects/six_layer_hdi"),
        "SixLayerHdi.reda",
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let upward = pcb
        .stackup()
        .via_types
        .iter()
        .find(|v| v.name.as_deref() == Some("Bottom blind"))
        .unwrap()
        .uuid;
    engine.run_ok("pcb.via_type.set", json!({"via_type":upward, "micro":true, "name":"Bottom laser", "diameter":{"start":"0.5mm","end":"0.7mm"}}));
    let pcb = engine.project().unwrap().pcb().unwrap();
    let t = Tokens::new(egui::Theme::Dark, &Settings::default());
    let body = Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 400.0));
    let info = pcb.stackup().info();
    let names = Names::new(pcb, &info);
    let section = Section::new(
        pcb,
        &info,
        &names,
        body,
        Scale::Physical,
        BarrelColumns {
            first: 180.0,
            pitch: 110.0,
            staggered: false,
        },
        &t,
    );
    let top = pcb.copper_layers().next().unwrap().uuid;
    assert!(section.scene.items_of(top).any(|item| matches!(
        item.primitive,
        Primitive::Rect {
            size: (_, Nm(35_000)),
            ..
        }
    )));
    let cone = section
        .scene
        .items_of(upward)
        .find_map(|item| match &item.primitive {
            Primitive::Triangles(mesh) => Some(&mesh.points),
            _ => None,
        })
        .unwrap();
    // The bottom foil starts at -1.6 mm. The top of L5 is 135 μm above it.
    let opening = |y| {
        let mut edge = cone.iter().filter(|p| p.y == Nm(y)).map(|p| p.x);
        let a = edge.next().unwrap();
        let b = edge.next().unwrap();
        a.max(b) - a.min(b)
    };
    assert!(
        opening(-1_600_000) > opening(-1_465_000),
        "the opening is widest at the drilling origin"
    );
    assert_golden(
        root.join("tests/snapshots/stackup_cross_section.scene"),
        section.scene.to_text().as_bytes(),
    );
}
