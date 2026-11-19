//! The base artwork of the Stackup preview. The Scene is in nm, y up, with its
//! origin on the upper left copper surface. Selection and drag feedback are overlays.
use super::{Band, Barrel, Scale, color, drawing, mask_color, mm};
use crate::theme::Tokens;
use crate::widgets;
use egui::{Align2, Color32, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{PcbDocument, StackupInfo, Uuid};
use reda_engine::render::{
    Align, ColorKey, GlyphRun, HAlign, Item, Mesh, Primitive, Scene, SceneLayer, SceneSource,
    VAlign,
};
use std::collections::BTreeMap;

#[derive(PartialEq)]
pub(super) struct Names {
    pub bands: Vec<String>,
    pub callouts: Vec<String>,
    pub vias: Vec<String>,
}

impl Names {
    pub fn new(pcb: &PcbDocument, info: &StackupInfo) -> Self {
        let bands: Vec<_> = (0..info.entries.len())
            .map(|i| super::super::entry_name(pcb, i))
            .collect();
        let callouts = bands
            .iter()
            .zip(&info.entries)
            .map(|(name, e)| format!("{name} · {} mm", mm(e.thickness)))
            .collect();
        let vias = pcb
            .stackup()
            .via_types
            .iter()
            .map(|v| crate::pcb::via_type_name(pcb, v))
            .collect();
        Self {
            bands,
            callouts,
            vias,
        }
    }
}

struct Projection {
    origin: Pos2,
    nm_per_point: f64,
}
impl Projection {
    fn nm(&self, length: f32) -> Nm {
        Nm((f64::from(length) * self.nm_per_point).round() as i64)
    }
    fn length(&self, nm: Nm) -> f32 {
        (nm.0 as f64 / self.nm_per_point) as f32
    }
    fn point(&self, p: Pos2) -> Point {
        Point::new(self.nm(p.x - self.origin.x), self.nm(self.origin.y - p.y))
    }
    fn screen(&self, p: Point) -> Pos2 {
        self.origin + vec2(self.length(p.x), -self.length(p.y))
    }
}

pub(super) struct Section {
    pub scene: Scene,
    pub bands: Vec<Band>,
    pub barrels: Vec<(usize, Barrel)>,
    projection: Projection,
    stack: Uuid,
}

impl Section {
    pub fn new(
        pcb: &PcbDocument,
        info: &StackupInfo,
        names: &Names,
        body: Rect,
        scale: Scale,
        pitch: f32,
        t: &Tokens,
    ) -> Self {
        let stack = pcb.stackup();
        let bands = drawing::bands(pcb, info, body, scale, t.ty.small.line);
        let first =
            body.center().x - (stack.via_types.len().saturating_sub(1) as f32) * pitch / 2.0;
        let mut s = Self {
            scene: Scene::new(SceneSource::Pcb(pcb.uuid)),
            projection: Projection {
                origin: body.min,
                nm_per_point: info.totals.total.0.max(1) as f64 / f64::from(body.height()),
            },
            bands: Vec::new(),
            barrels: Vec::new(),
            stack: stack.uuid,
        };
        let mask = mask_color(stack.mask_color.as_deref());
        for y in [body.top() - t.stroke.accent_line, body.bottom()] {
            s.rect(
                stack.uuid,
                Rect::from_min_size(
                    pos2(body.left(), y),
                    vec2(body.width(), t.stroke.accent_line),
                ),
                mask,
                None,
            );
        }
        for (i, band) in bands.iter().enumerate() {
            s.rect(band.id, band.rect, color(&stack.entries[i]), None);
            let end = pos2(body.right() + t.space.gap, band.label_y);
            s.line(
                band.id,
                [band.rect.right_center(), end],
                t.stroke.hairline,
                t.color.stroke.strong,
            );
            s.label(
                band.id,
                end + vec2(t.space.gap, 0.0),
                HAlign::Left,
                &names.callouts[i],
                t.ty.small.size,
                t.color.text.primary,
            );
        }
        let x = body.left() - t.space.gap;
        let ink = t.color.text.primary;
        s.line(
            stack.uuid,
            [pos2(x, body.top()), pos2(x, body.bottom())],
            t.stroke.hairline,
            ink,
        );
        for y in [body.top(), body.bottom()] {
            s.line(
                stack.uuid,
                [pos2(x - t.space.gap, y), pos2(body.left(), y)],
                t.stroke.hairline,
                ink,
            );
        }
        s.label(
            stack.uuid,
            pos2(x - t.space.gap, body.center().y),
            HAlign::Right,
            &format!("{} mm", info.totals.total.mm_exact()),
            t.ty.numeric.size,
            ink,
        );
        let copper = widgets::data_color(0xc8, 0x75, 0x33);
        for (i, kind) in stack.via_types.iter().enumerate() {
            let x = first + pitch * i as f32;
            let Some(barrel) = drawing::barrel(&bands, kind, x, t.size.of(13.0)) else {
                continue;
            };
            for pad in &barrel.pads {
                s.rect(kind.uuid, *pad, copper, None);
                s.rect(
                    kind.uuid,
                    *pad,
                    t.color.stroke.strong,
                    Some(t.stroke.hairline),
                );
            }
            let [a, b, c, d] = barrel.hole;
            let mesh = Mesh {
                points: barrel.hole.map(|p| s.projection.point(p)).to_vec(),
                indices: vec![0, 1, 2, 0, 2, 3],
            };
            s.push(kind.uuid, t.color.surface.base, Primitive::Triangles(mesh));
            for line in [[a, d], [b, c]] {
                s.line(kind.uuid, line, t.stroke.accent_line, copper);
            }
            if kind.micro {
                s.line(kind.uuid, [d, c], t.stroke.accent_line, copper);
            }
            s.label(
                kind.uuid,
                pos2(x, body.top() - t.space.gap),
                HAlign::Center,
                &names.vias[i],
                t.ty.small.size,
                ink,
            );
            s.barrels.push((i, barrel));
        }
        s.bands = bands;
        s
    }

    fn push(&mut self, object: Uuid, color: Color32, primitive: Primitive) {
        self.scene.items.push(Item {
            object,
            child: None,
            layer: SceneLayer::Annotation,
            color: ColorKey::Fixed([color.r(), color.g(), color.b()]),
            primitive,
            look: Default::default(),
        });
    }
    fn rect(&mut self, id: Uuid, rect: Rect, color: Color32, stroke: Option<f32>) {
        let p = &self.projection;
        self.push(
            id,
            color,
            Primitive::Rect {
                center: p.point(rect.center()),
                size: (p.nm(rect.width()), p.nm(rect.height())),
                rotation: Millideg(0),
                corner_radius: Nm(0),
                stroke: stroke.map(|w| p.nm(w)),
            },
        );
    }
    fn line(&mut self, id: Uuid, [a, b]: [Pos2; 2], width: f32, color: Color32) {
        let p = &self.projection;
        self.push(
            id,
            color,
            Primitive::Capsule {
                a: p.point(a),
                b: p.point(b),
                width: p.nm(width),
            },
        );
    }
    fn label(&mut self, id: Uuid, at: Pos2, h: HAlign, text: &str, size: f32, color: Color32) {
        let v = if h == HAlign::Center {
            VAlign::Baseline
        } else {
            VAlign::Middle
        };
        self.push(
            id,
            color,
            Primitive::Glyphs(GlyphRun::label(
                text,
                self.projection.point(at),
                self.projection.nm(size),
                Align::new(h, v),
            )),
        );
    }

    /// Paints the Scene at the tab's current scale and returns the labels' click targets.
    pub fn paint(
        &self,
        draw: &Painter,
        t: &Tokens,
        selected: Option<Uuid>,
    ) -> BTreeMap<Uuid, Rect> {
        let mut labels = BTreeMap::new();
        let p = &self.projection;
        for item in &self.scene.items {
            let ColorKey::Fixed([r, g, b]) = item.color else {
                unreachable!("cross-section ink")
            };
            let ink = widgets::data_color(r, g, b);
            match &item.primitive {
                Primitive::Rect {
                    center,
                    size: (w, h),
                    stroke,
                    ..
                } => {
                    let rect =
                        Rect::from_center_size(p.screen(*center), vec2(p.length(*w), p.length(*h)));
                    if let Some(w) = stroke {
                        draw.rect_stroke(
                            rect,
                            0.0,
                            Stroke::new(p.length(*w), ink),
                            StrokeKind::Inside,
                        );
                    } else {
                        draw.rect_filled(rect, 0.0, ink);
                    }
                }
                Primitive::Capsule { a, b, width } => {
                    draw.line_segment(
                        [p.screen(*a), p.screen(*b)],
                        Stroke::new(p.length(*width), ink),
                    );
                }
                Primitive::Triangles(mesh) => {
                    draw.add(egui::Shape::convex_polygon(
                        mesh.points.iter().map(|q| p.screen(*q)).collect(),
                        ink,
                        Stroke::NONE,
                    ));
                }
                Primitive::Glyphs(run) => {
                    let align = match run.align.h {
                        HAlign::Left => Align2::LEFT_CENTER,
                        HAlign::Right => Align2::RIGHT_CENTER,
                        HAlign::Center => Align2::CENTER_BOTTOM,
                    };
                    let ty = if item.object == self.stack {
                        &t.ty.numeric
                    } else {
                        &t.ty.small
                    };
                    let ink = if Some(item.object) == selected {
                        t.color.accent.text
                    } else {
                        ink
                    };
                    let rect = draw.text(p.screen(run.at), align, &run.text, ty.font_id(), ink);
                    labels.insert(item.object, rect);
                }
                _ => unreachable!("cross-section primitive"),
            }
        }
        labels
    }
}
