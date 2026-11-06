//! Multiple board instances through Gerber and drill read-back, and the CPL file.

mod drill_reader;
mod gerber_reader;

use reda_command::{Fonts, Timestamp};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{FileOrigin, Uuid};
use reda_output::{BoardInstance, InstanceTransform, Plot, cpl_file, drill_files, gerber_files};
use reda_testkit::{TempDir, probe_engine};
use serde_json::json;

#[test]
fn every_writer_applies_each_board_instances_rigid_transform() {
    let dir = TempDir::new("output-instances");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.25mm",
        "points": [{"x": "2mm", "y": "3mm"}, {"x": "4mm", "y": "3mm"}]}),
    );
    let added = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "3mm", "y": "4mm"},
        "pads": [{"at": {"x": 0, "y": 0}, "width": "2mm", "height": "1mm"}]}),
    );
    let footprint: Uuid = added["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    engine.run_ok(
        "pcb.mechanical.set",
        json!({"footprints": [footprint], "in_cpl": true}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "rect",
        "at": {"x": "5mm", "y": "6mm"}, "width": "3mm", "height": "2mm",
        "hole": {"width": "1.2mm", "height": "0.4mm"}}),
    );
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "shape": "rect",
        "at": {"x": "12mm", "y": "8mm"}, "width": "2mm", "height": "1mm",
        "mask": {"top": {"expansion": "0.2mm"}},
        "paste": {"windows": [{"at": {"x": "0.5mm", "y": 0},
            "width": "0.4mm", "height": "0.2mm"}]}}),
    );
    let project = engine.project().unwrap();
    let pcb = project.pcb().unwrap();
    let fills = reda_pcb::zone::current(pcb, None);
    let preset = project.fab_presets().next().unwrap().preset();
    let fonts = Fonts::default();
    let empty = Plot {
        project,
        instances: &[],
        gerber: &preset.gerber,
        fonts: &fonts,
        created: Timestamp(0),
    };
    assert!(gerber_files(&empty).is_empty());
    assert!(drill_files(&empty).is_empty());
    let instances = [
        BoardInstance::single(pcb, &fills, FileOrigin::Absolute),
        BoardInstance {
            pcb,
            fills: &fills,
            transform: InstanceTransform {
                translation: Point::new(Nm(10_000_000), Nm(20_000_000)),
                rotation: Millideg(90_000),
            },
        },
    ];
    let plot = Plot {
        project,
        instances: &instances,
        gerber: &preset.gerber,
        fonts: &fonts,
        created: Timestamp(0),
    };
    let gerbers = gerber_files(&plot);
    let read =
        |name: &str| gerber_reader::read(&gerbers.iter().find(|f| f.name == name).unwrap().bytes);
    let copper = read("Demo.GTL");
    assert!(copper.draws().any(|o| matches!(
        o,
        gerber_reader::Object::Draw {
            from: (7_000_000, 22_000_000),
            to: (7_000_000, 24_000_000),
            ..
        }
    )));
    for name in ["Demo.GTL", "Demo.GTS", "Demo.GTP"] {
        assert!(
            read(name).flashes().any(|o| matches!(
                o,
                gerber_reader::Object::Flash {
                    at: (6_000_000, 23_000_000),
                    ..
                }
            )),
            "{name}"
        );
    }
    let aperture = copper
        .flashes()
        .find_map(|o| match o {
            gerber_reader::Object::Flash {
                at: (6_000_000, 23_000_000),
                aperture,
                ..
            } => Some(aperture),
            _ => None,
        })
        .unwrap();
    assert!(aperture.contains(0.0, 900_000.0));
    assert!(
        !aperture.contains(900_000.0, 0.0),
        "the rectangular pad rotates with its board"
    );
    for (name, at, inside_x, outside_x, inside_y, outside_y) in [
        (
            "Demo.GTS",
            (2_000_000, 32_000_000),
            690_000.0,
            710_000.0,
            1_190_000.0,
            1_210_000.0,
        ),
        (
            "Demo.GTP",
            (2_000_000, 32_500_000),
            90_000.0,
            110_000.0,
            190_000.0,
            210_000.0,
        ),
    ] {
        let layer = read(name);
        let aperture = layer
            .flashes()
            .find_map(|o| match o {
                gerber_reader::Object::Flash {
                    at: position,
                    aperture,
                    ..
                } if *position == at => Some(aperture),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: transformed opening at {at:?}"));
        assert!(aperture.contains(inside_x, 0.0), "{name}");
        assert!(!aperture.contains(outside_x, 0.0), "{name}");
        assert!(aperture.contains(0.0, inside_y), "{name}");
        assert!(!aperture.contains(0.0, outside_y), "{name}");
    }
    let profile = read("Demo.GKO");
    assert_eq!(profile.draws().count(), 8);
    let drills = drill_files(&plot);
    let plated = drill_reader::read(
        &drills
            .iter()
            .find(|f| f.function == "drill_plated")
            .unwrap()
            .bytes,
    );
    assert_eq!(plated.hits.len(), 2);
    assert_eq!(plated.hits[1].at, (4_000_000, 24_600_000));
    assert_eq!(plated.hits[1].to, Some((4_000_000, 25_400_000)));
    let cpl = cpl_file(project, &instances, &preset.cpl.columns);
    assert_eq!(
        String::from_utf8(cpl.bytes).unwrap(),
        "Designator,Mid X,Mid Y,Layer,Rotation\nTP,3.000000,4.000000,Top,0.000\nTP,6.000000,23.000000,Top,90.000\n"
    );
}
