//! Turning a Scene into GPU instances: one fixed-size record per primitive, grouped by
//! Layer, plus the map from objects to their instances that the flag buffer is built from.

use std::collections::BTreeMap;
use std::ops::Range;

use reda_model::Uuid;
use reda_model::geom::{Nm, Point};

use crate::color::{ColorKey, SchematicColor};
use crate::scene::{Primitive, Scene, SceneLayer};

/// Bytes per instance: four `vec2<f32>`, a `vec2<u32>` (kind, colour slot) and a
/// `vec2<f32>` (view-label height, unused).
pub(crate) const INSTANCE_SIZE: u64 = 48;

const CAPSULE: u32 = 0;
const RECT: u32 = 1;
const RING: u32 = 2;
const TRIANGLE: u32 = 3;

/// What an object is marked as. Combine with `|`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(pub u32);

impl Flags {
    pub const NONE: Flags = Flags(0);
    /// Lightened, with a halo.
    pub const SELECTED: Flags = Flags(1);
    /// Faintly lightened.
    pub const HOVERED: Flags = Flags(2);
    /// Net-highlighted or cross-probed. While anything a Scene draws is highlighted,
    /// everything else it draws is dimmed.
    pub const HIGHLIGHTED: Flags = Flags(4);
    /// Faded towards the background.
    pub const DIMMED: Flags = Flags(8);
    /// In flight: drawn at the canvas's moving offset ([`Canvas::set_moving_offset`]), so
    /// a drag moves on screen with no geometry or flag writes.
    ///
    /// [`Canvas::set_moving_offset`]: super::Canvas::set_moving_offset
    pub const MOVING: Flags = Flags(16);
    /// Not drawn: a Wire a drag in flight redraws elsewhere.
    pub const HIDDEN: Flags = Flags(32);

