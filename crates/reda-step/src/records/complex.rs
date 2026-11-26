//! The records that are complex instances in the file (`#n=(A(..)B(..));`): rational
//! B-splines, geometric contexts with their units, units, and the placement of a child shape.
//! Each also reads the simple form where the entity has one.

use std::collections::HashMap;

use super::fields::{EnumValue, Field, Logical};
use super::{Entity, RecordId};
use crate::part21::{Param, Part};

/// The parts of a complex instance by name, refusing names outside `allowed`.
fn parts_by_name<'a>(parts: &'a [Part], allowed: &[&str]) -> Option<HashMap<&'a str, &'a [Param]>> {
    let mut map = HashMap::new();
    for part in parts {
        if !allowed.contains(&part.name.as_str()) {
            return None;
        }
        if map
            .insert(part.name.as_str(), part.params.as_slice())
            .is_some()
        {
            return None;
        }
    }
    Some(map)
}

/// An optional part's value: `Some(None)` when the part is absent, `None` when it does not fit.
fn optional<T>(p: Option<&Param>, read: fn(&Param) -> Option<T>) -> Option<Option<T>> {
    match p {
        None => Some(None),
        Some(p) => read(p).map(Some),
    }
}

/// Reads exactly `N` parameters.
fn params<const N: usize>(p: &[Param]) -> Option<&[Param; N]> {
    p.try_into().ok()
}

/// Writes a complex instance from `(name, params)` parts, which Part 21 wants in alphabetical
/// order.
fn write_complex(out: &mut String, mut parts: Vec<(&str, String)>) {
    parts.sort_by(|a, b| a.0.cmp(b.0));
    out.push('(');
    for (name, params) in parts {
        out.push_str(name);
        out.push('(');
        out.push_str(&params);
        out.push(')');
    }
    out.push(')');
}

fn fields(fields: &[&dyn Field]) -> String {
    let mut s = String::new();
    super::write_fields(&mut s, fields);
    s
}

// ---------------------------------------------------------------------------------------------

/// A B-spline curve with knots, rational when it has weights.
#[derive(Clone, Debug, PartialEq)]
pub struct BSplineCurve {
    pub name: String,
    pub degree: i64,
    pub control_points_list: Vec<RecordId>,
    pub curve_form: EnumValue,
    pub closed_curve: Logical,
    pub self_intersect: Logical,
    pub knot_multiplicities: Vec<i64>,
    pub knots: Vec<f64>,
    pub knot_spec: EnumValue,
    /// One weight per control point, for a rational curve.
    pub weights: Option<Vec<f64>>,
}

impl BSplineCurve {
    fn read_body(
        name: &Param,
        curve: &[Param],
        knots: &[Param],
        weights: Option<&[Param]>,
    ) -> Option<Self> {
        let [degree, cps, form, closed, self_int] = params::<5>(curve)?;
        let [mults, ks, spec] = params::<3>(knots)?;
        let weights = match weights {
            Some(w) => Some(Vec::<f64>::read(&params::<1>(w)?[0])?),
            None => None,
        };
        Some(BSplineCurve {
            name: String::read(name)?,
            degree: i64::read(degree)?,
            control_points_list: Vec::read(cps)?,
            curve_form: EnumValue::read(form)?,
            closed_curve: Logical::read(closed)?,
            self_intersect: Logical::read(self_int)?,
            knot_multiplicities: Vec::read(mults)?,
            knots: Vec::read(ks)?,
            knot_spec: EnumValue::read(spec)?,
            weights,
        })
    }
}

