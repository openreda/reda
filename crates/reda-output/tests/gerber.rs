//! Gerber X2 plotting: the files read back (with a reader of our own) to the model's
//! geometry, and byte-exact goldens.

mod gerber_reader;

use gerber_reader::{Object, read};
use reda_command::Timestamp;
use reda_model::Uuid;
use reda_output::{OutputFile, Plot, gerber_files};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn mm(v: f64) -> i64 {
    (v * 1_000_000.0).round() as i64
}

fn pt(x: f64, y: f64) -> Value {
    json!({"x": mm(x), "y": mm(y)})
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine
}

fn net(engine: &mut TestEngine, name: &str) -> Value {
    engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"].clone()
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
        fonts: &reda_command::Fonts::default(),
        created: Timestamp(1_767_225_600_000),
    })
}

fn file<'a>(files: &'a [OutputFile], name: &str) -> &'a OutputFile {
    files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no {name} in {:?}", names(files)))
}

fn names(files: &[OutputFile]) -> Vec<&str> {
    files.iter().map(|f| f.name.as_str()).collect()
}

#[test]
fn the_files_are_named_the_protel_way() {
    let dir = TempDir::new("gerber-names");
    let engine = board(&dir);
    let files = plot(&engine);
    assert_eq!(
        names(&files),
        [
            "Demo.GTL", "Demo.GBL", "Demo.GTS", "Demo.GBS", "Demo.GTP", "Demo.GBP", "Demo.GTO",
            "Demo.GBO", "Demo.GKO"
        ]
    );
    let top = read(&file(&files, "Demo.GTL").bytes);
    assert!(top.ended);
    assert_eq!(top.file[".FileFunction"], "Copper,L1,Top");
    assert_eq!(top.file[".FilePolarity"], "Positive");
    assert_eq!(top.file[".CreationDate"], "2026-01-01T00:00:00+00:00");
    let bottom = read(&file(&files, "Demo.GBL").bytes);
    assert_eq!(bottom.file[".FileFunction"], "Copper,L2,Bot");
    let profile = read(&file(&files, "Demo.GKO").bytes);
    assert_eq!(profile.file[".FileFunction"], "Profile,NP");
}

#[test]
fn tracks_read_back_as_draws_with_their_width_and_net() {
    let dir = TempDir::new("gerber-tracks");
    let mut engine = board(&dir);
    let gnd = net(&mut engine, "GND");
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_bottom", "net": gnd, "width": "0.25mm",
               "points": [pt(2.0, 2.0), pt(10.0, 2.0), pt(12.5, 4.5)]}),
    );
    let files = plot(&engine);
    let top = read(&file(&files, "Demo.GTL").bytes);
    assert_eq!(top.draws().count(), 0);
    let bottom = read(&file(&files, "Demo.GBL").bytes);
    let mut draws: Vec<_> = bottom
        .draws()
        .map(|d| match d {
            Object::Draw {
                from,
                to,
                arc,
                aperture,
                function,
                attributes,
            } => {
                assert_eq!(*arc, None);
                assert_eq!(*aperture, gerber_reader::Aperture::Circle(250_000.0));
                assert_eq!(function.as_deref(), Some("Conductor"));
                assert_eq!(attributes[".N"], "GND");
                (*from, *to)
            }
            _ => unreachable!(),
        })
        .collect();
    draws.sort();
    let p = |x: f64, y: f64| (mm(x), mm(y));
    let mut expected = vec![(p(2.0, 2.0), p(10.0, 2.0)), (p(10.0, 2.0), p(12.5, 4.5))];
    expected.sort();
    assert_eq!(draws, expected);
    let _ = uuid;
}

/// Probe points in a Pad's own frame (x along its width) and whether each lies on the Pad:
/// just inside and just outside each side, and near a corner.
fn probes(pad: &reda_pcb::placed::PlacedPad) -> Vec<((f64, f64), bool)> {
    use reda_model::PadShape;
    let (w, h) = (pad.width.0 as f64 / 2.0, pad.height.0 as f64 / 2.0);
    let d = 2_000.0;
    let mut out = vec![
        ((0.0, 0.0), true),
        ((w - d, 0.0), true),
        ((-(w - d), 0.0), true),
        ((0.0, h - d), true),
        ((0.0, -(h - d)), true),
        ((w + d, 0.0), false),
        ((-(w + d), 0.0), false),
        ((0.0, h + d), false),
        ((0.0, -(h + d)), false),
    ];
    let corner = (w - d, h - d);
    let r = match pad.shape {
        PadShape::Rect => 0.0,
        PadShape::RoundedRect => pad.corner_radius.map_or(0.0, |r| r.0 as f64),
        PadShape::Circle | PadShape::Oval => w.min(h),
    };
    // The corner is on the Pad only when the rounding leaves it.
    let (cx, cy) = (w - r, h - r);
    let in_corner = r == 0.0 || (corner.0 - cx).max(0.0).hypot((corner.1 - cy).max(0.0)) <= r;
    out.push((corner, in_corner));
    out
}

