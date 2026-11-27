//! Unit conversion: a model's lengths to millimetres and its angles to radians.
//!
//! Each geometric context says what its lengths and plane angles are measured in. Every
//! geometry and topology record reached from a representation's items (or from an assembly
//! placement) is scaled by its context's factors, the context's units are replaced by the
//! millimetre and the radian, and the unit records left unused are pruned.
//!
//! What is a length: point coordinates, vector magnitudes, radii and semi-axes. Plane angles:
//! a cone's semi-angle, and trim parameters on circles and ellipses. A line's parameter counts
//! lengths of its vector and stays as it is. A pcurve's 2D curve lives in its surface's
//! parameter space, where each of `u` and `v` is a length, an angle or a plain number by the
//! surface type, and is scaled axis by axis.

use std::collections::HashMap;

use crate::StepModel;
use crate::records::*;

/// What [`StepModel::to_millimetres`] could not convert.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitReport {
    /// One line per record left in its file's units, or per unit REDA does not know.
    pub warnings: Vec<String>,
}

/// Millimetres per length unit and radians per plane-angle unit.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Factors {
    length: f64,
    angle: f64,
}

const ONE: Factors = Factors {
    length: 1.0,
    angle: 1.0,
};

#[derive(Clone, Copy, PartialEq)]
enum Quantity {
    Length,
    Angle,
}

impl StepModel {
    /// Converts the model's lengths to millimetres and its plane angles to radians, and makes
    /// every context say so. A model already in millimetres and radians is left as it is.
    pub fn to_millimetres(&mut self) -> UnitReport {
        let mut report = UnitReport::default();
        let mut context_factors = HashMap::new();
        for (id, ctx) in self.iter::<RepresentationContext>() {
            if let Some(units) = &ctx.units {
                let f = self.context_factors(units, &mut report);
                if f != ONE {
                    context_factors.insert(id, f);
                }
            }
        }
        if context_factors.is_empty() {
            return report;
        }
        let factors_of = |ctx: RecordId| context_factors.get(&ctx).copied().unwrap_or(ONE);

        // Seeds: each representation's items in its context, and each assembly placement in
        // the context of the representation it belongs to.
        let mut seeds = Vec::new();
        for (_, rep) in self.iter::<Representation>() {
            if rep.kind != RepresentationKind::Definitional {
                seeds.extend(rep.items.iter().map(|&i| (i, factors_of(rep.context))));
            }
        }
        for (_, rel) in self.iter::<ShapeRepresentationTransformation>() {
            let ctx = |rep| {
                self.get::<Representation>(rep)
                    .map(|r| factors_of(r.context))
            };
            if let Some(idt) = self.get::<ItemDefinedTransformation>(rel.transformation_operator) {
                if let Some(f) = ctx(rel.rep_1) {
                    seeds.push((idt.transform_item_1, f));
                }
                if let Some(f) = ctx(rel.rep_2) {
                    seeds.push((idt.transform_item_2, f));
                }
            }
        }
        let scale3 = self.spread(seeds, &mut report);

        // Pcurves: their 2D curves scale by what the basis surface's parameters measure.
        let mut seeds2 = Vec::new();
        for (&id, f) in &scale3 {
            let Some(pc) = self.get::<Pcurve>(id) else {
                continue;
            };
            let Some((su, sv)) = self.parameter_factors(pc.basis_surface, *f) else {
                report.warnings.push(format!(
                    "#{}: the pcurve's surface ({}) has parameters REDA cannot scale; left as it is",
                    id.index() + 1,
                    self.record(pc.basis_surface).entity_name()
                ));
                continue;
            };
            if (su, sv) != (1.0, 1.0)
                && let Some(def) = self.get::<Representation>(pc.reference_to_curve)
            {
                seeds2.extend(def.items.iter().map(|&i| {
                    (
                        i,
                        Factors {
                            length: su,
                            angle: sv,
                        },
                    )
                }));
            }
        }
        // In 2D, `length` carries the u factor and `angle` the v factor.
        let scale2 = self.spread(seeds2, &mut report);

        self.apply_3d(&scale3, &mut report);
        self.apply_2d(&scale2, &mut report);
        self.rewrite_contexts(&context_factors, &mut report);
        self.prune();
        report
    }

