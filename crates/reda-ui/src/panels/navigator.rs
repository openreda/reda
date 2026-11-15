//! The Board navigator: one Panel with Nets, Footprints and Zones modes, as in the
//! leading suite's PCB panel; it replaces a separate Zones panel. Its top has the mode
//! switch, a search box and the Dim, Select and Zoom toggles, which say what clicking a
//! row does. Drawing only: every change is a Command, returned for the window to run.
//!
//! The rows and what a click runs are in [`rows`]; the mode, search and toggles are this
//! viewer's, kept in egui's memory.

pub mod rows;

use egui::{Id, Rect, Role, ScrollArea, Sense, Stroke, Ui, WidgetInfo};
use reda_engine::model::{PcbDocument, Side, Uuid};
use reda_engine::pcb::LayerZones;
use reda_engine::pcb::zone::FillState;
use reda_engine::preferences::Units;
use reda_engine::render::Rgba;
use serde_json::{Value, json};

use self::rows::{OnClick, Target};
use crate::layout::PanelId;
use crate::panels::layers::layer_name;
use crate::pcb::view::{format_length, parse_hex};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column, RowLook, Selection, Status, Tooltip as _};

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

/// The Net colours the swatch offers, besides any colour typed in. They are data: the
/// colours a Net can be given, sent to `pcb.net.set_color` as written here.
pub const NET_COLOURS: [&str; 12] = [
    // theme-exempt: the Net palette is data, the colours a Net can be given
    "#e6194b", "#f58231", "#ffe119", "#bfef45", "#3cb44b", "#42d4f4", "#4363d8", "#911eb4",
    // theme-exempt: the Net palette is data, the colours a Net can be given
    "#f032e6", "#fabed4", "#9a6324", "#a9a9a9",
];

/// How many of the Net colours the palette shows in a row.
const PALETTE_ROW: usize = 6;
/// The width of the palette's field for a typed colour.
const HEX_WIDTH: f32 = 80.0;
/// The Zones table's fixed columns: the drag handle, a Zone's Net and its area.
const GRIP_WIDTH: f32 = 10.0;
const ZONE_NET_WIDTH: f32 = 56.0;
const ZONE_AREA_WIDTH: f32 = 72.0;

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
    let look = Look {
        units: shell.board_view().units,
        // The tint of a selected row: live only while this Panel holds keyboard focus.
        selected: Selection::of(ui, true, shell.panel_focused(PanelId::Navigator)),
    };
    let mut clicked = None;
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        clicked = match state.mode {
            Mode::Nets => nets(ui, shell, pcb, &state.search, look, &mut out),
            Mode::Footprints => footprints(ui, shell, pcb, &state.search, look, &mut out),
            Mode::Zones => {
                let fills = project.zone_fills(pcb.uuid);
                let listing = rows::zones(pcb, fills, &state.search);
                zones(ui, shell, pcb, &listing.layers, look, &mut out)
            }
        };
    });
    if let Some(target) = clicked {
        out.extend(rows::click(pcb, pcb.uuid, target, state.on));
    }
    ui.data_mut(|d| d.insert_temp(state_id, state));
    out
}

/// What every mode's rows share.
#[derive(Clone, Copy)]
struct Look {
    units: Units,
    /// How a selected row shows.
    selected: Selection,
}

impl Look {
    fn row(self, selected: bool) -> RowLook {
        RowLook::selected(if selected {
            self.selected
        } else {
            Selection::None
        })
    }
}

fn no_board(ui: &mut Ui, out: Vec<Invocation>) -> Vec<Invocation> {
    ui.add_space(ui.tokens().space.gap);
    ui.vertical_centered(|ui| ui.weak(tr!("nav-no-pcb")));
    out
}

/// The mode switch, the search box and the toggles.
fn top(ui: &mut Ui, state: &mut State) {
    ui.horizontal_wrapped(|ui| {
        for (mode, title) in [
            (Mode::Nets, tr!("nav-mode-nets")),
            (Mode::Footprints, tr!("nav-mode-footprints")),
            (Mode::Zones, tr!("nav-mode-zones")),
        ] {
            let on = state.mode == mode;
            let response = ui.add(widgets::button(title.as_str()).on(on));
            let label = tr!("nav-mode", mode = title.as_str());
            response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, on, &label));
            if response.clicked() {
                state.mode = mode;
            }
        }
    });
    let search = ui.add(
        widgets::text_field(&mut state.search)
            .search()
            .hint(tr!("nav-search"))
            .width(f32::INFINITY),
    );
    let label = tr!("nav-search-label");
    search.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &label));
    ui.horizontal_wrapped(|ui| {
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
            let response = ui.add(widgets::button(title.as_str()).on(on)).tooltip(hint);
            let label = tr!("nav-on-click", what = title.as_str());
            response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, &label));
            if response.clicked() {
                *flag = !on;
            }
        }
    });
}

