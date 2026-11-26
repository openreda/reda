//! Converting a model to millimetres and radians.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

use reda_step::records::*;
use reda_step::{RecordId, StepModel, read, read_file, write};

fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn fixture(name: &str) -> StepModel {
    read_file(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
    .model
}

fn close(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (x - y).abs() <= 1e-9 * (1.0 + y.abs()))
}

fn corners(m: &StepModel) -> (Vec<f64>, Vec<f64>) {
    let mut lo = vec![f64::MAX; 3];
    let mut hi = vec![f64::MIN; 3];
    for (_, v) in m.iter::<VertexPoint>() {
        let p = &m
            .get::<CartesianPoint>(v.vertex_geometry)
            .unwrap()
            .coordinates;
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

fn units_of(m: &StepModel, ctx: RecordId) -> Vec<Unit> {
    let ctx = m.get::<RepresentationContext>(ctx).unwrap();
    ctx.units
        .as_ref()
        .unwrap()
        .iter()
        .map(|&u| m.get::<Unit>(u).unwrap().clone())
        .collect()
}

#[test]
fn a_model_in_millimetres_is_left_as_it_is() {
    for name in [
        "ap214_face_colours.step",
        "ap203_cylinder.step",
        "nested_assembly.step",
    ] {
        let mut m = fixture(name);
        let before = m.clone();
        let report = m.to_millimetres();
        assert!(report.warnings.is_empty(), "{name}: {:?}", report.warnings);
        assert_eq!(m, before, "{name}");
    }
}

#[test]
fn an_inch_model_is_scaled_to_millimetres() {
    let mut m = fixture("inch_block.step");
    assert_eq!(corners(&m), (vec![0.0; 3], vec![1.0, 0.5, 0.25]));
    let report = m.to_millimetres();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let (lo, hi) = corners(&m);
    assert_eq!(lo, [0.0; 3]);
    assert!(close(&hi, &[25.4, 12.7, 6.35]), "{hi:?}");
    // Every edge's line runs at the edge's length per unit parameter, now in millimetres.
    for (_, v) in m.iter::<Vector>() {
        assert!(close(&[v.magnitude], &[25.4]), "{v:?}");
    }
    // The context now says millimetres and radians, and its tolerance is in millimetres.
    let (_, rep) = m.iter::<Representation>().next().unwrap();
    assert_eq!(
        units_of(&m, rep.context),
        [
            Unit::si(UnitKind::Length, Some("MILLI"), "METRE"),
            Unit::si(UnitKind::PlaneAngle, None, "RADIAN"),
            Unit::si(UnitKind::SolidAngle, None, "STERADIAN"),
        ]
    );
    let ctx = m.get::<RepresentationContext>(rep.context).unwrap();
    let unc = m
        .get::<UncertaintyMeasureWithUnit>(ctx.uncertainty.as_ref().unwrap()[0])
        .unwrap();
    assert!(close(&[unc.value.value], &[2.54e-4]), "{unc:?}");
    assert_eq!(
        m.get::<Unit>(unc.unit).unwrap(),
        &Unit::si(UnitKind::Length, Some("MILLI"), "METRE")
    );
    // The inch and degree definitions are no longer used, and are gone.
    assert!(
        m.iter::<Unit>()
            .all(|(_, u)| matches!(u.definition, UnitDefinition::Si { .. }))
    );
    assert_eq!(m.iter::<MeasureWithUnit>().count(), 0);
    // A converted model writes and reads back as it is.
    assert_eq!(read(write(&m).as_bytes()).unwrap().model, m);
}

#[test]
fn an_assembly_in_metres_is_scaled_with_its_placements() {
    // The nested assembly with its length unit made the metre: every length is a thousand
    // times larger in millimetres, placements included.
    let text = fixture_text("nested_assembly.step")
        .replace("SI_UNIT(.MILLI.,.METRE.)", "SI_UNIT($,.METRE.)");
    let mut m = read(text.as_bytes()).unwrap().model;
    let report = m.to_millimetres();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let (nauo, _) = m
        .iter::<NextAssemblyUsageOccurrence>()
        .find(|(_, n)| n.name == "module:1")
        .unwrap();
    let (pds, _) = m
        .iter::<ProductDefinitionShape>()
        .find(|(_, s)| s.definition == nauo)
        .unwrap();
    let (_, cdsr) = m
        .iter::<ContextDependentShapeRepresentation>()
        .find(|(_, c)| c.represented_product_relation == pds)
        .unwrap();
    let rel = m
        .get::<ShapeRepresentationTransformation>(cdsr.representation_relation)
        .unwrap();
    let idt = m
        .get::<ItemDefinedTransformation>(rel.transformation_operator)
        .unwrap();
    let to = m.get::<Axis2Placement3d>(idt.transform_item_2).unwrap();
    let loc = &m.get::<CartesianPoint>(to.location).unwrap().coordinates;
    assert!(close(loc, &[10_000.0, 0.0, 1_600.0]), "{loc:?}");
    let dir = &m
        .get::<Direction>(to.ref_direction.unwrap())
        .unwrap()
        .ratios;
    assert_eq!(dir, &[0.0, 1.0, 0.0]);
    let (lo, hi) = corners(&m);
    assert!(
        close(&lo, &[-500.0, -500.0, 0.0]) && close(&hi, &[500.0, 500.0, 3000.0]),
        "{lo:?} {hi:?}"
    );
    let (_, unc) = m.iter::<UncertaintyMeasureWithUnit>().next().unwrap();
    assert!(close(&[unc.value.value], &[1e-4]), "{unc:?}");
}

/// A file in one context with `length` and `angle` units, holding `data` in a shape.
fn in_units(length: &str, angle: &str, data: &str, items: &str) -> StepModel {
    let text = format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\nFILE_NAME('','',(''),(''),'','','');\n\
         FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n\
         #1=( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );\n\
         #2=( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) );\n\
         #3=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#1);\n\
         #4=( CONVERSION_BASED_UNIT('INCH',#3) LENGTH_UNIT() NAMED_UNIT(#6) );\n\
         #5=PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(0.0174532925199433),#2);\n\
         #6=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);\n\
         #7=( CONVERSION_BASED_UNIT('DEGREE',#5) NAMED_UNIT(#8) PLANE_ANGLE_UNIT() );\n\
         #8=DIMENSIONAL_EXPONENTS(0.,0.,0.,0.,0.,0.,0.);\n\
         #9=( GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNIT_ASSIGNED_CONTEXT(({length},{angle})) \
         REPRESENTATION_CONTEXT('','') );\n\
         #10=SHAPE_REPRESENTATION('',({items}),#9);\n\
         #11=( GEOMETRIC_REPRESENTATION_CONTEXT(2) PARAMETRIC_REPRESENTATION_CONTEXT() REPRESENTATION_CONTEXT('2D SPACE','') );\n\
         {data}\nENDSEC;\nEND-ISO-10303-21;\n"
    );
    let read = read(text.as_bytes()).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    read.model
}

