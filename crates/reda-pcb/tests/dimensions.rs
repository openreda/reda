//! Linear Dimensions: adding them with the spec's fields, following their references,
//! exploding them into free lines and text.

mod common;

use common::{FIDUCIAL, MOUNTING_HOLE_M3, PCB, created, engine, pcb};
use reda_command::Fonts;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    ArrowPosition, Dimension, DimensionFormat, DimensionTextPosition, DimensionUnits, GraphicShape,
    Hotspot, HotspotRef, LayerKind, Side, TextHJustify, TextVJustify, Uuid,
};
use reda_pcb::dimension::{drawing, label, value};
use reda_testkit::{TempDir, TestEngine};
use serde_json::json;

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

fn dimension(engine: &TestEngine, uuid: Uuid) -> &Dimension {
    pcb(engine).dimension(uuid).expect("the Dimension")
}

fn add(engine: &mut TestEngine, params: serde_json::Value) -> Uuid {
    created(&engine.run_ok("pcb.dimension.add", params))
}

fn free_pad(engine: &mut TestEngine, x: &str, y: &str) -> Uuid {
    created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": x, "y": y}, "width": "1mm"}),
    ))
}

fn pointer(err: &reda_command::CommandError) -> &str {
    err.details
        .as_ref()
        .and_then(|d| d["pointer"].as_str())
        .unwrap_or("")
}

#[test]
fn a_new_dimension_takes_the_spec_fields_with_defaults() {
    let dir = TempDir::new("dimension-defaults");
    let mut engine = engine(&dir);
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"point": {"x": 0, "y": 0}},
               "b": {"point": {"x": "10mm", "y": "5mm"}}, "rotation": 0}),
    );
    let pcb = pcb(&engine);
    let d = pcb.dimension(uuid).unwrap();
    assert_eq!(
        d.layer,
        pcb.layers().find(LayerKind::Mechanical, None).unwrap().uuid
    );
    assert_eq!((d.a.point, d.b.point), (mm(0.0, 0.0), mm(10.0, 5.0)));
    assert_eq!((d.a.reference, d.b.reference), (None, None));
    assert_eq!(d.rotation, Millideg::ZERO);
    assert_eq!(d.width, Nm::from_um(150));
    assert_eq!(d.extension_width, Nm::from_um(150));
    assert_eq!(d.gap, Nm::from_um(500));
    assert_eq!(d.offset, Nm::from_mm(2));
    assert_eq!(d.text_gap, Nm::from_um(250));
    assert_eq!(d.arrow_size, Nm::from_mm(1));
    assert_eq!(d.arrow_length, Nm::from_mm(2));
    assert_eq!(d.arrow_position, ArrowPosition::Inside);
    assert_eq!(d.text_position, DimensionTextPosition::Automatic);
    assert_eq!(d.units, DimensionUnits::Automatic);
    assert_eq!((d.precision, d.format), (2, DimensionFormat::ValueUnit));
    assert_eq!(d.font, "Hershey Roman Simplex");
    assert_eq!((d.height, d.thickness), (Nm::from_mm(1), Nm::from_um(150)));
    // Measured along x only: the 5 mm of y does not count.
    assert_eq!(value(d), Nm::from_mm(10));
    assert_eq!(label(d), "10.00mm");
}

#[test]
fn without_a_rotation_it_measures_along_the_line_between_its_ends() {
    let dir = TempDir::new("dimension-aligned");
    let mut engine = engine(&dir);
    let uuid = add(
        &mut engine,
        json!({"layer": "silkscreen_top", "a": {"point": {"x": 0, "y": 0}},
               "b": {"point": {"x": "3mm", "y": "4mm"}}}),
    );
    let d = dimension(&engine, uuid);
    assert_eq!(d.rotation, Millideg(53_130));
    assert_eq!(label(d), "5.00mm");
}

