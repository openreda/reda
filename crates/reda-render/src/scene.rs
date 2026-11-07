//! The GPU-free Scene: what a Document looks like, as a flat list of tagged primitives.

use reda_model::Uuid;
use reda_model::geom::{Millideg, Nm, Point};
use reda_text::{Mesh, Segment};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::color::ColorKey;

/// A Document drawn as primitives, in draw order, ready for the wgpu backend, PDF or SVG.
///
/// Coordinates are the Document's own: nm, y up. Colours are [`ColorKey`]s, resolved
/// against a [`Palette`](crate::Palette) when drawn, so switching the theme or overriding a
/// colour needs no rebuild.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scene {
    /// What the Scene was built from.
    pub source: SceneSource,
    /// Every primitive, in draw order: later items draw over earlier ones on the same
    /// Layer.
    pub items: Vec<Item>,
}

/// The Document a Scene was built from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneSource {
    #[default]
    None,
    /// A Schematic Sheet, by its Document UUID.
    Schematic(Uuid),
    /// A PCB Document, by its UUID.
    Pcb(Uuid),
    /// One Unit of a Library Symbol in the Symbol editor: the Symbol's and the Unit's UUIDs.
    Symbol(Uuid, Uuid),
}

/// One primitive, tagged with the object it draws and the Layer it is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The persisted object this primitive belongs to: what hit-testing, selection and
    /// highlighting key on. The sheet frame and title block use the sheet's frame object,
    /// or the sheet's Document UUID when it has none.
    pub object: Uuid,
    /// Independently pickable text or Pad within its owning object.
    pub child: Option<Child>,
    pub layer: SceneLayer,
    pub color: ColorKey,
    pub primitive: Primitive,
}

/// A canvas target inside a placed Component or Footprint. The owner remains the
/// primitive's `object`, so a parent move and highlight also include its children.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Child {
    Field { unit: Uuid, field: String },
    Pad { footprint: Uuid, pad: Uuid },
    FootprintText { footprint: Uuid, text: Uuid },
}

impl Child {
    /// A stable view-only identity for GPU flags. Version 8 keeps it separate from
    /// persisted version-4 object UUIDs. The actual public identity is the full reference.
    pub fn id(&self) -> Uuid {
        let mut hash = 0x6c62272e07bb014262b821756295c58du128;
        let bytes = serde_json::to_vec(self).expect("a Child is serializable");
        for byte in bytes {
            hash = (hash ^ u128::from(byte)).wrapping_mul(0x1000000000000000000013b);
        }
        Uuid::from_u128((hash & !(0xf << 76)) | (8 << 76))
    }

    pub fn owner(&self) -> Uuid {
        match self {
            Self::Field { unit, .. } => *unit,
            Self::Pad { footprint, .. } | Self::FootprintText { footprint, .. } => *footprint,
        }
    }
}

/// The Layer an item is composited on. Each Layer is drawn opaque, then blended at its
/// opacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SceneLayer {
    /// The schematic: one Layer at full opacity.
    Schematic,
    /// A PCB Layer, by its UUID.
    Board(Uuid),
    /// Drilled holes (and the numbers of Pads with a hole), drawn over every PCB Layer.
    Drill,
    /// The Ratsnest's lines, drawn over everything while the Ratsnest is visible.
    Ratsnest,
}

/// A drawable shape. Each maps to one instanced SDF primitive, a triangle list or a run of
/// glyphs in the backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Primitive {
    /// A round-capped stroke from `a` to `b`, `width` wide (Wires, lines, Tracks, stroke
    /// text). `a == b` draws a dot.
    Capsule { a: Point, b: Point, width: Nm },
    /// A rectangle `size` wide and high about `center`, turned `rotation` counter-clockwise,
    /// with rounded corners. `stroke: None` fills it; `Some(w)` draws only its outline,
    /// centred on the edge, `w` wide.
    Rect {
        center: Point,
        size: (Nm, Nm),
        rotation: Millideg,
        corner_radius: Nm,
        stroke: Option<Nm>,
    },
    /// The annulus between `inner` and `outer` radius about `center`. `inner == 0` is a
    /// filled disc (Junctions); a circle outline `w` wide is `r ± w/2` (Vias, Pads).
    Ring { center: Point, outer: Nm, inner: Nm },
    /// Filled triangles (polygons, Zone fills).
    Triangles(Mesh),
    /// A run of text, already laid out in Document coordinates.
    Glyphs(GlyphRun),
}

