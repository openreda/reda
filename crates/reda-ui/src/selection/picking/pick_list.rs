//! The double-click pick list: every object of a stack, grouped by Layer, with a live
//! preview of the hovered entry and a lock on protected ones.

use reda_engine::model::Uuid;
use reda_engine::render::{Child, Scene, SceneLayer};
use serde_json::json;

use super::{document_scene, hit_stack_on, rank_with_locks, rules, selected, set_command};
use crate::shell::Shell;
use crate::working_state::FilterKind;

#[derive(Clone, Debug)]
pub struct PickList {
    document: Uuid,
    objects: Vec<Uuid>,
    at: egui::Pos2,
}

/// A double click explicitly offers protected objects as well, so they can be unlocked.
pub fn double_click(
    shell: &mut Shell,
    document: Uuid,
    at: (f64, f64),
    tolerance: f64,
    screen: egui::Pos2,
) {
    let Some(tab) = shell.document_tab(document) else {
        return;
    };
    let scene = document_scene(shell, document);
    let mut objects = hit_stack_on(&scene, tab.scene.layers.as_deref(), at, tolerance, |i| {
        rank_with_locks(shell, document, i, false)
    });
    if objects.len() <= 1 {
        if let Some(id) = objects.first() {
            let (set, params) = set_command(&selected(shell, document, &[*id]));
            let _ = shell.run_reporting(&set, params);
            let _ = shell.run_reporting("panel.show", json!({"panel":"properties"}));
        }
        return;
    }
    // The clicks of the double-click picked from the stack already; the list opens over
    // the selection from before them, and only a choice in it changes the selection.
    if let Some((_, before)) = shell
        .selection_picking_mut()
        .gesture
        .take()
        .filter(|(d, _)| *d == document)
        && before != *shell.selection()
    {
        let (set, params) = set_command(&before);
        let _ = shell.run_reporting(&set, params);
    }
    let layers = layer_list(shell, document);
    let layer = |id: Uuid| scene.items.iter().find(|i| i.object == id).map(|i| i.layer);
    objects.sort_by_key(|id| {
        (
            match layer(*id) {
                Some(SceneLayer::Board(id)) => layers
                    .iter()
                    .position(|l| l.uuid == id)
                    .unwrap_or(usize::MAX - 1),
                _ => usize::MAX,
            },
            *id,
        )
    });
    shell.selection_picking_mut().list = Some(PickList {
        document,
        objects,
        at: screen,
    });
}

