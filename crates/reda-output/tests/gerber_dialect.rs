//! REDA's one Gerber dialect (outputs.md, "Gerber"): the fixed format, the file and object
//! attributes, the aperture functions, the geometry the format can't carry, the `G04 #@!`
//! form with X2 off, which files are always written, and text as whole regions. Every
//! file is read back with the tests' own reader.

mod gerber_reader;

use gerber_reader::{Layer, Object, read, winding};
use reda_command::Fonts;
use reda_model::{FileNaming, GerberSettings, LayerFileName};
use reda_output::{CoordinateOutOfRange, Gerbers, OutputFile, Plot, gerber_files};
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

fn net(engine: &mut TestEngine, name: &str) -> Value {
    engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"].clone()
}

fn plot_with(
    engine: &TestEngine,
    settings: impl FnOnce(&mut GerberSettings),
) -> Result<Gerbers, CoordinateOutOfRange> {
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let mut gerber = project.fab_presets().next().unwrap().preset().gerber;
    settings(&mut gerber);
    gerber_files(&Plot {
        project,
        instances: &[reda_output::BoardInstance::single(
            pcb,
            &fills,
            reda_model::FileOrigin::Absolute,
        )],
        gerber: &gerber,
        fonts: &Fonts::default(),
        version: reda_testkit::TEST_VERSION,
        part: reda_output::Part::Single,
        not_fitted_paste: reda_model::NotFittedPaste::Keep,
    })
}

fn plot(engine: &TestEngine) -> Vec<OutputFile> {
    plot_with(engine, |_| {}).unwrap().files
}

fn layer(files: &[OutputFile], name: &str) -> Layer {
    read(&files.iter().find(|f| f.name == name).unwrap().bytes)
}

fn function(o: &Object) -> Option<&str> {
    match o {
        Object::Flash { function, .. }
        | Object::Draw { function, .. }
        | Object::Region { function, .. } => function.as_deref(),
    }
}

fn attributes(o: &Object) -> &std::collections::BTreeMap<String, String> {
    match o {
        Object::Flash { attributes, .. }
        | Object::Draw { attributes, .. }
        | Object::Region { attributes, .. } => attributes,
    }
}

/// A board with one of each kind of object the aperture table names.
fn every_kind(dir: &TempDir) -> TestEngine {
    let mut engine = board(dir);
    let gnd = net(&mut engine, "GND");
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(8.0, 10.0), "designator": "U1", "pads": [
            {"at": pt(-1.0, 0.0), "width": "1mm", "height": "0.6mm", "net": gnd},
            // The opening shrinks inside the copper on every side: mask-defined.
            {"at": pt(1.0, 0.0), "width": "1mm", "height": "0.6mm", "mask_margin": "-0.05mm"},
            {"at": pt(3.0, 0.0), "width": "1.6mm", "height": "1.6mm", "kind": "plated_through_hole",
             "hole": {"width": 800_000, "height": 800_000}, "net": gnd}],
            "graphics": [{"layer": "silkscreen_top", "width": "0.12mm",
                          "shape": {"kind": "line", "start": pt(-2.0, 1.0), "end": pt(4.0, 1.0)}}]}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "shape": "circle", "at": pt(25.0, 5.0), "width": "1mm"}),
    );
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": gnd, "width": "0.25mm",
               "points": [pt(7.0, 10.0), pt(7.0, 5.0)]}),
    );
    engine.run_ok("pcb.via.add", json!({"at": pt(7.0, 5.0), "net": gnd}));
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_bottom", "net": gnd,
               "vertices": [pt(2.0, 2.0), pt(28.0, 2.0), pt(28.0, 18.0), pt(2.0, 18.0)]}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "copper_top", "text": "v1", "x": "22mm", "y": "16mm"}),
    );
    engine
}

