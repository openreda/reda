//! The Layer control: switching Layers with the Layers panel collapsed. A Preference
//! (`pcb.view.layer_control`) picks its form, and neither changes any output:
//!
//! - **Swatch** (default): a chip at the canvas's top right with the active Layer's
//!   colour and name. Its dropdown lists the Layers copper first, each as a chip, then
//!   the Layer sets and **Save current as…**.
//! - **Strip:** a 24 px row along the canvas bottom with a Layer sets button and a chip
//!   per shown Layer, copper first. The Tool's bar and the grid readout move up to clear
//!   it ([`crate::bars::draw::bottom_inset`]).
//!
//! In either, a click on a chip activates the Layer (`pcb.layer.activate`) and `Mod`+click
//! highlights its content (`pcb.layer.highlight`). Drawing only: each click is a Command
//! returned for the window to run.

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Order, Rect, Response, Role, Sense, Stroke, Ui,
    Vec2, WidgetInfo, vec2,
};
use reda_engine::model::{Layer, LayerKind, PcbDocument, Uuid};
use reda_engine::preferences::LayerControl;
use reda_engine::render::{BoardColor, ColorKey};
use serde_json::{Value, json};

use super::view_commands::LAYER_SETS;
use crate::panels::layers::layer_name;
use crate::shell::Shell;
use crate::{theme, tr};

/// A Command for the window to run (one lacking params opens its dialog).
pub type Invocation = (String, Value);

/// The chip's colour square, in points.
const SQUARE: f32 = 10.0;
/// Under this width the swatch drops its name, so the centred Active Bar stays clear.
const NARROW_CANVAS: f32 = 600.0;
/// How far the swatch sits from the canvas's right edge.
const SWATCH_INSET: f32 = 8.0;

/// Whether the board canvas shows the Layer strip: the Preference asks for it and the
/// Project has a board.
pub fn strip_shown(shell: &Shell) -> bool {
    shell.preferences().pcb.view.layer_control == LayerControl::Strip
        && shell.engine().project().and_then(|p| p.pcb()).is_some()
}

/// The board's Layers in the Layer control's order: copper first, top to bottom, then
/// the rest in the board's order.
pub fn ordered(pcb: &PcbDocument) -> Vec<&Layer> {
    let layers = &pcb.layers().layers;
    let copper = layers.iter().filter(|l| l.kind == LayerKind::Copper);
    let rest = layers.iter().filter(|l| l.kind != LayerKind::Copper);
    copper.chain(rest).collect()
}

/// A built-in Layer set's name.
pub fn set_title(set: &str) -> String {
    match set {
        "all" => tr!("layers-set-all"),
        "signal" => tr!("layers-set-signal"),
        "non_signal" => tr!("layers-set-non-signal"),
        "mechanical" => tr!("layers-set-mechanical"),
        "top" => tr!("layers-set-top"),
        "bottom" => tr!("layers-set-bottom"),
        _ => tr!("layers-set-fab"),
    }
}

/// The Layer set buttons, built-in sets then saved ones, and **Save current as…**:
/// pushes `pcb.view.layer_set.apply` or the paramless `save` (whose dialog asks the name).
pub fn set_buttons(ui: &mut Ui, shell: &Shell, out: &mut Vec<Invocation>) {
    let saved = shell.preferences().pcb.view.layer_sets.iter();
    let sets = LAYER_SETS
        .iter()
        .map(|s| (s.to_string(), set_title(s)))
        .chain(saved.map(|s| (s.name.clone(), s.name.clone())));
    for (set, title) in sets {
        if ui.button(title).clicked() {
            out.push(("pcb.view.layer_set.apply".into(), json!({ "set": set })));
        }
    }
    if ui.button(tr!("layers-set-save")).clicked() {
        out.push(("pcb.view.layer_set.save".into(), json!({})));
    }
}

/// The Layer control over the board canvas `canvas` (id `id`), in the form the
/// Preference picks.
pub fn show(ui: &Ui, shell: &Shell, id: Id, canvas: Rect) -> Vec<Invocation> {
    let mut out = Vec::new();
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return out;
    };
    if strip_shown(shell) {
        strip(ui, shell, pcb, id, canvas, &mut out);
    } else {
        swatch(ui, shell, pcb, id, canvas, &mut out);
    }
    out
}