impl Entity for BSplineCurve {
    const NAMES: &'static [&'static str] = &["B_SPLINE_CURVE_WITH_KNOTS"];

    fn read(_name: &str, p: &[Param]) -> Option<Self> {
        if p.len() != 9 {
            return None;
        }
        Self::read_body(&p[0], &p[1..6], &p[6..9], None)
    }

    fn read_complex(parts: &[Part]) -> Option<Self> {
        let map = parts_by_name(
            parts,
            &[
                "BOUNDED_CURVE",
                "B_SPLINE_CURVE",
                "B_SPLINE_CURVE_WITH_KNOTS",
                "CURVE",
                "GEOMETRIC_REPRESENTATION_ITEM",
                "RATIONAL_B_SPLINE_CURVE",
                "REPRESENTATION_ITEM",
            ],
        )?;
        let name = map
            .get("REPRESENTATION_ITEM")
            .map_or(Some(&Param::Unset), |p| p.first())?;
        Self::read_body(
            name,
            map.get("B_SPLINE_CURVE")?,
            map.get("B_SPLINE_CURVE_WITH_KNOTS")?,
            map.get("RATIONAL_B_SPLINE_CURVE").copied(),
        )
    }

    fn write(&self, out: &mut String) {
        let curve = [
            &self.degree as &dyn Field,
            &self.control_points_list,
            &self.curve_form,
            &self.closed_curve,
            &self.self_intersect,
        ];
        let knots = [
            &self.knot_multiplicities as &dyn Field,
            &self.knots,
            &self.knot_spec,
        ];
        match &self.weights {
            None => {
                out.push_str("B_SPLINE_CURVE_WITH_KNOTS(");
                let mut all: Vec<&dyn Field> = vec![&self.name];
                all.extend(curve);
                all.extend(knots);
                super::write_fields(out, &all);
                out.push(')');
            }
            Some(w) => write_complex(
                out,
                vec![
                    ("BOUNDED_CURVE", String::new()),
                    ("B_SPLINE_CURVE", fields(&curve)),
                    ("B_SPLINE_CURVE_WITH_KNOTS", fields(&knots)),
                    ("CURVE", String::new()),
                    ("GEOMETRIC_REPRESENTATION_ITEM", String::new()),
                    ("RATIONAL_B_SPLINE_CURVE", fields(&[w])),
                    ("REPRESENTATION_ITEM", fields(&[&self.name])),
                ],
            ),
        }
    }

    fn entity_name(&self) -> &'static str {
        if self.weights.is_some() {
            "RATIONAL_B_SPLINE_CURVE"
        } else {
            "B_SPLINE_CURVE_WITH_KNOTS"
        }
    }

    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        self.control_points_list.refs(f);
    }

    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        self.control_points_list.refs_mut(f);
    }
}

// ---------------------------------------------------------------------------------------------

/// A B-spline surface with knots, rational when it has weights.
#[derive(Clone, Debug, PartialEq)]
pub struct BSplineSurface {
    pub name: String,
    pub u_degree: i64,
    pub v_degree: i64,
    /// Rows of control points, one row per `u`.
    pub control_points_list: Vec<Vec<RecordId>>,
    pub surface_form: EnumValue,
    pub u_closed: Logical,
    pub v_closed: Logical,
    pub self_intersect: Logical,
    pub u_multiplicities: Vec<i64>,
    pub v_multiplicities: Vec<i64>,
    pub u_knots: Vec<f64>,
    pub v_knots: Vec<f64>,
    pub knot_spec: EnumValue,
    /// One weight per control point, for a rational surface.
    pub weights: Option<Vec<Vec<f64>>>,
}

impl BSplineSurface {
    fn read_body(
        name: &Param,
        surface: &[Param],
        knots: &[Param],
        weights: Option<&[Param]>,
    ) -> Option<Self> {
        let [ud, vd, cps, form, uc, vc, si] = params::<7>(surface)?;
        let [um, vm, uk, vk, spec] = params::<5>(knots)?;
        let weights = match weights {
            Some(w) => Some(Vec::<Vec<f64>>::read(&params::<1>(w)?[0])?),
            None => None,
        };
        Some(BSplineSurface {
            name: String::read(name)?,
            u_degree: i64::read(ud)?,
            v_degree: i64::read(vd)?,
            control_points_list: Vec::read(cps)?,
            surface_form: EnumValue::read(form)?,
            u_closed: Logical::read(uc)?,
            v_closed: Logical::read(vc)?,
            self_intersect: Logical::read(si)?,
            u_multiplicities: Vec::read(um)?,
            v_multiplicities: Vec::read(vm)?,
            u_knots: Vec::read(uk)?,
            v_knots: Vec::read(vk)?,
            knot_spec: EnumValue::read(spec)?,
            weights,
        })
    }
}