fn flash_at(layer: &gerber_reader::Layer, at: (i64, i64)) -> &Object {
    layer
        .flashes()
        .find(|f| matches!(f, Object::Flash { at: a, .. } if *a == at))
        .unwrap_or_else(|| panic!("no flash at {at:?}"))
}

#[test]
fn pads_read_back_with_their_shape_turn_and_side() {
    let dir = TempDir::new("gerber-pads");
    let mut engine = board(&dir);
    let gnd = net(&mut engine, "GND");
    let pad = |x: f64, shape: &str, w: f64, h: f64, extra: Value| {
        let mut p = json!({"at": pt(x, 0.0), "width": format!("{w}mm"),
                           "height": format!("{h}mm"), "shape": shape});
        p.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        p
    };
    for (at, rotation, side) in [
        (pt(5.0, 5.0), "0deg", "top"),
        (pt(15.0, 10.0), "30deg", "top"),
        (pt(20.0, 15.0), "90deg", "bottom"),
    ] {
        engine.run_ok(
            "test.footprint.add",
            json!({"at": at, "rotation": rotation, "side": side, "pads": [
                pad(0.0, "rect", 1.2, 0.6, json!({"net": gnd})),
                pad(2.0, "rounded_rect", 1.0, 1.6, json!({"corner_radius": "0.25mm"})),
                pad(4.0, "oval", 1.8, 0.9, json!({"rotation": "15deg"})),
                pad(6.0, "oval", 0.7, 1.5, json!({})),
                pad(8.0, "circle", 0.8, 0.8, json!({})),
            ]}),
        );
    }
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "circle", "at": pt(25.0, 5.0),
               "width": "1.6mm", "hole": {"width": "0.8mm"}, "net": gnd}),
    );
    let files = plot(&engine);
    let top = read(&file(&files, "Demo.GTL").bytes);
    let bottom = read(&file(&files, "Demo.GBL").bytes);

    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let mut pads: Vec<_> = pcb
        .footprints()
        .flat_map(|f| reda_pcb::placed::placed_pads(pcb, f))
        .collect();
    pads.extend(pcb.free_pads().map(reda_pcb::placed::placed_free_pad));
    let mut expected = (0, 0);
    for pad in &pads {
        let layers: Vec<&gerber_reader::Layer> = match (pad.kind, pad.side) {
            (reda_model::PadKind::Smd, Some(reda_model::Side::Bottom)) => vec![&bottom],
            (reda_model::PadKind::Smd, _) => vec![&top],
            _ => vec![&top, &bottom],
        };
        for layer in layers {
            if std::ptr::eq(layer, &top) {
                expected.0 += 1;
            } else {
                expected.1 += 1;
            }
            let Object::Flash {
                aperture,
                function,
                attributes,
                ..
            } = flash_at(layer, (pad.at.x.0, pad.at.y.0))
            else {
                unreachable!()
            };
            let smd = pad.kind == reda_model::PadKind::Smd;
            assert_eq!(
                function.as_deref(),
                Some(if smd { "SMDPad,CuDef" } else { "ComponentPad" })
            );
            if pad.net.is_some() {
                assert_eq!(attributes[".N"], "GND");
            }
            let angle = (pad.rotation.0 as f64 / 1000.0).to_radians();
            let (s, c) = angle.sin_cos();
            for ((x, y), inside) in probes(pad) {
                let (dx, dy) = (x * c - y * s, x * s + y * c);
                assert_eq!(
                    aperture.contains(dx, dy),
                    inside,
                    "{:?} pad {} at {:?} probe ({x}, {y}) via {aperture:?}",
                    pad.shape,
                    pad.number,
                    pad.at
                );
            }
        }
    }
    assert_eq!(top.flashes().count(), expected.0);
    assert_eq!(bottom.flashes().count(), expected.1);
}

