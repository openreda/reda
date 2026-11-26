//! The records the fixtures read into: product structure, placements, B-rep shapes, colours
//! and units, for AP203, AP214 and AP242 files. The expected values are the ones each fixture
//! was drawn with (tests/fixtures/README.md).

use reda_step::records::*;
use reda_step::{Protocol, Read, RecordId, StepModel, read_file};

fn fixture(name: &str) -> Read {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let read = read_file(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(read.warnings.is_empty(), "{name}: {:?}", read.warnings);
    read
}

fn point(m: &StepModel, id: RecordId) -> Vec<f64> {
    m.get::<CartesianPoint>(id)
        .expect("a point")
        .coordinates
        .clone()
}

fn direction(m: &StepModel, id: Option<RecordId>) -> Vec<f64> {
    m.get::<Direction>(id.expect("a direction"))
        .expect("a direction")
        .ratios
        .clone()
}

fn product_name(m: &StepModel, pd: RecordId) -> String {
    let pd = m.get::<ProductDefinition>(pd).unwrap();
    let pdf = m.get::<ProductDefinitionFormation>(pd.formation).unwrap();
    m.get::<Product>(pdf.of_product).unwrap().name.clone()
}

/// The colour a styled item gives, through its style chain, as RGB or a predefined name.
fn style_colour(m: &StepModel, styles: &[RecordId]) -> Option<Colour> {
    for &psa in styles {
        for &ssu in &m.get::<PresentationStyleAssignment>(psa)?.styles {
            let sss = m.get::<SurfaceStyleUsage>(ssu)?.style;
            for &s in &m.get::<SurfaceSideStyle>(sss)?.styles {
                let Some(fill) = m.get::<SurfaceStyleFillArea>(s) else {
                    continue;
                };
                let fas = m.get::<FillAreaStyle>(fill.fill_area)?;
                for &fasc in &fas.fill_styles {
                    let c = m.get::<FillAreaStyleColour>(fasc)?.fill_colour;
                    if let Some(rgb) = m.get::<ColourRgb>(c) {
                        return Some(Colour::Rgb(rgb.red, rgb.green, rgb.blue));
                    }
                    if let Some(named) = m.get::<DraughtingPreDefinedColour>(c) {
                        return Some(Colour::Named(named.name.clone()));
                    }
                }
            }
        }
    }
    None
}

#[derive(Debug, PartialEq)]
enum Colour {
    Rgb(f64, f64, f64),
    Named(String),
}

#[test]
fn ap214_block_has_six_planar_faces_in_one_closed_shell() {
    let read = fixture("ap214_face_colours.step");
    assert_eq!(read.protocol, Protocol::Ap214);
    let m = &read.model;
    let solids: Vec<_> = m.iter::<SolidBrep>().collect();
    assert_eq!(solids.len(), 1);
    assert_eq!(solids[0].1.kind, SolidKind::Manifold);
    let shell = m.get::<Shell>(solids[0].1.outer).unwrap();
    assert_eq!(shell.kind, ShellKind::Closed);
    assert_eq!(shell.cfs_faces.len(), 6);
    for &f in &shell.cfs_faces {
        let face = m.get::<Face>(f).unwrap();
        assert_eq!(face.kind, FaceKind::Advanced);
        assert!(m.get::<Plane>(face.face_geometry).is_some());
        let [bound] = face.bounds.as_slice() else {
            panic!("one bound")
        };
        let bound = m.get::<FaceBound>(*bound).unwrap();
        assert_eq!(bound.kind, FaceBoundKind::Outer);
        let lp = m.get::<EdgeLoop>(bound.bound).unwrap();
        assert_eq!(lp.edge_list.len(), 4);
        for &oe in &lp.edge_list {
            let oe = m.get::<OrientedEdge>(oe).unwrap();
            let edge = m.get::<EdgeCurve>(oe.edge_element).unwrap();
            assert!(m.get::<Line>(edge.edge_geometry).is_some());
            assert!(m.get::<VertexPoint>(edge.edge_start).is_some());
        }
    }
    // The 12 edges and 8 corners of a block, each corner a vertex.
    assert_eq!(m.iter::<EdgeCurve>().count(), 12);
    let mut corners: Vec<_> = m
        .iter::<VertexPoint>()
        .map(|(_, v)| point(m, v.vertex_geometry))
        .collect();
    corners.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(corners.len(), 8);
    assert_eq!(corners[0], [0.0, 0.0, 0.0]);
    assert_eq!(corners[7], [10.0, 20.0, 5.0]);
}

#[test]
fn per_face_colours_are_read() {
    let m = fixture("ap214_face_colours.step").model;
    let mut by_face: Vec<(String, Colour)> = m
        .iter::<StyledItem>()
        .map(|(_, s)| {
            let face = m.get::<Face>(s.item).expect("styled items colour faces");
            (
                face.name.clone(),
                style_colour(&m, &s.styles).expect("a colour"),
            )
        })
        .collect();
    by_face.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        by_face,
        [
            ("back".to_string(), Colour::Rgb(0.0, 1.0, 0.0)),
            ("bottom".to_string(), Colour::Rgb(0.1, 0.1, 0.1)),
            ("front".to_string(), Colour::Rgb(1.0, 0.0, 0.0)),
            ("left".to_string(), Colour::Rgb(0.0, 0.0, 1.0)),
            ("right".to_string(), Colour::Rgb(1.0, 0.5, 0.0)),
            ("top".to_string(), Colour::Rgb(1.0, 1.0, 1.0)),
        ]
    );
    // The styled items are collected in the presentation representation.
    let (_, pres) = m
        .iter::<Representation>()
        .find(|(_, r)| r.kind == RepresentationKind::MechanicalDesignGeometricPresentation)
        .unwrap();
    assert_eq!(pres.items.len(), 6);
}

