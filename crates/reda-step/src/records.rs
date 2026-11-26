//! The typed records of a STEP model: product structure, placements, B-rep shapes, styles and
//! units.
//!
//! Each record type mirrors one entity (or one fixed complex instance) of the STEP integrated
//! resources, field for field in EXPRESS order, so that writing a record gives back the entity
//! it was read from. A reference to another record is a [`RecordId`]. An entity REDA has no
//! type for is kept as a [`RawRecord`] when a typed record refers to it, so it can still be
//! written back.
//!
//! Records are protocol-neutral: the AP203-only `MECHANICAL_CONTEXT` and `DESIGN_CONTEXT` read
//! as [`ProductContext`] and [`ProductDefinitionContext`], and the protocol a file declares
//! lives only in [`ApplicationContext`] and [`ApplicationProtocolDefinition`].

mod complex;
mod defs;
mod fields;

use std::collections::HashMap;
use std::sync::OnceLock;

pub use complex::{
    BSplineCurve, BSplineSurface, RepresentationContext, ShapeRepresentationTransformation, Unit,
    UnitDefinition, UnitKind,
};
pub use defs::*;
pub use fields::{Derived, EnumValue, Logical, Measure, Trim};

use crate::part21::{Param, Part};
use fields::Field;

/// The index of a record in its [`StepModel`](crate::StepModel). Written as `#(index + 1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecordId(pub(crate) u32);

impl RecordId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub(crate) fn from_index(i: usize) -> Self {
        RecordId(u32::try_from(i).expect("fewer than 2^32 records"))
    }
}

/// What every record type does: read from and write to Part 21, and walk its references.
pub(crate) trait Entity: Sized {
    /// The simple-instance entity names this type reads; the first is the one it writes.
    const NAMES: &'static [&'static str];

    /// Reads a simple instance whose parameters have their references already resolved to
    /// record indices. `None` when the parameters do not fit.
    fn read(name: &str, params: &[Param]) -> Option<Self>;

    /// Reads a complex instance. Only the types that stand for one do.
    fn read_complex(_parts: &[Part]) -> Option<Self> {
        None
    }

    /// Writes the instance body after `#n=` (no `;`).
    fn write(&self, out: &mut String);

    /// The entity name written for this record.
    fn entity_name(&self) -> &'static str;

    fn refs(&self, f: &mut dyn FnMut(RecordId));
    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId));
}

/// A record type that is one variant of [`Record`], for typed lookups such as
/// [`StepModel::iter`](crate::StepModel::iter).
pub trait RecordKind: Sized + 'static {
    fn of(record: &Record) -> Option<&Self>;
    fn of_mut(record: &mut Record) -> Option<&mut Self>;
}

/// An entity REDA keeps without a type of its own, its references resolved to record indices
/// (`Param::Ref(n)` is `RecordId` index `n`).
#[derive(Clone, Debug, PartialEq)]
pub struct RawRecord {
    /// One part for a simple instance; the parts in order for a complex one.
    pub parts: Vec<Part>,
    pub complex: bool,
}

impl RawRecord {
    fn write(&self, out: &mut String) {
        if self.complex {
            out.push('(');
        }
        for part in &self.parts {
            out.push_str(&part.name);
            out.push('(');
            for (i, p) in part.params.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                crate::part21::encode_param(p, 1, out);
            }
            out.push(')');
        }
        if self.complex {
            out.push(')');
        }
    }

    fn refs(&self, f: &mut dyn FnMut(RecordId)) {
        fn walk(p: &Param, f: &mut dyn FnMut(RecordId)) {
            match p {
                Param::Ref(r) => f(RecordId(*r as u32)),
                Param::List(items) => items.iter().for_each(|i| walk(i, f)),
                Param::Typed(_, inner) => walk(inner, f),
                _ => {}
            }
        }
        self.parts
            .iter()
            .flat_map(|p| &p.params)
            .for_each(|p| walk(p, f));
    }

    fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
        fn walk(p: &mut Param, f: &mut dyn FnMut(&mut RecordId)) {
            match p {
                Param::Ref(r) => {
                    let mut id = RecordId(*r as u32);
                    f(&mut id);
                    *r = u64::from(id.0);
                }
                Param::List(items) => items.iter_mut().for_each(|i| walk(i, f)),
                Param::Typed(_, inner) => walk(inner, f),
                _ => {}
            }
        }
        self.parts
            .iter_mut()
            .flat_map(|p| &mut p.params)
            .for_each(|p| walk(p, f));
    }
}

type SimpleReader = fn(&str, &[Param]) -> Option<Record>;

