//! Part 21 syntax as `read` sees it: the header, the protocol, strings and their escapes, and
//! what a malformed file does.

use reda_step::records::{CartesianPoint, Direction, EdgeCurve, EdgeLoop, OrientedEdge, Record};
use reda_step::{Protocol, read};

/// A file holding `data`, with a shape representation that lists every instance in it, so
/// the reader keeps them (it drops records that no representation or product reaches).
fn file(schema: &str, data: &str) -> String {
    // Every `#n` followed by `=` defines an instance.
    let mut ids: Vec<&str> = Vec::new();
    for (at, _) in data.match_indices('#') {
        let rest = &data[at + 1..];
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if digits > 0
            && rest[digits..].trim_start().starts_with('=')
            && !ids.contains(&&rest[..digits])
        {
            ids.push(&rest[..digits]);
        }
    }
    let items: Vec<String> = ids.iter().map(|id| format!("#{id}")).collect();
    format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('a test'),'2;1');\n\
         FILE_NAME('t.step','2026-11-26T10:00:00',('Ann'),('REDA'),'pre','orig','auth');\n\
         FILE_SCHEMA(('{schema}'));\nENDSEC;\nDATA;\n{data}\n\
         #9000=SHAPE_REPRESENTATION('',({}),#9001);\n\
         #9001=GEOMETRIC_REPRESENTATION_CONTEXT('','',3);\nENDSEC;\nEND-ISO-10303-21;\n",
        items.join(",")
    )
}

#[test]
fn a_point_reads_as_a_typed_record() {
    let text = file(
        "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }",
        "#10=CARTESIAN_POINT('origin',(1.5,-2.,3.E-1));",
    );
    let read = read(text.as_bytes()).unwrap();
    assert_eq!(read.protocol, Protocol::Ap214);
    let points: Vec<_> = read.model.iter::<CartesianPoint>().collect();
    assert_eq!(points.len(), 1, "{:?}", read.model.records());
    assert_eq!(points[0].1.name, "origin");
    assert_eq!(points[0].1.coordinates, vec![1.5, -2.0, 0.3]);
}

#[test]
fn the_header_is_read() {
    let text = file("AUTOMOTIVE_DESIGN", "#1=CARTESIAN_POINT('',(0.,0.,0.));");
    let header = read(text.as_bytes()).unwrap().model.header;
    assert_eq!(header.description, vec!["a test".to_string()]);
    assert_eq!(header.name, "t.step");
    assert_eq!(header.time_stamp, "2026-11-26T10:00:00");
    assert_eq!(header.author, vec!["Ann".to_string()]);
    assert_eq!(header.organization, vec!["REDA".to_string()]);
    assert_eq!(header.preprocessor_version, "pre");
    assert_eq!(header.originating_system, "orig");
    assert_eq!(header.authorization, "auth");
}

#[test]
fn the_protocol_comes_from_the_file_schema() {
    for (schema, protocol) in [
        ("CONFIG_CONTROL_DESIGN", Protocol::Ap203),
        (
            "AP203_CONFIGURATION_CONTROLLED_3D_DESIGN_OF_MECHANICAL_PARTS_AND_ASSEMBLIES_MIM_LF { 1 0 10303 403 1 1 4 }",
            Protocol::Ap203,
        ),
        ("AUTOMOTIVE_DESIGN { 1 0 10303 214 3 1 1 }", Protocol::Ap214),
        // The AP214 committee draft, which KiCad's older models declare.
        (
            "AUTOMOTIVE_DESIGN_CC2 { 1 2 10303 214 -1 1 5 4 }",
            Protocol::Ap214,
        ),
        ("automotive_design", Protocol::Ap214),
        (
            "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }",
            Protocol::Ap242,
        ),
        (
            "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF. {1 0 10303 442 1 1 4 }",
            Protocol::Ap242,
        ),
        ("SOMETHING_ELSE", Protocol::Other("SOMETHING_ELSE".into())),
    ] {
        let text = file(schema, "#1=CARTESIAN_POINT('',(0.,0.,0.));");
        assert_eq!(
            read(text.as_bytes()).unwrap().protocol,
            protocol,
            "{schema}"
        );
    }
}