#[test]
fn every_field_can_be_given_and_set() {
    let dir = TempDir::new("dimension-fields");
    let mut engine = engine(&dir);
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"point": {"x": 0, "y": 0}},
               "b": {"point": {"x": 0, "y": "25.4mm"}}, "rotation": "90deg",
               "width": "0.2mm", "extension_width": "0.1mm", "gap": "0.3mm",
               "offset": "-4mm", "text_gap": "0.5mm", "arrow_size": "0.8mm",
               "arrow_length": "1.5mm", "arrow_position": "outside",
               "text_position": "inside_left",
               "style": {"font": "Noto Sans", "height": "1.5mm", "thickness": "0.2mm",
                         "knockout": {"margin": "0.1mm"}},
               "units": "mil", "precision": 1, "format": "value_bracketed_unit",
               "prefix": "W ", "suffix": " max"}),
    );
    let d = dimension(&engine, uuid);
    assert_eq!(d.rotation, Millideg::from_deg(90));
    assert_eq!(
        (d.width, d.extension_width),
        (Nm::from_um(200), Nm::from_um(100))
    );
    assert_eq!(
        (d.gap, d.offset, d.text_gap),
        (Nm::from_um(300), Nm::from_mm(-4), Nm::from_um(500))
    );
    assert_eq!(
        (d.arrow_size, d.arrow_length),
        (Nm::from_um(800), Nm::from_um(1500))
    );
    assert_eq!(d.arrow_position, ArrowPosition::Outside);
    assert_eq!(d.text_position, DimensionTextPosition::InsideLeft);
    assert_eq!(
        (d.font.as_str(), d.height),
        ("Noto Sans", Nm::from_um(1500))
    );
    assert_eq!(d.knockout.unwrap().margin, Nm::from_um(100));
    assert_eq!(label(d), "W 1000.0 (mil) max");

    engine.run_ok(
        "pcb.dimension.set",
        json!({"uuid": uuid, "units": "in", "precision": 3, "format": "value",
               "prefix": "", "suffix": "", "text_position": {"manual": {"x": "1mm", "y": "2mm"}},
               "layer": "silkscreen_bottom", "b": {"point": {"x": 0, "y": "50.8mm"}}}),
    );
    let d = dimension(&engine, uuid);
    assert_eq!(label(d), "2.000");
    assert_eq!(d.text_position, DimensionTextPosition::Manual(mm(1.0, 2.0)));
    let silk = pcb(&engine)
        .layers()
        .find(LayerKind::Silkscreen, Some(Side::Bottom))
        .unwrap()
        .uuid;
    assert_eq!(d.layer, silk);
    for (format, units, shown) in [
        ("value_unit", "cm", "5.080cm"),
        ("value_unit", "mm", "50.800mm"),
        ("none", "mm", ""),
    ] {
        engine.run_ok(
            "pcb.dimension.set",
            json!({"uuid": uuid, "format": format, "units": units}),
        );
        assert_eq!(label(dimension(&engine, uuid)), shown);
    }
}

#[test]
fn bad_fields_are_refused_with_a_pointer() {
    let dir = TempDir::new("dimension-refused");
    let mut engine = engine(&dir);
    let base = json!({"layer": "mechanical", "a": {"point": {"x": 0, "y": 0}},
                      "b": {"point": {"x": "10mm", "y": 0}}});
    let with = |extra: serde_json::Value| {
        let mut p = base.clone();
        p.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        p
    };
    for (extra, at) in [
        (json!({"layer": "courtyard_top"}), "/layer"),
        (json!({"width": 0}), "/width"),
        (json!({"gap": "-1mm"}), "/gap"),
        (json!({"precision": 7}), "/precision"),
        (json!({"rotation": "90deg"}), "/rotation"),
        (json!({"b": {"point": {"x": 0, "y": 0}}}), "/b"),
        (json!({"b": {}}), "/b"),
        (
            json!({"b": {"ref": {"object": Uuid::from_u128(7), "hotspot": "center"}}}),
            "/b/ref",
        ),
    ] {
        let err = engine.run_err("pcb.dimension.add", with(extra.clone()));
        assert_eq!(pointer(&err), at, "{extra}: {err:?}");
    }
    assert_eq!(pcb(&engine).dimensions().count(), 0);
}

#[test]
fn it_re_measures_when_a_reference_moves() {
    let dir = TempDir::new("dimension-follow");
    let mut engine = engine(&dir);
    let left = free_pad(&mut engine, "0mm", "0mm");
    let right = free_pad(&mut engine, "10mm", "0mm");
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical",
               "a": {"ref": {"object": left, "hotspot": "center"}},
               "b": {"ref": {"object": right, "hotspot": "center"}}}),
    );
    assert_eq!(label(dimension(&engine, uuid)), "10.00mm");

    engine.run_ok(
        "pcb.move",
        json!({"objects": [right], "by": {"x": "2.5mm", "y": 0}}),
    );
    let d = dimension(&engine, uuid);
    assert_eq!(d.b.point, mm(12.5, 0.0), "the point is kept current");
    assert_eq!(label(d), "12.50mm");

    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(
        label(dimension(&engine, uuid)),
        "10.00mm",
        "undo restores both"
    );
    engine.run_ok("edit.redo", json!({"document": PCB}));
    assert_eq!(label(dimension(&engine, uuid)), "12.50mm");
}

