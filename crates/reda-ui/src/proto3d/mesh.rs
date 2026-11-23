//! PROTOTYPE (issue #433): the board as triangles. Throwaway.
//!
//! Built from what the 2D canvas already has: the PCB Document's [`Scene`] (copper,
//! silkscreen and holes, each primitive tagged with its object) plus the Board Outline,
//! the Stackup's thickness and each Footprint's body box and `height`. Solder mask is a
//! colour, not geometry: the board's faces and the copper under it take the mask colour.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use i_triangle::i_overlay::core::fill_rule::FillRule;
use i_triangle::i_overlay::core::simplify::Simplify;
use i_triangle::i_overlay::i_float::int::point::IntPoint;
use i_triangle::int::unchecked::IntUncheckedTriangulatable;
use reda_engine::model::geom::{Millideg, Point, Vertex, arc_centre};
use reda_engine::model::{LayerKind, PcbDocument, Side, Uuid};
use reda_engine::render::{
    BoardColor, Child, ColorKey, Mesh, Palette, Primitive, Scene, SceneLayer,
};

use super::camera::{V3, add, cross, dot, norm, scale, sub};

/// What a triangle belongs to: shown, hidden and faded together.
pub const BOARD: u32 = 0;
pub const COPPER: u32 = 1;
pub const INNER: u32 = 2;
pub const SILK: u32 = 3;
pub const BODY: u32 = 4;
pub const HOLE: u32 = 5;
pub const CATS: usize = 6;

pub const NO_OBJECT: u32 = u32::MAX;
pub const NO_LAYER: u16 = u16::MAX;

#[derive(Clone, Copy)]
pub struct Tri {
    pub p: [V3; 3],
    pub n: V3,
    /// sRGB, straight alpha.
    pub color: [f32; 4],
    /// How far (mm) it moves along z when the Layers are fully spread.
    pub lift: f32,
    pub cat: u32,
    /// Index into [`Model::objects`], or [`NO_OBJECT`].
    pub obj: u32,
    /// Index into [`Model::layers`], or [`NO_LAYER`].
    pub layer: u16,
}

/// One copper Layer, for the Layer rail.
pub struct LayerRow {
    pub uuid: Uuid,
    pub name: String,
    pub color: [f32; 4],
}

pub struct Body {
    pub obj: u32,
    pub designator: String,
    /// The Footprint's stored height; `None` when the stand-in's height is a guess.
    pub height: Option<f32>,
    /// The middle of its far face.
    pub top: V3,
    pub lift: f32,
}

pub struct Model {
    pub tris: Vec<Tri>,
    pub objects: Vec<Uuid>,
    pub layers: Vec<LayerRow>,
    pub bodies: Vec<Body>,
    /// The Document point (nm) at the model's origin.
    pub center: (f64, f64),
    pub radius: f32,
    pub thickness: f32,
    pub build_ms: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Look {
    /// Material colours; otherwise the 2D canvas's Layer colours.
    pub realistic: bool,
    pub mask: bool,
}

#[derive(Clone, Copy)]
struct Style {
    color: [f32; 4],
    lift: f32,
    cat: u32,
    obj: u32,
    layer: u16,
}

const COPPER_MM: f32 = 0.035;

struct Builder {
    tris: Vec<Tri>,
    center: (f64, f64),
}

impl Builder {
    fn xy(&self, p: Point) -> [f32; 2] {
        [
            ((p.x.0 as f64 - self.center.0) / 1e6) as f32,
            ((p.y.0 as f64 - self.center.1) / 1e6) as f32,
        ]
    }

    fn tri(&mut self, p: [V3; 3], s: &Style) {
        let n = norm(cross(sub(p[1], p[0]), sub(p[2], p[0])));
        self.tris.push(Tri {
            p,
            n,
            color: s.color,
            lift: s.lift,
            cat: s.cat,
            obj: s.obj,
            layer: s.layer,
        });
    }

    /// A triangle of a flat face at `z`, wound to face up or down.
    fn flat(&mut self, a: [f32; 2], b: [f32; 2], c: [f32; 2], z: f32, up: bool, s: &Style) {
        let (a, b, c) = ([a[0], a[1], z], [b[0], b[1], z], [c[0], c[1], z]);
        let ccw = cross(sub(b, a), sub(c, a))[2] > 0.0;
        self.tri(if ccw == up { [a, b, c] } else { [a, c, b] }, s);
    }

