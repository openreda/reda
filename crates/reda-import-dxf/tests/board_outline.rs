//! The DXF layer mapped to the Board Outline goes through the loop builder: the largest
//! loop becomes the Board Outline, loops inside it Board Cutouts, and the rest lands on
//! the Reference Layer with a reason.

use std::path::PathBuf;

use reda_model::geom::{Millideg, Nm, Point, Vertex};
use reda_model::{GraphicShape, PcbDocument, PcbGraphic, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A new two-layer board: its Mechanical Layers are Dimensions and Reference.
fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":2}),
    );
    engine
}

fn pcb(e: &TestEngine) -> &PcbDocument {
    e.project().unwrap().pcb().unwrap()
}

fn layer_named(e: &TestEngine, title: &str) -> Uuid {
    let pcb = pcb(e);
    pcb.layers()
        .layers
        .iter()
        .find(|l| pcb.layer_title(l) == title)
        .unwrap_or_else(|| panic!("no Layer {title}"))
        .uuid
}

fn import(e: &mut TestEngine, params: Value) -> Value {
    e.run_ok("pcb.import_dxf", params)["result"].clone()
}

fn on_layer(e: &TestEngine, title: &str) -> Vec<PcbGraphic> {
    let layer = layer_named(e, title);
    pcb(e)
        .graphics()
        .filter(|g| g.layer == layer)
        .cloned()
        .collect()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

/// The points of a contour from its lowest (by x, then y), counter-clockwise, so two
/// tracings of one loop compare equal.
fn normalized(vertices: &[Vertex]) -> Vec<(Point, Millideg)> {
    let n = vertices.len();
    // The test contours are rectangles (shoelace) or circles (two half arcs, by sweep).
    let shoelace: i128 = (0..n)
        .map(|i| {
            let (a, b) = (vertices[i].point(), vertices[(i + 1) % n].point());
            i128::from(a.x.0) * i128::from(b.y.0) - i128::from(b.x.0) * i128::from(a.y.0)
        })
        .sum();
    let sweep: i64 = vertices.iter().map(|v| v.sweep.0).sum();
    let ccw = if shoelace != 0 {
        shoelace > 0
    } else {
        sweep > 0
    };
    let ring: Vec<(Point, Millideg)> = if ccw {
        vertices.iter().map(|v| (v.point(), v.sweep)).collect()
    } else {
        (0..n)
            .rev()
            .map(|i| (vertices[(i + 1) % n].point(), -vertices[i].sweep))
            .collect()
    };
    let start = (0..n)
        .min_by_key(|&i| (ring[i].0.x.0, ring[i].0.y.0))
        .unwrap();
    (0..n).map(|i| ring[(start + i) % n]).collect()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(Point, Millideg)> {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .into_iter()
        .map(|(x, y)| (mm(x, y), Millideg::ZERO))
        .collect()
}

fn circle(cx: f64, cy: f64, r: f64) -> Vec<(Point, Millideg)> {
    let half = Millideg::from_deg(180);
    vec![(mm(cx - r, cy), half), (mm(cx + r, cy), half)]
}

#[test]
fn the_largest_loop_becomes_the_board_outline_and_loops_inside_it_board_cutouts() {
    let dir = TempDir::new("dxf-outline");
    let mut e = board(&dir);
    let out = import(&mut e, json!({"path": fixture("outline.dxf")}));

    let outline = pcb(&e).outline().expect("a Board Outline").clone();
    assert_eq!(normalized(&outline.vertices), rect(0.0, 0.0, 100.0, 80.0));
    let mut cutouts: Vec<_> = pcb(&e).cutouts().map(|c| normalized(&c.vertices)).collect();
    cutouts.sort_by_key(|c| (c[0].0.x.0, c[0].0.y.0));
    // The circle is a cutout too, and four loose lines in mixed directions close one.
    assert_eq!(
        cutouts,
        [
            rect(10.0, 50.0, 30.0, 70.0),
            circle(20.0, 20.0, 3.0),
            rect(50.0, 30.0, 70.0, 50.0),
        ]
    );
    let made: Vec<Uuid> = serde_json::from_value(out["objects"].clone()).unwrap();
    assert!(made.contains(&outline.uuid));
    assert!(pcb(&e).cutouts().all(|c| made.contains(&c.uuid)));
}

fn messages(out: &Value) -> Vec<String> {
    out["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_owned())
        .collect()
}

fn count(said: &[String], words: &str) -> usize {
    said.iter().filter(|m| m.contains(words)).count()
}

#[test]
fn loops_outside_in_a_cutout_or_across_the_edge_land_on_the_reference_layer_with_a_reason() {
    let dir = TempDir::new("dxf-outline-strays");
    let mut e = board(&dir);
    let out = import(&mut e, json!({"path": fixture("outline.dxf")}));
    let reference = on_layer(&e, "Reference");
    let shapes: Vec<GraphicShape> = reference.iter().map(|g| g.shape).collect();
    let half = Millideg::from_deg(180);
    for (centre, r) in [((60.0, 40.0), 2.0), ((150.0, 40.0), 5.0)] {
        let (left, right) = (mm(centre.0 - r, centre.1), mm(centre.0 + r, centre.1));
        let arcs = [
            GraphicShape::Arc {
                from: right,
                to: left,
                sweep: half,
            },
            GraphicShape::Arc {
                from: left,
                to: right,
                sweep: half,
            },
        ];
        assert!(
            arcs.iter().all(|a| shapes.contains(a)),
            "the circle at {centre:?} stays a graphic: {shapes:?}"
        );
    }
    // The rectangle across the right edge stays four lines.
    let across = [
        (mm(90.0, 60.0), mm(110.0, 60.0)),
        (mm(110.0, 60.0), mm(110.0, 65.0)),
        (mm(110.0, 65.0), mm(90.0, 65.0)),
        (mm(90.0, 65.0), mm(90.0, 60.0)),
    ];
    for (from, to) in across {
        assert!(
            shapes.contains(&GraphicShape::Line { from, to }),
            "{shapes:?}"
        );
    }
    assert_eq!(reference.len(), 2 + 2 + 4);
    assert_eq!(out["outline"]["on_reference"], 8);

    let said = messages(&out);
    let row = "lines and arcs of DXF layer \"OUTLINE\"";
    let outside = format!(
        "2 {row} form loops outside the Board Outline, so they went on the Layer \"Reference\""
    );
    assert_eq!(count(&said, &outside), 1, "{said:?}");
    let nested = format!("2 {row} form loops inside a Board Cutout");
    assert_eq!(count(&said, &nested), 1, "{said:?}");
    let crossing = format!("4 {row} form loops that cross themselves or another loop");
    assert_eq!(count(&said, &crossing), 1, "{said:?}");
    assert_eq!(
        count(&said, "a shape crossing at (100 mm, 60 mm)"),
        1,
        "{said:?}"
    );
    // The text on that layer is skipped and counted.
    assert_eq!(
        out["skipped"],
        json!({"text on the Board Outline layer": 1})
    );
    assert_eq!(pcb(&e).texts().count(), 0);
}

#[test]
fn an_existing_board_outline_and_its_cutouts_are_replaced_and_other_objects_stay() {
    let dir = TempDir::new("dxf-outline-replace");
    let mut e = board(&dir);
    e.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "50mm", "height": "50mm"}),
    );
    e.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": [
            {"x": "10mm", "y": "10mm"}, {"x": "20mm", "y": "10mm"}, {"x": "20mm", "y": "20mm"},
        ]}),
    );
    e.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "silkscreen_top", "from": {"x": "1mm", "y": "1mm"},
               "to": {"x": "40mm", "y": "1mm"}}),
    );
    let before = pcb(&e).clone();
    let old_outline = before.outline().unwrap().uuid;
    let old_cutout = before.cutouts().next().unwrap().uuid;

    let report = e.run_ok(
        "pcb.import_dxf.preview",
        json!({"path": fixture("outline.dxf")}),
    );
    assert_eq!(report["outline"]["replaces"], true);
    assert_eq!(report["outline"]["builds_outline"], true);
    assert_eq!(report["outline"]["board_cutouts"], 3);
    assert_eq!(pcb(&e), &before, "the preview changes nothing");

    let out = import(&mut e, json!({"path": fixture("outline.dxf")}));
    let after = pcb(&e);
    let outline = after.outline().unwrap();
    assert_ne!(outline.uuid, old_outline);
    assert_eq!(normalized(&outline.vertices), rect(0.0, 0.0, 100.0, 80.0));
    assert!(after.cutout(old_cutout).is_none());
    assert_eq!(after.cutouts().count(), 3);
    assert_eq!(out["board_outline"], json!(outline.uuid));
    // The silkscreen line did not move.
    for g in before.graphics() {
        assert_eq!(after.graphic(g.uuid), Some(g));
    }
    let said = messages(&out);
    let replaced = "replacing the board's Board Outline and its 1 Board Cutouts";
    assert_eq!(count(&said, replaced), 1, "{said:?}");

    // One undo step brings the old ones back.
    let doc = pcb(&e).uuid;
    e.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(pcb(&e), &before);
}

