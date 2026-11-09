//! The Board navigator: one Panel with Nets, Footprints and Zones modes, as in the
//! leading suite's PCB panel; it replaces a separate Zones panel. Its top has the mode
//! switch, a search box and the Dim, Select and Zoom toggles, which say what clicking a
//! row does. Drawing only: every change is a Command, returned for the window to run.
//!
//! The rows and what a click runs are in [`rows`]; the mode, search and toggles are this
//! viewer's, kept in egui's memory.

pub mod rows;

use egui::{
    Color32, Id, Rect, Response, Role, ScrollArea, Sense, Stroke, TextEdit, Ui, WidgetInfo,
};
use reda_engine::model::{PcbDocument, Side, Uuid};
use reda_engine::pcb::LayerZones;
use reda_engine::pcb::zone::FillState;
use reda_engine::preferences::Units;
use reda_engine::render::Rgba;
use serde_json::{Value, json};

use self::rows::{OnClick, Target};
use crate::panels::layers::layer_name;
use crate::pcb::view::{format_length, parse_hex};
use crate::shell::Shell;
use crate::tr;

/// A Command for the window to run.
pub type Invocation = (String, Value);

/// The Panel's three modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Nets,
    Footprints,
    Zones,
}

/// What this viewer set at the Panel's top.
#[derive(Clone, Debug, Default)]
struct State {
    mode: Mode,
    search: String,
    on: OnClick,
}

/// The Net colours the swatch offers, besides any colour from the picker.
pub const NET_COLOURS: [&str; 12] = [
    "#e6194b", "#f58231", "#ffe119", "#bfef45", "#3cb44b", "#42d4f4", "#4363d8", "#911eb4",
    "#f032e6", "#fabed4", "#9a6324", "#a9a9a9",
];

const SWATCH: egui::Vec2 = egui::vec2(18.0, 12.0);

/// A dragged Zone row: the Zone and its Layer.
#[derive(Clone, Copy)]
struct ZoneDrag {
    layer: Uuid,
    zone: Uuid,
}

/// Draws the panel into `ui`.
pub fn show(ui: &mut Ui, shell: &Shell) -> Vec<Invocation> {
    let mut out = Vec::new();
    let Some(project) = shell.engine().project() else {
        return no_board(ui, out);
    };
    let Some(pcb) = project.pcb() else {
        return no_board(ui, out);
    };
    let state_id = Id::NULL.with("board-navigator");
    let mut state: State = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();

    top(ui, &mut state);
    ui.separator();
    let units = shell.board_view().units;
    let mut clicked = None;
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        clicked = match state.mode {
            Mode::Nets => nets(ui, shell, pcb, &state.search, units, &mut out),
            Mode::Footprints => footprints(ui, shell, pcb, &state.search, &mut out),
            Mode::Zones => {
                let fills = project.zone_fills(pcb.uuid);
                let listing = rows::zones(pcb, fills, &state.search);
                zones(ui, shell, pcb, &listing.layers, units, &mut out)
            }
        };
    });
    if let Some(target) = clicked {
        out.extend(rows::click(pcb, pcb.uuid, target, state.on));
    }
    ui.data_mut(|d| d.insert_temp(state_id, state));
    out
}

fn no_board(ui: &mut Ui, out: Vec<Invocation>) -> Vec<Invocation> {
    ui.add_space(8.0);
    ui.vertical_centered(|ui| ui.weak(tr!("nav-no-pcb")));
    out
}