    /// A convex polygon's face.
    fn face(&mut self, poly: &[[f32; 2]], z: f32, up: bool, s: &Style) {
        for i in 1..poly.len().saturating_sub(1) {
            self.flat(poly[0], poly[i], poly[i + 1], z, up, s);
        }
    }

    /// The walls of a counter-clockwise polygon between two heights, facing out, or in
    /// for a hole.
    fn walls(&mut self, poly: &[[f32; 2]], z0: f32, z1: f32, inward: bool, s: &Style) {
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (a0, b0, b1, a1) = (
                [a[0], a[1], z0],
                [b[0], b[1], z0],
                [b[0], b[1], z1],
                [a[0], a[1], z1],
            );
            if inward {
                self.tri([a0, b1, b0], s);
                self.tri([a0, a1, b1], s);
            } else {
                self.tri([a0, b0, b1], s);
                self.tri([a0, b1, a1], s);
            }
        }
    }

    /// A convex polygon as a slab between two heights: the face on the side that shows,
    /// and its walls unless `thin`.
    fn slab(&mut self, poly: &[[f32; 2]], z: (f32, f32), up: bool, thin: bool, s: &Style) {
        self.face(poly, if up { z.1 } else { z.0 }, up, s);
        if !thin {
            self.walls(poly, z.0, z.1, false, s);
        }
    }

    fn mesh(&mut self, mesh: &Mesh, z: f32, up: bool, s: &Style) {
        for t in mesh.indices.chunks_exact(3) {
            let p = |i: u32| self.xy(mesh.points[i as usize]);
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            self.flat(a, b, c, z, up, s);
        }
    }

    fn primitive(&mut self, prim: &Primitive, z: (f32, f32), up: bool, thin: bool, s: &Style) {
        let caps = if thin { 3 } else { 6 };
        let top = if up { z.1 } else { z.0 };
        match prim {
            Primitive::Capsule { a, b, width } => {
                let poly = capsule(self.xy(*a), self.xy(*b), width.0 as f32 / 2e6, caps);
                self.slab(&poly, z, up, thin, s);
            }
            Primitive::Rect {
                center,
                size,
                rotation,
                corner_radius,
                stroke,
            } => {
                let (w, h) = (size.0.0 as f32 / 1e6, size.1.0 as f32 / 1e6);
                let c = self.xy(*center);
                match stroke {
                    None => {
                        let r = corner_radius.0 as f32 / 1e6;
                        let poly = rounded_rect(c, (w, h), radians(*rotation), r);
                        self.slab(&poly, z, up, thin, s);
                    }
                    Some(pen) => {
                        let q = rounded_rect(c, (w, h), radians(*rotation), 0.0);
                        for i in 0..4 {
                            let poly = capsule(q[i], q[(i + 1) % 4], pen.0 as f32 / 2e6, 3);
                            self.slab(&poly, z, up, thin, s);
                        }
                    }
                }
            }
            Primitive::Ring {
                center,
                outer,
                inner,
            } => {
                let c = self.xy(*center);
                let out = circle(c, outer.0 as f32 / 1e6, 24);
                if inner.0 <= 0 {
                    self.slab(&out, z, up, thin, s);
                } else {
                    let inn = circle(c, inner.0 as f32 / 1e6, 24);
                    for i in 0..24 {
                        let j = (i + 1) % 24;
                        self.flat(out[i], out[j], inn[j], top, up, s);
                        self.flat(out[i], inn[j], inn[i], top, up, s);
                    }
                    if !thin {
                        self.walls(&out, z.0, z.1, false, s);
                    }
                }
            }
            Primitive::Triangles(mesh) => self.mesh(mesh, top, up, s),
            Primitive::Glyphs(run) => {
                for seg in &run.strokes {
                    let r = run.stroke_width.0 as f32 / 2e6;
                    let poly = capsule(self.xy(seg.a), self.xy(seg.b), r, 2);
                    self.face(&poly, top, up, s);
                }
                self.mesh(&run.fill, top, up, s);
            }
            Primitive::Hairline { .. } => {}
        }
    }
}

fn radians(a: Millideg) -> f32 {
    (a.0 as f64 / 1000.0).to_radians() as f32
}