#[test]
fn every_file_is_millimetres_in_four_six_absolute_with_x_and_y_on_every_command() {
    let dir = TempDir::new("gerber-format");
    let engine = every_kind(&dir);
    for file in plot(&engine) {
        // The reader refuses an operation missing X or Y.
        let read = read(&file.bytes);
        assert_eq!(read.format, "LAX46Y46", "{}", file.name);
        assert_eq!(read.unit, "MM", "{}", file.name);
        assert!(read.ended, "{}", file.name);
        let text = String::from_utf8(file.bytes).unwrap();
        for legacy in [
            "G90", "G91", "G70", "G71", "%IN", "%IP", "%AS", "%MI", "%SF", "%OF",
        ] {
            assert!(!text.contains(legacy), "{} has {legacy}", file.name);
        }
    }
}

#[test]
fn file_attributes_name_the_version_and_the_part_and_no_date() {
    let dir = TempDir::new("gerber-file-attributes");
    let engine = every_kind(&dir);
    for file in plot(&engine) {
        let read = read(&file.bytes);
        assert_eq!(
            read.file[".GenerationSoftware"], "openreda,REDA,0.0.0-test",
            "{}",
            file.name
        );
        assert_eq!(read.file[".Part"], "Single", "{}", file.name);
        assert!(!read.file.contains_key(".CreationDate"), "{}", file.name);
        for name in [
            ".ProjectId",
            ".SameCoordinates",
            ".FileFunction",
            ".FilePolarity",
        ] {
            assert!(read.file.contains_key(name), "{} lacks {name}", file.name);
        }
    }
}

#[test]
fn every_copper_object_has_a_net_attribute_empty_without_a_net() {
    let dir = TempDir::new("gerber-net-attribute");
    let engine = every_kind(&dir);
    let files = plot(&engine);
    for name in ["Demo.gtl", "Demo.gbl"] {
        let copper = layer(&files, name);
        let mut empty = 0;
        for object in &copper.objects {
            // Clear regions remove copper; they are not copper objects themselves.
            if matches!(object, Object::Region { dark: false, .. }) {
                continue;
            }
            let n = attributes(object)
                .get(".N")
                .unwrap_or_else(|| panic!("{name}: no .N on {object:?}"));
            empty += usize::from(n.is_empty());
        }
        if name == "Demo.gtl" {
            // The second Pad, the Free Pad and the copper text "v1" have no Net.
            assert!(empty >= 3, "{name}: {empty} objects with an empty .N");
        }
    }
}

#[test]
fn aperture_functions_follow_the_table() {
    let dir = TempDir::new("gerber-aperture-functions");
    let engine = every_kind(&dir);
    let files = plot(&engine);
    let top = layer(&files, "Demo.gtl");
    let at = |layer: &Layer, x: f64, y: f64| -> Option<String> {
        layer.flashes().find_map(|o| match o {
            Object::Flash { at, function, .. } if *at == (mm(x), mm(y)) => function.clone(),
            _ => None,
        })
    };
    assert_eq!(at(&top, 7.0, 10.0).as_deref(), Some("SMDPad,CuDef"));
    assert_eq!(at(&top, 9.0, 10.0).as_deref(), Some("SMDPad,SMDef"));
    assert_eq!(at(&top, 11.0, 10.0).as_deref(), Some("ComponentPad"));
    assert_eq!(at(&top, 7.0, 5.0).as_deref(), Some("ViaPad"));
    assert_eq!(at(&top, 25.0, 5.0).as_deref(), Some("OtherPad,FreePad"));
    let conductor = top
        .draws()
        .filter(|o| attributes(o).get(".N").is_some_and(|n| n == "GND"))
        .map(function)
        .collect::<Vec<_>>();
    assert_eq!(conductor, [Some("Conductor")], "the Track");
    let text: Vec<_> = top
        .draws()
        .filter(|o| attributes(o)[".N"].is_empty())
        .map(function)
        .collect();
    assert!(!text.is_empty() && text.iter().all(|f| *f == Some("NonConductor")));
    assert!(
        top.flashes().all(|o| function(o) != Some("NonConductor")),
        "copper text and graphics are never flashed"
    );
    // Mask, paste and ink are material, whatever they open over.
    for name in ["Demo.gts", "Demo.gbs", "Demo.gtp", "Demo.gto"] {
        let file = layer(&files, name);
        assert!(!file.objects.is_empty(), "{name}");
        for o in file.flashes().chain(file.draws()) {
            if matches!(o, Object::Flash { dark: false, .. }) {
                continue; // the silkscreen cleared over mask openings
            }
            assert_eq!(function(o), Some("Material"), "{name}: {o:?}");
        }
    }
    let profile = layer(&files, "Demo.gm1");
    assert!(profile.draws().all(|o| function(o) == Some("Profile")));
    // Zone fills are conductor; their holes clear it.
    let bottom = layer(&files, "Demo.gbl");
    assert!(bottom.regions().count() > 1);
    assert!(bottom.regions().all(|o| function(o) == Some("Conductor")));
}