/// Laid-out text: the geometry to draw plus what produced it, which snapshots and the
/// view-label size limit use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphRun {
    pub text: String,
    /// The anchor the text was aligned to.
    pub at: Point,
    /// Capital height.
    pub height: Nm,
    /// The reading direction, counter-clockwise from +x.
    pub rotation: Millideg,
    pub align: Align,
    /// View-only labels (Pin numbers and names) are generated by the renderer and hidden
    /// below a minimum on-screen size; Document text is always drawn.
    pub view_label: bool,
    /// Stroke-font pen strokes, drawn as capsules `stroke_width` wide.
    pub strokes: Vec<Segment>,
    pub stroke_width: Nm,
    /// Outline-font glyphs, filled.
    pub fill: Mesh,
}

/// How text sits on its anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Align {
    pub h: HAlign,
    pub v: VAlign,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VAlign {
    /// The anchor is on the baseline.
    #[default]
    Baseline,
    /// The anchor is half a capital height above the baseline.
    Middle,
    /// The anchor is on the capital height.
    Top,
}

impl Align {
    pub const fn new(h: HAlign, v: VAlign) -> Align {
        Align { h, v }
    }
}

impl From<reda_model::TextJustify> for Align {
    fn from(justify: reda_model::TextJustify) -> Self {
        let h = match justify.h {
            reda_model::TextHJustify::Left => HAlign::Left,
            reda_model::TextHJustify::Center => HAlign::Center,
            reda_model::TextHJustify::Right => HAlign::Right,
        };
        let v = match justify.v {
            reda_model::TextVJustify::Bottom => VAlign::Baseline,
            reda_model::TextVJustify::Center => VAlign::Middle,
            reda_model::TextVJustify::Top => VAlign::Top,
        };
        Self::new(h, v)
    }
}

impl Primitive {
    /// The axis-aligned bounds of the primitive's ink, as (min, max).
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut b = Bounds::default();
        match self {
            Primitive::Capsule { a, b: end, width } => {
                let r = Nm(width.0 / 2);
                b.add_padded(*a, r);
                b.add_padded(*end, r);
            }
            Primitive::Rect {
                center,
                size,
                rotation,
                stroke,
                ..
            } => {
                let pad = Nm(stroke.map_or(0, |w| w.0 / 2));
                let (hw, hh) = (size.0.0 / 2, size.1.0 / 2);
                for (x, y) in [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)] {
                    let (x, y) = crate::transform::rotate(x, y, *rotation);
                    b.add_padded(Point::new(center.x + Nm(x), center.y + Nm(y)), pad);
                }
            }
            Primitive::Ring { center, outer, .. } => b.add_padded(*center, *outer),
            Primitive::Triangles(mesh) => mesh.points.iter().for_each(|p| b.add(*p)),
            Primitive::Glyphs(run) => {
                let r = Nm(run.stroke_width.0 / 2);
                for s in &run.strokes {
                    b.add_padded(s.a, r);
                    b.add_padded(s.b, r);
                }
                run.fill.points.iter().for_each(|p| b.add(*p));
            }
        }
        b.get()
    }
}

impl Scene {
    pub fn new(source: SceneSource) -> Scene {
        Scene {
            source,
            items: Vec::new(),
        }
    }

    /// The bounds of everything drawn, as (min, max); `None` for an empty Scene. "Zoom to
    /// fit" frames this.
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut b = Bounds::default();
        for (lo, hi) in self.items.iter().filter_map(|i| i.primitive.bounds()) {
            b.add(lo);
            b.add(hi);
        }
        b.get()
    }

    /// Every item drawing `object`.
    pub fn items_of(&self, object: Uuid) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(move |i| i.object == object)
    }

    pub(crate) fn push(
        &mut self,
        object: Uuid,
        layer: SceneLayer,
        color: ColorKey,
        primitive: Primitive,
    ) {
        self.items.push(Item {
            object,
            child: None,
            layer,
            color,
            primitive,
        });
    }
}

/// A growing axis-aligned box.
#[derive(Default)]
struct Bounds(Option<(Point, Point)>);

impl Bounds {
    fn add(&mut self, p: Point) {
        self.0 = Some(match self.0 {
            None => (p, p),
            Some((lo, hi)) => (
                Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                Point::new(hi.x.max(p.x), hi.y.max(p.y)),
            ),
        });
    }

    fn add_padded(&mut self, p: Point, r: Nm) {
        self.add(Point::new(p.x - r, p.y - r));
        self.add(Point::new(p.x + r, p.y + r));
    }

    fn get(&self) -> Option<(Point, Point)> {
        self.0
    }
}
