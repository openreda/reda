//! Gerber Solder Mask, Paste and Silkscreen: the files read back (with the reader of our
//! own) to the model's geometry.

mod gerber_reader;

use gerber_reader::{Aperture, Layer, Object, read};
use reda_command::{Fonts, Timestamp};
use reda_model::geom::Nm;
use reda_model::{LayerKind, PadKind, PcbText, Side, Uuid};
use reda_output::{OutputFile, Plot, gerber_files};
use reda_pcb::placed::{PlacedPad, placed_free_pad, placed_pads};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn mm(v: f64) -> i64 {
    (v * 1_000_000.0).round() as i64
}

fn pt(x: f64, y: f64) -> Value {
    json!({"x": mm(x), "y": mm(y)})
}

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine
}

fn plot(engine: &TestEngine) -> Vec<OutputFile> {
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let preset = project.fab_presets().next().unwrap().preset();
    gerber_files(&Plot {
        project,
        pcb,
        fills: &fills,
        gerber: preset.gerber,
        fonts: &Fonts::default(),
        created: Timestamp(1_767_225_600_000),
    })
}

fn layer(files: &[OutputFile], name: &str) -> Layer {
    let file = files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no {name}"));
    let layer = read(&file.bytes);
    assert!(layer.ended, "{name} ends");
    layer
}

/// Every placed Pad of the board, Footprints' then Free Pads.
fn pads(engine: &TestEngine) -> Vec<PlacedPad> {
    let pcb = engine.project().unwrap().pcb().unwrap();
    let mut pads: Vec<_> = pcb.footprints().flat_map(|f| placed_pads(pcb, f)).collect();
    pads.extend(pcb.free_pads().map(placed_free_pad));
    pads
}

fn flash_at(layer: &Layer, at: (i64, i64)) -> Option<(&Aperture, Option<&str>)> {
    layer.flashes().find_map(|f| match f {
        Object::Flash {
            at: a,
            aperture,
            function,
            ..
        } if *a == at => Some((aperture, function.as_deref())),
        _ => None,
    })
}

/// Checks the flashed aperture is the Pad grown by `margin` on every side: probes just
/// inside and just outside each side, in the Pad's own frame.
fn assert_grown(pad: &PlacedPad, aperture: &Aperture, margin: i64) {
    let (w, h) = (
        (pad.width.0 + 2 * margin) as f64 / 2.0,
        (pad.height.0 + 2 * margin) as f64 / 2.0,
    );
    let d = 2_000.0;
    let angle = (pad.rotation.0 as f64 / 1000.0).to_radians();
    let (s, c) = angle.sin_cos();
    for ((x, y), inside) in [
        ((0.0, 0.0), true),
        ((w - d, 0.0), true),
        ((-(w - d), 0.0), true),
        ((0.0, h - d), true),
        ((0.0, -(h - d)), true),
        ((w + d, 0.0), false),
        ((-(w + d), 0.0), false),
        ((0.0, h + d), false),
        ((0.0, -(h + d)), false),
    ] {
        let (dx, dy) = (x * c - y * s, x * s + y * c);
        assert_eq!(
            aperture.contains(dx, dy),
            inside,
            "pad {} at {:?} grown by {margin}, probe ({x}, {y}) via {aperture:?}",
            pad.number,
            pad.at
        );
    }
}

fn at(pad: &PlacedPad) -> (i64, i64) {
    (pad.at.x.0, pad.at.y.0)
}