#[test]
fn without_x2_attributes_the_same_attributes_are_g04_comments() {
    let dir = TempDir::new("gerber-x2-off");
    let engine = every_kind(&dir);
    let with = plot(&engine);
    let without = plot_with(&engine, |g| g.x2_attributes = false)
        .unwrap()
        .files;
    assert_eq!(with.len(), without.len());
    for (x2, x1) in with.iter().zip(&without) {
        let text = String::from_utf8(x1.bytes.clone()).unwrap();
        for command in ["%TF", "%TA", "%TO", "%TD"] {
            assert!(!text.contains(command), "{} has {command}", x1.name);
        }
        assert!(text.contains("G04 #@! TF.FileFunction,"), "{}", x1.name);
        let (a, b) = (read(&x1.bytes), read(&x2.bytes));
        assert!(a.comment_attributes > 0);
        assert_eq!(b.comment_attributes, 0);
        assert_eq!(a.file, b.file, "{}", x1.name);
        assert_eq!(
            a.objects, b.objects,
            "{}: the same functions and attributes",
            x1.name
        );
    }
}

#[test]
fn the_board_outline_goes_on_every_plot_only_when_asked() {
    let dir = TempDir::new("gerber-outline-every-plot");
    let engine = every_kind(&dir);
    let outline = |layer: &Layer| -> Vec<Object> {
        layer
            .draws()
            .filter(|o| function(o) == Some("Profile"))
            .cloned()
            .collect()
    };
    let off = plot(&engine);
    let profile = outline(&layer(&off, "Demo.gm1"));
    assert!(!profile.is_empty());
    for file in off.iter().filter(|f| f.function != "profile") {
        assert!(outline(&read(&file.bytes)).is_empty(), "{}", file.name);
    }
    assert!(
        !reda_model::GerberSettings::default().outline_on_every_plot,
        "off by default"
    );
    let on = plot_with(&engine, |g| g.outline_on_every_plot = true)
        .unwrap()
        .files;
    for file in &on {
        assert_eq!(outline(&read(&file.bytes)), profile, "{}", file.name);
    }
}

#[test]
fn copper_mask_and_profile_are_always_written_and_empty_silkscreen_and_paste_are_skipped() {
    let dir = TempDir::new("gerber-skipped");
    let engine = board(&dir);
    let gerbers = plot_with(&engine, |_| {}).unwrap();
    let names: Vec<_> = gerbers.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["Demo.gtl", "Demo.gbl", "Demo.gts", "Demo.gbs", "Demo.gm1"]
    );
    let skipped: Vec<_> = gerbers
        .skipped
        .iter()
        .map(|f| (f.name.as_str(), f.function))
        .collect();
    assert_eq!(
        skipped,
        [
            ("Demo.gtp", "paste_top"),
            ("Demo.gbp", "paste_bottom"),
            ("Demo.gto", "silkscreen_top"),
            ("Demo.gbo", "silkscreen_bottom"),
        ]
    );
    // An outline added to every plot doesn't make an empty file worth writing.
    let on = plot_with(&engine, |g| g.outline_on_every_plot = true).unwrap();
    assert_eq!(on.skipped, gerbers.skipped);
}