const FRAME: &str = "#20=CARTESIAN_POINT('',(0.,0.,0.));\n#21=DIRECTION('',(0.,0.,1.));\n\
    #22=DIRECTION('',(1.,0.,0.));\n#23=AXIS2_PLACEMENT_3D('',#20,#21,#22);";

#[test]
fn angles_in_degrees_become_radians() {
    let data = format!(
        "{FRAME}\n#30=CONICAL_SURFACE('',#23,2.,45.);\n#31=CIRCLE('',#23,3.);\n\
         #32=TRIMMED_CURVE('',#31,(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(90.)),.T.,.PARAMETER.);"
    );
    let mut m = in_units("#1", "#7", &data, "#30,#32");
    let report = m.to_millimetres();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let (_, cone) = m.iter::<ConicalSurface>().next().unwrap();
    assert_eq!(cone.radius, 2.0);
    assert!(close(&[cone.semi_angle], &[FRAC_PI_4]), "{cone:?}");
    let (_, c) = m.iter::<Circle>().next().unwrap();
    assert_eq!(c.radius, 3.0);
    let (_, t) = m.iter::<TrimmedCurve>().next().unwrap();
    let [Trim::Parameter(a)] = t.trim_1[..] else {
        panic!()
    };
    let [Trim::Parameter(b)] = t.trim_2[..] else {
        panic!()
    };
    assert!(close(&[a, b], &[0.0, FRAC_PI_2]), "{t:?}");
}