/// A read-out cell, named `label` for screen readers.
fn readout(row: &mut widgets::TableRow<'_>, text: &str, label: &str) {
    let response = row.text(text);
    response.widget_info(|| WidgetInfo::labeled(Role::Group, true, label));
}

/// A cell holding a checkbox named `label`; returns the new value when it was clicked.
fn check(
    row: &mut widgets::TableRow<'_>,
    value: bool,
    label: &str,
    hint: Option<&str>,
) -> Option<bool> {
    let mut v = value;
    let mut response = row.ui(|ui| ui.add(widgets::checkbox(&mut v, "")));
    if let Some(hint) = hint {
        response = response.tooltip(hint);
    }
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, v, label));
    response.changed().then_some(v)
}

/// Names a row for screen readers: it is the row's click target, a button named `label`.
fn name_row(row: &egui::Response, selected: bool, label: &str) {
    row.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, label));
}

fn nothing(ui: &mut Ui) {
    ui.add_space(ui.tokens().space.gap);
    ui.vertical_centered(|ui| ui.weak(tr!("nav-nothing")));
}

fn nets(
    ui: &mut Ui,
    shell: &Shell,
    pcb: &PcbDocument,
    search: &str,
    look: Look,
    out: &mut Vec<Invocation>,
) -> Option<Target> {
    let ratsnest = &shell.board_view().layers.ratsnest;
    let rows = rows::nets(pcb, ratsnest, search);
    if rows.is_empty() {
        nothing(ui);
        return None;
    }
    let selection = shell.selection();
    let mut clicked = None;
    // The Ratsnest box and the colour share the first column, so the name has the room.
    let columns = [
        Column::auto(),
        Column::fill(),
        Column::auto().number(),
        Column::auto().number(),
    ];
    let table = widgets::table("nav-nets").columns(columns);
    table.show(ui, |table| {
        table.header(|h| {
            h.empty();
            h.text(tr!("nav-column-net"));
            h.text(tr!("nav-column-tracks"));
            h.text(tr!("nav-column-length"));
        });
        for row in rows {
            let net = row.name.as_str();
            let selected = selection.nets.contains(&row.uuid);
            let response = table.row_with(look.row(selected), |cells| {
                let label = tr!("nav-ratsnest", net = net);
                let (visible, picked) = cells.ui(|ui| {
                    let mut shown = row.ratsnest;
                    let check = ui.add(widgets::checkbox(&mut shown, ""));
                    check.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, shown, &label));
                    let picked = swatch(ui, row.uuid, net, row.color.as_deref());
                    (check.changed().then_some(shown), picked)
                });
                if let Some(visible) = visible {
                    out.push((
                        "pcb.ratsnest.show".to_owned(),
                        json!({"net": row.uuid, "visible": visible}),
                    ));
                }
                if let Some(color) = picked {
                    let params = match color {
                        Some(c) => json!({"nets": [row.uuid], "color": c}),
                        None => json!({"nets": [row.uuid]}),
                    };
                    out.push(("pcb.net.set_color".to_owned(), params));
                }
                cells.text(net);
                let count = row.tracks.to_string();
                let label = tr!("nav-tracks", net = net, count = count.as_str());
                readout(cells, &count, &label);
                let length = format_length(row.length, look.units);
                let label = tr!("nav-length", net = net, length = length.as_str());
                readout(cells, &length, &label);
            });
            name_row(&response, selected, &tr!("nav-net", net = net));
            if response.clicked() {
                clicked = Some(Target::Net(row.uuid));
            }
        }
    });
    clicked
}