/// The mode switch, the search box and the toggles.
fn top(ui: &mut Ui, state: &mut State) {
    ui.horizontal(|ui| {
        for (mode, title) in [
            (Mode::Nets, tr!("nav-mode-nets")),
            (Mode::Footprints, tr!("nav-mode-footprints")),
            (Mode::Zones, tr!("nav-mode-zones")),
        ] {
            let on = state.mode == mode;
            let response = ui.selectable_label(on, title.as_str());
            let label = tr!("nav-mode", mode = title.as_str());
            response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, on, &label));
            if response.clicked() {
                state.mode = mode;
            }
        }
    });
    let search = ui.add(
        TextEdit::singleline(&mut state.search)
            .hint_text(tr!("nav-search"))
            .desired_width(f32::INFINITY),
    );
    let label = tr!("nav-search-label");
    search.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &label));
    ui.horizontal(|ui| {
        for (flag, title, hint) in [
            (&mut state.on.dim, tr!("nav-dim"), tr!("nav-dim-hint")),
            (
                &mut state.on.select,
                tr!("nav-select"),
                tr!("nav-select-hint"),
            ),
            (&mut state.on.zoom, tr!("nav-zoom"), tr!("nav-zoom-hint")),
        ] {
            let on = *flag;
            let response = ui.selectable_label(on, title.as_str()).on_hover_text(hint);
            let label = tr!("nav-on-click", what = title.as_str());
            response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, &label));
            if response.clicked() {
                *flag = !on;
            }
        }
    });
}

/// A read-out cell `width` wide, named `label` for screen readers.
fn readout(ui: &mut Ui, width: f32, text: &str, label: &str) {
    let response = ui
        .allocate_ui(egui::vec2(width, ui.spacing().interact_size.y), |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.add(egui::Label::new(text).truncate())
        })
        .inner;
    response.widget_info(|| WidgetInfo::labeled(Role::Group, true, label));
}

/// A checkbox named `label`; returns the new value when it was clicked.
fn check(ui: &mut Ui, value: bool, label: &str, hint: Option<&str>) -> Option<bool> {
    let mut v = value;
    let mut response = ui.checkbox(&mut v, "");
    if let Some(hint) = hint {
        response = response.on_hover_text(hint);
    }
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, v, label));
    response.changed().then_some(v)
}

/// The row's name: a selectable label that is the row's click target.
fn name(ui: &mut Ui, width: f32, selected: bool, text: &str, label: &str) -> bool {
    let response = ui
        .allocate_ui(egui::vec2(width, ui.spacing().interact_size.y), |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.add(egui::Button::selectable(selected, text).truncate())
        })
        .inner;
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, label));
    response.clicked()
}

/// The width left for a row's name once its `fixed` columns are laid out, so a row fits
/// the Dock's width.
fn name_width(ui: &Ui, fixed: &[f32]) -> f32 {
    let spacing = ui.spacing().item_spacing.x;
    let taken: f32 = fixed.iter().map(|w| w + spacing).sum();
    (ui.available_width() - taken - 4.0).max(48.0)
}

/// A cell `width` wide holding a checkbox named `label`.
fn check_cell(ui: &mut Ui, width: f32, value: bool, label: &str) -> Option<bool> {
    ui.allocate_ui(egui::vec2(width, ui.spacing().interact_size.y), |ui| {
        ui.set_min_width(width);
        check(ui, value, label, None)
    })
    .inner
}

fn header(ui: &mut Ui, columns: &[(f32, String)]) {
    ui.horizontal(|ui| {
        for (width, title) in columns {
            ui.allocate_ui(egui::vec2(*width, ui.spacing().interact_size.y), |ui| {
                ui.set_min_width(*width);
                ui.weak(title.as_str());
            });
        }
    });
}

fn nothing(ui: &mut Ui) {
    ui.add_space(6.0);
    ui.vertical_centered(|ui| ui.weak(tr!("nav-nothing")));
}

