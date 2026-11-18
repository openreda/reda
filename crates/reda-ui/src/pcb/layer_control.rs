//! The Layer control: switching Layers with the Layers panel collapsed. A Preference
//! (`pcb.view.layer_control`) picks its form, and neither changes any output:
//!
//! - **Swatch** (default): a [`widgets::chip`] on the floating surface at the canvas's top
//!   right with the active Layer's swatch and name. Its menu lists the Layers copper
//!   first, each as a row with its swatch, then the Layer sets and **Save current as…**.
//! - **Strip:** a [`widgets::chip_strip`] along the canvas bottom, 24 px at the default
//!   text size ([`crate::bars::draw::layer_strip_height`]), with a Layer sets chip and a
//!   chip per shown Layer, copper first. Where the names do not all fit, every Layer chip
//!   shows its swatch alone, its name in its tooltip. The Tool's bar and the grid readout
//!   move up to clear it ([`strip_inset`]).
//!
//! In either, a click on a chip activates the Layer (`pcb.layer.activate`) and `Mod`+click
//! highlights its content (`pcb.layer.highlight`). Drawing only: each click is a Command
//! returned for the window to run.

use egui::{Align2, Id, Order, Rect, Response, Role, Ui, WidgetInfo, vec2};
use reda_engine::model::{Layer, LayerKind, PcbDocument, Uuid};
use reda_engine::preferences::LayerControl;
use reda_engine::render::{BoardColor, ColorKey};
use serde_json::{Value, json};

use super::view_commands::LAYER_SETS;
use crate::panels::layers::layer_name;
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::widgets::{self, Tooltip as _};
use crate::{theme, tr};

/// A Command for the window to run (one lacking params opens its dialog).
pub type Invocation = (String, Value);

/// Under this width the swatch drops its name, so the centred Active Bar stays clear.
const NARROW_CANVAS: f32 = 600.0;
/// How far the swatch sits from the canvas's right edge.
const SWATCH_INSET: f32 = 8.0;

/// How much the Layer strip takes from the board canvas's bottom: its height while it
/// shows, else nothing. Everything anchored to the bottom moves up by this much.
pub fn strip_inset(ui: &Ui, shell: &Shell) -> f32 {
    if strip_shown(shell) {
        crate::bars::draw::layer_strip_height(&ui.tokens())
    } else {
        0.0
    }
}

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
    sets(ui, shell, out, |ui, text| ui.add(widgets::button(text)));
}

/// The Layer sets as the items of a menu, as [`set_buttons`] has them.
fn set_items(ui: &mut Ui, shell: &Shell, out: &mut Vec<Invocation>) {
    sets(ui, shell, out, |ui, text| ui.add(widgets::menu_item(text)));
}

