//! Live hover: an outline around the Net under the cursor on the board, as a Preference
//! (`pcb.view.live_hover`) with Off, On and Shift only (the default). The canvas notes the
//! Net under the cursor as it moves ([`net_at`]); the decoration draws the outline of each
//! of the Net's copper items ([`outline`]) while the Preference lets it show.

use egui::{Stroke, Ui};
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{PcbDocument, Uuid};
use reda_engine::preferences::LiveHover;
use reda_engine::render::{Child, Item, Primitive, Scene, SceneLayer};

use crate::canvas::CanvasInputs;
use crate::shell::Shell;

/// Whether the outline shows under `mode` with Shift `shift` held.
pub fn shows(mode: LiveHover, shift: bool) -> bool {
    match mode {
        LiveHover::Off => false,
        LiveHover::On => true,
        LiveHover::ShiftOnly => shift,
    }
}

/// The Net of the object picked as `hit` (an object or a Pad's child id) in `scene`: a
/// Track, Via, Zone, free Pad or Footprint Pad on a Net.
pub fn net_at(pcb: &PcbDocument, scene: &Scene, hit: Uuid) -> Option<Uuid> {
    if let Some(Child::Pad { footprint, pad }) = scene
        .items
        .iter()
        .filter_map(|i| i.child.as_ref())
        .find(|c| c.id() == hit)
    {
        let footprint = pcb.footprint(*footprint)?;
        return footprint.pads.iter().find(|p| p.pad == *pad)?.net;
    }
    pcb.track(hit)
        .and_then(|t| t.net)
        .or_else(|| pcb.via(hit).and_then(|v| v.net))
        .or_else(|| pcb.zone(hit).and_then(|z| z.net))
        .or_else(|| pcb.free_pad(hit).and_then(|p| p.net))
}

/// The copper items of Net `net` in `scene`: its Tracks, Vias, Zone fills and Pads, on
/// each copper Layer they are on; their labels are left out.
pub fn net_items<'s>(pcb: &PcbDocument, scene: &'s Scene, net: Uuid) -> Vec<&'s Item> {
    let on = Some(net);
    let of_net = |item: &Item| match &item.child {
        Some(Child::Pad { footprint, pad }) => pcb
            .footprint(*footprint)
            .and_then(|f| f.pads.iter().find(|p| p.pad == *pad))
            .is_some_and(|p| p.net == on),
        Some(_) => false,
        None => {
            let o = item.object;
            pcb.track(o).is_some_and(|t| t.net == on)
                || pcb.via(o).is_some_and(|v| v.net == on)
                || pcb.zone(o).is_some_and(|z| z.net == on)
                || pcb.free_pad(o).is_some_and(|p| p.net == on)
        }
    };
    scene
        .items
        .iter()
        .filter(|i| match i.layer {
            SceneLayer::Board(l) => pcb
                .layers()
                .layer(l)
                .is_some_and(|l| l.kind == reda_engine::model::LayerKind::Copper),
            _ => false,
        })
        .filter(|i| {
            !matches!(
                i.primitive,
                Primitive::Glyphs(_) | Primitive::Hairline { .. }
            )
        })
        .filter(|i| of_net(i))
        .collect()
}

/// Points on each round end of a stadium.
const ARC_STEPS: usize = 12;

/// The edge of `primitive` as polylines in board coordinates; a closed one repeats its
/// first point at the end. A Track is its stadium, a Pad its rectangle, a Via or round
/// Pad its circle, a Zone fill its boundary edges. Lines and text have none.
pub fn outline(primitive: &Primitive) -> Vec<Vec<Point>> {
    let at = |x: f64, y: f64| Point::new(Nm(x.round() as i64), Nm(y.round() as i64));
    match primitive {
        Primitive::Capsule { a, b, width } => {
            let r = width.0 as f64 / 2.0;
            let (ax, ay, bx, by) = (a.x.0 as f64, a.y.0 as f64, b.x.0 as f64, b.y.0 as f64);
            let angle = (by - ay).atan2(bx - ax);
            let half = |cx: f64, cy: f64, from: f64| {
                (0..=ARC_STEPS).map(move |i| {
                    let t = from + std::f64::consts::PI * i as f64 / ARC_STEPS as f64;
                    at(cx + r * t.cos(), cy + r * t.sin())
                })
            };
            let right = std::f64::consts::FRAC_PI_2;
            let mut points: Vec<Point> = half(bx, by, angle - right)
                .chain(half(ax, ay, angle + right))
                .collect();
            points.push(points[0]);
            vec![points]
        }
        Primitive::Rect {
            center,
            size,
            rotation,
            ..
        } => {
            let (w, h) = (size.0.0 as f64 / 2.0, size.1.0 as f64 / 2.0);
            let t = (rotation.0 as f64 / 1000.0).to_radians();
            let (c, s) = (t.cos(), t.sin());
            let (cx, cy) = (center.x.0 as f64, center.y.0 as f64);
            let mut points: Vec<Point> = [(-w, -h), (w, -h), (w, h), (-w, h), (-w, -h)]
                .into_iter()
                .map(|(x, y)| at(cx + x * c - y * s, cy + x * s + y * c))
                .collect();
            points.dedup();
            vec![points]
        }
        Primitive::Ring { center, outer, .. } => {
            let r = outer.0 as f64;
            let (cx, cy) = (center.x.0 as f64, center.y.0 as f64);
            let steps = 2 * ARC_STEPS;
            vec![
                (0..=steps)
                    .map(|i| {
                        let t = std::f64::consts::TAU * i as f64 / steps as f64;
                        at(cx + r * t.cos(), cy + r * t.sin())
                    })
                    .collect(),
            ]
        }
        Primitive::Triangles(_) => super::view::draft(primitive)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Hairline { a, b } => Some(vec![a, b]),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Draws the live hover outline over the board canvas and notes what it shows
/// ([`Shell::live_hover_shown`]).
pub fn draw(ui: &Ui, shell: &mut Shell, input: &CanvasInputs) {
    let mode = shell.preferences().pcb.view.live_hover;
    let shift = ui.input(|i| i.modifiers.shift);
    let net = shell
        .live_hover_candidate()
        .filter(|_| input.response.hovered() && shows(mode, shift));
    shell.set_live_hover_shown(net);
    let Some(net) = net else {
        return;
    };
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return;
    };
    let Some(tab) = shell.document_tab(pcb.uuid) else {
        return;
    };
    let painter = ui.painter_at(input.rect);
    let stroke = Stroke::new(2.0, ui.visuals().selection.stroke.color);
    for item in net_items(pcb, &tab.scene.scene, net) {
        for line in outline(&item.primitive) {
            let points: Vec<egui::Pos2> = line
                .iter()
                .map(|p| input.screen_point((p.x.0 as f64, p.y.0 as f64)))
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
    }
}