fn circle(c: [f32; 2], r: f32, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// A stadium around the segment `a`–`b`, counter-clockwise, `caps` steps per end.
fn capsule(a: [f32; 2], b: [f32; 2], r: f32, caps: usize) -> Vec<[f32; 2]> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    if dx.abs() + dy.abs() < 1e-6 {
        return circle(a, r, caps * 2 + 2);
    }
    let dir = dy.atan2(dx);
    let half = std::f32::consts::FRAC_PI_2;
    let mut out = Vec::with_capacity(caps * 2 + 2);
    for (c, start) in [(b, dir - half), (a, dir + half)] {
        for i in 0..=caps {
            let t = start + i as f32 / caps as f32 * std::f32::consts::PI;
            out.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
        }
    }
    out
}

fn rounded_rect(c: [f32; 2], size: (f32, f32), rot: f32, r: f32) -> Vec<[f32; 2]> {
    let (hw, hh) = (size.0 / 2.0, size.1 / 2.0);
    let r = r.min(hw).min(hh).max(0.0);
    let steps = if r > 1e-4 { 5 } else { 0 };
    let (sin, cos) = rot.sin_cos();
    let mut out = Vec::new();
    for (k, (sx, sy)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .into_iter()
        .enumerate()
    {
        let (cx, cy) = (sx * (hw - r), sy * (hh - r));
        for i in 0..=steps {
            let t = (k as f32 + i as f32 / steps.max(1) as f32) * std::f32::consts::FRAC_PI_2;
            let (x, y) = (cx + r * t.cos(), cy + r * t.sin());
            out.push([c[0] + x * cos - y * sin, c[1] + x * sin + y * cos]);
        }
    }
    out
}

/// A contour's arcs as short straight edges.
fn flatten(vertices: &[Vertex]) -> Vec<Point> {
    use reda_engine::model::geom::Nm;
    let mut out = Vec::new();
    for (i, v) in vertices.iter().enumerate() {
        let from = Point::new(v.x, v.y);
        out.push(from);
        if v.sweep.0 == 0 {
            continue;
        }
        let next = vertices[(i + 1) % vertices.len()];
        let to = Point::new(next.x, next.y);
        let c = arc_centre(from, to, v.sweep);
        let (dx, dy) = ((from.x.0 - c.x.0) as f64, (from.y.0 - c.y.0) as f64);
        let (r, start) = (dx.hypot(dy), dy.atan2(dx));
        let sweep = (v.sweep.0 as f64 / 1000.0).to_radians();
        let steps = (sweep.abs() / 0.15).ceil().max(1.0) as usize;
        for k in 1..steps {
            let t = start + sweep * k as f64 / steps as f64;
            out.push(Point::new(
                Nm(c.x.0 + (r * t.cos()).round() as i64),
                Nm(c.y.0 + (r * t.sin()).round() as i64),
            ));
        }
    }
    out
}

fn fill(contours: &[Vec<Point>]) -> Mesh {
    let contours: Vec<Vec<IntPoint<i64>>> = contours
        .iter()
        .map(|c| c.iter().map(|p| IntPoint::new(p.x.0, p.y.0)).collect())
        .collect();
    let shapes = contours.simplify(FillRule::EvenOdd, Default::default());
    let t = shapes.uncheck_triangulate().into_triangulation::<u32>();
    Mesh {
        points: (t.points.iter())
            .map(|p| {
                Point::new(
                    reda_engine::model::geom::Nm(p.x),
                    reda_engine::model::geom::Nm(p.y),
                )
            })
            .collect(),
        indices: t.indices,
    }
}

fn ccw(mut poly: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    let area: f32 = (0..poly.len())
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if area < 0.0 {
        poly.reverse();
    }
    poly
}

fn rgb(r: f32, g: f32, b: f32) -> [f32; 4] {
    [r, g, b, 1.0]
}

/// A colour typed into the Stackup ("green", "matte black"), when it names one we know.
fn named(text: Option<&str>) -> Option<[f32; 4]> {
    let t = text?.to_lowercase();
    [
        ("green", rgb(0.05, 0.32, 0.15)),
        ("red", rgb(0.55, 0.08, 0.08)),
        ("blue", rgb(0.05, 0.16, 0.48)),
        ("black", rgb(0.07, 0.07, 0.08)),
        ("white", rgb(0.90, 0.90, 0.88)),
        ("yellow", rgb(0.78, 0.66, 0.10)),
        ("purple", rgb(0.30, 0.10, 0.42)),
    ]
    .into_iter()
    .find(|(name, _)| t.contains(name))
    .map(|(_, c)| c)
}

fn lighter(c: [f32; 4], by: f32) -> [f32; 4] {
    [
        c[0] + (1.0 - c[0]) * by,
        c[1] + (1.0 - c[1]) * by,
        c[2] + (1.0 - c[2]) * by,
        c[3],
    ]
}

fn body_color(designator: &str) -> [f32; 4] {
    let prefix: String = designator
        .chars()
        .take_while(|c| c.is_alphabetic())
        .collect();
    match prefix.as_str() {
        "C" => rgb(0.66, 0.54, 0.38),
        "R" => rgb(0.10, 0.10, 0.11),
        "J" | "P" | "CN" | "USB" => rgb(0.80, 0.79, 0.75),
        "Y" | "X" => rgb(0.72, 0.73, 0.75),
        "D" | "LED" => rgb(0.86, 0.84, 0.70),
        "L" | "FB" => rgb(0.24, 0.24, 0.26),
        "SW" | "S" => rgb(0.55, 0.56, 0.58),
        _ => rgb(0.15, 0.15, 0.17),
    }
}

/// Builds the model. `visible` is the Layers the 2D canvas shows; `None` shows all.
pub fn build(
    doc: &PcbDocument,
    scene: &Scene,
    palette: &Palette,
    visible: Option<&HashSet<SceneLayer>>,
    look: Look,
) -> Model {
    let started = std::time::Instant::now();
    let outline = doc.outline().map(|o| flatten(&o.vertices));
    let bounds = match &outline {
        Some(points) if !points.is_empty() => {
            let xs = points.iter().map(|p| p.x.0);
            let ys = points.iter().map(|p| p.y.0);
            Some((
                (xs.clone().min().unwrap(), ys.clone().min().unwrap()),
                (xs.max().unwrap(), ys.max().unwrap()),
            ))
        }
        _ => (scene.bounds()).map(|(lo, hi)| ((lo.x.0, lo.y.0), (hi.x.0, hi.y.0))),
    };
    let ((x0, y0), (x1, y1)) = bounds.unwrap_or(((0, 0), (50_000_000, 50_000_000)));
    let center = ((x0 + x1) as f64 / 2.0, (y0 + y1) as f64 / 2.0);
    let radius = (((x1 - x0) as f64).hypot((y1 - y0) as f64) / 2e6) as f32;
    let thickness = (doc.stackup().thickness.0 as f32 / 1e6).max(0.1);
    let (z_top, z_bot) = (thickness / 2.0, -thickness / 2.0);
    let spread = (radius * 0.22).max(4.0);

    let mut b = Builder {
        tris: Vec::new(),
        center,
    };
    let mut objects: Vec<Uuid> = Vec::new();
    let mut index: HashMap<Uuid, u32> = HashMap::new();
    let mut object = |uuid: Uuid| {
        *index.entry(uuid).or_insert_with(|| {
            objects.push(uuid);
            objects.len() as u32 - 1
        })
    };

    let stackup = doc.stackup();
    let mask = named(stackup.mask_color.as_deref()).unwrap_or(rgb(0.05, 0.32, 0.15));
    let silk = named(stackup.silk_color.as_deref()).unwrap_or(rgb(0.93, 0.93, 0.90));
    let substrate = rgb(0.62, 0.56, 0.36);
    let bare_copper = rgb(0.80, 0.50, 0.26);
    let finish = rgb(0.84, 0.70, 0.34);
    let unit = |c: reda_engine::render::Rgba| {
        let c = c.0.map(|v| f32::from(v) / 255.0);
        [c[0], c[1], c[2], 1.0]
    };

    // The copper Layers, top to bottom.
    let coppers: Vec<Uuid> = doc.copper_layers().map(|l| l.uuid).collect();
    let mut layers = Vec::new();
    for (i, id) in coppers.iter().enumerate() {
        let layer = doc.layers().layer(*id);
        let name = match layer.and_then(|l| l.side) {
            Some(Side::Top) => "Top".to_owned(),
            Some(Side::Bottom) => "Bottom".to_owned(),
            None => (doc.inner_layer_label(*id)).unwrap_or_else(|| format!("Inner {i}")),
        };
        let color = layer.map_or(bare_copper, |l| {
            unit(palette.color(ColorKey::Board(BoardColor::for_layer(doc, l))))
        });
        layers.push(LayerRow {
            uuid: *id,
            name,
            color,
        });
    }
    let depth = |i: usize| -> f32 {
        // 1 at the top copper Layer, -1 at the bottom one.
        if coppers.len() < 2 {
            1.0
        } else {
            1.0 - 2.0 * i as f32 / (coppers.len() - 1) as f32
        }
    };

    // Holes: what the 2D canvas draws on its drill Layer.
    let hole_key = ColorKey::Board(BoardColor::Hole);
    let mut holes: Vec<Vec<[f32; 2]>> = Vec::new();
    for item in &scene.items {
        if item.layer != SceneLayer::Drill || item.color != hole_key {
            continue;
        }
        match &item.primitive {
            Primitive::Ring { center, outer, .. } => {
                holes.push(circle(b.xy(*center), outer.0 as f32 / 1e6, 16));
            }
            Primitive::Rect {
                center,
                size,
                rotation,
                corner_radius,
                ..
            } => holes.push(rounded_rect(
                b.xy(*center),
                (size.0.0 as f32 / 1e6, size.1.0 as f32 / 1e6),
                radians(*rotation),
                corner_radius.0 as f32 / 1e6,
            )),
            _ => {}
        }
    }

    // The board: two faces with the holes cut out, its edge, and the hole walls.
    if let Some(points) = outline.filter(|p| p.len() >= 3) {
        let to_nm = |p: &[f32; 2]| {
            use reda_engine::model::geom::Nm;
            Point::new(
                Nm((p[0] as f64 * 1e6 + center.0).round() as i64),
                Nm((p[1] as f64 * 1e6 + center.1).round() as i64),
            )
        };
        let cutouts: Vec<Vec<Point>> = doc.cutouts().map(|c| flatten(&c.vertices)).collect();
        let mut contours = vec![points.clone()];
        contours.extend(cutouts.iter().cloned());
        contours.extend(holes.iter().map(|h| h.iter().map(to_nm).collect()));
        let faces = fill(&contours);
        let face_color = match (look.realistic, look.mask) {
            (true, true) => mask,
            (true, false) => substrate,
            (false, _) => rgb(0.35, 0.37, 0.40),
        };
        let edge_color = if look.realistic {
            substrate
        } else {
            rgb(0.45, 0.47, 0.50)
        };
        let style = |color| Style {
            color,
            lift: 0.0,
            cat: BOARD,
            obj: NO_OBJECT,
            layer: NO_LAYER,
        };
        b.mesh(&faces, z_top, true, &style(face_color));
        b.mesh(&faces, z_bot, false, &style(face_color));
        let edge: Vec<[f32; 2]> = points.iter().map(|p| b.xy(*p)).collect();
        b.walls(&ccw(edge), z_bot, z_top, false, &style(edge_color));
        for cut in &cutouts {
            let cut: Vec<[f32; 2]> = cut.iter().map(|p| b.xy(*p)).collect();
            b.walls(&ccw(cut), z_bot, z_top, true, &style(edge_color));
        }
        let barrel = Style {
            color: if look.realistic {
                finish
            } else {
                rgb(0.55, 0.50, 0.30)
            },
            cat: HOLE,
            ..style(edge_color)
        };
        for hole in &holes {
            b.walls(&ccw(hole.clone()), z_bot, z_top, true, &barrel);
        }
    }

    // Copper and silkscreen, from the 2D Scene.
    for item in &scene.items {
        let SceneLayer::Board(id) = item.layer else {
            continue;
        };
        if visible.is_some_and(|v| !v.contains(&item.layer)) {
            continue;
        }
        if matches!(&item.primitive, Primitive::Glyphs(g) if g.view_label) {
            continue;
        }
        let Some(layer) = doc.layers().layer(id) else {
            continue;
        };
        let obj = object(item.object);
        let by_layer = unit(palette.color(item.color));
        match layer.kind {
            LayerKind::Copper => {
                let Some(i) = coppers.iter().position(|c| *c == id) else {
                    continue;
                };
                let is_pad = matches!(item.child, Some(Child::Pad { .. }))
                    || doc.free_pad(item.object).is_some();
                let color = match (look.realistic, look.mask, is_pad) {
                    (false, _, _) => by_layer,
                    (true, _, true) => finish,
                    (true, true, false) => lighter(mask, 0.30),
                    (true, false, false) => bare_copper,
                };
                let mut s = Style {
                    color,
                    lift: depth(i) * spread,
                    cat: COPPER,
                    obj,
                    layer: i as u16,
                };
                // Pads sit a hair proud of Tracks, so their finish wins where they overlap.
                let proud = if is_pad { 0.012 } else { 0.0 };
                match layer.side {
                    Some(Side::Top) => {
                        let z = (z_top, z_top + COPPER_MM + proud);
                        b.primitive(&item.primitive, z, true, false, &s);
                    }
                    Some(Side::Bottom) => {
                        let z = (z_bot - COPPER_MM - proud, z_bot);
                        b.primitive(&item.primitive, z, false, false, &s);
                    }
                    None => {
                        s.cat = INNER;
                        if look.realistic {
                            s.color = bare_copper;
                        }
                        let z = z_bot + thickness * (depth(i) + 1.0) / 2.0;
                        b.primitive(&item.primitive, (z, z), true, true, &s);
                    }
                }
            }
            LayerKind::Silkscreen => {
                let top = layer.side != Some(Side::Bottom);
                let s = Style {
                    color: if look.realistic { silk } else { by_layer },
                    lift: if top { 1.25 } else { -1.25 } * spread,
                    cat: SILK,
                    obj,
                    layer: NO_LAYER,
                };
                let z = if top {
                    z_top + COPPER_MM + 0.03
                } else {
                    z_bot - COPPER_MM - 0.03
                };
                b.primitive(&item.primitive, (z, z), top, true, &s);
            }
            _ => {}
        }
    }

    // Stand-in Bodies: each Footprint's body box, as tall as its `height`.
    let mut bodies = Vec::new();
    for inst in doc.footprints().filter(|f| !f.not_fitted) {
        let Some(copy) = doc.footprint_copy(inst.footprint) else {
            continue;
        };
        let Some(bbox) = copy.footprint.body_box() else {
            continue;
        };
        let (w, h) = (
            (bbox.max.x.0 - bbox.min.x.0) as f32 / 1e6,
            (bbox.max.y.0 - bbox.min.y.0) as f32 / 1e6,
        );
        // A Courtyard is the body plus its clearance; draw the body a little inside it.
        let inset = (w.min(h) * 0.12).min(0.25);
        let (lo, hi) = (
            [
                bbox.min.x.0 as f32 / 1e6 + inset,
                bbox.min.y.0 as f32 / 1e6 + inset,
            ],
            [
                bbox.max.x.0 as f32 / 1e6 - inset,
                bbox.max.y.0 as f32 / 1e6 - inset,
            ],
        );
        let top = inst.side != Side::Bottom;
        let (sin, cos) = radians(inst.rotation).sin_cos();
        let at = b.xy(inst.at);
        let place = |x: f32, y: f32| {
            let x = if top { x } else { -x };
            [at[0] + x * cos - y * sin, at[1] + x * sin + y * cos]
        };
        let poly = ccw(vec![
            place(lo[0], lo[1]),
            place(hi[0], lo[1]),
            place(hi[0], hi[1]),
            place(lo[0], hi[1]),
        ]);
        let stored = copy.footprint.height.map(|h| h.0 as f32 / 1e6);
        let tall = stored.unwrap_or_else(|| (w.min(h) * 0.45).clamp(0.4, 2.5));
        let obj = object(inst.uuid);
        let lift = if top { 1.8 } else { -1.8 } * spread;
        let color = if look.realistic {
            body_color(&inst.designator)
        } else {
            rgb(0.62, 0.64, 0.68)
        };
        let s = Style {
            color,
            lift,
            cat: BODY,
            obj,
            layer: NO_LAYER,
        };
        let base = COPPER_MM + 0.02;
        let z = if top {
            (z_top + base, z_top + base + tall)
        } else {
            (z_bot - base - tall, z_bot - base)
        };
        b.slab(
            &poly,
            z,
            top,
            false,
            &Style {
                color: lighter(color, 0.08),
                ..s
            },
        );
        b.walls(&poly, z.0, z.1, false, &s);
        let mid = place((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0);
        bodies.push(Body {
            obj,
            designator: inst.designator.clone(),
            height: stored,
            top: [mid[0], mid[1], if top { z.1 } else { z.0 }],
            lift,
        });
    }

    Model {
        tris: b.tris,
        objects,
        layers,
        bodies,
        center,
        radius: radius.max(5.0),
        thickness,
        build_ms: started.elapsed().as_secs_f32() * 1000.0,
    }
}

/// How the triangles are coloured and filtered on their way to the GPU.
pub struct Marks<'a> {
    pub selected: &'a HashSet<u32>,
    pub hidden_layers: &'a HashSet<u16>,
    pub selected_color: [f32; 3],
    /// Fade everything unselected towards this colour while something is selected.
    pub dim_to: Option<[f32; 3]>,
}

/// Vertex bytes grouped by kind, with each kind's (first vertex, count).
pub struct Encoded {
    pub bytes: Arc<Vec<u8>>,
    pub ranges: [(u32, u32); CATS],
}

pub const VERTEX_SIZE: u64 = 48;

pub fn encode(model: &Model, marks: &Marks<'_>) -> Encoded {
    let mut buckets: [Vec<&Tri>; CATS] = Default::default();
    for t in &model.tris {
        if t.layer == NO_LAYER || !marks.hidden_layers.contains(&t.layer) {
            buckets[t.cat as usize].push(t);
        }
    }
    let dim = marks.dim_to.filter(|_| !marks.selected.is_empty());
    let mut bytes = Vec::with_capacity(model.tris.len() * 3 * VERTEX_SIZE as usize);
    let mut ranges = [(0, 0); CATS];
    let mut first = 0u32;
    for (cat, bucket) in buckets.iter().enumerate() {
        for t in bucket {
            let mut c = t.color;
            if t.obj != NO_OBJECT && marks.selected.contains(&t.obj) {
                for k in 0..3 {
                    c[k] = c[k] * 0.2 + marks.selected_color[k] * 0.8;
                }
            } else if let Some(bg) = dim {
                for k in 0..3 {
                    c[k] = c[k] * 0.5 + bg[k] * 0.5;
                }
            }
            for p in t.p {
                for f in p.into_iter().chain(t.n).chain(c).chain([t.lift]) {
                    bytes.extend_from_slice(&f.to_le_bytes());
                }
                bytes.extend_from_slice(&t.cat.to_le_bytes());
            }
        }
        let count = bucket.len() as u32 * 3;
        ranges[cat] = (first, count);
        first += count;
    }
    Encoded {
        bytes: Arc::new(bytes),
        ranges,
    }
}

/// What is shown, for picking: each kind's opacity, how far the Layers are spread, and
/// the cut (everything with x beyond it is gone).
pub struct Shown<'a> {
    pub alpha: [f32; CATS],
    pub explode: f32,
    pub cut_x: f32,
    pub hidden_layers: &'a HashSet<u16>,
}