    /// The factors of a context's units; a unit REDA cannot resolve counts as 1 and is noted.
    fn context_factors(&self, units: &[RecordId], report: &mut UnitReport) -> Factors {
        let mut f = ONE;
        for &u in units {
            match self.unit_factor(u, 0) {
                Some((Quantity::Length, v)) => f.length = v,
                Some((Quantity::Angle, v)) => f.angle = v,
                None => {
                    if let Some(unit) = self.get::<Unit>(u)
                        && matches!(unit.kind, Some(UnitKind::Length | UnitKind::PlaneAngle))
                    {
                        report.warnings.push(format!(
                            "#{}: a unit REDA does not know; taken as 1",
                            u.index() + 1
                        ));
                    }
                }
            }
        }
        f
    }

    /// Millimetres (for a length unit) or radians (for a plane-angle unit) per unit.
    fn unit_factor(&self, id: RecordId, depth: usize) -> Option<(Quantity, f64)> {
        if depth > 8 {
            return None;
        }
        let unit = self.get::<Unit>(id)?;
        match &unit.definition {
            UnitDefinition::Si { prefix, name } => {
                let p = match prefix.as_ref().map(|p| p.0.as_str()) {
                    None => 1.0,
                    Some(p) => si_prefix(p)?,
                };
                match name.0.as_str() {
                    "METRE" => Some((Quantity::Length, p * 1000.0)),
                    "RADIAN" => Some((Quantity::Angle, p)),
                    _ => None,
                }
            }
            UnitDefinition::ConversionBased {
                conversion_factor, ..
            } => {
                let m = self.get::<MeasureWithUnit>(*conversion_factor)?;
                let (q, base) = self.unit_factor(m.unit, depth + 1)?;
                let v = m.value.value * base;
                (v.is_finite() && v > 0.0).then_some((q, v))
            }
        }
    }

    /// Walks geometry and topology from `seeds`, giving each record reached its seed's
    /// factors. A record reached with two different factors keeps the first and is noted.
    fn spread(
        &self,
        seeds: Vec<(RecordId, Factors)>,
        report: &mut UnitReport,
    ) -> HashMap<RecordId, Factors> {
        let mut out: HashMap<RecordId, Factors> = HashMap::new();
        let mut stack = seeds;
        while let Some((id, f)) = stack.pop() {
            if id.index() >= self.records().len() || !is_geometry(self.record(id)) {
                continue;
            }
            match out.get(&id) {
                Some(g) if *g != f => {
                    report.warnings.push(format!(
                        "#{} ({}) is used in contexts with different units; kept in the first",
                        id.index() + 1,
                        self.record(id).entity_name()
                    ));
                    continue;
                }
                Some(_) => continue,
                None => {}
            }
            out.insert(id, f);
            match self.record(id) {
                // The mapped representation keeps the units of its own context; only the
                // target placement is in this one.
                Record::MappedItem(m) => stack.push((m.mapping_target, f)),
                r => r.refs(&mut |r| stack.push((r, f))),
            }
        }
        out
    }

    /// What a surface's (u, v) parameters measure, as factors.
    fn parameter_factors(&self, surface: RecordId, f: Factors) -> Option<(f64, f64)> {
        let (l, a) = (f.length, f.angle);
        let curve = |c: RecordId| match self.record(c) {
            Record::Circle(_) | Record::Ellipse(_) => a,
            _ => 1.0,
        };
        Some(match self.record(surface) {
            Record::Plane(_) => (l, l),
            Record::CylindricalSurface(_) | Record::ConicalSurface(_) => (a, l),
            Record::SphericalSurface(_)
            | Record::ToroidalSurface(_)
            | Record::DegenerateToroidalSurface(_) => (a, a),
            Record::BSplineSurface(_) => (1.0, 1.0),
            // σ(u, v) = curve(u) + v * axis, the axis a vector with its length.
            Record::SurfaceOfLinearExtrusion(s) => (curve(s.swept_curve), 1.0),
            // σ(u, v) = curve(v) turned by u about the axis.
            Record::SurfaceOfRevolution(s) => (a, curve(s.swept_curve)),
            _ => return None,
        })
    }