/// The Layer sets and **Save current as…**, each drawn by `add`.
fn sets(
    ui: &mut Ui,
    shell: &Shell,
    out: &mut Vec<Invocation>,
    add: impl Fn(&mut Ui, String) -> Response,
) {
    let saved = shell.preferences().pcb.view.layer_sets.iter();
    let sets = LAYER_SETS
        .iter()
        .map(|s| (s.to_string(), set_title(s)))
        .chain(saved.map(|s| (s.name.clone(), s.name.clone())));
    for (set, title) in sets {
        if add(ui, title).clicked() {
            out.push(("pcb.view.layer_set.apply".into(), json!({ "set": set })));
        }
    }
    if add(ui, tr!("layers-set-save")).clicked() {
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
    let t = ui.tokens();
    egui::Area::new(id.with("layer-swatch"))
        .order(Order::Middle)
        .fixed_pos(canvas.right_top() + vec2(-SWATCH_INSET, crate::bars::draw::ACTIVE_BAR_TOP))
        .pivot(Align2::RIGHT_TOP)
        .show(ui.ctx(), |ui| {
            widgets::floating_frame(&t).show(ui, |ui| {
                let name = layer_name(pcb, active);
                let label = tr!("layer-control-active", layer = name.as_str());
                // On a narrow canvas the swatch shows its colour only, clear of the
                // Active Bar; the name is in its tooltip.
                let face = if canvas.width() < NARROW_CANVAS {
                    chip(ui, shell, active, "", label, true).tooltip(name)
                } else {
                    chip(ui, shell, active, &name, label, true)
                };
                let width = menu_width(ui, pcb);
                widgets::menu(&face).width(width).show(|ui| {
                    layer_rows(ui, shell, pcb, active.uuid, out);
                    widgets::menu_separator(ui);
                    ui.label(
                        t.ty.small
                            .rich(tr!("layers-sets"))
                            .color(t.color.text.muted),
                    );
                    set_items(ui, shell, out);
                });
            });
        });
}

/// The active Layer's name on the board, for the status bar.
pub fn active_layer_name(shell: &Shell) -> Option<String> {
    let project = shell.engine().project()?;
    let pcb = project.pcb()?;
    let active = shell.board_view().active_layer(project)?;
    pcb.layers().layer(active).map(|l| layer_name(pcb, l))
}

/// The status bar's swatch: the active Layer's chip, whose menu lists the Layers.
pub fn status_swatch(ui: &mut Ui, shell: &Shell, out: &mut Vec<Invocation>) {
    let Some(project) = shell.engine().project() else {
        return;
    };
    let Some(pcb) = project.pcb() else {
        return;
    };
    let active = shell.board_view().active_layer(project);
    let Some(active) = active.and_then(|a| pcb.layers().layer(a)) else {
        return;
    };
    let name = layer_name(pcb, active);
    let label = tr!("status-layer", layer = name.as_str());
    // On the status bar it is one of the bar's items: flat, in small text, with the
    // Layer's swatch. Its menu lists the Layers as rows.
    let [r, g, b] = layer_rgb(ui, shell, active);
    let face = ui.add(widgets::status_item(name.as_str()).swatch_rgb(r, g, b));
    face.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    let width = menu_width(ui, pcb);
    widgets::menu(&face)
        .width(width)
        .show(|ui| layer_rows(ui, shell, pcb, active.uuid, out));
}

/// The least width of a menu listing the Layers.
const MENU_WIDTH: f32 = 160.0;

/// The width of a menu listing `pcb`'s Layers: the widest row (a swatch and a name) in
/// the menu's frame, and no narrower than [`MENU_WIDTH`].
fn menu_width(ui: &Ui, pcb: &PcbDocument) -> f32 {
    let t = ui.tokens();
    // The popup's width takes in its padding and rim.
    let frame = 2.0 * (t.space.menu + t.stroke.hairline);
    let lead = widgets::SWATCH + widgets::ICON_GAP + 2.0 * t.space.row_x + frame;
    ordered(pcb)
        .into_iter()
        .map(|layer| {
            let job =
                t.ty.body
                    .job(&layer_name(pcb, layer), egui::Color32::TRANSPARENT);
            ui.painter().layout_job(job).size().x.ceil() + lead
        })
        .fold(MENU_WIDTH, f32::max)
}

/// The board's Layers as the rows of a menu, each with its swatch, the `active` one
/// selected. A row is a Layer chip in a list: a click activates, `Mod`+click highlights.
fn layer_rows(
    ui: &mut Ui,
    shell: &Shell,
    pcb: &PcbDocument,
    active: Uuid,
    out: &mut Vec<Invocation>,
) {
    for layer in ordered(pcb) {
        let name = layer_name(pcb, layer);
        let label = tr!("layer-chip", layer = name.as_str());
        let chosen = layer.uuid == active;
        let [r, g, b] = layer_rgb(ui, shell, layer);
        let row = widgets::list_row(name.as_str())
            .swatch_rgb(r, g, b)
            .selected(chosen);
        let r = ui.add(row);
        r.widget_info(|| WidgetInfo::selected(Role::Button, true, chosen, label.clone()));
        chip_clicked(ui, &r, layer.uuid, out);
    }
}

/// The red, green and blue `layer` is drawn in on the canvas, in the theme of `ui`.
fn layer_rgb(ui: &Ui, shell: &Shell, layer: &Layer) -> [u8; 3] {
    let canvas_theme = theme::canvas_theme(ui.ctx().theme());
    let palette = shell.palette().palette(canvas_theme);
    let [r, g, b, _] = palette
        .color(ColorKey::Board(BoardColor::layer(layer.kind, layer.side)))
        .0;
    [r, g, b]
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
    let height = crate::bars::draw::layer_strip_height(&ui.tokens());
    let rect = Rect::from_min_max(
        canvas.left_bottom() - vec2(0.0, height),
        canvas.right_bottom(),
    );
    egui::Area::new(id.with("layer-strip"))
        .order(Order::Middle)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            widgets::chip_strip(ui, rect, |ui| {
                let sets = ui.add(widgets::chip(tr!("layers-sets")).chevron());
                widgets::menu(&sets).show(|ui| set_items(ui, shell, out));
                widgets::chip_rule(ui);
                let shown: Vec<_> = ordered(pcb)
                    .into_iter()
                    .filter(|l| !view.layers.hidden.contains(&l.uuid))
                    .map(|layer| (layer, layer_name(pcb, layer)))
                    .collect();
                // The names fit, or every chip shows its swatch alone.
                let gap = widgets::CHIP_GAP;
                let needed: f32 = shown
                    .iter()
                    .map(|(_, name)| {
                        widgets::chip(name.as_str()).swatch_rgb(0, 0, 0).width(ui) + gap
                    })
                    .sum();
                let named = needed <= ui.available_width();
                for (layer, name) in shown {
                    let label = tr!("layer-chip", layer = name.as_str());
                    let text = if named { name.as_str() } else { "" };
                    let r = chip(ui, shell, layer, text, label, active == Some(layer.uuid));
                    let r = if named { r } else { r.tooltip(name.as_str()) };
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

/// A Layer chip: its swatch and name, tinted when `selected`; `label` is what assistive
/// tech reads.
fn chip(
    ui: &mut Ui,
    shell: &Shell,
    layer: &Layer,
    name: &str,
    label: String,
    selected: bool,
) -> Response {
    let [r, g, b] = layer_rgb(ui, shell, layer);
    ui.add(
        widgets::chip(name)
            .swatch_rgb(r, g, b)
            .selected(selected)
            .name(label),
    )
}