/// Twice the area a contour fills; cut-in pairs cancel.
fn twice_area(points: &[(i64, i64)]) -> i128 {
    (0..points.len())
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            a.0 as i128 * b.1 as i128 - b.0 as i128 * a.1 as i128
        })
        .sum()
}

#[test]
fn outline_font_glyphs_are_whole_dark_regions_with_cut_ins() {
    let dir = TempDir::new("gerber-outline-text");
    let mut engine = board(&dir);
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "B", "x": "5mm", "y": "5mm",
               "height": "2mm", "font": "Noto Sans"}),
    );
    let files = plot(&engine);
    let silk = layer(&files, "Demo.gto");
    let regions: Vec<_> = silk.regions().collect();
    assert_eq!(
        regions.len(),
        1,
        "\"B\" is one region, its two counters joined in"
    );
    let Object::Region { dark, points, .. } = regions[0] else {
        unreachable!()
    };
    assert!(*dark, "no clear polarity: nothing underneath is erased");
    assert_eq!(function(regions[0]), Some("Material"));
    // The same area the canvas fills with triangles, within what merging points under
    // 1 µm apart can change.
    let pcb = engine.project().unwrap().pcb().unwrap();
    let text = pcb.texts().next().unwrap();
    let mesh = reda_pcb::text_geometry(text, &Fonts::default()).fill;
    let triangles: i128 = mesh
        .indices
        .chunks_exact(3)
        .map(|t| {
            let [a, b, c] = [0, 1, 2].map(|i| mesh.points[t[i] as usize]);
            ((b.x.0 - a.x.0) as i128 * (c.y.0 - a.y.0) as i128
                - (b.y.0 - a.y.0) as i128 * (c.x.0 - a.x.0) as i128)
                .abs()
        })
        .sum();
    let area = twice_area(points).abs();
    assert!(
        (area - triangles).abs() * 1000 < triangles,
        "region {area} vs triangles {triangles}"
    );
    // Every triangle's centre is filled, and the middle of each counter is not.
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| mesh.points[t[i] as usize]);
        let centre = ((a.x.0 + b.x.0 + c.x.0) / 3, (a.y.0 + b.y.0 + c.y.0) / 3);
        assert_ne!(winding(points, centre), 0, "{centre:?} left out");
    }
    let regions = reda_pcb::text_geometry(text, &Fonts::default()).regions;
    assert_eq!(regions[0].holes.len(), 2);
    for hole in &regions[0].holes {
        let n = hole.len() as i64;
        let centre = (
            hole.iter().map(|p| p.x.0).sum::<i64>() / n,
            hole.iter().map(|p| p.y.0).sum::<i64>() / n,
        );
        assert_eq!(
            winding(points, centre),
            0,
            "counter at {centre:?} is filled"
        );
    }
}

#[test]
fn a_filled_circle_is_a_region_of_two_arcs_not_a_flash() {
    let dir = TempDir::new("gerber-disc");
    let mut engine = board(&dir);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "pads": [], "graphics": [
            {"layer": "silkscreen_top", "width": "0.1mm", "fill": true,
             "shape": {"kind": "circle", "center": pt(0.0, 0.0), "radius": 450_000}}]}),
    );
    let files = plot(&engine);
    let silk = layer(&files, "Demo.gto");
    assert_eq!(silk.flashes().count(), 0);
    let regions: Vec<_> = silk.regions().collect();
    assert_eq!(regions.len(), 1);
    let Object::Region { points, arcs, .. } = regions[0] else {
        unreachable!()
    };
    // Diameter 0.9 mm plus the 0.1 mm pen: radius 0.5 mm.
    assert_eq!(
        points,
        &[
            (mm(10.5), mm(10.0)),
            (mm(9.5), mm(10.0)),
            (mm(10.5), mm(10.0))
        ]
    );
    assert_eq!(arcs.len(), 2);
    assert!(
        arcs.iter()
            .all(|(_, a)| a.centre == (mm(10.0), mm(10.0)) && a.ccw)
    );
}