impl Entity for BSplineSurface {
    const NAMES: &'static [&'static str] = &["B_SPLINE_SURFACE_WITH_KNOTS"];

    fn read(_name: &str, p: &[Param]) -> Option<Self> {
        if p.len() != 13 {
            return None;
        }
        Self::read_body(&p[0], &p[1..8], &p[8..13], None)
    }

    fn read_complex(parts: &[Part]) -> Option<Self> {
        let map = parts_by_name(
            parts,
            &[
                "BOUNDED_SURFACE",
                "B_SPLINE_SURFACE",
                "B_SPLINE_SURFACE_WITH_KNOTS",
                "GEOMETRIC_REPRESENTATION_ITEM",
                "RATIONAL_B_SPLINE_SURFACE",
                "REPRESENTATION_ITEM",
                "SURFACE",
            ],
        )?;
        let name = map
            .get("REPRESENTATION_ITEM")
            .map_or(Some(&Param::Unset), |p| p.first())?;
        Self::read_body(
            name,
            map.get("B_SPLINE_SURFACE")?,
            map.get("B_SPLINE_SURFACE_WITH_KNOTS")?,
            map.get("RATIONAL_B_SPLINE_SURFACE").copied(),
        )
    }

    fn write(&self, out: &mut String) {
        let surface = [
            &self.u_degree as &dyn Field,
            &self.v_degree,
            &self.control_points_list,
            &self.surface_form,
            &self.u_closed,
            &self.v_closed,
            &self.self_intersect,
        ];
        let knots = [
            &self.u_multiplicities as &dyn Field,
            &self.v_multiplicities,
            &self.u_knots,
            &self.v_knots,
            &self.knot_spec,
        ];
        match &self.weights {
            None => {
                out.push_str("B_SPLINE_SURFACE_WITH_KNOTS(");
                let mut all: Vec<&dyn Field> = vec![&self.name];
                all.extend(surface);
                all.extend(knots);
                super::write_fields(out, &all);
                out.push(')');
            }
            Some(w) => write_complex(
                out,
                vec![
                    ("BOUNDED_SURFACE", String::new()),
                    ("B_SPLINE_SURFACE", fields(&surface)),
                    ("B_SPLINE_SURFACE_WITH_KNOTS", fields(&knots)),
                    ("GEOMETRIC_REPRESENTATION_ITEM", String::new()),
                    ("RATIONAL_B_SPLINE_SURFACE", fields(&[w])),
                    ("REPRESENTATION_ITEM", fields(&[&self.name])),
                    ("SURFACE", String::new()),
                ],
            ),
        }
    }

    fn entity_name(&self) -> &'static str {
        if self.weights.is_some() {
            "RATIONAL_B_SPLINE_SURFACE"
        } else {
            "B_SPLINE_SURFACE_WITH_KNOTS"
        }
    }

    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        self.control_points_list.refs(f);
    }

    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        self.control_points_list.refs_mut(f);
    }
}

// ---------------------------------------------------------------------------------------------

/// A representation context: in a model's shapes, the 3D space with its units and
/// tolerance; in a pcurve, a surface's 2D parameter space.
#[derive(Clone, Debug, PartialEq)]
pub struct RepresentationContext {
    pub context_identifier: String,
    pub context_type: String,
    /// The coordinate space dimension of a geometric context.
    pub dimension: Option<i64>,
    /// A surface's parameter space (`PARAMETRIC_REPRESENTATION_CONTEXT`).
    pub parametric: bool,
    /// The context's tolerances (`GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT`).
    pub uncertainty: Option<Vec<RecordId>>,
    /// The context's units (`GLOBAL_UNIT_ASSIGNED_CONTEXT`), each a [`Unit`].
    pub units: Option<Vec<RecordId>>,
}