#[test]
fn string_escapes_decode() {
    // ISO 10303-21 6.4.3: '' is a quote, \\ a backslash, \X2\ UTF-16 code units up to \X0\,
    // \X4\ UCS-4 code points, \X\ one ISO 8859-1 byte, \S\ an upper-half character, and \P?\
    // a code page directive that draws nothing.
    let cases = [
        (r"'it''s'", "it's"),
        (r"'a\\b'", r"a\b"),
        (r"'\X2\00E6\X0\rø'", "ærø"),
        (r"'\X2\00B50394\X0\'", "µΔ"),
        (r"'\X2\D83DDE00\X0\'", "😀"),
        (r"'\X4\0001F600\X0\'", "😀"),
        (r"'caf\X\E9'", "café"),
        (r"'\S\i'", "é"),
        (r"'\PA\x'", "x"),
    ];
    for (raw, want) in cases {
        let text = file(
            "AUTOMOTIVE_DESIGN",
            &format!("#1=CARTESIAN_POINT({raw},(0.,0.,0.));"),
        );
        let model = read(text.as_bytes()).unwrap().model;
        let (_, point) = model.iter::<CartesianPoint>().next().unwrap();
        assert_eq!(point.name, want, "{raw}");
    }
}

#[test]
fn raw_utf8_and_latin1_bytes_in_strings_are_read() {
    let mut text = file(
        "AUTOMOTIVE_DESIGN",
        "#1=CARTESIAN_POINT('\u{e6}\u{394}',(0.,0.,0.));",
    )
    .into_bytes();
    let model = read(&text).unwrap().model;
    assert_eq!(model.iter::<CartesianPoint>().next().unwrap().1.name, "æΔ");

    // A lone 0xE9 is not UTF-8; it is read as ISO 8859-1.
    let at = text.windows(2).position(|w| w == "æ".as_bytes()).unwrap();
    text.splice(at..at + 2, [0xE9]);
    let model = read(&text).unwrap().model;
    assert_eq!(model.iter::<CartesianPoint>().next().unwrap().1.name, "éΔ");
}

#[test]
fn line_breaks_inside_a_string_are_dropped() {
    // Some exporters wrap long strings across lines; Part 21 ignores the break.
    let text = file(
        "AUTOMOTIVE_DESIGN",
        "#1=CARTESIAN_POINT('long\r\n name',(0.,0.,0.));",
    );
    let model = read(text.as_bytes()).unwrap().model;
    assert_eq!(
        model.iter::<CartesianPoint>().next().unwrap().1.name,
        "long name"
    );
}

#[test]
fn comments_whitespace_and_split_records_are_allowed() {
    let text = file(
        "AUTOMOTIVE_DESIGN",
        "/* a comment; with a semicolon */\n#7 =\n  DIRECTION ( 'up' ,\n ( 0. , 0. , 1. ) ) ;\n\
         #8=CARTESIAN_POINT('/* not a comment */',(0.,0.,0.));",
    );
    let model = read(text.as_bytes()).unwrap().model;
    let (_, d) = model.iter::<Direction>().next().unwrap();
    assert_eq!(
        (d.name.as_str(), d.ratios.as_slice()),
        ("up", &[0.0, 0.0, 1.0][..])
    );
    let (_, p) = model.iter::<CartesianPoint>().next().unwrap();
    assert_eq!(p.name, "/* not a comment */");
}

#[test]
fn integers_read_where_reals_are_expected() {
    let text = file("AUTOMOTIVE_DESIGN", "#1=CARTESIAN_POINT('',(1,-2,+3));");
    let model = read(text.as_bytes()).unwrap().model;
    assert_eq!(
        model.iter::<CartesianPoint>().next().unwrap().1.coordinates,
        vec![1.0, -2.0, 3.0]
    );
}

#[test]
fn a_file_that_is_not_part_21_is_refused() {
    for bad in [
        "",
        "solid cube\nendsolid\n",
        "ISO-10303-21;\nHEADER;\n",
        "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=CARTESIAN_POINT('',(0.,0.,0.));\n",
        "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=CARTESIAN_POINT('unterminated,(0.,0.,0.));\nENDSEC;\nEND-ISO-10303-21;\n",
    ] {
        assert!(read(bad.as_bytes()).is_err(), "{bad:?}");
    }
}

#[test]
fn a_malformed_record_is_skipped_with_a_warning() {
    let text = file(
        "AUTOMOTIVE_DESIGN",
        "#1=CARTESIAN_POINT('',(0.,0.,0.));\n#2=CARTESIAN_POINT('',(0.,,0.));\n#3=CARTESIAN_POINT('b',(1.,0.,0.));",
    );
    let read = read(text.as_bytes()).unwrap();
    assert_eq!(read.model.iter::<CartesianPoint>().count(), 2);
    assert!(
        read.warnings.iter().any(|w| w.contains("#2")),
        "{:?}",
        read.warnings
    );
}

#[test]
fn a_duplicate_id_keeps_the_first_record() {
    let text = file(
        "AUTOMOTIVE_DESIGN",
        "#1=CARTESIAN_POINT('first',(0.,0.,0.));\n#1=CARTESIAN_POINT('second',(0.,0.,0.));",
    );
    let read = read(text.as_bytes()).unwrap();
    let names: Vec<_> = read
        .model
        .iter::<CartesianPoint>()
        .map(|(_, p)| p.name.clone())
        .collect();
    assert_eq!(names, ["first"]);
    assert!(
        read.warnings.iter().any(|w| w.contains("#1")),
        "{:?}",
        read.warnings
    );
}