#[test]
fn it_follows_a_footprint_pad_and_origin() {
    let dir = TempDir::new("dimension-follow-footprint");
    let mut engine = engine(&dir);
    let place = |engine: &mut TestEngine, part: &str, x: &str| {
        let out = engine.run_ok(
            "pcb.mechanical.place",
            json!({"part": part, "at": {"x": x, "y": "0mm"}}),
        );
        out["result"]["footprint"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap()
    };
    let hole = place(&mut engine, MOUNTING_HOLE_M3, "0mm");
    let fiducial = place(&mut engine, FIDUCIAL, "20mm");
    let copy = pcb(&engine).footprint(hole).unwrap().footprint;
    let pad = pcb(&engine).footprint_copy(copy).unwrap().footprint.pads[0].uuid;
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical",
               "a": {"ref": {"object": hole, "hotspot": {"pad": pad}}},
               "b": {"ref": {"object": fiducial, "hotspot": "origin"}}, "rotation": 0}),
    );
    assert_eq!(value(dimension(&engine, uuid)), Nm::from_mm(20));
    engine.run_ok(
        "pcb.move",
        json!({"objects": [hole, fiducial], "by": {"x": "-5mm", "y": "1mm"}}),
    );
    engine.run_ok(
        "pcb.move",
        json!({"objects": [fiducial], "by": {"x": "3mm", "y": 0}}),
    );
    let d = dimension(&engine, uuid);
    assert_eq!(d.a.point, mm(-5.0, 1.0));
    assert_eq!(d.b.point, mm(18.0, 1.0));
    assert_eq!(value(d), Nm::from_mm(23));
}

#[test]
fn deleting_a_reference_keeps_the_dimension_without_its_ref() {
    let dir = TempDir::new("dimension-lost-ref");
    let mut engine = engine(&dir);
    let left = free_pad(&mut engine, "0mm", "0mm");
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical",
               "a": {"ref": {"object": left, "hotspot": "center"}},
               "b": {"point": {"x": "4mm", "y": 0}}}),
    );
    engine.run_ok("pcb.free_pad.delete", json!({"pads": [left]}));
    let d = dimension(&engine, uuid);
    assert_eq!(d.a.reference, None);
    assert_eq!(d.a.point, mm(0.0, 0.0));
    assert_eq!(label(d), "4.00mm");
    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(
        dimension(&engine, uuid).a.reference,
        Some(HotspotRef {
            object: left,
            hotspot: Hotspot::Center
        })
    );
}

#[test]
fn a_dimension_is_drawn_as_extension_lines_arrows_a_line_and_text() {
    let dir = TempDir::new("dimension-drawing");
    let mut engine = engine(&dir);
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"point": {"x": 0, "y": 0}},
               "b": {"point": {"x": "20mm", "y": 0}}, "offset": "3mm", "gap": "0.5mm",
               "text_gap": "0.25mm"}),
    );
    let drawn = drawing(dimension(&engine, uuid), &Fonts::default());
    let lines: Vec<(Point, Point)> = drawn.lines.iter().map(|l| (l.from, l.to)).collect();
    // Extension lines from 0.5 mm off each point to 0.5 mm past the line at y = 3 mm.
    assert!(lines.contains(&(mm(0.0, 0.5), mm(0.0, 3.5))), "{lines:?}");
    assert!(lines.contains(&(mm(20.0, 0.5), mm(20.0, 3.5))));
    // The dimension line between them, and two arrowhead sides at each end.
    assert!(lines.contains(&(mm(0.0, 3.0), mm(20.0, 3.0))));
    assert_eq!(lines.len(), 2 + 4 + 1);
    let heads: Vec<&(Point, Point)> = lines.iter().filter(|(a, _)| *a == mm(0.0, 3.0)).collect();
    assert_eq!(
        heads.len(),
        3,
        "two arrowhead sides and the line start at the left end"
    );
    assert!(
        heads.iter().all(|(_, b)| b.x > Nm::ZERO),
        "inside arrows point out"
    );
    let text = drawn.text.unwrap();
    assert_eq!(text.text, "20.00mm");
    // Automatic puts fitting text on top, centred, its bottom a text gap and half a pen
    // above the line.
    assert_eq!(text.position, mm(10.0, 3.0 + 0.25 + 0.075));
    assert_eq!(
        (text.justify.h, text.justify.v),
        (TextHJustify::Center, TextVJustify::Bottom)
    );
    assert_eq!(text.rotation, Millideg::ZERO);
}