impl Entity for RepresentationContext {
    const NAMES: &'static [&'static str] =
        &["GEOMETRIC_REPRESENTATION_CONTEXT", "REPRESENTATION_CONTEXT"];

    fn read(name: &str, p: &[Param]) -> Option<Self> {
        let dimension = match (name, p.len()) {
            ("REPRESENTATION_CONTEXT", 2) => None,
            ("GEOMETRIC_REPRESENTATION_CONTEXT", 3) => Some(i64::read(&p[2])?),
            _ => return None,
        };
        Some(RepresentationContext {
            context_identifier: String::read(&p[0])?,
            context_type: String::read(&p[1])?,
            dimension,
            parametric: false,
            uncertainty: None,
            units: None,
        })
    }

    fn read_complex(parts: &[Part]) -> Option<Self> {
        let map = parts_by_name(
            parts,
            &[
                "GEOMETRIC_REPRESENTATION_CONTEXT",
                "GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT",
                "GLOBAL_UNIT_ASSIGNED_CONTEXT",
                "PARAMETRIC_REPRESENTATION_CONTEXT",
                "REPRESENTATION_CONTEXT",
            ],
        )?;
        let [id, ty] = params::<2>(map.get("REPRESENTATION_CONTEXT")?)?;
        let one = |name: &str| -> Option<Option<&Param>> {
            match map.get(name) {
                None => Some(None),
                Some(p) => Some(Some(&params::<1>(p)?[0])),
            }
        };
        let parametric = match map.get("PARAMETRIC_REPRESENTATION_CONTEXT") {
            None => false,
            Some([]) => true,
            Some(_) => return None,
        };
        Some(RepresentationContext {
            context_identifier: String::read(id)?,
            context_type: String::read(ty)?,
            dimension: optional(one("GEOMETRIC_REPRESENTATION_CONTEXT")?, i64::read)?,
            parametric,
            uncertainty: optional(one("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT")?, Vec::read)?,
            units: optional(one("GLOBAL_UNIT_ASSIGNED_CONTEXT")?, Vec::read)?,
        })
    }

    fn write(&self, out: &mut String) {
        let base = fields(&[&self.context_identifier, &self.context_type]);
        if !self.parametric && self.uncertainty.is_none() && self.units.is_none() {
            match self.dimension {
                Some(d) => {
                    out.push_str("GEOMETRIC_REPRESENTATION_CONTEXT(");
                    out.push_str(&base);
                    out.push(',');
                    d.write(out);
                    out.push(')');
                }
                None => {
                    out.push_str("REPRESENTATION_CONTEXT(");
                    out.push_str(&base);
                    out.push(')');
                }
            }
            return;
        }
        let mut parts = vec![("REPRESENTATION_CONTEXT", base)];
        if let Some(d) = self.dimension {
            parts.push(("GEOMETRIC_REPRESENTATION_CONTEXT", fields(&[&d])));
        }
        if self.parametric {
            parts.push(("PARAMETRIC_REPRESENTATION_CONTEXT", String::new()));
        }
        if let Some(u) = &self.uncertainty {
            parts.push(("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT", fields(&[u])));
        }
        if let Some(u) = &self.units {
            parts.push(("GLOBAL_UNIT_ASSIGNED_CONTEXT", fields(&[u])));
        }
        write_complex(out, parts);
    }

    fn entity_name(&self) -> &'static str {
        if self.dimension.is_some() {
            "GEOMETRIC_REPRESENTATION_CONTEXT"
        } else {
            "REPRESENTATION_CONTEXT"
        }
    }

    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        self.uncertainty.refs(f);
        self.units.refs(f);
    }

    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        self.uncertainty.refs_mut(f);
        self.units.refs_mut(f);
    }
}

