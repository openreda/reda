//! The records that are one simple entity instance each, field for field in EXPRESS order.

use super::fields::{Derived, EnumValue, Field, Logical, Measure, Trim};
use super::{Entity, RecordId};
use crate::part21::Param;

/// Defines a record struct and its [`Entity`] impl. `Name = "ENTITY" | "ALIAS" { fields }`
/// reads the entity and its aliases and writes the first name; `Name [Kind { Variant =
/// "ENTITY", .. }] { fields }` gives the struct a `kind` field for entities that differ only
/// in name.
macro_rules! entity {
    (
        $(#[$m:meta])*
        $ty:ident = $name:literal $(| $alias:literal)* {
            $($(#[$fm:meta])* $f:ident : $ft:ty),* $(,)?
        }
    ) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $ty {
            $($(#[$fm])* pub $f: $ft,)*
        }

        impl Entity for $ty {
            const NAMES: &'static [&'static str] = &[$name $(, $alias)*];

            fn read(_name: &str, params: &[Param]) -> Option<Self> {
                let mut it = params.iter();
                let r = $ty { $($f: <$ft as Field>::read(it.next()?)?,)* };
                it.next().is_none().then_some(r)
            }

            fn write(&self, out: &mut String) {
                out.push_str($name);
                out.push('(');
                super::write_fields(out, &[$(&self.$f),*]);
                out.push(')');
            }

            fn entity_name(&self) -> &'static str {
                $name
            }

            fn refs(&self, _f: &mut dyn FnMut(RecordId)) {
                $(Field::refs(&self.$f, _f);)*
            }

            fn refs_mut(&mut self, _f: &mut dyn FnMut(&mut RecordId)) {
                $(Field::refs_mut(&mut self.$f, _f);)*
            }
        }
    };
    (
        $(#[$m:meta])*
        $ty:ident [$kind:ident { $($(#[$km:meta])* $kv:ident = $kname:literal),+ $(,)? }] {
            $($(#[$fm:meta])* $f:ident : $ft:ty),* $(,)?
        }
    ) => {
        #[doc = concat!("Which entity a [`", stringify!($ty), "`] is.")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $kind {
            $($(#[$km])* $kv,)+
        }

        impl $kind {
            /// The entity name of this kind.
            pub fn entity_name(self) -> &'static str {
                match self {
                    $($kind::$kv => $kname,)+
                }
            }
        }

        $(#[$m])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $ty {
            pub kind: $kind,
            $($(#[$fm])* pub $f: $ft,)*
        }

        impl Entity for $ty {
            const NAMES: &'static [&'static str] = &[$($kname),+];

            fn read(name: &str, params: &[Param]) -> Option<Self> {
                let kind = match name {
                    $($kname => $kind::$kv,)+
                    _ => return None,
                };
                let mut it = params.iter();
                let r = $ty { kind, $($f: <$ft as Field>::read(it.next()?)?,)* };
                it.next().is_none().then_some(r)
            }

            fn write(&self, out: &mut String) {
                out.push_str(self.kind.entity_name());
                out.push('(');
                super::write_fields(out, &[$(&self.$f),*]);
                out.push(')');
            }

            fn entity_name(&self) -> &'static str {
                self.kind.entity_name()
            }

            fn refs(&self, _f: &mut dyn FnMut(RecordId)) {
                $(Field::refs(&self.$f, _f);)*
            }

            fn refs_mut(&mut self, _f: &mut dyn FnMut(&mut RecordId)) {
                $(Field::refs_mut(&mut self.$f, _f);)*
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// Product structure (ISO 10303-41 and -44)

entity! {
    /// The application a file's product data belongs to; its text names the protocol.
    ApplicationContext = "APPLICATION_CONTEXT" { application: String }
}

entity! {
    /// The protocol schema a file follows.
    ApplicationProtocolDefinition = "APPLICATION_PROTOCOL_DEFINITION" {
        status: String,
        schema_name: String,
        year: i64,
        application: RecordId,
    }
}

entity! {
    /// The discipline a product belongs to. AP203's `MECHANICAL_CONTEXT` reads as this.
    ProductContext = "PRODUCT_CONTEXT" | "MECHANICAL_CONTEXT" {
        name: String,
        frame_of_reference: RecordId,
        discipline_type: String,
    }
}

entity! {
    /// A part or an assembly.
    Product = "PRODUCT" {
        id: String,
        name: String,
        description: String,
        frame_of_reference: Vec<RecordId>,
    }
}

entity! {
    ProductRelatedProductCategory = "PRODUCT_RELATED_PRODUCT_CATEGORY" {
        name: String,
        description: Option<String>,
        products: Vec<RecordId>,
    }
}

entity! {
    /// A version of a [`Product`].
    ProductDefinitionFormation = "PRODUCT_DEFINITION_FORMATION" {
        id: String,
        description: String,
        of_product: RecordId,
    }
}

entity! {
    /// A version of a [`Product`] that says whether it is made or bought (AP203 writes these).
    ProductDefinitionFormationWithSpecifiedSource = "PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE" {
        id: String,
        description: String,
        of_product: RecordId,
        make_or_buy: EnumValue,
    }
}

entity! {
    /// The life-cycle stage of a definition. AP203's `DESIGN_CONTEXT` reads as this.
    ProductDefinitionContext = "PRODUCT_DEFINITION_CONTEXT" | "DESIGN_CONTEXT" {
        name: String,
        frame_of_reference: RecordId,
        life_cycle_stage: String,
    }
}

entity! {
    /// A definition of a product version: the node assembly structure and shapes hang off.
    ProductDefinition = "PRODUCT_DEFINITION" {
        id: String,
        description: String,
        formation: RecordId,
        frame_of_reference: RecordId,
    }
}

entity! {
    /// The shape of a [`ProductDefinition`], or of a [`NextAssemblyUsageOccurrence`].
    ProductDefinitionShape = "PRODUCT_DEFINITION_SHAPE" {
        name: String,
        description: String,
        definition: RecordId,
    }
}

entity! {
    /// Ties a [`ProductDefinitionShape`] to the [`Representation`] that draws it.
    ShapeDefinitionRepresentation = "SHAPE_DEFINITION_REPRESENTATION" {
        definition: RecordId,
        used_representation: RecordId,
    }
}

entity! {
    /// One use of a child definition in a parent assembly.
    NextAssemblyUsageOccurrence = "NEXT_ASSEMBLY_USAGE_OCCURRENCE" {
        id: String,
        name: String,
        description: String,
        relating_product_definition: RecordId,
        related_product_definition: RecordId,
        reference_designator: Option<String>,
    }
}

entity! {
    /// Places a child's shape in its parent for one occurrence: a
    /// [`ShapeRepresentationTransformation`] and the occurrence's [`ProductDefinitionShape`].
    ContextDependentShapeRepresentation = "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION" {
        representation_relation: RecordId,
        represented_product_relation: RecordId,
    }
}

// ---------------------------------------------------------------------------------------------
// Representations (ISO 10303-43)

entity! {
    /// A set of items in one context: a shape, a B-rep, a pcurve's 2D curve, or the styled
    /// items of a presentation.
    Representation [RepresentationKind {
        Representation = "REPRESENTATION",
        Shape = "SHAPE_REPRESENTATION",
        AdvancedBrep = "ADVANCED_BREP_SHAPE_REPRESENTATION",
        FacetedBrep = "FACETED_BREP_SHAPE_REPRESENTATION",
        ManifoldSurface = "MANIFOLD_SURFACE_SHAPE_REPRESENTATION",
        GeometricallyBoundedSurface = "GEOMETRICALLY_BOUNDED_SURFACE_SHAPE_REPRESENTATION",
        GeometricallyBoundedWireframe = "GEOMETRICALLY_BOUNDED_WIREFRAME_SHAPE_REPRESENTATION",
        EdgeBasedWireframe = "EDGE_BASED_WIREFRAME_SHAPE_REPRESENTATION",
        /// The parameter-space curve of a [`Pcurve`].
        Definitional = "DEFINITIONAL_REPRESENTATION",
        /// The styled items that colour a model.
        MechanicalDesignGeometricPresentation = "MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION",
        DraughtingModel = "DRAUGHTING_MODEL",
    }] {
        name: String,
        items: Vec<RecordId>,
        context: RecordId,
    }
}

entity! {
    /// Says two representations draw the same shape (AP203 and AP242 tie a
    /// `SHAPE_REPRESENTATION` to its B-rep this way).
    ShapeRepresentationRelationship = "SHAPE_REPRESENTATION_RELATIONSHIP" {
        name: String,
        description: String,
        rep_1: RecordId,
        rep_2: RecordId,
    }
}

entity! {
    /// The transformation from placement `item_1` to placement `item_2`.
    ItemDefinedTransformation = "ITEM_DEFINED_TRANSFORMATION" {
        name: String,
        description: String,
        transform_item_1: RecordId,
        transform_item_2: RecordId,
    }
}

entity! {
    /// A representation offered for reuse, with the placement in it that a
    /// [`MappedItem`] lines up with its own.
    RepresentationMap = "REPRESENTATION_MAP" {
        mapping_origin: RecordId,
        mapped_representation: RecordId,
    }
}

entity! {
    /// One use of a [`RepresentationMap`] as an item of another representation: the map's
    /// origin moved onto `mapping_target`.
    MappedItem = "MAPPED_ITEM" {
        name: String,
        mapping_source: RecordId,
        mapping_target: RecordId,
    }
}

// ---------------------------------------------------------------------------------------------
// Units and measures (ISO 10303-41)

entity! {
    DimensionalExponents = "DIMENSIONAL_EXPONENTS" {
        length: f64,
        mass: f64,
        time: f64,
        electric_current: f64,
        thermodynamic_temperature: f64,
        amount_of_substance: f64,
        luminous_intensity: f64,
    }
}

entity! {
    /// A value in a unit, such as the 25.4 mm of an inch.
    MeasureWithUnit [MeasureKind {
        Generic = "MEASURE_WITH_UNIT",
        Length = "LENGTH_MEASURE_WITH_UNIT",
        PlaneAngle = "PLANE_ANGLE_MEASURE_WITH_UNIT",
        SolidAngle = "SOLID_ANGLE_MEASURE_WITH_UNIT",
    }] {
        value: Measure,
        unit: RecordId,
    }
}

entity! {
    /// A context's geometric tolerance.
    UncertaintyMeasureWithUnit = "UNCERTAINTY_MEASURE_WITH_UNIT" {
        value: Measure,
        unit: RecordId,
        name: String,
        description: String,
    }
}

impl MeasureWithUnit {
    pub(crate) fn read_complex_parts(parts: &[crate::part21::Part]) -> Option<Self> {
        // ( LENGTH_MEASURE_WITH_UNIT() MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#9) )
        let mut kind = None;
        let mut body = None;
        for part in parts {
            match part.name.as_str() {
                "MEASURE_WITH_UNIT" => body = Some(&part.params),
                "LENGTH_MEASURE_WITH_UNIT" if part.params.is_empty() => {
                    kind = Some(MeasureKind::Length)
                }
                "PLANE_ANGLE_MEASURE_WITH_UNIT" if part.params.is_empty() => {
                    kind = Some(MeasureKind::PlaneAngle)
                }
                "SOLID_ANGLE_MEASURE_WITH_UNIT" if part.params.is_empty() => {
                    kind = Some(MeasureKind::SolidAngle)
                }
                _ => return None,
            }
        }
        let mut m = Self::read("MEASURE_WITH_UNIT", body?)?;
        m.kind = kind?;
        Some(m)
    }
}

// ---------------------------------------------------------------------------------------------
// Geometry (ISO 10303-42)

entity! {
    CartesianPoint = "CARTESIAN_POINT" { name: String, coordinates: Vec<f64> }
}

entity! {
    Direction = "DIRECTION" { name: String, ratios: Vec<f64> }
}

entity! {
    /// A direction and a length.
    Vector = "VECTOR" { name: String, orientation: RecordId, magnitude: f64 }
}

entity! {
    Axis1Placement = "AXIS1_PLACEMENT" {
        name: String,
        location: RecordId,
        axis: Option<RecordId>,
    }
}

entity! {
    Axis2Placement2d = "AXIS2_PLACEMENT_2D" {
        name: String,
        location: RecordId,
        ref_direction: Option<RecordId>,
    }
}

entity! {
    /// A position and orientation: the frame of a surface, a circle, or a part in an assembly.
    Axis2Placement3d = "AXIS2_PLACEMENT_3D" {
        name: String,
        location: RecordId,
        axis: Option<RecordId>,
        ref_direction: Option<RecordId>,
    }
}

entity! {
    /// `pnt + u * dir`, where `dir` is a [`Vector`].
    Line = "LINE" { name: String, pnt: RecordId, dir: RecordId }
}

entity! {
    Circle = "CIRCLE" { name: String, position: RecordId, radius: f64 }
}

entity! {
    Ellipse = "ELLIPSE" {
        name: String,
        position: RecordId,
        semi_axis_1: f64,
        semi_axis_2: f64,
    }
}

entity! {
    /// `C(t) = o + a·(t²·x + 2t·y)`, `a` the focal distance. A plane parallel to a cone's
    /// generator cuts it in one.
    Parabola = "PARABOLA" { name: String, position: RecordId, focal_dist: f64 }
}

entity! {
    /// `C(t) = o + a·cosh t·x + b·sinh t·y`. A plane parallel to a cone's axis cuts it in one.
    Hyperbola = "HYPERBOLA" {
        name: String,
        position: RecordId,
        semi_axis: f64,
        semi_imag_axis: f64,
    }
}

entity! {
    Polyline = "POLYLINE" { name: String, points: Vec<RecordId> }
}

entity! {
    /// A part of a basis curve between two trims.
    TrimmedCurve = "TRIMMED_CURVE" {
        name: String,
        basis_curve: RecordId,
        trim_1: Vec<Trim>,
        trim_2: Vec<Trim>,
        sense_agreement: bool,
        master_representation: EnumValue,
    }
}

entity! {
    /// A 3D curve with its parameter-space curves on the surfaces it lies on.
    SurfaceCurve [SurfaceCurveKind {
        Surface = "SURFACE_CURVE",
        /// The seam of a closed surface: the same surface on both sides.
        Seam = "SEAM_CURVE",
        Intersection = "INTERSECTION_CURVE",
    }] {
        name: String,
        curve_3d: RecordId,
        associated_geometry: Vec<RecordId>,
        master_representation: EnumValue,
    }
}

entity! {
    /// A curve in a surface's parameter space, held by a definitional [`Representation`].
    Pcurve = "PCURVE" {
        name: String,
        basis_surface: RecordId,
        reference_to_curve: RecordId,
    }
}

entity! {
    Plane = "PLANE" { name: String, position: RecordId }
}

entity! {
    CylindricalSurface = "CYLINDRICAL_SURFACE" {
        name: String,
        position: RecordId,
        radius: f64,
    }
}

entity! {
    ConicalSurface = "CONICAL_SURFACE" {
        name: String,
        position: RecordId,
        radius: f64,
        semi_angle: f64,
    }
}

entity! {
    SphericalSurface = "SPHERICAL_SURFACE" {
        name: String,
        position: RecordId,
        radius: f64,
    }
}

entity! {
    ToroidalSurface = "TOROIDAL_SURFACE" {
        name: String,
        position: RecordId,
        major_radius: f64,
        minor_radius: f64,
    }
}

entity! {
    /// A torus whose minor radius is larger than its major one; `select_outer` picks the
    /// outer or the inner part.
    DegenerateToroidalSurface = "DEGENERATE_TOROIDAL_SURFACE" {
        name: String,
        position: RecordId,
        major_radius: f64,
        minor_radius: f64,
        select_outer: bool,
    }
}

entity! {
    /// Curves joined end to end.
    CompositeCurve = "COMPOSITE_CURVE" {
        name: String,
        segments: Vec<RecordId>,
        self_intersect: Logical,
    }
}

entity! {
    CompositeCurveSegment = "COMPOSITE_CURVE_SEGMENT" {
        transition: EnumValue,
        same_sense: bool,
        parent_curve: RecordId,
    }
}

entity! {
    /// Loose points, curves and surfaces shown with a shape (wireframe).
    GeometricSet [GeometricSetKind {
        Set = "GEOMETRIC_SET",
        CurveSet = "GEOMETRIC_CURVE_SET",
    }] {
        name: String,
        elements: Vec<RecordId>,
    }
}

entity! {
    SurfaceOfRevolution = "SURFACE_OF_REVOLUTION" {
        name: String,
        swept_curve: RecordId,
        axis_position: RecordId,
    }
}

entity! {
    SurfaceOfLinearExtrusion = "SURFACE_OF_LINEAR_EXTRUSION" {
        name: String,
        swept_curve: RecordId,
        extrusion_axis: RecordId,
    }
}

// ---------------------------------------------------------------------------------------------
// Topology (ISO 10303-42)

entity! {
    VertexPoint = "VERTEX_POINT" { name: String, vertex_geometry: RecordId }
}

entity! {
    /// An edge between two vertices along a curve.
    EdgeCurve = "EDGE_CURVE" {
        name: String,
        edge_start: RecordId,
        edge_end: RecordId,
        edge_geometry: RecordId,
        same_sense: bool,
    }
}

entity! {
    /// An [`EdgeCurve`] used in a loop, forwards or backwards.
    OrientedEdge = "ORIENTED_EDGE" {
        name: String,
        edge_start: Derived,
        edge_end: Derived,
        edge_element: RecordId,
        orientation: bool,
    }
}

entity! {
    EdgeLoop = "EDGE_LOOP" { name: String, edge_list: Vec<RecordId> }
}

entity! {
    VertexLoop = "VERTEX_LOOP" { name: String, loop_vertex: RecordId }
}

entity! {
    PolyLoop = "POLY_LOOP" { name: String, polygon: Vec<RecordId> }
}

entity! {
    /// A loop bounding a face.
    FaceBound [FaceBoundKind {
        Inner = "FACE_BOUND",
        Outer = "FACE_OUTER_BOUND",
    }] {
        name: String,
        bound: RecordId,
        orientation: bool,
    }
}

entity! {
    /// A face: bounds on a surface.
    Face [FaceKind {
        Advanced = "ADVANCED_FACE",
        Surface = "FACE_SURFACE",
    }] {
        name: String,
        bounds: Vec<RecordId>,
        face_geometry: RecordId,
        same_sense: bool,
    }
}

entity! {
    Shell [ShellKind {
        Closed = "CLOSED_SHELL",
        Open = "OPEN_SHELL",
    }] {
        name: String,
        cfs_faces: Vec<RecordId>,
    }
}

entity! {
    /// A solid bounded by one closed shell.
    SolidBrep [SolidKind {
        Manifold = "MANIFOLD_SOLID_BREP",
        Faceted = "FACETED_BREP",
    }] {
        name: String,
        outer: RecordId,
    }
}

entity! {
    /// A closed shell used inside out, as the void of a [`BrepWithVoids`].
    OrientedClosedShell = "ORIENTED_CLOSED_SHELL" {
        name: String,
        cfs_faces: Derived,
        closed_shell_element: RecordId,
        orientation: bool,
    }
}

entity! {
    /// A solid with an outer shell and inner voids.
    BrepWithVoids = "BREP_WITH_VOIDS" {
        name: String,
        outer: RecordId,
        voids: Vec<RecordId>,
    }
}

entity! {
    /// A shape made of shells that need not close.
    ShellBasedSurfaceModel = "SHELL_BASED_SURFACE_MODEL" {
        name: String,
        sbsm_boundary: Vec<RecordId>,
    }
}

// ---------------------------------------------------------------------------------------------
// Styles (ISO 10303-46)

entity! {
    /// Gives an item (a solid, a shell, a face) its styles.
    StyledItem = "STYLED_ITEM" {
        name: String,
        styles: Vec<RecordId>,
        item: RecordId,
    }
}

entity! {
    /// A style that overrides another for the same item.
    OverRidingStyledItem = "OVER_RIDING_STYLED_ITEM" {
        name: String,
        styles: Vec<RecordId>,
        item: RecordId,
        over_ridden_style: RecordId,
    }
}

entity! {
    /// An overriding style that holds in one assembly context only.
    ContextDependentOverRidingStyledItem = "CONTEXT_DEPENDENT_OVER_RIDING_STYLED_ITEM" {
        name: String,
        styles: Vec<RecordId>,
        item: RecordId,
        over_ridden_style: RecordId,
        style_context: Vec<RecordId>,
    }
}

entity! {
    PresentationStyleAssignment = "PRESENTATION_STYLE_ASSIGNMENT" { styles: Vec<RecordId> }
}

entity! {
    SurfaceStyleUsage = "SURFACE_STYLE_USAGE" { side: EnumValue, style: RecordId }
}

entity! {
    SurfaceSideStyle = "SURFACE_SIDE_STYLE" { name: String, styles: Vec<RecordId> }
}

entity! {
    SurfaceStyleFillArea = "SURFACE_STYLE_FILL_AREA" { fill_area: RecordId }
}

entity! {
    FillAreaStyle = "FILL_AREA_STYLE" { name: String, fill_styles: Vec<RecordId> }
}

entity! {
    FillAreaStyleColour = "FILL_AREA_STYLE_COLOUR" { name: String, fill_colour: RecordId }
}

entity! {
    /// A colour, each channel from 0 to 1.
    ColourRgb = "COLOUR_RGB" { name: String, red: f64, green: f64, blue: f64 }
}

entity! {
    /// A named colour: `red`, `green`, `blue`, `yellow`, `magenta`, `cyan`, `black`, `white`.
    DraughtingPreDefinedColour = "DRAUGHTING_PRE_DEFINED_COLOUR" { name: String }
}

entity! {
    SurfaceStyleRendering = "SURFACE_STYLE_RENDERING" {
        rendering_method: EnumValue,
        surface_colour: RecordId,
    }
}

entity! {
    SurfaceStyleRenderingWithProperties = "SURFACE_STYLE_RENDERING_WITH_PROPERTIES" {
        rendering_method: EnumValue,
        surface_colour: RecordId,
        properties: Vec<RecordId>,
    }
}

entity! {
    /// How a curve is drawn: font, width and colour.
    CurveStyle = "CURVE_STYLE" {
        name: String,
        curve_font: RecordId,
        curve_width: Measure,
        curve_colour: RecordId,
    }
}

entity! {
    DraughtingPreDefinedCurveFont = "DRAUGHTING_PRE_DEFINED_CURVE_FONT" { name: String }
}

entity! {
    /// Transparency from 0 (opaque) to 1.
    SurfaceStyleTransparent = "SURFACE_STYLE_TRANSPARENT" { transparency: f64 }
}