fn nets(
    ui: &mut Ui,
    shell: &Shell,
    pcb: &PcbDocument,
    search: &str,
    units: Units,
    out: &mut Vec<Invocation>,
) -> Option<Target> {
    let ratsnest = &shell.board_view().layers.ratsnest;
    let rows = rows::nets(pcb, ratsnest, search);
    if rows.is_empty() {
        nothing(ui);
        return None;
    }
    let (tracks_w, length_w) = (40.0, 58.0);
    let lead = ui.spacing().icon_width + ui.spacing().item_spacing.x + SWATCH.x;
    let name_w = name_width(ui, &[lead, tracks_w, length_w]);
    header(
        ui,
        &[
            (lead, String::new()),
            (name_w, tr!("nav-column-net")),
            (tracks_w, tr!("nav-column-tracks")),
            (length_w, tr!("nav-column-length")),
        ],
    );
    let selection = shell.selection();
    let mut clicked = None;
    for row in rows {
        let net = row.name.as_str();
        ui.horizontal(|ui| {
            if let Some(visible) = check(ui, row.ratsnest, &tr!("nav-ratsnest", net = net), None) {
                out.push((
                    "pcb.ratsnest.show".to_owned(),
                    json!({"net": row.uuid, "visible": visible}),
                ));
            }
            if let Some(color) = swatch(ui, row.uuid, net, row.color.as_deref()) {
                let params = match color {
                    Some(c) => json!({"nets": [row.uuid], "color": c}),
                    None => json!({"nets": [row.uuid]}),
                };
                out.push(("pcb.net.set_color".to_owned(), params));
            }
            let selected = selection.nets.contains(&row.uuid);
            if name(ui, name_w, selected, net, &tr!("nav-net", net = net)) {
                clicked = Some(Target::Net(row.uuid));
            }
            let count = row.tracks.to_string();
            readout(
                ui,
                tracks_w,
                &count,
                &tr!("nav-tracks", net = net, count = count.as_str()),
            );
            let length = format_length(row.length, units);
            readout(
                ui,
                length_w,
                &length,
                &tr!("nav-length", net = net, length = length.as_str()),
            );
        });
    }
    clicked
}

/// The Net colour swatch of `net`: a click opens the palette. Returns the colour picked,
/// `Some(None)` for No colour.
fn swatch(ui: &mut Ui, net: Uuid, name: &str, color: Option<&str>) -> Option<Option<String>> {
    let (rect, response) = ui.allocate_exact_size(SWATCH, Sense::click());
    let stroke = Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color);
    match color.and_then(parse_hex) {
        Some(Rgba([r, g, b, _])) => {
            ui.painter()
                .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
        }
        None => {
            // No colour: a hollow box with a slash.
            ui.painter()
                .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
            ui.painter()
                .line_segment([rect.left_bottom(), rect.right_top()], stroke);
        }
    }
    let shown = color.map_or_else(|| tr!("nav-colour-none"), str::to_owned);
    let label = tr!("nav-colour", net = name, colour = shown.as_str());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
    let mut picked = None;
    let pending_id = Id::NULL.with(("nav-net-colour", net));
    let menu =
        egui::Popup::menu(&response).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let open = menu.show(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(6.0 * (SWATCH.x + 6.0));
            for hex in NET_COLOURS {
                let Some(Rgba([r, g, b, _])) = parse_hex(hex) else {
                    continue;
                };
                let (rect, chip) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::click());
                ui.painter()
                    .rect_filled(rect.shrink(1.0), 2.0, Color32::from_rgb(r, g, b));
                if color == Some(hex) {
                    ui.painter()
                        .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
                }
                let label = tr!("nav-colour-pick", colour = hex);
                chip.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
                if chip.clicked() {
                    picked = Some(Some(hex.to_owned()));
                    ui.close();
                }
            }
        });
        if ui.button(tr!("nav-colour-clear")).clicked() {
            picked = Some(None);
            ui.close();
        }
        ui.separator();
        // Any other colour, typed as #rrggbb and written on Enter.
        let mut text: String = ui
            .data(|d| d.get_temp(pending_id))
            .unwrap_or_else(|| color.unwrap_or_default().to_owned());
        let field = ui.add(
            TextEdit::singleline(&mut text)
                .hint_text("#rrggbb")
                .desired_width(80.0),
        );
        let label = tr!("nav-colour-hex", net = name);
        field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &label));
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let typed = text.trim().to_lowercase();
        if entered && parse_hex(&typed).is_some() {
            ui.data_mut(|d| d.remove::<String>(pending_id));
            picked = Some(Some(typed));
            ui.close();
        } else {
            ui.data_mut(|d| d.insert_temp(pending_id, text));
        }
    });
    if open.is_none() {
        // A closed palette forgets what was typed.
        ui.data_mut(|d| d.remove::<String>(pending_id));
    }
    picked
}