macro_rules! record_enum {
    ($($v:ident),* $(,)?) => {
        /// One record of a STEP model.
        #[derive(Clone, Debug, PartialEq)]
        pub enum Record {
            $($v($v),)*
            Raw(RawRecord),
        }

        impl Record {
            /// The entity name the record is written as (for a raw complex record, its first
            /// part's).
            pub fn entity_name(&self) -> &str {
                match self {
                    $(Record::$v(r) => r.entity_name(),)*
                    Record::Raw(r) => r.parts.first().map_or("", |p| p.name.as_str()),
                }
            }

            /// Calls `f` with every record this one refers to.
            pub fn refs(&self, f: &mut dyn FnMut(RecordId)) {
                match self {
                    $(Record::$v(r) => r.refs(f),)*
                    Record::Raw(r) => r.refs(f),
                }
            }

            /// Calls `f` with every reference, so it can be changed.
            pub fn refs_mut(&mut self, f: &mut dyn FnMut(&mut RecordId)) {
                match self {
                    $(Record::$v(r) => r.refs_mut(f),)*
                    Record::Raw(r) => r.refs_mut(f),
                }
            }

            pub(crate) fn write(&self, out: &mut String) {
                match self {
                    $(Record::$v(r) => r.write(out),)*
                    Record::Raw(r) => r.write(out),
                }
            }

            pub(crate) fn read_complex(parts: &[Part]) -> Option<Record> {
                $(if let Some(r) = <$v as Entity>::read_complex(parts) {
                    return Some(Record::$v(r));
                })*
                None
            }
        }

        fn simple_readers() -> HashMap<&'static str, SimpleReader> {
            let mut m: HashMap<&'static str, SimpleReader> = HashMap::new();
            $(for name in <$v as Entity>::NAMES {
                m.insert(name, |name, params| <$v as Entity>::read(name, params).map(Record::$v));
            })*
            m
        }

        $(
            impl RecordKind for $v {
                fn of(record: &Record) -> Option<&Self> {
                    if let Record::$v(r) = record { Some(r) } else { None }
                }
                fn of_mut(record: &mut Record) -> Option<&mut Self> {
                    if let Record::$v(r) = record { Some(r) } else { None }
                }
            }

            impl From<$v> for Record {
                fn from(r: $v) -> Record {
                    Record::$v(r)
                }
            }
        )*
    };
}

record_enum! {
    // Product structure
    ApplicationContext,
    ApplicationProtocolDefinition,
    ProductContext,
    Product,
    ProductRelatedProductCategory,
    ProductDefinitionFormation,
    ProductDefinitionFormationWithSpecifiedSource,
    ProductDefinitionContext,
    ProductDefinition,
    ProductDefinitionShape,
    ShapeDefinitionRepresentation,
    NextAssemblyUsageOccurrence,
    ContextDependentShapeRepresentation,
    // Representations and placements between them
    Representation,
    RepresentationContext,
    ShapeRepresentationRelationship,
    ShapeRepresentationTransformation,
    ItemDefinedTransformation,
    // Units
    Unit,
    DimensionalExponents,
    MeasureWithUnit,
    UncertaintyMeasureWithUnit,
    // Geometry
    CartesianPoint,
    Direction,
    Vector,
    Axis1Placement,
    Axis2Placement2d,
    Axis2Placement3d,
    Line,
    Circle,
    Ellipse,
    Polyline,
    CompositeCurve,
    CompositeCurveSegment,
    BSplineCurve,
    TrimmedCurve,
    SurfaceCurve,
    Pcurve,
    Plane,
    CylindricalSurface,
    ConicalSurface,
    SphericalSurface,
    ToroidalSurface,
    DegenerateToroidalSurface,
    SurfaceOfRevolution,
    SurfaceOfLinearExtrusion,
    BSplineSurface,
    GeometricSet,
    // Topology
    VertexPoint,
    EdgeCurve,
    OrientedEdge,
    EdgeLoop,
    VertexLoop,
    PolyLoop,
    FaceBound,
    Face,
    Shell,
    OrientedClosedShell,
    SolidBrep,
    BrepWithVoids,
    ShellBasedSurfaceModel,
    // Styles
    StyledItem,
    OverRidingStyledItem,
    ContextDependentOverRidingStyledItem,
    PresentationStyleAssignment,
    SurfaceStyleUsage,
    SurfaceSideStyle,
    SurfaceStyleFillArea,
    FillAreaStyle,
    FillAreaStyleColour,
    ColourRgb,
    DraughtingPreDefinedColour,
    SurfaceStyleRendering,
    SurfaceStyleRenderingWithProperties,
    SurfaceStyleTransparent,
    CurveStyle,
    DraughtingPreDefinedCurveFont,
}

impl Record {
    /// Reads one instance, its references already resolved to record indices: a typed record
    /// when REDA knows the entity and its parameters fit, a raw one otherwise.
    pub(crate) fn from_instance(parts: Vec<Part>, complex: bool) -> Record {
        static READERS: OnceLock<HashMap<&'static str, SimpleReader>> = OnceLock::new();
        let typed = if complex {
            Record::read_complex(&parts).or_else(|| {
                MeasureWithUnit::read_complex_parts(&parts).map(Record::MeasureWithUnit)
            })
        } else {
            let part = &parts[0];
            READERS
                .get_or_init(simple_readers)
                .get(part.name.as_str())
                .and_then(|read| read(&part.name, &part.params))
        };
        typed.unwrap_or(Record::Raw(RawRecord { parts, complex }))
    }

    /// Whether the record stands on its own in a model: product structure, representations
    /// and styled items. Every other record is kept only while one of these reaches it.
    pub(crate) fn is_root(&self) -> bool {
        matches!(
            self,
            Record::ApplicationProtocolDefinition(_)
                | Record::Product(_)
                | Record::ProductRelatedProductCategory(_)
                | Record::ProductDefinition(_)
                | Record::ShapeDefinitionRepresentation(_)
                | Record::NextAssemblyUsageOccurrence(_)
                | Record::ContextDependentShapeRepresentation(_)
                | Record::Representation(_)
                | Record::ShapeRepresentationRelationship(_)
                | Record::ShapeRepresentationTransformation(_)
                | Record::StyledItem(_)
                | Record::OverRidingStyledItem(_)
                | Record::ContextDependentOverRidingStyledItem(_)
        )
    }
}

/// Writes `fields` as a comma-separated parameter list.
pub(crate) fn write_fields(out: &mut String, fields: &[&dyn Field]) {
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        f.write(out);
    }
}