// ---------------------------------------------------------------------------------------------

/// What a [`Unit`] measures.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UnitKind {
    Length,
    PlaneAngle,
    SolidAngle,
    /// Another `*_UNIT` entity, by name.
    Other(String),
}

impl UnitKind {
    fn from_entity(name: &str) -> Option<UnitKind> {
        Some(match name {
            "LENGTH_UNIT" => UnitKind::Length,
            "PLANE_ANGLE_UNIT" => UnitKind::PlaneAngle,
            "SOLID_ANGLE_UNIT" => UnitKind::SolidAngle,
            n if n.ends_with("_UNIT")
                && !matches!(n, "NAMED_UNIT" | "SI_UNIT" | "CONVERSION_BASED_UNIT") =>
            {
                UnitKind::Other(n.to_string())
            }
            _ => return None,
        })
    }

    fn entity_name(&self) -> &str {
        match self {
            UnitKind::Length => "LENGTH_UNIT",
            UnitKind::PlaneAngle => "PLANE_ANGLE_UNIT",
            UnitKind::SolidAngle => "SOLID_ANGLE_UNIT",
            UnitKind::Other(n) => n,
        }
    }
}

/// How a [`Unit`] is defined.
#[derive(Clone, Debug, PartialEq)]
pub enum UnitDefinition {
    /// An SI unit with an optional prefix: `.MILLI.` `.METRE.`, `$` `.RADIAN.`.
    Si {
        prefix: Option<EnumValue>,
        name: EnumValue,
    },
    /// A unit defined as a multiple of another, such as `'INCH'` as 25.4 mm: the factor is a
    /// [`MeasureWithUnit`](super::MeasureWithUnit).
    ConversionBased {
        name: String,
        conversion_factor: RecordId,
    },
}

/// A named unit (`NAMED_UNIT` with `SI_UNIT` or `CONVERSION_BASED_UNIT` and its kind).
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub kind: Option<UnitKind>,
    /// The unit's [`DimensionalExponents`](super::DimensionalExponents); `None` (`*`) for an
    /// SI unit, whose dimensions follow from its name.
    pub dimensions: Option<RecordId>,
    pub definition: UnitDefinition,
}

impl Unit {
    /// An SI unit with no dimensions record, such as `Unit::si(UnitKind::Length,
    /// Some("MILLI"), "METRE")`.
    pub fn si(kind: UnitKind, prefix: Option<&str>, name: &str) -> Unit {
        Unit {
            kind: Some(kind),
            dimensions: None,
            definition: UnitDefinition::Si {
                prefix: prefix.map(EnumValue::new),
                name: EnumValue::new(name),
            },
        }
    }
}