/// The Net colour swatch of `net`: a click opens the palette. Returns the colour picked,
/// `Some(None)` for No colour.
fn swatch(ui: &mut Ui, net: Uuid, name: &str, color: Option<&str>) -> Option<Option<String>> {
    let t = ui.tokens();
    let shown = color.map_or_else(|| tr!("nav-colour-none"), str::to_owned);
    let label = tr!("nav-colour", net = name, colour = shown.as_str());
    let response = match color.and_then(parse_hex) {
        Some(Rgba([r, g, b, _])) => ui.add(widgets::swatch_rgb(r, g, b).button(label)),
        None => ui.add(widgets::swatch_none().button(label)),
    };
    let mut picked = None;
    let pending_id = Id::NULL.with(("nav-net-colour", net));
    let open = widgets::menu(&response).stay_open().show(|ui| {
        for line in NET_COLOURS.chunks(PALETTE_ROW) {
            ui.horizontal(|ui| {
                for &hex in line {
                    let Some(Rgba([r, g, b, _])) = parse_hex(hex) else {
                        continue;
                    };
                    let label = tr!("nav-colour-pick", colour = hex);
                    let chip = ui.add(widgets::swatch_rgb(r, g, b).button(label));
                    if color == Some(hex) {
                        // The Net's colour is ringed.
                        let ring = Stroke::new(t.stroke.accent_line, t.color.accent.text);
                        let at = chip.rect.expand(t.stroke.accent_line);
                        let radius = t.radius.mark + t.stroke.accent_line;
                        ui.painter()
                            .rect_stroke(at, radius, ring, egui::StrokeKind::Inside);
                    }
                    if chip.clicked() {
                        picked = Some(Some(hex.to_owned()));
                        ui.close();
                    }
                }
            });
            ui.add_space(t.space.gap / 2.0);
        }
        if ui
            .add(widgets::menu_item(tr!("nav-colour-clear")))
            .clicked()
        {
            picked = Some(None);
            ui.close();
        }
        widgets::menu_separator(ui);
        // Any other colour, typed as #rrggbb and written on Enter.
        let mut text: String = ui
            .data(|d| d.get_temp(pending_id))
            .unwrap_or_else(|| color.unwrap_or_default().to_owned());
        let field = ui.add(
            widgets::text_field(&mut text)
                .hint("#rrggbb")
                .width(HEX_WIDTH),
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
    look: Look,
    out: &mut Vec<Invocation>,
) -> Option<Target> {
    let rows = rows::footprints(pcb, search);
    if rows.is_empty() {
        nothing(ui);
        return None;
    }
    let selection = shell.selection();
    let mut clicked = None;
    let columns = [
        Column::fill(),
        Column::auto(),
        Column::auto(),
        Column::auto(),
    ];
    let table = widgets::table("nav-footprints").columns(columns);
    table.show(ui, |table| {
        table.header(|h| {
            h.text(tr!("nav-column-designator"));
            h.text(tr!("nav-column-side"));
            h.text(tr!("nav-column-locked"));
            h.text(tr!("nav-column-not-fitted"));
        });
        for row in rows {
            let d = row.designator.as_str();
            let selected = selection.objects.contains(&row.uuid);
            let response = table.row_with(look.row(selected), |cells| {
                cells.text(d);
                let side = match row.side {
                    Side::Top => tr!("nav-side-top"),
                    Side::Bottom => tr!("nav-side-bottom"),
                };
                let label = tr!("nav-side", designator = d, side = side.as_str());
                readout(cells, &side, &label);
                let label = tr!("nav-locked", designator = d);
                if let Some(locked) = check(cells, row.locked, &label, None) {
                    out.push((
                        "pcb.lock".to_owned(),
                        json!({"objects": [row.uuid], "locked": locked}),
                    ));
                }
                let label = tr!("nav-not-fitted", designator = d);
                if let Some(not_fitted) = check(cells, row.not_fitted, &label, None) {
                    out.push((
                        "pcb.footprint.set".to_owned(),
                        json!({"footprints": [row.uuid], "not_fitted": not_fitted}),
                    ));
                }
            });
            name_row(&response, selected, &tr!("nav-footprint", designator = d));
            if response.clicked() {
                clicked = Some(Target::Object(row.uuid));
            }
        }
    });
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
    look: Look,
    out: &mut Vec<Invocation>,
) -> Option<Target> {
    let t = ui.tokens();
    ui.horizontal(|ui| {
        let auto = ui
            .add(widgets::button(tr!("nav-auto-order")))
            .tooltip(tr!("nav-auto-order-hint"));
        if auto.clicked() {
            out.push(("pcb.zone.auto_order".to_owned(), json!({})));
        }
        if pcb.zones().any(|z| z.shelved)
            && ui.add(widgets::button(tr!("nav-restore-all"))).clicked()
        {
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
    // Every Layer's table has the same fixed columns, so they line up: the drag handle,
    // the shelve box, the stale mark, the name, the Net and the area.
    let columns = [
        Column::fixed(GRIP_WIDTH),
        Column::fixed(widgets::MARK),
        Column::fixed(widgets::glyph::BOX),
        Column::fill(),
        Column::fixed(ZONE_NET_WIDTH),
        Column::fixed(ZONE_AREA_WIDTH).number(),
    ];
    for layer in layers {
        let title = pcb
            .layers()
            .layers
            .iter()
            .find(|l| l.uuid == layer.layer)
            .map(|l| layer_name(pcb, l))
            .unwrap_or_default();
        ui.add_space(t.space.gap / 2.0);
        ui.label(t.ty.body_strong.rich(title).color(t.color.text.primary));
        let table = widgets::table(("nav-zones", layer.layer)).columns(columns);
        table.show(ui, |table| {
            for zone in &layer.zones {
                let z = zone.name.as_str();
                let selected = selection.objects.contains(&zone.uuid);
                let row = table.row_with(look.row(selected), |cells| {
                    let drag = ZoneDrag {
                        layer: layer.layer,
                        zone: zone.uuid,
                    };
                    let id = Id::NULL.with(("nav-zone-drag", zone.uuid));
                    let grip = cells.ui(|ui| {
                        ui.dnd_drag_source(id, drag, |ui| {
                            let size = egui::vec2(GRIP_WIDTH, widgets::glyph::BOX);
                            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                            grip_lines(ui, rect);
                        })
                        .response
                    });
                    let label = tr!("nav-zone-drag", zone = z);
                    grip.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
                    let label = tr!("nav-zone-shelve", zone = z);
                    let hint = tr!("nav-zone-shelve-hint");
                    if let Some(shelve) = check(cells, zone.shelved, &label, Some(&hint)) {
                        let id = if shelve {
                            "pcb.zone.shelve"
                        } else {
                            "pcb.zone.restore"
                        };
                        out.push((id.to_owned(), json!({ "zones": [zone.uuid] })));
                    }
                    stale_marker(cells, zone.state == FillState::Stale, z);
                    cells.text(z);
                    let net = zone
                        .net
                        .and_then(|n| pcb.net(n))
                        .map_or_else(|| tr!("nav-zone-no-net"), |n| n.name.clone());
                    let label = tr!("nav-zone-net", zone = z, net = net.as_str());
                    readout(cells, &net, &label);
                    let area = format_area(zone.outline_area, look.units);
                    let label = tr!("nav-zone-area", zone = z, area = area.as_str());
                    readout(cells, &area, &label);
                });
                name_row(&row, selected, &tr!("nav-zone", zone = z));
                if row.clicked() {
                    clicked = Some(Target::Object(zone.uuid));
                }
                if let Some(drag) = row.dnd_hover_payload::<ZoneDrag>()
                    && drag.layer == layer.layer
                {
                    let after = pointer_below_middle(&row);
                    let y = if after {
                        row.rect.bottom()
                    } else {
                        row.rect.top()
                    };
                    let stroke = Stroke::new(t.stroke.accent_line, t.color.accent.text);
                    row.ctx
                        .layer_painter(row.layer_id)
                        .hline(row.rect.x_range(), y, stroke);
                }
                if let Some(drag) = row.dnd_release_payload::<ZoneDrag>()
                    && drag.layer == layer.layer
                {
                    let after = pointer_below_middle(&row);
                    dropped = Some((layer, drag.zone, zone.uuid, after));
                }
            }
        });
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

/// Whether the pointer is in the lower half of `row`.
fn pointer_below_middle(row: &egui::Response) -> bool {
    let rect: Rect = row.rect;
    row.ctx
        .input(|i| i.pointer.interact_pos())
        .is_some_and(|p| p.y > rect.center().y)
}

/// The drag handle: three short lines.
fn grip_lines(ui: &Ui, rect: Rect) {
    let t = ui.tokens();
    let stroke = Stroke::new(t.stroke.hairline, t.color.text.muted);
    for dy in [-3.0, 0.0, 3.0] {
        let y = rect.center().y + dy;
        ui.painter()
            .hline(rect.left() + 1.0..=rect.right() - 1.0, y, stroke);
    }
}

/// The stale mark: a warning mark while the Zone needs refilling, else an empty cell.
fn stale_marker(row: &mut widgets::TableRow<'_>, stale: bool, zone: &str) {
    if !stale {
        row.empty();
        return;
    }
    let response = row.status(Status::Warning);
    let label = tr!("nav-zone-stale", zone = zone);
    response.widget_info(|| WidgetInfo::labeled(Role::Group, true, &label));
    response.tooltip(label);
}
