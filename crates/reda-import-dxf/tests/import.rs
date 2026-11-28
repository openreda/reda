//! `pcb.import_dxf.preview` and `pcb.import_dxf` on a new board, through the engine.

use std::path::PathBuf;

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{GraphicShape, LayerKind, PcbDocument, PcbGraphic, Purpose, Uuid};
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

fn preview(e: &mut TestEngine, params: Value) -> Value {
    e.run_ok("pcb.import_dxf.preview", params)
}

fn import(e: &mut TestEngine, params: Value) -> Value {
    e.run_ok("pcb.import_dxf", params)["result"].clone()
}

fn graphics(e: &TestEngine) -> Vec<PcbGraphic> {
    pcb(e).graphics().cloned().collect()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

/// The rows as (DXF layer, entities, target, Layer title, pre-fill rule).
fn rows(report: &Value) -> Vec<(String, u64, String, String, String)> {
    report["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let text = |k: &str| r[k].as_str().unwrap_or("").to_owned();
            (
                text("name"),
                r["entities"].as_u64().unwrap(),
                text("target"),
                text("layer_name"),
                text("prefill"),
            )
        })
        .collect()
}

#[test]
fn the_preview_changes_nothing_and_reports_rows_mapping_size_and_skipped() {
    let dir = TempDir::new("dxf-preview");
    let mut e = board(&dir);
    let before = pcb(&e).clone();
    let report = preview(&mut e, json!({"path": fixture("layers.dxf")}));
    assert_eq!(pcb(&e), &before);
    let silk = layer_named(&e, "Top Silkscreen").to_string();
    let dims = layer_named(&e, "Dimensions").to_string();
    let reference = layer_named(&e, "Reference").to_string();
    let row = |name: &str, n, target: &str, layer: &str, rule: &str| {
        (name.into(), n, target.into(), layer.into(), rule.into())
    };
    assert_eq!(
        rows(&report),
        [
            // 1. The same name as a board Layer, in any case.
            row("Top Silkscreen", 1, &silk, "Top Silkscreen", "same_name"),
            row("dimensions", 1, &dims, "Dimensions", "same_name"),
            // 2. An outline-like name: the Board Outline, for the first such row only.
            row("EDGE_CUTS", 1, "board_outline", "", "outline"),
            // 4. Anything else: the first Reference Layer.
            row("PCB-PROFILE", 1, &reference, "Reference", "reference"),
            // 3. A name that suggests a Purpose: the first Layer with it.
            row("Dim-Annotations", 1, &dims, "Dimensions", "purpose"),
            row("Notes", 1, &reference, "Reference", "reference"),
        ]
    );
    assert_eq!(report["unit"], "mm");
    assert_eq!(report["file_unit"], "mm");
    assert_eq!(report["scale"], 1.0);
    assert_eq!(report["size"], json!({"width": 100.0, "height": 80.0}));
    assert_eq!(report["origin"], json!({"x": 0, "y": 0}));
    assert_eq!(report["line_width"], 150_000);
    assert_eq!(report["close_gaps"], 1_000);
    assert_eq!(report["skipped"], json!({"HATCH": 1, "TEXT": 1}));
}

#[test]
fn every_row_lands_on_its_target_as_one_undo_step() {
    let dir = TempDir::new("dxf-import");
    let mut e = board(&dir);
    let layers_before = pcb(&e).layers().clone();
    let out = import(&mut e, json!({"path": fixture("layers.dxf")}));
    let objects: Vec<Uuid> = serde_json::from_value(out["objects"].clone()).unwrap();
    // A line, a line, the outline's four lines, two half arcs, a line and an arc.
    assert_eq!(objects.len(), 10);
    let all = graphics(&e);
    assert_eq!(all.len(), 10);
    assert!(objects.iter().all(|o| pcb(&e).graphic(*o).is_some()));
    let on = |title: &str| {
        let layer = layer_named(&e, title);
        all.iter().filter(|g| g.layer == layer).count()
    };
    assert_eq!(on("Top Silkscreen"), 1);
    assert_eq!(on("Dimensions"), 2);
    // The Board Outline row lands on the Reference Layer, with a Messages entry saying
    // how to make it the outline, beside PCB-PROFILE's circle and the Notes arc.
    assert_eq!(on("Reference"), 4 + 2 + 1);
    assert_eq!(pcb(&e).layers(), &layers_before);
    let messages = out["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        messages[0]
            .as_str()
            .unwrap()
            .contains("pcb.board_outline.add_from_graphics"),
        "{messages:?}"
    );
    let doc = pcb(&e).uuid;
    e.run_ok("edit.undo", json!({"document": doc}));
    assert!(graphics(&e).is_empty());
}