#[test]
fn the_preview_reports_each_gap_with_its_size_and_each_crossing_within_close_gaps() {
    let dir = TempDir::new("dxf-outline-gaps");
    let mut e = board(&dir);
    let report = e.run_ok(
        "pcb.import_dxf.preview",
        json!({"path": fixture("gaps.dxf")}),
    );
    assert_eq!(report["close_gaps"], 1_000);
    // The 0.0005 mm corner closes within the default 0.001 mm; the 0.05 mm one doesn't,
    // and the bow tie crosses itself in its middle.
    assert_eq!(
        report["outline"],
        json!({
            "layer": "OUTLINE",
            "builds_outline": false,
            "board_cutouts": 0,
            "replaces": false,
            "on_reference": 8,
            "problems": [
                {"kind": "gap", "at": {"x": 0, "y": 25_000}, "size": 50_000},
                {"kind": "crossing", "at": {"x": 130_000_000, "y": 10_000_000}},
            ],
        })
    );
    // A wider tolerance closes the 0.05 mm gap too.
    let report = e.run_ok(
        "pcb.import_dxf.preview",
        json!({"path": fixture("gaps.dxf"), "close_gaps": "0.1mm"}),
    );
    assert_eq!(report["close_gaps"], 100_000);
    assert_eq!(report["outline"]["builds_outline"], true);
    assert_eq!(
        report["outline"]["problems"],
        json!([{"kind": "crossing", "at": {"x": 130_000_000, "y": 10_000_000}}])
    );
    let out = import(
        &mut e,
        json!({"path": fixture("gaps.dxf"), "close_gaps": "0.1mm"}),
    );
    assert_eq!(out["close_gaps"], 100_000);
    let outline = pcb(&e).outline().unwrap();
    assert_eq!(normalized(&outline.vertices), rect(0.0, 0.0, 100.0, 80.0));
}