#[test]
fn per_solid_colour_and_transparency_are_read_from_ap242() {
    let read = fixture("ap242_solid_colour.step");
    assert_eq!(read.protocol, Protocol::Ap242);
    let m = &read.model;
    let [(_, styled)] = m.iter::<StyledItem>().collect::<Vec<_>>()[..] else {
        panic!("one styled item")
    };
    assert!(m.get::<SolidBrep>(styled.item).is_some());
    assert_eq!(
        style_colour(m, &styled.styles),
        Some(Colour::Rgb(0.8, 0.1, 0.1))
    );
    let (_, rendering) = m
        .iter::<SurfaceStyleRenderingWithProperties>()
        .next()
        .unwrap();
    assert!(rendering.rendering_method.is("NORMAL_SHADING"));
    let t = m
        .get::<SurfaceStyleTransparent>(rendering.properties[0])
        .unwrap();
    assert_eq!(t.transparency, 0.25);
}

#[test]
fn ap242_shape_reaches_its_brep_through_a_relationship() {
    let m = fixture("ap242_solid_colour.step").model;
    let (_, sdr) = m.iter::<ShapeDefinitionRepresentation>().next().unwrap();
    let shape = m.get::<Representation>(sdr.used_representation).unwrap();
    assert_eq!(shape.kind, RepresentationKind::Shape);
    let (_, rel) = m.iter::<ShapeRepresentationRelationship>().next().unwrap();
    assert_eq!(rel.rep_1, sdr.used_representation);
    let brep = m.get::<Representation>(rel.rep_2).unwrap();
    assert_eq!(brep.kind, RepresentationKind::AdvancedBrep);
    assert!(m.get::<SolidBrep>(brep.items[0]).is_some());
    // AP242's product metadata (ID_ATTRIBUTE) is not part of the shape and is not kept.
    assert!(
        m.records()
            .iter()
            .all(|r| r.entity_name() != "ID_ATTRIBUTE")
    );
}

#[test]
fn ap203_contexts_read_as_their_protocol_neutral_records() {
    let read = fixture("ap203_cylinder.step");
    assert_eq!(read.protocol, Protocol::Ap203);
    let m = &read.model;
    let (_, app) = m.iter::<ApplicationContext>().next().unwrap();
    assert_eq!(
        app.application,
        "configuration controlled 3d designs of mechanical parts and assemblies"
    );
    let (_, apd) = m.iter::<ApplicationProtocolDefinition>().next().unwrap();
    assert_eq!(
        (apd.schema_name.as_str(), apd.year),
        ("config_control_design", 1994)
    );
    // MECHANICAL_CONTEXT and DESIGN_CONTEXT are AP203's names for these.
    let (_, pc) = m.iter::<ProductContext>().next().unwrap();
    assert_eq!(pc.discipline_type, "mechanical");
    let (_, pdc) = m.iter::<ProductDefinitionContext>().next().unwrap();
    assert_eq!(pdc.life_cycle_stage, "design");
    let (_, pdf) = m
        .iter::<ProductDefinitionFormationWithSpecifiedSource>()
        .next()
        .unwrap();
    assert!(pdf.make_or_buy.is("MADE"));
    assert_eq!(m.get::<Product>(pdf.of_product).unwrap().name, "cylinder");
    // The configuration-management records (approvals, people, dates, security) say nothing
    // about the shape and are not kept.
    for r in m.records() {
        let name = r.entity_name();
        assert!(
            !name.starts_with("CC_DESIGN")
                && !name.contains("APPROVAL")
                && !name.starts_with("PERSON")
                && !name.contains("DATE")
                && !name.contains("SECURITY"),
            "{name}"
        );
    }
}