#[test]
fn the_mapping_overrides_the_pre_fill_row_by_row() {
    let dir = TempDir::new("dxf-mapping");
    let mut e = board(&dir);
    let mapping = json!({
        "Top Silkscreen": "skip",
        "Notes": "new_mechanical",
        "PCB-PROFILE": "silkscreen_bottom",
        "Dim-Annotations": "courtyard_top",
    });
    let report = preview(
        &mut e,
        json!({"path": fixture("layers.dxf"), "mapping": mapping}),
    );
    let by_name = |name: &str| rows(&report).into_iter().find(|r| r.0 == name).unwrap();
    let bottom_silk = layer_named(&e, "Bottom Silkscreen").to_string();
    let courtyard = layer_named(&e, "Top Courtyard").to_string();
    assert_eq!(by_name("Top Silkscreen").2, "skip");
    assert_eq!(
        by_name("Top Silkscreen").4,
        "",
        "a given target has no pre-fill rule"
    );
    assert_eq!(by_name("PCB-PROFILE").2, bottom_silk);
    assert_eq!(by_name("Dim-Annotations").2, courtyard);
    assert_eq!(
        (by_name("Notes").2.as_str(), by_name("Notes").3.as_str()),
        ("new_mechanical", "Notes")
    );
    let notes = report["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Notes")
        .unwrap()
        .clone();
    assert_eq!(notes["new_layer"], true);

    let out = import(
        &mut e,
        json!({"path": fixture("layers.dxf"), "mapping": mapping}),
    );
    // "New Mechanical Layer" is named after the DXF layer, with the Purpose Reference.
    let added: Vec<Uuid> = serde_json::from_value(out["layers_added"].clone()).unwrap();
    assert_eq!(added.len(), 1);
    let layer = pcb(&e).layers().layer(added[0]).unwrap().clone();
    assert_eq!(
        (layer.kind, layer.name.as_deref(), layer.purpose),
        (LayerKind::Mechanical, Some("Notes"), Purpose::Reference)
    );
    let on = |layer: Uuid| graphics(&e).iter().filter(|g| g.layer == layer).count();
    assert_eq!(on(added[0]), 1);
    assert_eq!(on(layer_named(&e, "Top Silkscreen")), 0);
    assert_eq!(on(layer_named(&e, "Bottom Silkscreen")), 2);
    assert_eq!(on(layer_named(&e, "Top Courtyard")), 1);
    // Undo takes the new Layer away with the graphics.
    let doc = pcb(&e).uuid;
    e.run_ok("edit.undo", json!({"document": doc}));
    assert!(pcb(&e).layers().layer(added[0]).is_none());
}

#[test]
fn a_board_without_a_reference_layer_gets_one_for_the_rows_that_need_it() {
    let dir = TempDir::new("dxf-no-reference");
    let mut e = board(&dir);
    let reference = layer_named(&e, "Reference");
    e.run_ok("pcb.layer.remove", json!({"layer": reference}));
    let report = preview(&mut e, json!({"path": fixture("layers.dxf")}));
    let notes = rows(&report).into_iter().find(|r| r.0 == "Notes").unwrap();
    assert_eq!(
        (notes.2.as_str(), notes.3.as_str(), notes.4.as_str()),
        ("reference", "Reference", "reference")
    );
    let out = import(&mut e, json!({"path": fixture("layers.dxf")}));
    let added: Vec<Uuid> = serde_json::from_value(out["layers_added"].clone()).unwrap();
    assert_eq!(added.len(), 1, "one Reference Layer serves every row");
    let layer = pcb(&e).layers().layer(added[0]).unwrap();
    assert_eq!(
        (layer.name.as_deref(), layer.purpose),
        (Some("Reference"), Purpose::Reference)
    );
    let on_it = graphics(&e).iter().filter(|g| g.layer == added[0]).count();
    assert_eq!(on_it, 4 + 2 + 1);
}

