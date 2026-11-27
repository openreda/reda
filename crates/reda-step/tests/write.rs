//! The writer: AP214 from records, `\X2\` names, plain or gzip by extension, and the
//! read → write → read round trip.

use reda_step::records::Product;
use reda_step::{
    AP214_APPLICATION, AP214_SCHEMA, Compression, Protocol, StepModel, encode, read, read_file,
    write, write_file,
};

const FIXTURES: [&str; 8] = [
    "ap203_cylinder.step",
    "ap214_face_colours.step",
    "ap242_solid_colour.step",
    "inch_block.step",
    "nested_assembly.step",
    "parabola_cut.step",
    "hyperbola_cut.step",
    "mapped_items.step",
];

fn fixture(name: &str) -> StepModel {
    read_file(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
    .model
}

/// A folder of its own under the target directory, for files a test writes.
fn out_dir(test: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("reda-step")
        .join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn read_write_read_keeps_every_record_and_the_header() {
    for name in FIXTURES {
        let first = fixture(name);
        let again = read(write(&first).as_bytes()).unwrap();
        assert!(again.warnings.is_empty(), "{name}: {:?}", again.warnings);
        assert_eq!(again.protocol, Protocol::Ap214, "{name}");
        // Writing converts the records that name a protocol to AP214, and nothing else.
        let mut want = first.clone();
        want.to_ap214();
        assert_eq!(again.model.header, want.header, "{name}");
        assert_eq!(again.model.records().len(), want.records().len(), "{name}");
        for (i, (a, b)) in want.records().iter().zip(again.model.records()).enumerate() {
            assert_eq!(a, b, "{name}: record #{}", i + 1);
        }
        // A second write is the same text: nothing drifts.
        assert_eq!(write(&again.model), write(&first), "{name}");
    }
}

#[test]
fn ap214_sources_round_trip_unchanged() {
    for name in [
        "ap214_face_colours.step",
        "inch_block.step",
        "nested_assembly.step",
    ] {
        let first = fixture(name);
        let again = read(write(&first).as_bytes()).unwrap().model;
        assert_eq!(again, first, "{name}");
    }
}

#[test]
fn the_writer_declares_ap214() {
    let text = write(&fixture("ap203_cylinder.step"));
    assert!(text.starts_with("ISO-10303-21;\nHEADER;\n"), "{text}");
    assert!(
        text.contains(&format!("FILE_SCHEMA(('{AP214_SCHEMA}'));")),
        "{text}"
    );
    assert!(
        text.contains(&format!("APPLICATION_CONTEXT('{AP214_APPLICATION}')")),
        "{text}"
    );
    assert!(text.contains(
        "APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#"
    ));
    assert!(text.ends_with("ENDSEC;\nEND-ISO-10303-21;\n"));
    // AP203's own entities are written as AP214's.
    assert!(
        !text.contains("MECHANICAL_CONTEXT") && !text.contains("DESIGN_CONTEXT("),
        "{text}"
    );
    assert!(
        text.contains("PRODUCT_CONTEXT('',#") && text.contains("PRODUCT_DEFINITION_CONTEXT('',#")
    );
    assert!(!text.contains("CONFIG_CONTROL_DESIGN") && !text.contains("config_control_design"));
    assert!(
        !text.contains("CC_DESIGN") && !text.contains("APPROVAL"),
        "{text}"
    );
}

#[test]
fn records_are_numbered_in_model_order() {
    let model = fixture("ap214_face_colours.step");
    let text = write(&model);
    let ids: Vec<usize> = text
        .lines()
        .filter_map(|l| l.strip_prefix('#'))
        .map(|l| l.split('=').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(ids, (1..=model.records().len()).collect::<Vec<_>>());
}

#[test]
fn complex_instances_are_written_with_their_parts_in_alphabetical_order() {
    let text = write(&fixture("nested_assembly.step"));
    for want in [
        "=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));",
        "=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));",
        "=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#",
        "=(REPRESENTATION_RELATIONSHIP('','',#",
    ] {
        assert!(text.contains(want), "{want}");
    }
    let text = write(&fixture("ap203_cylinder.step"));
    assert!(text.contains(
        "=(GEOMETRIC_REPRESENTATION_CONTEXT(2)PARAMETRIC_REPRESENTATION_CONTEXT()REPRESENTATION_CONTEXT('2D SPACE',''));"
    ));
}

fn rename_products(model: &mut StepModel, name: &str) {
    let ids: Vec<_> = model.iter::<Product>().map(|(id, _)| id).collect();
    for id in ids {
        model.get_mut::<Product>(id).unwrap().name = name.to_string();
    }
}

#[test]
fn non_ascii_names_are_written_with_x2_escapes() {
    let mut model = fixture("ap214_face_colours.step");
    // ISO 10303-21: \X2\ holds UTF-16 code units in hex up to \X0\; a character outside the
    // Basic Multilingual Plane goes in \X4\ as one code point.
    rename_products(&mut model, "Ω-µ Kondensator 'ü' \\ 😀");
    let text = write(&model);
    assert!(
        text.contains(r"PRODUCT('block','\X2\03A9\X0\-\X2\00B5\X0\ Kondensator ''\X2\00FC\X0\'' \\ \X4\0001F600\X0\',"),
        "{text}"
    );
    assert!(text.is_ascii());
    let again = read(text.as_bytes()).unwrap().model;
    assert_eq!(
        again.iter::<Product>().next().unwrap().1.name,
        "Ω-µ Kondensator 'ü' \\ 😀"
    );
}

#[test]
fn reals_round_trip_exactly() {
    let mut model = fixture("ap214_face_colours.step");
    let ids: Vec<_> = model
        .iter::<reda_step::records::CartesianPoint>()
        .map(|(id, _)| id)
        .collect();
    let values = [0.1, 1.0 / 3.0, -2.5e-12, 1.0e20, 123456.789, -0.0];
    for (id, v) in ids.iter().zip(values.iter().cycle()) {
        model
            .get_mut::<reda_step::records::CartesianPoint>(*id)
            .unwrap()
            .coordinates = vec![*v, -v, v * 7.0];
    }
    let again = read(write(&model).as_bytes()).unwrap().model;
    assert_eq!(again, model);
}

#[test]
fn compression_follows_the_extension() {
    assert_eq!(Compression::for_path("a/b.step"), Compression::Plain);
    assert_eq!(Compression::for_path("b.stp"), Compression::Plain);
    assert_eq!(Compression::for_path("b.STEP"), Compression::Plain);
    assert_eq!(Compression::for_path("b.stpz"), Compression::Gzip);
    assert_eq!(Compression::for_path("B.STPZ"), Compression::Gzip);

    let model = fixture("nested_assembly.step");
    let dir = out_dir("compression");
    for (name, gzip) in [
        ("a.step", false),
        ("a.stp", false),
        ("a.stpz", true),
        ("A.STPZ", true),
    ] {
        let path = dir.join(name);
        write_file(&model, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.starts_with(&[0x1F, 0x8B]), gzip, "{name}");
        assert_eq!(bytes.starts_with(b"ISO-10303-21;"), !gzip, "{name}");
        assert_eq!(read_file(&path).unwrap().model, model, "{name}");
    }
    // Gzip is the same text, compressed.
    let gz = encode(&model, Compression::Gzip);
    let mut text = String::new();
    std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(gz.as_slice()), &mut text)
        .unwrap();
    assert_eq!(text, write(&model));
}

#[test]
fn the_stpz_fixture_reads_as_its_step_twin() {
    let step = fixture("nested_assembly.step");
    let stpz = fixture("nested_assembly.stpz");
    let bytes = std::fs::read(format!(
        "{}/tests/fixtures/nested_assembly.stpz",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    assert!(bytes.starts_with(&[0x1F, 0x8B]));
    assert_eq!(stpz.records(), step.records());
}

#[test]
fn a_built_model_writes_and_reads_back() {
    // The writer works from records alone; a model built in code needs no source file.
    use reda_step::records::*;
    let mut m = StepModel::default();
    m.header.name = "built".into();
    m.header.originating_system = "REDA 0.3.0".into();
    let app = m.add(ApplicationContext {
        application: AP214_APPLICATION.into(),
    });
    m.add(ApplicationProtocolDefinition {
        status: "international standard".into(),
        schema_name: "automotive_design".into(),
        year: 2000,
        application: app,
    });
    let pc = m.add(ProductContext {
        name: String::new(),
        frame_of_reference: app,
        discipline_type: "mechanical".into(),
    });
    let p = m.add(Product {
        id: "U1".into(),
        name: "U1".into(),
        description: String::new(),
        frame_of_reference: vec![pc],
    });
    let pdf = m.add(ProductDefinitionFormation {
        id: String::new(),
        description: String::new(),
        of_product: p,
    });
    let pdc = m.add(ProductDefinitionContext {
        name: "part definition".into(),
        frame_of_reference: app,
        life_cycle_stage: "design".into(),
    });
    let pd = m.add(ProductDefinition {
        id: "design".into(),
        description: String::new(),
        formation: pdf,
        frame_of_reference: pdc,
    });
    let pds = m.add(ProductDefinitionShape {
        name: String::new(),
        description: String::new(),
        definition: pd,
    });
    let mm = m.add(Unit::si(UnitKind::Length, Some("MILLI"), "METRE"));
    let ctx = m.add(RepresentationContext {
        context_identifier: String::new(),
        context_type: String::new(),
        dimension: Some(3),
        parametric: false,
        uncertainty: None,
        units: Some(vec![mm]),
    });
    let o = m.add(CartesianPoint {
        name: String::new(),
        coordinates: vec![0.0, 0.0, 0.0],
    });
    let ax = m.add(Axis2Placement3d {
        name: String::new(),
        location: o,
        axis: None,
        ref_direction: None,
    });
    let rep = m.add(Representation {
        kind: RepresentationKind::Shape,
        name: "U1".into(),
        items: vec![ax],
        context: ctx,
    });
    m.add(ShapeDefinitionRepresentation {
        definition: pds,
        used_representation: rep,
    });

    let text = write(&m);
    // FILE_NAME's author and organization lists hold at least one string.
    assert!(
        text.contains("FILE_NAME('built','',(''),(''),'','REDA 0.3.0','');"),
        "{text}"
    );
    assert!(text.contains("#10=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#9))REPRESENTATION_CONTEXT('',''));"), "{text}");
    assert!(
        text.contains("#12=AXIS2_PLACEMENT_3D('',#11,$,$);"),
        "{text}"
    );
    let again = read(text.as_bytes()).unwrap();
    assert!(again.warnings.is_empty());
    assert_eq!(again.model, m);
    assert!(matches!(again.model.records()[8], Record::Unit(_)));
}