/// The nearest triangle the ray hits: (triangle index, hit point).
pub fn pick(model: &Model, origin: V3, dir: V3, shown: &Shown<'_>) -> Option<(usize, V3)> {
    let mut best: Option<(f32, usize)> = None;
    for (i, t) in model.tris.iter().enumerate() {
        if shown.alpha[t.cat as usize] <= 0.0
            || (t.layer != NO_LAYER && shown.hidden_layers.contains(&t.layer))
        {
            continue;
        }
        let up = [0.0, 0.0, t.lift * shown.explode];
        let (a, b, c) = (add(t.p[0], up), add(t.p[1], up), add(t.p[2], up));
        let (e1, e2) = (sub(b, a), sub(c, a));
        let h = cross(dir, e2);
        let det = dot(e1, h);
        if det.abs() < 1e-9 {
            continue;
        }
        let s = sub(origin, a);
        let u = dot(s, h) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = cross(s, e1);
        let v = dot(dir, q) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let d = dot(e2, q) / det;
        if d <= 0.0 || best.is_some_and(|(b, _)| b <= d) {
            continue;
        }
        if add(origin, scale(dir, d))[0] > shown.cut_x {
            continue;
        }
        best = Some((d, i));
    }
    best.map(|(d, i)| (i, add(origin, scale(dir, d))))
}