    fn apply_3d(&mut self, scale: &HashMap<RecordId, Factors>, report: &mut UnitReport) {
        // Trim parameters are angles on circles and ellipses; look the basis up first.
        let angle_trims: HashMap<RecordId, bool> = scale
            .keys()
            .filter_map(|&id| {
                let t = self.get::<TrimmedCurve>(id)?;
                Some((
                    id,
                    matches!(
                        self.record(t.basis_curve),
                        Record::Circle(_) | Record::Ellipse(_)
                    ),
                ))
            })
            .collect();
        let mut ids: Vec<_> = scale.iter().filter(|(_, f)| **f != ONE).collect();
        ids.sort_by_key(|(id, _)| **id);
        for (&id, f) in ids {
            let (l, a) = (f.length, f.angle);
            match self.record_mut(id) {
                Record::CartesianPoint(p) => p.coordinates.iter_mut().for_each(|c| *c *= l),
                Record::Vector(v) => v.magnitude *= l,
                Record::Circle(c) => c.radius *= l,
                Record::Ellipse(e) => {
                    e.semi_axis_1 *= l;
                    e.semi_axis_2 *= l;
                }
                // Their parameters are plain numbers, so trims on them stay as they are.
                Record::Parabola(p) => p.focal_dist *= l,
                Record::Hyperbola(h) => {
                    h.semi_axis *= l;
                    h.semi_imag_axis *= l;
                }
                Record::CylindricalSurface(s) => s.radius *= l,
                Record::ConicalSurface(s) => {
                    s.radius *= l;
                    s.semi_angle *= a;
                }
                Record::SphericalSurface(s) => s.radius *= l,
                Record::ToroidalSurface(s) => {
                    s.major_radius *= l;
                    s.minor_radius *= l;
                }
                Record::DegenerateToroidalSurface(s) => {
                    s.major_radius *= l;
                    s.minor_radius *= l;
                }
                Record::TrimmedCurve(t) => {
                    if angle_trims.get(&id) == Some(&true) {
                        for trim in t.trim_1.iter_mut().chain(t.trim_2.iter_mut()) {
                            if let Trim::Parameter(v) = trim {
                                *v *= a;
                            }
                        }
                    }
                }
                Record::Raw(r) => report.warnings.push(format!(
                    "#{} ({}) is geometry REDA has no type for; left in the file's units",
                    id.index() + 1,
                    r.parts.first().map_or("", |p| p.name.as_str())
                )),
                _ => {}
            }
        }
    }

    fn apply_2d(&mut self, scale: &HashMap<RecordId, Factors>, report: &mut UnitReport) {
        let stretch = |d: &[f64], su: f64, sv: f64| -> Vec<f64> {
            match d {
                [x, y] => vec![x * su, y * sv],
                other => other.to_vec(),
            }
        };
        let norm = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
        // A vector's length changes by how much its direction stretches; read the directions
        // before they are changed.
        let vector_stretch: HashMap<RecordId, f64> = scale
            .iter()
            .filter_map(|(&id, f)| {
                let v = self.get::<Vector>(id)?;
                let d = self.get::<Direction>(v.orientation)?;
                let n0 = norm(&d.ratios);
                (n0 > 0.0).then(|| (id, norm(&stretch(&d.ratios, f.length, f.angle)) / n0))
            })
            .collect();
        let mut ids: Vec<_> = scale.iter().filter(|(_, f)| **f != ONE).collect();
        ids.sort_by_key(|(id, _)| **id);
        for (&id, f) in ids {
            let (su, sv) = (f.length, f.angle);
            match self.record_mut(id) {
                Record::CartesianPoint(p) => p.coordinates = stretch(&p.coordinates, su, sv),
                Record::Direction(d) => {
                    let s = stretch(&d.ratios, su, sv);
                    let n = norm(&s);
                    if n > 0.0 {
                        d.ratios = s.iter().map(|x| x / n).collect();
                    }
                }
                Record::Vector(v) => v.magnitude *= vector_stretch.get(&id).copied().unwrap_or(1.0),
                Record::Circle(c) if su == sv => c.radius *= su,
                Record::Ellipse(e) if su == sv => {
                    e.semi_axis_1 *= su;
                    e.semi_axis_2 *= su;
                }
                r @ (Record::Circle(_) | Record::Ellipse(_) | Record::Raw(_)) => {
                    report.warnings.push(format!(
                        "#{} ({}) in a pcurve cannot be scaled by ({su}, {sv}); left as it is",
                        id.index() + 1,
                        r.entity_name()
                    ))
                }
                _ => {}
            }
        }
    }