#[test]
fn line_parameters_stay_as_they_are() {
    // A line's parameter counts lengths of its vector, so scaling the vector is enough.
    let data = format!(
        "{FRAME}\n#30=VECTOR('',#22,1.);\n#31=LINE('',#20,#30);\n\
         #32=TRIMMED_CURVE('',#31,(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(2.)),.T.,.PARAMETER.);"
    );
    let mut m = in_units("#4", "#2", &data, "#32");
    m.to_millimetres();
    let (_, t) = m.iter::<TrimmedCurve>().next().unwrap();
    assert_eq!(t.trim_2, [Trim::Parameter(2.0)]);
    let (_, v) = m.iter::<Vector>().next().unwrap();
    assert!(close(&[v.magnitude], &[25.4]));
}

#[test]
fn pcurves_scale_with_their_surface_parameters() {
    // On a plane both parameters are lengths; on a cylinder u is an angle and v a length.
    let data = format!(
        "{FRAME}\n#30=PLANE('',#23);\n#31=CYLINDRICAL_SURFACE('',#23,1.);\n\
         #40=CARTESIAN_POINT('',(1.,2.));\n#41=DIRECTION('',(1.,0.));\n#42=VECTOR('',#41,1.);\n\
         #43=LINE('',#40,#42);\n#44=DEFINITIONAL_REPRESENTATION('',(#43),#11);\n#45=PCURVE('',#30,#44);\n\
         #50=CARTESIAN_POINT('',(1.,2.));\n#51=DIRECTION('',(0.6,0.8));\n#52=VECTOR('',#51,1.);\n\
         #53=LINE('',#50,#52);\n#54=DEFINITIONAL_REPRESENTATION('',(#53),#11);\n#55=PCURVE('',#31,#54);\n\
         #61=VECTOR('',#21,1.);\n#62=LINE('',#20,#61);\n#60=SURFACE_CURVE('',#62,(#45,#55),.CURVE_3D.);"
    );
    let mut m = in_units("#4", "#2", &data, "#60");
    let report = m.to_millimetres();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let p = |id| m.get::<CartesianPoint>(id).unwrap().coordinates.clone();
    let lines: Vec<&Line> = m
        .iter::<Line>()
        .map(|(_, l)| l)
        .filter(|l| p(l.pnt).len() == 2)
        .collect();
    assert_eq!(lines.len(), 2);
    let v = |id| m.get::<Vector>(id).unwrap().clone();
    let d = |id| m.get::<Direction>(id).unwrap().ratios.clone();

    // On the plane: (1, 2) in becomes (25.4, 50.8) mm, along u at 25.4 mm per parameter.
    assert!(close(&p(lines[0].pnt), &[25.4, 50.8]));
    let v0 = v(lines[0].dir);
    assert!(close(&[v0.magnitude], &[25.4]) && close(&d(v0.orientation), &[1.0, 0.0]));

    // On the cylinder: u stays an angle, v becomes 50.8 mm; the direction (0.6, 0.8) in
    // (rad, in) is (0.6, 20.32) in (rad, mm).
    assert!(close(&p(lines[1].pnt), &[1.0, 50.8]));
    let v1 = v(lines[1].dir);
    let len = (0.6f64 * 0.6 + 20.32 * 20.32).sqrt();
    assert!(close(&[v1.magnitude], &[len]), "{v1:?}");
    assert!(close(&d(v1.orientation), &[0.6 / len, 20.32 / len]));
    // The 3D cylinder radius and line are lengths too.
    assert!(close(
        &[m.iter::<CylindricalSurface>().next().unwrap().1.radius],
        &[25.4]
    ));
    let (_, sc) = m.iter::<SurfaceCurve>().next().unwrap();
    let line = m.get::<Line>(sc.curve_3d).unwrap();
    assert!(close(&[v(line.dir).magnitude], &[25.4]));
}

#[test]
fn geometry_reda_has_no_type_for_is_reported_when_it_cannot_be_converted() {
    let data = format!("{FRAME}\n#30=OFFSET_SURFACE('',#31,0.5,.F.);\n#31=PLANE('',#23);");
    let mut m = in_units("#4", "#2", &data, "#30");
    let report = m.to_millimetres();
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("OFFSET_SURFACE"),
        "{:?}",
        report.warnings
    );
    // What lies under it is still converted.
    let (_, plane) = m.iter::<Plane>().next().unwrap();
    let frame = m.get::<Axis2Placement3d>(plane.position).unwrap();
    assert_eq!(
        m.get::<CartesianPoint>(frame.location).unwrap().coordinates,
        [0.0, 0.0, 0.0]
    );
}