impl Entity for Unit {
    const NAMES: &'static [&'static str] = &[];

    fn read(_: &str, _: &[Param]) -> Option<Self> {
        None
    }

    fn read_complex(parts: &[Part]) -> Option<Self> {
        let mut named = None;
        let mut definition = None;
        let mut kind = None;
        for part in parts {
            let p = part.params.as_slice();
            match part.name.as_str() {
                "NAMED_UNIT" => named = Some(&params::<1>(p)?[0]),
                "SI_UNIT" => {
                    let [prefix, name] = params::<2>(p)?;
                    definition = Some(UnitDefinition::Si {
                        prefix: Option::<EnumValue>::read(prefix)?,
                        name: EnumValue::read(name)?,
                    });
                }
                "CONVERSION_BASED_UNIT" => {
                    let [name, factor] = params::<2>(p)?;
                    definition = Some(UnitDefinition::ConversionBased {
                        name: String::read(name)?,
                        conversion_factor: RecordId::read(factor)?,
                    });
                }
                other if p.is_empty() && kind.is_none() => {
                    kind = Some(UnitKind::from_entity(other)?)
                }
                _ => return None,
            }
        }
        let dimensions = match named? {
            Param::Derived | Param::Unset => None,
            p => Some(RecordId::read(p)?),
        };
        Some(Unit {
            kind,
            dimensions,
            definition: definition?,
        })
    }

    fn write(&self, out: &mut String) {
        let mut dims = String::new();
        match self.dimensions {
            Some(d) => d.write(&mut dims),
            None => dims.push('*'),
        }
        let mut parts = vec![("NAMED_UNIT", dims)];
        match &self.definition {
            UnitDefinition::Si { prefix, name } => parts.push(("SI_UNIT", fields(&[prefix, name]))),
            UnitDefinition::ConversionBased {
                name,
                conversion_factor,
            } => parts.push(("CONVERSION_BASED_UNIT", fields(&[name, conversion_factor]))),
        }
        if let Some(kind) = &self.kind {
            parts.push((kind.entity_name(), String::new()));
        }
        write_complex(out, parts);
    }

    fn entity_name(&self) -> &'static str {
        match self.definition {
            UnitDefinition::Si { .. } => "SI_UNIT",
            UnitDefinition::ConversionBased { .. } => "CONVERSION_BASED_UNIT",
        }
    }

    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        self.dimensions.refs(f);
        if let UnitDefinition::ConversionBased {
            conversion_factor, ..
        } = &self.definition
        {
            f(*conversion_factor);
        }
    }

    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        self.dimensions.refs_mut(f);
        if let UnitDefinition::ConversionBased {
            conversion_factor, ..
        } = &mut self.definition
        {
            f(conversion_factor);
        }
    }
}

// ---------------------------------------------------------------------------------------------

/// Places representation `rep_1` (the child's shape) in `rep_2` (the parent's) through an
/// [`ItemDefinedTransformation`](super::ItemDefinedTransformation) from a placement in
/// `rep_1` to one in `rep_2`: the complex `REPRESENTATION_RELATIONSHIP` /
/// `..._WITH_TRANSFORMATION` / `SHAPE_REPRESENTATION_RELATIONSHIP` instance of an assembly.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeRepresentationTransformation {
    pub name: String,
    pub description: String,
    pub rep_1: RecordId,
    pub rep_2: RecordId,
    pub transformation_operator: RecordId,
}

impl Entity for ShapeRepresentationTransformation {
    const NAMES: &'static [&'static str] = &[];

    fn read(_: &str, _: &[Param]) -> Option<Self> {
        None
    }

    fn read_complex(parts: &[Part]) -> Option<Self> {
        let map = parts_by_name(
            parts,
            &[
                "REPRESENTATION_RELATIONSHIP",
                "REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION",
                "SHAPE_REPRESENTATION_RELATIONSHIP",
            ],
        )?;
        let [name, description, rep_1, rep_2] =
            params::<4>(map.get("REPRESENTATION_RELATIONSHIP")?)?;
        let [op] = params::<1>(map.get("REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION")?)?;
        if !map.get("SHAPE_REPRESENTATION_RELATIONSHIP")?.is_empty() {
            return None;
        }
        Some(ShapeRepresentationTransformation {
            name: String::read(name)?,
            description: String::read(description)?,
            rep_1: RecordId::read(rep_1)?,
            rep_2: RecordId::read(rep_2)?,
            transformation_operator: RecordId::read(op)?,
        })
    }

    fn write(&self, out: &mut String) {
        write_complex(
            out,
            vec![
                (
                    "REPRESENTATION_RELATIONSHIP",
                    fields(&[&self.name, &self.description, &self.rep_1, &self.rep_2]),
                ),
                (
                    "REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION",
                    fields(&[&self.transformation_operator]),
                ),
                ("SHAPE_REPRESENTATION_RELATIONSHIP", String::new()),
            ],
        );
    }

    fn entity_name(&self) -> &'static str {
        "REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION"
    }

    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        f(self.rep_1);
        f(self.rep_2);
        f(self.transformation_operator);
    }

    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        f(&mut self.rep_1);
        f(&mut self.rep_2);
        f(&mut self.transformation_operator);
    }
}