#[test]
fn the_unit_comes_from_insunits_or_is_mm_and_the_scale_multiplies() {
    let dir = TempDir::new("dxf-units");
    let mut e = board(&dir);
    let inch = preview(&mut e, json!({"path": fixture("inch.dxf")}));
    assert_eq!(
        (&inch["unit"], &inch["file_unit"]),
        (&json!("inch"), &json!("inch"))
    );
    assert_eq!(inch["size"], json!({"width": 50.8, "height": 25.4}));
    let as_mm = preview(&mut e, json!({"path": fixture("inch.dxf"), "unit": "mm"}));
    assert_eq!(as_mm["size"], json!({"width": 2.0, "height": 1.0}));
    let scaled = preview(
        &mut e,
        json!({"path": fixture("inch.dxf"), "unit": "mil", "scale": 2.5}),
    );
    assert_eq!(scaled["size"], json!({"width": 0.127, "height": 0.0635}));
    let missing = preview(&mut e, json!({"path": fixture("no_insunits.dxf")}));
    assert_eq!(missing["unit"], "mm");
    assert!(missing.get("file_unit").is_none());
    assert_eq!(missing["size"], json!({"width": 1.0, "height": 2.0}));

    import(&mut e, json!({"path": fixture("inch.dxf")}));
    let GraphicShape::Line { from, to } = graphics(&e)[0].shape else {
        panic!("not a line")
    };
    assert_eq!((from, to), (mm(0.0, 0.0), mm(50.8, 25.4)));
}

#[test]
fn the_drawing_lands_at_its_file_coordinates_on_the_output_origin() {
    let dir = TempDir::new("dxf-origin");
    let mut e = board(&dir);
    // Design zero without an Output Origin; board Y grows upwards, as DXF's does.
    import(
        &mut e,
        json!({"path": fixture("line.dxf"), "line_width": "0.2mm"}),
    );
    let line = graphics(&e)[0].clone();
    assert_eq!(
        line.shape,
        GraphicShape::Line {
            from: mm(10.0, -5.5),
            to: mm(40.25, 15.0)
        }
    );
    assert_eq!(line.width, Nm(200_000));
    let doc = pcb(&e).uuid;
    e.run_ok("edit.undo", json!({"document": doc}));

    e.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "5mm", "y": "7mm"}}),
    );
    let out = import(&mut e, json!({"path": fixture("line.dxf")}));
    assert_eq!(out["origin"], json!({"x": 5_000_000, "y": 7_000_000}));
    let line = graphics(&e)[0].clone();
    assert_eq!(
        line.shape,
        GraphicShape::Line {
            from: mm(15.0, 1.5),
            to: mm(45.25, 22.0)
        }
    );
}

#[test]
fn place_with_cursor_puts_the_drawings_lower_left_corner_at_the_point() {
    let dir = TempDir::new("dxf-at");
    let mut e = board(&dir);
    e.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "5mm", "y": "7mm"}}),
    );
    let out = import(
        &mut e,
        json!({"path": fixture("line.dxf"), "at": {"x": "100mm", "y": "100mm"}}),
    );
    let GraphicShape::Line { from, to } = graphics(&e)[0].shape else {
        panic!("not a line")
    };
    assert_eq!((from, to), (mm(100.0, 100.0), mm(130.25, 120.5)));
    assert_eq!(out["origin"], json!({"x": 90_000_000, "y": 105_500_000}));
}