#[test]
fn short_arcs_become_draws_and_near_full_arcs_are_split() {
    let dir = TempDir::new("gerber-arcs");
    let mut engine = board(&dir);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "pads": [], "graphics": [
            // 1 mm radius through 0.1°: 1.7 µm long.
            {"layer": "silkscreen_top", "width": "0.1mm",
             "shape": {"kind": "arc", "center": pt(0.0, 0.0), "start": pt(1.0, 0.0), "sweep": 100}},
            // 359.95°: its ends 0.9 µm apart.
            {"layer": "silkscreen_bottom", "width": "0.1mm",
             "shape": {"kind": "arc", "center": pt(0.0, 0.0), "start": pt(1.0, 0.0),
                       "sweep": 359_950}}]}),
    );
    let files = plot(&engine);
    // The 0.1 mm pen's draws: the Designator's strokes are drawn with another.
    let pen = |layer: Layer| -> Vec<Object> {
        layer
            .draws()
            .filter(|o| {
                matches!(o, Object::Draw { aperture, .. }
                    if *aperture == gerber_reader::Aperture::Circle(100_000.0))
            })
            .cloned()
            .collect()
    };
    let short = pen(layer(&files, "Demo.gto"));
    assert_eq!(short.len(), 1);
    let Object::Draw { from, arc, .. } = &short[0] else {
        unreachable!()
    };
    assert_eq!((*from, *arc), ((mm(11.0), mm(10.0)), None));
    let full = pen(layer(&files, "Demo.gbo"));
    assert_eq!(full.len(), 2, "one arc written as two halves");
    for draw in &full {
        let Object::Draw { from, to, arc, .. } = draw else {
            unreachable!()
        };
        let arc = arc.expect("both halves are arcs");
        let d = ((from.0 - to.0) as f64).hypot((from.1 - to.1) as f64);
        assert!(d > 1_000_000.0, "each half's ends are far apart: {d}");
        let r = |p: (i64, i64)| ((p.0 - arc.centre.0) as f64).hypot((p.1 - arc.centre.1) as f64);
        assert!((r(*from) - 1e6).abs() < 2.0 && (r(*to) - 1e6).abs() < 2.0);
    }
    let Object::Draw { to: end, .. } = &full[1] else {
        unreachable!()
    };
    let Object::Draw { from: start, .. } = &full[0] else {
        unreachable!()
    };
    assert_eq!(*start, (mm(11.0), mm(10.0)));
    assert!(*end != *start, "the arc stops short of a full turn");
}

#[test]
fn contour_points_under_a_micrometre_apart_merge() {
    let dir = TempDir::new("gerber-contour-merge");
    let mut engine = board(&dir);
    let at = |x: i64, y: i64| json!({"x": x, "y": y});
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(10.0, 10.0), "pads": [], "graphics": [
            {"layer": "silkscreen_top", "width": 0, "fill": true,
             "shape": {"kind": "polygon", "points": [
                at(0, 0), at(500, 0), at(1_000_000, 0), at(1_000_000, 1_000_000),
                at(0, 1_000_000), at(0, 999)]}}]}),
    );
    let files = plot(&engine);
    let silk = layer(&files, "Demo.gto");
    let Some(Object::Region { points, .. }) = silk.regions().next() else {
        panic!("no region")
    };
    let p = |x: f64, y: f64| (mm(10.0 + x), mm(10.0 + y));
    assert_eq!(
        points,
        &[
            p(0.0, 0.0),
            p(1.0, 0.0),
            p(1.0, 1.0),
            p(0.0, 1.0),
            p(0.0, 0.0)
        ]
    );
}