#[test]
fn ap203_cylinder_has_circles_a_cylinder_and_a_seam_with_pcurves() {
    let m = fixture("ap203_cylinder.step").model;
    let (_, cyl) = m.iter::<CylindricalSurface>().next().unwrap();
    assert_eq!(cyl.radius, 2.0);
    let circles: Vec<_> = m.iter::<Circle>().map(|(_, c)| c.radius).collect();
    assert_eq!(circles, [2.0, 2.0]);
    let (_, seam) = m.iter::<SurfaceCurve>().next().unwrap();
    assert_eq!(seam.kind, SurfaceCurveKind::Seam);
    assert!(m.get::<Line>(seam.curve_3d).is_some());
    assert_eq!(seam.associated_geometry.len(), 2);
    let mut us = Vec::new();
    for &pc in &seam.associated_geometry {
        let pc = m.get::<Pcurve>(pc).unwrap();
        assert!(m.get::<CylindricalSurface>(pc.basis_surface).is_some());
        let def = m.get::<Representation>(pc.reference_to_curve).unwrap();
        assert_eq!(def.kind, RepresentationKind::Definitional);
        let ctx = m.get::<RepresentationContext>(def.context).unwrap();
        assert!(ctx.parametric);
        assert_eq!(ctx.dimension, Some(2));
        let line = m.get::<Line>(def.items[0]).unwrap();
        us.push(point(&m, line.pnt)[0]);
    }
    assert_eq!(us, [0.0, std::f64::consts::TAU]);
    // The side face is bounded by one loop: bottom circle, seam up, top circle, seam down.
    let side = m.iter::<Face>().find(|(_, f)| f.name == "side").unwrap().1;
    let bound = m.get::<FaceBound>(side.bounds[0]).unwrap();
    let senses: Vec<bool> = m
        .get::<EdgeLoop>(bound.bound)
        .unwrap()
        .edge_list
        .iter()
        .map(|&oe| m.get::<OrientedEdge>(oe).unwrap().orientation)
        .collect();
    assert_eq!(senses, [true, true, false, false]);
}

#[test]
fn units_and_tolerance_are_read() {
    let m = fixture("inch_block.step").model;
    let (_, rep) = m.iter::<Representation>().next().unwrap();
    let ctx = m.get::<RepresentationContext>(rep.context).unwrap();
    assert_eq!(ctx.dimension, Some(3));
    let units: Vec<&Unit> = ctx
        .units
        .as_ref()
        .unwrap()
        .iter()
        .map(|&u| m.get::<Unit>(u).unwrap())
        .collect();
    assert_eq!(units.len(), 3);

    assert_eq!(units[0].kind, Some(UnitKind::Length));
    let UnitDefinition::ConversionBased {
        name,
        conversion_factor,
    } = &units[0].definition
    else {
        panic!("{:?}", units[0])
    };
    assert_eq!(name, "INCH");
    let factor = m.get::<MeasureWithUnit>(*conversion_factor).unwrap();
    assert_eq!(
        (factor.kind, factor.value.value),
        (MeasureKind::Length, 25.4)
    );
    let mm = m.get::<Unit>(factor.unit).unwrap();
    assert_eq!(*mm, Unit::si(UnitKind::Length, Some("MILLI"), "METRE"));
    let dims = m
        .get::<DimensionalExponents>(units[0].dimensions.unwrap())
        .unwrap();
    assert_eq!(dims.length, 1.0);

    assert_eq!(units[1].kind, Some(UnitKind::PlaneAngle));
    let UnitDefinition::ConversionBased { name, .. } = &units[1].definition else {
        panic!()
    };
    assert_eq!(name, "DEGREE");
    assert_eq!(*units[2], Unit::si(UnitKind::SolidAngle, None, "STERADIAN"));

    let unc = m
        .get::<UncertaintyMeasureWithUnit>(ctx.uncertainty.as_ref().unwrap()[0])
        .unwrap();
    assert_eq!(unc.value, Measure::new("LENGTH_MEASURE", 1e-5));
    assert_eq!(unc.name, "distance_accuracy_value");
}