#[test]
fn the_solder_mask_opens_over_every_pad_by_its_margin_and_tents_vias() {
    let dir = TempDir::new("gerber-mask");
    let mut engine = board(&dir);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(5.0, 5.0), "rotation": "30deg", "pads": [
            {"at": pt(0.0, 0.0), "width": "1.2mm", "height": "0.6mm", "mask_margin": "0.1mm"},
            {"at": pt(2.0, 0.0), "width": "1mm", "height": "1.6mm", "shape": "rounded_rect",
             "corner_radius": "0.25mm", "mask_margin": "-0.05mm"},
            {"at": pt(4.0, 0.0), "width": "1.6mm", "height": "1.6mm", "shape": "circle",
             "kind": "plated_through_hole", "hole": {"width": 800000, "height": 800000}}]}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(20.0, 15.0), "side": "bottom", "pads": [
            {"at": pt(0.0, 0.0), "width": "1.5mm", "height": "0.8mm", "shape": "oval",
             "mask_margin": "0.05mm"}]}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "circle", "at": pt(25.0, 5.0),
               "width": "2mm", "hole": {"width": "2mm"}}),
    );
    engine.run_ok("pcb.via.add", json!({"at": pt(12.0, 12.0)}));
    let files = plot(&engine);
    let top = layer(&files, "Demo.GTS");
    let bottom = layer(&files, "Demo.GBS");
    assert_eq!(top.file[".FileFunction"], "Soldermask,Top");
    assert_eq!(bottom.file[".FileFunction"], "Soldermask,Bot");
    // The image is the openings.
    assert_eq!(top.file[".FilePolarity"], "Negative");

    let mut expected = (0, 0);
    for pad in pads(&engine) {
        let on: &[&Layer] = match (pad.kind, pad.side) {
            (PadKind::Smd, Some(Side::Bottom)) => &[&bottom],
            (PadKind::Smd, _) => &[&top],
            _ => &[&top, &bottom],
        };
        for layer in on {
            if std::ptr::eq(*layer, &top) {
                expected.0 += 1;
            } else {
                expected.1 += 1;
            }
            let (aperture, _) = flash_at(layer, at(&pad))
                .unwrap_or_else(|| panic!("no mask opening for pad at {:?}", pad.at));
            assert_grown(&pad, aperture, pad.mask_margin.unwrap_or(Nm::ZERO).0);
        }
    }
    assert_eq!(top.flashes().count(), expected.0, "the Via is tented");
    assert_eq!(bottom.flashes().count(), expected.1, "the Via is tented");
}

#[test]
fn the_paste_opens_over_smd_pads_by_their_margin_only() {
    let dir = TempDir::new("gerber-paste");
    let mut engine = board(&dir);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(5.0, 5.0), "rotation": "90deg", "pads": [
            {"at": pt(0.0, 0.0), "width": "1.2mm", "height": "0.6mm", "paste_margin": "-0.1mm"},
            {"at": pt(2.0, 0.0), "width": "1mm", "height": "1mm", "mask_margin": "0.2mm"},
            {"at": pt(4.0, 0.0), "width": "1.6mm", "height": "1.6mm", "shape": "circle",
             "kind": "plated_through_hole", "hole": {"width": 800000, "height": 800000}},
            // Shrunk to nothing: no aperture at all.
            {"at": pt(6.0, 0.0), "width": "0.2mm", "height": "0.2mm", "paste_margin": "-0.1mm"}]}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(20.0, 15.0), "side": "bottom", "pads": [
            {"at": pt(0.0, 0.0), "width": "1.5mm", "height": "0.8mm", "shape": "oval"}]}),
    );
    let files = plot(&engine);
    let top = layer(&files, "Demo.GTP");
    let bottom = layer(&files, "Demo.GBP");
    assert_eq!(top.file[".FileFunction"], "Paste,Top");
    assert_eq!(bottom.file[".FileFunction"], "Paste,Bot");
    assert_eq!(top.file[".FilePolarity"], "Positive");

    assert_eq!(top.flashes().count(), 2, "two SMD Pads, one shrunk away");
    assert_eq!(bottom.flashes().count(), 1);
    for pad in pads(&engine).iter().filter(|p| p.kind == PadKind::Smd) {
        let layer = match pad.side {
            Some(Side::Bottom) => &bottom,
            _ => &top,
        };
        let margin = pad.paste_margin.unwrap_or(Nm::ZERO).0;
        if pad.width.0 + 2 * margin <= 0 {
            assert!(flash_at(layer, at(pad)).is_none());
            continue;
        }
        let (aperture, function) = flash_at(layer, at(pad)).unwrap();
        assert_eq!(function, Some("SMDPad,CuDef"));
        assert_grown(pad, aperture, margin);
    }
}

