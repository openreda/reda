//! Lines, arcs and rectangles on the board, through Commands.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{GraphicShape, LayerKind, PcbDocument, Side, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::json;

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok("document.add", json!({"kind": "pcb"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn created(out: &serde_json::Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn pointer(err: &reda_command::CommandError) -> &str {
    err.details
        .as_ref()
        .and_then(|d| d["pointer"].as_str())
        .unwrap_or("")
}

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

#[test]
fn lines_arcs_and_rectangles_are_drawn_on_a_layer() {
    let dir = TempDir::new("graphics-add");
    let mut engine = engine_with_pcb(&dir);
    let line = created(&engine.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "silkscreen_top", "from": {"x": 0, "y": 0},
               "to": {"x": "10mm", "y": 0}, "width": "0.2mm"}),
    ));
    let arc = created(&engine.run_ok(
        "pcb.graphic.add_arc",
        json!({"layer": "copper_bottom", "from": {"x": "10mm", "y": 0},
               "to": {"x": 0, "y": "10mm"}, "sweep": "90deg"}),
    ));
    let rect = created(&engine.run_ok(
        "pcb.graphic.add_rect",
        json!({"layer": "mechanical", "from": {"x": "5mm", "y": "5mm"},
               "to": {"x": 0, "y": 0}, "filled": true}),
    ));
    let pcb = pcb(&engine);
    let line = pcb.graphic(line).unwrap();
    assert_eq!(
        line.layer,
        pcb.layers()
            .find(LayerKind::Silkscreen, Some(Side::Top))
            .unwrap()
            .uuid
    );
    assert_eq!(line.width, Nm::from_um(200));
    assert_eq!(
        line.shape,
        GraphicShape::Line {
            from: mm(0, 0),
            to: mm(10, 0)
        }
    );
    let arc = pcb.graphic(arc).unwrap();
    assert_eq!(arc.width, Nm::from_um(150), "the default pen width");
    assert_eq!(
        arc.shape,
        GraphicShape::Arc {
            from: mm(10, 0),
            to: mm(0, 10),
            sweep: Millideg::from_deg(90)
        }
    );
    assert_eq!(
        pcb.graphic(rect).unwrap().shape,
        GraphicShape::Rect {
            from: mm(5, 5),
            to: mm(0, 0),
            filled: true
        }
    );
}

#[test]
fn degenerate_graphics_and_wrong_layers_are_refused() {
    let dir = TempDir::new("graphics-refused");
    let mut engine = engine_with_pcb(&dir);
    let p = |x: i64, y: i64| json!({"x": x, "y": y});
    for (id, params, expected) in [
        (
            "pcb.graphic.add_line",
            json!({"layer": "paste_top", "from": p(0, 0), "to": p(1, 0)}),
            "/layer",
        ),
        (
            "pcb.graphic.add_line",
            json!({"layer": "mechanical", "from": p(1, 1), "to": p(1, 1)}),
            "/to",
        ),
        (
            "pcb.graphic.add_line",
            json!({"layer": "mechanical", "from": p(0, 0), "to": p(1, 0), "width": 0}),
            "/width",
        ),
        (
            "pcb.graphic.add_arc",
            json!({"layer": "mechanical", "from": p(0, 0), "to": p(1, 0), "sweep": 0}),
            "/sweep",
        ),
        (
            "pcb.graphic.add_arc",
            json!({"layer": "mechanical", "from": p(0, 0), "to": p(1, 0), "sweep": "400deg"}),
            "/sweep",
        ),
        (
            "pcb.graphic.add_rect",
            json!({"layer": "mechanical", "from": p(0, 0), "to": p(5, 0)}),
            "/to",
        ),
    ] {
        let err = engine.run_err(id, params);
        assert_eq!(pointer(&err), expected, "{id}: {}", err.message);
    }
}

#[test]
fn a_graphic_is_deleted() {
    let dir = TempDir::new("graphics-delete");
    let mut engine = engine_with_pcb(&dir);
    let uuid = created(&engine.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "documentation", "from": {"x": 0, "y": 0}, "to": {"x": 1, "y": 1}}),
    ));
    engine.run_ok("pcb.graphic.delete", json!({"uuid": uuid}));
    assert!(pcb(&engine).graphic(uuid).is_none());
    let err = engine.run_err("pcb.graphic.delete", json!({"uuid": uuid}));
    assert_eq!(err.code, reda_command::ErrorCode::NotFound);
}