    pub const fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Flags {
    type Output = Flags;
    fn bitor(self, rhs: Flags) -> Flags {
        Flags(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Flags {
    fn bitor_assign(&mut self, rhs: Flags) {
        self.0 |= rhs.0;
    }
}

/// The marks on a Scene's objects. Objects not listed are drawn plainly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Highlight {
    marks: BTreeMap<Uuid, Flags>,
}

impl Highlight {
    pub fn new() -> Highlight {
        Highlight::default()
    }

    /// Adds `flags` to `object`'s marks.
    pub fn mark(&mut self, object: Uuid, flags: Flags) -> &mut Highlight {
        *self.marks.entry(object).or_default() |= flags;
        self
    }

    /// Adds `flags` to each object's marks.
    pub fn mark_all(
        &mut self,
        objects: impl IntoIterator<Item = Uuid>,
        flags: Flags,
    ) -> &mut Highlight {
        for o in objects {
            self.mark(o, flags);
        }
        self
    }

    pub fn get(&self, object: Uuid) -> Flags {
        self.marks.get(&object).copied().unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }
}

/// A Scene laid out as instances.
pub(crate) struct Encoded {
    pub bytes: Vec<u8>,
    pub count: u32,
    /// Each Layer's instances, in the order the Layers first appear in the Scene.
    pub layers: Vec<(SceneLayer, Range<u32>)>,
    /// Every object's instance ranges.
    pub objects: BTreeMap<Uuid, Vec<Range<u32>>>,
    /// Instances that highlighting never dims: the paper.
    pub undimmed: Vec<Range<u32>>,
    /// The Document point instance coordinates are relative to.
    pub origin: Point,
}

/// Lays out `scene` as instances, keeping draw order within each Layer.
pub(crate) fn encode(scene: &Scene) -> Encoded {
    let origin = scene
        .bounds()
        .map(|(lo, hi)| Point::new(Nm((lo.x.0 + hi.x.0) / 2), Nm((lo.y.0 + hi.y.0) / 2)))
        .unwrap_or_default();
    let mut order: Vec<SceneLayer> = Vec::new();
    for item in &scene.items {
        if !order.contains(&item.layer) {
            order.push(item.layer);
        }
    }
    let mut out = Writer {
        bytes: Vec::new(),
        count: 0,
        origin,
    };
    let mut layers = Vec::new();
    let mut objects: BTreeMap<Uuid, Vec<Range<u32>>> = BTreeMap::new();
    let mut undimmed = Vec::new();
    let paper = ColorKey::Schematic(SchematicColor::Paper);
    for layer in order {
        let start = out.count;
        for item in scene.items.iter().filter(|i| i.layer == layer) {
            let first = out.count;
            out.primitive(&item.primitive, item.color.slot() as u32);
            if out.count > first {
                objects
                    .entry(item.object)
                    .or_default()
                    .push(first..out.count);
                if let Some(child) = &item.child {
                    objects
                        .entry(child.id())
                        .or_default()
                        .push(first..out.count);
                }
                if item.color == paper {
                    undimmed.push(first..out.count);
                }
            }
        }
        layers.push((layer, start..out.count));
    }
    Encoded {
        bytes: out.bytes,
        count: out.count,
        layers,
        objects,
        undimmed,
        origin,
    }
}

/// One flag word per instance, from `highlight`.
pub(crate) fn flag_words(encoded: &Encoded, highlight: &Highlight) -> Vec<u8> {
    let (count, objects) = (encoded.count, &encoded.objects);
    // Only a highlighted object this Scene draws dims the rest: a cross-probe marks the
    // other Document's objects on every canvas.
    let dim_rest = objects
        .keys()
        .any(|o| highlight.get(*o).contains(Flags::HIGHLIGHTED));
    let base = if dim_rest { Flags::DIMMED } else { Flags::NONE };
    let mut words = vec![base.0; count as usize];
    for (object, ranges) in objects {
        let mut flags = highlight.get(*object);
        if !flags.contains(Flags::HIGHLIGHTED) {
            flags |= base;
        }
        for r in ranges {
            for w in &mut words[r.start as usize..r.end as usize] {
                *w |= flags.0;
                if *w & Flags::HIGHLIGHTED.0 != 0 {
                    *w &= !Flags::DIMMED.0;
                }
            }
        }
    }
    if dim_rest {
        for r in &encoded.undimmed {
            for w in &mut words[r.start as usize..r.end as usize] {
                *w &= !Flags::DIMMED.0;
            }
        }
    }
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

struct Writer {
    bytes: Vec<u8>,
    count: u32,
    origin: Point,
}

impl Writer {
    fn rel(&self, p: Point) -> [f32; 2] {
        [
            (p.x.0 - self.origin.x.0) as f32,
            (p.y.0 - self.origin.y.0) as f32,
        ]
    }

    fn push(&mut self, kind: u32, color: u32, v: [[f32; 2]; 4], label: f32) {
        for pair in v {
            for f in pair {
                self.bytes.extend_from_slice(&f.to_le_bytes());
            }
        }
        self.bytes.extend_from_slice(&kind.to_le_bytes());
        self.bytes.extend_from_slice(&color.to_le_bytes());
        self.bytes.extend_from_slice(&label.to_le_bytes());
        self.bytes.extend_from_slice(&0f32.to_le_bytes());
        self.count += 1;
    }

    fn capsule(&mut self, a: Point, b: Point, half_width: f32, color: u32, label: f32) {
        let (a, b) = (self.rel(a), self.rel(b));
        self.push(CAPSULE, color, [a, b, [half_width, 0.0], [0.0; 2]], label);
    }

    fn mesh(&mut self, mesh: &reda_text::Mesh, color: u32, label: f32) {
        for t in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|k| self.rel(mesh.points[t[k] as usize]));
            self.push(TRIANGLE, color, [a, b, c, [0.0; 2]], label);
        }
    }

    fn primitive(&mut self, primitive: &Primitive, color: u32) {
        match primitive {
            Primitive::Capsule { a, b, width } => {
                self.capsule(*a, *b, width.0 as f32 / 2.0, color, 0.0);
            }
            Primitive::Rect {
                center,
                size,
                rotation,
                corner_radius,
                stroke,
            } => {
                let angle = (rotation.0 as f64 / 1000.0).to_radians();
                let c = self.rel(*center);
                let half = [size.0.0 as f32 / 2.0, size.1.0 as f32 / 2.0];
                let stroke = stroke.map_or(-1.0, |w| w.0 as f32 / 2.0);
                self.push(
                    RECT,
                    color,
                    [
                        c,
                        half,
                        [angle.cos() as f32, angle.sin() as f32],
                        [corner_radius.0 as f32, stroke],
                    ],
                    0.0,
                );
            }
            Primitive::Ring {
                center,
                outer,
                inner,
            } => {
                let c = self.rel(*center);
                self.push(
                    RING,
                    color,
                    [c, [outer.0 as f32, inner.0 as f32], [0.0; 2], [0.0; 2]],
                    0.0,
                );
            }
            Primitive::Triangles(mesh) => self.mesh(mesh, color, 0.0),
            Primitive::Glyphs(run) => {
                let label = if run.view_label {
                    run.height.0 as f32
                } else {
                    0.0
                };
                let hw = run.stroke_width.0 as f32 / 2.0;
                for s in &run.strokes {
                    self.capsule(s.a, s.b, hw, color, label);
                }
                self.mesh(&run.fill, color, label);
            }
        }
    }
}