type Draw = ((i64, i64), (i64, i64), i64);

/// The draws of a file as `(from, to, pen)`, sorted.
fn draws(layer: &Layer) -> Vec<Draw> {
    let mut out: Vec<_> = layer
        .draws()
        .map(|d| match d {
            Object::Draw {
                from,
                to,
                aperture: Aperture::Circle(pen),
                function,
                ..
            } => {
                assert_eq!(*function, None, "silkscreen takes no aperture function");
                (*from, *to, *pen as i64)
            }
            other => panic!("not a round-pen draw: {other:?}"),
        })
        .collect();
    out.sort();
    out
}

/// The strokes of `texts` as the canvas lays them out, as `(from, to, pen)`.
fn strokes(texts: &[PcbText]) -> Vec<Draw> {
    texts
        .iter()
        .flat_map(|t| {
            let laid = reda_pcb::text_geometry(t, &Fonts::default());
            let pen = laid.stroke_width.0;
            laid.strokes
                .into_iter()
                .map(move |s| ((s.a.x.0, s.a.y.0), (s.b.x.0, s.b.y.0), pen))
        })
        .collect()
}

#[test]
fn the_silkscreen_holds_footprint_drawing_designators_and_free_text() {
    let dir = TempDir::new("gerber-silk");
    let mut engine = board(&dir);
    let top_fp = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "designator": "R7", "graphics": [
            {"layer": "silkscreen_top", "width": "0.15mm",
             "shape": {"kind": "line", "start": pt(-1.0, -1.0), "end": pt(1.0, -1.0)}},
            {"layer": "silkscreen_top", "width": "0.1mm",
             "shape": {"kind": "circle", "center": pt(2.0, 0.0), "radius": mm(0.5)}},
            {"layer": "courtyard_top", "width": "0.05mm",
             "shape": {"kind": "rect", "start": pt(-2.0, -2.0), "end": pt(2.0, 2.0)}}],
            "pads": [{"at": pt(0.0, 0.0), "width": "1mm", "height": "1mm"}]}),
    )["result"]["footprint"]
        .clone();
    let bottom_fp = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(20.0, 10.0), "side": "bottom", "designator": "R8",
               "pads": [{"at": pt(0.0, 0.0), "width": "1mm", "height": "1mm"}]}),
    )["result"]["footprint"]
        .clone();
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "Hi", "x": "3mm", "y": "15mm",
               "height": "1.5mm"}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_bottom", "text": "Rev", "x": "20mm", "y": "3mm",
               "font": "Noto Sans"}),
    );
    engine.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "silkscreen_bottom", "from": pt(1.0, 1.0), "to": pt(5.0, 1.0),
               "width": "0.2mm"}),
    );
    let files = plot(&engine);
    let top = layer(&files, "Demo.GTO");
    let bottom = layer(&files, "Demo.GBO");
    assert_eq!(top.file[".FileFunction"], "Legend,Top");
    assert_eq!(bottom.file[".FileFunction"], "Legend,Bot");
    assert_eq!(top.file[".FilePolarity"], "Positive");

    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fonts = Fonts::default();
    let texts_of = |v: &Value| {
        let uuid: Uuid = v.as_str().unwrap().parse().unwrap();
        let instance = pcb.footprints().find(|f| f.uuid == uuid).unwrap();
        reda_pcb::footprint_texts(pcb, instance, &fonts)
    };
    let (r7, r8) = (texts_of(&top_fp), texts_of(&bottom_fp));
    let free = |side: Side| -> Vec<PcbText> {
        let layer = pcb.layers().find(LayerKind::Silkscreen, Some(side));
        let layer = layer.unwrap().uuid;
        pcb.texts().filter(|t| t.layer == layer).cloned().collect()
    };

    // Top: the Footprint's line and circle (two half turns), R7 and "Hi"; no courtyard.
    let mut expected = vec![
        ((mm(9.0), mm(9.0)), (mm(11.0), mm(9.0)), mm(0.15)),
        ((mm(12.5), mm(10.0)), (mm(11.5), mm(10.0)), mm(0.1)),
        ((mm(11.5), mm(10.0)), (mm(12.5), mm(10.0)), mm(0.1)),
    ];
    expected.extend(strokes(&r7));
    expected.extend(strokes(&free(Side::Top)));
    expected.sort();
    assert_eq!(draws(&top), expected);
    assert_eq!(top.regions().count(), 0);

    // The Designator is centred on the Footprint.
    let r7_strokes = strokes(&r7);
    assert!(!r7_strokes.is_empty());
    let mid = |v: Vec<i64>| (v.iter().min().unwrap() + v.iter().max().unwrap()) / 2;
    let cx = mid(r7_strokes.iter().flat_map(|s| [s.0.0, s.1.0]).collect());
    let cy = mid(r7_strokes.iter().flat_map(|s| [s.0.1, s.1.1]).collect());
    assert!((cx - mm(10.0)).abs() < mm(0.05), "x centre {cx}");
    assert!((cy - mm(10.0)).abs() < mm(0.05), "y centre {cy}");

    // Bottom: the free line and R8 (flipped over with its Footprint) as strokes, the
    // outline-font text as filled triangles.
    let mut expected = vec![((mm(1.0), mm(1.0)), (mm(5.0), mm(1.0)), mm(0.2))];
    expected.extend(strokes(&r8));
    expected.sort();
    assert_eq!(draws(&bottom), expected);
    let rev = reda_pcb::text_geometry(&free(Side::Bottom)[0], &fonts);
    assert!(!rev.fill.indices.is_empty());
    assert_eq!(bottom.regions().count(), rev.fill.indices.len() / 3);
    for region in bottom.regions() {
        let Object::Region { dark, points, .. } = region else {
            unreachable!()
        };
        assert!(dark);
        assert_eq!(points.len(), 4, "a closed triangle");
    }
}

