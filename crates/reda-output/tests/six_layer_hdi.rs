//! Read the manufacturing files of the routed six-Layer coupon back against its PCB.
mod drill_reader;
mod gerber_reader;
mod job_reader;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use gerber_reader::{Aperture, Object};
use reda_model::geom::Point;
use reda_model::{PadKind, PadShape, PcbDocument, Side, StackupEntry};
use reda_testkit::{open_reference_copy, read_csv};
use serde_json::json;

fn xy(p: Point) -> (i64, i64) {
    (p.x.0, p.y.0)
}

#[test]
fn the_six_layer_hdi_job_describes_the_board() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/six_layer_hdi");
    let (_dir, mut engine) = open_reference_copy(&source, "SixLayerHdi.reda");
    let job = engine.project().unwrap().output_jobs().next().unwrap().uuid;
    let report = engine.run_ok("output.job.run", json!({"job":job}));
    assert_eq!(report["warnings"], json!([]));
    assert_eq!(report["skipped"], json!([]));
    let output = Path::new(report["destination"].as_str().unwrap());
    let pcb = engine.project().unwrap().pcb().unwrap();

    let fab = output.join("fab");
    let job = job_reader::read(&fs::read(fab.join("SixLayerHdi-job.gbrjob")).unwrap());
    check_materials(pcb, &job);
    check_copper(pcb, &fab, &job);
    check_drills(pcb, &fab);
    check_placements(pcb, &output.join("SixLayerHdi-HDI-CPL.csv"));
}

fn check_materials(pcb: &PcbDocument, job: &job_reader::Job) {
    assert_eq!(job.general_specs.layer_number, pcb.copper_layers().count());
    let stack = pcb.stackup();
    let rows = &job.material_stackup;
    assert_eq!(rows.len(), stack.entries.len() + 4);
    for (i, kind, color) in [
        (0, "Legend", &stack.silk_color),
        (1, "SolderMask", &stack.mask_color),
        (rows.len() - 2, "SolderMask", &stack.mask_color),
        (rows.len() - 1, "Legend", &stack.silk_color),
    ] {
        assert_eq!((&*rows[i].kind, &rows[i].color), (kind, color));
    }
    for (row, sheet) in rows[2..].iter().zip(&stack.entries) {
        match sheet {
            StackupEntry::Copper(c) => {
                let layer = pcb.layers().layer(c.layer).unwrap();
                let pos = pcb.copper_number(c.layer).unwrap();
                let name = layer.name.clone().unwrap_or_else(|| format!("L{pos}"));
                assert_eq!((&*row.kind, row.name.as_ref()), ("Copper", Some(&name)));
                assert_eq!(row.thickness, Some(c.thickness.0 as f64 / 1e6));
            }
            StackupEntry::Dielectric(d) => {
                assert_eq!(row.kind, "Dielectric");
                assert_eq!(row.name, d.name);
                assert_eq!(row.material, d.material);
                assert_eq!(row.thickness, d.thickness.map(|v| v.0 as f64 / 1e6));
                assert_eq!(row.dielectric_constant, d.dk.map(|v| f64::from(v) / 1000.));
                assert_eq!(row.loss_tangent, d.df.map(|v| f64::from(v) / 1e6));
            }
        }
    }
}