    /// Points every converted context at millimetres and radians, and converts tolerances.
    fn rewrite_contexts(&mut self, contexts: &HashMap<RecordId, Factors>, report: &mut UnitReport) {
        let mm = self.add(Unit::si(UnitKind::Length, Some("MILLI"), "METRE"));
        let rad = self.add(Unit::si(UnitKind::PlaneAngle, None, "RADIAN"));
        let mut done = Vec::new();
        let mut ids: Vec<_> = contexts.keys().copied().collect();
        ids.sort();
        for ctx in ids {
            let c = self
                .get::<RepresentationContext>(ctx)
                .expect("a context")
                .clone();
            let units: Vec<RecordId> = c
                .units
                .unwrap_or_default()
                .into_iter()
                .map(|u| match self.unit_factor(u, 0) {
                    Some((Quantity::Length, _)) => mm,
                    Some((Quantity::Angle, _)) => rad,
                    None => u,
                })
                .collect();
            for &unc in c.uncertainty.iter().flatten() {
                if done.contains(&unc) {
                    continue;
                }
                done.push(unc);
                let Some(m) = self.get::<UncertaintyMeasureWithUnit>(unc) else {
                    continue;
                };
                match self.unit_factor(m.unit, 0) {
                    Some((Quantity::Length, v)) => {
                        let m = self
                            .get_mut::<UncertaintyMeasureWithUnit>(unc)
                            .expect("checked");
                        m.value.value *= v;
                        m.unit = mm;
                    }
                    Some((Quantity::Angle, v)) => {
                        let m = self
                            .get_mut::<UncertaintyMeasureWithUnit>(unc)
                            .expect("checked");
                        m.value.value *= v;
                        m.unit = rad;
                    }
                    None => report.warnings.push(format!(
                        "#{}: a tolerance in a unit REDA does not know",
                        unc.index() + 1
                    )),
                }
            }
            self.get_mut::<RepresentationContext>(ctx)
                .expect("a context")
                .units = Some(units);
        }
    }
}

fn si_prefix(p: &str) -> Option<f64> {
    Some(match p {
        "EXA" => 1e18,
        "PETA" => 1e15,
        "TERA" => 1e12,
        "GIGA" => 1e9,
        "MEGA" => 1e6,
        "KILO" => 1e3,
        "HECTO" => 1e2,
        "DECA" => 1e1,
        "DECI" => 1e-1,
        "CENTI" => 1e-2,
        "MILLI" => 1e-3,
        "MICRO" => 1e-6,
        "NANO" => 1e-9,
        "PICO" => 1e-12,
        "FEMTO" => 1e-15,
        "ATTO" => 1e-18,
        _ => return None,
    })
}

/// Geometry and topology: the records whose numbers are measured in a context's units, and
/// the records between them. Entities REDA has no type for are walked through too, except
/// property values (`*_REPRESENTATION_ITEM`), which are not geometry.
fn is_geometry(r: &Record) -> bool {
    matches!(
        r,
        Record::CartesianPoint(_)
            | Record::Direction(_)
            | Record::Vector(_)
            | Record::Axis1Placement(_)
            | Record::Axis2Placement2d(_)
            | Record::Axis2Placement3d(_)
            | Record::Line(_)
            | Record::Circle(_)
            | Record::Ellipse(_)
            | Record::Parabola(_)
            | Record::Hyperbola(_)
            | Record::Polyline(_)
            | Record::CompositeCurve(_)
            | Record::CompositeCurveSegment(_)
            | Record::BSplineCurve(_)
            | Record::TrimmedCurve(_)
            | Record::SurfaceCurve(_)
            | Record::Pcurve(_)
            | Record::Plane(_)
            | Record::CylindricalSurface(_)
            | Record::ConicalSurface(_)
            | Record::SphericalSurface(_)
            | Record::ToroidalSurface(_)
            | Record::DegenerateToroidalSurface(_)
            | Record::SurfaceOfRevolution(_)
            | Record::SurfaceOfLinearExtrusion(_)
            | Record::BSplineSurface(_)
            | Record::GeometricSet(_)
            | Record::VertexPoint(_)
            | Record::EdgeCurve(_)
            | Record::OrientedEdge(_)
            | Record::EdgeLoop(_)
            | Record::VertexLoop(_)
            | Record::PolyLoop(_)
            | Record::FaceBound(_)
            | Record::Face(_)
            | Record::Shell(_)
            | Record::OrientedClosedShell(_)
            | Record::SolidBrep(_)
            | Record::BrepWithVoids(_)
            | Record::ShellBasedSurfaceModel(_)
            | Record::MappedItem(_)
    ) || matches!(r, Record::Raw(raw) if !raw.parts.iter().any(|p| p.name.ends_with("REPRESENTATION_ITEM")))
}