/// Layer-grouped visual choices with a live canvas preview. Hover never changes Selection.
pub fn show(ui: &egui::Ui, shell: &mut Shell, document: Uuid) {
    let Some(list) = shell
        .selection_picking_mut()
        .list
        .clone()
        .filter(|l| l.document == document)
    else {
        return;
    };
    let mut open = true;
    let mut preview = None;
    let mut choose = None;
    let scene = document_scene(shell, document);
    let layers = layer_list(shell, document);
    egui::Window::new(crate::tr!("selection-pick-title"))
        .id(egui::Id::NULL.with(("pick-object", document)))
        .default_pos(list.at)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            let mut previous = None;
            for (ordinal, id) in list.objects.iter().enumerate() {
                if !rules::allowed(shell, document, *id) {
                    continue;
                }
                let layer = scene
                    .items
                    .iter()
                    .find(|i| i.object == *id)
                    .map(|i| i.layer);
                if layer != previous {
                    let name = match layer {
                        Some(SceneLayer::Board(layer)) => shell
                            .engine()
                            .project()
                            .and_then(|p| p.pcb_document(document))
                            .and_then(|p| p.layers().layer(layer))
                            .map(|l| {
                                crate::panels::layers::layer_name(
                                    shell
                                        .engine()
                                        .project()
                                        .and_then(|p| p.pcb_document(document))
                                        .expect("board"),
                                    l,
                                )
                            })
                            .or_else(|| {
                                layers
                                    .iter()
                                    .find(|l| l.uuid == layer)
                                    .map(editor_layer_name)
                            })
                            .unwrap_or_else(|| crate::tr!("selection-pick-layer")),
                        Some(SceneLayer::Drill) => crate::tr!("selection-pick-drill"),
                        _ => crate::tr!("selection-pick-sheet"),
                    };
                    ui.strong(name);
                    previous = layer;
                }
                let label = entry_label(shell, document, *id, ordinal);
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 20.0), egui::Sense::hover());
                    thumbnail(ui, &scene, *id, rect);
                    let row = ui.selectable_label(false, label);
                    if row.hovered() {
                        preview = Some(*id);
                    }
                    if row.clicked() {
                        choose = Some(*id);
                    }
                    if rules::locked(shell, document, *id) {
                        let (lock, response) =
                            ui.allocate_exact_size(egui::vec2(16.0, 20.0), egui::Sense::hover());
                        let color = ui.visuals().text_color();
                        ui.painter().circle_stroke(
                            lock.center() - egui::vec2(0.0, 3.0),
                            4.0,
                            egui::Stroke::new(1.5, color),
                        );
                        ui.painter().rect_filled(
                            egui::Rect::from_center_size(
                                lock.center() + egui::vec2(0.0, 3.0),
                                egui::vec2(11.0, 9.0),
                            ),
                            2.0,
                            color,
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Image,
                                true,
                                crate::tr!("selection-pick-locked"),
                            )
                        });
                    }
                });
            }
        });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    if let Some(id) = choose {
        let (set, params) = set_command(&selected(shell, document, &[id]));
        let _ = shell.run_reporting(&set, params);
        open = false;
    }
    if let Some(tab) = shell.document_tab_mut(document) {
        tab.live = crate::canvas::LiveHandle::new(if open { preview } else { None }, &[], &[]);
    }
    if !open {
        shell.selection_picking_mut().list = None;
    }
}

fn layer_list(shell: &Shell, document: Uuid) -> Vec<reda_engine::model::Layer> {
    if let Some(pcb) = shell
        .engine()
        .project()
        .and_then(|p| p.pcb_document(document))
    {
        return pcb.layers().layers.clone();
    }
    if let Some(reda_engine::model::LibraryElement::Footprint(f)) = shell.engine().element(document)
    {
        return reda_engine::footprint_scene(f, shell.engine().fonts()).layers;
    }
    Vec::new()
}

fn editor_layer_name(layer: &reda_engine::model::Layer) -> String {
    use reda_engine::model::{LayerKind, Side};
    let kind = match layer.kind {
        LayerKind::Copper => crate::tr!("layer-copper"),
        LayerKind::Silkscreen => crate::tr!("layer-silkscreen"),
        LayerKind::SolderMask => crate::tr!("layer-solder-mask"),
        LayerKind::Paste => crate::tr!("layer-paste"),
        LayerKind::Courtyard => crate::tr!("layer-courtyard"),
        LayerKind::Assembly => crate::tr!("layer-assembly"),
        LayerKind::BoardOutline => crate::tr!("layer-board-outline"),
        LayerKind::Mechanical => crate::tr!("layer-mechanical"),
        LayerKind::Documentation => crate::tr!("layer-documentation"),
    };
    match layer.side {
        Some(Side::Top) => crate::tr!("layer-top", kind = kind.as_str()),
        Some(Side::Bottom) => crate::tr!("layer-bottom", kind = kind.as_str()),
        None => kind,
    }
}