fn check_copper(pcb: &PcbDocument, fab: &Path, job: &job_reader::Job) {
    let pad = pcb
        .free_pads()
        .find(|p| p.kind == PadKind::PlatedThroughHole)
        .unwrap();
    let names = [
        "SixLayerHdi.gtl",
        "SixLayerHdi-L2.ger",
        "SixLayerHdi-L3.ger",
        "SixLayerHdi-L4.ger",
        "SixLayerHdi-L5.ger",
        "SixLayerHdi.gbl",
    ];
    let copper_files: Vec<_> = job
        .files_attributes
        .iter()
        .filter(|f| f.file_function.starts_with("Copper,"))
        .collect();
    assert_eq!(
        copper_files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        names
    );
    for (layer, file) in pcb.copper_layers().zip(copper_files) {
        let artwork = gerber_reader::read(&fs::read(fab.join(&file.path)).unwrap());
        assert!(artwork.ended);
        assert_eq!(artwork.file[".FileFunction"], file.file_function);
        let form = pad.form_on(layer.side);
        let land = match form.shape {
            PadShape::Circle => Aperture::Circle(form.width.0 as f64),
            PadShape::Rect => Aperture::Rect(form.width.0 as f64, form.height.0 as f64),
            PadShape::Oval => Aperture::Oval(form.width.0 as f64, form.height.0 as f64),
            shape => panic!("unexpected HDI through-Pad shape: {shape:?}"),
        };
        let mut lands = Vec::new();
        let mut vias = Vec::new();
        let mut component_pads = Vec::new();
        for flash in artwork.flashes() {
            let Object::Flash {
                dark,
                at,
                aperture,
                function,
                attributes,
            } = flash
            else {
                unreachable!()
            };
            assert!(dark);
            match function.as_deref() {
                Some("OtherPad,FreePad") => lands.push((*at, aperture)),
                Some("ViaPad") => {
                    let Aperture::Circle(d) = aperture else {
                        panic!("Via land must be circular")
                    };
                    vias.push((*at, *d as i64));
                }
                Some("SMDPad,CuDef") => component_pads.push((*at, attributes[".P"].clone())),
                other => panic!("unexpected flash {other:?}"),
            }
        }
        assert_eq!(lands, [(xy(pad.at), &land)], "through Pad on {}", file.path);
        let mut expected_vias: Vec<_> = pcb
            .vias()
            .filter(|v| pcb.span_layers(v.span).contains(&layer.uuid))
            .map(|v| (xy(v.at), v.diameter_on(layer.uuid).0))
            .collect();
        expected_vias.sort();
        vias.sort();
        assert_eq!(vias, expected_vias, "Via lands on {}", file.path);
        let mut expected_pads = Vec::new();
        for fp in pcb.footprints().filter(|fp| Some(fp.side) == layer.side) {
            for placed in reda_pcb::placed::placed_pads(pcb, fp) {
                expected_pads.push((
                    xy(placed.at),
                    format!("{},{}", fp.designator, placed.number),
                ));
            }
        }
        component_pads.sort();
        expected_pads.sort();
        assert_eq!(component_pads, expected_pads);
        let mut tracks = Vec::new();
        for object in artwork.draws() {
            let Object::Draw {
                from,
                to,
                aperture: Aperture::Circle(d),
                ..
            } = object
            else {
                panic!("expected a round Track")
            };
            tracks.push((*from, *to, *d as i64));
        }
        let mut expected_tracks: Vec<_> = pcb
            .tracks()
            .filter(|t| t.layer == layer.uuid)
            .map(|t| {
                let (a, b) = t.geometry.ends();
                (xy(a), xy(b), t.width.0)
            })
            .collect();
        tracks.sort();
        expected_tracks.sort();
        assert_eq!(tracks, expected_tracks, "Tracks on {}", file.path);
    }
}

fn check_drills(pcb: &PcbDocument, fab: &Path) {
    let mut expected = BTreeMap::new();
    for via in pcb.vias() {
        let a = pcb.copper_number(via.span.from).unwrap();
        let b = pcb.copper_number(via.span.to).unwrap();
        let (first, last) = (a.min(b), a.max(b));
        let kind = if first == 1 || last == 6 {
            "Blind"
        } else {
            "Buried"
        };
        expected
            .entry(format!("Plated,{first},{last},{kind}"))
            .or_insert_with(Vec::new)
            .push((xy(via.at), via.hole.0, "ViaDrill".to_string()));
    }
    for pad in pcb.free_pads() {
        let function = match pad.kind {
            PadKind::PlatedThroughHole => "Plated,1,6,PTH",
            PadKind::NonPlated => "NonPlated,1,6,NPTH",
            PadKind::Smd => continue,
        };
        expected.entry(function.into()).or_default().push((
            xy(pad.at),
            pad.hole.unwrap().width.0,
            "MechanicalDrill".into(),
        ));
    }
    let mut actual = BTreeMap::new();
    for path in fs::read_dir(fab)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "drl"))
    {
        let drill = drill_reader::read(&fs::read(&path).unwrap());
        assert!(drill.ended && drill.metric);
        let hits = drill
            .hits
            .into_iter()
            .map(|h| {
                assert!(h.to.is_none());
                (h.at, h.diameter, h.function.unwrap())
            })
            .collect::<Vec<_>>();
        let function = drill.file["FileFunction"].clone();
        assert!(
            actual.insert(function, hits).is_none(),
            "span and plating repeated in {}",
            path.display()
        );
    }
    assert_eq!(actual, expected);
}

fn check_placements(pcb: &PcbDocument, path: &Path) {
    let csv = read_csv(&fs::read_to_string(path).unwrap());
    assert_eq!(
        csv[0],
        ["Designator", "Mid X", "Mid Y", "Layer", "Rotation"]
    );
    let mut placements = BTreeMap::new();
    for row in &csv[1..] {
        let coords = [1, 2, 4].map(|i| row[i].parse::<f64>().unwrap());
        assert!(
            placements
                .insert(row[0].as_str(), (row[3].as_str(), coords))
                .is_none()
        );
    }
    assert_eq!(placements.len(), 2);
    for fp in pcb.footprints() {
        // Both resistor lands are symmetric about the origin. The Generic preset
        // uses the top view for XY and adds 180 degrees to bottom-side rotation.
        let (side, offset) = match fp.side {
            Side::Top => ("Top", 0.),
            Side::Bottom => ("Bottom", 180.),
        };
        let expected = [
            fp.at.x.0 as f64 / 1e6,
            fp.at.y.0 as f64 / 1e6,
            (fp.rotation.0 as f64 / 1000. + offset).rem_euclid(360.),
        ];
        assert_eq!(
            placements.remove(fp.designator.as_str()),
            Some((side, expected))
        );
    }
    assert!(placements.is_empty());
}