#[test]
fn the_silkscreen_is_cleared_over_the_solder_mask_openings_on_its_side() {
    let dir = TempDir::new("gerber-silk-clip");
    let mut engine = board(&dir);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "pads": [
            {"at": pt(0.0, 0.0), "width": "1mm", "height": "1mm", "mask_margin": "0.1mm"}],
            "graphics": [
            {"layer": "silkscreen_top", "width": "0.15mm",
             "shape": {"kind": "line", "start": pt(-2.0, 0.0), "end": pt(2.0, 0.0)}}]}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(20.0, 10.0), "side": "bottom", "pads": [
            {"at": pt(0.0, 0.0), "width": "1mm", "height": "1mm"}]}),
    );
    let files = plot(&engine);
    let top = layer(&files, "Demo.GTO");
    let pads = pads(&engine);
    let clears: Vec<_> = top
        .objects
        .iter()
        .filter_map(|o| match o {
            Object::Flash {
                dark: false,
                at,
                aperture,
                ..
            } => Some((*at, aperture.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(clears.len(), 1, "only the top Pad's opening: {clears:?}");
    let (at_, aperture) = &clears[0];
    assert_eq!(*at_, at(&pads[0]));
    assert_grown(&pads[0], aperture, 100_000);
    // The silkscreen is drawn first, then cleared.
    let last_draw = top
        .objects
        .iter()
        .rposition(|o| matches!(o, Object::Draw { .. }))
        .unwrap();
    let first_clear = top
        .objects
        .iter()
        .position(|o| matches!(o, Object::Flash { dark: false, .. }))
        .unwrap();
    assert!(last_draw < first_clear);
}

#[test]
fn copper_text_and_drawing_plot_on_their_copper_layer_as_non_conductor() {
    let dir = TempDir::new("gerber-copper-text");
    let mut engine = board(&dir);
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "copper_top", "text": "v2", "x": "3mm", "y": "3mm"}),
    );
    engine.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "copper_bottom", "from": pt(1.0, 1.0), "to": pt(5.0, 1.0),
               "width": "0.3mm"}),
    );
    let files = plot(&engine);
    let functions = |layer: &Layer| -> Vec<Option<String>> {
        layer
            .draws()
            .map(|d| match d {
                Object::Draw { function, .. } => function.clone(),
                _ => unreachable!(),
            })
            .collect()
    };
    let pcb = engine.project().unwrap().pcb().unwrap();
    let text = pcb.texts().next().unwrap();
    let top = functions(&layer(&files, "Demo.GTL"));
    assert_eq!(top.len(), strokes(std::slice::from_ref(text)).len());
    assert!(top.iter().all(|f| f.as_deref() == Some("NonConductor")));
    let bottom = functions(&layer(&files, "Demo.GBL"));
    assert_eq!(bottom, [Some("NonConductor".to_owned())]);
    let silk = layer(&files, "Demo.GTO");
    assert_eq!(silk.draws().count(), 0, "nothing on the silkscreen");
}