fn footprints(
    ui: &mut Ui,
    shell: &Shell,
    pcb: &PcbDocument,
    search: &str,
    out: &mut Vec<Invocation>,
) -> Option<Target> {
    let rows = rows::footprints(pcb, search);
    if rows.is_empty() {
        nothing(ui);
        return None;
    }
    let (side_w, locked_w, fitted_w) = (48.0, 44.0, 60.0);
    let name_w = name_width(ui, &[side_w, locked_w, fitted_w]);
    header(
        ui,
        &[
            (name_w, tr!("nav-column-designator")),
            (side_w, tr!("nav-column-side")),
            (locked_w, tr!("nav-column-locked")),
            (fitted_w, tr!("nav-column-not-fitted")),
        ],
    );
    let selection = shell.selection();
    let mut clicked = None;
    for row in rows {
        let d = row.designator.as_str();
        ui.horizontal(|ui| {
            let selected = selection.objects.contains(&row.uuid);
            if name(
                ui,
                name_w,
                selected,
                d,
                &tr!("nav-footprint", designator = d),
            ) {
                clicked = Some(Target::Object(row.uuid));
            }
            let side = match row.side {
                Side::Top => tr!("nav-side-top"),
                Side::Bottom => tr!("nav-side-bottom"),
            };
            let label = tr!("nav-side", designator = d, side = side.as_str());
            readout(ui, side_w, &side, &label);
            let label = tr!("nav-locked", designator = d);
            if let Some(locked) = check_cell(ui, locked_w, row.locked, &label) {
                out.push((
                    "pcb.lock".to_owned(),
                    json!({"objects": [row.uuid], "locked": locked}),
                ));
            }
            let label = tr!("nav-not-fitted", designator = d);
            if let Some(not_fitted) = check_cell(ui, fitted_w, row.not_fitted, &label) {
                out.push((
                    "pcb.footprint.set".to_owned(),
                    json!({"footprints": [row.uuid], "not_fitted": not_fitted}),
                ));
            }
        });
    }
    clicked
}

