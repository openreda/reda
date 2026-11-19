//! Independent fabrication read-back of the four-Layer assembly coupon.
mod drill_reader;
mod gerber_reader;
mod job_reader;

use gerber_reader::Object;
use reda_testkit::{open_reference_copy, read_csv};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[test]
fn the_four_layer_assembly_coupon_exports_its_routed_copper_and_corrected_placements() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/four_layer");
    let (_dir, mut engine) = open_reference_copy(&source, "FourLayer.reda");
    let job = engine.project().unwrap().output_jobs().next().unwrap().uuid;
    engine.run_ok("pcb.zone.fill", json!({}));
    let report = engine.run_ok("output.job.run", json!({"job":job}));
    assert_eq!(report["warnings"], json!([]));
    assert_eq!(report["skipped"], json!([]));
    let output = Path::new(report["destination"].as_str().unwrap());
    let fab = output.join("fab");
    let job = job_reader::read(&fs::read(fab.join("FourLayer-job.gbrjob")).unwrap());
    assert_eq!(job.general_specs.layer_number, 4);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let fills = engine.project().unwrap().zone_fills(pcb.uuid).unwrap();
    let targets: BTreeSet<_> = fills
        .teardrops
        .iter()
        .filter(|d| !d.shapes.is_empty())
        .map(|d| {
            let target = d.location.target;
            if let Some(id) = d.location.pad {
                let instance = pcb.footprint(target).unwrap();
                let copy = pcb.footprint_copy(instance.footprint).unwrap();
                let pad = copy.footprint.pads.iter().find(|p| p.uuid == id).unwrap();
                assert_eq!(pad.kind, reda_model::PadKind::Smd);
                "SMD Pad"
            } else if let Some(pad) = pcb.free_pad(target) {
                assert_eq!(pad.kind, reda_model::PadKind::PlatedThroughHole);
                "Through-hole Pad"
            } else if pcb.via(target).is_some() {
                "Via"
            } else {
                assert!(
                    pcb.track(target).unwrap().width > pcb.track(d.location.track).unwrap().width
                );
                "Width change"
            }
        })
        .collect();
    assert_eq!(
        targets,
        BTreeSet::from(["Through-hole Pad", "SMD Pad", "Via", "Width change"])
    );
    job_reader::check_materials(pcb, &job);
    assert_eq!(
        pcb.stackup().construction.as_deref(),
        Some("JLC04161H-7628")
    );
    assert_eq!(
        job.material_stackup
            .iter()
            .filter(|r| r.kind == "Dielectric")
            .count(),
        3
    );
    let files: Vec<_> = job
        .files_attributes
        .iter()
        .filter(|f| f.file_function.starts_with("Copper,"))
        .collect();
    assert_eq!(files.len(), 4);
    for (index, (ext, function, via_x)) in [
        ("gtl", "Copper,L1,Top,Signal", &[6][..]),
        ("g2l", "Copper,L2,Inr,Plane", &[6, 14][..]),
        ("g3l", "Copper,L3,Inr,Plane", &[][..]),
        ("gbl", "Copper,L4,Bot,Signal", &[14][..]),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(files[index].path, format!("FourLayer.{ext}"));
        assert_eq!(files[index].file_function, function);
        let copper = gerber_reader::read(&fs::read(fab.join(&files[index].path)).unwrap());
        assert_eq!(copper.file[".FileFunction"], function);
        assert!(copper.ended);
        let layer = pcb.copper_layers().nth(index).unwrap().uuid;
        let mut lands: Vec<_> = copper
            .flashes()
            .filter_map(|o| match o {
                Object::Flash {
                    dark: true,
                    at,
                    function,
                    ..
                } if function.as_deref() == Some("ViaPad") => Some(*at),
                _ => None,
            })
            .collect();
        lands.sort();
        assert_eq!(
            lands,
            via_x
                .iter()
                .map(|x| (x * 1_000_000, 9_000_000))
                .collect::<Vec<_>>()
        );
        // There are no Zones. Every dark region is derived copper attached to a routed Net.
        let mut actual = Vec::new();
        for region in copper.regions() {
            let Object::Region {
                dark,
                points,
                arcs,
                function,
                attributes,
            } = region
            else {
                unreachable!()
            };
            assert!(*dark && points.len() >= 4 && arcs.is_empty());
            assert_eq!(function.as_deref(), Some("Conductor"));
            assert_eq!(points.first(), points.last());
            let ring = canonical_ring(points[..points.len() - 1].to_vec());
            actual.push((attributes[".N"].clone(), ring));
        }
        let mut expected = Vec::new();
        for drop in fills.teardrops.iter().filter(|d| d.location.layer == layer) {
            let track = pcb.track(drop.location.track).unwrap();
            let name = &pcb.net(track.net.unwrap()).unwrap().name;
            for shape in &drop.shapes {
                assert!(shape.holes.is_empty());
                let vertices = shape.outer.iter().map(|p| (p.x.0, p.y.0)).collect();
                expected.push((name.clone(), canonical_ring(vertices)));
            }
        }
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "conductor regions on L{}", index + 1);
        assert_eq!(actual.is_empty(), index == 2);
    }

    let drill_files: BTreeMap<_, _> = job
        .files_attributes
        .iter()
        .filter(|r| r.path.ends_with(".drl"))
        .map(|r| (r.path.as_str(), r.file_function.as_str()))
        .collect();
    assert_eq!(
        drill_files,
        BTreeMap::from([
            ("FourLayer-PTH.drl", "Plated,1,4,PTH,Drill"),
            ("FourLayer-NPTH.drl", "NonPlated,1,4,NPTH,Drill")
        ])
    );
    for (name, expected) in [
        (
            "FourLayer-PTH.drl",
            vec![((6, 9), 300_000), ((10, 4), 800_000), ((14, 9), 300_000)],
        ),
        ("FourLayer-NPTH.drl", vec![((2, 2), 1_200_000)]),
    ] {
        let drill = drill_reader::read(&fs::read(fab.join(name)).unwrap());
        assert!(drill.ended && drill.metric);
        assert_eq!(drill.file["FileFunction"], drill_files[name]);
        let mut holes = drill
            .hits
            .into_iter()
            .map(|h| {
                assert!(h.to.is_none());
                (h.at, h.diameter)
            })
            .collect::<Vec<_>>();
        holes.sort();
        assert_eq!(
            holes,
            expected
                .into_iter()
                .map(|((x, y), d)| ((x * 1_000_000, y * 1_000_000), d))
                .collect::<Vec<_>>()
        );
    }
    let bom = read_csv(&fs::read_to_string(output.join("FourLayer-JLCPCB-BOM.csv")).unwrap());
    assert_eq!(
        bom,
        [
            ["Comment", "Designator", "Footprint", "LCSC Part #"],
            ["0R", "R1", "RESC1608X55N", "C21189"],
            ["10k", "R2", "RESC1608X55N", "C25804"]
        ]
    );
    let placements =
        read_csv(&fs::read_to_string(output.join("FourLayer-JLCPCB-CPL.csv")).unwrap());
    assert_eq!(
        placements,
        [
            ["Designator", "Mid X", "Mid Y", "Layer", "Rotation"],
            ["R1", "4.250000", "6.000000", "Top", "90.000"],
            ["R2", "17.000000", "6.250000", "Bottom", "180.000"]
        ]
    );
}

/// A simple outer contour may start at any vertex and wind in either direction.
fn canonical_ring(mut vertices: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    assert!(vertices.len() >= 3);
    let first = (0..vertices.len()).min_by_key(|&i| vertices[i]).unwrap();
    vertices.rotate_left(first);
    if vertices[1] > vertices[vertices.len() - 1] {
        vertices[1..].reverse();
    }
    vertices
}