#[test]
fn outside_arrows_and_text_that_does_not_fit_go_beyond_the_extension_lines() {
    let dir = TempDir::new("dimension-outside");
    let mut engine = engine(&dir);
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"point": {"x": 0, "y": 0}},
               "b": {"point": {"x": "2mm", "y": 0}}, "rotation": 0, "offset": "-1mm",
               "arrow_position": "outside", "arrow_length": "1mm"}),
    );
    let drawn = drawing(dimension(&engine, uuid), &Fonts::default());
    let lines: Vec<(Point, Point)> = drawn.lines.iter().map(|l| (l.from, l.to)).collect();
    assert!(
        lines.contains(&(mm(-1.0, -1.0), mm(3.0, -1.0))),
        "{lines:?}"
    );
    assert!(
        lines.contains(&(mm(0.0, -0.5), mm(0.0, -1.5))),
        "extension below the points"
    );
    let text = drawn.text.unwrap();
    assert_eq!(
        text.position,
        mm(3.25, -1.0),
        "right of the line, a text gap out"
    );
    assert_eq!(text.justify.h, TextHJustify::Left);
}

#[test]
fn text_reads_upright_at_any_angle_and_unidirectional_stays_level() {
    let dir = TempDir::new("dimension-angles");
    let mut engine = engine(&dir);
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"point": {"x": "10mm", "y": 0}},
               "b": {"point": {"x": 0, "y": 0}}, "text_position": "top"}),
    );
    let d = dimension(&engine, uuid);
    assert_eq!(d.rotation, Millideg::from_deg(180));
    let text = drawing(d, &Fonts::default()).text.unwrap();
    assert_eq!(
        text.rotation,
        Millideg::ZERO,
        "turned half a turn to read upright"
    );
    // The line is 2 mm to the left of a→b (−x), so at y = −2 mm; top reads above it.
    assert_eq!(text.position, mm(5.0, -2.0 + 0.325));

    engine.run_ok(
        "pcb.dimension.set",
        json!({"uuid": uuid, "rotation": "90deg", "b": {"point": {"x": "10mm", "y": "8mm"}},
               "text_position": "unidirectional"}),
    );
    let text = drawing(dimension(&engine, uuid), &Fonts::default())
        .text
        .unwrap();
    assert_eq!(text.rotation, Millideg::ZERO);
    assert_eq!(
        text.position,
        mm(8.0 - 0.325, 4.0),
        "level, left of the vertical line"
    );
    assert_eq!(
        (text.justify.h, text.justify.v),
        (TextHJustify::Right, TextVJustify::Center)
    );
}

#[test]
fn explode_replaces_it_with_free_lines_and_text_and_only_undo_brings_it_back() {
    let dir = TempDir::new("dimension-explode");
    let mut engine = engine(&dir);
    let pad = free_pad(&mut engine, "0mm", "0mm");
    let uuid = add(
        &mut engine,
        json!({"layer": "silkscreen_top", "a": {"ref": {"object": pad, "hotspot": "center"}},
               "b": {"point": {"x": "20mm", "y": 0}}}),
    );
    let drawn = drawing(dimension(&engine, uuid), &Fonts::default());
    let out = engine.run_ok("pcb.dimension.explode", json!({"uuid": uuid}));
    let pcb_now = pcb(&engine);
    assert!(pcb_now.dimension(uuid).is_none());
    let graphics = out["result"]["graphics"].as_array().unwrap();
    assert_eq!(graphics.len(), drawn.lines.len());
    for (g, line) in graphics.iter().zip(&drawn.lines) {
        let g = pcb_now
            .graphic(g.as_str().unwrap().parse().unwrap())
            .unwrap();
        let layer = drawn.text.as_ref().unwrap().layer;
        assert_eq!((g.layer, g.width), (layer, line.width));
        assert_eq!(
            g.shape,
            GraphicShape::Line {
                from: line.from,
                to: line.to
            }
        );
    }
    let text: Uuid = out["result"]["text"].as_str().unwrap().parse().unwrap();
    let shown = pcb_now.text(text).unwrap();
    assert_eq!(shown.text, "20.00mm");
    assert_eq!(shown.position, drawn.text.unwrap().position);
    // The free lines no longer follow the Pad.
    engine.run_ok(
        "pcb.move",
        json!({"objects": [pad], "by": {"x": "1mm", "y": 0}}),
    );
    assert_eq!(pcb(&engine).text(text).unwrap().text, "20.00mm");
    engine.run_ok("edit.undo", json!({"document": PCB}));
    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert!(pcb(&engine).dimension(uuid).is_some());
    assert_eq!(pcb(&engine).graphics().count(), 0);
}