/// An area in square millimetres or square mils.
pub(crate) fn format_area(area: i64, units: Units) -> String {
    let (value, unit) = match units {
        Units::Mm => (area as f64 / 1e12, "mm²"),
        Units::Mil => (area as f64 / (25_400.0 * 25_400.0), "mil²"),
    };
    let mut text = format!("{value:.2}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    format!("{text} {unit}")
}

fn zones(
    ui: &mut Ui,
    shell: &Shell,
    pcb: &PcbDocument,
    layers: &[LayerZones],
    units: Units,
    out: &mut Vec<Invocation>,
) -> Option<Target> {
    ui.horizontal(|ui| {
        let auto = ui
            .button(tr!("nav-auto-order"))
            .on_hover_text(tr!("nav-auto-order-hint"));
        if auto.clicked() {
            out.push(("pcb.zone.auto_order".to_owned(), json!({})));
        }
        if pcb.zones().any(|z| z.shelved) && ui.button(tr!("nav-restore-all")).clicked() {
            out.push(("pcb.zone.restore".to_owned(), json!({})));
        }
    });
    if layers.is_empty() {
        nothing(ui);
        return None;
    }
    let selection = shell.selection();
    let mut clicked = None;
    let mut dropped = None;
    let (net_w, area_w) = (44.0, 60.0);
    let name_w = name_width(ui, &[10.0, ui.spacing().icon_width, 12.0, net_w, area_w]);
    for layer in layers {
        let title = pcb
            .layers()
            .layers
            .iter()
            .find(|l| l.uuid == layer.layer)
            .map(|l| layer_name(pcb, l))
            .unwrap_or_default();
        ui.add_space(4.0);
        ui.strong(title);
        for zone in &layer.zones {
            let z = zone.name.as_str();
            let row = ui.horizontal(|ui| {
                let drag = ZoneDrag {
                    layer: layer.layer,
                    zone: zone.uuid,
                };
                let grip =
                    ui.dnd_drag_source(Id::NULL.with(("nav-zone-drag", zone.uuid)), drag, |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 14.0), Sense::hover());
                        grip_lines(ui, rect);
                    });
                let label = tr!("nav-zone-drag", zone = z);
                grip.response
                    .widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
                let label = tr!("nav-zone-shelve", zone = z);
                let hint = tr!("nav-zone-shelve-hint");
                if let Some(shelve) = check(ui, zone.shelved, &label, Some(&hint)) {
                    let id = if shelve {
                        "pcb.zone.shelve"
                    } else {
                        "pcb.zone.restore"
                    };
                    out.push((id.to_owned(), json!({ "zones": [zone.uuid] })));
                }
                stale_marker(ui, zone.state == FillState::Stale, z);
                let selected = selection.objects.contains(&zone.uuid);
                if name(ui, name_w, selected, z, &tr!("nav-zone", zone = z)) {
                    clicked = Some(Target::Object(zone.uuid));
                }
                let net = zone
                    .net
                    .and_then(|n| pcb.net(n))
                    .map_or_else(|| tr!("nav-zone-no-net"), |n| n.name.clone());
                let label = tr!("nav-zone-net", zone = z, net = net.as_str());
                readout(ui, net_w, &net, &label);
                let area = format_area(zone.outline_area, units);
                let label = tr!("nav-zone-area", zone = z, area = area.as_str());
                readout(ui, area_w, &area, &label);
            });
            let row = ui.interact(
                row.response.rect,
                Id::NULL.with(("nav-zone-row", zone.uuid)),
                Sense::hover(),
            );
            if let Some(drag) = row.dnd_hover_payload::<ZoneDrag>()
                && drag.layer == layer.layer
            {
                let after = pointer_below_middle(ui, row.rect);
                let y = if after {
                    row.rect.bottom()
                } else {
                    row.rect.top()
                };
                let stroke = Stroke::new(2.0, ui.visuals().selection.stroke.color);
                ui.painter().hline(row.rect.x_range(), y, stroke);
            }
            if let Some(drag) = row.dnd_release_payload::<ZoneDrag>()
                && drag.layer == layer.layer
            {
                let after = pointer_below_middle(ui, row.rect);
                dropped = Some((layer, drag.zone, zone.uuid, after));
            }
        }
    }
    if let Some((layer, moved, target, after)) = dropped {
        // The full order on the Layer, shelved and filtered-out Zones too.
        let order: Vec<Uuid> = reda_engine::pcb::zone::layer_order(pcb, layer.layer)
            .into_iter()
            .map(|z| z.uuid)
            .collect();
        let new = rows::reordered(&order, moved, target, after);
        if new != order {
            out.push((
                "pcb.zone.reorder".to_owned(),
                json!({"layer": layer.layer, "order": new}),
            ));
        }
    }
    clicked
}

fn pointer_below_middle(ui: &Ui, rect: Rect) -> bool {
    ui.input(|i| i.pointer.interact_pos())
        .is_some_and(|p| p.y > rect.center().y)
}

/// The drag handle: three short lines.
fn grip_lines(ui: &Ui, rect: Rect) {
    let stroke = Stroke::new(1.0, ui.visuals().weak_text_color());
    for dy in [-3.0, 0.0, 3.0] {
        let y = rect.center().y + dy;
        ui.painter()
            .hline(rect.left() + 1.0..=rect.right() - 1.0, y, stroke);
    }
}

/// The stale marker: an amber dot while the Zone needs refilling, else a blank of the
/// same size.
fn stale_marker(ui: &mut Ui, stale: bool, zone: &str) {
    let (rect, response): (Rect, Response) =
        ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
    if stale {
        ui.painter()
            .circle_filled(rect.center(), 4.0, ui.visuals().warn_fg_color);
        let label = tr!("nav-zone-stale", zone = zone);
        response.widget_info(|| WidgetInfo::labeled(Role::Group, true, &label));
        response.on_hover_text(tr!("nav-zone-stale", zone = zone));
    }
}