fn entry_label(shell: &Shell, document: Uuid, id: Uuid, ordinal: usize) -> String {
    use reda_engine::model::Object;
    let kind = crate::selection::filter_ui::kind_label(
        rules::kind(shell, document, id).unwrap_or(FilterKind::Graphic),
    );
    if let Some(object) = shell.engine().elements().and_then(|p| p.object(id)) {
        let detail = match object {
            Object::Unit(u) => u.object.designator,
            Object::FootprintInstance(f) => f.object.designator,
            Object::Wire(w) => crate::tr!(
                "selection-pick-length",
                length = format!(
                    "{:.2}",
                    w.segments()
                        .map(|(a, b)| ((b.x - a.x).0 as f64).hypot((b.y - a.y).0 as f64))
                        .sum::<f64>()
                        / 1e6
                )
            ),
            Object::FreePad(p) => crate::tr!(
                "selection-pick-size",
                width = format!("{:.2}", p.width.0 as f64 / 1e6),
                height = format!("{:.2}", p.height.0 as f64 / 1e6)
            ),
            _ => format!("{}", ordinal + 1),
        };
        return crate::tr!(
            "selection-pick-entry",
            kind = kind.as_str(),
            detail = detail
        );
    }
    if let Some(item) = rules::child_item(shell, document, id) {
        if let reda_engine::render::Primitive::Glyphs(run) = &item.primitive {
            return crate::tr!(
                "selection-pick-entry",
                kind = kind.as_str(),
                detail = run.text.as_str()
            );
        }
        if let Some(Child::Pad { footprint, pad }) = &item.child
            && let Some(pcb) = shell
                .engine()
                .project()
                .and_then(|p| p.pcb_document(document))
            && let Some(f) = pcb.footprint(*footprint)
            && let Some(p) = pcb
                .footprint_copy(f.footprint)
                .and_then(|copy| copy.footprint.pads.iter().find(|p| p.uuid == *pad))
        {
            return crate::tr!(
                "selection-pick-pad",
                kind = kind.as_str(),
                number = p.number.as_str(),
                designator = f.designator.as_str()
            );
        }
    }
    crate::tr!(
        "selection-pick-entry",
        kind = kind.as_str(),
        detail = (ordinal + 1).to_string()
    )
}

fn thumbnail(ui: &egui::Ui, scene: &Scene, object: Uuid, rect: egui::Rect) {
    use reda_engine::render::Primitive;
    let items: Vec<_> = scene.items.iter().filter(|i| i.object == object).collect();
    let bounds = items
        .iter()
        .filter_map(|i| i.primitive.bounds())
        .reduce(|(a, b), (c, d)| {
            (
                reda_engine::model::geom::Point::new(a.x.min(c.x), a.y.min(c.y)),
                reda_engine::model::geom::Point::new(b.x.max(d.x), b.y.max(d.y)),
            )
        });
    let Some((lo, hi)) = bounds else {
        return;
    };
    let scale = ((rect.width() - 4.0) as f64 / ((hi.x - lo.x).0.max(1) as f64))
        .min((rect.height() - 4.0) as f64 / ((hi.y - lo.y).0.max(1) as f64));
    let center = (
        ((lo.x.0 + hi.x.0) as f64 / 2.0),
        ((lo.y.0 + hi.y.0) as f64 / 2.0),
    );
    let at = |p: reda_engine::model::geom::Point| {
        rect.center()
            + egui::vec2(
                ((p.x.0 as f64 - center.0) * scale) as f32,
                ((center.1 - p.y.0 as f64) * scale) as f32,
            )
    };
    let color = ui.visuals().selection.stroke.color;
    let stroke = egui::Stroke::new(1.3, color);
    for item in items {
        match &item.primitive {
            Primitive::Capsule { a, b, .. } => {
                ui.painter().line_segment([at(*a), at(*b)], stroke);
            }
            Primitive::Ring { center, outer, .. } => {
                ui.painter()
                    .circle_stroke(at(*center), (outer.0 as f64 * scale) as f32, stroke);
            }
            Primitive::Glyphs(_) => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "T",
                    egui::FontId::proportional(14.0),
                    color,
                );
            }
            other => {
                if let Some((a, b)) = other.bounds() {
                    ui.painter().rect_stroke(
                        egui::Rect::from_two_pos(at(a), at(b)),
                        1.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
    }
}