#[test]
fn delete_removes_only_the_dimension() {
    let dir = TempDir::new("dimension-delete");
    let mut engine = engine(&dir);
    let pad = free_pad(&mut engine, "0mm", "0mm");
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"ref": {"object": pad, "hotspot": "center"}},
               "b": {"point": {"x": "5mm", "y": 0}}}),
    );
    engine.run_ok("pcb.dimension.delete", json!({"uuid": uuid}));
    assert!(pcb(&engine).dimension(uuid).is_none());
    assert!(pcb(&engine).free_pad(pad).is_some());
    let err = engine.run_err("pcb.dimension.delete", json!({"uuid": uuid}));
    assert_eq!(err.code, reda_command::ErrorCode::NotFound);
}

#[test]
fn a_dimension_moves_with_its_references_but_not_alone() {
    let dir = TempDir::new("dimension-move");
    let mut engine = engine(&dir);
    let pad = free_pad(&mut engine, "0mm", "0mm");
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"ref": {"object": pad, "hotspot": "center"}},
               "b": {"point": {"x": "10mm", "y": 0}}, "rotation": 0}),
    );
    let err = engine.run_err(
        "pcb.move",
        json!({"objects": [uuid], "by": {"x": "1mm", "y": 0}}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::Rejected);
    assert_eq!(err.details.unwrap()["references"], json!([pad]));

    engine.run_ok(
        "pcb.move",
        json!({"objects": [uuid, pad], "by": {"x": "1mm", "y": "2mm"}}),
    );
    let d = dimension(&engine, uuid);
    assert_eq!((d.a.point, d.b.point), (mm(1.0, 2.0), mm(11.0, 2.0)));
    assert_eq!(value(d), Nm::from_mm(10));

    let free = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"point": {"x": 0, "y": 0}},
               "b": {"point": {"x": "4mm", "y": 0}}, "rotation": 0}),
    );
    engine.run_ok(
        "pcb.rotate",
        json!({"objects": [free], "angle": "90deg", "center": {"x": 0, "y": 0}}),
    );
    let d = dimension(&engine, free);
    assert_eq!(
        (d.b.point, d.rotation),
        (mm(0.0, 4.0), Millideg::from_deg(90))
    );
    engine.run_ok(
        "pcb.flip",
        json!({"objects": [free], "center": {"x": "1mm", "y": 0}}),
    );
    let d = dimension(&engine, free);
    assert_eq!(d.a.point, mm(2.0, 0.0));
    assert_eq!(
        d.rotation,
        Millideg::from_deg(90),
        "a vertical direction stays vertical"
    );
    assert_eq!(
        d.offset,
        Nm::from_mm(-2),
        "the line stays on the mirrored side"
    );
}

#[test]
fn dimensions_are_kept_in_the_board_file_with_their_refs() {
    let dir = TempDir::new("dimension-save");
    let mut engine = engine(&dir);
    let pad = free_pad(&mut engine, "0mm", "0mm");
    let uuid = add(
        &mut engine,
        json!({"layer": "mechanical", "a": {"ref": {"object": pad, "hotspot": "center"}},
               "b": {"point": {"x": "10mm", "y": 0}}, "prefix": "L=",
               "text_position": {"manual": {"x": "5mm", "y": "4mm"}}}),
    );
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let file = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "redapcb"))
        .unwrap();
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains("\"dimensions\": ["), "{text}");
    assert!(text.contains("\"ref\":{\"object\":"), "{text}");

    let saved = dimension(&engine, uuid).clone();
    let mut reopened = reda_testkit::test_engine();
    reopened.run_ok("project.open", json!({"path": folder}));
    assert_eq!(pcb(&reopened).dimension(uuid), Some(&saved));
}