#[test]
fn deep_nesting_is_refused_not_overflowed() {
    let deep = format!(
        "#1=CARTESIAN_POINT('',{}0.{});",
        "(".repeat(100_000),
        ")".repeat(100_000)
    );
    let text = file("AUTOMOTIVE_DESIGN", &deep);
    let read = read(text.as_bytes()).unwrap();
    assert_eq!(read.model.iter::<CartesianPoint>().count(), 0);
    assert!(
        read.warnings.iter().any(|w| w.contains("#1")),
        "{:?}",
        read.warnings
    );
}

#[test]
fn records_nothing_reaches_are_dropped() {
    // Only product structure, representations and styled items stand on their own; a point
    // no representation uses is not part of the model.
    let text = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\nFILE_NAME('','',(''),(''),'','','');\n\
         FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n\
         #1=CARTESIAN_POINT('loose',(0.,0.,0.));\n#2=CARTESIAN_POINT('used',(1.,0.,0.));\n\
         #3=SHAPE_REPRESENTATION('',(#2),#4);\n#4=GEOMETRIC_REPRESENTATION_CONTEXT('','',3);\n\
         #5=APPROVAL_STATUS('approved');\nENDSEC;\nEND-ISO-10303-21;\n";
    let model = read(text.as_bytes()).unwrap().model;
    let names: Vec<_> = model
        .records()
        .iter()
        .map(|r| r.entity_name().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "CARTESIAN_POINT",
            "SHAPE_REPRESENTATION",
            "GEOMETRIC_REPRESENTATION_CONTEXT"
        ]
    );
    assert_eq!(
        model.iter::<CartesianPoint>().next().unwrap().1.name,
        "used"
    );
}

#[test]
fn a_reference_to_a_missing_instance_drops_what_needs_it() {
    // #3's vertex points at a missing #99: the vertex and the edge on it go, and the loop
    // that lists the edge loses just that element.
    let text = file(
        "AUTOMOTIVE_DESIGN",
        "#1=CARTESIAN_POINT('',(0.,0.,0.));\n#2=VERTEX_POINT('',#1);\n#3=VERTEX_POINT('',#99);\n\
         #4=LINE('',#1,#5);\n#5=VECTOR('',#6,1.);\n#6=DIRECTION('',(1.,0.,0.));\n\
         #7=EDGE_CURVE('a',#2,#2,#4,.T.);\n#8=EDGE_CURVE('b',#2,#3,#4,.T.);\n\
         #9=ORIENTED_EDGE('',*,*,#7,.T.);\n#10=ORIENTED_EDGE('',*,*,#8,.T.);\n\
         #11=EDGE_LOOP('',(#9,#10));",
    );
    let read = read(text.as_bytes()).unwrap();
    let model = &read.model;
    let edges: Vec<_> = model
        .iter::<EdgeCurve>()
        .map(|(_, e)| e.name.as_str())
        .collect();
    assert_eq!(edges, ["a"]);
    let (_, l) = model.iter::<EdgeLoop>().next().unwrap();
    assert_eq!(l.edge_list.len(), 1);
    let kept = model.get::<OrientedEdge>(l.edge_list[0]).unwrap();
    assert_eq!(model.get::<EdgeCurve>(kept.edge_element).unwrap().name, "a");
    for id in ["#3 ", "#8 ", "#10 "] {
        assert!(
            read.warnings.iter().any(|w| w.starts_with(id)),
            "{id}: {:?}",
            read.warnings
        );
    }
}

#[test]
fn records_of_unknown_types_survive_as_raw_records() {
    // An unknown entity referenced by a known one is kept, so it can be written back.
    let text = file(
        "AUTOMOTIVE_DESIGN",
        "#1=CARTESIAN_POINT('',(0.,0.,0.));\n#2=FANCY_CURVE('f',#1,(1.,2.),.T.,$,*,LENGTH_MEASURE(2.));\n\
         #3=FANCY_SET('set',(#2));\n#4=SHAPE_REPRESENTATION('sr',(#3),#5);\n\
         #5=GEOMETRIC_REPRESENTATION_CONTEXT('','',3);",
    );
    let model = read(text.as_bytes()).unwrap().model;
    let raw: Vec<_> = model
        .records()
        .iter()
        .filter_map(|r| match r {
            Record::Raw(raw) => Some(raw.parts[0].name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(raw, ["FANCY_CURVE", "FANCY_SET"]);
}