/// DRC's silkscreen check, the canvas and the Gerber silkscreen share one layout of
/// Footprint text ([`reda_pcb::footprint_texts`]): on a rotated Footprint flipped to the
/// bottom, the Gerber draws exactly that layout, and DRC finds the silkscreen right where
/// the Gerber draws it. Text in a Footprint's drawing is anchored at the start of its
/// baseline, the Designator centred on its placement.
#[test]
fn drc_and_the_gerber_silkscreen_agree_on_a_rotated_flipped_footprint() {
    let dir = TempDir::new("gerber-silk-drc");
    let mut engine = board(&dir);
    let footprint = engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(15.0, 10.0), "rotation": "30deg", "side": "bottom",
               "designator": "U9", "graphics": [
            {"layer": "silkscreen_top", "width": "0.1mm",
             "shape": {"kind": "text", "at": pt(1.0, 1.0), "text": "AB", "size": mm(1.0),
                       "rotation": 90_000}}],
            "pads": [{"at": pt(-4.0, -4.0), "width": "0.5mm", "height": "0.5mm"}]}),
    )["result"]["footprint"]
        .clone();
    let uuid: Uuid = footprint.as_str().unwrap().parse().unwrap();
    let files = plot(&engine);
    let bottom = draws(&layer(&files, "Demo.GBO"));

    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fonts = Fonts::default();
    let instance = pcb.footprints().find(|f| f.uuid == uuid).unwrap();
    let texts = reda_pcb::footprint_texts(pcb, instance, &fonts);
    assert_eq!(texts.len(), 2, "the drawing text and the Designator");
    let mut expected = strokes(&texts);
    expected.sort();
    assert_eq!(bottom, expected, "the Gerber draws the shared layout");

    // The drawing text starts at its placed anchor (the start of its baseline), not
    // centred on it.
    let drawing = &texts[0];
    assert!(drawing.mirrored);
    let frame = reda_pcb::placed::Frame::of(instance);
    let anchor = frame.point(reda_model::geom::Point::new(Nm(mm(1.0)), Nm(mm(1.0))));
    assert_eq!(drawing.position, anchor);

    // A small bottom Pad on the middle of each drawing-text stroke the Gerber draws: DRC
    // sees silkscreen over every one. Where the text would be had it been centred on its
    // anchor, clear of every stroke drawn, DRC sees none.
    let silk_hits = |engine: &mut TestEngine, at: (i64, i64)| {
        let pad = engine.run_ok(
            "pcb.free_pad.add",
            json!({"kind": "smd", "side": "bottom", "width": "0.05mm",
                   "at": {"x": at.0, "y": at.1}}),
        )["result"]["uuid"]
            .clone();
        let project = engine.project().unwrap();
        let pcb = project.pcb().unwrap();
        let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
        let hits = reda_pcb::drc::check(pcb, &fills, &Fonts::default())
            .into_iter()
            .filter(|v| v.check == "silkscreen_over_pads" && v.objects.contains(&uuid))
            .count();
        engine.run_ok("pcb.free_pad.delete", json!({"pads": [pad]}));
        hits
    };
    let mid = |(a, b, _): &Draw| ((a.0 + b.0) / 2, (a.1 + b.1) / 2);
    let drawn = strokes(std::slice::from_ref(drawing));
    for stroke in &drawn {
        assert_eq!(silk_hits(&mut engine, mid(stroke)), 1, "under {stroke:?}");
    }
    let xs = drawn.iter().flat_map(|(a, b, _)| [a.0, b.0]);
    let ys = drawn.iter().flat_map(|(a, b, _)| [a.1, b.1]);
    let centre = (
        (xs.clone().min().unwrap() + xs.max().unwrap()) / 2,
        (ys.clone().min().unwrap() + ys.max().unwrap()) / 2,
    );
    let shift = (anchor.x.0 - centre.0, anchor.y.0 - centre.1);
    let clear = |p: (i64, i64)| {
        expected
            .iter()
            .all(|s| distance(p, s.0, s.1) > (s.2 / 2 + mm(0.1)) as f64)
    };
    let centred: Vec<(i64, i64)> = drawn
        .iter()
        .map(mid)
        .map(|(x, y)| (x + shift.0, y + shift.1))
        .filter(|&p| clear(p))
        .collect();
    assert!(
        !centred.is_empty(),
        "centring would move the text clear of itself"
    );
    for p in centred {
        assert_eq!(
            silk_hits(&mut engine, p),
            0,
            "where centred text would be: {p:?}"
        );
    }
}