/// The root of an assembly: the product definition no occurrence uses as a child.
fn root(m: &StepModel) -> RecordId {
    let children: Vec<_> = m
        .iter::<NextAssemblyUsageOccurrence>()
        .map(|(_, n)| n.related_product_definition)
        .collect();
    let roots: Vec<_> = m
        .iter::<ProductDefinition>()
        .map(|(id, _)| id)
        .filter(|id| !children.contains(id))
        .collect();
    assert_eq!(roots.len(), 1, "{roots:?}");
    roots[0]
}

fn children(m: &StepModel, parent: RecordId) -> Vec<(String, String)> {
    m.iter::<NextAssemblyUsageOccurrence>()
        .filter(|(_, n)| n.relating_product_definition == parent)
        .map(|(_, n)| {
            (
                n.name.clone(),
                product_name(m, n.related_product_definition),
            )
        })
        .collect()
}

#[test]
fn a_nested_assembly_keeps_its_tree() {
    let m = fixture("nested_assembly.step").model;
    let root = root(&m);
    assert_eq!(product_name(&m, root), "board-assy");
    let top = children(&m, root);
    assert_eq!(
        top,
        [
            ("module:1".to_string(), "module".to_string()),
            ("pin:3".to_string(), "pin".to_string()),
            ("cap:1".to_string(), "cap".to_string()),
        ]
    );
    let module = m
        .iter::<NextAssemblyUsageOccurrence>()
        .find(|(_, n)| n.name == "module:1")
        .unwrap()
        .1
        .related_product_definition;
    assert_eq!(
        children(&m, module),
        [
            ("pin:1".to_string(), "pin".to_string()),
            ("pin:2".to_string(), "pin".to_string())
        ]
    );
    // Both pin occurrences in the module and the one on the board share one definition.
    let pins: Vec<_> = m
        .iter::<NextAssemblyUsageOccurrence>()
        .filter(|(_, n)| n.name.starts_with("pin"))
        .map(|(_, n)| n.related_product_definition)
        .collect();
    assert_eq!(pins.len(), 3);
    assert!(pins.iter().all(|&p| p == pins[0]));
}

/// The placement of occurrence `name` in its parent: location, axis and reference direction.
fn placement(m: &StepModel, name: &str) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let (nauo, _) = m
        .iter::<NextAssemblyUsageOccurrence>()
        .find(|(_, n)| n.name == name)
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
    // The child's own frame is the identity; the parent's placement says where it goes.
    let from = m.get::<Axis2Placement3d>(idt.transform_item_1).unwrap();
    assert_eq!(point(m, from.location), [0.0, 0.0, 0.0]);
    let to = m.get::<Axis2Placement3d>(idt.transform_item_2).unwrap();
    let parent = m.get::<Representation>(rel.rep_2).unwrap();
    assert!(parent.items.contains(&idt.transform_item_2));
    (
        point(m, to.location),
        direction(m, to.axis),
        direction(m, to.ref_direction),
    )
}

#[test]
fn assembly_placements_are_read() {
    let m = fixture("nested_assembly.step").model;
    assert_eq!(
        placement(&m, "module:1"),
        (
            vec![10.0, 0.0, 1.6],
            vec![0.0, 0.0, 1.0],
            vec![0.0, 1.0, 0.0]
        )
    );
    assert_eq!(
        placement(&m, "pin:2"),
        (
            vec![2.54, 0.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![1.0, 0.0, 0.0]
        )
    );
    assert_eq!(
        placement(&m, "cap:1"),
        (
            vec![0.0, -5.0, 0.0],
            vec![0.0, 0.0, -1.0],
            vec![1.0, 0.0, 0.0]
        )
    );
}

#[test]
fn assembly_parts_carry_per_solid_colours() {
    let m = fixture("nested_assembly.step").model;
    let mut colours: Vec<(String, Colour)> = m
        .iter::<StyledItem>()
        .map(|(_, s)| {
            let (rep, _) = m
                .iter::<Representation>()
                .find(|(_, r)| {
                    r.items.contains(&s.item) && r.kind == RepresentationKind::AdvancedBrep
                })
                .unwrap();
            let name = m.get::<Representation>(rep).unwrap().name.clone();
            (name, style_colour(&m, &s.styles).unwrap())
        })
        .collect();
    colours.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        colours,
        [
            ("cap".to_string(), Colour::Rgb(0.2, 0.2, 0.8)),
            ("pin".to_string(), Colour::Named("yellow".into()))
        ]
    );
}