#[test]
fn importing_open_shapes_anyway_puts_them_on_the_reference_layer_and_keeps_the_outline() {
    let dir = TempDir::new("dxf-outline-open");
    let mut e = board(&dir);
    e.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "50mm", "height": "50mm"}),
    );
    let before = pcb(&e).outline().unwrap().clone();
    let out = import(&mut e, json!({"path": fixture("gaps.dxf")}));
    // No guessed outline: the board's own stays.
    assert_eq!(pcb(&e).outline(), Some(&before));
    assert_eq!(pcb(&e).cutouts().count(), 0);
    assert!(out.get("board_outline").is_none());
    // The four lines exactly as drawn, gaps and all, and the bow tie's four.
    let reference = on_layer(&e, "Reference");
    assert_eq!(reference.len(), 8);
    let shapes: Vec<GraphicShape> = reference.iter().map(|g| g.shape).collect();
    assert!(shapes.contains(&GraphicShape::Line {
        from: mm(0.0, 80.0),
        to: mm(0.0, 0.05)
    }));
    assert!(shapes.contains(&GraphicShape::Line {
        from: mm(100.0, 0.0005),
        to: mm(100.0, 80.0)
    }));
    let said = messages(&out);
    let gap = "a gap of 0.05 mm at (0 mm, 0.025 mm)";
    assert_eq!(count(&said, gap), 1, "{said:?}");
    let crossing = "a shape crossing at (130 mm, 10 mm)";
    assert_eq!(count(&said, crossing), 1, "{said:?}");
    let open = "4 lines and arcs of DXF layer \"OUTLINE\" do not close a loop, so they went \
                on the Layer \"Reference\" as graphics: repair them there and use \
                pcb.board_outline.add_from_graphics";
    assert_eq!(count(&said, open), 1, "{said:?}");
    let kept = "so it made no Board Outline; the board's own is kept";
    assert_eq!(count(&said, kept), 1, "{said:?}");
}

#[test]
fn the_reference_layer_is_added_only_when_something_lands_on_it() {
    let dir = TempDir::new("dxf-outline-no-reference");
    let mut e = board(&dir);
    let reference = layer_named(&e, "Reference");
    e.run_ok("pcb.layer.remove", json!({"layer": reference}));
    // The lwpolyline fixture's closed slot, mapped to the Board Outline, closes whole.
    let out = import(
        &mut e,
        json!({"path": fixture("lwpolyline.dxf"),
               "mapping": {"Slot": "board_outline", "Open": "skip"}}),
    );
    assert!(pcb(&e).outline().is_some());
    assert!(out.get("layers_added").is_none(), "{out}");
    assert_eq!(pcb(&e).graphics().count(), 0);
}