/// The distance from `p` to the segment `a`–`b`.
fn distance(p: (i64, i64), a: (i64, i64), b: (i64, i64)) -> f64 {
    let (px, py) = (p.0 as f64, p.1 as f64);
    let (ax, ay, bx, by) = (a.0 as f64, a.1 as f64, b.0 as f64, b.1 as f64);
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    };
    (px - ax - t * dx).hypot(py - ay - t * dy)
}

#[test]
fn every_copper_layer_gets_a_file_with_inner_copper_between_top_and_bottom() {
    let dir = TempDir::new("gerber-inner");
    let mut engine = board(&dir);
    engine.run_ok("project.save", json!({}));
    let path = dir.path().join("Demo/Demo.redapcb");
    let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let layers = file["layers"]["layers"].as_array_mut().unwrap();
    let bottom = layers
        .iter()
        .position(|l| l["kind"] == "copper" && l["side"] == "bottom")
        .unwrap();
    let inner = [
        "00000000-0000-4000-8000-0000000000a1",
        "00000000-0000-4000-8000-0000000000a2",
    ];
    for (i, uuid) in inner.iter().enumerate() {
        layers.insert(bottom + i, json!({"uuid": uuid, "kind": "copper"}));
    }
    let entries = file["stackups"][0]["entries"].as_array_mut().unwrap();
    for (i, uuid) in inner.iter().enumerate() {
        entries.insert(2 + 2 * i, json!({"layer": uuid, "thickness": 17_500}));
        entries.insert(
            3 + 2 * i,
            json!({"uuid": format!("00000000-0000-4000-8000-0000000000b{i}")}),
        );
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap()).unwrap();
    let mut engine = probe_engine();
    engine.run_ok("project.open", json!({"path": dir.path().join("Demo")}));

    let files = plot(&engine);
    let copper: Vec<(&str, &str)> = files
        .iter()
        .filter(|f| f.function.starts_with("copper"))
        .map(|f| (f.name.as_str(), f.function))
        .collect();
    assert_eq!(
        copper,
        [
            ("Demo.GTL", "copper_top"),
            ("Demo.G1", "copper_inner"),
            ("Demo.G2", "copper_inner"),
            ("Demo.GBL", "copper_bottom"),
        ]
    );
    for (name, function) in [
        ("Demo.GTL", "Copper,L1,Top"),
        ("Demo.G1", "Copper,L2,Inr"),
        ("Demo.G2", "Copper,L3,Inr"),
        ("Demo.GBL", "Copper,L4,Bot"),
    ] {
        let file = files.iter().find(|f| f.name == name).unwrap();
        let text = String::from_utf8_lossy(&file.bytes);
        assert!(
            text.contains(&format!("%TF.FileFunction,{function}*%")),
            "{name}: {text}"
        );
    }
}