#[test]
fn vias_flash_on_every_layer_they_span() {
    let dir = TempDir::new("gerber-vias");
    let mut engine = board(&dir);
    let gnd = net(&mut engine, "GND");
    engine.run_ok(
        "pcb.via.add",
        json!({"at": pt(7.0, 8.0), "net": gnd, "diameter": "0.6mm", "hole": "0.3mm"}),
    );
    let files = plot(&engine);
    for name in ["Demo.GTL", "Demo.GBL"] {
        let layer = read(&file(&files, name).bytes);
        let Object::Flash {
            aperture,
            function,
            attributes,
            ..
        } = flash_at(&layer, (mm(7.0), mm(8.0)))
        else {
            unreachable!()
        };
        assert_eq!(*aperture, gerber_reader::Aperture::Circle(600_000.0));
        assert_eq!(function.as_deref(), Some("ViaPad"));
        assert_eq!(attributes[".N"], "GND");
    }
}

/// A read region: dark or clear, its closed ring and its Net.
type Region = (bool, Vec<(i64, i64)>, Option<String>);

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!([pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)])
}

#[test]
fn zone_fills_read_back_as_regions_with_their_holes_cleared() {
    let dir = TempDir::new("gerber-zones");
    let mut engine = board(&dir);
    let gnd = net(&mut engine, "GND");
    let sig = net(&mut engine, "SIG");
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "pads": [
            {"at": pt(0.0, 0.0), "width": "1mm", "height": "1mm", "net": sig},
            {"at": pt(5.0, 0.0), "width": "1mm", "height": "1mm", "net": gnd}]}),
    );
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd, "vertices": rect(1.0, 1.0, 29.0, 19.0)}),
    );
    // A SIG Zone inside the hole the GND fill leaves round the SIG Zone.
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": sig, "priority": 1,
               "vertices": rect(8.0, 8.0, 12.0, 12.0)}),
    );
    let files = plot(&engine);
    let top = read(&file(&files, "Demo.GTL").bytes);
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let closed = |ring: &[reda_model::geom::Point]| -> Vec<(i64, i64)> {
        let mut v: Vec<_> = ring.iter().map(|p| (p.x.0, p.y.0)).collect();
        v.push(v[0]);
        v
    };
    let regions: Vec<Region> = top
        .regions()
        .map(|r| match r {
            Object::Region {
                dark,
                points,
                attributes,
            } => (*dark, points.clone(), attributes.get(".N").cloned()),
            _ => unreachable!(),
        })
        .collect();
    let mut expected = Vec::new();
    for zone in pcb.zones() {
        let name = pcb.net(zone.net.unwrap()).unwrap().name.clone();
        for shape in &fills.zones[&zone.uuid] {
            expected.push((true, closed(&shape.outer), Some(name.clone())));
            for hole in &shape.holes {
                expected.push((false, closed(hole), None));
            }
        }
    }
    let mut sorted = regions.clone();
    sorted.sort();
    expected.sort();
    assert_eq!(sorted, expected);
    // The SIG fill sits in a hole of the GND fill, so it is drawn after that hole is
    // cleared.
    let sig_fill = regions
        .iter()
        .position(|r| r.2.as_deref() == Some("SIG"))
        .unwrap();
    let first_clear = regions.iter().position(|r| !r.0).unwrap();
    assert!(sig_fill > first_clear, "{regions:?}");
}

#[test]
fn the_profile_follows_the_board_outline_arcs_included() {
    let dir = TempDir::new("gerber-profile");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo",
               "board": {"width": "30mm", "height": "20mm", "corner_radius": "2mm"}}),
    );
    let files = plot(&engine);
    let profile = read(&file(&files, "Demo.GKO").bytes);
    let outline = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .outline()
        .unwrap()
        .vertices
        .clone();
    let draws: Vec<_> = profile.draws().collect();
    assert_eq!(draws.len(), outline.len());
    for (i, (v, draw)) in outline.iter().zip(draws).enumerate() {
        let next = outline[(i + 1) % outline.len()];
        let Object::Draw {
            from,
            to,
            arc,
            function,
            ..
        } = draw
        else {
            unreachable!()
        };
        assert_eq!(function.as_deref(), Some("Profile"));
        assert_eq!(*from, (v.x.0, v.y.0));
        assert_eq!(*to, (next.x.0, next.y.0));
        match arc {
            None => assert_eq!(v.sweep.0, 0),
            Some(arc) => {
                assert_eq!(arc.ccw, v.sweep.0 > 0);
                let r = |p: (i64, i64)| {
                    ((p.0 - arc.centre.0) as f64).hypot((p.1 - arc.centre.1) as f64)
                };
                assert!((r(*from) - 2_000_000.0).abs() < 1.0, "{arc:?}");
                assert!((r(*to) - 2_000_000.0).abs() < 1.0, "{arc:?}");
            }
        }
    }
}