#[test]
fn a_coordinate_beyond_the_32_bit_range_is_refused() {
    let dir = TempDir::new("gerber-range");
    let mut engine = board(&dir);
    // ±2147.483647 mm fits; a nanometre more does not.
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.2mm",
               "points": [{"x": -2_147_483_647i64, "y": 0}, {"x": 0, "y": 0}]}),
    );
    assert!(plot_with(&engine, |_| {}).is_ok());
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_bottom", "width": "0.2mm",
               "points": [{"x": 0, "y": 0}, {"x": 0, "y": 2_147_483_648i64}]}),
    );
    let err = plot_with(&engine, |_| {}).unwrap_err();
    assert_eq!(err.file, "Demo.gbl");
    assert_eq!(
        err.at,
        reda_model::geom::Point::new(reda_model::geom::Nm(0), reda_model::geom::Nm(2_147_483_648),)
    );
}

fn names(gerbers: &Gerbers) -> (Vec<&str>, Vec<&str>) {
    (
        gerbers.files.iter().map(|f| f.name.as_str()).collect(),
        gerbers.skipped.iter().map(|f| f.name.as_str()).collect(),
    )
}

#[test]
fn classic_names_are_lowercase_with_gm1_for_the_profile() {
    let dir = TempDir::new("gerber-classic-names");
    let engine = board(&dir);
    let gerbers = plot_with(&engine, |g| g.naming = FileNaming::Classic).unwrap();
    assert_eq!(
        names(&gerbers),
        (
            vec!["Demo.gtl", "Demo.gbl", "Demo.gts", "Demo.gbs", "Demo.gm1"],
            vec!["Demo.gtp", "Demo.gbp", "Demo.gto", "Demo.gbo"],
        )
    );
}

#[test]
fn descriptive_names_are_the_project_and_layer_with_underscores_for_spaces() {
    let dir = TempDir::new("gerber-descriptive-names");
    let engine = board(&dir);
    let gerbers = plot_with(&engine, |g| g.naming = FileNaming::Descriptive).unwrap();
    assert_eq!(
        names(&gerbers),
        (
            vec![
                "Demo-Top_Copper.gbr",
                "Demo-Bottom_Copper.gbr",
                "Demo-Top_Solder_Mask.gbr",
                "Demo-Bottom_Solder_Mask.gbr",
                "Demo-Board_Outline.gbr",
            ],
            vec![
                "Demo-Top_Paste.gbr",
                "Demo-Bottom_Paste.gbr",
                "Demo-Top_Silkscreen.gbr",
                "Demo-Bottom_Silkscreen.gbr",
            ],
        )
    );
}

#[test]
fn file_names_override_any_layers_file_in_either_naming() {
    let dir = TempDir::new("gerber-file-names");
    let engine = board(&dir);
    for naming in [FileNaming::Classic, FileNaming::Descriptive] {
        let gerbers = plot_with(&engine, |g| {
            g.naming = naming;
            g.file_names = vec![
                LayerFileName {
                    layer: "Top Copper".into(),
                    name: "top.gbr".into(),
                },
                LayerFileName {
                    layer: "Board Outline".into(),
                    name: "Demo.GKO".into(),
                },
                LayerFileName {
                    layer: "Top Paste".into(),
                    name: "stencil.gbr".into(),
                },
            ];
        })
        .unwrap();
        let (files, skipped) = names(&gerbers);
        assert_eq!(files[0], "top.gbr", "{naming:?}");
        assert_eq!(files[4], "Demo.GKO", "{naming:?}");
        assert_eq!(skipped[0], "stencil.gbr", "{naming:?}");
        assert!(!files[1].contains("top"), "{naming:?}: only the named Layer");
    }
}