#[test]
fn bulged_segments_become_exact_arcs_with_one_line_width() {
    let dir = TempDir::new("dxf-bulge");
    let mut e = board(&dir);
    let out = import(
        &mut e,
        json!({"path": fixture("lwpolyline.dxf"), "line_width": "0.3mm"}),
    );
    let slot: Vec<GraphicShape> = serde_json::from_value::<Vec<Uuid>>(out["objects"].clone())
        .unwrap()
        .iter()
        .take(4)
        .map(|o| pcb(&e).graphic(*o).unwrap().shape)
        .collect();
    assert_eq!(
        slot,
        [
            GraphicShape::Line {
                from: mm(0.0, 0.0),
                to: mm(20.0, 0.0)
            },
            GraphicShape::Arc {
                from: mm(20.0, 0.0),
                to: mm(20.0, 10.0),
                sweep: Millideg(180_000)
            },
            GraphicShape::Line {
                from: mm(20.0, 10.0),
                to: mm(0.0, 10.0)
            },
            GraphicShape::Arc {
                from: mm(0.0, 10.0),
                to: mm(0.0, 0.0),
                sweep: Millideg(180_000)
            },
        ]
    );
    let all = graphics(&e);
    assert_eq!(all.len(), 6);
    assert!(all.iter().all(|g| g.width == Nm(300_000)));
    assert!(all.iter().any(|g| g.shape
        == GraphicShape::Arc {
            from: mm(0.0, 0.0),
            to: mm(10.0, 0.0),
            sweep: Millideg(-90_000)
        }));
    // The slot's ends bulge out to x = -5 and 25.
    let report = preview(&mut e, json!({"path": fixture("lwpolyline.dxf")}));
    assert_eq!(report["size"], json!({"width": 30.0, "height": 10.0}));
}

#[test]
fn bad_params_are_refused_and_change_nothing() {
    let dir = TempDir::new("dxf-refusals");
    let mut e = board(&dir);
    let before = pcb(&e).clone();
    let path = fixture("layers.dxf");
    for (params, pointer) in [
        (json!({"path": "layers.dxf"}), "/path"),
        (json!({"path": path, "scale": 0}), "/scale"),
        (json!({"path": path, "line_width": 0}), "/line_width"),
        (json!({"path": path, "close_gaps": "-1mm"}), "/close_gaps"),
        (
            json!({"path": path, "mapping": {"Nowhere": "skip"}}),
            "/mapping/Nowhere",
        ),
        (
            json!({"path": path, "mapping": {"Notes": "solder_mask_top"}}),
            "/mapping/Notes",
        ),
        (
            json!({"path": path, "mapping": {"Notes": "board_outline", "EDGE_CUTS": "board_outline"}}),
            "/mapping/Notes",
        ),
    ] {
        let err = e.run_err("pcb.import_dxf", params.clone());
        assert_eq!(
            err.details.as_ref().and_then(|d| d["pointer"].as_str()),
            Some(pointer),
            "{params}: {err}"
        );
    }
    let err = e.run_err(
        "pcb.import_dxf",
        json!({"path": dir.path().join("missing.dxf")}),
    );
    assert!(err.to_string().contains("does not exist"), "{err}");
    let not_dxf = dir.path().join("not.dxf");
    std::fs::write(&not_dxf, "hello\nworld\n").unwrap();
    let err = e.run_err("pcb.import_dxf.preview", json!({"path": not_dxf}));
    assert!(err.to_string().contains("line 1"), "{err}");
    assert_eq!(pcb(&e), &before);
}

#[test]
fn a_drawing_reaching_past_a_metre_previews_with_a_warning_and_is_not_imported() {
    let dir = TempDir::new("dxf-far");
    let mut e = board(&dir);
    let params = json!({"path": fixture("layers.dxf"), "unit": "inch", "scale": 0.5});
    let report = preview(&mut e, params.clone());
    assert_eq!(report["size"], json!({"width": 1270.0, "height": 1016.0}));
    let warnings = report["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("check the unit and scale")),
        "{warnings:?}"
    );
    let err = e.run_err("pcb.import_dxf", params);
    assert!(
        err.to_string().contains("check the unit and scale"),
        "{err}"
    );
    assert!(graphics(&e).is_empty());
}