/// The swatch: the active Layer's chip at the top right, opening the Layer list.
fn swatch(
    ui: &Ui,
    shell: &Shell,
    pcb: &PcbDocument,
    id: Id,
    canvas: Rect,
    out: &mut Vec<Invocation>,
) {
    let view = shell.board_view();
    let active = shell.engine().project().and_then(|p| view.active_layer(p));
    let Some(active) = active.and_then(|a| pcb.layers().layer(a)) else {
        return;
    };
    egui::Area::new(id.with("layer-swatch"))
        .order(Order::Middle)
        .fixed_pos(canvas.right_top() + vec2(-SWATCH_INSET, crate::bars::draw::ACTIVE_BAR_TOP))
        .pivot(Align2::RIGHT_TOP)
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(egui::Margin::same(3))
                .corner_radius(CornerRadius::same(5))
                .show(ui, |ui| {
                    let name = layer_name(pcb, active);
                    let label = tr!("layer-control-active", layer = name.as_str());
                    // On a narrow canvas the swatch shows its colour only, clear of the
                    // Active Bar; the name is in its tooltip.
                    let face = if canvas.width() < NARROW_CANVAS {
                        chip(ui, shell, active, "", label, true).on_hover_text(name)
                    } else {
                        chip(ui, shell, active, &name, label, true)
                    };
                    egui::Popup::menu(&face).show(|ui| {
                        ui.set_min_width(160.0);
                        for layer in ordered(pcb) {
                            let name = layer_name(pcb, layer);
                            let label = tr!("layer-chip", layer = name.as_str());
                            let r = chip(ui, shell, layer, &name, label, layer.uuid == active.uuid);
                            chip_clicked(ui, &r, layer.uuid, out);
                        }
                        ui.separator();
                        ui.weak(tr!("layers-sets"));
                        set_buttons(ui, shell, out);
                    });
                });
        });
}

/// The strip: a row along the canvas bottom with the Layer sets button and a chip per
/// shown Layer.
fn strip(
    ui: &Ui,
    shell: &Shell,
    pcb: &PcbDocument,
    id: Id,
    canvas: Rect,
    out: &mut Vec<Invocation>,
) {
    let view = shell.board_view();
    let active = shell.engine().project().and_then(|p| view.active_layer(p));
    let height = crate::bars::draw::LAYER_STRIP_HEIGHT;
    let rect = Rect::from_min_max(
        canvas.left_bottom() - vec2(0.0, height),
        canvas.right_bottom(),
    );
    egui::Area::new(id.with("layer-strip"))
        .order(Order::Middle)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            ui.painter().rect_filled(rect, 0.0, ui.visuals().panel_fill);
            ui.painter().hline(
                rect.x_range(),
                rect.top(),
                Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
            );
            let layout = egui::Layout::left_to_right(egui::Align::Center);
            ui.allocate_ui_with_layout(rect.size(), layout, |ui| {
                ui.set_clip_rect(rect);
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.x = 2.0;
                let sets = ui.add(egui::Button::new(tr!("layers-sets")).small());
                egui::Popup::menu(&sets).show(|ui| set_buttons(ui, shell, out));
                ui.separator();
                for layer in ordered(pcb)
                    .into_iter()
                    .filter(|l| !view.layers.hidden.contains(&l.uuid))
                {
                    let name = layer_name(pcb, layer);
                    let label = tr!("layer-chip", layer = name.as_str());
                    let r = chip(ui, shell, layer, &name, label, active == Some(layer.uuid));
                    chip_clicked(ui, &r, layer.uuid, out);
                }
            });
        });
}

/// A click on a chip activates its Layer; `Mod`+click highlights its content.
fn chip_clicked(ui: &Ui, response: &Response, layer: Uuid, out: &mut Vec<Invocation>) {
    if !response.clicked() {
        return;
    }
    if ui.input(|i| i.modifiers.command) {
        out.push(("pcb.layer.highlight".into(), json!({ "layer": layer })));
    } else {
        out.push(("pcb.layer.activate".into(), json!({ "layer": layer })));
    }
}

/// A Layer chip: its colour square and name, accent-framed when `selected`; `label` is
/// what assistive tech reads.
fn chip(
    ui: &mut Ui,
    shell: &Shell,
    layer: &Layer,
    name: &str,
    label: String,
    selected: bool,
) -> Response {
    let font = FontId::proportional(12.0);
    let text = ui.visuals().text_color();
    let galley = ui.painter().layout_no_wrap(name.to_owned(), font, text);
    let size = if name.is_empty() {
        Vec2::new(SQUARE + 10.0, 20.0)
    } else {
        Vec2::new(SQUARE + 14.0 + galley.size().x, 20.0)
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    let visuals = ui.style().interact_selectable(&response, selected);
    let painter = ui.painter();
    if selected || response.hovered() {
        painter.rect(
            rect,
            3.0,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }
    let canvas_theme = theme::canvas_theme(ui.ctx().theme());
    let palette = shell.palette().palette(canvas_theme);
    let [r, g, b, _] = palette
        .color(ColorKey::Board(BoardColor::layer(layer.kind, layer.side)))
        .0;
    let square = Rect::from_center_size(
        rect.left_center() + vec2(5.0 + SQUARE / 2.0, 0.0),
        Vec2::splat(SQUARE),
    );
    painter.rect_filled(square, 2.0, Color32::from_rgb(r, g, b));
    let at = rect.left_center() + vec2(SQUARE + 9.0, -galley.size().y / 2.0);
    painter.galley(at, galley, visuals.text_color());
    response
}