/// A small board with every kind of copper this plot draws.
fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo",
               "board": {"width": "30mm", "height": "20mm", "corner_radius": "1mm"}}),
    );
    engine.run_ok("project.properties.set", json!({"revision": "A, first"}));
    let gnd = net(&mut engine, "GND");
    let sig = net(&mut engine, "SIG");
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(8.0, 10.0), "rotation": "45deg", "designator": "C1", "pads": [
            {"at": pt(-1.0, 0.0), "width": "1mm", "height": "0.6mm", "net": sig,
             "shape": "rounded_rect", "corner_radius": "0.1mm", "mask_margin": "0.05mm",
             "paste_margin": "-0.025mm"},
            {"at": pt(1.0, 0.0), "width": "1mm", "height": "0.6mm", "net": gnd}],
            "graphics": [
                {"layer": "silkscreen_top", "width": "0.12mm",
                 "shape": {"kind": "line", "start": pt(-1.8, 0.6), "end": pt(1.8, 0.6)}},
                {"layer": "silkscreen_top", "width": "0.12mm",
                 "shape": {"kind": "arc", "center": pt(-1.8, 0.0), "start": pt(-1.8, 0.6),
                           "sweep": 180000}}]}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "REDA", "x": "3mm", "y": "16mm"}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_bottom", "text": "B", "x": "25mm", "y": "3mm",
               "font": "Noto Sans"}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "copper_top", "text": "v1", "x": "22mm", "y": "16mm"}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(20.0, 10.0), "side": "bottom", "pads": [
            {"at": pt(0.0, 0.0), "width": "1.5mm", "height": "0.8mm", "shape": "oval",
             "net": sig}]}),
    );
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": sig, "width": "0.2mm",
               "points": [pt(7.293, 9.293), pt(7.0, 6.0), pt(15.0, 6.0)]}),
    );
    engine.run_ok("pcb.via.add", json!({"at": pt(15.0, 6.0), "net": sig}));
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_bottom", "net": sig, "width": "0.2mm",
               "points": [pt(15.0, 6.0), pt(20.0, 10.0)]}),
    );
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_bottom", "net": gnd, "vertices": rect(2.0, 2.0, 28.0, 18.0)}),
    );
    engine.run_ok("pcb.via.add", json!({"at": pt(25.0, 15.0), "net": gnd}));
    engine
}

#[test]
fn the_files_match_their_goldens_byte_for_byte() {
    let dir = TempDir::new("gerber-golden");
    let engine = demo(&dir);
    for f in plot(&engine) {
        reda_testkit::assert_golden(
            format!(
                "{}/tests/golden/gerber/{}",
                env!("CARGO_MANIFEST_DIR"),
                f.name
            ),
            &f.bytes,
        );
    }
}

#[test]
fn without_x2_attributes_the_files_are_plain_x1() {
    let dir = TempDir::new("gerber-x1");
    let engine = demo(&dir);
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, None);
    let files = gerber_files(&Plot {
        project,
        pcb,
        fills: &fills,
        gerber: reda_model::GerberSettings {
            x2_attributes: false,
            naming: reda_model::FileNaming::Protel,
        },
        fonts: &reda_command::Fonts::default(),
        created: Timestamp(0),
    });
    let with = plot(&engine);
    for (x1, x2) in files.iter().zip(&with) {
        let text = String::from_utf8(x1.bytes.clone()).unwrap();
        for attribute in ["%TF", "%TA", "%TO", "%TD"] {
            assert!(!text.contains(attribute), "{} has {attribute}", x1.name);
        }
        let (a, b) = (read(&x1.bytes), read(&x2.bytes));
        assert!(a.file.is_empty());
        assert_eq!(a.objects.len(), b.objects.len(), "{}", x1.name);
    }
}

#[test]
fn the_zip_holds_every_file_stamped_with_the_clock() {
    let dir = TempDir::new("gerber-zip");
    let engine = demo(&dir);
    let files = plot(&engine);
    let created = Timestamp(1_767_225_600_000 + 3_723_000);
    let bytes = reda_output::zip(&files, created);
    assert_eq!(
        bytes,
        reda_output::zip(&files, created),
        "the same every time"
    );
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(archive.len(), files.len());
    for (i, f) in files.iter().enumerate() {
        let mut entry = archive.by_index(i).unwrap();
        assert_eq!(entry.name().unwrap(), f.name);
        let stamp = entry.last_modified().unwrap();
        assert_eq!((stamp.year(), stamp.month(), stamp.day()), (2026, 1, 1));
        assert_eq!((stamp.hour(), stamp.minute(), stamp.second()), (1, 2, 2));
        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut content).unwrap();
        assert_eq!(content, f.bytes);
    }
}
